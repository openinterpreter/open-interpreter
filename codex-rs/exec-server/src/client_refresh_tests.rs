use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::task::AbortOnDropHandle;

use super::*;
use crate::ExecServerRuntimePaths;
use crate::NoiseChannelIdentity;
use crate::ProcessId;
use crate::relay::HarnessKeyValidator;
use crate::relay::run_multiplexed_environment;
use crate::server::ConnectionProcessor;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;

// These tests exercise real sockets, not keepalive deadlines. The shared unit-test
// Pong timeout is only 100 ms and can expire during Noise handshakes under load.
// A blocking task prevents paused time from auto-advancing while socket I/O is
// pending; dropping the returned sender releases it without polling or sleeping.
fn freeze_clock() -> std::sync::mpsc::Sender<()> {
    tokio::time::pause();
    let (guard, dropped) = std::sync::mpsc::channel();
    tokio::task::spawn_blocking(move || {
        let _ = dropped.recv();
    });
    guard
}

#[derive(Clone)]
struct Target {
    url: String,
    identity: NoiseChannelIdentity,
    registration: String,
}

struct Registry {
    target: Mutex<Target>,
    next_lookup: Mutex<Option<oneshot::Receiver<()>>>,
    lookup_started: Notify,
}

impl Registry {
    fn block_next_lookup(&self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        *self.next_lookup.lock().unwrap() = Some(rx);
        tx
    }
}

impl NoiseRendezvousConnectProvider for Registry {
    fn connect_bundle(
        &self,
        _: NoiseChannelPublicKey,
    ) -> BoxFuture<'_, Result<NoiseRendezvousConnectBundle, ExecServerError>> {
        Box::pin(async move {
            let target = self.target.lock().unwrap().clone();
            let block = self.next_lookup.lock().unwrap().take();
            if let Some(block) = block {
                self.lookup_started.notify_one();
                block
                    .await
                    .map_err(|_| ExecServerError::Protocol("test lookup failed".to_owned()))?;
            }
            Ok(NoiseRendezvousConnectBundle {
                websocket_url: target.url,
                environment_id: "environment".to_owned(),
                executor_registration_id: target.registration,
                executor_public_key: target.identity.public_key(),
                harness_key_authorization: "authorization".to_owned(),
            })
        })
    }
}

#[derive(Clone, Default)]
struct Validator {
    handshake: Option<Arc<Notify>>,
    started: Arc<Notify>,
}

impl HarnessKeyValidator for Validator {
    async fn validate_harness_key(
        &self,
        _: &NoiseChannelPublicKey,
        _: &str,
    ) -> Result<(), ExecServerError> {
        if let Some(handshake) = &self.handshake {
            self.started.notify_one();
            handshake.notified().await;
        }
        Ok(())
    }
}

struct Executor {
    target: Target,
    _server: AbortOnDropHandle<()>,
}

impl Executor {
    async fn start(validator: Validator) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let target = Target {
            url: format!("ws://{}", listener.local_addr()?),
            identity: NoiseChannelIdentity::generate()?,
            registration: uuid::Uuid::new_v4().to_string(),
        };
        let executor = target.clone();
        let processor = ConnectionProcessor::new(ExecServerRuntimePaths::new(
            std::env::current_exe()?,
            /*codex_linux_sandbox_exe*/ None,
        )?);
        let server = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    socket = listener.accept() => {
                        let (socket, _) = socket.unwrap();
                        let executor = executor.clone();
                        let processor = processor.clone();
                        let validator = validator.clone();
                        connections.spawn(async move {
                            let socket = tokio_tungstenite::accept_async(socket).await.unwrap();
                            run_multiplexed_environment(socket, processor, "environment".to_owned(), executor.registration, executor.identity, validator).await;
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            target,
            _server: AbortOnDropHandle::new(server),
        })
    }

