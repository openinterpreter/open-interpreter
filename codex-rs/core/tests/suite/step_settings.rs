use anyhow::Result;
use codex_config::McpServerConfig;
use codex_core::CodexThread;
use codex_core::TurnInputRequest;
use codex_core::config::Constrained;
use codex_core::config::TokenBudgetConfig;
use codex_features::Feature;
use codex_login::CodexAuth;
use codex_models_manager::bundled_models_response;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::config_types::ApprovalsReviewer;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::config_types::ServiceTier;
use codex_protocol::openai_models::ConfirmationPolicies;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelTokenBudgetConfig;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::openai_models::ToolMessage;
use codex_protocol::openai_models::ToolMessages;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::CONTEXT_WINDOW_GUIDANCE_CLOSE_TAG;
use codex_protocol::protocol::CONTEXT_WINDOW_GUIDANCE_OPEN_TAG;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SafetyBufferingEvent;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::protocol::TurnSettingsUpdate;
use codex_protocol::protocol::TurnSettingsUpdateOutcome;
use codex_protocol::request_user_input::RequestUserInputAnswer;
use codex_protocol::request_user_input::RequestUserInputEvent;
use codex_protocol::request_user_input::RequestUserInputResponse;
use codex_protocol::user_input::UserInput;
use core_test_support::apps_test_server::AppsTestServer;
use core_test_support::apps_test_server::recorded_apps_tool_calls;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_models_once;
use core_test_support::responses::mount_response_sequence;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_completed;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use core_test_support::wait_for_mcp_server;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use test_case::test_case;

const MODEL_A: &str = "step-settings-a";
const MODEL_B: &str = "step-settings-b";
const MODEL_C: &str = "step-settings-c";
const TURN_STATE_HEADER: &str = "x-codex-turn-state";

fn step_settings_models() -> Vec<ModelInfo> {
    let model = bundled_models_response()
        .expect("bundled models should parse")
        .models
        .into_iter()
        .find(|model| model.slug == "gpt-5.4")
        .expect("bundled gpt-5.4 model");
    [MODEL_A, MODEL_B, MODEL_C]
        .into_iter()
        .map(|slug| {
            let mut model = model.clone();
            // Tests add model-owned differences when they need to exercise
            // activation or an explicit safety restriction.
            model.slug = slug.to_string();
            model
        })
        .collect()
}

fn step_settings_test() -> TestCodexBuilder {
    test_codex().with_model(MODEL_A).with_config(move |config| {
        for feature in [
            Feature::StepModelSwitching,
            Feature::DefaultModeRequestUserInput,
            Feature::FastMode,
        ] {
            config
                .features
                .enable(feature)
                .expect("test config should allow feature update");
        }
        config.model_catalog = Some(ModelsResponse {
            models: step_settings_models(),
        });
        config.model_reasoning_effort = Some(ReasoningEffort::Low);
        config.model_reasoning_summary = Some(ReasoningSummary::Concise);
        config.service_tier = None;
        config.permissions.approval_policy = Constrained::allow_any(AskForApproval::OnRequest);
        config.approvals_reviewer = ApprovalsReviewer::User;
    })
}

fn paused_response(response_id: &str, call_id: &str) -> String {
    sse(vec![
        ev_response_created(response_id),
        ev_function_call(
            call_id,
            "request_user_input",
            &json!({
                "questions": [{
                    "id": "continue",
                    "header": "Continue",
                    "question": "Continue after the settings update?",
                    "options": [{
                        "label": "Yes (Recommended)",
                        "description": "Continue the current turn."
                    }, {
                        "label": "No",
                        "description": "Stop the current turn."
                    }]
                }]
            })
            .to_string(),
        ),
        ev_completed(response_id),
    ])
}

