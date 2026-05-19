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
//!
//! Phase 3 (Plan 08): each connection ALSO spawns a `bcast_forwarder`
//! task that subscribes to `state.ws_broadcast` and forwards every
//! server-owned frame (`KernelChanged`, `ConfigUpdate`, `ConfigInvalid`,
//! …) into the same per-connection mpsc that the writer drains. So a
//! connected `/ws` peer ends up with three tokio tasks per connection:
//! the reader loop (this fn), the writer (Phase 2), and the broadcast
//! forwarder (Phase 3).

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
use tokio::sync::{broadcast::error::RecvError, mpsc};

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
    // Three-task architecture (Plan 03-08):
    //   1. Reader loop (this fn body) — drains inbound WS frames.
    //   2. Writer task — drains the per-conn mpsc into the WS sink.
    //   3. Broadcast forwarder task — subscribes to `state.ws_broadcast`
    //      and forwards every server-owned frame into the same mpsc.
    //
    // Pitfall #3 (03-RESEARCH.md): broadcast sends with zero receivers
    // are silently dropped by tokio's broadcast channel. That is
    // acceptable here because `/api/config` (HTTP) is the source-of-truth
    // fallback the browser fetches on connect, so a client that misses a
    // pre-connect `ConfigUpdate` still recovers via that GET.
    tracing::info!("ws connection opened");
    let (mut sink, mut stream) = socket.split();

    // Bounded at 32 per 02-RESEARCH.md Security Domain "WS frame flooding"
    // mitigation (T-02-15). Producers `send().await`, so when the writer
    // task falls behind the upstream naturally back-pressures.
    let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

    // Subscribe to the broadcast channel BEFORE sending Hello so that any
    // frame published between Hello and the start of the reader loop is
    // still captured by THIS connection (T-03-08-03). Subscribing is
    // cheap and only registers a per-receiver back-buffer; no allocation
    // happens until the first `send` after subscribe.
    let mut bcast_rx = state.ws_broadcast.subscribe();
    let tx_for_bcast = tx.clone();

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

    // Broadcast forwarder task: drain `state.ws_broadcast` and feed the
    // per-connection mpsc. `Lagged` is logged and the loop CONTINUES so
    // that a momentarily-slow writer task does not permanently silence
    // server-pushed frames for this client (T-03-08-01 — `skipped` is a
    // u64 count, never user-controlled bytes, so the warn line has no
    // amplification surface). `Closed` should be unreachable in the
    // current AppState lifecycle (the broadcast Sender lives as long as
    // the process) but the path is handled for completeness.
    let bcast_forwarder = tokio::spawn(async move {
        loop {
            match bcast_rx.recv().await {
                Ok(msg) => {
                    if tx_for_bcast.send(msg).await.is_err() {
                        // The per-connection mpsc was dropped (writer
                        // gone / connection closing). Exit cleanly.
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "ws broadcast receiver lagged");
                }
                Err(RecvError::Closed) => break,
            }
        }
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
                        // WR-02-01: truncate the payload before logging.
                        // axum 0.8's default WS frame size is multi-MB; a
                        // misbehaving client could amplify junk frames
                        // into a self-inflicted log/journald DoS on the
                        // loopback `--host 127.0.0.1` default (and worse
                        // if the operator opens `--host 0.0.0.0`). The
                        // event level stays at warn (operator-visible
                        // signal); the payload sample is capped at 256
                        // bytes which is plenty to disambiguate "client
                        // sent malformed JSON" from "client used a
                        // future protocol version".
                        tracing::warn!(
                            error = %e,
                            payload = %truncate_for_log(text.as_str(), 256),
                            "bad WsMessage"
                        );
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
    // Fire-and-forget cleanup of the broadcast forwarder. The forwarder
    // would also exit naturally when its `tx_for_bcast.send()` errors
    // (after the writer dropped `rx`), but explicit abort guarantees no
    // straggler iteration if the broadcast channel is busy. Acceptable
    // per T-03-08-05: the task may write to an already-dropped mpsc
    // (will Err and break) or be parked in `recv()` (abort interrupts
    // cleanly).
    bcast_forwarder.abort();
    tracing::info!("ws connection closed");
}

/// WR-02-01: cap an arbitrary client-supplied string before logging it.
///
/// Returns the original `s` when it fits within `max` bytes; otherwise
/// returns a UTF-8-safe prefix followed by an ellipsis and a "(truncated,
/// N bytes total)" tail. Truncation snaps backward to the nearest UTF-8
/// scalar boundary so we never split a codepoint mid-byte (which would
/// corrupt the log line for tools that parse it as JSON or UTF-8).
fn truncate_for_log(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…(truncated, {} bytes total)", &s[..cut], s.len())
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
        WsMessage::State { .. }
        | WsMessage::Hello { .. }
        | WsMessage::KernelChanged { .. }
        | WsMessage::ConfigUpdate { .. }
        | WsMessage::ConfigInvalid { .. } => {
            // Protocol error — these are server-owned message kinds.
            // Per CONTEXT.md `<deferred>` recovery posture, we log and
            // keep the connection up instead of disconnecting.
            tracing::warn!("client sent server-owned message kind");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_log;

    #[test]
    fn truncate_for_log_passthrough_under_limit() {
        assert_eq!(truncate_for_log("hello", 256), "hello");
        assert_eq!(truncate_for_log("", 256), "");
    }

    #[test]
    fn truncate_for_log_clips_oversize_ascii() {
        let big = "a".repeat(1024);
        let out = truncate_for_log(&big, 16);
        assert!(out.starts_with(&"a".repeat(16)));
        assert!(out.contains("truncated"));
        assert!(out.contains("1024 bytes total"));
    }

    #[test]
    fn truncate_for_log_respects_utf8_boundary() {
        // "é" is two bytes (0xc3 0xa9). With max=1 we must not split the
        // codepoint; the prefix length should fall back to 0.
        let s = "é".repeat(64); // 128 bytes
        let out = truncate_for_log(&s, 1);
        // The "(truncated, N bytes total)" tail is appended after the
        // safely-clipped prefix. The prefix must be valid UTF-8.
        assert!(out.contains("truncated"));
        assert!(out.contains("128 bytes total"));
        // The prefix between "" and "…" should be either empty or one
        // full "é" — never the lone 0xc3 byte. Verify the whole thing
        // is still valid UTF-8 (trivially true since it's a `String`,
        // but checking by re-encoding it via String::from_utf8 makes
        // the intent explicit).
        assert!(String::from_utf8(out.into_bytes()).is_ok());
    }
}
