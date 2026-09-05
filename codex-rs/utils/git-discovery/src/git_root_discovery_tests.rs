//! Verifies shared probe results, capacity retention after cancellation, and
//! fresh discovery after repository changes. Blocked probes must not delay runtime shutdown.

use super::GitRootDiscovery;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;
use tokio::time::timeout;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Simulates a filesystem call that stays blocked until the test releases it.
fn blocked_root_discovery(
    capacity: usize,
) -> (Arc<GitRootDiscovery>, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let discovery = Arc::new(GitRootDiscovery {
        capacity,
        find_root: Arc::new(move |cwd| {
            entered_tx.send(()).expect("signal probe entry");
            let _ = release_rx
                .lock()
                .expect("release receiver")
                .recv_timeout(PROBE_TIMEOUT);
            Some(cwd.to_path_buf())
        }),
        ..Default::default()
    });
    (discovery, entered_rx, release_tx)
}

#[test]
fn root_discovery_shares_probes_across_callers_and_cancellation() {
    const TIMEOUT: Duration = Duration::from_secs(5);
    let workspace = tempfile::tempdir().expect("workspace");
    let first_cwd = AbsolutePathBuf::from_absolute_path(workspace.path().join("first"))
        .expect("absolute first cwd");
    let second_cwd = AbsolutePathBuf::from_absolute_path(workspace.path().join("second"))
        .expect("absolute second cwd");
    let third_cwd = AbsolutePathBuf::from_absolute_path(workspace.path().join("third"))
        .expect("absolute third cwd");
    let (discovery, entered_rx, release_tx) = blocked_root_discovery(/*capacity*/ 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    runtime.block_on(async {
        let mut first = Box::pin(discovery.discover(first_cwd.clone()));
        let mut duplicate = Box::pin(discovery.discover(first_cwd.clone()));
        let mut second = Box::pin(discovery.discover(second_cwd.clone()));
        assert!(futures::poll!(first.as_mut()).is_pending());
        assert!(futures::poll!(duplicate.as_mut()).is_pending());
        // Sharing the first cwd leaves room for a different directory.
        assert!(futures::poll!(second.as_mut()).is_pending());
        entered_rx
            .recv_timeout(TIMEOUT)
            .expect("first probe started");
        entered_rx
            .recv_timeout(TIMEOUT)
            .expect("second probe started");
        drop(first);
        drop(duplicate);
        // Cancelling every waiter does not free capacity while its worker is blocked.
        let mut third = Box::pin(discovery.discover(third_cwd.clone()));
        assert!(futures::poll!(third.as_mut()).is_pending());
        let mut cancelled = Box::pin(discovery.discover(first_cwd.join("cancelled")));
        assert!(futures::poll!(cancelled.as_mut()).is_pending());
        drop(cancelled);
        assert!(entered_rx.try_recv().is_err());
        let mut retry = Box::pin(discovery.discover(first_cwd.clone()));
        // The same directory can still join its probe when capacity is full.
        assert!(futures::poll!(retry.as_mut()).is_pending());
        let mut follower = Box::pin(discovery.discover(first_cwd.clone()));
        assert!(futures::poll!(follower.as_mut()).is_pending());

        release_tx.send(()).expect("release first probe");
        release_tx.send(()).expect("release second probe");
        release_tx.send(()).expect("release queued probe");
        let roots = timeout(TIMEOUT, async {
            (retry.await, follower.await, second.await, third.await)
        })
        .await
        .expect("running and queued root probes finished");
        assert_eq!(
            roots,
            (
                Some(first_cwd.to_path_buf()),
                Some(first_cwd.to_path_buf()),
                Some(second_cwd.to_path_buf()),
                Some(third_cwd.to_path_buf()),
            )
        );
        entered_rx.try_recv().expect("queued probe started");
        assert!(entered_rx.try_recv().is_err());
    });
}

#[tokio::test]
async fn completed_root_discoveries_are_not_cached() {
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = AbsolutePathBuf::from_absolute_path(workspace.path()).expect("absolute cwd");
    let discovery = Arc::new(GitRootDiscovery {
        capacity: 1,
        ..Default::default()
    });
    assert_eq!(discovery.discover(cwd.clone()).await, None);

    std::fs::create_dir(cwd.join(".git")).expect("git directory");
    std::fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/main\n").expect("git HEAD");
    assert_eq!(
        discovery.discover(cwd.clone()).await,
        Some(cwd.to_path_buf())
    );

    std::fs::remove_dir_all(cwd.join(".git")).expect("remove Git metadata");
    assert_eq!(discovery.discover(cwd).await, None);
}

#[tokio::test]
async fn saturated_discovery_does_not_block_other_instances() {
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = AbsolutePathBuf::from_absolute_path(workspace.path()).expect("absolute cwd");
    let (saturated, entered_rx, release_tx) = blocked_root_discovery(/*capacity*/ 1);
    let mut blocked = Box::pin(saturated.discover(cwd.clone()));
    assert!(futures::poll!(blocked.as_mut()).is_pending());
    entered_rx
        .recv_timeout(PROBE_TIMEOUT)
        .expect("probe started");

    let available = Arc::new(GitRootDiscovery::default());
    let root = timeout(Duration::from_secs(5), available.discover(cwd.clone())).await;
    release_tx.send(()).expect("release probe");
    assert_eq!(root.expect("independent discovery finishes"), None);
    assert_eq!(blocked.await, Some(cwd.to_path_buf()));
}

#[test]
fn blocked_root_discovery_does_not_delay_runtime_shutdown() {
    const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
    const WORKER_THREADS: usize = 1;
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = AbsolutePathBuf::from_absolute_path(workspace.path()).expect("absolute cwd");
    let (discovery, entered_rx, release_tx) = blocked_root_discovery(/*capacity*/ 1);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .enable_all()
        .build()
        .expect("runtime");
    let lookup = runtime.spawn({
        let discovery = Arc::clone(&discovery);
        async move { discovery.discover(cwd).await }
    });
    entered_rx
        .recv_timeout(PROBE_TIMEOUT)
        .expect("probe started");
    lookup.abort();
    assert!(
        runtime
            .block_on(lookup)
            .expect_err("cancelled lookup")
            .is_cancelled()
    );

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let shutdown = std::thread::spawn(move || {
        drop(runtime);
        let _ = shutdown_tx.send(());
    });
    let shutdown_result = shutdown_rx.recv_timeout(SHUTDOWN_TIMEOUT);
    let outstanding = discovery
        .in_flight
        .lock()
        .expect("outstanding probes")
        .len();
    // Release the probe even if shutdown timed out so a regression cannot hang the test.
    release_tx.send(()).expect("release probe");
    shutdown.join().expect("shutdown thread");
    assert_eq!((shutdown_result, outstanding), (Ok(()), 1));
}
