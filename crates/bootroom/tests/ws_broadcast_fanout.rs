//! Plan 03-08: server -> WS subscriber fan-out integration tests.
//!
//! Verifies that the per-connection broadcast forwarder added to
//! `handle_socket` correctly delivers every `WsMessage` published via
//! `state.ws_broadcast.send(...)` to every connected `/ws` client.
//!
//! Tests:
//! 1. `single_client_receives_kernel_changed` — one client, one broadcast,
//!    asserts the JSON frame round-trips through the WS framing intact.
//! 2. `two_clients_both_receive_one_send` — fan-out: a single
//!    `broadcast::Sender::send` reaches BOTH connected clients.
//! 3. `client_misses_broadcasts_before_connect` — Pitfall #3
//!    confirmation: zero-receiver `broadcast::Sender::send` is silently
//!    dropped, so a late-joining client does NOT see pre-connect frames.
//!    This is the contract that motivates the `/api/config` HTTP fetch
//!    on connect (last-known-good fallback).
//! 4. `config_invalid_frame_round_trips` — `ConfigInvalid { line, col }`
//!    survives the WS hop and re-deserializes as the exact same enum.
//! 5. `lagged_receiver_logged_and_continues` — burst 20 frames at the
//!    16-capacity broadcast channel; expect `Lagged` to fire on the
//!    forwarder's receiver. The forwarder must NOT die — proved by the
//!    21st frame (a distinct `Launch`) eventually reaching the client
//!    after it resumes reading.

mod common;

use bootroom_core::WsMessage;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::header::ORIGIN,
};

/// CR-02: build a `ws://` upgrade request carrying an `Origin` header
/// derived from the test server's `base_url`, so the server-side origin
/// gate accepts the handshake.
fn ws_request(base_url: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let ws_url = base_url.replace("http://", "ws://") + "/ws";
    let origin = base_url.to_owned();
    let mut req = ws_url.into_client_request().expect("build WS request");
    req.headers_mut().insert(
        ORIGIN,
        origin.parse().expect("origin parses as HeaderValue"),
    );
    req
}

/// Open a WS client and consume the initial `Hello` frame so subsequent
/// reads start at the first server-pushed payload. Returns the live socket.
async fn connect_and_swallow_hello(
    base_url: &str,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let (mut socket, _resp) = tokio_tungstenite::connect_async(ws_request(base_url))
        .await
        .expect("ws connect");
    let hello = timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("hello timeout")
        .expect("server closed before hello")
        .expect("ws recv error on hello");
    match hello {
        Message::Text(t) => {
            let parsed: WsMessage =
                serde_json::from_str(t.as_str()).expect("hello not a WsMessage");
            assert!(
                matches!(parsed, WsMessage::Hello { .. }),
                "first frame should be Hello, got: {parsed:?}"
            );
        }
        other => panic!("expected Text greeting, got: {other:?}"),
    }
    socket
}

/// Receive the next `Text` frame and parse it as a `WsMessage`.
async fn next_ws_message<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    wait: Duration,
) -> WsMessage
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let frame = timeout(wait, socket.next())
        .await
        .expect("ws recv timeout")
        .expect("ws stream ended")
        .expect("ws recv error");
    match frame {
        Message::Text(t) => {
            serde_json::from_str(t.as_str()).expect("frame is not a WsMessage")
        }
        other => panic!("expected Text frame, got: {other:?}"),
    }
}

#[tokio::test]
async fn single_client_receives_kernel_changed() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let (server, state) =
        common::spawn_with_broadcast_handle(kernel.path().to_path_buf(), None).await;
    let mut socket = connect_and_swallow_hello(&server.base_url).await;

    // Give the broadcast forwarder a brief moment to actually subscribe()
    // before we publish. (handle_socket calls .subscribe() synchronously
    // BEFORE awaiting Hello-send, so by the time we observed Hello above
    // the subscriber is guaranteed registered — but tokio task scheduling
    // could in theory delay the spawn of the forwarder; a tiny sleep
    // makes the test robust against that ordering without papering over
    // a real bug.)
    tokio::time::sleep(Duration::from_millis(20)).await;

    let frame = WsMessage::KernelChanged {
        ok: true,
        mtime: 1_715_000_000,
        size: 64,
        sha256_prefix: "abc123def456".into(),
        reason: None,
    };
    state
        .ws_broadcast
        .send(frame.clone())
        .expect("broadcast send (at least 1 receiver expected)");

    let got = next_ws_message(&mut socket, Duration::from_secs(2)).await;
    match got {
        WsMessage::KernelChanged {
            ok,
            mtime,
            size,
            sha256_prefix,
            reason,
        } => {
            assert!(ok);
            assert_eq!(mtime, 1_715_000_000);
            assert_eq!(size, 64);
            assert_eq!(sha256_prefix, "abc123def456");
            assert_eq!(reason, None);
        }
        other => panic!("expected KernelChanged, got: {other:?}"),
    }
}

