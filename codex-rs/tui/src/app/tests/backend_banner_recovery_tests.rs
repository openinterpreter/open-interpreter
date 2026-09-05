use super::*;
use crate::app::rate_limit_refresh::RateLimitReadStatus;
use crate::app::rate_limit_refresh::RateLimitRefreshOutcome;
use app_test_support::ChatGptAuthFixture;
use app_test_support::write_chatgpt_auth;
use codex_app_server_protocol::CodexErrorInfo;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_config::types::AuthCredentialsStoreMode;
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn response_with_banner() -> GetAccountRateLimitsResponse {
    serde_json::from_value(json!({
        "accountId":"workspace-a", "rateLimitUpsell":{
            "banner_type":"selected_model_limit", "model_slug":"test-model-a", "presentation":"inline",
            "title":"Selected model usage exhausted", "description":"Switch to another model or contact your owner.",
            "ctas":[{"action":"notify_owner","label":"Notify owner"}]
        },
        "rateLimits":{}, "rateLimitsByLimitId":null,"rateLimitResetCredits":null
    })).unwrap()
}

async fn next_rate_limits_loaded(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) -> Result<AppEvent> {
    Ok(
        tokio::time::timeout(std::time::Duration::from_secs(/*secs*/ 10), async {
            loop {
                let event = events.recv().await.expect("app event channel");
                if matches!(event, AppEvent::RateLimitsLoaded { .. }) {
                    break event;
                }
            }
        })
        .await?,
    )
}

