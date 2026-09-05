//! Ensures Guardian authorization survives compaction and internal context, but not user changes.

use anyhow::Result;
use codex_core::TurnInputRequest;
use codex_core::context::ContextualUserFragment;
use codex_core::context::InternalContextSource;
use codex_core::context::InternalModelContextFragment;
use codex_features::Feature;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::user_input::UserInput;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guardian_authorization_revision_survives_compaction_not_user_input_or_rollback()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("initial")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config
                .features
                .enable(Feature::TokenBudget)
                .expect("enable context windows");
        })
        .build_with_auto_env(&server)
        .await?;
    test.submit_text_turn("Inspect the deployment.").await?;
    let mut expected = test.codex.guardian_authorization_version().await;

    let internal_context = InternalModelContextFragment::new(
        InternalContextSource::from_static("goal"),
        "Inspecting the deployment.",
    );
    let notification_text = internal_context.render();
    test.codex
        .inject_response_items(vec![ContextualUserFragment::into(internal_context)])
        .await?;
    assert_eq!(test.codex.guardian_authorization_version().await, expected);

    // The same text submitted by the user must invalidate, even if it looks internal.
    responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("user-followup")]),
    )
    .await;
    test.submit_text_turn(&notification_text).await?;
    expected.user_message_revision += 1;
    assert_eq!(test.codex.guardian_authorization_version().await, expected);

    // Failed image preparation must not turn a real user message into internal context.
    responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("user-image")]),
    )
    .await;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Image {
            image_url: "data:image/png;base64,not-an-image".to_owned(),
            detail: None,
        }]))
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    expected.user_message_revision += 1;
    assert_eq!(test.codex.guardian_authorization_version().await, expected);

    test.codex.submit(Op::Compact).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    assert_eq!(test.codex.guardian_authorization_version().await, expected);

    test.codex.ensure_rollout_materialized().await;
    test.codex
        .submit(Op::ThreadRollback { num_turns: 1 })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ThreadRolledBack(_))
    })
    .await;
    assert_ne!(test.codex.guardian_authorization_version().await, expected);
    Ok(())
}
