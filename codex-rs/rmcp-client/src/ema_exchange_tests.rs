use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use codex_exec_server::RouteAwareHttpClient;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use futures::FutureExt;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::*;
use crate::ema_claims::validate_oidc_identity_assertion;

fn http_client() -> Arc<dyn HttpClient> {
    Arc::new(RouteAwareHttpClient::new(HttpClientFactory::new(
        OutboundProxyPolicy::ReqwestDefault,
    )))
}

fn unique_form_fields(body: &[u8]) -> HashMap<String, String> {
    let pairs = url::form_urlencoded::parse(body)
        .into_owned()
        .collect::<Vec<_>>();
    let fields = pairs.iter().cloned().collect::<HashMap<_, _>>();
    assert_eq!(
        pairs.len(),
        fields.len(),
        "OAuth form must not contain duplicate fields"
    );
    fields
}

fn jwt(claims: &Value) -> String {
    format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","typ":"oauth-id-jag+jwt"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("serialize claims"))
    )
}

fn claims(issuer: &str, audience: &str, resource: &str) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time")
        .as_secs();
    json!({"iss":issuer,"aud":audience,"sub":"user","client_id":"mcp-client",
        "jti":"unique-jag","iat":now,"exp":now + 3600,"resource":resource,"scope":"files.read"})
}

fn jag_response(claims: &Value) -> Value {
    let mut response = json!({"access_token":jwt(claims),"issued_token_type":ID_JAG_TOKEN_TYPE,
        "token_type":"N_A","resource":claims["resource"]});
    if let Some(scope) = claims.get("scope") {
        response["scope"] = scope.clone();
    }
    response
}

fn token_response() -> Value {
    json!({"access_token":"resource-token","token_type":"Bearer","expires_in":300})
}

#[tokio::test]
async fn public_client_round_trip_preserves_signed_narrowing() -> Result<()> {
    let requested_scopes = ["files.read".to_string(), "files.write".to_string()];
    let client = http_client();
    for (scopes, echo_scope, refresh_token) in [
        (requested_scopes.as_slice(), true, "opaque-refresh-token"),
        (&[], true, "opaque-refresh-token"),
        (&[], false, "opaque-refresh-token"),
        (requested_scopes.as_slice(), true, ""),
        (requested_scopes.as_slice(), true, " \t"),
    ] {
        let server = MockServer::start().await;
        let issuer = format!("{}/idp", server.uri());
        let audience = format!("{}/as", server.uri());
        let resource = format!("{}/mcp", server.uri());
        let mut jag = jag_response(&claims(&issuer, &audience, &resource));
        if !echo_scope {
            jag.as_object_mut()
                .expect("ID-JAG response")
                .remove("scope");
        }
        let valid = !refresh_token.trim().is_empty();
        for (endpoint, response) in [("/idp/token", jag.clone()), ("/as/token", token_response())] {
            Mock::given(method("POST"))
                .and(path(endpoint))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(u64::from(valid))
                .mount(&server)
                .await;
        }
        let expected_subject = refresh_token.to_string();
        let result = exchange_id_jag(EmaIdJagExchangeRequest {
            resource: &resource,
            scopes,
            mcp_client_id: "mcp-client",
            authorization_server_issuer: &audience,
            authorization_server_token_endpoint: &format!("{audience}/token"),
            idp_token_endpoint: &format!("{issuer}/token"),
            idp_issuer: &issuer,
            idp_client_id: "idp-client",
            refresh_token: refresh_token.to_string(),
            idp_http_client: Arc::clone(&client),
            resource_http_client: Arc::clone(&client),
        })
        .boxed()
        .await;
        if !valid {
            assert!(result.is_err(), "invalid subject must fail before HTTP");
            assert!(
                server
                    .received_requests()
                    .await
                    .expect("requests")
                    .is_empty()
            );
            continue;
        }
        assert_eq!(
            result?,
            EmaAccessToken {
                access_token: "resource-token".to_string(),
                expires_in: Some(Duration::from_secs(300)),
            }
        );
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|request| request.headers.get("authorization").is_none())
        );
        let mut forms = requests
            .iter()
            .map(|request| unique_form_fields(&request.body))
            .collect::<Vec<_>>();
        assert_eq!(
            forms[0].remove("scope"),
            (!scopes.is_empty()).then(|| scopes.join(" "))
        );
        assert_eq!(
            forms[0],
            HashMap::from([
                (
                    "grant_type".to_string(),
                    TOKEN_EXCHANGE_GRANT_TYPE.to_string()
                ),
                (
                    "requested_token_type".to_string(),
                    ID_JAG_TOKEN_TYPE.to_string(),
                ),
                ("subject_token".to_string(), expected_subject),
                (
                    "subject_token_type".to_string(),
                    "urn:ietf:params:oauth:token-type:refresh_token".to_string(),
                ),
                ("audience".to_string(), audience.clone()),
                ("resource".to_string(), resource.clone()),
                ("client_id".to_string(), "idp-client".to_string()),
            ])
        );
        assert_eq!(
            forms[1],
            HashMap::from([
                ("grant_type".to_string(), JWT_BEARER_GRANT_TYPE.to_string()),
                (
                    "assertion".to_string(),
                    jag["access_token"].as_str().expect("JAG").to_string()
                ),
                ("client_id".to_string(), "mcp-client".to_string()),
            ])
        );
    }
    Ok(())
}

