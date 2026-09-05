//! Account banner state, independent of the current model and the lifetime of its rendered view.
//!
//! Only explicitly dismissible, previously shown occurrences can be dismissed. Model changes hide
//! content without dismissing it; an authoritative replacement/absence starts a new occurrence.

use super::ChatWidget;
use crate::backend_banners::BackendBanner;
use crate::backend_banners::BannerPresentation;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::openai_models::ModelPreset;

#[derive(Default)]
pub(super) struct BackendBannerState {
    banner: Option<BackendBanner>,
    presented: Option<BackendBanner>,
    shown: bool,
    dismissed: bool,
}

impl ChatWidget {
    pub(crate) fn backend_banner_fallback(&self) -> Option<ModelPreset> {
        let banner = self.backend_banner_state.banner.as_ref()?;
        if !self.has_chatgpt_account
            || !self.config.model_provider.requires_openai_auth
            || banner.blocked_model_slug.as_deref() != Some(self.current_model())
        {
            return None;
        }
        let models = self.model_catalog.try_list_models().ok()?;
        banner.fallback_model_slugs.iter().find_map(|candidate| {
            models
                .iter()
                .find(|model| {
                    model.show_in_picker
                        && model.model == *candidate
                        && model.model != self.current_model()
                })
                .cloned()
        })
    }

    /// Commit a successfully applied task setting and keep its recovery notice visible together.
    pub(crate) fn finish_backend_banner_fallback(&mut self, mode: CollaborationMode) {
        self.set_model(mode.model());
        self.set_reasoning_effort(mode.reasoning_effort());
        if mode.mode == ModeKind::Plan {
            self.set_plan_mode_reasoning_effort(mode.reasoning_effort());
        }
        self.set_effective_collaboration_mode(mode);
        // Showing the post-switch notice is distinct from having seen the original blocked state.
        self.backend_banner_state.shown = false;
        self.backend_banner_state.dismissed = false;
        self.backend_banner_notice_model = Some(self.current_model().to_string());
        self.refresh_backend_banner_visibility();
    }

    pub(crate) fn inherit_backend_banner_state(&mut self, previous: &mut ChatWidget) {
        previous.observe_backend_banner_view();
        self.backend_banner_state = std::mem::take(&mut previous.backend_banner_state);
        self.backend_banner_state.presented = None;
        self.refresh_backend_banner_visibility();
    }

    pub(crate) fn update_backend_banner(&mut self, response: &GetAccountRateLimitsResponse) {
        self.observe_backend_banner_view();
        let banner = response
            .rate_limit_upsell
            .as_ref()
            .and_then(BackendBanner::parse)
            .map(|mut banner| {
                banner.account_id = response.account_id.clone().unwrap_or_default();
                banner.plan_type = response.rate_limits.plan_type;
                banner
            });
        let same_occurrence = self
            .backend_banner_state
            .banner
            .as_ref()
            .zip(banner.as_ref())
            .is_some_and(|(old, new)| {
                old.account_id == new.account_id
                    && old.banner_type == new.banner_type
                    && old.reset_at == new.reset_at
                    && old.presentation == new.presentation
                    && old.blocked_model_slug.as_ref().or(old.model_slug.as_ref())
                        == new.blocked_model_slug.as_ref().or(new.model_slug.as_ref())
                    && old.fallback_model_slugs == new.fallback_model_slugs
            });
        if !same_occurrence {
            self.backend_banner_state.shown = false;
            self.backend_banner_state.dismissed = false;
            self.backend_banner_notice_model = None;
        }
        self.backend_banner_state.banner = banner;
        self.refresh_backend_banner_visibility();
    }

    fn observe_backend_banner_view(&mut self) {
        let (shown, dismissed) = self.bottom_pane.inline_banner_lifecycle();
        self.backend_banner_state.shown |= shown;
        self.backend_banner_state.dismissed |= dismissed;
    }

    pub(super) fn refresh_backend_banner_visibility(&mut self) {
        let banner = self.backend_banner_state.banner.as_ref().filter(|banner| {
            // Explicit fallback payloads describe the selected replacement, not a pending switch.
            let matches_selected_model = match banner.blocked_model_slug.as_deref() {
                Some(blocked) if !banner.fallback_model_slugs.is_empty() => {
                    blocked != self.current_model()
                        && banner
                            .fallback_model_slugs
                            .iter()
                            .any(|model| model == self.current_model())
                }
                Some(_) | None => {
                    banner
                        .model_slug
                        .as_deref()
                        .is_none_or(|model| model == self.current_model())
                        || self.backend_banner_notice_model.as_deref() == Some(self.current_model())
                }
            };
            !self.backend_banner_state.dismissed && matches_selected_model
        });
        if banner == self.backend_banner_state.presented.as_ref() {
            return;
        }
        let content = banner.map(BackendBanner::actionable_banner);
        self.backend_banner_state.presented = banner.cloned();
        if content.is_some() {
            self.bottom_pane
                .dismiss_view_by_id(super::rate_limits::WORKSPACE_NUDGE_VIEW_ID);
            if self
                .bottom_pane
                .dismiss_view_by_id(super::rate_limits::RATE_LIMIT_SWITCH_PROMPT_VIEW_ID)
            {
                // Replacing the view is not the user's decision to dismiss the prompt.
                self.rate_limit_switch_prompt = super::RateLimitSwitchPromptState::Pending;
            }
        }
        self.bottom_pane.set_inline_banner(content);
        if self.backend_banner_state.presented.is_none()
            && !self.bottom_pane.is_task_running()
            && self.bottom_pane.no_modal_or_popup_active()
        {
            self.maybe_show_pending_rate_limit_prompt();
        }
    }

    pub(super) fn sync_backend_banner_view(&mut self) {
        self.observe_backend_banner_view();
        self.refresh_backend_banner_visibility();
    }

    pub(super) fn dismiss_backend_banner_for_new_turn(&mut self) {
        self.observe_backend_banner_view();
        if self.backend_banner_state.shown
            && self
                .backend_banner_state
                .presented
                .as_ref()
                .is_some_and(|banner| banner.presentation == BannerPresentation::Dismissible)
        {
            self.backend_banner_state.dismissed = true;
            self.refresh_backend_banner_visibility();
        }
    }

    pub(super) fn has_applicable_backend_banner(&self) -> bool {
        !self.backend_banner_state.dismissed && self.backend_banner_state.presented.is_some()
    }

    pub(crate) fn clear_backend_banner(&mut self) {
        self.backend_banner_state = BackendBannerState::default();
        self.backend_banner_notice_model = None;
        self.bottom_pane.set_inline_banner(/*banner*/ None);
    }
}