async fn start_paused_turn(thread: &CodexThread) -> Result<RequestUserInputEvent> {
    thread
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "pause before continuing".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    Ok(wait_for_event_match(thread, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await)
}

async fn answer_paused_turn(thread: &CodexThread, turn_id: &str) -> Result<()> {
    thread
        .submit(Op::UserInputAnswer {
            id: turn_id.to_string(),
            response: RequestUserInputResponse {
                answers: HashMap::from([(
                    "continue".to_string(),
                    RequestUserInputAnswer {
                        answers: vec!["Yes (Recommended)".to_string()],
                    },
                )]),
            },
        })
        .await?;
    Ok(())
}

async fn submit_turn_settings(
    thread: &CodexThread,
    turn_id: &str,
    update: TurnSettingsUpdate,
) -> Result<TurnSettingsUpdateOutcome> {
    let (reply, outcome) = tokio::sync::oneshot::channel();
    thread
        .submit(Op::TurnSettings {
            turn_id: turn_id.to_string(),
            update,
            reply,
        })
        .await?;
    Ok(tokio::time::timeout(std::time::Duration::from_secs(/*secs*/ 10), outcome).await??)
}

fn request_settings(request: &ResponsesRequest) -> Value {
    let body = request.body_json();
    json!({
        "model": body["model"],
        "reasoning": body["reasoning"],
        "service_tier": body.get("service_tier"),
    })
}

fn request_turn_id(request: &ResponsesRequest) -> String {
    let metadata: Value = serde_json::from_str(
        &request
            .header("x-codex-turn-metadata")
            .expect("request should include turn metadata"),
    )
    .expect("valid turn metadata");
    metadata["turn_id"]
        .as_str()
        .expect("request should include turn_id")
        .to_string()
}

#[derive(Clone, Copy)]
enum SettingsTarget {
    Thread,
    Turn,
}

#[test_case(SettingsTarget::Thread; "thread updates stay next-turn-only with the feature enabled")]
#[test_case(SettingsTarget::Turn; "turn updates leave future settings unchanged")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_updates_preserve_turn_identity_and_target(target: SettingsTarget) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_response_sequence(
        &server,
        vec![
            sse_response(paused_response("resp-1", "pause-turn"))
                .insert_header(TURN_STATE_HEADER, "original-turn-state"),
            sse_response(sse_completed("resp-2")),
            sse_response(sse_completed("resp-3")),
        ],
    )
    .await;
    let test = step_settings_test().build_with_auto_env(&server).await?;
    let original_settings = test.codex.thread_settings_snapshot().await;
    let request = start_paused_turn(&test.codex).await?;

    match target {
        SettingsTarget::Thread => {
            test.codex
                .submit(Op::ThreadSettings {
                    thread_settings: ThreadSettingsOverrides {
                        model: Some(MODEL_B.to_string()),
                        effort: Some(Some(ReasoningEffort::High)),
                        summary: Some(ReasoningSummary::Detailed),
                        service_tier: Some(Some(ServiceTier::Fast.request_value().to_string())),
                        ..Default::default()
                    },
                })
                .await?;
        }
        SettingsTarget::Turn => {
            let update = TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                effort: Some(Some(ReasoningEffort::High)),
                summary: Some(ReasoningSummary::Detailed),
                service_tier: Some(Some(ServiceTier::Fast.request_value().to_string())),
                ..Default::default()
            };
            assert_eq!(
                submit_turn_settings(&test.codex, "different-turn", update.clone()).await?,
                TurnSettingsUpdateOutcome::TargetUnavailable
            );
            assert_eq!(
                submit_turn_settings(&test.codex, &request.turn_id, update).await?,
                TurnSettingsUpdateOutcome::Applied
            );
        }
    }
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    let mut settings_events = Vec::new();
    let mut new_turns = Vec::new();
    let completion = wait_for_event(&test.codex, |event| match event {
        EventMsg::ThreadSettingsApplied(event) => {
            settings_events.push(event.thread_settings.clone());
            false
        }
        EventMsg::TurnStarted(event) => {
            new_turns.push(event.turn_id.clone());
            false
        }
        EventMsg::Error(error) => panic!("settings activation failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;
    let EventMsg::TurnComplete(completion) = completion else {
        unreachable!("waited for turn completion")
    };
    assert_eq!(completion.turn_id, request.turn_id);
    assert_eq!(new_turns, Vec::<String>::new());

    let mut changed_settings = original_settings.clone();
    changed_settings.model = MODEL_B.to_string();
    changed_settings.reasoning_effort = Some(ReasoningEffort::High);
    changed_settings.reasoning_summary = Some(ReasoningSummary::Detailed);
    changed_settings.service_tier = Some(ServiceTier::Fast.request_value().to_string());
    changed_settings.collaboration_mode = changed_settings.collaboration_mode.with_updates(
        Some(MODEL_B.to_string()),
        Some(Some(ReasoningEffort::High)),
        /*developer_instructions*/ None,
    );
    let expected_future_settings = match target {
        SettingsTarget::Turn => {
            assert_eq!(settings_events, Vec::new());
            original_settings
        }
        SettingsTarget::Thread => {
            assert_eq!(settings_events, vec![changed_settings.clone()]);
            changed_settings
        }
    };
    assert_eq!(
        test.codex.thread_settings_snapshot().await,
        expected_future_settings
    );
    test.submit_text_turn("start the next turn").await?;

    let original_request_settings = json!({
        "model": MODEL_A,
        "reasoning": { "effort": "low", "summary": "concise" },
        "service_tier": null,
    });
    let changed_request_settings = json!({
        "model": MODEL_B,
        "reasoning": { "effort": "high", "summary": "detailed" },
        "service_tier": "priority",
    });
    let (continued_settings, next_turn_settings) = match target {
        SettingsTarget::Thread => (original_request_settings.clone(), changed_request_settings),
        SettingsTarget::Turn => (changed_request_settings, original_request_settings.clone()),
    };
    let requests = response_mock.requests();
    assert_eq!(
        requests.iter().map(request_settings).collect::<Vec<_>>(),
        vec![
            original_request_settings,
            continued_settings,
            next_turn_settings,
        ]
    );
    assert_eq!(request_turn_id(&requests[0]), request.turn_id);
    assert_eq!(request_turn_id(&requests[1]), request.turn_id);
    assert_ne!(request_turn_id(&requests[2]), request.turn_id);
    let session_id = requests[0]
        .header("session-id")
        .expect("initial request session id");
    assert_eq!(requests[1].header("session-id"), Some(session_id));
    assert_eq!(requests[0].header(TURN_STATE_HEADER), None);
    assert_eq!(
        requests[1].header(TURN_STATE_HEADER),
        Some("original-turn-state".to_string())
    );
    assert_eq!(requests[2].header(TURN_STATE_HEADER), None);

    Ok(())
}

#[derive(Clone, Copy)]
enum TokenBudgetScenario {
    ModelDefaults,
    ExplicitDefaultTemplate,
    ReloadPreferences,
    DestinationWindowOnly,
    InitialWindowOnly,
    DestinationWithoutGuidance,
}

#[test_case(TokenBudgetScenario::ModelDefaults)]
#[test_case(TokenBudgetScenario::ExplicitDefaultTemplate)]
#[test_case(TokenBudgetScenario::ReloadPreferences)]
#[test_case(TokenBudgetScenario::DestinationWindowOnly)]
#[test_case(TokenBudgetScenario::InitialWindowOnly)]
#[test_case(TokenBudgetScenario::DestinationWithoutGuidance)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_model_switch_resolves_token_budget_from_original_preferences(
    scenario: TokenBudgetScenario,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let explicit_default_template =
        matches!(scenario, TokenBudgetScenario::ExplicitDefaultTemplate);
    let reload_user_config = matches!(scenario, TokenBudgetScenario::ReloadPreferences);
    let context_window_model = match scenario {
        TokenBudgetScenario::DestinationWindowOnly => Some(MODEL_B),
        TokenBudgetScenario::InitialWindowOnly => Some(MODEL_A),
        TokenBudgetScenario::ModelDefaults
        | TokenBudgetScenario::ExplicitDefaultTemplate
        | TokenBudgetScenario::ReloadPreferences
        | TokenBudgetScenario::DestinationWithoutGuidance => None,
    };
    let destination_has_guidance =
        !matches!(scenario, TokenBudgetScenario::DestinationWithoutGuidance);
    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-model-budget-switch"),
            paused_response("resp-2", "pause-after-model-budget-switch"),
            sse_completed("resp-3"),
        ],
    )
    .await;
    let test = step_settings_test()
        .with_pre_build_hook(move |home| {
            let config = if explicit_default_template {
                let default_template = TokenBudgetConfig::default().reminder_message_template;
                format!(
                    "[features.token_budget]\nenabled = true\nreminder_message_template = {default_template:?}\n"
                )
            } else {
                "[features.token_budget]\nenabled = true\n".to_string()
            };
            std::fs::write(home.join("config.toml"), config)
                .expect("write token-budget preferences");
        })
        .with_config(move |config| {
            config
                .features
                .enable(Feature::TokenBudget)
                .expect("enable token-budget feature");
            if context_window_model.is_some() {
                config.model_context_window = None;
            }
            for model in &mut config
                .model_catalog
                .as_mut()
                .expect("controlled model catalog")
                .models
            {
                let slug = model.slug.clone();
                let initial_model = slug == MODEL_A;
                if let Some(context_window_model) = context_window_model {
                    model.context_window = (slug == context_window_model).then_some(128_000);
                    model.max_context_window = None;
                }
                model
                    .model_messages
                    .as_mut()
                    .expect("model messages")
                    .token_budget = (initial_model || destination_has_guidance).then(|| ModelTokenBudgetConfig {
                    enabled: false,
                    use_history_notes_extension: false,
                    reminder_threshold_tokens: if initial_model { 8_000 } else { 2_000 },
                    reminder_message_template: format!(
                        "Reminder for {slug}: {{n_remaining}} tokens remain."
                    ),
                    guidance_message: format!("Use {slug} token-budget guidance."),
                    auto_compact_fallback_prompt: format!("Save {slug} state before rollover."),
                    auto_compact_fallback_buffer_tokens: if initial_model {
                        16_000
                    } else {
                        4_000
                    },
                });
            }
        })
        .build_with_auto_env(&server)
        .await?;
    let request = start_paused_turn(&test.codex).await?;

    if reload_user_config {
        std::fs::write(
            test.codex_home_path().join("config.toml"),
            "[features.token_budget]\nenabled = true\nreminder_message_template = \"Reloaded reminder\"\n",
        )?;
        test.codex.submit(Op::ReloadUserConfig).await?;
    }

    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            },
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    let request = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::Error(error) => panic!("settings activation failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;

    let requests = response_mock.requests();
    assert_eq!(requests.len(), 3);
    let initial_guidance = format!("Use {MODEL_A} token-budget guidance.");
    let initial_guidance_expected =
        !explicit_default_template && context_window_model != Some(MODEL_B);
    assert_eq!(
        requests[0].body_contains_text(&initial_guidance),
        initial_guidance_expected
    );
    let mut expected_guidance = Vec::new();
    if initial_guidance_expected {
        expected_guidance.push(format!(
            "{CONTEXT_WINDOW_GUIDANCE_OPEN_TAG}\n{initial_guidance}\n{CONTEXT_WINDOW_GUIDANCE_CLOSE_TAG}"
        ));
    }
    if !explicit_default_template
        && destination_has_guidance
        && context_window_model != Some(MODEL_A)
    {
        let replacement_notice = if initial_guidance_expected {
            "This context-window guidance replaces all previously provided context-window guidance.\n\n"
        } else {
            ""
        };
        expected_guidance.push(format!(
            "{CONTEXT_WINDOW_GUIDANCE_OPEN_TAG}\n{replacement_notice}Use {MODEL_B} token-budget guidance.\n{CONTEXT_WINDOW_GUIDANCE_CLOSE_TAG}"
        ));
    } else if initial_guidance_expected {
        expected_guidance.push(format!(
            "{CONTEXT_WINDOW_GUIDANCE_OPEN_TAG}\nThe previously provided context-window guidance no longer applies.\n{CONTEXT_WINDOW_GUIDANCE_CLOSE_TAG}"
        ));
    }
    for request in &requests[1..] {
        let guidance = request
            .message_input_texts("developer")
            .into_iter()
            .filter(|text| text.starts_with(CONTEXT_WINDOW_GUIDANCE_OPEN_TAG))
            .collect::<Vec<_>>();
        assert_eq!(
            guidance, expected_guidance,
            "preserve history and append the guidance transition only once"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_confirmation_policy_follows_step_model_changes() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const BROWSER_POLICY_A: &str = "  # Browser policy A\r\n{literal}\n";
    const BROWSER_POLICY_B: &str = "\t# Browser policy B\n<raw> & café\r\n ";
    const COMPUTER_POLICY_A: &str = "\t# Native policy A\n{{literal}}\r\n";
    const COMPUTER_POLICY_B: &str = "  # Native policy B\r\n<computer> ${native}\n ";
    const BROWSER_ONLY_MODEL: &str = "policy-browser-only";
    const COMPUTER_ONLY_MODEL: &str = "policy-computer-only";
    let server = start_mock_server().await;
    AppsTestServer::mount(&server).await?;
    let policy_call = |response_id: &str, call_id: &str| {
        sse(vec![
            ev_response_created(response_id),
            ev_function_call_with_namespace(
                call_id,
                "mcp__node_repl",
                "calendar_list_events",
                "{}",
            ),
            ev_completed(response_id),
        ])
    };
    mount_sse_sequence(
        &server,
        vec![
            policy_call("resp-1", "policy-a"),
            sse_completed("resp-2"),
            sse(vec![
                ev_response_created("resp-3"),
                ev_function_call_with_namespace(
                    "policy-a-pending",
                    "mcp__node_repl",
                    "calendar_create_event",
                    &json!({
                        "title": "Policy snapshot test",
                        "starts_at": "2026-08-26T12:00:00Z",
                    })
                    .to_string(),
                ),
                ev_completed("resp-3"),
            ]),
            policy_call("resp-4", "policy-b"),
            sse_completed("resp-5"),
            policy_call("resp-6", "browser-only"),
            sse_completed("resp-7"),
            policy_call("resp-8", "computer-only"),
            sse_completed("resp-9"),
            policy_call("resp-10", "no-policy"),
            sse_completed("resp-11"),
        ],
    )
    .await;
    let mcp_url = format!("{}/api/codex/ps/mcp", server.uri());
    let test = step_settings_test()
        .with_config(move |config| {
            config
                .features
                .disable(Feature::ToolCallMcpElicitation)
                .expect("disable MCP elicitation for the approval barrier");
            let models = &mut config.model_catalog.as_mut().expect("test models").models;
            for slug in [BROWSER_ONLY_MODEL, COMPUTER_ONLY_MODEL] {
                let mut model = models[0].clone();
                model.slug = slug.to_string();
                models.push(model);
            }
            for model in models {
                let messages = model
                    .model_messages
                    .as_mut()
                    .expect("bundled model messages");
                messages.confirmation_policies = match model.slug.as_str() {
                    MODEL_A => Some(ConfirmationPolicies {
                        browser_use: Some(BROWSER_POLICY_A.to_string()),
                        computer_use: Some(COMPUTER_POLICY_A.to_string()),
                    }),
                    MODEL_B => Some(ConfirmationPolicies {
                        browser_use: Some(BROWSER_POLICY_B.to_string()),
                        computer_use: Some(COMPUTER_POLICY_B.to_string()),
                    }),
                    BROWSER_ONLY_MODEL => Some(ConfirmationPolicies {
                        browser_use: Some(BROWSER_POLICY_B.to_string()),
                        computer_use: None,
                    }),
                    COMPUTER_ONLY_MODEL => Some(ConfirmationPolicies {
                        browser_use: None,
                        computer_use: Some(COMPUTER_POLICY_B.to_string()),
                    }),
                    MODEL_C => None,
                    _ => unreachable!("unexpected test model"),
                };
            }
            let node_repl: McpServerConfig = serde_json::from_value(json!({
                "url": mcp_url,
                "tools": {
                    "calendar_create_event": {
                        "approval_mode": "prompt",
                    },
                },
            }))
            .expect("valid test MCP server");
            config
                .mcp_servers
                .set(HashMap::from([("node_repl".to_string(), node_repl)]))
                .expect("configure test MCP server");
        })
        .build_with_auto_env(&server)
        .await?;
    wait_for_mcp_server(&test.codex, "node_repl").await?;
    test.submit_text_turn("call the tool with model A").await?;

    let request = start_paused_turn(&test.codex).await?;
    assert_eq!(request.call_id, "policy-a-pending");
    // The pending call must retain model A's policies after this settings update.
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            },
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied,
    );
    test.codex
        .submit(Op::UserInputAnswer {
            id: request.turn_id.clone(),
            response: RequestUserInputResponse {
                answers: HashMap::from([(
                    request.questions[0].id.clone(),
                    RequestUserInputAnswer {
                        answers: vec!["Allow".to_string()],
                    },
                )]),
            },
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    for model in [BROWSER_ONLY_MODEL, COMPUTER_ONLY_MODEL, MODEL_C] {
        core_test_support::submit_thread_settings(
            &test.codex,
            ThreadSettingsOverrides {
                model: Some(model.to_string()),
                ..Default::default()
            },
        )
        .await?;
        test.submit_text_turn("call the tool").await?;
    }

    assert_eq!(
        recorded_apps_tool_calls(&server)
            .await
            .into_iter()
            .map(|call| {
                let meta = &call["params"]["_meta"];
                (
                    meta["callId"].clone(),
                    meta["openai/confirmation_policies"].clone(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                json!("policy-a"),
                json!({
                    "browser_use": BROWSER_POLICY_A,
                    "computer_use": COMPUTER_POLICY_A,
                })
            ),
            (
                json!("policy-a-pending"),
                json!({
                    "browser_use": BROWSER_POLICY_A,
                    "computer_use": COMPUTER_POLICY_A,
                })
            ),
            (
                json!("policy-b"),
                json!({
                    "browser_use": BROWSER_POLICY_B,
                    "computer_use": COMPUTER_POLICY_B,
                })
            ),
            (
                json!("browser-only"),
                json!({"browser_use": BROWSER_POLICY_B})
            ),
            (
                json!("computer-only"),
                json!({"computer_use": COMPUTER_POLICY_B})
            ),
            (json!("no-policy"), json!({})),
        ],
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_metadata_uses_the_captured_model_after_a_turn_update() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut created = ev_response_created("resp-2");
    created["safety_buffering"] = json!({
        "use_cases": ["cyber"],
        "reasons": ["policy-check"],
        "retry_model": MODEL_C,
    });
    let response_mock = mount_response_sequence(
        &server,
        vec![
            sse_response(paused_response("resp-1", "pause-turn")),
            sse_response(sse(vec![created, ev_completed("resp-2")]))
                .insert_header("OpenAI-Model", MODEL_B),
        ],
    )
    .await;
    let test = step_settings_test().build_with_auto_env(&server).await?;
    let request = start_paused_turn(&test.codex).await?;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            },
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &request.turn_id).await?;

    let mut reroutes = Vec::new();
    let mut buffering_events = Vec::new();
    wait_for_event(&test.codex, |event| match event {
        EventMsg::ModelReroute(event) => {
            reroutes.push(event.clone());
            false
        }
        EventMsg::SafetyBuffering(event) => {
            buffering_events.push(event.clone());
            false
        }
        EventMsg::Error(error) => panic!("sampling failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;

    assert_eq!(
        response_mock
            .requests()
            .iter()
            .map(|request| request.body_json()["model"].clone())
            .collect::<Vec<_>>(),
        vec![json!(MODEL_A), json!(MODEL_B)]
    );
    // B's matching response header is not a reroute from the turn's initial A.
    // Buffering metadata likewise belongs to the captured B step.
    assert_eq!(
        (reroutes, buffering_events),
        (
            vec![],
            vec![SafetyBufferingEvent {
                model: MODEL_B.to_string(),
                use_cases: vec!["cyber".to_string()],
                reasons: vec!["policy-check".to_string()],
                show_buffering_ui: true,
                faster_model: Some(MODEL_C.to_string()),
            }],
        )
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sparse_updates_preserve_divergent_active_and_future_models() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-first-step"),
            paused_response("resp-2", "pause-second-step"),
            sse_completed("resp-3"),
            sse_completed("resp-4"),
        ],
    )
    .await;
    let test = step_settings_test().build_with_auto_env(&server).await?;
    let request = start_paused_turn(&test.codex).await?;

    core_test_support::submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            model: Some(MODEL_B.to_string()),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_C.to_string()),
                effort: Some(Some(ReasoningEffort::High)),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    let second_request = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    assert_eq!(second_request.turn_id, request.turn_id);
    assert_eq!(second_request.call_id, "pause-second-step");

    test.codex
        .submit(Op::ThreadSettings {
            thread_settings: ThreadSettingsOverrides {
                service_tier: Some(Some(ServiceTier::Fast.request_value().to_string())),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                service_tier: Some(Some(ServiceTier::Fast.request_value().to_string())),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    let durable_settings = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::ThreadSettingsApplied(event) => Some(event.thread_settings.clone()),
        _ => None,
    })
    .await;
    assert_eq!(durable_settings.model, MODEL_B);
    assert_eq!(
        durable_settings.reasoning_effort,
        Some(ReasoningEffort::Low)
    );
    assert_eq!(
        durable_settings.service_tier,
        Some(ServiceTier::Fast.request_value().to_string())
    );
    answer_paused_turn(&test.codex, &second_request.turn_id).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    test.submit_text_turn("start the next turn").await?;

    let requests = response_mock.requests();
    assert_eq!(
        requests.iter().map(request_settings).collect::<Vec<_>>(),
        vec![
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_C,
                "reasoning": { "effort": "high", "summary": "concise" },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_C,
                "reasoning": { "effort": "high", "summary": "concise" },
                "service_tier": "priority",
            }),
            json!({
                "model": MODEL_B,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": "priority",
            }),
        ]
    );
    assert_eq!(
        requests[..3]
            .iter()
            .map(request_turn_id)
            .collect::<Vec<_>>(),
        vec![request.turn_id.clone(); 3]
    );
    assert_ne!(request_turn_id(&requests[3]), request.turn_id);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_settings_do_not_target_idle_or_finished_turns() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![sse_completed("resp-1"), sse_completed("resp-2")],
    )
    .await;
    let test = step_settings_test().build_with_auto_env(&server).await?;

    for (discarded_model, model) in [(MODEL_C, MODEL_B), (MODEL_A, MODEL_C)] {
        let turn_id = response_mock
            .requests()
            .last()
            .map(request_turn_id)
            .unwrap_or_else(|| "never-started".to_string());
        let before = test.codex.thread_settings_snapshot().await;
        assert_eq!(
            submit_turn_settings(
                &test.codex,
                &turn_id,
                TurnSettingsUpdate {
                    model: Some(discarded_model.to_string()),
                    ..Default::default()
                }
            )
            .await?,
            TurnSettingsUpdateOutcome::TargetUnavailable
        );
        assert_eq!(test.codex.thread_settings_snapshot().await, before);
        core_test_support::submit_thread_settings(
            &test.codex,
            ThreadSettingsOverrides {
                model: Some(model.to_string()),
                ..Default::default()
            },
        )
        .await?;
        test.submit_text_turn("start the next turn").await?;
    }

    assert_eq!(
        response_mock
            .requests()
            .iter()
            .map(request_settings)
            .collect::<Vec<_>>(),
        vec![
            json!({
                "model": MODEL_B,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_C,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": null,
            }),
        ]
    );

    Ok(())
}

#[test_case(None, "detailed"; "unset summary follows the destination model")]
#[test_case(Some(ReasoningSummary::Concise), "concise"; "explicit summary is preserved")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_activation_uses_destination_metadata_defaults(
    configured_summary: Option<ReasoningSummary>,
    expected_summary: &str,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-model-change"),
            paused_response("resp-2", "pause-before-restoring-model"),
            sse_completed("resp-3"),
        ],
    )
    .await;
    let test = step_settings_test()
        .with_config(move |config| {
            config.model_context_window = None;
            config.model_auto_compact_token_limit = None;
            config.model_reasoning_summary = configured_summary;
            config.service_tier = Some(ServiceTier::Fast.request_value().to_string());
            config.base_instructions = None;
            for model in &mut config
                .model_catalog
                .as_mut()
                .expect("controlled model catalog")
                .models
            {
                model.context_window = Some(256_000);
                model.auto_compact_token_limit = Some(200_000);
                model.default_reasoning_summary = ReasoningSummary::Concise;
                if model.slug == MODEL_B {
                    model.context_window = Some(128_000);
                    model.auto_compact_token_limit = Some(100_000);
                    model.default_reasoning_summary = ReasoningSummary::Detailed;
                    model
                        .service_tiers
                        .retain(|tier| tier.id != ServiceTier::Fast.request_value());
                    model
                        .model_messages
                        .as_mut()
                        .expect("model instruction metadata")
                        .instructions_template =
                        Some("Destination model instructions.".to_string());
                }
            }
        })
        .build_with_auto_env(&server)
        .await?;
    let request = start_paused_turn(&test.codex).await?;

    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    let paused = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    // B cannot use the requested tier. Switching back must recover that
    // selection and preserve an unset summary, not reuse B's effective values.
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_A.to_string()),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &paused.turn_id).await?;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::Error(error) => panic!("model activation failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;

    let requests = response_mock.requests();
    assert_eq!(
        requests.iter().map(request_settings).collect::<Vec<_>>(),
        vec![
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": "priority",
            }),
            json!({
                "model": MODEL_B,
                "reasoning": { "effort": "low", "summary": expected_summary },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": "priority",
            }),
        ]
    );
    assert_eq!(
        requests.iter().map(request_turn_id).collect::<Vec<_>>(),
        vec![request.turn_id; 3],
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_async_description_follows_mid_turn_model_changes() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-async-message-model-change"),
            sse_completed("resp-2"),
        ],
    )
    .await;
    let test = step_settings_test()
        .with_config(|config| {
            for model in &mut config
                .model_catalog
                .as_mut()
                .expect("controlled model catalog")
                .models
            {
                model
                    .experimental_supported_tools
                    .push("send_user_message_async".to_string());
                model
                    .model_messages
                    .as_mut()
                    .expect("model instruction metadata")
                    .tools = Some(ToolMessages {
                    send_user_message_async: Some(ToolMessage {
                        description: Some(format!("Async message description for {}.", model.slug)),
                    }),
                });
            }
        })
        .build_with_auto_env(&server)
        .await?;
    let paused = start_paused_turn(&test.codex).await?;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &paused.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &paused.turn_id).await?;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::Error(error) => panic!("model activation failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;

    assert_eq!(
        response_mock
            .requests()
            .iter()
            .map(|request| {
                let body = request.body_json();
                let tool = body["tools"]
                    .as_array()
                    .expect("request tools")
                    .iter()
                    .find(|tool| tool["name"] == "request_user_input_async")
                    .expect("async message tool");
                json!({"model": body["model"], "description": tool["description"]})
            })
            .collect::<Vec<_>>(),
        [MODEL_A, MODEL_B]
            .map(|model| json!({
                "model": model,
                "description": format!("Async message description for {model}."),
            }))
            .to_vec(),
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistent_instructions_follow_mid_turn_model_changes() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-persistent-model-change"),
            sse_completed("resp-2"),
        ],
    )
    .await;
    let test = step_settings_test()
        .with_config(|config| {
            config.model_reasoning_effort = Some(ReasoningEffort::Persistent);
            for model in &mut config
                .model_catalog
                .as_mut()
                .expect("controlled model catalog")
                .models
            {
                model
                    .supported_reasoning_levels
                    .push(ReasoningEffortPreset {
                        effort: ReasoningEffort::Persistent,
                        description: ReasoningEffort::Persistent.to_string(),
                    });
                model
                    .model_messages
                    .as_mut()
                    .expect("model instruction metadata")
                    .persistent_instructions =
                    Some(format!("Persistent instructions for {}.", model.slug));
            }
        })
        .build_with_auto_env(&server)
        .await?;
    let paused = start_paused_turn(&test.codex).await?;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &paused.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &paused.turn_id).await?;
    wait_for_event(&test.codex, |event| match event {
        EventMsg::Error(error) => panic!("model activation failed: {}", error.message),
        EventMsg::TurnComplete(_) => true,
        _ => false,
    })
    .await;

    let initial =
        format!("<persistent_mode>\nPersistent instructions for {MODEL_A}.\n</persistent_mode>");
    let update = format!(
        "<persistent_mode>\nThese persistent-mode instructions replace all previously provided persistent-mode instructions.\n\nPersistent instructions for {MODEL_B}.\n</persistent_mode>"
    );
    let requests = response_mock.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| {
                let instructions = request
                    .message_input_texts("developer")
                    .into_iter()
                    .filter(|text| text.starts_with("<persistent_mode>"))
                    .collect::<Vec<_>>();
                json!({"model": request.body_json()["model"], "instructions": instructions})
            })
            .collect::<Vec<_>>(),
        vec![
            json!({"model": MODEL_A, "instructions": [initial]}),
            json!({"model": MODEL_B, "instructions": [initial, update]}),
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_settings_rejection_preserves_independent_future_settings() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-node-repl-restriction"),
            sse_completed("resp-2"),
            sse_completed("resp-3"),
        ],
    )
    .await;
    let test = step_settings_test()
        .with_config(|config| {
            for model in &mut config
                .model_catalog
                .as_mut()
                .expect("controlled model catalog")
                .models
            {
                model.node_repl_disabled = model.slug == MODEL_B;
            }
        })
        .build_with_auto_env(&server)
        .await?;
    let request = start_paused_turn(&test.codex).await?;
    core_test_support::submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            model: Some(MODEL_B.to_string()),
            ..Default::default()
        },
    )
    .await?;
    let future = test.codex.thread_settings_snapshot().await;
    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                model: Some(MODEL_B.to_string()),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Rejected {
            reason: "the destination changes the admitted node REPL availability restriction"
                .to_string(),
        }
    );
    assert_eq!(test.codex.thread_settings_snapshot().await, future);
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    test.submit_text_turn("admit the next turn").await?;

    let requests = response_mock.requests();
    assert_eq!(
        requests.iter().map(request_settings).collect::<Vec<_>>(),
        [MODEL_A, MODEL_A, MODEL_B]
            .into_iter()
            .map(|model| {
                json!({
                    "model": model,
                    "reasoning": { "effort": "low", "summary": "concise" },
                    "service_tier": null,
                })
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(request_turn_id(&requests[1]), request.turn_id);
    assert_ne!(request_turn_id(&requests[2]), request.turn_id);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_preference_activation_keeps_admitted_model_metadata() -> Result<()> {
    skip_if_no_network!(Ok(()));

    // This test owns both catalog responses. The generic mock-server helper
    // installs an extra one-shot /models response that would shift the sequence.
    let server = wiremock::MockServer::start().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            paused_response("resp-1", "pause-before-models-refresh"),
            sse_completed("resp-2"),
            sse_completed("resp-3"),
        ],
    )
    .await;
    let mut models = step_settings_models();
    for model in &mut models {
        model.default_reasoning_summary = ReasoningSummary::Concise;
    }
    let initial_catalog = mount_models_once(
        &server,
        ModelsResponse {
            models: models.clone(),
        },
    )
    .await;
    let test = step_settings_test()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(|config| {
            // Use the real refreshable models manager, and retain an unset
            // preference so the selected model's default is observable.
            config.model_catalog = None;
            config.model_reasoning_summary = None;
        })
        .build_with_auto_env(&server)
        .await?;
    let request = start_paused_turn(&test.codex).await?;
    assert_eq!(initial_catalog.requests().len(), 1);

    for model in &mut models {
        model.default_reasoning_summary = ReasoningSummary::Detailed;
    }
    let refresh = mount_models_once(&server, ModelsResponse { models }).await;
    test.thread_manager
        .get_models_manager()
        .list_models(
            RefreshStrategy::Online,
            codex_core::test_support::default_http_client_factory(),
        )
        .await;
    assert_eq!(refresh.requests().len(), 1);

    assert_eq!(
        submit_turn_settings(
            &test.codex,
            &request.turn_id,
            TurnSettingsUpdate {
                effort: Some(Some(ReasoningEffort::High)),
                ..Default::default()
            }
        )
        .await?,
        TurnSettingsUpdateOutcome::Applied
    );
    answer_paused_turn(&test.codex, &request.turn_id).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    test.submit_text_turn("admit the next turn").await?;

    assert_eq!(
        response_mock
            .requests()
            .iter()
            .map(request_settings)
            .collect::<Vec<_>>(),
        vec![
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "low", "summary": "concise" },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "high", "summary": "concise" },
                "service_tier": null,
            }),
            json!({
                "model": MODEL_A,
                "reasoning": { "effort": "low", "summary": "detailed" },
                "service_tier": null,
            }),
        ]
    );

    Ok(())
}
