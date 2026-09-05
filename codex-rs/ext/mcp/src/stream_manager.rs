//! Owns task-scoped MCP event streams without retaining their thread or runtime.
//! Each subscription has one worker, and cancellation never waits for output capacity.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadReadyInput;
use codex_extension_api::ThreadStartInput;
use codex_mcp::McpEventNotification;
use codex_mcp::McpEventStreamOpener;
use codex_mcp::McpResourceClient;
use codex_protocol::ThreadId;
use serde_json::Map;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::time::timeout_at;

const EVENT_UPDATE_CAPACITY: usize = 64;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 90);

/// A notification or stream ending, identified by task, subscription, and stream attempt.
pub enum McpEventStreamUpdate {
    Notification {
        thread_id: ThreadId,
        subscription_id: String,
        stream_attempt_id: u64,
        notification: McpEventNotification,
    },
    Ended {
        thread_id: ThreadId,
        subscription_id: String,
        stream_attempt_id: u64,
        result: Result<()>,
    },
}

struct ManagedEventStream {
    stream_attempt_id: u64,
    active: Arc<AtomicBool>,
    worker: JoinHandle<()>,
    cancel_event_streams_on_server_removal: watch::Sender<()>,
}

/// Tracks event streams by thread and subscription ID.
///
/// Notifications and stream endings go into a queue. Cancelling a
/// stream does not require adding an `Ended` update, so it still works when the queue is full.
pub struct McpEventStreamManager {
    streams: Mutex<HashMap<(ThreadId, String), ManagedEventStream>>,
    next_stream_attempt_id: AtomicU64,
    updates: mpsc::Sender<McpEventStreamUpdate>,
    shutdown: AtomicBool,
}

impl McpEventStreamManager {
    pub fn new() -> (Self, mpsc::Receiver<McpEventStreamUpdate>) {
        let (updates, receiver) = mpsc::channel(EVENT_UPDATE_CAPACITY);
        (
            Self {
                streams: Mutex::default(),
                next_stream_attempt_id: AtomicU64::new(/*v*/ 1),
                updates,
                shutdown: AtomicBool::new(/*v*/ false),
            },
            receiver,
        )
    }

