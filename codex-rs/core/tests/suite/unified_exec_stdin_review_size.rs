//! Stdin approval must cover the complete input before any bytes reach a terminal.

use anyhow::Result;
use codex_core::TurnInputRequest;
use codex_features::Feature;
use codex_protocol::approvals::ExecApprovalKind;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::ReviewDecision;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::skip_if_sandbox;
use core_test_support::skip_if_target_windows;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::json;

// The oversized input fits in 8KB before JSON escaping. Neither rejected input
// may reach the shell, including the trailing assignment.
#[test_case::test_case(
    format!("#{}\nREJECTED=1\n", "\"".repeat(4_100)),
    "too large to review safely";
    "oversized"
)]
#[test_case::test_case("\0REJECTED=1\n".to_string(), "NUL byte"; "nul")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unreviewable_stdin_is_rejected_before_approval_or_execution(
    rejected_input: String,
    expected_error: &str,
) -> Result<()> {
    skip_if_target_windows!(Ok(()), "uses a POSIX interactive shell");
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::WriteStdinApproval)
            .expect("enable stdin approvals");
    });
    let test = builder.build_with_auto_env(&server).await?;
    let mut sequence = Vec::new();
    for (id, tool, args) in [
        (
            "open",
            "exec_command",
            json!({"cmd":"/bin/bash --noprofile --norc", "tty":true, "yield_time_ms":200, "sandbox_permissions":"require_escalated"}),
        ),
        (
            "rejected",
            "write_stdin",
            json!({"session_id":1000, "chars":rejected_input, "yield_time_ms":1000}),
        ),
        (
            "allowed",
            "write_stdin",
            json!({"session_id":1000, "chars":"test -z \"${REJECTED-}\" && printf 'STDIN_%s_OK\\n' REVIEW; exit\n", "yield_time_ms":1000}),
        ),
    ] {
        sequence.push(sse(vec![
            ev_function_call(id, tool, &args.to_string()),
            ev_completed(id),
        ]));
    }
    sequence.push(sse(vec![ev_completed("done")]));
    let responses = mount_sse_sequence(&server, sequence).await;
    test.codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "send terminal input".to_string(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                approval_policy: Some(AskForApproval::OnRequest),
                sandbox_policy: Some(SandboxPolicy::ReadOnly {
                    network_access: false,
                }),
                ..Default::default()
            }),
        )
        .await?;
    let mut approvals = Vec::new();
    loop {
        match wait_for_event(&test.codex, |_| true).await {
            EventMsg::ExecApprovalRequest(request) => {
                let id = request.effective_approval_id();
                approvals.push((request.kind, id.clone()));
                test.codex
                    .submit(Op::ExecApproval {
                        id,
                        turn_id: Some(request.turn_id),
                        decision: ReviewDecision::Approved,
                    })
                    .await?;
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }
    assert_eq!(
        approvals,
        vec![
            (ExecApprovalKind::Command, "open".to_string()),
            (ExecApprovalKind::WriteStdin, "allowed".to_string()),
        ]
    );
    let requests = responses.requests();
    let last = requests.last().expect("final model request");
    let rejected = last.function_call_output("rejected");
    assert!(rejected.to_string().contains(expected_error), "{rejected}");
    let allowed = last.function_call_output("allowed");
    assert!(allowed.to_string().contains("STDIN_REVIEW_OK"), "{allowed}");
    test.codex.submit(Op::Shutdown).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ShutdownComplete)
    })
    .await;
    Ok(())
}
