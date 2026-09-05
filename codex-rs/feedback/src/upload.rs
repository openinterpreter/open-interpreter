use std::io;
use std::io::Write;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use bytes::Bytes;
use codex_http_client::RouteAwareClientPool;
use codex_http_client::RouteAwareRequestBuilder;
use flate2::Compression;
use flate2::write::GzEncoder;
use http::HeaderMap;
use http::StatusCode;
use sentry::ClientOptions;
use sentry::protocol::Attachment;
use sentry::protocol::Envelope;
use sentry::protocol::EnvelopeHeaders;
use sentry::types::Dsn;

use crate::MAX_DECODED_UPLOAD_BYTES;
use crate::MAX_UPLOAD_BYTES;
use crate::attachment_truncation::truncate_attachment;

pub(super) const DEFAULT_RATE_LIMIT: Duration = Duration::from_secs(/*secs*/ 60);

pub(super) enum EnvelopeKind {
    Event,
    Attachment,
}

pub(super) fn gzip_envelope_request(
    client_pool: &RouteAwareClientPool,
    dsn: &Dsn,
    body: Bytes,
    timeout: Duration,
) -> RouteAwareRequestBuilder {
    let sentry_options = ClientOptions::default();
    let sentry_auth = dsn.to_auth(Some(sentry_options.user_agent.as_ref()));
    client_pool
        .post(dsn.envelope_api_url().as_str())
        .header("X-Sentry-Auth", sentry_auth.to_string())
        .header("Content-Encoding", "gzip")
        .body(body)
        .timeout(timeout)
}

/// Send an already-serialized envelope within the report's shared deadline.
pub(super) async fn send_gzip_envelope(
    client_pool: &RouteAwareClientPool,
    dsn: &Dsn,
    body: Vec<u8>,
    kind: EnvelopeKind,
    deadline: Instant,
    rate_limited: &mut bool,
) -> Result<StatusCode> {
    let body = Bytes::from(body);
    // Retain the exact gzip bytes for at most two diagnostic retries. Never replay the core.
    let mut retry_delays = [250, 500].map(Duration::from_millis).into_iter();
    let response = loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        anyhow::ensure!(!timeout.is_zero(), "feedback upload deadline exceeded");
        let response = gzip_envelope_request(client_pool, dsn, body.clone(), timeout)
            .send()
            .await;
        if response.as_ref().is_ok_and(|response| {
            sentry_rate_limit_delay(response.headers()).is_some_and(|delay| !delay.is_zero())
                || (response.status().is_success()
                    && response.headers().get("Retry-After").is_some_and(|value| {
                        parse_retry_after(value.to_str().unwrap_or_default())
                            .is_none_or(|delay| !delay.is_zero())
                    }))
        }) {
            // Even accepted requests can impose a quota on events or attachments.
            // https://develop.sentry.dev/sdk/foundations/transport/rate-limiting/
            *rate_limited = true;
            break response;
        }
        let retryable = match &response {
            Ok(response) => matches!(response.status().as_u16(), 408 | 429 | 500..=599),
            Err(error) => {
                error.is_timeout()
                    || (error.failure_class().is_none()
                        && (error.is_connect() || error.is_request() || error.is_body()))
            }
        };
        if matches!(kind, EnvelopeKind::Event) || !retryable {
            break response;
        }
        let retry_delay = retry_delays.next();
        if response.as_ref().is_ok_and(|response| {
            response.status() == StatusCode::TOO_MANY_REQUESTS
                && (retry_delay.is_none() || !response.headers().contains_key("Retry-After"))
        }) {
            // Never let later files bypass an exhausted rate limit.
            *rate_limited = true;
            break response;
        }
        let Some(retry_delay) = retry_delay else {
            break response;
        };
        let retry_after = response
            .as_ref()
            .ok()
            .and_then(|response| response.headers().get("Retry-After"))
            .map(|value| {
                // An invalid cooldown is not permission to send immediately.
                parse_retry_after(value.to_str().unwrap_or_default())
                    .unwrap_or_else(|| deadline.saturating_duration_since(Instant::now()))
            });
        let delay = retry_delay.max(retry_after.unwrap_or_default());
        if delay >= deadline.saturating_duration_since(Instant::now()) {
            // Leave long cooldowns for a later submission, including its remaining files.
            *rate_limited = true;
            break response;
        }
        tokio::time::sleep(delay).await;
    };
    response
        .map(|response| response.status())
        .context("failed to upload feedback to Sentry")
}