#[tokio::test]
async fn backend_banner_limit_error_refreshes_again_after_intervening_rolling_hard_stop()
-> Result<()> {
    let server = MockServer::start().await;
    let home = tempdir()?;
    write_chatgpt_auth(
        home.path(),
        ChatGptAuthFixture::new("local-test-token")
            .account_id("workspace-a")
            .chatgpt_user_id("user-a")
            .plan_type("team"),
        AuthCredentialsStoreMode::File,
    )
    .expect("write synthetic auth");
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "chatgpt_base_url = {:?}\ncli_auth_credentials_store = \"file\"\n",
            server.uri()
        ),
    )?;
    let (mut app, mut events, _ops) = make_test_app_with_channels().await;
    app.config.codex_home = home.path().to_path_buf().abs();
    app.config.chatgpt_base_url = server.uri();
    app.config.sqlite = codex_state::SqliteConfig::new_for_testing(home.path().abs());
    set_chatgpt_auth(&mut app.chat_widget);
    app.chat_widget.set_model("test-model-a");
    let mut healthy = response_with_banner();
    healthy.rate_limit_upsell = None;
    app.chat_widget.update_backend_banner(&healthy);
    assert!(
        !render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    Mock::given(method("GET"))
        .and(path("/api/codex/usage"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(/*millis*/ 100))
                .set_body_json(json!({
                    "account_id":"workspace-a","user_id":"user-a","plan_type":"team",
                    "rate_limit":{"allowed":true,"limit_reached":false},
                    "rate_limit_upsell":response_with_banner().rate_limit_upsell,
                    "rate_limit_reset_credits":{"available_count":0}
                })),
        )
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/codex/rate-limit-reset-credits"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"available_count":0,"credits":[]})),
        )
        .expect(3)
        .mount(&server)
        .await;
    let mut session = Box::pin(crate::start_embedded_app_server_for_picker(&app.config)).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    while events.try_recv().is_ok() {}
    app.chat_widget.handle_server_notification(
        ServerNotification::Error(ErrorNotification {
            error: AppServerTurnError {
                misalignment: None,
                message: "credits exhausted".into(),
                codex_error_info: Some(CodexErrorInfo::UsageLimitExceeded),
                additional_details: None,
            },
            will_retry: false,
            thread_id: "thread-a".into(),
            turn_id: "turn-a".into(),
        }),
        /*replay_kind*/ None,
    );
    let recovery = loop {
        let event = events
            .try_recv()
            .expect("limit error should request recovery without a command");
        if matches!(
            event,
            AppEvent::RefreshRateLimits {
                origin: RateLimitRefreshOrigin::Recovery
            }
        ) {
            break event;
        }
    };
    app.handle_event(&mut tui, &mut session, recovery).await?;
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::Recovery,
        },
    )
    .await?;
    let mut rolling = response_with_banner().rate_limits;
    rolling.rate_limit_reached_type =
        Some(codex_app_server_protocol::RateLimitReachedType::WorkspaceMemberCreditsDepleted);
    app.handle_app_server_event(
        &session,
        codex_app_server_client::AppServerEvent::ServerNotification(Box::new(
            ServerNotification::AccountRateLimitsUpdated(
                codex_app_server_protocol::AccountRateLimitsUpdatedNotification {
                    rate_limits: rolling.clone(),
                },
            ),
        )),
    )
    .await;
    let loaded = next_rate_limits_loaded(&mut events).await?;
    assert!(
        matches!(&loaded, AppEvent::RateLimitsLoaded { result: Ok(_), .. }),
        "{loaded:?}"
    );
    app.handle_event(&mut tui, &mut session, loaded).await?;
    assert!(
        !render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    // No second inference Error or explicit refresh: the obsolete read starts its own replacement.
    let loaded = next_rate_limits_loaded(&mut events).await?;
    app.handle_event(&mut tui, &mut session, loaded).await?;
    let rendered = render_bottom_popup(&app.chat_widget, /*width*/ 90);
    assert!(rendered.contains("Selected model usage exhausted"));
    assert!(rendered.contains("Notify owner"));
    // A background operation can report the same hard stop after the full response omitted
    // aggregate reached_type: included usage for other models is still available.
    app.handle_app_server_event(
        &session,
        codex_app_server_client::AppServerEvent::ServerNotification(Box::new(
            ServerNotification::AccountRateLimitsUpdated(
                codex_app_server_protocol::AccountRateLimitsUpdatedNotification {
                    rate_limits: rolling,
                },
            ),
        )),
    )
    .await;
    assert!(
        render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    let loaded = next_rate_limits_loaded(&mut events).await?;
    app.handle_event(&mut tui, &mut session, loaded).await?;
    assert!(
        render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    app.chat_widget.set_model("test-model-b");
    assert!(
        !render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    session.shutdown().await?;
    server.verify().await;
    Ok(())
}

#[test]
fn backend_banner_recovery_follows_rolling_invalidation_without_another_error() {
    let mut refresh = crate::app::rate_limit_refresh::RateLimitRefreshState::default();
    let mut epoch = 0;
    let first = refresh
        .start(RateLimitRefreshOrigin::Recovery, &mut epoch)
        .unwrap();
    epoch += 1;
    assert_eq!(
        refresh.finish(first.0, first.1, epoch, RateLimitReadStatus::Succeeded),
        RateLimitRefreshOutcome::RefreshRecovery
    );
}

#[tokio::test]
async fn backend_banner_rolling_only_recovery_holds_new_input() -> Result<()> {
    let (mut app, _events, mut ops) = make_test_app_with_channels().await;
    let mut session = Box::pin(crate::start_embedded_app_server_for_picker(&app.config)).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    app.chat_widget.handle_thread_session(test_thread_session(
        ThreadId::new(),
        app.config.cwd.to_path_buf(),
    ));
    set_chatgpt_auth(&mut app.chat_widget);
    app.chat_widget.set_model("test-model-a");
    let mut rolling = response_with_banner().rate_limits;
    rolling.spend_control_reached = Some(true);
    app.handle_app_server_event(
        &session,
        codex_app_server_client::AppServerEvent::ServerNotification(Box::new(
            ServerNotification::AccountRateLimitsUpdated(
                codex_app_server_protocol::AccountRateLimitsUpdatedNotification {
                    rate_limits: rolling,
                },
            ),
        )),
    )
    .await;
    app.chat_widget.apply_external_edit("follow-up".into());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["follow-up"]
    );
    assert!(
        !std::iter::from_fn(|| ops.try_recv().ok()).any(|op| matches!(op, Op::UserTurn { .. }))
    );
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RateLimitsLoaded {
            request_id: 1,
            origin: RateLimitRefreshOrigin::Recovery,
            hard_stop_generation: app.rate_limit_hard_stop_generation,
            result: Ok(response_with_banner()),
        },
    )
    .await?;
    assert!(
        matches!(next_user_turn_op(&mut ops), Op::UserTurn { items, .. }
        if items == vec![UserInput::Text { text: "follow-up".into(), text_elements: Vec::new() }])
    );
    session.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn backend_banner_account_changes_invalidate_pending_recovery() -> Result<()> {
    let (mut app, _events, _ops) = make_test_app_with_channels().await;
    let mut session = Box::pin(crate::start_embedded_app_server_for_picker(&app.config)).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    app.chat_widget.set_model("test-model-a");
    let current = response_with_banner();
    app.chat_widget.update_backend_banner(&current);
    assert!(
        render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    let pending = app
        .rate_limit_refresh_state
        .start(
            RateLimitRefreshOrigin::Recovery,
            &mut app.rate_limit_hard_stop_generation,
        )
        .unwrap();
    app.handle_app_server_event(
        &session,
        codex_app_server_client::AppServerEvent::ServerNotification(Box::new(
            ServerNotification::AccountUpdated(
                codex_app_server_protocol::AccountUpdatedNotification {
                    auth_mode: None,
                    plan_type: None,
                },
            ),
        )),
    )
    .await;
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RateLimitsLoaded {
            request_id: pending.0,
            origin: RateLimitRefreshOrigin::Recovery,
            hard_stop_generation: pending.1,
            result: Ok(current),
        },
    )
    .await?;
    assert!(
        !render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    assert!(
        app.rate_limit_refresh_state
            .start(
                RateLimitRefreshOrigin::Recovery,
                &mut app.rate_limit_hard_stop_generation
            )
            .is_some()
    );
    session.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn backend_banner_reset_redemption_rejects_pre_reset_content() -> Result<()> {
    let (mut app, _events, mut ops) = make_test_app_with_channels().await;
    let mut session = Box::pin(crate::start_embedded_app_server_for_picker(&app.config)).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    app.chat_widget.handle_thread_session(test_thread_session(
        ThreadId::new(),
        app.config.cwd.to_path_buf(),
    ));
    app.chat_widget.set_model("test-model-a");
    let banner = response_with_banner();
    app.chat_widget.update_backend_banner(&banner);
    assert!(
        render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    let pending = app
        .rate_limit_refresh_state
        .start(
            RateLimitRefreshOrigin::Recovery,
            &mut app.rate_limit_hard_stop_generation,
        )
        .unwrap();
    app.chat_widget.hold_rate_limit_recovery();
    app.chat_widget.apply_external_edit("after reset".into());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["after reset"]
    );
    let request_id = app.chat_widget.show_rate_limit_reset_consuming_popup();
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RateLimitResetCreditConsumed {
            request_id,
            idempotency_key: "mock-reset".into(),
            credit_id: None,
            result: Ok(
                codex_app_server_protocol::ConsumeAccountRateLimitResetCreditResponse {
                    outcome:
                        codex_app_server_protocol::ConsumeAccountRateLimitResetCreditOutcome::Reset,
                },
            ),
        },
    )
    .await?;
    let mut recovered = banner.clone();
    recovered.rate_limit_upsell = None;
    recovered.rate_limit_reset_credits =
        Some(codex_app_server_protocol::RateLimitResetCreditsSummary {
            available_count: 0,
            credits: None,
        });
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RateLimitsLoaded {
            request_id: 2,
            origin: RateLimitRefreshOrigin::ResetConsume { request_id },
            hard_stop_generation: app.rate_limit_hard_stop_generation,
            result: Ok(recovered),
        },
    )
    .await?;
    assert!(
        matches!(next_user_turn_op(&mut ops), Op::UserTurn { items, .. }
        if items == vec![UserInput::Text { text: "after reset".into(), text_elements: Vec::new() }])
    );
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_event(
        &mut tui,
        &mut session,
        AppEvent::RateLimitsLoaded {
            request_id: pending.0,
            origin: RateLimitRefreshOrigin::Recovery,
            hard_stop_generation: pending.1,
            result: Ok(banner),
        },
    )
    .await?;
    assert!(
        !render_bottom_popup(&app.chat_widget, /*width*/ 90)
            .contains("Selected model usage exhausted")
    );
    assert!(
        app.rate_limit_refresh_state
            .start(
                RateLimitRefreshOrigin::Recovery,
                &mut app.rate_limit_hard_stop_generation
            )
            .is_some()
    );
    session.shutdown().await?;
    Ok(())
}

#[test]
fn backend_banner_post_reset_read_preserves_newer_recovery_hold() {
    let mut refresh = crate::app::rate_limit_refresh::RateLimitRefreshState::default();
    let mut epoch = 0;
    let old = refresh
        .start(RateLimitRefreshOrigin::Recovery, &mut epoch)
        .unwrap();
    epoch += 1;
    refresh.invalidate_recovery();
    let reset = refresh
        .start(
            RateLimitRefreshOrigin::ResetConsume { request_id: 1 },
            &mut epoch,
        )
        .unwrap();
    assert_eq!(
        refresh.finish(old.0, old.1, epoch, RateLimitReadStatus::Succeeded),
        RateLimitRefreshOutcome::Ignore
    );
    assert!(refresh.has_pending_recovery());
    let newer = refresh
        .start(RateLimitRefreshOrigin::Recovery, &mut epoch)
        .unwrap();
    assert_eq!(
        refresh.finish(reset.0, reset.1, epoch, RateLimitReadStatus::Succeeded),
        RateLimitRefreshOutcome::Ignore
    );
    assert!(refresh.has_pending_recovery());
    assert_eq!(
        refresh.finish(newer.0, newer.1, epoch, RateLimitReadStatus::Failed),
        RateLimitRefreshOutcome::Ignore
    );
    assert!(!refresh.has_pending_recovery());
}

#[test]
fn backend_banner_recovery_coalesces_until_newer_epoch_can_be_read() {
    let mut refresh = crate::app::rate_limit_refresh::RateLimitRefreshState::default();
    let mut epoch = 0;
    let first = refresh
        .start(RateLimitRefreshOrigin::Recovery, &mut epoch)
        .unwrap();
    epoch += 1; // A newer rolling hard stop makes the in-flight response obsolete.
    for _ in 0..10 {
        assert_eq!(
            refresh.start(RateLimitRefreshOrigin::Recovery, &mut epoch),
            None
        );
    }
    assert_eq!(
        refresh.finish(first.0, first.1, epoch, RateLimitReadStatus::Succeeded),
        RateLimitRefreshOutcome::RefreshRecovery
    );
    let next = refresh
        .start(RateLimitRefreshOrigin::Recovery, &mut epoch)
        .unwrap();
    assert_eq!(
        refresh.finish(next.0, next.1, epoch, RateLimitReadStatus::Succeeded),
        RateLimitRefreshOutcome::Apply
    );
}
