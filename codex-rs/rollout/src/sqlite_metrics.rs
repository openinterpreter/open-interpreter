use std::sync::Arc;
use std::time::Duration;

use codex_otel::ORIGINATOR_TAG;
use codex_otel::bounded_originator_tag_value;
use codex_state::DbTelemetry;
use codex_state::DbTelemetryHandle;
use codex_state::LOG_WRITE_BYTES_METRIC;
use codex_state::LOG_WRITE_MAX_ENTRY_BYTES_METRIC;

const LOG_WRITE_BYTES_BOUNDARIES: &[f64] = &[
    128.0,
    256.0,
    512.0,
    1_024.0,
    2_048.0,
    4_096.0,
    8_192.0,
    16_384.0,
    32_768.0,
    65_536.0,
    131_072.0,
    262_144.0,
    524_288.0,
    1_048_576.0,
    2_097_152.0,
    4_194_304.0,
    8_388_608.0,
    16_777_216.0,
];

struct OtelDbTelemetry {
    metrics: codex_otel::MetricsClient,
    originator: &'static str,
}

impl DbTelemetry for OtelDbTelemetry {
    fn counter(&self, name: &str, inc: i64, tags: &[(&str, &str)]) {
        let tags = with_originator(tags, self.originator);
        let _ = self.metrics.counter(name, inc, &tags);
    }

    fn histogram(&self, name: &str, value: i64, tags: &[(&str, &str)]) {
        let tags = with_originator(tags, self.originator);
        let _ = match name {
            LOG_WRITE_BYTES_METRIC | LOG_WRITE_MAX_ENTRY_BYTES_METRIC => self
                .metrics
                .histogram_with_boundaries(name, value, LOG_WRITE_BYTES_BOUNDARIES, &tags),
            _ => self.metrics.histogram(name, value, &tags),
        };
    }

    fn record_duration(&self, name: &str, duration: Duration, tags: &[(&str, &str)]) {
        let tags = with_originator(tags, self.originator);
        let _ = self.metrics.record_duration(name, duration, &tags);
    }
}

pub(crate) fn recorder(metrics: codex_otel::MetricsClient, originator: &str) -> DbTelemetryHandle {
    Arc::new(OtelDbTelemetry {
        metrics,
        originator: bounded_originator_tag_value(originator),
    })
}

fn with_originator<'a>(
    tags: &[(&'a str, &'a str)],
    originator: &'static str,
) -> Vec<(&'a str, &'a str)> {
    let mut tags = tags.to_vec();
    tags.push((ORIGINATOR_TAG, originator));
    tags
}