    /// Opens a stream and returns its attempt ID after the server confirms activation.
    pub async fn start(
        &self,
        thread_id: ThreadId,
        subscription_id: String,
        opener: McpEventStreamOpener,
        event_name: String,
        arguments: Value,
        request_meta: Option<Map<String, Value>>,
    ) -> Result<u64> {
        let key = (thread_id, subscription_id.clone());
        let (ready_tx, ready_rx) = oneshot::channel();
        {
            let mut streams = self.streams.lock().unwrap_or_else(PoisonError::into_inner);
            if self.shutdown.load(Ordering::Acquire) {
                return Err(anyhow!("MCP event stream manager is shut down"));
            }
            streams.retain(|_, stream| !stream.worker.is_finished());
            if let Some(stream) = streams.get(&key) {
                if stream.active.load(Ordering::Acquire) {
                    return Ok(stream.stream_attempt_id);
                }
                return Err(anyhow!(
                    "MCP event subscription is already starting or finishing"
                ));
            }

            let stream_attempt_id = self
                .next_stream_attempt_id
                .fetch_add(/*val*/ 1, Ordering::Relaxed);
            let active = Arc::new(AtomicBool::new(/*v*/ false));
            let worker_active = Arc::clone(&active);
            let cancel_event_streams_on_server_removal = opener.event_stream_cancellation_sender();
            let updates = self.updates.clone();
            let deadline = Instant::now() + ACTIVATION_TIMEOUT;
            // Keep the stream running if the caller stops waiting for activation.
            let worker = tokio::spawn(async move {
                let mut ready = Some(ready_tx);
                let forward = async {
                    let mut stream = timeout_at(
                        deadline,
                        opener.open(&event_name, &arguments, request_meta.as_ref()),
                    )
                    .await
                    .map_err(|_| anyhow!("MCP event stream activation timed out"))??;

                    loop {
                        let notification = if ready.is_some() {
                            timeout_at(deadline, stream.recv())
                                .await
                                .map_err(|_| anyhow!("MCP event stream activation timed out"))??
                        } else {
                            stream.recv().await?
                        };
                        let Some(notification) = notification else {
                            return Ok(());
                        };

                        let activates = notification.method == "notifications/events/active";
                        let terminates = notification.method == "notifications/events/terminated";
                        if activates {
                            worker_active.store(/*val*/ true, Ordering::Release);
                            if let Some(ready) = ready.take() {
                                let _ = ready.send(Ok(stream_attempt_id));
                            }
                        }

                        let send = updates.send(McpEventStreamUpdate::Notification {
                            thread_id,
                            subscription_id: subscription_id.clone(),
                            stream_attempt_id,
                            notification,
                        });
                        let sent = if ready.is_some() {
                            timeout_at(deadline, send)
                                .await
                                .map_err(|_| anyhow!("MCP event stream activation timed out"))?
                        } else {
                            send.await
                        };
                        sent.map_err(|_| anyhow!("MCP event stream receiver was dropped"))?;

                        if terminates {
                            return Ok(());
                        }
                    }
                };
                let result = tokio::select! {
                    biased;
                    () = opener.wait_for_access_change() => {
                        Err(anyhow!("MCP event stream access changed"))
                    }
                    () = updates.closed() => {
                        Err(anyhow!("MCP event stream receiver was dropped"))
                    }
                    result = forward => result,
                };

                worker_active.store(/*val*/ false, Ordering::Release);
                drop(opener);
                if let Some(ready) = ready {
                    let error = match &result {
                        Ok(()) => anyhow!("MCP event stream ended before becoming active"),
                        Err(error) => anyhow!("{error:#}"),
                    };
                    let _ = ready.send(Err(error));
                }

                let _ = updates
                    .send(McpEventStreamUpdate::Ended {
                        thread_id,
                        subscription_id,
                        stream_attempt_id,
                        result,
                    })
                    .await;
            });
            streams.insert(
                key,
                ManagedEventStream {
                    stream_attempt_id,
                    active,
                    worker,
                    cancel_event_streams_on_server_removal,
                },
            );
        }

        ready_rx
            .await
            .map_err(|_| anyhow!("MCP event stream ended before becoming active"))?
    }

    /// Await cancellation before starting the same subscription again.
    pub async fn cancel(&self, thread_id: ThreadId, subscription_id: &str) {
        let stream = self
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(thread_id, subscription_id.to_owned()));
        if let Some(stream) = stream {
            stream.worker.abort();
            let _ = stream.worker.await;
        }
    }

    /// Permanently rejects new starts and waits for all current workers to release their streams.
    pub async fn shutdown(&self) {
        self.shutdown.store(/*val*/ true, Ordering::Release);
        let streams = self
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, stream)| stream)
            .collect::<Vec<_>>();
        for stream in &streams {
            stream.worker.abort();
        }
        for stream in streams {
            let _ = stream.worker.await;
        }
    }
}

impl<C: Sync> ThreadLifecycleContributor<C> for McpEventStreamManager {
    fn on_thread_start<'a>(&'a self, input: ThreadStartInput<'a, C>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            if let Some(client) = input.mcp_resource_client {
                input.thread_store.insert(client.as_ref().clone());
            }
        })
    }

    fn on_thread_ready<'a>(&'a self, input: ThreadReadyInput<'a, C>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let Some(client) = input.thread_store.get::<McpResourceClient>() else {
                return;
            };
            let Ok(thread_id) = ThreadId::from_string(input.thread_store.level_id()) else {
                return;
            };
            let cancellation = self
                .streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .find(|((owner, _), stream)| {
                    let belongs_to_task = *owner == thread_id;
                    let worker_is_running = !stream.worker.is_finished();
                    belongs_to_task && worker_is_running
                })
                .map(|(_, stream)| stream.cancel_event_streams_on_server_removal.clone());
            if let Some(cancellation) = cancellation {
                client.forward_event_server_removals_to(cancellation);
            }
        })
    }
}

impl Drop for McpEventStreamManager {
    fn drop(&mut self) {
        let mut streams = self.streams.lock().unwrap_or_else(PoisonError::into_inner);
        for (_, stream) in streams.drain() {
            stream.worker.abort();
        }
    }
}
