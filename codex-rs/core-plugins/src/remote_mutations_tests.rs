//! Regression coverage for the shared mutation gate, refresh, and backend failure handling.

use super::*;
use crate::remote::REMOTE_GLOBAL_MARKETPLACE_NAME;
use crate::test_support::load_plugins_config;
use crate::test_support::test_auth_manager;
use crate::test_support::test_plugins_manager_with_auth_manager;
use crate::test_support::write_file;
use codex_protocol::auth::AuthMode;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tempfile::TempDir;
use tokio::sync::Notify;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test]
async fn uninstall_serializes_backend_mutations_and_preserves_cache_on_failure() {
    for uninstall_fails in [false, true] {
        let home = TempDir::new().unwrap();
        let server = MockServer::start().await;
        let mut config = load_plugins_config(home.path(), home.path()).await;
        config.chatgpt_base_url = format!("{}/backend-api", server.uri());
        let auth_manager = test_auth_manager(Some(AuthMode::Chatgpt));
        let auth = auth_manager.auth().await;
        let manager = Arc::new(test_plugins_manager_with_auth_manager(
            home.path().to_path_buf(),
            /*restriction_product*/ None,
            auth_manager,
        ));
        let remote_id = "b1234567-89ab-4cde-8f01-234567890abc";
        let plugin_id = PluginId::new(
            "sample".to_string(),
            REMOTE_GLOBAL_MARKETPLACE_NAME.to_string(),
        )
        .unwrap();
        let cache = manager.store.plugin_base_root(&plugin_id);
        write_file(
            cache.join("1.0.0/.codex-plugin/plugin.json").as_path(),
            r#"{"name":"sample","version":"1.0.0"}"#,
        );
        Mock::given(method("GET"))
            .and(path(format!("/backend-api/ps/plugins/{remote_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": remote_id, "name": "sample", "scope": "GLOBAL",
                "installation_policy": "AVAILABLE", "authentication_policy": "ON_USE",
                "release": {"display_name": "Sample", "description": "Sample plugin", "interface": {}}
            }))).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/backend-api/ps/plugins/installed"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"plugins": [], "pagination": {"next_page_token": null}})),
            )
            .expect(if uninstall_fails { 0 } else { 1 })
            .mount(&server)
            .await;
        let gate_held = Arc::new(AtomicBool::new(false));
        let gate_held_at_post = Arc::clone(&gate_held);
        let manager_at_post = Arc::clone(&manager);
        Mock::given(method("POST"))
            .and(path(format!(
                "/backend-api/ps/plugins/{remote_id}/uninstall"
            )))
            .respond_with(move |_: &wiremock::Request| {
                gate_held_at_post.store(
                    manager_at_post
                        .remote_installed_plugin_bundle_sync_gate
                        .available_permits()
                        == 0,
                    Ordering::SeqCst,
                );
                ResponseTemplate::new(if uninstall_fails { 400 } else { 200 })
                    .set_body_json(json!({"id": remote_id, "enabled": false}))
            })
            .expect(1)
            .mount(&server)
            .await;
        let refreshed = Arc::new(Notify::new());
        let refreshed_callback = Arc::clone(&refreshed);
        let outcome = manager
            .uninstall_remote_plugin(
                &config,
                auth.as_ref(),
                remote_id,
                Some(Arc::new(move |_| refreshed_callback.notify_one())),
            )
            .await;
        assert_eq!(
            (
                gate_held.load(Ordering::SeqCst),
                manager
                    .remote_installed_plugin_bundle_sync_gate
                    .available_permits(),
                cache.as_path().exists(),
                outcome.is_err()
            ),
            (true, 1, uninstall_fails, uninstall_fails),
        );
        if !uninstall_fails {
            tokio::time::timeout(std::time::Duration::from_secs(5), refreshed.notified())
                .await
                .expect("successful uninstall should refresh installed state");
        }
    }
}
