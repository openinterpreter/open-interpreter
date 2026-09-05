//! Non-interactive ID-JAG exchange against explicitly trusted OAuth endpoints.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_exec_server::HttpClient;
use rmcp::transport::auth::OAuthHttpRedirectPolicy;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::ema_auth_policy::EmaAuthFailure;
use crate::ema_auth_policy::EmaInvalidGrantSource;
use crate::ema_auth_policy::safe_oauth_error_code;
use crate::ema_auth_policy::validate_ema_oauth_endpoint;
use crate::ema_claims::ID_JAG_TOKEN_TYPE;
use crate::ema_claims::IdJagBinding;
use crate::ema_claims::IdJagResponse;
use crate::ema_claims::McpAccessTokenResponse;
use crate::http_client_adapter::StreamableHttpRedirectMode;
use crate::oauth_http_client::OAuthHttpClientAdapter;
use crate::utils::build_default_headers;

pub(crate) const TOKEN_EXCHANGE_GRANT_TYPE: &str =
    "urn:ietf:params:oauth:grant-type:token-exchange";
pub(crate) const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

/// A resource-bound bearer and its server-reported lifetime, with redacted diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub struct EmaAccessToken {
    pub access_token: String,
    pub expires_in: Option<Duration>,
}

impl std::fmt::Debug for EmaAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmaAccessToken")
            .field("access_token", &"[REDACTED]")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// The caller supplies trusted authorization-server metadata and an IdP credential.
/// This primitive does not perform resource discovery or interactive login.
pub struct EmaIdJagExchangeRequest<'a> {
    pub resource: &'a str,
    pub scopes: &'a [String],
    pub mcp_client_id: &'a str,
    pub authorization_server_issuer: &'a str,
    pub authorization_server_token_endpoint: &'a str,
    pub idp_token_endpoint: &'a str,
    pub idp_issuer: &'a str,
    pub idp_client_id: &'a str,
    pub refresh_token: String,
    pub idp_http_client: Arc<dyn HttpClient>,
    pub resource_http_client: Arc<dyn HttpClient>,
}

/// Exchanges an enterprise IdP credential for a resource-bound MCP bearer token.
pub async fn exchange_id_jag(request: EmaIdJagExchangeRequest<'_>) -> Result<EmaAccessToken> {
    for (endpoint, description) in [
        (request.resource, "enterprise MCP resource"),
        (
            request.authorization_server_issuer,
            "MCP authorization server issuer",
        ),
        (
            request.authorization_server_token_endpoint,
            "MCP token endpoint",
        ),
        (request.idp_issuer, "enterprise IdP issuer"),
        (request.idp_token_endpoint, "enterprise IdP token endpoint"),
    ] {
        validate_ema_oauth_endpoint(endpoint, description)?;
    }
    if request.authorization_server_issuer == request.idp_issuer {
        bail!("enterprise IdP and MCP authorization server issuers must be different for ID-JAG");
    }
    if request.mcp_client_id.trim().is_empty() || request.idp_client_id.trim().is_empty() {
        bail!("enterprise authorization requires the registered IdP and MCP client IDs");
    }
    if request.refresh_token.trim().is_empty() {
        bail!("enterprise IdP refresh token must not be empty");
    }
    let requested_scopes = request
        .scopes
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if requested_scopes.len() != request.scopes.len()
        || request
            .scopes
            .iter()
            .any(|scope| scope.is_empty() || scope.chars().any(char::is_whitespace))
    {
        bail!("enterprise MCP authorization scopes must be distinct, non-empty scope tokens");
    }
    let scope = (!request.scopes.is_empty()).then(|| request.scopes.join(" "));
    let mut params = vec![
        ("grant_type", TOKEN_EXCHANGE_GRANT_TYPE),
        ("requested_token_type", ID_JAG_TOKEN_TYPE),
        ("audience", request.authorization_server_issuer),
        ("resource", request.resource),
        ("subject_token", request.refresh_token.as_str()),
        (
            "subject_token_type",
            "urn:ietf:params:oauth:token-type:refresh_token",
        ),
    ];
    if let Some(scope) = scope.as_deref() {
        params.push(("scope", scope));
    }
    let id_jag: IdJagResponse = post_form(
        &request.idp_http_client,
        request.idp_token_endpoint,
        &params,
        request.idp_client_id,
        EmaInvalidGrantSource::EnterpriseIdentity,
        "enterprise IdP ID-JAG exchange",
    )
    .await?;
    let granted_scopes = id_jag.validate(IdJagBinding {
        issuer: request.idp_issuer,
        audience: request.authorization_server_issuer,
        client_id: request.mcp_client_id,
        resource: request.resource,
        requested_scopes: &requested_scopes,
    })?;
    // Only the signed assertion carries authority to the Resource AS. Repeating
    // the requested resource or scopes could undo enterprise policy narrowing.
    let access_token: McpAccessTokenResponse = post_form(
        &request.resource_http_client,
        request.authorization_server_token_endpoint,
        &[
            ("grant_type", JWT_BEARER_GRANT_TYPE),
            ("assertion", id_jag.access_token.as_str()),
        ],
        request.mcp_client_id,
        EmaInvalidGrantSource::ResourceAuthorization,
        "MCP JWT bearer exchange",
    )
    .await?;
    access_token.validate(request.resource, &granted_scopes)
}

#[derive(Deserialize)]
struct OAuthErrorResponse {
    error: Option<String>,
}

pub(crate) async fn post_form<T: DeserializeOwned>(
    http_client: &Arc<dyn HttpClient>,
    url: &str,
    params: &[(&str, &str)],
    client_id: &str,
    invalid_grant_source: EmaInvalidGrantSource,
    operation: &str,
) -> Result<T> {
    let client = OAuthHttpClientAdapter::new_with_redirect_mode(
        Arc::clone(http_client),
        build_default_headers(/*http_headers*/ None, /*env_http_headers*/ None)?,
        url,
        /*has_configured_headers*/ false,
        StreamableHttpRedirectMode::Legacy,
    )?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.extend_pairs(params.iter().copied());
        form.append_pair("client_id", client_id);
        form.finish().into_bytes()
    };
    let builder = oauth2::http::Request::builder()
        .method("POST")
        .uri(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json");
    let response = client
        .execute_request(
            builder.body(body)?,
            OAuthHttpRedirectPolicy::Stop,
            Some(Duration::from_secs(30)),
        )
        .await
        .map_err(|error| anyhow!("{operation} request failed: {error}"))?;
    if !response.status().is_success() {
        let error = serde_json::from_slice::<OAuthErrorResponse>(response.body()).ok();
        // Provider-controlled text may reflect the submitted assertion or secret.
        // Only known OAuth codes may reach callers.
        let code = safe_oauth_error_code(error.as_ref().and_then(|error| error.error.as_deref()));
        if code == "invalid_grant" {
            return Err(anyhow::Error::new(EmaAuthFailure::InvalidGrant {
                grant_source: invalid_grant_source,
            })
            .context(format!(
                "{operation} returned HTTP {}: invalid_grant",
                response.status()
            )));
        }
        if code == "insufficient_user_authentication" {
            return Err(
                anyhow::Error::new(EmaAuthFailure::InsufficientUserAuthentication).context(
                    format!("{operation} returned HTTP {}: {code}", response.status()),
                ),
            );
        }
        bail!("{operation} returned HTTP {}: {code}", response.status());
    }
    serde_json::from_slice(response.body())
        .map_err(|_| anyhow!("failed to parse {operation} response"))
}

#[cfg(test)]
#[path = "ema_exchange_tests.rs"]
mod tests;
