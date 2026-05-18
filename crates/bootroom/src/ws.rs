//! `/ws` — tagged-JSON WebSocket endpoint (WS-01, WS-04 server side).
//!
//! Phase 2 is a pass-through observer: the server accepts client frames
//! and logs them. The only frame the server produces is the initial
//! `Hello { version }` greeting. Phase 4 will react to `SerialIn` /
//! `SerialOut` for the headless `bootroom run` driver — the mpsc + split
//! pattern keeps that future hook free of structural churn.
//!
//! Pattern: per 02-RESEARCH.md Pattern 1 — split socket into sink + stream,
//! spawn a writer task draining a bounded `tokio::sync::mpsc::channel`
//! (capacity 32, T-02-15 back-pressure mitigation), and dispatch frames
//! in a reader loop.

use crate::state::AppState;
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use bootroom_core::WsMessage;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

/// axum extractor entrypoint. Upgrades the connection and hands the
/// `WebSocket` to `handle_socket`. State extractor is wired so future
/// phases can inject per-connection logic without changing the route.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("ws connection opened");
    let (mut sink, mut stream) = socket.split();

    // Bounded at 32 per 02-RESEARCH.md Security Domain "WS frame flooding"
    // mitigation (T-02-15). Producers `send().await`, so when the writer
    // task falls behind the upstream naturally back-pressures.
    let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

    // Writer task: drain the channel; serialize each `WsMessage` to
    // JSON; emit as `Message::Text`. On send failure (peer gone) log and
    // continue draining so the channel doesn't fill and dead-lock the
    // reader's `tx.send().await` in `handle_wire`.
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if let Err(e) = sink.send(Message::Text(json.into())).await {
                        tracing::debug!(error = %e, "ws sink send failed; peer likely gone");
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "WsMessage serialize failed (bug)");
                }
            }
        }
        // Channel closed (tx dropped) — best-effort close the sink.
        let _ = sink.close().await;
    });

    // Initial greeting. Ignored result is intentional: if the channel is
    // already closed the reader loop below will exit on the first poll.
    let _ = tx
        .send(WsMessage::Hello {
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
        .await;

    // Reader loop: dispatch every inbound frame. Errors and Close break;
    // everything else continues so a misbehaving client cannot trivially
    // disconnect itself by sending unexpected frame kinds.
    while let Some(msg_res) = stream.next().await {
        match msg_res {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<WsMessage>(text.as_str()) {
                    Ok(wire) => handle_wire(wire, &tx, &state).await,
                    Err(e) => {
                        tracing::warn!(error = %e, payload = %text.as_str(), "bad WsMessage");
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                tracing::warn!("unexpected binary WS frame; protocol is JSON");
            }
            Ok(Message::Ping(_) | Message::Pong(_)) => {
                // axum auto-handles pings — nothing to do.
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::debug!(error = %e, "ws recv error; closing");
                break;
            }
        }
    }

    // Drop the sender so the writer task's `rx.recv()` returns None and
    // the task exits cleanly; then await its join handle (best-effort).
    drop(tx);
    let _ = writer.await;
    tracing::info!("ws connection closed");
}

// Async signature is kept forward-compatible: Phase 4's `Launch` / `Reset`
// handling will `tx.send(...).await` outbound frames. Allow the lint
// rather than churn the signature on the next phase.
#[allow(clippy::unused_async)]
async fn handle_wire(wire: WsMessage, _tx: &mpsc::Sender<WsMessage>, _state: &AppState) {
    match wire {
        WsMessage::SerialIn { data: _ } => {
            tracing::trace!("SerialIn frame received");
        }
        WsMessage::SerialOut { data: _ } => {
            tracing::trace!("SerialOut frame received");
        }
        WsMessage::Launch => {
            tracing::info!("client Launch");
        }
        WsMessage::Reset => {
            tracing::info!("client Reset");
        }
        WsMessage::State { .. } | WsMessage::Hello { .. } => {
            // Protocol error — these are server-owned message kinds.
            // Per CONTEXT.md `<deferred>` recovery posture, we log and
            // keep the connection up instead of disconnecting.
            tracing::warn!("client sent server-owned message kind");
        }
    }
}
