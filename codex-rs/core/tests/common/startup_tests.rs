//! Keeps startup failures visible even when downstream mocks have unmet expectations.

use super::expect_startup;
use crate::responses::ev_completed;
use crate::responses::mount_sse_sequence;
use crate::responses::sse;
use crate::responses::start_mock_server;
use anyhow::Result;
use anyhow::anyhow;
use pretty_assertions::assert_eq;
use std::future::pending;
use std::future::ready;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test(start_paused = true)]
async fn startup_can_complete_after_five_seconds() {
    let value = expect_startup(async {
        sleep(Duration::from_secs(/*secs*/ 6)).await;
        Ok("ready")
    })
    .await;
    assert_eq!(value, "ready");
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "test fixture startup timed out")]
async fn startup_timeout_is_not_masked_by_mock_expectations() {
    let server = start_mock_server().await;
    mount_sse_sequence(&server, vec![sse(vec![ev_completed("unused")])]).await;

    expect_startup(pending::<Result<()>>()).await;
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "injected startup failure")]
async fn startup_error_is_not_masked_by_mock_expectations() {
    let server = start_mock_server().await;
    mount_sse_sequence(&server, vec![sse(vec![ev_completed("unused")])]).await;
    let startup: Result<()> = Err(anyhow!("injected startup failure"));

    expect_startup(ready(startup)).await;
}
