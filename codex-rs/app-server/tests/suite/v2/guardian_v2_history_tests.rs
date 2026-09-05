//! Exercises async review retention across app-server tool execution, compaction, and rollback.

use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_chatgpt_auth;
use axum::Json;
use axum::Router;
use axum::http::header;
use axum::routing::get;
use axum::routing::post;
use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ThreadCompactStartParams;
use codex_app_server_protocol::ThreadCompactStartResponse;
use codex_app_server_protocol::ThreadHistoryMode;
use codex_app_server_protocol::ThreadRollbackParams;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_config::types::AuthCredentialsStoreMode;
use codex_features::Feature;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::time::timeout;

use super::MockResponsesState;
use super::TEST_SERVER_NAME;
use super::TEST_TOOL_NAME;
use super::TIMEOUT;
use super::luna_websocket;
use super::start_mcp_server;
use super::wait_for_luna_request;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_guardian_retains_evidence_after_compaction_and_discards_it_after_rollback()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    const RESTRICTION: &str = "Only publish to a private repository.";
    const EVIDENCE: &str = "repository is private";
    const SUMMARY: &str = "Repository inspection was summarized.";
    let expected_output = format!("\"echoed\":\"{EVIDENCE}\"");
    let parent_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let compact_requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let classifier = Arc::new(MockResponsesState {
        luna_score: 1.0,
        ..Default::default()
    });
    let router = Router::new()
        .route(
            "/v1/responses",
            get(luna_websocket).post({
                let parent_requests = Arc::clone(&parent_requests);
                move |Json(request): Json<Value>| {
                    let parent_requests = Arc::clone(&parent_requests);
                    async move {
                        let events = if request["client_metadata"]["x-openai-subagent"]
                            == "guardian"
                        {
                            vec![
                                responses::ev_assistant_message("review", r#"{"outcome":"allow"}"#),
                                responses::ev_completed("review"),
                            ]
                        } else {
                            let mut requests = parent_requests.lock().unwrap();
                            let step = requests.len();
                            requests.push(request);
                            if step.is_multiple_of(/*rhs*/ 2) {
                                let message = if step == 0 {
                                    EVIDENCE
                                } else {
                                    "current inspection"
                                };
                                vec![
                                    responses::ev_function_call_with_namespace(
                                        &format!("inspect-{step}"),
                                        &format!("mcp__{TEST_SERVER_NAME}"),
                                        TEST_TOOL_NAME,
                                        &json!({"message": message}).to_string(),
                                    ),
                                    responses::ev_completed(&format!("inspect-{step}")),
                                ]
                            } else {
                                vec![responses::ev_completed(&format!("done-{step}"))]
                            }
                        };
                        (
                            [(header::CONTENT_TYPE, "text/event-stream")],
                            responses::sse(events),
                        )
                    }
                }
            }),
        )
        .route(
            "/v1/responses/compact",
            post({
                let compact_requests = Arc::clone(&compact_requests);
                move |Json(request): Json<Value>| {
                    let compact_requests = Arc::clone(&compact_requests);
                    async move {
                        compact_requests.lock().unwrap().push(request);
                        Json(json!({"output": [
                            {"type": "message", "role": "assistant", "content": [
                                {"type": "output_text", "text": SUMMARY}
                            ]},
                            {"type": "compaction", "encrypted_content": "encrypted summary"}
                        ]}))
                    }
                }
            }),
        )
        .with_state(Arc::clone(&classifier));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let responses_url = format!("http://{}", listener.local_addr()?);
    let responses_server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (mcp_url, mcp_server) = start_mcp_server(/*sensitive_action*/ None).await?;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&responses_url)
        .with_provider_name("OpenAI")
        .with_provider_config("requires_openai_auth = true\nsupports_websockets = false")
        .with_root_config("approvals_reviewer = \"auto_review\"\nmodel_auto_compact_token_limit = 1000000")
        .enable_feature(Feature::GuardianApproval)
        .disable_feature(Feature::EnableRequestCompression)
        .disable_feature(Feature::RemoteCompactionV2)
        .disable_feature(Feature::TokenBudget)
        .with_extra_config(&format!(
            "[mcp_servers.{TEST_SERVER_NAME}]\nurl = \"{mcp_url}/mcp\"\ndefault_tools_approval_mode = \"prompt\"\n\n[features.guardianv2]\nenabled = true\n\n[features.guardianv2.review_scope]\ncomputer_use_only = false"
        ))
        .write(codex_home.path())?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("access-chatgpt").plan_type("pro"),
        AuthCredentialsStoreMode::File,
    )?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .with_env_overrides(&[("OPENAI_API_KEY", None)])
        .build_initialized_with_timeout(TIMEOUT)
        .await?;
    let thread_id = app_server
        .start_thread(ThreadStartParams {
            approval_policy: Some(AskForApproval::OnRequest),
            approvals_reviewer: Some(ApprovalsReviewer::AutoReview),
            history_mode: Some(ThreadHistoryMode::Legacy),
            ..Default::default()
        })
        .await?
        .thread
        .id;

    for (index, prompt) in [
        RESTRICTION,
        "Recheck the repository.",
        "Inspect a different repository.",
    ]
    .into_iter()
    .enumerate()
    {
        if index == 1 {
            let id = app_server
                .send_thread_compact_start_request(ThreadCompactStartParams {
                    thread_id: thread_id.clone(),
                })
                .await?;
            let _: ThreadCompactStartResponse =
                timeout(TIMEOUT, app_server.read_response(id)).await??;
            let completed: TurnCompletedNotification =
                timeout(TIMEOUT, app_server.read_notification("turn/completed")).await??;
            assert_eq!(completed.turn.status, TurnStatus::Completed);
            assert_eq!(compact_requests.lock().unwrap().len(), 1);
        } else if index == 2 {
            let id = app_server
                .send_thread_rollback_request(ThreadRollbackParams {
                    thread_id: thread_id.clone(),
                    num_turns: 2,
                })
                .await?;
            let _: ThreadRollbackResponse =
                timeout(TIMEOUT, app_server.read_response(id)).await??;
        }

        let completed = timeout(
            TIMEOUT,
            app_server.start_turn_and_wait_for_completion(TurnStartParams {
                thread_id: thread_id.clone(),
                input: vec![UserInput::Text {
                    text: prompt.to_owned(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            }),
        )
        .await??;
        assert_eq!(completed.turn.status, TurnStatus::Completed);
        let request = wait_for_luna_request(&classifier, index).await?;
        let content = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["role"] == "user")
            .filter_map(|item| item["content"].as_array())
            .flatten()
            .filter_map(|part| part["text"].as_str())
            .collect::<String>();
        let transcript = content
            .split_once(">>> TRANSCRIPT START\n")
            .unwrap()
            .1
            .split_once(">>> TRANSCRIPT END")
            .unwrap()
            .0;
        assert!(
            transcript.contains(prompt),
            "current user input missing: {transcript}"
        );

        let parent = parent_requests.lock().unwrap();
        assert_eq!(parent.len(), (index + 1) * 2);
        if index == 0 {
            let input = parent[1]["input"].as_array().unwrap();
            let output = input
                .iter()
                .find(|item| item["type"] == "function_call_output")
                .expect("the MCP tool must actually execute before compaction");
            let output = output["output"].as_str().unwrap();
            assert!(output.contains(&expected_output), "{output}");
        } else {
            let parent_input = serde_json::to_string(&parent[index * 2]["input"])?;
            assert!(!parent_input.contains(RESTRICTION));
            assert!(!parent_input.contains(EVIDENCE));
            if index == 1 {
                assert!(parent_input.contains(SUMMARY));
                assert!(
                    transcript.contains(RESTRICTION),
                    "retained user restriction missing: {transcript}"
                );
                assert!(transcript.contains(&format!("tool {TEST_TOOL_NAME} call:")));
                assert!(transcript.contains(&format!("tool {TEST_TOOL_NAME} result:")));
                assert!(
                    transcript.contains(&expected_output),
                    "retained MCP output missing: {transcript}"
                );
            } else {
                assert!(!transcript.contains(RESTRICTION));
                assert!(!transcript.contains(EVIDENCE));
                assert!(!transcript.contains("Recheck the repository."));
                assert!(!transcript.contains("\"echoed\":\"current inspection\""));
            }
        }
        classifier.allow_luna.notify_one();
    }

    app_server.shutdown_gracefully().await?;
    mcp_server.abort();
    responses_server.abort();
    Ok(())
}
