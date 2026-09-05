use std::sync::Mutex;
use std::time::Duration;

use pretty_assertions::assert_eq;

use super::DbTelemetry;
use super::record_log_queue_drop;
use super::record_log_write;
use crate::LOG_QUEUE_DROPPED_METRIC;
use crate::LOG_WRITE_BYTES_METRIC;
use crate::LOG_WRITE_DURATION_METRIC;
use crate::LOG_WRITE_ENTRIES_METRIC;
use crate::LOG_WRITE_MAX_ENTRY_BYTES_METRIC;
use crate::LOG_WRITE_METRIC;
use crate::LogEntry;

type MetricEvent = (String, i64, Vec<(String, String)>);

#[derive(Default)]
struct TestTelemetry(Mutex<Vec<MetricEvent>>);

impl TestTelemetry {
    fn record(&self, name: &str, value: i64, tags: &[(&str, &str)]) {
        self.0.lock().expect("telemetry lock").push((
            name.to_string(),
            value,
            tags.iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        ));
    }
}

impl DbTelemetry for TestTelemetry {
    fn counter(&self, name: &str, inc: i64, tags: &[(&str, &str)]) {
        self.record(name, inc, tags);
    }

    fn histogram(&self, name: &str, value: i64, tags: &[(&str, &str)]) {
        self.record(name, value, tags);
    }

    fn record_duration(&self, name: &str, duration: Duration, tags: &[(&str, &str)]) {
        self.record(name, duration.as_millis() as i64, tags);
    }
}

fn log_entry(message: &str, feedback_log_body: Option<&str>) -> LogEntry {
    LogEntry {
        ts: 1,
        ts_nanos: 2,
        level: "INFO".to_string(),
        target: "t".to_string(),
        message: Some(message.to_string()),
        feedback_log_body: feedback_log_body.map(str::to_string),
        thread_id: None,
        process_uuid: None,
        module_path: Some("m".to_string()),
        file: Some("f".to_string()),
        line: None,
    }
}

fn expected_events(metrics: &[(&str, i64)], tags: &[(&str, &str)]) -> Vec<MetricEvent> {
    metrics
        .iter()
        .map(|(name, value)| {
            (
                (*name).to_string(),
                *value,
                tags.iter()
                    .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn records_successful_log_write_duration_batch_size_and_largest_entry() {
    let telemetry = TestTelemetry::default();
    let entries = [
        log_entry("abc", /*feedback_log_body*/ None),
        log_entry("ignored", Some("longer")),
    ];

    record_log_write(
        Some(&telemetry),
        Duration::from_millis(17),
        &entries,
        &Ok(()),
    );

    assert_eq!(
        *telemetry.0.lock().expect("telemetry lock"),
        expected_events(
            &[
                (LOG_WRITE_METRIC, 1),
                (LOG_WRITE_DURATION_METRIC, 17),
                (LOG_WRITE_BYTES_METRIC, 23),
                (LOG_WRITE_ENTRIES_METRIC, 2),
                (LOG_WRITE_MAX_ENTRY_BYTES_METRIC, 13),
            ],
            &[("status", "success"), ("error", "none")],
        ),
    );
}

#[test]
fn records_failed_log_write_with_bounded_sqlite_error() {
    let telemetry = TestTelemetry::default();
    let entries = [log_entry("abc", /*feedback_log_body*/ None)];
    let result = Err(anyhow::Error::from(sqlx::Error::PoolTimedOut));

    record_log_write(
        Some(&telemetry),
        Duration::from_millis(35),
        &entries,
        &result,
    );

    assert_eq!(
        *telemetry.0.lock().expect("telemetry lock"),
        expected_events(
            &[
                (LOG_WRITE_METRIC, 1),
                (LOG_WRITE_DURATION_METRIC, 35),
                (LOG_WRITE_BYTES_METRIC, 10),
                (LOG_WRITE_ENTRIES_METRIC, 1),
                (LOG_WRITE_MAX_ENTRY_BYTES_METRIC, 10),
            ],
            &[("status", "failed"), ("error", "pool_timeout")],
        ),
    );
}

#[test]
fn records_queue_drop_reason() {
    let telemetry = TestTelemetry::default();

    record_log_queue_drop("full", Some(&telemetry));

    assert_eq!(
        *telemetry.0.lock().expect("telemetry lock"),
        expected_events(&[(LOG_QUEUE_DROPPED_METRIC, 1)], &[("reason", "full")]),
    );
}
