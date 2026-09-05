//! Forwards loopback OTLP trace batches to a separate WebSocket listener.
//!
//! Forwarding is best effort: disconnected or lagging clients lose batches.
//! Callers must flush their trace provider before dropping the bridge.
//! Dropping the bridge aborts its listeners without draining queued batches.

use std::net::SocketAddr;

use anyhow::Context;
use anyhow::Result;
use axum::Router;
use axum::body::Body;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::http::header::ORIGIN;
use axum::middleware;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::any;
use axum::routing::post;
use futures::SinkExt;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tracing::info;
use tracing::warn;

const TRACE_CHANNEL_CAPACITY: usize = 1_024;
const MAX_TRACE_WEBSOCKET_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Owns the loopback OTLP receiver and trace WebSocket listener tasks.
pub struct TraceWebSocket {
    exporter_endpoint: String,
    listen_addr: SocketAddr,
    tasks: JoinSet<Result<()>>,
}

impl TraceWebSocket {
    /// Binds both listeners before returning, so startup reports address conflicts.
    pub async fn start(listen_url: &str) -> Result<Self> {
        let bind_address = listen_url
            .strip_prefix("ws://")
            .ok_or_else(|| anyhow::anyhow!("unsupported --otel-trace-listen URL '{listen_url}'"))?
            .parse::<SocketAddr>()
            .with_context(|| {
                format!("invalid --otel-trace-listen URL '{listen_url}'; expected 'ws://IP:PORT'")
            })?;
        let websocket_listener = TcpListener::bind(bind_address)
            .await
            .with_context(|| format!("failed to bind OTEL trace websocket to {bind_address}"))?;
        let listen_addr = websocket_listener
            .local_addr()
            .context("failed to read OTEL trace websocket listen address")?;
        let otlp_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("failed to bind loopback OTLP trace receiver")?;
        let otlp_addr = otlp_listener
            .local_addr()
            .context("failed to read OTLP trace receiver listen address")?;
        let exporter_endpoint = format!("http://{otlp_addr}/v1/traces");
        let (sender, _) = broadcast::channel(TRACE_CHANNEL_CAPACITY);
        let otlp_router = Router::new()
            .route("/v1/traces", post(receive_otlp_trace_batch))
            .with_state(sender.clone());
        let websocket_router = Router::new()
            .route("/", any(trace_websocket_upgrade_handler))
            .layer(middleware::from_fn(reject_requests_with_origin_header))
            .with_state(sender);
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            axum::serve(otlp_listener, otlp_router)
                .await
                .context("OTLP trace receiver failed")
        });
        tasks.spawn(async move {
            axum::serve(websocket_listener, websocket_router)
                .await
                .context("OTEL trace websocket listener failed")
        });
        info!("OTLP trace receiver listening on http://{otlp_addr}");
        info!("OTEL trace websocket listening on ws://{listen_addr}");
        Ok(Self {
            exporter_endpoint,
            listen_addr,
            tasks,
        })
    }

    /// Returns the local OTLP/HTTP endpoint to configure with JSON trace export.
    pub fn exporter_endpoint(&self) -> &str {
        &self.exporter_endpoint
    }

    /// Returns the bound WebSocket address, including an assigned ephemeral port.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Waits for either listener to stop, for use alongside the main server future.
    pub async fn wait_for_failure(&mut self) -> Result<()> {
        self.tasks
            .join_next()
            .await
            .context("OTEL trace websocket server tasks stopped")?
            .context("OTEL trace websocket server task failed")?
    }
}

async fn reject_requests_with_origin_header(
    request: Request<Body>,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    if request.headers().contains_key(ORIGIN) {
        warn!(
            method = %request.method(),
            uri = %request.uri(),
            "rejecting OTEL trace websocket request with Origin header"
        );
        Err(StatusCode::FORBIDDEN)
    } else {
        Ok(next.run(request).await)
    }
}

async fn receive_otlp_trace_batch(
    State(sender): State<broadcast::Sender<Vec<u8>>>,
    body: Bytes,
) -> impl IntoResponse {
    let _ = sender.send(body.to_vec());
    ([(CONTENT_TYPE, "application/json")], "{}")
}

async fn trace_websocket_upgrade_handler(
    websocket: WebSocketUpgrade,
    State(sender): State<broadcast::Sender<Vec<u8>>>,
) -> impl IntoResponse {
    websocket
        .max_frame_size(MAX_TRACE_WEBSOCKET_FRAME_BYTES)
        .max_message_size(MAX_TRACE_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |stream| stream_trace_batches(stream, sender))
}

async fn stream_trace_batches(stream: WebSocket, sender: broadcast::Sender<Vec<u8>>) {
    let (mut writer, mut reader) = stream.split();
    let mut receiver = sender.subscribe();
    info!("OTEL trace websocket client connected");
    loop {
        tokio::select! {
            incoming = reader.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Text(_))) => {}
                Some(Err(error)) => {
                    warn!(%error, "OTEL trace websocket reader failed");
                    break;
                }
            },
            outgoing = receiver.recv() => match outgoing {
                Ok(batch) => {
                    if let Err(error) = writer.send(Message::Binary(batch.into())).await {
                        warn!(%error, "OTEL trace websocket writer failed");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(skipped, "OTEL trace websocket client lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
}
