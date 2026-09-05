//! Bounded, shared Git-root discovery for optional metadata.
//!
//! Each instance bounds detached probes and shares one outstanding probe per
//! supplied cwd. Workers retain their entries through caller cancellation and remove
//! them when filesystem work ends; completed results are not cached. Capacity waits
//! are cancellable; keys are compared without filesystem canonicalization.
//! Probes never enter Tokio's blocking pool, so a stuck filesystem cannot keep its
//! runtime shutdown waiting for optional metadata work.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use codex_git_utils::get_git_repo_root;
use codex_utils_absolute_path::AbsolutePathBuf;
use futures::FutureExt;
use futures::future::BoxFuture;
use futures::future::Shared;
use tokio::sync::Notify;
use tokio::sync::oneshot;

const MAX_CONCURRENT_ROOT_PROBES: usize = 8;

type SharedRootProbe = Shared<BoxFuture<'static, Option<PathBuf>>>;
type RootFinder = dyn Fn(&Path) -> Option<PathBuf> + Send + Sync;

/// Shares outstanding metadata probes by cwd and bounds probes across directories.
/// Entries survive caller cancellation but are not cached after filesystem work ends.
pub struct GitRootDiscovery {
    capacity: usize,
    in_flight: Mutex<HashMap<AbsolutePathBuf, SharedRootProbe>>,
    find_root: Arc<RootFinder>,
    capacity_changed: Notify,
}

impl Default for GitRootDiscovery {
    fn default() -> Self {
        Self {
            capacity: MAX_CONCURRENT_ROOT_PROBES,
            in_flight: Mutex::new(HashMap::new()),
            find_root: Arc::new(get_git_repo_root),
            capacity_changed: Notify::new(),
        }
    }
}

impl GitRootDiscovery {
    /// Joins the cwd's existing probe, waiting for capacity to start a detached
    /// worker when necessary. Dropping the future cancels only the wait.
    pub async fn discover(self: &Arc<Self>, cwd: AbsolutePathBuf) -> Option<PathBuf> {
        let (result, new_probe) = loop {
            // notify_waiters reaches futures created before the capacity check,
            // even if they have not been polled yet.
            let capacity_changed = self.capacity_changed.notified();
            {
                let mut in_flight = self
                    .in_flight
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(result) = in_flight.get(&cwd) {
                    break (result.clone(), None);
                }
                if in_flight.len() < self.capacity {
                    let (result_tx, result_rx) = oneshot::channel();
                    let result = async move { result_rx.await.unwrap_or_default() }
                        .boxed()
                        .shared();
                    in_flight.insert(cwd.clone(), result.clone());
                    let probe = ProbeGuard {
                        cwd,
                        discovery: Arc::clone(self),
                    };
                    break (result, Some((probe, result_tx)));
                }
            }
            capacity_changed.await;
        };
        if let Some((probe, result_tx)) = new_probe {
            // Other waiters for this cwd can now join the new probe.
            self.capacity_changed.notify_waiters();
            // A failed spawn drops the closure and its guard, so do not hold the map lock.
            // Dropping the handle detaches the thread; Tokio never owns or joins it.
            let worker = std::thread::Builder::new()
                .name("codex-git-root".to_string())
                .spawn(move || {
                    let root = (probe.discovery.find_root)(probe.cwd.as_path());
                    drop(probe);
                    let _ = result_tx.send(root);
                });
            if let Err(error) = worker {
                tracing::warn!(%error, "failed to start optional Git root probe");
            }
        }
        result.await
    }
}

/// Keeps an outstanding entry owned by its blocking worker, including during
/// cancellation or unwinding, rather than by any caller waiting for the result.
struct ProbeGuard {
    cwd: AbsolutePathBuf,
    discovery: Arc<GitRootDiscovery>,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        self.discovery
            .in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.cwd);
        self.discovery.capacity_changed.notify_waiters();
    }
}

#[cfg(test)]
#[path = "git_root_discovery_tests.rs"]
mod tests;
