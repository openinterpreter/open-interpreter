use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use rmcp::transport::AuthorizationManager;
use rmcp::transport::AuthorizationRequest;
use rmcp::transport::AuthorizationSession;
use rmcp::transport::auth::OAuthHttpClient;
use rmcp::transport::auth::OAuthState;
use url::Url;

use crate::oauth::validate_authorization_server_endpoints;
use crate::oauth_callback::McpOAuthCallbackMode;
use crate::oauth_callback::append_callback_id_to_redirect_uri;
use crate::oauth_callback::callback_mode;
use crate::oauth_callback::validate_callback_redirect;

/// OAuth client-registration strategy for one interactive HTTP MCP login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpOAuthClientRegistration {
    /// Prefer a supported native CIMD and otherwise use advertised DCR.
    #[default]
    Auto,
    /// Require a ChatGPT-hosted Codex public native Client ID Metadata Document.
    Cimd,
    /// Require the authorization server's Dynamic Client Registration endpoint.
    Dcr,
}

/// OAuth state prepared from one authorization-server metadata resolution.
pub(crate) struct PreparedOAuthLogin {
    pub(crate) oauth_state: OAuthState,
    pub(crate) authorization_server_issuer: Option<String>,
    pub(crate) redirect_uri: String,
}

pub(crate) async fn start_authorization(
    server_url: &str,
    http_client: Arc<dyn OAuthHttpClient>,
    scopes: &[&str],
    redirect_uri: &str,
    callback_id: &str,
    client_registration: McpOAuthClientRegistration,
) -> Result<PreparedOAuthLogin> {
    let mut auth_manager =
        AuthorizationManager::new_with_oauth_http_client(server_url, http_client).await?;
    auth_manager.set_allow_missing_issuer(true);
    let metadata = auth_manager.resolve_metadata().await?.metadata;
    validate_authorization_server_endpoints(&metadata)?;
    let authorization_server_issuer = metadata.issuer.clone();
    let callback_mode = callback_mode(&metadata)?;

    let cimd_advertised = metadata
        .additional_fields
        .get("client_id_metadata_document_supported")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let public_client_auth_supported = metadata
        .additional_fields
        .get("token_endpoint_auth_methods_supported")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|methods| methods.iter().any(|method| method.as_str() == Some("none")));

    let uses_shared_callback = callback_mode == McpOAuthCallbackMode::IssuerBound;
    let redirect_uri = if uses_shared_callback {
        redirect_uri.to_string()
    } else {
        append_callback_id_to_redirect_uri(redirect_uri, callback_id)?
    };
    let parsed_redirect_uri = Url::parse(&redirect_uri)?;
    let expected_callback_path = if uses_shared_callback {
        "/callback".to_string()
    } else {
        format!("/callback/{callback_id}")
    };
    let native_redirect_supported = parsed_redirect_uri.scheme() == "http"
        && matches!(
            parsed_redirect_uri.host_str(),
            Some("127.0.0.1" | "localhost")
        )
        && parsed_redirect_uri.port().is_some_and(|port| port > 0)
        && parsed_redirect_uri.path() == expected_callback_path
        && parsed_redirect_uri.query().is_none()
        && parsed_redirect_uri.fragment().is_none()
        && parsed_redirect_uri.username().is_empty()
        && parsed_redirect_uri.password().is_none();
    validate_callback_redirect(&redirect_uri, callback_id, callback_mode)?;
    // MCP 2026-07-28 priority: pre-registered clients never reach this path; offer
    // advertised CIMD here and otherwise let rmcp fall back to DCR.
    // https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
    let offer_cimd = match client_registration {
        McpOAuthClientRegistration::Auto => {
            cimd_advertised && native_redirect_supported && public_client_auth_supported
        }
        McpOAuthClientRegistration::Cimd => {
            if !cimd_advertised || !public_client_auth_supported {
                bail!(
                    "MCP authorization server does not advertise CIMD with token endpoint auth method `none`"
                );
            }
            if !native_redirect_supported {
                bail!(
                    "MCP OAuth CIMD requires an ephemeral loopback callback at `{expected_callback_path}`"
                );
            }
            true
        }
        McpOAuthClientRegistration::Dcr => false,
    };

    auth_manager.set_metadata(metadata);
    let mut request = AuthorizationRequest::new(redirect_uri.clone())
        .with_scopes(scopes.iter().copied())
        .with_client_name("Codex");
    if offer_cimd {
        // CIMD is an active IETF Internet-Draft: this HTTPS client identifier resolves
        // to its self-referential JSON metadata document.
        // https://datatracker.ietf.org/doc/draft-ietf-oauth-client-id-metadata-document/
        let client_metadata_url = if uses_shared_callback {
            "https://chatgpt.com/oauth/codex/client.json".to_string()
        } else {
            format!("https://chatgpt.com/oauth/codex/{callback_id}/client.json")
        };
        request = request.with_client_metadata_url(client_metadata_url);
    }
    let session = AuthorizationSession::new(auth_manager, request)
        .await
        .map_err(|(_auth_manager, error)| error)?;

    Ok(PreparedOAuthLogin {
        oauth_state: OAuthState::Session(session),
        authorization_server_issuer,
        redirect_uri,
    })
}

#[cfg(test)]
#[path = "oauth_client_registration_tests.rs"]
mod tests;
