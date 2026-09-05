//! Covers OAuth connection replacement when a running thread refreshes its feature config.

use super::EnvVarGuard;
use super::OAuthCredentialExpiry;
use super::write_fallback_oauth_tokens;
use std::sync::Arc;

use codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID;
use codex_config::McpServerConfig;
use codex_config::types::OAuthCredentialsStoreMode;
use codex_features::Feature;
use core_test_support::apps_test_server::AppsTestServer;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::json;
use serial_test::serial;
use test_case::test_case;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

enum ConfigRefreshPath {
    Runtime,
    Mcp,
}

#[test_case(ConfigRefreshPath::Runtime; "runtime configuration refresh")]
#[test_case(ConfigRefreshPath::Mcp; "MCP configuration refresh")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial(codex_home)]
async fn oauth_mode_refresh_replaces_the_live_connection(
    refresh_path: ConfigRefreshPath,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let responses_server = responses::start_mock_server().await;
    let mcp_server = responses::start_mock_server().await;
    let (http_server, startup_control) =
        AppsTestServer::mount_with_startup_control(&mcp_server).await?;
    let server_name = "oauth_refresh";
    let server_url = format!("{}/api/codex/ps/mcp", http_server.chatgpt_base_url);
    Mock::given(method("GET"))
        .and(path(
            "/.well-known/oauth-authorization-server/api/codex/ps/mcp",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": server_url,
            "authorization_endpoint": format!("{}/oauth/authorize", mcp_server.uri()),
            "token_endpoint": format!("{}/oauth/token", mcp_server.uri()),
        })))
        .mount(&mcp_server)
        .await;
    let server_config: McpServerConfig = serde_json::from_value(json!({
        "url": server_url,
        "environment_id": DEFAULT_MCP_SERVER_ENVIRONMENT_ID,
    }))?;
    let home = Arc::new(tempfile::tempdir()?);
    let _codex_home_guard = EnvVarGuard::set("CODEX_HOME", home.path().as_os_str());
    write_fallback_oauth_tokens(
        server_config.oauth_credential_name(server_name).as_ref(),
        &server_url,
        "test-client-id",
        "synthetic-access-token",
        "synthetic-refresh-token",
        OAuthCredentialExpiry::Valid,
    )?;
    let fixture = test_codex()
        .with_home(home)
        .with_config(move |config| {
            config
                .features
                .disable(Feature::McpOAuthRefreshCoordination)
                .expect("test config should allow the OAuth coordination feature");
            config.mcp_oauth_credentials_store_mode = OAuthCredentialsStoreMode::File;
            config
                .mcp_servers
                .set([(server_name.to_string(), server_config)].into())
                .expect("test config should allow the OAuth server");
        })
        // The mock MCP endpoint is on the host, including when the executor is remote.
        .build_with_remote_and_local_env(&responses_server)
        .await?;

    let initial_result = fixture
        .codex
        .call_mcp_tool(
            server_name,
            "calendar_list_events",
            /*arguments*/ None,
            /*meta*/ None,
        )
        .await?;
    assert_eq!(startup_control.initialize_attempts(), 1);

    for (enabled, expected_initializations) in [(true, 2), (true, 2), (false, 3)] {
        let mut refreshed_config = fixture.config.clone();
        refreshed_config
            .features
            .set_enabled(Feature::McpOAuthRefreshCoordination, enabled)?;
        match refresh_path {
            ConfigRefreshPath::Runtime => {
                fixture.codex.refresh_runtime_config(refreshed_config).await;
            }
            ConfigRefreshPath::Mcp => {
                fixture.codex.refresh_mcp_config(refreshed_config).await;
            }
        }
        // A normal tool call reconciles the refreshed config without forcing a reconnect.
        let result = fixture
            .codex
            .call_mcp_tool(
                server_name,
                "calendar_list_events",
                /*arguments*/ None,
                /*meta*/ None,
            )
            .await?;
        assert_eq!(result, initial_result);
        assert_eq!(
            startup_control.initialize_attempts(),
            expected_initializations
        );
    }

    fixture.codex.shutdown_and_wait().await?;
    Ok(())
}
