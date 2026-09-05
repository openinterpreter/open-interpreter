use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::TestAppServer;
use app_test_support::write_chatgpt_auth;
use codex_app_server_protocol::CyberAccessProgram;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_login::AuthCredentialsStoreMode;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test]
async fn turn_start_forwards_explicit_cyber_access_program() -> Result<()> {
    core_test_support::skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(426))
        .mount(&server)
        .await;
    let programs = [
        None,
        Some(CyberAccessProgram::DaybreakBlue),
        Some(CyberAccessProgram::DaybreakRed),
        None,
        Some(CyberAccessProgram::Standard),
        None,
    ];
    let requests = responses::mount_sse_sequence(
        &server,
        programs
            .iter()
            .enumerate()
            .map(|(index, _)| {
                responses::sse(vec![responses::ev_completed(&format!("resp-{index}"))])
            })
            .collect(),
    )
    .await;
    let home = TempDir::new()?;
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "model = \"gpt-5.5\"\napproval_policy = \"never\"\nopenai_base_url = \"{}/v1\"\ncli_auth_credentials_store = \"file\"\n",
            server.uri()
        ),
    )?;
    write_chatgpt_auth(
        home.path(),
        ChatGptAuthFixture::new("chatgpt-test-token").plan_type("pro"),
        AuthCredentialsStoreMode::File,
    )?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .without_managed_config()
        .build_initialized()
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    for program in programs {
        let completed = app
            .start_turn_and_wait_for_completion(TurnStartParams {
                thread_id: thread.id.clone(),
                cyber_access_program: program,
                input: vec![UserInput::Text {
                    text: "hello".to_owned(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            })
            .await?;
        assert_eq!(completed.turn.status, TurnStatus::Completed);
    }
    let requests = requests.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.body_json().get("access_programs").cloned())
            .collect::<Vec<_>>(),
        [
            None,
            Some(json!({"cyber": "daybreak_blue"})),
            Some(json!({"cyber": "daybreak_red"})),
            None,
            Some(json!({"cyber": "standard"})),
            None,
        ]
    );
    Ok(())
}

#[tokio::test]
async fn turn_start_forwards_cyber_access_program_with_personal_access_token() -> Result<()> {
    core_test_support::skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(426))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/user-auth-credential/whoami"))
        .and(header("Authorization", "Bearer at-test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "email": null,
            "chatgpt_user_id": "user-123",
            "chatgpt_account_id": "account-123",
            "chatgpt_plan_type": "enterprise_cbp_automation",
            "chatgpt_account_is_fedramp": false,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/backend-api/wham/config/bundle"))
        .and(header("Authorization", "Bearer at-test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    let request = responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("resp-1")]),
    )
    .await;
    let home = TempDir::new()?;
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "model = \"gpt-5.5\"\napproval_policy = \"never\"\nopenai_base_url = \"{0}/v1\"\nchatgpt_base_url = \"{0}/backend-api\"\n",
            server.uri(),
        ),
    )?;
    let authapi_base_url = server.uri();
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .without_managed_config()
        .with_env_overrides(&[
            ("OPENAI_API_KEY", None),
            ("CODEX_ACCESS_TOKEN", Some("at-test-token")),
            ("CODEX_AUTHAPI_BASE_URL", Some(authapi_base_url.as_str())),
        ])
        .build_initialized()
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;

    app.start_turn_and_wait_for_completion(TurnStartParams {
        thread_id: thread.id,
        cyber_access_program: Some(CyberAccessProgram::DaybreakBlue),
        input: vec![UserInput::Text {
            text: "hello".to_owned(),
            text_elements: Vec::new(),
        }],
        ..Default::default()
    })
    .await?;

    assert_eq!(
        request.single_request().body_json()["access_programs"],
        json!({"cyber": "daybreak_blue"})
    );
    Ok(())
}
