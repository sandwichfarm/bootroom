---
phase: 2
name: WebSocket + Live Serial
date: 2026-05-18
mode: discuss
---

# Phase 2 Discussion Log

## Areas Presented

1. /ws message protocol shape
2. Launch / Reset semantics
3. Keyboard input + write funnel
4. Browser auto-open + status pill source

User selected: all four.

## Area 1 — /ws message protocol

Selected: **Tagged JSON only**. Bytes inside SerialIn/SerialOut as base64. Binary framing rejected as premature optimization. `bootroom-core` defines the `WsMessage` enum that Phase 4 will reuse for headless `run`.

## Area 2 — Launch / Reset

Selected: **Both = full page reload**. Spike A's in-place reset path documented but deferred. Scrollback loss accepted; in-place reset revisited only when the cost actually bites.

## Area 3 — Write funnel

Selected: **Client-side funnel**. Removes Phase 1's `attachCustomKeyEventHandler` no-op. User typing flows at native speed (pacingMs=0); WS-injected SerialIn paces at configured rate (default 15ms). Single sender by construction → satisfies WS-02.

## Area 4 — Auto-open + status pill

Selected: **`open` crate, --no-open opt-out; status from emscripten + WS**. Status pill state machine:
- Idle: initial render
- Loading: xterm mounted, qemu init pending
- Running: onRuntimeInitialized + first SerialOut byte
- Halted: onExit/onAbort or server `State{Halted}` push

Server's WS `State` message is authoritative when connected; client falls back to local Module events before WS opens.

## Claude's Discretion

- Exact WS message field names (`type`, `data`, `state`) — JS+serde idiomatic choice
- Specific axum WebSocket handler shape — researcher/planner will detail
- Button placement in header (right of kinfo, left of pill) — UI-SPEC extension
- Test strategy for WS round-trip — tokio-tungstenite client driver in integration tests

## Deferred Ideas

(Captured in CONTEXT.md `<deferred>` section.)
