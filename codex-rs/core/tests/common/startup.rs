//! Bounded startup assertions that preserve fixture failures during teardown.

use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

/// Allow local configuration and state-database setup on loaded CI workers.
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 30);

/// Fail before the caller's mock server drops and verifies downstream requests.
/// Returning an error instead would let an unmet mock expectation mask its cause.
pub async fn expect_startup<T>(startup: impl Future<Output = Result<T>>) -> T {
    timeout(STARTUP_TIMEOUT, startup)
        .await
        .expect("test fixture startup timed out")
        .expect("test fixture startup failed")
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod tests;
