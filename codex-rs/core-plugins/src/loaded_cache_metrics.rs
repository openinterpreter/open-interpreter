//! Request counts exclude disabled plugins, account-switch discards, and cancellations.
//! Auth retries keep the furthest request outcome reached; each completed wait/load is timed.
//! Skill-snapshot lookups are not requests. Clear events include already-empty caches.
//! Tags never contain configuration data.

use std::time::Duration;

pub(crate) const LOAD_DURATION: &str = "codex.plugins.loaded_cache.load.duration_ms";
pub(crate) const WAIT_DURATION: &str = "codex.plugins.loaded_cache.wait.duration_ms";

pub(crate) enum RequestOutcome {
    Hit,
    HitAfterWait,
    Load,
}

impl RequestOutcome {
    pub(crate) fn record(self) {
        let Some(metrics) = codex_otel::global() else {
            return;
        };
        let outcome = match self {
            Self::Hit => "hit",
            Self::HitAfterWait => "hit_after_wait",
            Self::Load => "load",
        };
        let _ = metrics.counter(
            "codex.plugins.loaded_cache.request",
            /*inc*/ 1,
            &[("outcome", outcome)],
        );
    }
}

pub(crate) fn record_duration(name: &'static str, duration: Duration) {
    if let Some(metrics) = codex_otel::global() {
        let _ = metrics.record_duration(name, duration, &[]);
    }
}

pub(crate) fn record_event(event: &'static str) {
    if let Some(metrics) = codex_otel::global() {
        let _ = metrics.counter(
            "codex.plugins.loaded_cache.event",
            /*inc*/ 1,
            &[("event", event)],
        );
    }
}
