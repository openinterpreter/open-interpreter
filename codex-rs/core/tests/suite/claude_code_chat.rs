#![cfg(unix)]

use anyhow::Result;
use codex_model_provider_info::WireApi;
use codex_protocol::models::PermissionProfile;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use wiremock::Mock;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

struct ChatSequenceResponder {
    next_response: AtomicUsize,
    responses: Vec<String>,
}

impl Respond for ChatSequenceResponder {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let response_index = self.next_response.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(
                self.responses
                    .get(response_index)
                    .expect("unexpected extra chat completion request")
                    .clone(),
            )
    }
}

fn chat_sse(chunks: Vec<Value>) -> String {
    let mut body = chunks
        .into_iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect::<String>();
    body.push_str("data: [DONE]\n\n");
    body
}

fn tool_call_response(call_id: &str) -> String {
    let arguments =
        json!({ "command": "printf 'GPT_CLAUDE_CHAT_OK\\n' > claude-chat-proof.txt" }).to_string();
    chat_sse(vec![
        json!({
            "id": "chatcmpl-claude-tool",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "gpt-5.6-sol",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "tool_calls": [{
                        "index": 0,
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": "Bash",
                            "arguments": arguments
                        }
                    }]
                },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chatcmpl-claude-tool",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "gpt-5.6-sol",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "tool_calls"
            }]
        }),
    ])
}

fn final_response() -> String {
    chat_sse(vec![
        json!({
            "id": "chatcmpl-claude-final",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "gpt-5.6-sol",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "done"
                },
                "finish_reason": null
            }]
        }),
        json!({
            "id": "chatcmpl-claude-final",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "gpt-5.6-sol",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }]
        }),
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gpt_model_runs_claude_code_harness_over_chat_completions() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "call-claude-chat-proof";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ChatSequenceResponder {
            next_response: AtomicUsize::new(0),
            responses: vec![tool_call_response(call_id), final_response()],
        })
        .expect(2)
        .mount(&server)
        .await;

    let mut builder = test_codex()
        .with_model("gpt-5.6-sol")
        .with_config(|config| {
            config.harness = Some("claude-code".to_string());
            config.model_provider.wire_api = WireApi::Chat;
        });
    let test = builder.build(&server).await?;

    test.submit_turn_with_permission_profile(
        "Create the requested proof file, then confirm completion.",
        PermissionProfile::Disabled,
    )
    .await?;

    assert_eq!(
        std::fs::read_to_string(test.cwd.path().join("claude-chat-proof.txt"))?,
        "GPT_CLAUDE_CHAT_OK\n"
    );

    let requests = server.received_requests().await.unwrap_or_default();
    let chat_requests = requests
        .iter()
        .filter(|request| request.url.path() == "/v1/chat/completions")
        .collect::<Vec<_>>();
    assert_eq!(chat_requests.len(), 2);

    let first_body: Value = serde_json::from_slice(&chat_requests[0].body)?;
    assert_eq!(first_body["model"], "gpt-5.6-sol");
    assert!(
        first_body["messages"][0]["content"]
            .as_str()
            .is_some_and(|instructions| instructions.contains("Claude Code"))
    );
    assert!(
        first_body["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["function"]["name"] == "Bash"))
    );

    let second_body: Value = serde_json::from_slice(&chat_requests[1].body)?;
    let messages = second_body["messages"]
        .as_array()
        .expect("second chat request should contain messages");
    let assistant_index = messages
        .iter()
        .position(|message| message["role"] == "assistant" && message["tool_calls"].is_array())
        .expect("second request should replay the Claude harness tool call");
    assert_eq!(messages[assistant_index + 1]["role"], "tool");
    assert_eq!(messages[assistant_index + 1]["tool_call_id"], call_id);

    Ok(())
}
