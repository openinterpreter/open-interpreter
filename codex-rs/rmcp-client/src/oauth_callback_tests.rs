use pretty_assertions::assert_eq;

use super::McpOAuthCallbackMode;
use super::callback_id_from_server_url;
use super::resolve_mcp_oauth_callback_url;
use super::validate_callback_redirect;

#[test]
fn resolved_callbacks_follow_the_selected_mix_up_defense() {
    let server_url = "https://mcp.example.com/mcp?tenant=one";
    let callback_id = callback_id_from_server_url(server_url).expect("resolve callback ID");
    let distinct_callback = format!("http://127.0.0.1/callback/{callback_id}");

    for (callback, mode, expected) in [
        (
            None,
            McpOAuthCallbackMode::CallbackSpecific,
            distinct_callback.as_str(),
        ),
        (
            None,
            McpOAuthCallbackMode::IssuerBound,
            "http://127.0.0.1/callback",
        ),
        (
            Some("http://127.0.0.1:8080/oauth/callback"),
            McpOAuthCallbackMode::IssuerBound,
            "http://127.0.0.1:8080/oauth/callback",
        ),
    ] {
        assert_eq!(
            resolve_mcp_oauth_callback_url(server_url, callback, mode)
                .expect("resolve registered callback"),
            expected
        );
    }
}

#[test]
fn callback_redirect_requires_a_server_specific_id_or_issuer_support() {
    for (redirect_uri, mode, expected_valid) in [
        (
            "http://127.0.0.1/callback/expected-id",
            McpOAuthCallbackMode::CallbackSpecific,
            true,
        ),
        (
            "http://127.0.0.1/callback",
            McpOAuthCallbackMode::IssuerBound,
            true,
        ),
        (
            "http://127.0.0.1/callback/wrong-id",
            McpOAuthCallbackMode::CallbackSpecific,
            false,
        ),
    ] {
        assert_eq!(
            validate_callback_redirect(redirect_uri, "expected-id", mode).is_ok(),
            expected_valid
        );
    }
}
