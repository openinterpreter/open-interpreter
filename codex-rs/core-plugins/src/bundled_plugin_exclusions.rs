//! Account/backend-scoped exclusions for bundled plugins replaced by remote plugins.
//! Only plugin IDs are persisted; remote installation and enablement remain server-owned.
// TODO(sites-migration): Remove this module when bundled Sites is retired and rollback support ends.

use super::*;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

const BUNDLED_SITES_PLUGIN_ID: &str = "sites@openai-bundled";
const REMOTE_SITES_PLUGIN_ID: &str = "sites@openai-curated-remote";

fn is_remote_sites(plugin: &RemoteInstalledPlugin) -> bool {
    plugin.name == "sites"
        && plugin.marketplace_name == REMOTE_GLOBAL_MARKETPLACE_NAME
        && plugin.id.strip_prefix("plugins~").unwrap_or(&plugin.id)
            == "plugin_connector_1p_689987207de08191979cf68eca2941c6"
}

#[derive(Default, Deserialize, Serialize)]
struct BundledPluginExclusions {
    #[serde(rename = "disabled-bundled-plugin-ids")]
    ids: BTreeSet<String>,
}

fn read_exclusions(path: &Path) -> BundledPluginExclusions {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

impl PluginsManager {
    // Temporary direct-access guard using the same exclusion state as catalog/runtime loading.
    pub(super) fn bundled_sites_is_hidden(
        &self,
        config: &PluginsConfigInput,
        plugin_id: &str,
    ) -> bool {
        plugin_id == BUNDLED_SITES_PLUGIN_ID
            && self.excluded_bundled_plugin_ids(config).contains(plugin_id)
    }

    fn bundled_plugin_exclusion_path(
        &self,
        base_url: &str,
        auth: Option<&CodexAuth>,
    ) -> Option<PathBuf> {
        if self.restriction_product != Some(Product::Codex) {
            return None;
        }
        let auth = auth.filter(|auth| auth.uses_codex_backend())?;
        let account_id = auth.get_account_id().filter(|id| !id.is_empty())?;
        let key = serde_json::to_vec(&(
            base_url,
            account_id,
            auth.get_chatgpt_user_id(),
            auth.is_workspace_account(),
        ))
        .ok()?;
        let digest = Sha256::digest(key);
        Some(
            self.codex_home
                .join("cache/bundled_plugin_exclusions")
                .join(format!("{digest:x}.json")),
        )
    }

    fn remote_sites_bundle_is_loadable(&self) -> bool {
        PluginId::parse(REMOTE_SITES_PLUGIN_ID)
            .ok()
            .is_some_and(|id| self.store.active_plugin_root(&id).is_some())
    }

    pub(super) fn excluded_bundled_plugin_ids(
        &self,
        config: &PluginsConfigInput,
    ) -> BTreeSet<String> {
        if !config.plugins_enabled || !self.remote_global_catalog_active(config) {
            return BTreeSet::new();
        }
        let auth = self.auth_manager.auth_cached();
        let Some(path) =
            self.bundled_plugin_exclusion_path(&config.chatgpt_base_url, auth.as_ref())
        else {
            return BTreeSet::new();
        };
        let mut exclusions = read_exclusions(&path).ids;
        // Preserve the bundled fallback if the replacement's local files are missing.
        if !self.remote_sites_bundle_is_loadable() {
            exclusions.remove(BUNDLED_SITES_PLUGIN_ID);
        }
        exclusions
    }

    // Called only after a current, successful installed-snapshot fetch. Failed requests never
    // erase the last known exclusion; successful absence restores the bundled fallback.
    pub(super) fn update_sites_exclusion(
        &self,
        base_url: &str,
        auth: Option<&CodexAuth>,
        plugins: &[RemoteInstalledPlugin],
    ) -> bool {
        let Some(path) = self.bundled_plugin_exclusion_path(base_url, auth) else {
            return false;
        };
        let mut exclusions = read_exclusions(&path);
        let remote_sites_ready =
            plugins.iter().any(is_remote_sites) && self.remote_sites_bundle_is_loadable();
        let changed = if remote_sites_ready {
            exclusions.ids.insert(BUNDLED_SITES_PLUGIN_ID.to_string())
        } else {
            exclusions.ids.remove(BUNDLED_SITES_PLUGIN_ID)
        };
        if changed {
            let result = serde_json::to_string(&exclusions)
                .map_err(std::io::Error::other)
                .and_then(|contents| codex_utils_path::write_atomically(&path, &contents));
            if let Err(err) = result {
                warn!(error = %err, "failed to persist bundled Sites exclusion");
            }
        }
        changed
    }

    fn sites_migration_needed(
        &self,
        config: &PluginsConfigInput,
        auth: Option<&CodexAuth>,
    ) -> bool {
        if !config.plugins_enabled
            || !self.remote_global_catalog_active(config)
            || self
                .excluded_bundled_plugin_ids(config)
                .contains(BUNDLED_SITES_PLUGIN_ID)
        {
            return false;
        }
        let Some(path) = self.bundled_plugin_exclusion_path(&config.chatgpt_base_url, auth) else {
            return false;
        };
        !self
            .sites_migration_checked_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|(checked_path, checked_at)| {
                checked_path == &path && checked_at.elapsed() < Duration::from_secs(60)
            })
    }

    /// Coordinates the first Sites migration at the shared inventory boundary. A persisted
    /// exclusion skips migration on restart without restoring any remote installed metadata.
    pub async fn ensure_sites_migration_ready(
        &self,
        config: &PluginsConfigInput,
        auth: Option<&CodexAuth>,
    ) -> Result<Option<EffectivePluginsChange>, RemoteInstalledPluginBundleSyncError> {
        if !self.sites_migration_needed(config, auth) {
            return Ok(None);
        }
        // Sites is implicitly installed by Plugin Service. Never create an explicit install or
        // overwrite account-wide preferences from bundled config; /installed owns both decisions.
        let plugins = crate::remote::fetch_remote_installed_plugins(
            &remote_plugin_service_config(config),
            auth,
        )
        .await?;
        let remote_sites_available = plugins.iter().any(is_remote_sites);
        // A rollout-off check must not wait behind unrelated startup bundle downloads.
        let _guard = if remote_sites_available {
            let guard = self.acquire_remote_installed_plugin_sync_guard().await?;
            // Another catalog request or background sync may have completed while we waited.
            if !self.sites_migration_needed(config, auth) {
                return Ok(None);
            }
            Some(guard)
        } else {
            None
        };
        *self
            .sites_migration_checked_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self
            .bundled_plugin_exclusion_path(&config.chatgpt_base_url, auth)
            .map(|path| (path, Instant::now()));
        if !remote_sites_available {
            return Ok(None);
        }
        let (outcome, changed) =
            Box::pin(self.reconcile_remote_installed_plugins_after_acquiring_gate(config, auth))
                .await?;
        // Preserve materialization details so the existing callback can trust eligible hooks.
        Ok(
            (changed || !outcome.changed_plugins.is_empty()).then_some(EffectivePluginsChange {
                materialized_remote_plugins: outcome.materialized_remote_plugins,
            }),
        )
    }
}
