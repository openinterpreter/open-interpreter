//! Apply an accepted account usage response to the currently selected task and model.

use super::App;
use crate::app_server_session::AppServerSession;
use crate::service_tier_resolution;
use codex_app_server_protocol::ThreadSettingsUpdateParams;

impl App {
    pub(super) async fn apply_backend_banner_fallback(
        &mut self,
        app_server: &mut AppServerSession,
    ) {
        let Some(thread_id) = self.active_thread_id else {
            return;
        };
        if Some(thread_id) != self.chat_widget.thread_id() {
            return;
        }
        let Some(target) = self.chat_widget.backend_banner_fallback() else {
            return;
        };
        let effort = self
            .chat_widget
            .current_reasoning_effort()
            .filter(|effort| {
                target
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.effort == *effort)
            })
            .unwrap_or(target.default_reasoning_effort);
        let mut mode = self.chat_widget.effective_collaboration_mode();
        mode.settings.model = target.model.clone();
        mode.settings.reasoning_effort = Some(effort.clone());
        // Automatic recovery must not apply the permission defaults used by manual model selection.
        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            model: Some(target.model.clone()),
            effort: Some(effort.clone()),
            collaboration_mode: Some(mode.clone()),
            service_tier: service_tier_resolution::service_tier_update_for_core(
                self.chat_widget.config_ref(),
                &target.model,
                &self.model_catalog.try_list_models().unwrap_or_default(),
            ),
            ..ThreadSettingsUpdateParams::default()
        };
        // Older remote servers can decline this method. Keep the existing recovery UI in that
        // case or on failure, rather than changing local selection before the task accepts it.
        // Event dispatch awaits this operation; a queued manual model selection runs afterward.
        if self.send_thread_settings_update(app_server, params).await {
            self.chat_widget.finish_backend_banner_fallback(mode);
            self.sync_active_thread_service_tier_to_cached_session()
                .await;
            let mut message = format!("Automatically switched to {}", target.model);
            if let Some(label) = Self::reasoning_label_for(&target.model, Some(&effort)) {
                message.push(' ');
                message.push_str(&label);
            }
            message.push_str(" due to usage limits.");
            self.chat_widget.add_info_message(message, /*hint*/ None);
        }
    }
}