pub(super) fn sentry_rate_limit_delay(headers: &HeaderMap) -> Option<Duration> {
    let mut retry_after = None;
    for value in headers.get_all("X-Sentry-Rate-Limits") {
        for quota in value.to_str().unwrap_or_default().split(',') {
            let mut fields = quota.trim().split(':');
            let seconds = fields.next().unwrap_or_default();
            let categories = fields.next().unwrap_or_default();
            if !categories.is_empty()
                && !categories
                    .split(';')
                    .any(|category| matches!(category, "error" | "attachment"))
            {
                continue;
            }
            let delay = seconds
                .parse::<f64>()
                .ok()
                .and_then(|seconds| Duration::try_from_secs_f64(seconds.ceil()).ok())
                .unwrap_or(DEFAULT_RATE_LIMIT);
            retry_after = retry_after.max(Some(delay));
        }
    }
    retry_after
}

pub(super) fn parse_retry_after(value: &str) -> Option<Duration> {
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .or_else(|_| {
            httpdate::parse_http_date(value)
                .map(|date| date.duration_since(SystemTime::now()).unwrap_or_default())
        })
        .ok()
}

pub(super) fn gzip_envelope(envelope: &Envelope) -> io::Result<(Vec<u8>, usize)> {
    let mut writer = CountingWriter {
        inner: GzEncoder::new(Vec::new(), Compression::fast()),
        bytes: 0,
    };
    envelope.to_writer(&mut writer)?;
    Ok((writer.inner.finish()?, writer.bytes))
}

pub(super) fn gzip_attachment_envelope(
    headers: &EnvelopeHeaders,
    mut attachment: Attachment,
) -> Result<Vec<u8>> {
    let envelope = Envelope::new().with_headers(headers.clone());
    // Apply the file reader's bound to in-memory attachments too. The extra byte
    // lets the format-aware truncation below detect and label a shortened copy.
    attachment.buffer.truncate(MAX_DECODED_UPLOAD_BYTES + 1);
    let mut target_bytes = MAX_DECODED_UPLOAD_BYTES;
    loop {
        truncate_attachment(
            &mut attachment.filename,
            &mut attachment.buffer,
            target_bytes,
        )?;
        let mut writer = CountingWriter {
            inner: GzEncoder::new(Vec::new(), Compression::fast()),
            bytes: 0,
        };
        // Use the SDK's framing while retaining the buffer for in-place truncation.
        // https://github.com/getsentry/sentry-rust/blob/0.46.1/sentry-types/src/protocol/envelope.rs#L453-L487
        envelope.to_writer(&mut writer)?;
        attachment.to_writer(&mut writer)?;
        writeln!(writer)?;
        let decoded_bytes = writer.bytes;
        let body = writer.inner.finish()?;
        if body.len() <= MAX_UPLOAD_BYTES && decoded_bytes <= MAX_DECODED_UPLOAD_BYTES {
            return Ok(body);
        }
        anyhow::ensure!(
            !attachment.buffer.is_empty(),
            "feedback attachment headers exceed the size limit"
        );
        target_bytes = attachment
            .buffer
            .len()
            .saturating_sub(decoded_bytes.saturating_sub(MAX_DECODED_UPLOAD_BYTES));
        if body.len() > MAX_UPLOAD_BYTES {
            target_bytes /= 2;
        }
    }
}

struct CountingWriter<W> {
    inner: W,
    bytes: usize,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