    fn client(&self) -> Result<(LazyRemoteExecServerClient, Arc<Registry>)> {
        let registry = Arc::new(Registry {
            target: Mutex::new(self.target.clone()),
            next_lookup: Mutex::new(None),
            lookup_started: Notify::new(),
        });
        let client = LazyRemoteExecServerClient::new(
            ExecServerTransportParams::NoiseRendezvous {
                provider: registry.clone(),
                identity: NoiseChannelIdentity::generate()?,
            },
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        );
        Ok((client, registry))
    }
}

async fn disconnect(client: &ExecServerClient) {
    let rpc_client = {
        let connection = client.inner.connection.lock().unwrap();
        let ConnectionStatus::Connected(rpc_client) = &connection.status else {
            panic!("expected connected session")
        };
        Arc::clone(rpc_client)
    };
    rpc_client.close_transport().await;
}

#[tokio::test]
async fn refresh_cancels_old_recovery_and_connects_without_resuming_old_session() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let original = client.get().await?;
    let process = original
        .register_session(&ProcessId::from("old-process"))
        .await?;
    let blocked_recovery = registry.block_next_lookup();
    disconnect(&original).await;
    registry.lookup_started.notified().await;
    *registry.target.lock().unwrap() = new.target.clone();
    let refresh = client.refresh_connection();
    tokio::pin!(refresh);
    let started = tokio::time::Instant::now();
    assert!(futures::poll!(refresh.as_mut()).is_pending());
    assert!(original.inner.retired.is_cancelled());
    assert!(original.is_disconnected());
    assert_eq!(started.elapsed(), Duration::ZERO);
    refresh.await?;
    let replacement = client.get().await?;
    assert_ne!(original.session_id(), replacement.session_id());
    assert_eq!(
        *client.environment_connection_state_tx.borrow(),
        EnvironmentConnectionState::Connected
    );
    assert!(matches!(
        process.write(b"never replay".to_vec()).await,
        Err(ExecServerError::Disconnected(_))
    ));
    // Releasing an old registry response cannot reinstall or disconnect the replacement.
    let _ = blocked_recovery.send(());
    tokio::task::yield_now().await;
    assert!(Arc::ptr_eq(&client.get().await?.inner, &replacement.inner));
    replacement.environment_status().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_retires_a_still_connected_old_executor() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let original = client.get().await?;
    *registry.target.lock().unwrap() = new.target.clone();
    client.refresh_connection().await?;
    assert!(original.is_disconnected());
    assert_ne!(original.session_id(), client.get().await?.session_id());
    Ok(())
}

