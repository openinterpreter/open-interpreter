//! Backend banner payload and CTA mapping, supplied by the existing account usage read.
//!
//! The backend owns eligibility, copy, actions, and ordered model fallback instructions.

use codex_protocol::account::PlanType;
use serde::Deserialize;

mod actions;
mod render;

#[cfg(test)]
#[path = "backend_banners_tests.rs"]
mod tests;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct BackendBanner {
    pub(crate) banner_type: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) ctas: Vec<BackendBannerCta>,
    pub(crate) reset_at: Option<i64>,
    pub(crate) model_slug: Option<String>,
    pub(crate) blocked_model_slug: Option<String>,
    #[serde(default)]
    pub(crate) fallback_model_slugs: Vec<String>,
    #[serde(default)]
    pub(crate) presentation: BannerPresentation,
    request_url: Option<String>,
    #[serde(skip)]
    pub(crate) account_id: String,
    #[serde(skip)]
    pub(crate) plan_type: Option<PlanType>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BannerPresentation {
    #[default]
    Inline,
    Dismissible,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct BackendBannerCta {
    action: String,
    label: String,
}

impl BackendBanner {
    /// Parse supported, bounded content before constructing rendered copy or CTA closures.
    pub(crate) fn parse(raw: &serde_json::Value) -> Option<Self> {
        serde_json::from_value::<Self>(raw.clone())
            .ok()
            .filter(|banner| {
                let valid_slug = |slug: &str| {
                    !slug.trim().is_empty()
                        && slug.len() <= 256
                        && !slug.chars().any(char::is_control)
                };
                banner.title.len() <= 1024
                    && banner.description.len() <= 4096
                    && banner.ctas.len() <= 8
                    && !banner.title.trim().is_empty()
                    && banner.title.lines().count() <= 3
                    && banner.description.lines().count() <= 12
                    && banner.blocked_model_slug.as_deref().is_none_or(valid_slug)
                    && banner.fallback_model_slugs.len() <= 16
                    && banner
                        .fallback_model_slugs
                        .iter()
                        .all(|slug| valid_slug(slug))
            })
    }
}
