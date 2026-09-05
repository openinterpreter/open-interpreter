//! OAuth callback identity and authorization-server mix-up protection.
//!
//! Codex can authorize against many independent MCP servers. If those servers
//! share a callback URL and a response does not identify its authorization
//! server, Codex could associate an authorization code with the wrong server
//! and send that code to an attacker-controlled token endpoint. RFC 9700 calls
//! this an authorization-server mix-up attack:
//! https://www.rfc-editor.org/rfc/rfc9700#section-4.4
//!
//! MCP prefers issuer identification: authorization servers SHOULD include
//! `iss` in authorization responses; servers that include `iss` MUST advertise
//! that support, and clients MUST validate any returned issuer before
//! exchanging the code. This also lets a CIMD document advertise one stable
//! redirect instead of separate redirects for every authorization server:
//! https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization#authorization-response-validation
//!
//! This module prefers issuer binding when available and otherwise retains a
//! callback-specific compatibility fallback:
//!
//! - `IssuerBound`: reuse a stable callback only when authorization metadata
//!   advertises `authorization_response_iss_parameter_supported` and contains
//!   its issuer. RMCP validates the response's `iss` against that metadata
//!   issuer before exchanging the authorization code, rejecting missing or
//!   mismatched issuers:
//!   https://www.rfc-editor.org/rfc/rfc9700#section-4.4.2.1
//!   https://www.rfc-editor.org/rfc/rfc9207#section-2.4
//! - `CallbackSpecific`: append an ID derived from the complete MCP server URL.
//!   Distinct callback paths bind each response to its intended server. This is
//!   the required fallback when issuer identification is not supported:
//!   https://www.rfc-editor.org/rfc/rfc9700#section-4.4.2.2

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rmcp::transport::auth::AuthorizationMetadata;
use sha2::Digest;
use sha2::Sha256;
use url::Url;

/// The OAuth mix-up defense associated with a newly registered callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpOAuthCallbackMode {
    /// A server-specific redirect path identifies the authorization server.
    CallbackSpecific,
    /// A validated authorization-response issuer permits a shared redirect.
    IssuerBound,
}

pub(crate) fn callback_mode(metadata: &AuthorizationMetadata) -> Result<McpOAuthCallbackMode> {
    let issuer_response_supported = metadata
        .additional_fields
        .get("authorization_response_iss_parameter_supported")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if !issuer_response_supported {
        return Ok(McpOAuthCallbackMode::CallbackSpecific);
    }
    if metadata.issuer.is_none() {
        bail!("OAuth authorization server advertises issuer support without a metadata issuer");
    }

    Ok(McpOAuthCallbackMode::IssuerBound)
}

/// Resolves the registered callback independently of runtime listener ports.
pub fn resolve_mcp_oauth_callback_url(
    server_url: &str,
    callback_url: Option<&str>,
    callback_mode: McpOAuthCallbackMode,
) -> Result<String> {
    let callback_url = callback_url.unwrap_or("http://127.0.0.1/callback");

    match callback_mode {
        McpOAuthCallbackMode::IssuerBound => {
            Url::parse(callback_url)
                .with_context(|| format!("invalid redirect URI `{callback_url}`"))?;
            Ok(callback_url.to_string())
        }
        McpOAuthCallbackMode::CallbackSpecific => {
            let callback_id = callback_id_from_server_url(server_url)?;
            append_callback_id_to_redirect_uri(callback_url, &callback_id)
        }
    }
}

pub(crate) fn callback_id_from_server_url(server_url: &str) -> Result<String> {
    // Native Codex callback IDs intentionally hash the complete MCP URL (minus its fragment)
    // with SHA-256. Python connector callback IDs use SHAKE-256 over the origin and are distinct.
    let mut parsed =
        Url::parse(server_url).with_context(|| format!("invalid MCP server URL `{server_url}`"))?;
    parsed
        .host_str()
        .ok_or_else(|| anyhow!("MCP server URL `{server_url}` must include a host"))?;
    parsed.set_fragment(None);

    let digest = Sha256::digest(parsed.as_str().as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(&digest[..9]))
}

pub(crate) fn append_callback_id_to_redirect_uri(
    redirect_uri: &str,
    callback_id: &str,
) -> Result<String> {
    let mut parsed = Url::parse(redirect_uri)
        .with_context(|| format!("invalid redirect URI `{redirect_uri}`"))?;
    if parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        == Some(callback_id)
    {
        return Ok(parsed.to_string());
    }
    let path = parsed.path();
    let new_path = if path.ends_with('/') {
        format!("{path}{callback_id}")
    } else {
        format!("{path}/{callback_id}")
    };
    parsed.set_path(&new_path);
    Ok(parsed.to_string())
}

pub(crate) fn validate_callback_redirect(
    redirect_uri: &str,
    callback_id: &str,
    callback_mode: McpOAuthCallbackMode,
) -> Result<()> {
    let has_expected_callback_id = Url::parse(redirect_uri)?
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        == Some(callback_id);

    if !has_expected_callback_id && callback_mode != McpOAuthCallbackMode::IssuerBound {
        bail!(
            "OAuth callback requires its expected callback ID or authorization response issuer support"
        );
    }

    Ok(())
}

#[cfg(test)]
#[path = "oauth_callback_tests.rs"]
mod tests;