#[tokio::test]
async fn refresh_preserves_a_current_session_across_registration_renewal() -> Result<()> {
    let _clock = freeze_clock();
    let executor = Executor::start(Validator::default()).await?;
    let (client, registry) = executor.client()?;
    let original = client.get().await?;
    registry.target.lock().unwrap().registration = "renewed-registration".to_owned();
    let concurrent = client.clone();
    let concurrent = tokio::spawn(async move { concurrent.refresh_connection().await });
    client.refresh_connection().await?;
    concurrent.await??;
    assert!(Arc::ptr_eq(&original.inner, &client.get().await?.inner));
    original.environment_status().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_cancels_a_stalled_initial_lookup() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let release = registry.block_next_lookup();
    let initial = client.get();
    tokio::pin!(initial);
    assert!(futures::poll!(initial.as_mut()).is_pending());
    *registry.target.lock().unwrap() = new.target.clone();
    client.refresh_connection().await?;
    assert!(initial.await.is_err());
    let _ = release.send(());
    client.get().await?.environment_status().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_cancels_a_stalled_noise_handshake() -> Result<()> {
    let _clock = freeze_clock();
    let validator = Validator {
        handshake: Some(Arc::new(Notify::new())),
        ..Default::default()
    };
    let old = Executor::start(validator.clone()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let connecting = client.clone();
    let initial = tokio::spawn(async move { connecting.get().await });
    validator.started.notified().await;
    *registry.target.lock().unwrap() = new.target.clone();
    client.refresh_connection().await?;
    assert!(initial.await?.is_err());
    validator.handshake.unwrap().notify_one();
    client.get().await?.environment_status().await?;
    assert_eq!(
        *client.environment_connection_state_tx.borrow(),
        EnvironmentConnectionState::Connected
    );
    Ok(())
}

#[tokio::test]
async fn failed_refresh_lookup_leaves_the_existing_session_usable() -> Result<()> {
    let _clock = freeze_clock();
    let executor = Executor::start(Validator::default()).await?;
    let (client, registry) = executor.client()?;
    let original = client.get().await?;
    drop(registry.block_next_lookup());
    assert!(client.refresh_connection().await.is_err());
    assert!(Arc::ptr_eq(&original.inner, &client.get().await?.inner));
    original.environment_status().await?;
    Ok(())
}

#[tokio::test]
async fn failed_replacement_connection_keeps_old_handles_retired_and_get_retries() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let original = client.get().await?;
    let process = original
        .register_session(&ProcessId::from("old-process"))
        .await?;

    // The registry knows the replacement, but its endpoint drops the new connection.
    let unavailable = TcpListener::bind("127.0.0.1:0").await?;
    let target = Target {
        url: format!("ws://{}", unavailable.local_addr()?),
        ..new.target.clone()
    };
    let _rejected_connection = AbortOnDropHandle::new(tokio::spawn(async move {
        drop(unavailable.accept().await.unwrap());
    }));
    *registry.target.lock().unwrap() = target;
    assert!(matches!(
        client.refresh_connection().await,
        Err(ExecServerError::ConnectionAttempt(_))
    ));
    assert!(original.inner.retired.is_cancelled());
    assert!(matches!(
        original.environment_status().await,
        Err(ExecServerError::Disconnected(_))
    ));
    assert!(matches!(
        process.write(b"never replay".to_vec()).await,
        Err(ExecServerError::Disconnected(_))
    ));
    assert_eq!(
        *client.environment_connection_state_tx.borrow(),
        EnvironmentConnectionState::Disconnected
    );

    // A later caller retries against the registry instead of reviving the retired client.
    *registry.target.lock().unwrap() = new.target.clone();
    let replacement = client.get().await?;
    assert_ne!(original.session_id(), replacement.session_id());
    replacement.environment_status().await?;
    assert_eq!(
        *client.environment_connection_state_tx.borrow(),
        EnvironmentConnectionState::Connected
    );
    assert!(matches!(
        process.write(b"still retired".to_vec()).await,
        Err(ExecServerError::Disconnected(_))
    ));
    Ok(())
}

#[tokio::test]
async fn superseded_refresh_lookup_does_not_retire_a_newer_session() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (client, registry) = old.client()?;
    let original = client.get().await?;
    let release = registry.block_next_lookup();
    let refreshing = client.refresh_connection();
    tokio::pin!(refreshing);
    assert!(futures::poll!(refreshing.as_mut()).is_pending());
    *registry.target.lock().unwrap() = new.target.clone();
    original.inner.retire().await;
    let replacement = client.get().await?;
    release.send(()).unwrap();
    refreshing.await?;
    assert!(Arc::ptr_eq(&replacement.inner, &client.get().await?.inner));
    assert!(!replacement.inner.retired.is_cancelled());
    replacement.environment_status().await?;
    Ok(())
}

