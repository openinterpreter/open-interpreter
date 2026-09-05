use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

#[test]
fn resource_origin_query_and_path_are_bound() {
    let server = "https://mcp.example/enterprise/tools?tenant=one";
    for (resource, valid) in [
        ("https://mcp.example/enterprise?tenant=one", true),
        ("https://other.example/enterprise?tenant=one", false),
        ("https://mcp.example/enterprise-admin?tenant=one", false),
        ("https://mcp.example/enterprise?tenant=two", false),
        ("https://mcp.example/enterprise", false),
        ("http://localhost:4000/enterprise?tenant=one", false),
    ] {
        assert_eq!(
            validate_ema_auth_resource(server, Some(resource)).is_ok(),
            valid,
            "{resource}"
        );
    }
    for (server, valid) in [
        ("https://mcp.example/mcp", true),
        ("http://localhost:4000/mcp", true),
        ("http://127.0.0.1:4000/mcp", true),
        ("http://mcp.example/mcp", false),
    ] {
        assert_eq!(
            validate_ema_auth_resource(server, /*resource*/ None).is_ok(),
            valid,
            "{server}"
        );
    }
}

#[test]
fn public_clients_require_an_explicitly_advertised_auth_method() {
    for (advertised, valid) in [
        (None, false),
        (Some(json!(["none"])), true),
        (Some(json!(["client_secret_basic", "none"])), true),
        (Some(json!(["private_key_jwt"])), false),
        (Some(json!("none")), false),
    ] {
        assert_eq!(
            validate_ema_public_client_auth(advertised.as_ref(), "IdP").is_ok(),
            valid
        );
    }
}