#[tokio::test]
async fn two_clients_both_receive_one_send() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let (server, state) =
        common::spawn_with_broadcast_handle(kernel.path().to_path_buf(), None).await;
    let mut a = connect_and_swallow_hello(&server.base_url).await;
    let mut b = connect_and_swallow_hello(&server.base_url).await;

    tokio::time::sleep(Duration::from_millis(20)).await;

    let cfg = serde_json::json!({
        "schema_version": 1,
        "actions": [],
        "scenarios": [],
    });
    state
        .ws_broadcast
        .send(WsMessage::ConfigUpdate { config: cfg.clone() })
        .expect("broadcast send");

    let got_a = next_ws_message(&mut a, Duration::from_secs(2)).await;
    let got_b = next_ws_message(&mut b, Duration::from_secs(2)).await;

    for (label, msg) in [("A", got_a), ("B", got_b)] {
        match msg {
            WsMessage::ConfigUpdate { config } => {
                assert_eq!(config, cfg, "client {label} ConfigUpdate config mismatch");
            }
            other => panic!("client {label} got unexpected frame: {other:?}"),
        }
    }
}

#[tokio::test]
async fn client_misses_broadcasts_before_connect() {
    // Pitfall #3: tokio broadcast channels DROP `send` when no receivers
    // are subscribed. A client that connects AFTER the broadcast must
    // not observe it — the documented recovery is fetching `/api/config`
    // over HTTP on connect.
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let (server, state) =
        common::spawn_with_broadcast_handle(kernel.path().to_path_buf(), None).await;
    // Broadcast with no subscribers: should return Err (no receivers),
    // confirming the message is dropped (not buffered).
    let send_result = state.ws_broadcast.send(WsMessage::KernelChanged {
        ok: true,
        mtime: 1_715_000_000,
        size: 64,
        sha256_prefix: "deadbeefcafe".into(),
        reason: None,
    });
    assert!(
        send_result.is_err(),
        "broadcast with zero receivers must return Err (got Ok with receivers={:?})",
        send_result.as_ref().ok()
    );

    // Now connect; the client should ONLY see Hello, never the prior
    // KernelChanged frame.
    let mut socket = connect_and_swallow_hello(&server.base_url).await;

    // Wait briefly to confirm no late delivery arrives.
    let next = timeout(Duration::from_millis(300), socket.next()).await;
    assert!(
        next.is_err(),
        "client unexpectedly received a frame after late-join: {next:?}"
    );
}

#[tokio::test]
async fn config_invalid_frame_round_trips() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let (server, state) =
        common::spawn_with_broadcast_handle(kernel.path().to_path_buf(), None).await;
    let mut socket = connect_and_swallow_hello(&server.base_url).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let frame = WsMessage::ConfigInvalid {
        error: "unknown field 'lable'".into(),
        line: Some(12),
        col: Some(1),
    };
    state
        .ws_broadcast
        .send(frame.clone())
        .expect("broadcast send");

    let got = next_ws_message(&mut socket, Duration::from_secs(2)).await;
    match got {
        WsMessage::ConfigInvalid { error, line, col } => {
            assert_eq!(error, "unknown field 'lable'");
            assert_eq!(line, Some(12));
            assert_eq!(col, Some(1));
        }
        other => panic!("expected ConfigInvalid, got: {other:?}"),
    }
}

#[tokio::test]
async fn lagged_receiver_logged_and_continues() {
    // Hardest test: prove that when the broadcast receiver inside the
    // bcast_forwarder task lags (channel capacity = 16, we publish 20
    // frames in a tight loop without the client reading), the forwarder
    // does NOT die. Proof: a 21st, distinct frame published after the
    // burst eventually reaches the client when it resumes reading.
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let (server, state) =
        common::spawn_with_broadcast_handle(kernel.path().to_path_buf(), None).await;
    let mut socket = connect_and_swallow_hello(&server.base_url).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    // 20-frame burst of KernelChanged. The broadcast channel itself has
    // capacity 16 (per AppState::WS_BROADCAST_CAPACITY); the per-conn
    // mpsc behind the forwarder has capacity 32. We rely on the fact
    // that the client is NOT calling `socket.next()` during the burst,
    // so the writer task stalls -> the forwarder's `tx_for_bcast.send`
    // back-pressures -> the broadcast receiver inside the forwarder
    // falls behind and `Lagged` fires.
    for i in 0..20u32 {
        // send returns Err when no receivers — receivers exist (this
        // connection's bcast_forwarder), so success is expected. We
        // ignore the count for back-pressure interpretation here; the
        // important thing is the burst rate exceeds the forwarder's
        // drain rate while the client doesn't read.
        let _ = state.ws_broadcast.send(WsMessage::KernelChanged {
            ok: true,
            mtime: i64::from(i),
            size: u64::from(i),
            sha256_prefix: format!("burst{i:08x}"),
            reason: None,
        });
    }

    // 21st frame: a `Launch` is structurally distinct from
    // KernelChanged (different `type` tag) and easy to identify when it
    // arrives.
    state
        .ws_broadcast
        .send(WsMessage::Launch)
        .expect("21st broadcast send");

    // Now resume reading. We expect to see SOME frames from the burst
    // (the ones that survived Lagged-drop) and eventually the Launch.
    // Allow 3s window for the forwarder to drain.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut saw_launch = false;
    while tokio::time::Instant::now() < deadline {
        let next = timeout(Duration::from_millis(500), socket.next()).await;
        let Ok(Some(Ok(Message::Text(t)))) = next else {
            // Either timed out this round, or stream ended / errored —
            // keep looping until the outer deadline.
            continue;
        };
        let parsed: WsMessage = match serde_json::from_str(t.as_str()) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if matches!(parsed, WsMessage::Launch) {
            saw_launch = true;
            break;
        }
        // Otherwise: a KernelChanged from the burst. Keep draining.
    }

    assert!(
        saw_launch,
        "forwarder appears to have died on Lagged — never saw the 21st Launch frame"
    );
}