#[tokio::test]
async fn environment_refresh_preserves_environment_and_filesystem_handles() -> Result<()> {
    let _clock = freeze_clock();
    let old = Executor::start(Validator::default()).await?;
    let new = Executor::start(Validator::default()).await?;
    let (_, registry) = old.client()?;
    let manager = crate::EnvironmentManager::from_snapshot(
        crate::environment_provider::EnvironmentProviderSnapshot {
            environments: Vec::new(),
            default: crate::environment_provider::EnvironmentDefault::Disabled,
            include_local: false,
        },
        /*local_runtime_paths*/ None,
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
    )?;
    let environment = manager
        .materialize_pending_noise_environment("environment".to_owned(), registry.clone())?;
    manager.report_environment_provisioning_status(
        "environment".to_owned(),
        Ok(crate::EnvironmentReadyInfo {
            selected_capability_roots: Vec::new(),
        }),
        registry.clone(),
    )?;
    environment.info().await?;
    let filesystem = environment.get_filesystem();
    *registry.target.lock().unwrap() = new.target.clone();
    environment.refresh_connection().await?;
    assert!(Arc::ptr_eq(
        &environment,
        &manager.get_environment("environment").unwrap()
    ));
    assert!(Arc::ptr_eq(&filesystem, &environment.get_filesystem()));
    environment.info().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_does_not_accept_cached_metadata_during_recovery() -> Result<()> {
    let _clock = freeze_clock();
    let executor = Executor::start(Validator::default()).await?;
    let (client, registry) = executor.client()?;
    let original = client.get().await?;
    original.environment_info().await?;
    let blocked_recovery = registry.block_next_lookup();
    disconnect(&original).await;
    registry.lookup_started.notified().await;
    let started = tokio::time::Instant::now();
    assert!(matches!(
        client.refresh_connection().await,
        Err(ExecServerError::Disconnected(_))
    ));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert!(!original.inner.retired.is_cancelled());
    drop(blocked_recovery);
    Ok(())
}

#[tokio::test]
async fn refresh_before_startup_marks_startup_finished() -> Result<()> {
    let _clock = freeze_clock();
    let executor = Executor::start(Validator::default()).await?;
    let (client, _) = executor.client()?;
    assert!(!client.startup_finished());
    client.refresh_connection().await?;
    assert!(client.startup_finished());
    assert!(matches!(client.readiness_result(), Some(Ok(()))));
    Ok(())
}

struct ControlledRpc {
    client: ExecServerClient,
    requests: tokio::sync::mpsc::Receiver<codex_exec_server_protocol::JSONRPCMessage>,
    responses: tokio::sync::mpsc::Sender<crate::connection::JsonRpcConnectionEvent>,
}

async fn controlled_rpc() -> Result<ControlledRpc> {
    use crate::connection::JsonRpcConnection;
    use crate::connection::JsonRpcConnectionEvent;
    use crate::connection::JsonRpcTransport;
    use codex_exec_server_protocol::JSONRPCMessage;
    use codex_exec_server_protocol::JSONRPCResponse;
    let (outgoing_tx, mut requests) = tokio::sync::mpsc::channel(/*buffer*/ 8);
    let (responses, incoming_rx) = tokio::sync::mpsc::channel(/*buffer*/ 8);
    let connection = JsonRpcConnection {
        outgoing_tx,
        incoming_rx,
        disconnected_rx: tokio::sync::watch::channel(/*init*/ false).1,
        task_handles: Vec::new(),
        transport: JsonRpcTransport::Plain,
    };
    let connecting = ExecServerClient::connect(connection, /*options*/ Default::default());
    tokio::pin!(connecting);
    assert!(futures::poll!(connecting.as_mut()).is_pending());
    let Some(JSONRPCMessage::Request(initialize)) = requests.recv().await else {
        anyhow::bail!("expected initialize request");
    };
    responses
        .send(JsonRpcConnectionEvent::message(JSONRPCMessage::Response(
            JSONRPCResponse {
                id: initialize.id,
                result: serde_json::json!({"sessionId": "controlled-session"}),
            },
        )))
        .await?;
    let client = connecting.await?;
    assert!(matches!(
        requests.recv().await,
        Some(JSONRPCMessage::Notification(_))
    ));
    Ok(ControlledRpc {
        client,
        requests,
        responses,
    })
}

#[tokio::test]
#[expect(
    clippy::await_holding_invalid_type,
    reason = "hold stream cleanup pending to exercise retirement ordering"
)]
async fn retirement_rejects_pending_mutation_before_stream_cleanup() -> Result<()> {
    use crate::connection::JsonRpcConnectionEvent;
    use codex_exec_server_protocol::JSONRPCMessage;
    use codex_exec_server_protocol::JSONRPCResponse;
    for response_queued in [false, true] {
        let mut rpc = controlled_rpc().await?;
        let call = rpc.client.fs_remove(crate::protocol::FsRemoveParams {
            path: "file:///retired-file".parse()?,
            recursive: None,
            force: None,
            follow_symlinks: None,
            sandbox: None,
        });
        tokio::pin!(call);
        assert!(futures::poll!(call.as_mut()).is_pending());
        let Some(JSONRPCMessage::Request(request)) = rpc.requests.recv().await else {
            anyhow::bail!("expected filesystem request");
        };
        let response = JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result: serde_json::json!({}),
        });
        if response_queued {
            rpc.responses
                .send(JsonRpcConnectionEvent::message(response.clone()))
                .await?;
            let transport = rpc.client.rpc_client_without_recovery()?;
            tokio::time::timeout(Duration::from_secs(5), async {
                while transport.pending_request_count().await != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
        }
        // Hold stream cleanup. Cover both a late response and one already queued
        // for the caller when retirement begins; closing the socket alone misses the latter.
        let streams = rpc.client.inner.http_body_streams_write_lock.lock().await;
        let retirement = rpc.client.inner.retire();
        tokio::pin!(retirement);
        assert!(futures::poll!(retirement.as_mut()).is_pending());
        if !response_queued {
            rpc.responses
                .send(JsonRpcConnectionEvent::message(response))
                .await?;
        }
        assert!(matches!(call.await, Err(ExecServerError::Disconnected(_))));
        drop(streams);
        retirement.await;
    }
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::await_holding_invalid_type,
    reason = "hold stream cleanup pending to exercise retirement ordering"
)]
async fn retirement_rejects_pending_process_start_before_stream_cleanup() -> Result<()> {
    use crate::connection::JsonRpcConnectionEvent;
    use codex_exec_server_protocol::JSONRPCMessage;
    use codex_exec_server_protocol::JSONRPCResponse;
    for response_queued in [false, true] {
        let mut rpc = controlled_rpc().await?;
        let process_id = ProcessId::from("retired-process");
        let call = rpc.client.start_process(
            crate::protocol::ExecParams {
                process_id: process_id.clone(),
                argv: vec!["unused".to_owned()],
                cwd: "file:///".parse()?,
                shell_snapshot: None,
                env_policy: None,
                env: Default::default(),
                tty: false,
                pipe_stdin: false,
                arg0: None,
                sandbox: None,
                enforce_managed_network: false,
                managed_network: None,
                network_proxy: None,
            },
            /*network_policy_decider*/ None,
        );
        tokio::pin!(call);
        assert!(futures::poll!(call.as_mut()).is_pending());
        let Some(JSONRPCMessage::Request(request)) = rpc.requests.recv().await else {
            anyhow::bail!("expected process start request");
        };
        let response = JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result: serde_json::json!({"processId": "retired-process"}),
        });
        if response_queued {
            rpc.responses
                .send(JsonRpcConnectionEvent::message(response.clone()))
                .await?;
            let state = rpc
                .client
                .inner
                .get_session(&process_id)
                .expect("pending process");
            // The start task has a second response channel to the original caller.
            tokio::time::timeout(Duration::from_secs(5), async {
                while !state.recoverable.load(std::sync::atomic::Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
        }
        let streams = rpc.client.inner.http_body_streams_write_lock.lock().await;
        let retirement = rpc.client.inner.retire();
        tokio::pin!(retirement);
        assert!(futures::poll!(retirement.as_mut()).is_pending());
        if !response_queued {
            rpc.responses
                .send(JsonRpcConnectionEvent::message(response))
                .await?;
        }
        assert!(matches!(call.await, Err(ExecServerError::Disconnected(_))));
        drop(streams);
        retirement.await;
    }
    Ok(())
}
