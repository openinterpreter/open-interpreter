//! Low-cardinality metrics for Guardian V2 classification and approval decisions.

use std::time::Duration;

use codex_extension_api::ExtensionMetrics;

pub(super) const CLASSIFICATION_METRIC: &str = "codex.guardian_v2.classification";
pub(super) const CLASSIFICATION_DURATION_METRIC: &str =
    "codex.guardian_v2.classification.duration_ms";
pub(super) const CLASSIFICATION_RISK_METRIC: &str = "codex.guardian_v2.classification.risk";
pub(super) const FAST_DECISION_METRIC: &str = "codex.guardian_v2.fast_decision";
pub(super) const REVIEW_FALLBACK_METRIC: &str = "codex.guardian_v2.review_fallback";
pub(super) const TOOL_CALL_LAG_METRIC: &str = "codex.guardian_v2.tool_call_lag";

pub(super) fn record_classification(
    metrics: Option<&dyn ExtensionMetrics>,
    duration: Duration,
    outcome: &str,
) {
    let Some(metrics) = metrics else {
        return;
    };
    let tags = [("outcome", outcome)];
    metrics.counter(CLASSIFICATION_METRIC, /*inc*/ 1, &tags);
    metrics.histogram(
        CLASSIFICATION_DURATION_METRIC,
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        &tags,
    );
}

pub(super) fn record_classification_risk(metrics: Option<&dyn ExtensionMetrics>, risk_level: &str) {
    let Some(metrics) = metrics else {
        return;
    };
    metrics.counter(
        CLASSIFICATION_RISK_METRIC,
        /*inc*/ 1,
        &[("risk_level", risk_level)],
    );
}

pub(super) fn record_fast_decision(
    metrics: Option<&dyn ExtensionMetrics>,
    decision: &str,
    reason: &str,
) {
    let Some(metrics) = metrics else {
        return;
    };
    metrics.counter(
        FAST_DECISION_METRIC,
        /*inc*/ 1,
        &[("decision", decision), ("reason", reason)],
    );
}
