//! 04-05 — End-to-end WS roundtrip pinning the `ScenarioResult` handoff.
//!
//! 1. Boot bootroom's full axum router on `127.0.0.1:0`.
//! 2. Install an oneshot via `state.install_scenario_oneshot()`.
//! 3. Connect a tungstenite WS client; send `ScenarioResult` JSON.
//! 4. Assert the oneshot receiver yields the same `WsMessage` variant.
//!
//! Pins: (a) same router as serve mode (RUN-03), (b) take-once
//! semantics (04-04), (c) WS reader path correctly identifies the new
//! variant and forwards it (04-05).

use bootroom::{AppState, build_router};
use bootroom_core::WsMessage;
use futures_util::{SinkExt, StreamExt};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn scenario_result_frame_lands_on_oneshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let bound = listener.local_addr().expect("local_addr");

    // Build state with the bound address in `allowed_origins` so the
    // WS upgrade passes the CR-02 origin gate.
    let mut state = AppState::new_for_test(PathBuf::from("/tmp/Image"), None);
    state.allowed_origins = vec![format!("http://{bound}")];
    let state = Arc::new(state);

    // Install the oneshot BEFORE the client connects. The receiver is
    // captured here and awaited at the bottom of the test.
    let rx = state.install_scenario_oneshot().await;

    // Spawn the axum server on the pre-bound listener so we know the
    // exact port for the Origin header.
    let app = build_router(state.clone());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // Build the WS client request with an Origin header matching the
    // bound address (CR-02 gate). Phase 3 `ws_broadcast_fanout` uses
    // the same incantation; lifted verbatim.
    let url = format!("ws://{bound}/ws");
    let mut req = url.into_client_request().expect("req");
    req.headers_mut().insert(
        "Origin",
        format!("http://{bound}").parse().expect("origin"),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("connect");

    // Drain the server's Hello frame so we don't conflate it with a
    // verdict.
    let hello = socket.next().await.expect("hello").expect("ok");
    assert!(
        matches!(hello, Message::Text(ref t) if t.as_str().contains("Hello")),
        "expected Hello greeting, got: {hello:?}"
    );

    // Send the ScenarioResult.
    let verdict = WsMessage::ScenarioResult {
        verdict: "pass".into(),
        scenario: "boot_smoke".into(),
        started_at: "2026-05-19T14:32:01.123Z".into(),
        ended_at: "2026-05-19T14:32:03.311Z".into(),
        actions: serde_json::json!([{"label":"reboot","verdict":"pass"}]),
        transcript: serde_json::json!([
            {"ts":"2026-05-19T14:32:01.123Z","type":"scenario_start"}
        ]),
        error: None,
    };
    let json = serde_json::to_string(&verdict).expect("serialize");
    socket
        .send(Message::Text(json.into()))
        .await
        .expect("send");

    // Await the oneshot with a generous outer timeout. 5 s is plenty
    // for a loopback roundtrip.
    let delivered = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .expect("oneshot timed out")
        .expect("oneshot delivered");

    assert_eq!(
        delivered, verdict,
        "ScenarioResult round-trip must be byte-exact"
    );

    server.abort();
}