#[test]
fn signed_claims_and_resource_tokens_cannot_widen_authority() -> Result<()> {
    let requested = HashSet::from(["files.read", "files.write"]);
    let original = claims(
        "https://idp.example",
        "https://as.example",
        "https://mcp.example",
    );
    let binding = || IdJagBinding {
        issuer: "https://idp.example",
        audience: "https://as.example",
        client_id: "mcp-client",
        resource: "https://mcp.example",
        requested_scopes: &requested,
    };
    let valid: IdJagResponse = serde_json::from_value(jag_response(&original))?;
    let granted = valid.validate(binding())?;
    assert_eq!(granted, HashSet::from(["files.read".to_string()]));
    for (requested_scopes, signed_scope, response_scope, valid) in [
        ("", Some("files.read"), Some("files.read"), true),
        ("", Some("files.read"), None, true),
        ("", None, None, true),
        ("files.read", Some("files.read"), None, true),
        ("files.read files.write", Some("files.read"), None, false),
        (
            "files.read",
            Some("files.read files.write"),
            Some("files.read files.write"),
            false,
        ),
        ("files.read", None, None, false),
        ("", Some("files.read"), Some("files.write"), false),
        ("", Some(" \t"), None, false),
        ("", Some("files.read files.read"), Some("files.read"), false),
        ("", Some("files.read"), Some(" \t"), false),
        ("", Some("files.read"), Some("files.read files.read"), false),
    ] {
        let requested_scopes = requested_scopes.split_ascii_whitespace().collect();
        let mut scoped_claims = original.clone();
        scoped_claims
            .as_object_mut()
            .expect("ID-JAG claims")
            .remove("scope");
        if let Some(scope) = signed_scope {
            scoped_claims["scope"] = json!(scope);
        }
        let mut response = jag_response(&scoped_claims);
        response
            .as_object_mut()
            .expect("ID-JAG response")
            .remove("scope");
        if let Some(scope) = response_scope {
            response["scope"] = json!(scope);
        }
        let response: IdJagResponse = serde_json::from_value(response)?;
        let result = response.validate(IdJagBinding {
            requested_scopes: &requested_scopes,
            ..binding()
        });
        assert_eq!(
            result.is_ok(),
            valid,
            "requested {requested_scopes:?}, signed {signed_scope:?}, response {response_scope:?}"
        );
        if let Ok(granted) = result {
            assert_eq!(
                granted,
                signed_scope
                    .into_iter()
                    .map(str::to_string)
                    .collect::<HashSet<_>>()
            );
            for (scope, valid_token) in [
                (None, true),
                (Some("files.read"), signed_scope.is_some()),
                (Some("files.read files.write"), false),
            ] {
                let mut response = token_response();
                if let Some(scope) = scope {
                    response["scope"] = json!(scope);
                }
                let response: McpAccessTokenResponse = serde_json::from_value(response)?;
                assert_eq!(
                    response.validate("https://mcp.example", &granted).is_ok(),
                    valid_token,
                    "ID-JAG {signed_scope:?}, bearer {scope:?}"
                );
            }
        }
    }
    let mut rotated = jag_response(&original);
    rotated["refresh_token"] = json!("unsupported-jag-refresh-token");
    let response: IdJagResponse = serde_json::from_value(rotated)?;
    assert!(response.validate(binding()).is_err());
    for header in [
        json!({"alg": "ES256", "typ": "JWT"}),
        json!({"alg": "ES256"}),
        json!({"alg": "none", "typ": "oauth-id-jag+jwt"}),
    ] {
        let mut changed = jag_response(&original);
        changed["access_token"] = json!(format!(
            "{}.{}.signature",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&original)?),
        ));
        let response: IdJagResponse = serde_json::from_value(changed)?;
        assert!(
            response.validate(binding()).is_err(),
            "accepted invalid ID-JAG header {header}"
        );
    }
    for (field, value) in [
        ("iss", json!("https://attacker.example")),
        ("aud", json!("https://attacker.example")),
        ("client_id", json!("other-client")),
        ("sub", json!("")),
        ("jti", json!("")),
        ("exp", json!(0)),
        ("iat", json!(u64::MAX)),
        ("scope", json!("files.admin")),
        (
            "resource",
            json!(["https://mcp.example", "https://other.example"]),
        ),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        let response: IdJagResponse = serde_json::from_value(jag_response(&changed))?;
        assert!(
            response.validate(binding()).is_err(),
            "accepted changed {field}"
        );
    }
    for (field, value) in [
        ("scope", json!("files.read files.write")),
        ("scope", json!("files.read files.read")),
        ("resource", json!("https://other.example")),
        ("expires_in", json!(0)),
        ("refresh_token", json!("refresh")),
        ("token_type", json!("N_A")),
        ("access_token", json!("")),
    ] {
        let mut changed = token_response();
        changed[field] = value;
        let response: McpAccessTokenResponse = serde_json::from_value(changed)?;
        assert!(
            response.validate("https://mcp.example", &granted).is_err(),
            "accepted changed {field}"
        );
    }
    let mut explicit_binding = token_response();
    explicit_binding["resource"] = json!("https://mcp.example");
    explicit_binding["scope"] = json!("files.read");
    let response: McpAccessTokenResponse = serde_json::from_value(explicit_binding)?;
    assert_eq!(
        response.validate("https://mcp.example", &granted)?,
        EmaAccessToken {
            access_token: "resource-token".to_string(),
            expires_in: Some(Duration::from_secs(300)),
        }
    );
    Ok(())
}

