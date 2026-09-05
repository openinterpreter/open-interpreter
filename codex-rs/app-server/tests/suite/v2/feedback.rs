use std::time::Duration;

use anyhow::Result;
use app_test_support::TestAppServer;
use codex_app_server_protocol::RequestId;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::time::timeout;

#[tokio::test]
async fn feedback_upload_limits_concurrency_and_releases_failed_uploads() -> Result<()> {
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_uri = format!("http://{}", proxy.local_addr()?);
    let mut app_server = TestAppServer::builder()
        .with_env_overrides(&[
            ("HTTPS_PROXY", Some(proxy_uri.as_str())),
            ("https_proxy", Some(proxy_uri.as_str())),
            ("NO_PROXY", Some("")),
            ("no_proxy", Some("")),
        ])
        .build_initialized()
        .await?;

    let mut pending = Vec::new();
    for _ in 0..3 {
        let request_id = app_server
            .send_raw_request(
                "feedback/upload",
                Some(json!({ "classification": "bug", "includeLogs": false })),
            )
            .await?;
        let (stream, _) = timeout(Duration::from_secs(/*secs*/ 15), proxy.accept()).await??;
        let mut stream = BufReader::new(stream);
        let mut request = String::new();
        timeout(
            Duration::from_secs(/*secs*/ 15),
            stream.read_line(&mut request),
        )
        .await??;
        assert!(request.starts_with("CONNECT "));
        pending.push((request_id, stream.into_inner()));
    }

    let excess_id = app_server
        .send_raw_request(
            "feedback/upload",
            Some(json!({ "classification": "bug", "includeLogs": false })),
        )
        .await?;
    let error = timeout(
        Duration::from_secs(/*secs*/ 15),
        app_server.read_stream_until_error_message(RequestId::Integer(excess_id)),
    )
    .await??;

    assert_eq!(error.error.code, -32001);

    let unavailable =
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    for (request_id, mut stream) in pending {
        stream.write_all(unavailable).await?;
        stream.shutdown().await?;
        let error = timeout(
            Duration::from_secs(/*secs*/ 15),
            app_server.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32603);
        assert!(error.error.message.contains("failed to upload feedback"));
    }

    let request_id = app_server
        .send_raw_request(
            "feedback/upload",
            Some(json!({ "classification": "bug", "includeLogs": false })),
        )
        .await?;
    let (mut stream, _) = timeout(Duration::from_secs(/*secs*/ 15), proxy.accept()).await??;
    stream.write_all(unavailable).await?;
    stream.shutdown().await?;
    let error = timeout(
        Duration::from_secs(/*secs*/ 15),
        app_server.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(error.error.code, -32603);
    Ok(())
}
