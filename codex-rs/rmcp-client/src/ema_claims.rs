//! Validate token routing and signed authorization before forwarding an ID-JAG.

use std::collections::HashSet;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::ema_exchange::EmaAccessToken;

pub(crate) const ID_JAG_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id-jag";

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    typ: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OAuthResource {
    Single(String),
    Multiple(Vec<String>),
}

impl OAuthResource {
    fn is_exact(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.as_slice() == [expected],
        }
    }
}

fn signed_jwt<T: DeserializeOwned>(token: &str) -> Result<(JwtHeader, T)> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        bail!("identity assertion is not a compact signed JWT");
    };
    if header.is_empty() || payload.is_empty() || signature.is_empty() {
        bail!("identity assertion contains an empty JWT segment");
    }
    let header: JwtHeader = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header)?)
        .map_err(|_| anyhow!("invalid identity assertion JWT header"))?;
    if header.alg.trim().is_empty() || header.alg.eq_ignore_ascii_case("none") {
        bail!("identity assertion is unsigned");
    }
    let claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)
        .map_err(|_| anyhow!("invalid identity assertion JWT claims"))?;
    Ok((header, claims))
}

#[derive(Deserialize)]
pub(crate) struct OidcClaims {
    iss: String,
    sub: String,
    aud: OAuthResource,
    azp: Option<String>,
    exp: u64,
}

pub(crate) fn oidc_identity(
    assertion: &str,
    expected_issuer: &str,
    expected_audience: &str,
) -> Result<OidcClaims> {
    let (_, claims): (_, OidcClaims) = signed_jwt(assertion)?;
    if claims.iss != expected_issuer || claims.sub.trim().is_empty() {
        bail!("OIDC identity assertion issuer or subject does not match the enterprise IdP");
    }
    let (audience_matches, multiple_audiences) = match &claims.aud {
        OAuthResource::Single(value) => (value == expected_audience, false),
        OAuthResource::Multiple(values) => (
            values.iter().any(|value| value == expected_audience),
            values.len() > 1,
        ),
    };
    if !audience_matches
        || claims
            .azp
            .as_deref()
            .is_some_and(|party| party != expected_audience)
        || multiple_audiences && claims.azp.as_deref() != Some(expected_audience)
    {
        bail!("OIDC identity assertion audience or authorized party does not match the IdP client");
    }
    Ok(claims)
}

pub fn validate_oidc_identity_assertion(
    assertion: &str,
    expected_issuer: &str,
    expected_audience: &str,
) -> Result<()> {
    let claims = oidc_identity(assertion, expected_issuer, expected_audience)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if claims.exp <= now {
        bail!("OIDC identity assertion is expired");
    }
    Ok(())
}

#[derive(Deserialize)]
struct IdJagClaims {
    iss: String,
    sub: String,
    aud: OAuthResource,
    client_id: String,
    jti: String,
    exp: u64,
    iat: u64,
    resource: OAuthResource,
    scope: Option<String>,
}

pub(crate) struct IdJagBinding<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    pub client_id: &'a str,
    pub resource: &'a str,
    /// Empty means the scope parameter was omitted, not an empty authorization ceiling.
    pub requested_scopes: &'a HashSet<&'a str>,
}

#[derive(Deserialize)]
pub(crate) struct IdJagResponse {
    pub access_token: String,
    issued_token_type: String,
    token_type: String,
    resource: Option<OAuthResource>,
    scope: Option<String>,
    refresh_token: Option<String>,
}

impl IdJagResponse {
    pub(crate) fn validate(&self, binding: IdJagBinding<'_>) -> Result<HashSet<String>> {
        if self.issued_token_type != ID_JAG_TOKEN_TYPE
            || self.token_type != "N_A"
            || self.refresh_token.is_some()
        {
            bail!("enterprise IdP returned an unsupported ID-JAG token type or refresh token");
        }
        let (header, claims): (_, IdJagClaims) = signed_jwt(&self.access_token)?;
        if header.typ.as_deref() != Some("oauth-id-jag+jwt")
            || claims.iss != binding.issuer
            || !claims.aud.is_exact(binding.audience)
            || claims.client_id != binding.client_id
            || claims.sub.trim().is_empty()
            || claims.jti.trim().is_empty()
        {
            bail!("ID-JAG type, issuer, audience, client, subject, or JWT ID is invalid");
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if claims.exp <= now || claims.iat > now.saturating_add(60) {
            bail!("enterprise IdP returned an expired or future-issued ID-JAG");
        }
        if !claims.resource.is_exact(binding.resource)
            || self
                .resource
                .as_ref()
                .is_some_and(|value| !value.is_exact(binding.resource))
        {
            bail!("ID-JAG must authorize exactly the configured MCP resource");
        }
        let granted = match claims.scope.as_deref() {
            Some(scope) => parse_scope(scope)?,
            None if binding.requested_scopes.is_empty() => HashSet::new(),
            None => bail!("ID-JAG is missing the requested scope authorization"),
        };
        if !binding.requested_scopes.is_empty() && !granted.is_subset(binding.requested_scopes) {
            bail!("ID-JAG contains a scope outside the enterprise authorization request");
        }
        match self.scope.as_deref() {
            Some(scope) if parse_scope(scope)? != granted => {
                bail!("enterprise IdP token response scope does not match the signed ID-JAG")
            }
            None if !binding.requested_scopes.is_empty()
                && granted != *binding.requested_scopes =>
            {
                bail!("enterprise IdP token response omitted its narrowed scope")
            }
            _ => {}
        }
        Ok(granted.into_iter().map(str::to_string).collect())
    }
}

fn parse_scope(scope: &str) -> Result<HashSet<&str>> {
    let scopes = scope.split_ascii_whitespace().collect::<HashSet<_>>();
    if scopes.is_empty() || scopes.len() != scope.split_ascii_whitespace().count() {
        bail!("enterprise authorization contains malformed or duplicate scopes");
    }
    Ok(scopes)
}

#[derive(Deserialize)]
pub(crate) struct McpAccessTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    resource: Option<OAuthResource>,
    scope: Option<String>,
    refresh_token: Option<String>,
}

impl McpAccessTokenResponse {
    pub(crate) fn validate(
        self,
        resource: &str,
        id_jag_scopes: &HashSet<String>,
    ) -> Result<EmaAccessToken> {
        if !self.token_type.eq_ignore_ascii_case("bearer") || self.access_token.trim().is_empty() {
            bail!("MCP authorization server returned an invalid bearer token");
        }
        if self.refresh_token.is_some() || self.expires_in == Some(0) {
            bail!("MCP authorization server returned a refresh token or zero token lifetime");
        }
        // The stable EMA response does not require the resource to be echoed;
        // when present, it must agree with the resource bound in the ID-JAG.
        if self
            .resource
            .as_ref()
            .is_some_and(|returned| !returned.is_exact(resource))
        {
            bail!("MCP access token must authorize exactly the configured MCP resource");
        }
        // RFC 6749 defines an omitted scope as unchanged from the request. Here
        // that authority is the scope carried by the validated ID-JAG.
        if let Some(scope) = self.scope.as_deref()
            && !parse_scope(scope)?
                .iter()
                .all(|scope| id_jag_scopes.contains(*scope))
        {
            bail!("MCP authorization server granted a scope outside the ID-JAG authorization");
        }
        Ok(EmaAccessToken {
            access_token: self.access_token,
            expires_in: self.expires_in.map(Duration::from_secs),
        })
    }
}
