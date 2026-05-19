//! WS-01 + WS-04 server-side integration tests for `/ws`.
//!
//! Tests:
//! 1. `ws_handshake_emits_hello` — server's first frame is
//!    `Hello { version: CARGO_PKG_VERSION }`.
//! 2. `ws_client_serial_in_is_logged_not_echoed` — `SerialIn` from the
//!    client is accepted (no panic, no premature close); Phase 2 logs
//!    rather than reacting.
//! 3. `ws_upgrade_response_carries_coop_coep` — Pitfall #4 regression:
//!    COOP `same-origin` and COEP `require-corp` survive on a GET to
//!    `/ws` (no Upgrade header).
//! 4. `ws_handshake_rejects_foreign_origin` — CR-02 regression: a
//!    handshake carrying `Origin: http://evil.example` is rejected
//!    with 403 instead of being upgraded.
//! 5. `ws_handshake_rejects_missing_origin` — CR-02 regression: a
//!    handshake with no `Origin` header is rejected with 403.

mod common;

use bootroom_core::WsMessage;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::header::ORIGIN,
};

/// CR-02 helper: build a `ws://` upgrade request that carries an
/// `Origin: http://<host>:<port>` header derived from `base_url` so the
/// server-side origin gate accepts the handshake.
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

#[tokio::test]
async fn ws_handshake_emits_hello() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;

    let (mut socket, _resp) = tokio_tungstenite::connect_async(ws_request(&server.base_url))
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

    let (mut socket, _resp) = tokio_tungstenite::connect_async(ws_request(&server.base_url))
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

/// CR-02 regression: a cross-origin WS handshake (browser tab on a
/// different origin attempts to attach to bootroom's loopback `/ws`)
/// must be rejected with 403 instead of being upgraded. Without this
/// gate, any web page the operator visits in the same browser can
/// subscribe to every server-pushed frame and inject Launch/Reset.
#[tokio::test]
async fn ws_handshake_rejects_foreign_origin() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;

    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";
    let mut req = ws_url
        .into_client_request()
        .expect("build WS request");
    req.headers_mut().insert(
        ORIGIN,
        "http://evil.example".parse().expect("origin header"),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("foreign-origin WS upgrade must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("403"),
        "expected HTTP 403 in tungstenite error, got: {msg}"
    );
}

/// CR-02 regression: a handshake with no `Origin` header at all must
/// be rejected with 403. Legitimate browsers always send `Origin` on
/// WS handshakes; absence is either a forged non-browser client or a
/// misconfigured tool, and accepting it would defeat the foreign-
/// origin check (an attacker could just strip the header).
#[tokio::test]
async fn ws_handshake_rejects_missing_origin() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;

    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";
    // `into_client_request` produces a request with NO `Origin` header
    // by default (tokio-tungstenite only adds `Sec-WebSocket-*`
    // handshake headers automatically). Leave the request untouched.
    let req = ws_url.into_client_request().expect("build WS request");

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("missing-origin WS upgrade must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("403"),
        "expected HTTP 403 in tungstenite error, got: {msg}"
    );
}