#[test]
fn identity_and_credential_destinations_are_bound() {
    for endpoint in [
        "http://idp.example/token",
        "https://user:pass@idp.example/token",
        "https://idp.example/token#fragment",
    ] {
        assert!(
            validate_ema_oauth_endpoint(endpoint, "IdP").is_err(),
            "accepted {endpoint}"
        );
    }
    let original = claims("https://idp.example", "idp-client", "https://mcp.example");
    assert!(
        validate_oidc_identity_assertion(&jwt(&original), "https://idp.example", "idp-client")
            .is_ok()
    );
    for (field, value) in [
        ("iss", json!("https://other.example")),
        ("aud", json!(["idp-client", "other"])),
        ("azp", json!("other")),
        ("exp", json!(0)),
        ("sub", json!("")),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        assert!(
            validate_oidc_identity_assertion(&jwt(&changed), "https://idp.example", "idp-client")
                .is_err(),
            "accepted changed {field}"
        );
    }
}

#[tokio::test]
async fn provider_errors_cannot_reflect_credentials() {
    const SENTINEL: &str = "secret-assertion-sentinel";
    for (code, expected) in [
        (SENTINEL, "OAuth token request rejected"),
        ("invalid_grant", "invalid_grant"),
        (
            "insufficient_user_authentication",
            "insufficient_user_authentication",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error":code,"error_description":SENTINEL,
            })))
            .mount(&server)
            .await;
        let error = post_form::<Value>(
            &http_client(),
            &format!("{}/token", server.uri()),
            &[("subject_token", SENTINEL)],
            "test-client",
            EmaInvalidGrantSource::EnterpriseIdentity,
            "test token exchange",
        )
        .await
        .expect_err("provider error should fail");
        match code {
            "invalid_grant" => assert_eq!(
                error.downcast_ref::<EmaAuthFailure>(),
                Some(&EmaAuthFailure::InvalidGrant {
                    grant_source: EmaInvalidGrantSource::EnterpriseIdentity,
                })
            ),
            "insufficient_user_authentication" => assert_eq!(
                error.downcast_ref::<EmaAuthFailure>(),
                Some(&EmaAuthFailure::InsufficientUserAuthentication)
            ),
            _ => assert_eq!(error.downcast_ref::<EmaAuthFailure>(), None),
        }
        let error = error.to_string();
        assert!(!error.contains(SENTINEL), "provider reflected a credential");
        assert!(error.ends_with(expected), "{error}");
    }
    let server = MockServer::start().await;
    let mut malformed = token_response();
    malformed["expires_in"] = json!(SENTINEL);
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(malformed))
        .mount(&server)
        .await;
    let error = post_form::<McpAccessTokenResponse>(
        &http_client(),
        &format!("{}/token", server.uri()),
        &[],
        "test-client",
        EmaInvalidGrantSource::EnterpriseIdentity,
        "test token exchange",
    )
    .await
    .err()
    .expect("malformed response should fail");
    assert!(
        !format!("{error:#}").contains(SENTINEL),
        "parser reflected a credential"
    );
}
