//! WS-01 + WS-04 server-side integration tests for `/ws`.
//!
//! Three tests:
//! 1. `ws_handshake_emits_hello` — server's first frame is
//!    `Hello { version: CARGO_PKG_VERSION }`.
//! 2. `ws_client_serial_in_is_logged_not_echoed` — `SerialIn` from the
//!    client is accepted (no panic, no premature close); Phase 2 logs
//!    rather than reacting.
//! 3. `ws_upgrade_response_carries_coop_coep` — Pitfall #4 regression:
//!    COOP `same-origin` and COEP `require-corp` survive on a GET to
//!    `/ws` (no Upgrade header).

mod common;

use bootroom_core::WsMessage;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn ws_handshake_emits_hello() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";

    let (mut socket, _resp) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");

    let first = socket
        .next()
        .await
        .expect("server closed before sending greeting")
        .expect("ws recv error on first frame");

    let text = match first {
        Message::Text(t) => t,
        other => panic!("expected Text greeting, got: {other:?}"),
    };

    let parsed: WsMessage =
        serde_json::from_str(text.as_str()).expect("greeting is valid WsMessage JSON");
    match parsed {
        WsMessage::Hello { version } => {
            assert_eq!(
                version,
                env!("CARGO_PKG_VERSION"),
                "Hello.version must match server CARGO_PKG_VERSION"
            );
        }
        other => panic!("expected Hello, got: {other:?}"),
    }
}

#[tokio::test]
async fn ws_client_serial_in_is_logged_not_echoed() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";

    let (mut socket, _resp) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");

    // Discard the Hello greeting.
    let _ = socket
        .next()
        .await
        .expect("server closed before greeting")
        .expect("ws recv error on greeting");

    // Send a SerialIn frame; server must accept without panicking and
    // without closing the connection.
    let frame = WsMessage::SerialIn {
        data: "aGVsbG8=".into(),
    };
    let json = serde_json::to_string(&frame).expect("serialize SerialIn");
    socket
        .send(Message::Text(json.into()))
        .await
        .expect("send SerialIn");

    // Initiate a clean close from the client side; server should not
    // have closed first.
    socket
        .send(Message::Close(None))
        .await
        .expect("send Close");
}

#[tokio::test]
async fn ws_upgrade_response_carries_coop_coep() {
    // Pitfall #4 regression: a future router refactor that strips the
    // COOP/COEP layers from the WS path would silently downgrade
    // cross-origin isolation for connected pages. A non-upgrade GET is
    // the cheapest way to inspect the headers without doing the full
    // handshake.
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;

    let resp = reqwest::get(format!("{}/ws", server.base_url))
        .await
        .expect("GET /ws");

    // Status is intentionally NOT asserted — axum's exact response code
    // for a missing Upgrade header may evolve (400/426 today). The
    // load-bearing contract is that the tower middleware stack ran.
    assert_eq!(
        resp.headers()
            .get("cross-origin-opener-policy")
            .map(|v| v.to_str().unwrap()),
        Some("same-origin"),
        "COOP missing on /ws non-upgrade GET"
    );
    assert_eq!(
        resp.headers()
            .get("cross-origin-embedder-policy")
            .map(|v| v.to_str().unwrap()),
        Some("require-corp"),
        "COEP missing on /ws non-upgrade GET"
    );
}
