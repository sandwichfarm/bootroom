# Phase 2: WebSocket + Live Serial — Research

**Researched:** 2026-05-18
**Domain:** axum 0.8 WebSocket plumbing + xterm-pty input/output bridging + browser auto-open + serde-tagged WS protocol in `bootroom-core` + tokio-tungstenite-driven integration tests
**Confidence:** HIGH for the Rust WS stack and the vendored xterm-pty API (verified by inspecting the in-tree minified source); HIGH-MEDIUM for the funnel/single-writer design (cross-referenced against xterm-pty's `master.activate` listener subscription); HIGH for `open` crate and Wayland xdg-open behaviour (verified against local environment).

## Summary

Phase 2 turns Phase 1's passive viewer into an interactive harness. The load-bearing pieces are a `/ws` endpoint that relays serde-tagged JSON `WsMessage` frames defined once in `bootroom-core`, a browser-side write funnel that guarantees single-sender semantics (WS-02) over the xterm-pty `slave`, and an `Idle`/`Loading`/`Running`/`Halted` status pill the server can override via authoritative `State` messages. Browser auto-open ships with `--no-open`; both Launch and Reset reduce to a full page reload (Spike A's `module-fs-write` substitution already runs unconditionally inside `onRuntimeInitialized` on every load, so a reload IS the freshest-kernel relaunch).

Three pieces have meaningful subtlety the planner must address explicitly: (1) **xterm-pty's `master` addon auto-subscribes to `xterm.onData` inside `master.activate`** — without intervention, user keystrokes would reach `slave.write` BOTH via master AND via the funnel, defeating WS-02 by construction. The cleanest fix is keeping `attachCustomKeyEventHandler` and returning `false` for printable keys after enqueueing into the funnel (verified against the vendored `xterm-pty.js`). (2) **`slave.read()` returns `number[]`, NOT a `Uint8Array`** — base64-encoding for the wire and decoding on the way back both need explicit byte-array conversions. (3) **COOP/COEP must remain on the `/ws` HTTP 101 Switching Protocols upgrade response** — the existing `SetResponseHeaderLayer::overriding` wrapping the whole `Router` covers this for free because axum's `WebSocketUpgrade` extractor turns into a normal axum `Response`; no special handling needed, but a regression test should pin this guarantee against future router refactors.

**Primary recommendation:** Land `WsMessage` + `GuestState` in `bootroom-core` first (it's pure types, zero blast radius). Wire `/ws` with axum's `WebSocketUpgrade` extractor and `socket.split()` → an `mpsc::channel<WsMessage>` sender held in per-connection state. On the browser, build `web/funnel.js` as the single writer to `slave.write`, intercept keystrokes via `attachCustomKeyEventHandler` (return `false`, enqueue into funnel), and bind `slave.onReadable` → drain → emit batched `SerialOut` frames over WS. Use `open::that_detached` after `listener.local_addr()` succeeds, gated by `!--no-open`. Integration-test the round-trip with `tokio-tungstenite 0.29` (the version the official axum testing-websockets example pins as of 2026-05).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### /ws message protocol — tagged JSON only

Single serde-tagged enum, JSON-on-the-wire, base64 for byte payloads.

- Message enum lives in `bootroom-core` (Phase 4 reuses it for headless):
  ```rust
  #[derive(Serialize, Deserialize, Debug, Clone)]
  #[serde(tag = "type")]
  pub enum WsMessage {
      SerialIn  { data: String },          // base64-encoded bytes, host -> guest
      SerialOut { data: String },          // base64-encoded bytes, guest -> host (mirror)
      State     { state: GuestState },     // server's authoritative status pill state
      Launch,                              // client requests a reboot (page reload, here)
      Reset,                               // client requests guest reset (page reload, here)
      Hello     { version: String },       // server -> client on connect
  }
  #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
  pub enum GuestState { Idle, Loading, Running, Halted }
  ```
- Browser: bytes from `slave.onReadable()` get `btoa()`'d into `data`; incoming `SerialIn.data` gets `atob()`'d before being enqueued for the slave.
- Accept ~33% wire overhead for base64. Binary framing was rejected as premature optimization.
- Endpoint: `GET /ws` upgraded by axum's WebSocket extractor; one connection per page; server-side state is per-connection (no broadcast in Phase 2).

#### Launch / Reset — both = full page reload

- Browser re-instantiates qemu-wasm Worker fresh; re-fetches `/api/kernel/info` + `/kernel`; runs staticInit/preRun/main as on first load.
- `/ws` auto-reconnects after the reload (server is stateless per-connection).
- xterm scrollback resets across reload — accepted cost in Phase 2.
- In-place qemu reset deferred (Spike A's `module-fs-write` substitution already documented for future revisit).
- Both buttons send a `Launch` / `Reset` WS message *before* reloading, so the server can log the request.

#### Keyboard input + client-side write funnel

- Remove Phase 1's `xterm.attachCustomKeyEventHandler(() => false)` from `app.js`.
- Replace with a write-funnel module (`web/funnel.js`): exposes `enqueue(bytes, {pacingMs})`; drains via an async loop with configurable pacing (default 15ms between bytes for injected sequences, 0ms for user typing).
- xterm `onData` callback calls `funnel.enqueue(bytes, { pacingMs: 0 })` — user typing flows at native speed.
- WS-received `SerialIn` messages call `funnel.enqueue(decoded, { pacingMs: config.pacingMs })`.
- Funnel writes bytes to `slave.write(...)`. Single sender = no interleaving (satisfies WS-02 by construction).
- SerialOut mirror: xterm-pty's `slave.onReadable` → read all available → emit `SerialOut` WS frame (batched per readable burst, not per byte).
- Config knob: query string param `?pacing=20` overrides; default 15ms.

#### Browser auto-open + status pill source

- `open = "5"` workspace dep; `open::that_detached(url)` after `axum::serve` is listening; failure logs warning and continues.
- CLI flag: `--no-open` (bool).
- Status pill state machine:
  | State | Trigger |
  |---|---|
  | `Idle` | Initial render (before xterm + qemu init) |
  | `Loading` | After xterm mount, before `Module.onRuntimeInitialized` |
  | `Running` | `Module.onRuntimeInitialized` AND first SerialOut byte received |
  | `Halted` | `Module.onExit` / `Module.onAbort` OR server `State{state: Halted}` |
- When WS connects, server `State` is authoritative.
- `Hello { version }` on connect; version mismatch logs but proceeds (tighten Phase 6).

### Claude's Discretion

Implementation freedoms granted to the executor by the discuss phase:

- Per-connection `ConnState` representation (axum `Extension`, `WsState`, or per-task locals).
- Whether `master` xterm-pty addon stays loaded (this RESEARCH recommends YES — see "Pattern 3: Single-writer funnel without losing xterm rendering").
- Exact base64 helper choice (`base64` crate on the Rust side; native `btoa`/`atob` on the browser side — but they are UCS-2/Latin-1-only; see Pitfall #3).
- Whether to factor `.btn-mono` shared CSS class for the four new buttons.
- Naming of the write-funnel module (`funnel.js` recommended; not contractually fixed).
- Server-side pacing knob: discretion-area says "Server doesn't currently care about the pacing value (client-side concern)." So no pacing field on `WsMessage`.

### Deferred Ideas (OUT OF SCOPE)

- **In-place qemu reset** (no page reload) — Spike A scaffolding stays; revisit when scrollback loss bites.
- **TOML config + action buttons** — Phase 3.
- **Kernel-path watcher** — Phase 3.
- **Headless `bootroom run`** — Phase 4 (reuses WS protocol shipped here).
- **WS reconnect with exponential backoff** — naive retry only in Phase 2.
- **Multi-tab / concurrent WS connections** — out of scope; loopback single-user.
- **Authentication on `/ws`** — loopback-only; explicit non-goal in PROJECT.md.
- **Binary WS frames for serial bytes** — JSON+base64 chosen; revisit only if profiling justifies it.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SERV-06 | Server opens user's default browser to harness URL on start (suppressed by `--no-open`) | `open` crate v5.3.5 (Standard Stack); Pattern 4 documents `open::that_detached` semantics on Wayland; CLI section in this doc covers `--no-open` flag |
| UI-02 | Live serial console rendered via xterm.js, mounted on xterm-pty `slave` | Pattern 3 (single-writer funnel without losing xterm rendering); already wired in Phase 1 — Phase 2 doesn't regress |
| UI-03 | Serial console supports free-form keyboard input written through to the guest | Pattern 3 + Code Examples §2 (funnel.js skeleton); intercept via `attachCustomKeyEventHandler` returning `false` |
| UI-04 | Console has "clear" and "copy all" controls | UI-SPEC contracts already cover; this doc adds Pitfall #5 on `xterm.buffer.active.translateToString` semantics |
| UI-06 | Status pill shows guest state (Idle / Loading / Running / Halted) | `GuestState` enum in Standard Stack; Pattern 5 (status pill state machine + WS authority) |
| UI-08 | "Launch" button (re)boots guest with freshest kernel build | Spike A's `module-fs-write` substitution runs unconditionally on every load → `Launch = sendWs(Launch); location.reload()` |
| UI-09 | "Reset" button restarts guest with currently-loaded kernel | Identical to Launch in Phase 2 by user decision; label distinguishes intent for log observers |
| WS-01 | Single `/ws` endpoint relays `SerialIn` / `SerialOut` | Pattern 1 (axum 0.8 WebSocket handler) + Code Examples §1 |
| WS-02 | All host→guest writes funneled through single sender (no byte-interleaving) | Pattern 3 (single-writer funnel) — by construction; Pitfall #1 (master addon double-subscription) is the failure mode to defend against |
| WS-03 | Inter-character pacing (~10–20ms, configurable) on injected sequences | Code Examples §2 (funnel drain loop) + `pacingMs` per-enqueue parameter |
| WS-04 | Message protocol is serde-tagged JSON; types live in `bootroom-core` | Standard Stack ships `WsMessage` enum literally in `bootroom-core/src/lib.rs`; Pattern 2 covers tagged-enum representation |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| `WsMessage` / `GuestState` type definitions | Shared types (`bootroom-core`) | — | Pure types; reused by Phase 4 headless `run`. Zero I/O, zero tokio. |
| `/ws` WebSocket upgrade + per-connection lifecycle | API / Backend (axum 0.8) | — | axum's `WebSocketUpgrade` extractor; one socket per browser tab. |
| Per-connection sender funnel (mpsc → SplitSink) | API / Backend (tokio) | — | Server has no real fan-in in Phase 2 (no scenarios yet), but the mpsc layer is the well-known thread-safe write pattern for axum WebSocket. |
| Browser auto-open | API / Backend (`open` crate) | OS shell (`xdg-open` on Linux) | After bind, before serving. Best-effort; failure does not crash the server. |
| Status pill state machine | Browser / Client | API / Backend (`State` authority) | Local emscripten lifecycle events are the default truth; server `State` messages override when WS is connected. |
| Single-writer write funnel to `slave.write` | Browser / Client | — | The WS-02 guarantee is purely client-side: server can only ever send `SerialIn` frames; the funnel queues them in order with user keystrokes. |
| xterm `onData` interception (block default master forward) | Browser / Client | — | Crucial — without this, `master` addon forwards keys to `slave` outside the funnel. See Pitfall #1. |
| `SerialOut` mirror over WS | Browser / Client | API / Backend (receives + logs) | `slave.onReadable` → drain `slave.read()` → base64 → WS send. Server's role in Phase 2 is observation only. |
| Launch / Reset → page reload | Browser / Client | API / Backend (receives `Launch`/`Reset` for logging) | Both buttons emit a WS frame for observability, then `location.reload()`. |
| Clear / Copy terminal controls | Browser / Client | — | `xterm.clear()` and `navigator.clipboard.writeText`. No server interaction. |

## Standard Stack

### Core (Rust)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` | 0.8.9 `[VERIFIED: workspace Cargo.toml; ratified in 01-RESEARCH]` | HTTP + WebSocket | Already locked. Needs the `ws` feature added in `crates/bootroom/Cargo.toml`. |
| `axum` `ws` feature | (feature flag) `[CITED: docs.rs/axum/latest/axum/extract/ws/]` | Enables `WebSocketUpgrade` extractor + `Message` type | Must be added explicitly — Phase 1 does not enable it. |
| `tokio` | 1.52.3 `[VERIFIED: workspace Cargo.toml]` | Async runtime + `sync::mpsc` for the per-connection sender funnel | Already locked. The `sync` feature is already enabled. |
| `futures-util` | 0.3.32 `[VERIFIED: crates.io 2026-05-18; latest stable]` | `StreamExt`, `SinkExt`, `SplitSink`, `SplitStream` for the WS handler split | Required for `socket.split()` pattern; the official `axum/examples/testing-websockets` example uses `futures-util = "0.3"`. |
| `tokio-tungstenite` | 0.29.0 `[VERIFIED: crates.io 2026-05-18; matches axum's official testing-websockets example pin]` | WS client for integration tests | Dev-dep only. The official axum 0.8 testing-websockets example pins `tokio-tungstenite = "0.29"` (verified by fetching the example's Cargo.toml). |
| `serde` / `serde_json` | 1.0.228 / 1.x `[VERIFIED: workspace Cargo.toml]` | Serialize/deserialize the tagged WsMessage enum | Already locked. `#[serde(tag = "type")]` produces the externally-untagged → internally-tagged JSON representation the protocol decision calls for. |
| `bootroom-core` | (workspace member) | Home of `WsMessage` + `GuestState` | Already wired; Phase 2 puts content into the empty lib.rs. |
| `open` | 5.3.5 `[VERIFIED: crates.io 2026-05-18; updated 2026-05-11]` | Browser auto-open on `serve` | Cross-platform; on Linux delegates to `xdg-open` → `gio open` → `gnome-open` → `kde-open` → `wslview`. `open::that_detached` is the right call for fire-and-forget. |
| `base64` | 0.22.1 `[VERIFIED: crates.io 2026-05-18; stable since 2024-04-30]` | Encode/decode `SerialIn` / `SerialOut` byte payloads on the Rust side | Server doesn't currently need to *decode* SerialIn (it's a pass-through observer in Phase 2), but Phase 4 will. Adding it now keeps the protocol type definitions complete. Use `base64::engine::general_purpose::STANDARD` — standard alphabet, padded. |
| `anyhow` | 1.0.102 | Error wrapping for the WS handler | Already locked. WS errors get `.context()`-wrapped before logging. |
| `tracing` | 0.1.44 | Per-connection logging | Already locked. WS handler should `tracing::info_span!` per connection. |

### Browser (vendored, already shipped)

| File | Version | Phase 2 use |
|------|---------|-------------|
| `xterm.js` | 5.3.0 `[VERIFIED: web/vendor/VERSIONS.md]` | `attachCustomKeyEventHandler` returning `false` to suppress default `onData` forwarding (Pattern 3) |
| `xterm-pty.js` | 0.12.0 `[VERIFIED: web/vendor/VERSIONS.md; inspected vendored source]` | `slave.write(bytes)`, `slave.read()`, `slave.onReadable()` — verified against the minified source the project ships |

**No new vendored deps in Phase 2.** Native browser primitives cover the rest: `WebSocket`, `btoa`/`atob`, `navigator.clipboard`, `location.reload`.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio-tungstenite` for integration tests | `axum-test` 18.x with WebSocket extension | `axum-test` is high-level (no manual TCP bind), but the official axum example uses `tokio-tungstenite` against `axum::serve` — and the existing `TestServer` harness in `tests/common/mod.rs` is built on `axum::serve`. Use `tokio-tungstenite` to keep the test pattern uniform. |
| `open` crate | `opener` crate | `open` is more widely cited (used by `cargo`'s docs.rs PR), more frequently updated. `opener` is functionally similar. No reason to deviate. |
| `base64` crate | `data-encoding`, `bs58` | `base64` 0.22 is the de-facto standard. Don't introduce alternatives. |
| Tagged JSON | MessagePack / CBOR / bincode | Decided against in `<decisions>`. Reaffirming: JSON+base64 is human-debuggable in the DevTools WS pane, which matters when scenarios fail. |
| `mpsc` between accept-task and write-task | `tokio::sync::Mutex<WebSocket>` over the whole socket | Mutex serializes reads behind writes — kills the duplex. mpsc + split is the documented axum pattern. |
| Per-connection broadcast | `tokio::sync::broadcast::Sender` on AppState | Broadcast is for fan-out (Phase 4 may need it if multiple tabs observe scenarios). Phase 2 has one connection per page; per-connection mpsc is the correct primitive. |
| Server-side write funnel | Browser-side funnel (`web/funnel.js`) | User locked client-side funnel in `<decisions>`. Rationale (re-verified): the funnel needs to merge user keystrokes (which never traverse `/ws` in Phase 2) with WS-arriving scenario bytes — only the browser sees both streams. |

### Cargo.toml deltas

Workspace `[workspace.dependencies]` additions:

```toml
futures-util = "0.3.32"
open = "5"
base64 = "0.22"
tokio-tungstenite = "0.29"   # dev-dep only (gated under [dev-dependencies] in bootroom)
```

`crates/bootroom/Cargo.toml`:

```toml
[dependencies]
axum = { workspace = true, features = ["ws"] }   # add "ws" feature
futures-util = { workspace = true }
open = { workspace = true }
base64 = { workspace = true }

[dev-dependencies]
tokio-tungstenite = { workspace = true }
futures-util = { workspace = true }              # already pulled by deps; explicit for tests
```

`crates/bootroom-core/Cargo.toml`:

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }   # optional — only if helpers ship serializers
```

**Version verification (2026-05-18):**
- `open` → 5.3.5 (published 2026-05-11) — current
- `futures-util` → 0.3.32 (published 2026-02-15) — current
- `tokio-tungstenite` → 0.29.0 (published 2026-03-17) — current; matches axum 0.8 official example
- `base64` → 0.22.1 (published 2024-04-30) — stable; no 0.23 yet

## Architecture Patterns

### System Architecture Diagram

```
                    Phase 2 dataflow (deltas from Phase 1)

  ┌─────────────────────────────────────────────────────────────────────────┐
  │                       bootroom (single process)                          │
  │                                                                          │
  │   ┌──────────────┐    ┌────────────────────────────────────────────┐     │
  │   │ clap         │───▶│ run(args)                                   │     │
  │   │ ServeArgs    │    │   - validate kernel                         │     │
  │   │  +--no-open  │    │   - build router (incl. NEW /ws route)      │     │
  │   │  +--pacing   │    │   - bind, print startup line                │     │
  │   │   (optional) │    │   - if !args.no_open: open::that_detached   │     │
  │   └──────────────┘    │   - axum::serve                             │     │
  │                       └────────────────────┬───────────────────────┘     │
  │                                            │                              │
  │   ┌────────────────────────────────────────▼───────────────────────┐     │
  │   │ Router (Phase 1 routes preserved)                              │     │
  │   │   .route("/ws", any(ws_handler))   ← NEW                       │     │
  │   │   .layer(coop_layer())                                         │     │
  │   │   .layer(coep_layer())   ← still applies to WS 101 upgrade     │     │
  │   └─────────────────────┬──────────────────────────────────────────┘     │
  │                         │                                                 │
  │                         │ Upgrade: websocket                              │
  │              ┌──────────▼─────────────────────────────────────────┐      │
  │              │ ws_handler(WebSocketUpgrade) → ws.on_upgrade(...)  │      │
  │              │   - socket.split() → (sink, stream)                │      │
  │              │   - per-conn state: tokio::mpsc::channel::<Wire>   │      │
  │              │   - spawn writer task: rx.recv() → sink.send       │      │
  │              │   - send Hello{version}                            │      │
  │              │   - loop stream.next():                            │      │
  │              │       SerialIn  → tracing::trace + (Phase 2: NOP)  │      │
  │              │       SerialOut → tracing::trace (observability)   │      │
  │              │       Launch/Reset → tracing::info!                │      │
  │              │       Close     → break                            │      │
  │              └────────────────────────────────────────────────────┘      │
  │                                                                          │
  └────────────────────────────────┬─────────────────────────────────────────┘
                                   │ TCP 127.0.0.1:8765
                                   ▼
   ┌────────────────────────────────────────────────────────────────────┐
   │ Browser tab (Phase 1 shell, Phase 2 deltas inline)                 │
   │                                                                    │
   │   ┌─ index.html (extended) ──────────────────────────────────┐     │
   │   │  <button id="btn-launch">LAUNCH</button>                 │     │
   │   │  <button id="btn-reset">RESET</button>                   │     │
   │   │  <div class="term-ctrls"><button>CLEAR</button>          │     │
   │   │                          <button>COPY</button></div>     │     │
   │   └──────────────────────────────────────────────────────────┘     │
   │                                                                    │
   │   ┌─ app.js (extended) ──────────────────────────────────────┐     │
   │   │  import { Funnel } from './funnel.js'                    │     │
   │   │  const funnel = new Funnel(slave)                        │     │
   │   │                                                          │     │
   │   │  // REMOVE Phase-1 attachCustomKeyEventHandler(()=>false)│     │
   │   │  // REPLACE with intercepting handler:                   │     │
   │   │  xterm.attachCustomKeyEventHandler(evt => {              │     │
   │   │    if (evt.type !== 'keydown') return true;              │     │
   │   │    const bytes = keyEventToBytes(evt);                   │     │
   │   │    if (bytes) funnel.enqueue(bytes, {pacingMs: 0});      │     │
   │   │    return false;  // suppress default onData → master    │     │
   │   │  });                                                     │     │
   │   │                                                          │     │
   │   │  // SerialOut mirror                                     │     │
   │   │  slave.onReadable(() => {                                │     │
   │   │    const bytes = slave.read();           // number[]     │     │
   │   │    if (bytes.length === 0) return;                       │     │
   │   │    ws.send(JSON.stringify({                              │     │
   │   │      type: 'SerialOut',                                  │     │
   │   │      data: bytesToB64(bytes)                             │     │
   │   │    }));                                                  │     │
   │   │    if (firstSerialOut && runtimeInitialized)             │     │
   │   │      setPill('RUNNING');                                 │     │
   │   │  });                                                     │     │
   │   │                                                          │     │
   │   │  // WS lifecycle                                         │     │
   │   │  const ws = new WebSocket(`ws://${location.host}/ws`);   │     │
   │   │  ws.onmessage = ev => handleWsFrame(JSON.parse(ev.data));│     │
   │   │  ws.onclose = () => scheduleReconnect();                 │     │
   │   └──────────────────────────────────────────────────────────┘     │
   └────────────────────────────────────────────────────────────────────┘
```

Primary flow (user typing → guest):
1. User presses key in xterm
2. `attachCustomKeyEventHandler` fires → translates KeyboardEvent → bytes → `funnel.enqueue(bytes, {pacingMs: 0})` → returns `false` (suppresses xterm's default → master)
3. Funnel's drain loop calls `slave.write(bytes)` (single writer; master never re-injects because we returned false)
4. xterm-pty's ldisc cooks the bytes (NL→CRLF, echo, etc.) and forwards to QEMU's chardev

Primary flow (guest → server mirror):
1. QEMU writes to ttyS0 → xterm-pty's ldisc → ldisc.onWriteToUpper → slave._onReadable fires
2. Browser's `slave.onReadable` handler drains `slave.read()` (number[]) → base64 → `ws.send({type:'SerialOut',data})`
3. Server logs at trace level (Phase 2 doesn't react; Phase 4's `run` will consume the same frames for assertions)

### Pattern 1: axum 0.8 WebSocket handler with split + mpsc + per-connection state

**What:** Single handler function extracted via `WebSocketUpgrade`. After upgrade, split the socket; one task pumps an `mpsc::Receiver<WireFrame>` into the `SplitSink`; the handler's main loop reads from the `SplitStream` and dispatches frames.

**When to use:** Always for `/ws`. Even though Phase 2 has only one writer in practice (the recv-task echoing Hello and the message loop), the mpsc pattern future-proofs Phase 4 where the scenario engine will inject `SerialIn` frames from a separate task.

**Example:**
```rust
// crates/bootroom/src/ws.rs (new file)
// Source: docs.rs/axum/0.8/axum/extract/ws/ + axum/examples/testing-websockets

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use bootroom_core::{GuestState, WsMessage};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::state::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

    // Writer task: serializes outbound frames.
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "serialize WsMessage");
                    continue;
                }
            };
            if sink.send(Message::Text(json.into())).await.is_err() {
                break; // client gone
            }
        }
        // Best-effort close
        let _ = sink.close().await;
    });

    // Initial Hello
    let _ = tx
        .send(WsMessage::Hello {
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
        .await;

    // Reader loop
    while let Some(msg_res) = stream.next().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, "ws recv error; closing");
                break;
            }
        };
        match msg {
            Message::Text(text) => {
                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(wire) => handle_wire(wire, &tx, &state).await,
                    Err(e) => tracing::warn!(error = %e, payload = %text, "bad WsMessage"),
                }
            }
            Message::Binary(_) => {
                tracing::warn!("unexpected binary WS frame; protocol is JSON");
            }
            Message::Ping(_) | Message::Pong(_) => {} // axum auto-handles pings
            Message::Close(_) => break,
        }
    }
    drop(tx); // closes writer task
    let _ = writer.await;
}

async fn handle_wire(wire: WsMessage, _tx: &mpsc::Sender<WsMessage>, _state: &AppState) {
    match wire {
        WsMessage::SerialIn { data: _ } => {
            // Phase 2: pass-through observer only. Phase 4 will react.
            tracing::trace!("SerialIn frame received");
        }
        WsMessage::SerialOut { data: _ } => {
            // Mirror frame from browser; Phase 4's run mode consumes these.
            tracing::trace!("SerialOut frame received");
        }
        WsMessage::Launch => tracing::info!("client Launch"),
        WsMessage::Reset => tracing::info!("client Reset"),
        WsMessage::State { .. } | WsMessage::Hello { .. } => {
            // Server is the source of these; client sending them is a protocol error in Phase 2.
            tracing::warn!("client sent server-owned message kind");
        }
    }
}
```

Wire it in:
```rust
// crates/bootroom/src/server.rs (build_router)
.route("/ws", axum::routing::any(crate::ws::ws_handler))
```

`any` is used (not `get`) per the axum example — WebSocket upgrades arrive as HTTP GET but `any` keeps the door open for future protocol negotiation.

### Pattern 2: `WsMessage` + `GuestState` in `bootroom-core`

**What:** Pure-types library. No tokio, no I/O, no axum types. Phase 4 imports this same enum for the headless driver.

**When to use:** This IS the protocol contract — exactly as locked in `<decisions>`. No deviation.

**Example:**
```rust
// crates/bootroom-core/src/lib.rs
//
// Phase 2: WS protocol types. Used by both the bootroom binary's /ws
// handler and Phase 4's headless run driver.

#![cfg_attr(not(test), deny(unsafe_code))]

use serde::{Deserialize, Serialize};

/// Wire-level message exchanged over /ws.
///
/// Externally tagged via `#[serde(tag = "type")]`, producing JSON of the form
/// `{"type": "SerialIn", "data": "..."}`. Byte payloads (`SerialIn`,
/// `SerialOut`) are base64-encoded so the protocol is JSON-only on the wire.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Bytes the host injects into guest stdin. Base64-encoded.
    SerialIn { data: String },
    /// Bytes the guest emitted on serial. Base64-encoded.
    SerialOut { data: String },
    /// Authoritative guest state from the server.
    State { state: GuestState },
    /// Client asks the server (and observers) to log a Launch action.
    Launch,
    /// Client asks the server (and observers) to log a Reset action.
    Reset,
    /// Server -> client on connect. Version is `CARGO_PKG_VERSION`.
    Hello { version: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestState {
    Idle,
    Loading,
    Running,
    Halted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_in_roundtrip() {
        let m = WsMessage::SerialIn { data: "aGVsbG8=".into() };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"type":"SerialIn","data":"aGVsbG8="}"#);
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn unit_variant_serializes_as_object_with_only_type() {
        let s = serde_json::to_string(&WsMessage::Launch).unwrap();
        assert_eq!(s, r#"{"type":"Launch"}"#);
    }

    #[test]
    fn state_message_contains_nested_state() {
        let s = serde_json::to_string(&WsMessage::State { state: GuestState::Running }).unwrap();
        assert_eq!(s, r#"{"type":"State","state":"Running"}"#);
    }
}
```

**Wire shape for the browser (locked):**
- `{"type":"Hello","version":"0.1.0"}` — server greeting
- `{"type":"SerialIn","data":"<b64>"}` — server can send (Phase 4 will); client must accept
- `{"type":"SerialOut","data":"<b64>"}` — client emits; server logs
- `{"type":"State","state":"Running"}` — server emits authoritative state
- `{"type":"Launch"}` / `{"type":"Reset"}` — client emits before reload

### Pattern 3: Single-writer funnel without losing xterm rendering

**What:** Keep `xterm.loadAddon(master)` so the OUTPUT path (guest serial → ldisc → master.onWrite → xterm.write) keeps working unchanged. Block the INPUT path that master sets up in its `activate` method by suppressing xterm's `onData` event entirely with `attachCustomKeyEventHandler(evt => { ...; return false; })`. Translate the keyboard event to bytes ourselves and push through the funnel — making the funnel the sole caller of `slave.write`.

**Why this matters (the trap):** Inspecting the vendored `xterm-pty.js` (master class `activate(e)`):
```javascript
let i = r => this.ldisc.writeFromLower(r);
this.disposables.push(e.onData(i), e.onBinary(i), e.onResize(...));
```
That `e.onData(i)` is the auto-subscription. Without our intervention, every keystroke fires both:
1. **xterm's default path:** keystroke → `xterm.onData(data)` → master forwards via `ldisc.writeFromLower(data)` → ends up in slave's upper buffer (the guest reads it)
2. **Our funnel path:** keystroke → `funnel.enqueue(bytes)` → eventually `slave.write(bytes)` (which goes through ldisc → upper buffer)

Result: every byte is double-injected, breaking WS-02 by construction. The fix is keeping the Phase-1 mechanism (`attachCustomKeyEventHandler`) but using it to **simultaneously intercept the keystroke for our funnel AND suppress xterm's default `onData`** (by returning `false`).

**When to use:** Always for the input path.

**Example (browser side):**
```javascript
// app.js (Phase 2 deltas to the input wiring)
//
// IMPORTANT: returning false from attachCustomKeyEventHandler suppresses
// xterm's default keystroke dispatch, which means xterm.onData does NOT
// fire and master.activate's e.onData(i) listener never sees the keystroke.
// We then have full ownership of the input path; the funnel is the only
// writer to slave.write. See Pitfall #1 for the failure mode.

import { Funnel, keyEventToBytes } from './funnel.js';
const funnel = new Funnel(slave);

xterm.attachCustomKeyEventHandler((evt) => {
  if (evt.type !== 'keydown') return true; // let keyup/keypress fall through
  const bytes = keyEventToBytes(evt);
  if (bytes && bytes.length > 0) {
    funnel.enqueue(bytes, { pacingMs: 0 });
  }
  return false; // suppress xterm default → master never sees these bytes
});
```

**`keyEventToBytes` reference:** The minimal correct translation covers printable keys (`evt.key.length === 1` → UTF-8 bytes of that codepoint), Enter (`[0x0d]` — ldisc handles CR→NL if `ICRNL`), Backspace (`[0x7f]` — termios `VERASE` default), arrows (CSI sequences `\x1b[A` etc.), Tab (`[0x09]`), Escape (`[0x1b]`). The exhaustive table lives in xterm.js's own internal `KeyboardHandler`. For Phase 2, a 30-line dispatcher covering the common keys is sufficient; flag unknown keys with `console.debug` for follow-up if a kernel TUI needs more.

### Pattern 4: Browser auto-open after bind succeeds

**What:** Call `open::that_detached(url)` AFTER `TcpListener::bind` succeeds but BEFORE blocking on `axum::serve`. Gate behind `!args.no_open`. Log a warning (don't fail) if the call returns an error.

**Why detached:** On Linux without `xdg-open` configured for HTTP URLs (rare), the call can hang the parent process if not detached. `open::that_detached` spawns the child detached so the server process keeps running even if the launcher does something unexpected.

**Wayland nuance (verified on target environment):** This dev environment runs Hyprland on Wayland; `xdg-open` is `/usr/bin/xdg-open` and properly delegates to the configured browser. No special args needed. `XDG_CURRENT_DESKTOP=Hyprland` is set; `WAYLAND_DISPLAY=wayland-1` is exported. `open` crate works unchanged.

**Example:**
```rust
// crates/bootroom/src/server.rs (inside run, after listener.local_addr())

let bound = listener.local_addr()?;
println!("Serving bootroom on http://{bound} (Ctrl-C to stop)");

if !args.no_open {
    let url = format!("http://{bound}");
    match open::that_detached(&url) {
        Ok(_) => tracing::info!("opened {url} in default browser"),
        Err(e) => {
            // UI-SPEC says: print "Could not open browser automatically — open the URL above manually."
            eprintln!("Could not open browser automatically — open the URL above manually.");
            tracing::warn!(error = %e, "open::that_detached failed");
        }
    }
}

axum::serve(listener, app).await.context("axum::serve exited")?;
```

CLI delta (`cli.rs`):
```rust
#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    // ... existing fields ...

    /// Do not auto-open the default browser on start.
    #[arg(long)]
    pub no_open: bool,
}
```

### Pattern 5: Status pill state machine with WS authority

**What:** Browser-side state machine drives the pill locally from emscripten lifecycle events. When `/ws` connects, server-pushed `State` messages override. `RUNNING` requires BOTH `Module.onRuntimeInitialized` AND first SerialOut byte observed (raises the bar from Phase 1's "init only" — RUNNING now means the guest is actually executing).

**Why first-byte detection:** Some kernels take 200–600 ms after `onRuntimeInitialized` before the first serial byte appears (PLIC + UART init). Phase 1 considered "runtime initialized" sufficient; Phase 2 tightens this so the pill is honest about whether the guest is executing.

**Example:**
```javascript
// app.js
let runtimeInitialized = false;
let firstSerialOutSeen = false;
let serverStateAuthority = null; // most recent WS State, if any

function recomputePillLocal() {
  if (serverStateAuthority !== null) {
    setPill(serverStateAuthority);
    return;
  }
  if (runtimeInitialized && firstSerialOutSeen) setPill('RUNNING');
  else if (runtimeInitialized) setPill('LOADING'); // brief window
  // IDLE and HALTED are set explicitly by their triggers
}

setPill('IDLE'); // initial

Module.onRuntimeInitialized = () => {
  runtimeInitialized = true;
  // existing kernel-swap logic stays here...
  recomputePillLocal();
};
Module.onExit = () => { serverStateAuthority = null; setPill('HALTED'); };
Module.onAbort = () => { serverStateAuthority = null; setPill('HALTED'); };

slave.onReadable(() => {
  const bytes = slave.read();
  if (bytes.length > 0 && !firstSerialOutSeen) {
    firstSerialOutSeen = true;
    recomputePillLocal();
  }
  // ...mirror via WS as in the architecture diagram...
});

// In WS handler:
function handleWsFrame(frame) {
  if (frame.type === 'State') {
    serverStateAuthority = frame.state.toUpperCase();
    setPill(serverStateAuthority);
  }
  // ...
}
```

### Anti-Patterns to Avoid

- **Calling `slave.write` from multiple places.** Violates WS-02. Funnel only.
- **Loading the `master` addon and leaving `xterm.onData` un-suppressed.** Pitfall #1; user keystrokes get double-injected.
- **`open::that` (non-detached).** Can hang the parent process if the launcher does something odd; use `open::that_detached`.
- **Auto-reconnecting WS with exponential backoff.** Out of scope per `<deferred>`. Naive 1s retry, terminal-logged.
- **Binary WS frames.** Out of scope per `<deferred>` and `<decisions>`.
- **Decoding base64 in the server (Phase 2).** Server is a pass-through; only Phase 4 needs to decode SerialOut for assertions.
- **Holding a `Mutex<WebSocket>`.** Use `split() + mpsc` per the official axum pattern.
- **Sending a Close frame from JS before reload.** `location.reload()` tears down the connection; the server sees a clean close. Don't add a `ws.close()` racing the reload.
- **Treating xterm `onData` as bytes.** `onData` callbacks receive a `string`; for raw bytes use `onBinary` (which we are NOT subscribing to in Phase 2).
- **`atob`/`btoa` on multi-byte UTF-8 strings without encoding.** They are Latin-1; multi-byte input throws `InvalidCharacterError`. The funnel deals with byte arrays directly to avoid this — see Pitfall #3.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WebSocket framing (RFC 6455) | Raw `hyper::upgrade` + tungstenite | `axum::extract::ws::WebSocketUpgrade` | axum 0.8 ships the extractor; one feature flag away. |
| Concurrent WS write serialization | `Mutex<WebSocket>` or hand-rolled queue | `socket.split() + mpsc::channel` | The well-trodden axum pattern; doesn't kill duplex like a Mutex would. |
| Browser auto-open (xdg-open / `start` / `open`) | `Command::new("xdg-open")` | `open::that_detached` | Cross-platform; handles WSL, BSD, etc.; detached process. |
| Base64 encode/decode | Hand-roll a 6-bit packer | `base64 = "0.22"` crate, `STANDARD` engine | Subtle padding rules, URL-safe variants, MIME quirks. Don't. |
| Tagged-enum JSON | Hand-roll a `{ "type": ..., "data": ... }` formatter | `serde` with `#[serde(tag = "type")]` | Already locked. |
| Per-byte UART pacing | `setInterval` with bookkeeping | Async drain loop with `await sleep(pacingMs)` between bytes | Code Examples §2; simpler, no leaked intervals on reload. |
| xterm keystroke → bytes translation | DIY full keymap | Use xterm's default for printable chars; only handle special keys explicitly | Pattern 3; minimal 30-line dispatcher is enough for Phase 2. |
| `keyEventToBytes` ANSI sequences | Manual lookup | The full table for arrows/Home/End/PageUp/PageDown is in xterm.js's `KeyboardHandler` source; copy the relevant subset rather than re-derive | The escape sequences are well-known but easy to typo. |

**Key insight:** Phase 2 has very little algorithmic content. It's connecting four well-shaped components (axum WS, xterm-pty slave, `WsMessage` enum, `open` crate) with a single hand-rolled piece — the write funnel — that exists purely to enforce single-writer invariant. Every other piece is "copy the canonical pattern."

## Runtime State Inventory

> Phase 2 is not a rename/refactor phase. This section is included briefly to confirm nothing carries over.

Phase 2 introduces:
- New file `crates/bootroom-core/src/lib.rs` content (was empty in Phase 1) — pure type definitions.
- New file `crates/bootroom/src/ws.rs` — WS handler module.
- New file `crates/bootroom/web/funnel.js` — write funnel singleton.
- Modifications to: `Cargo.toml` (workspace deps), `crates/bootroom/Cargo.toml` (deps + ws feature), `crates/bootroom/src/cli.rs` (--no-open), `crates/bootroom/src/server.rs` (route + auto-open), `crates/bootroom/web/index.html` (buttons + controls), `crates/bootroom/web/app.js` (WS lifecycle + status pill + funnel), `crates/bootroom/web/style.css` (button + .term-ctrls hooks).

**No stored data, no live service config, no OS-registered state, no secrets/env vars, no build artifacts to migrate.** Phase 2 is greenfield-on-Phase-1.

## Common Pitfalls

### Pitfall 1: Double-injection of user keystrokes (master addon auto-subscribes to xterm.onData)

**What goes wrong:** Removing Phase 1's `attachCustomKeyEventHandler(() => false)` and adding `xterm.onData(d => funnel.enqueue(...))` "to be a good citizen" means master's own `activate()` listener ALSO fires (`e.onData(i => this.ldisc.writeFromLower(i))`). Every keystroke reaches the guest twice — once through the funnel, once through master. WS-02 is violated by construction.

**Why it happens:** xterm's `onData` is a fan-out emitter (all subscribers fire). There's no "this is the only handler" semantics. The trap is invisible until a kernel echoes back what it received and you notice `hheelllloo` in the terminal.

**How to avoid:** Use `attachCustomKeyEventHandler` returning `false` to suppress xterm's default `onData` firing entirely, and translate the event to bytes inside that handler. Then the funnel is the sole writer. Code in Pattern 3.

**Warning signs:** Doubled characters in the terminal. Shells parsing commands like `lls` instead of `ls`. Echo loop with the kernel's serial echo enabled.

### Pitfall 2: Concurrent WS writes from multiple tasks lose frames or panic on second mut borrow

**What goes wrong:** A naive "spawn a task per source of frames, all writing to the same `WebSocket`" architecture deadlocks (Mutex) or panics (multiple mutable borrows of `SplitSink`).

**Why it happens:** `WebSocket::send` takes `&mut self`. Holding it across tasks needs serialization.

**How to avoid:** The mpsc-channel-to-writer-task pattern in Pattern 1. One task owns `SplitSink`; everyone else sends `WsMessage` into the channel. The channel is the serialization point.

**Warning signs:** Compilation errors about `SplitSink` not being `Clone`; runtime "second borrow" panics in tests.

### Pitfall 3: `btoa`/`atob` only handle Latin-1, not Uint8Array

**What goes wrong:** Naively writing `btoa(slave.read().join(''))` works for ASCII serial output and silently corrupts (or throws `InvalidCharacterError`) on any byte ≥ 0x80 — which any UTF-8 kernel printf with non-ASCII characters will produce.

**Why it happens:** `btoa` treats its input as a Latin-1 string. JS's `Uint8Array` + `btoa` is a classic interop gap; MDN documents the workaround.

**How to avoid:** Convert `number[]` (what `slave.read()` returns — verified by inspecting the vendored source: it's `splice()`'d from a number-array buffer) → `Uint8Array` → base64 via a small helper:

```javascript
// funnel.js
export function bytesToB64(bytes) {
  // bytes: number[] | Uint8Array
  let binary = '';
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  for (let i = 0; i < u8.length; i++) binary += String.fromCharCode(u8[i]);
  return btoa(binary);
}
export function b64ToBytes(b64) {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
```

`String.fromCharCode(u8[i])` for `u8[i] in [0, 255]` is safe because the resulting characters are all in the Latin-1 range that `btoa` accepts. The encoding doesn't interpret the bytes — it's byte-identity through a string proxy.

**Warning signs:** Kernel output with multi-byte UTF-8 garbles; WS frames intermittently throw at `btoa`; tests with `printf "café"` fail.

### Pitfall 4: COOP/COEP on WS upgrade response — verify, don't assume

**What goes wrong:** axum's WS upgrade response IS a normal HTTP 101 response that flows through the same tower middleware stack. `SetResponseHeaderLayer::overriding` on the router applies to it. BUT: a future router refactor (per-route layering, sub-router for `/ws`) could accidentally strip the layers from the WS upgrade. Browsers don't always validate COOP/COEP on the upgrade response (since the page already committed to COI from its main-document headers), but Spec ambiguity makes this brittle.

**Why it happens:** Devs assume "the page already has SAB; the WS upgrade doesn't need headers" and move the layers somewhere that excludes `/ws`.

**How to avoid:** Add a regression integration test that asserts both COOP and COEP appear on the response headers of an actual WS upgrade attempt. The test can be as simple as: send a non-WS GET to `/ws` (the extractor will reject with 400 or 426), inspect headers. Even better: do the upgrade with tokio-tungstenite and inspect the response object.

**Warning signs:** Future regression test fails after a router refactor. SAB unavailable on a page that previously had it.

### Pitfall 5: `xterm.buffer.active.translateToString(true, 0, length)` arguments

**What goes wrong:** UI-SPEC says Copy reads `xterm.buffer.active.translateToString(true, 0, xterm.buffer.active.length)`. The xterm.js API signature is `translateToString(trimRight?: boolean, startRow?: number, endRow?: number): string`. `endRow` is EXCLUSIVE in some versions and inclusive in others; passing `xterm.buffer.active.length` is correct (it's the row count, and `length` is one-past-the-last index).

**Why it happens:** Off-by-one on xterm's row API; behaviour has shifted between minor versions.

**How to avoid:** Test that copying with N lines of output yields N lines (not N±1) in the clipboard. The xterm.js 5.3.0 API matches this signature.

**Warning signs:** Last line missing or duplicated in Copy output.

### Pitfall 6: WS reconnect on page reload — ensure server is reload-resilient

**What goes wrong:** Browser page reload tears down WS → server sees `Close`. Next page load → new WS upgrade → new connection. If the server kept per-connection state in a non-cleaned-up `Arc`, memory leaks across reload cycles.

**Why it happens:** Holding a strong `Arc<ConnState>` on `AppState` or in a global registry that doesn't drop on disconnect.

**How to avoid:** Phase 2 has no per-connection state outside the handler-task scope. The mpsc + writer-task pattern drops cleanly when the handler returns. Don't add a "connections registry" until Phase 4 needs it.

**Warning signs:** RSS grows with each reload in long-running dev sessions.

### Pitfall 7: Funnel queue never drains because the drain loop awaits a producer that doesn't notify

**What goes wrong:** A simple `while (queue.length > 0) { await drain(); }` loop in `funnel.enqueue` race-conditions: if two enqueues happen in close succession, the second sees the loop running and doesn't kick. The bytes sit in the queue until something else triggers a drain.

**Why it happens:** Common bug in JS producer/consumer code.

**How to avoid:** Pattern the funnel as a strict single-task pump:
```javascript
class Funnel {
  constructor(slave) {
    this.slave = slave;
    this.queue = [];
    this.draining = false;
  }
  async enqueue(bytes, { pacingMs = 0 } = {}) {
    // Append (byte, pacingMs) tuples; do NOT mutate bytes during await
    for (const b of bytes) this.queue.push([b, pacingMs]);
    if (this.draining) return;
    this.draining = true;
    try { await this.#drain(); }
    finally { this.draining = false; }
  }
  async #drain() {
    while (this.queue.length > 0) {
      const [b, ms] = this.queue.shift();
      this.slave.write([b]); // slave.write accepts number[]
      if (ms > 0) await new Promise(r => setTimeout(r, ms));
    }
  }
}
```

The `draining` flag plus the explicit `if (this.draining) return` after enqueue ensures only one drain loop runs at a time; subsequent enqueues only append to the queue.

**Warning signs:** Bytes queued during a scenario appear after the scenario finishes; user typing stalls when a scenario is mid-injection.

### Pitfall 8 (carried from project PITFALLS.md #9): Concurrent serial writes from UI and scenario reorder bytes

**Load-bearing pitfall for this phase.** The funnel IS the prevention. Verified design: single funnel singleton, only writer to `slave.write`. Every enqueue is monotonic; drain is serialized by the `draining` flag.

**Verification:** integration test that interleaves WS-arriving `SerialIn` frames with synthetic xterm key events, observes the resulting `SerialOut` echo from a loop-back kernel (or a recording of guest serial), confirms no byte reordering.

**Phase 2 mitigation:** Browser-side funnel is sufficient. Phase 3 will add server-side "manual input disabled while scenario runs" (out of Phase 2 scope per `<deferred>`).

## Code Examples

Verified patterns from official sources and inspection of the in-tree vendored libraries.

### §1: Integration test — round-trip Hello and SerialOut over WS using tokio-tungstenite

```rust
// crates/bootroom/tests/ws_roundtrip.rs
//
// Source pattern: github.com/tokio-rs/axum/blob/main/examples/testing-websockets/
// Adapted for the existing TestServer harness in tests/common/mod.rs.

mod common;

use bootroom_core::{GuestState, WsMessage};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn ws_handshake_emits_hello() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;

    // common::spawn binds at http://; rewrite to ws://
    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";
    let (mut socket, _resp) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");

    // First frame from server MUST be Hello.
    let first = socket.next().await.expect("server frame").unwrap();
    let text = first.to_text().expect("text frame");
    let parsed: WsMessage = serde_json::from_str(text).expect("parse Hello");
    match parsed {
        WsMessage::Hello { version } => {
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[tokio::test]
async fn ws_client_serial_in_is_logged_not_echoed() {
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let ws_url = server.base_url.replace("http://", "ws://") + "/ws";
    let (mut socket, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // Discard Hello.
    let _ = socket.next().await.unwrap();

    // Send a SerialIn frame.
    let frame = WsMessage::SerialIn { data: "aGVsbG8=".into() };
    socket
        .send(Message::Text(serde_json::to_string(&frame).unwrap().into()))
        .await
        .expect("send");

    // Phase 2 server logs and does nothing visible to the client.
    // Ensure the connection stays open (no error frame).
    // Closing cleanly is the assertion.
    socket
        .send(Message::Close(None))
        .await
        .expect("close");
}

#[tokio::test]
async fn ws_upgrade_response_carries_coop_coep() {
    // CR / Pitfall 4: regression test that the WS 101 upgrade response
    // still carries COOP and COEP. Use reqwest to perform a raw GET
    // without the Upgrade header so we get an HTTP response back.
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let server = common::spawn(kernel.path().to_path_buf(), None).await;
    let resp = reqwest::get(format!("{}/ws", server.base_url)).await.unwrap();
    // We expect 400 (or whatever rejection axum chooses for a missing
    // Upgrade); the IMPORTANT assertion is the headers.
    assert_eq!(
        resp.headers().get("cross-origin-opener-policy").map(|v| v.to_str().unwrap()),
        Some("same-origin")
    );
    assert_eq!(
        resp.headers().get("cross-origin-embedder-policy").map(|v| v.to_str().unwrap()),
        Some("require-corp")
    );
}
```

### §2: Funnel skeleton with per-byte pacing

```javascript
// crates/bootroom/web/funnel.js
//
// Single-writer funnel to xterm-pty's slave. Enforces WS-02 (no
// byte-interleaving between user typing and WS-arriving SerialIn).
//
// Browsers do not deliver function calls in deterministic order across
// promise resolutions; the draining flag guarantees one pump at a time.

export class Funnel {
  constructor(slave) {
    this.slave = slave;
    /** Array of [byte:number, pacingMs:number] tuples. */
    this.queue = [];
    this.draining = false;
  }

  /**
   * Enqueue bytes for delivery to the guest.
   * @param {Uint8Array|number[]} bytes
   * @param {{pacingMs?: number}} opts pacingMs delays BETWEEN bytes (0 = no pacing)
   */
  enqueue(bytes, opts = {}) {
    const pacingMs = opts.pacingMs ?? 0;
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    for (let i = 0; i < u8.length; i++) this.queue.push([u8[i], pacingMs]);
    if (!this.draining) {
      this.draining = true;
      this.#drain().finally(() => { this.draining = false; });
    }
  }

  async #drain() {
    while (this.queue.length > 0) {
      const [b, ms] = this.queue.shift();
      // slave.write accepts number[] OR string (verified against vendored
      // xterm-pty.js — see writeFromUpper in the P class).
      this.slave.write([b]);
      if (ms > 0) await new Promise(r => setTimeout(r, ms));
    }
  }
}

export function bytesToB64(bytes) {
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let s = '';
  // Chunk to avoid argument-count limits on large bursts.
  const CHUNK = 0x8000;
  for (let i = 0; i < u8.length; i += CHUNK) {
    s += String.fromCharCode.apply(null, u8.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

export function b64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * Minimal KeyboardEvent → bytes translator covering the keys a kernel
 * REPL typically needs. Returns Uint8Array, or null if the event has no
 * byte representation (modifier-only keys, etc.).
 */
export function keyEventToBytes(evt) {
  // Ignore pure-modifier keypresses.
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(evt.key)) return null;

  // Ctrl-letter: emit C-x control byte.
  if (evt.ctrlKey && evt.key.length === 1) {
    const c = evt.key.toUpperCase().charCodeAt(0);
    if (c >= 0x40 && c <= 0x5f) return new Uint8Array([c - 0x40]);
  }

  // Special keys.
  switch (evt.key) {
    case 'Enter':      return new Uint8Array([0x0d]); // CR; ldisc handles ICRNL
    case 'Backspace':  return new Uint8Array([0x7f]); // DEL; termios VERASE default
    case 'Tab':        return new Uint8Array([0x09]);
    case 'Escape':     return new Uint8Array([0x1b]);
    case 'ArrowUp':    return enc('\x1b[A');
    case 'ArrowDown':  return enc('\x1b[B');
    case 'ArrowRight': return enc('\x1b[C');
    case 'ArrowLeft':  return enc('\x1b[D');
    case 'Home':       return enc('\x1b[H');
    case 'End':        return enc('\x1b[F');
    case 'PageUp':     return enc('\x1b[5~');
    case 'PageDown':   return enc('\x1b[6~');
    case 'Delete':     return enc('\x1b[3~');
  }

  // Printable single character (covers most ASCII + UTF-8 BMP).
  if (evt.key.length === 1) return new TextEncoder().encode(evt.key);
  return null;
}

function enc(s) { return new TextEncoder().encode(s); }
```

### §3: WS handler — full minimal axum 0.8 implementation

Already shown in Pattern 1. The Cargo deltas:

```toml
# crates/bootroom/Cargo.toml
[dependencies]
axum = { workspace = true, features = ["ws"] }
futures-util = { workspace = true }
open = { workspace = true }
base64 = { workspace = true }

[dev-dependencies]
tokio-tungstenite = { workspace = true }
```

```toml
# Cargo.toml [workspace.dependencies] additions
futures-util = "0.3.32"
open = "5"
base64 = "0.22"
tokio-tungstenite = "0.29"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| axum 0.7 WS handler with `#[async_trait]` | axum 0.8 native async traits | axum 0.8 release (2026) | Already in place; reaffirm — copy ONLY from axum 0.8 docs/examples. |
| `tokio-tungstenite` 0.20–0.24 (`Vec<u8>` in `Message::Binary`) | 0.26+ → 0.29 (`Bytes`/`Utf8Bytes` in `Message`) | axum 0.8 PR #3078 | axum 0.8's `Message::Text` carries `Utf8Bytes` (a `Bytes` wrapper with utf-8 invariant). Use `.into()` when constructing; use `.as_str()` / `.to_text()` to read. |
| Hand-rolled COOP/COEP per-route | `tower-http` `SetResponseHeaderLayer` at router root | Already in place (Phase 1) | No change; verify it still applies to `/ws` upgrade (Pitfall #4). |
| xterm 6.x (scoped `@xterm/xterm`) | xterm 5.3.0 (unscoped) | xterm-pty 0.12.0 targets 5.3.0 specifically | Already locked; do not bump. |
| `open` crate 4.x | `open` crate 5.x | 2024 | API is identical for `that_detached`; just use latest. |

**Deprecated/outdated:**
- Old "atomic-receive-then-write" WS patterns in pre-0.8 axum docs — superseded by `split() + mpsc`.
- `Message::ping(...)`/`Message::pong(...)` constructor functions — gone in tungstenite 0.26+; pings are axum-handled automatically (we don't need to send them).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | axum 0.8.9 + tokio-tungstenite 0.29 are wire-compatible (both use tungstenite 0.26+ Bytes/Utf8Bytes representation) | Standard Stack | If wrong, dev-dep version conflicts surface as compile errors; fix by pinning to whatever axum-internal tungstenite resolves to. Risk: LOW — the axum 0.8 official testing example uses 0.29. |
| A2 | `xterm.attachCustomKeyEventHandler` returning `false` for a `keydown` event prevents `xterm.onData` from firing at all (suppresses default key processing) | Pattern 3, Pitfall 1 | If wrong, double-injection occurs even with our handler. Mitigation: integration test that pipes a known sequence through xterm and confirms a single instance reaches the guest. Risk: LOW — well-documented xterm behaviour. |
| A3 | `slave.read()` always returns `number[]` (not `Uint8Array`) | Pitfall 3, Code Examples §2 | If a future xterm-pty bump changes this, the funnel's byte-array conversion still works because we accept either. Verified against vendored 0.12.0 source. Risk: LOW — pinned. |
| A4 | `open::that_detached` on Linux/Wayland with Hyprland reliably delegates to the configured browser via xdg-open | Pattern 4 | If xdg-open is unconfigured on a user's system, the call fails; we log and continue. Risk: LOW — `xdg-open` and Chromium both verified present in the target environment. |
| A5 | The browser's `WebSocket` API auto-reconnects are sufficient (naive setTimeout + new WebSocket on `onclose`) | Interaction contracts in UI-SPEC | Phase 2's reconnect is opt-out simple; if it proves flaky under real serve sessions, harden in Phase 3+. Risk: MEDIUM but explicitly accepted in `<deferred>`. |
| A6 | A page reload cleanly tears down the WS connection (no zombie sockets) | Pitfall 6 | If wrong, RSS grows over reload cycles. Mitigation: Phase 2 handler is fully scoped to the handler task; no global registry. Risk: LOW. |
| A7 | tower-http's `SetResponseHeaderLayer::overriding` applied at the router root covers the WS 101 upgrade response | Pitfall 4 | Verified via integration test (Code Examples §1, `ws_upgrade_response_carries_coop_coep`). If wrong, fix by inserting an explicit middleware on the ws route. Risk: LOW — axum's WS upgrade is a normal Response that flows through tower middleware. |
| A8 | `master` xterm-pty addon stays loaded (its output path is needed); only its input path is suppressed by `attachCustomKeyEventHandler` returning false | Pattern 3 | Verified against vendored source: master subscribes to `e.onData(i)` in `activate(e)`. Returning false from the custom key handler prevents `e.onData` from firing on those keystrokes. Risk: LOW. |
| A9 | `Message::Text(json.into())` in axum 0.8 accepts `String` (via `Utf8Bytes: From<String>`) | Code Examples §1 / Pattern 1 | Verified in axum 0.8 PR #3078. If wrong, use `Message::text(json)` constructor or `.into()` via `Utf8Bytes`. Risk: LOW. |
| A10 | The Phase 2 server does not need to react to client-sent `Launch` / `Reset` beyond logging — the page reload is the actual mechanism | `<decisions>` Launch/Reset | Confirmed in CONTEXT.md `<decisions>`. Phase 4 will add server-side observation hooks. Risk: NONE (explicitly stated). |

## Open Questions

1. **Should the funnel's drain loop yield to event-loop more aggressively to keep xterm rendering smooth under burst input?**
   - What we know: For `pacingMs > 0` we already yield via setTimeout. For `pacingMs = 0` (user typing), we don't yield between bytes — typing is human-rate, single bytes.
   - What's unclear: A WS-arriving 1MiB scenario step with `pacingMs = 0` would synchronously call `slave.write` thousands of times before xterm gets a chance to render.
   - Recommendation: Phase 2 ships the simple loop; if scenarios with no pacing prove laggy, add an `await new Promise(r => setTimeout(r, 0))` every N bytes. Out-of-scope unless observed.

2. **Should `--pacing-ms <N>` be a server CLI flag, or strictly a query-string?**
   - CONTEXT.md `<specifics>` mentions `?pacing=20` query param. `<decisions>` says "Config knob: query string param `?pacing=20` overrides; default 15ms. Server doesn't currently care about the pacing value (client-side concern)."
   - Recommendation: Query-string only. Don't add a server-side flag in Phase 2. Phase 3's TOML config may add a default-pacing setting; that's TBD.

3. **Should `Launch` / `Reset` WS frames carry a timestamp or reason?**
   - What we know: `<decisions>` shows them as unit variants. Phase 4 may want correlation IDs.
   - Recommendation: Ship as unit variants in Phase 2. If Phase 4 needs metadata, extend the variant — serde will gracefully handle a `Launch { reason: Option<String> }` extension because old clients deserialize the unit variant if the object has only `"type":"Launch"` (need to test: `#[serde(deny_unknown_fields)]` is NOT used on `WsMessage` per `<decisions>`).

4. **Should the integration test for SAB-on-WS-upgrade also exercise an actual successful upgrade (vs a 400-rejected GET)?**
   - What we know: Code Examples §1 has both `ws_handshake_emits_hello` (full upgrade via tokio-tungstenite) and `ws_upgrade_response_carries_coop_coep` (raw GET).
   - What's unclear: tokio-tungstenite's `connect_async` returns the response object — we should be able to assert headers on it directly. That's a cleaner test than the raw GET.
   - Recommendation: Test both. The raw-GET version catches "what does a misconfigured client see"; the upgraded version is the more honest test.

5. **Does the server need a graceful-shutdown handler that closes WS connections cleanly on Ctrl-C?**
   - What we know: Phase 1 uses default `axum::serve` without `with_graceful_shutdown`. WS connections terminate when the process exits.
   - What's unclear: Whether browsers show user-facing errors on abrupt close (probably not — `onclose` fires normally).
   - Recommendation: Defer to Phase 5 (Diagnostics & Doctor) when overall lifecycle gets a polish pass.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `rustc` | Build | ✓ | ≥ 1.85 (verified Phase 1) | — |
| `cargo` | Build | ✓ | ≥ 1.85 (verified Phase 1) | — |
| `xdg-open` | `open::that_detached` on Linux | ✓ | `/usr/bin/xdg-open` | If absent, `open` falls back through `gio open` → `gnome-open` → `kde-open`. Failure logged; server continues. |
| Wayland or X11 session | Auto-open to function | ✓ | Wayland (`WAYLAND_DISPLAY=wayland-1`, Hyprland) + XWayland | Headless CI: `--no-open` covers it. |
| `chromium` (or any browser) | The browser actually opens | ✓ | `/usr/bin/chromium` (148.0.7778.167 per Phase 1) | If no browser is installed, `xdg-open` fails; we log and continue. User can still navigate manually. |
| `tokio-tungstenite` | Integration tests only | ✓ via cargo | 0.29 | None needed; dev-dep. |

**Missing dependencies with no fallback:** None.

**Missing dependencies with fallback:** None — environment is fully provisioned for Phase 2.

## Validation Architecture

> `workflow.nyquist_validation = true` in `.planning/config.json` → this section is REQUIRED.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `cargo test`. Phase 1 already established `TestServer` harness at `crates/bootroom/tests/common/mod.rs`. |
| Config file | `Cargo.toml` `[dev-dependencies]` (no external runner). |
| Quick run command | `cargo test -p bootroom --lib && cargo test -p bootroom-core --lib` |
| Full suite command | `cargo test --workspace` |
| Headless browser smoke | `chromium --headless=new` against bootroom serving a fixture; or Spike B's chromiumoxide binary (already green per `01-STATE`). Phase 4 will productize this. |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WS-04 | `WsMessage` round-trips through serde-tagged JSON | unit | `cargo test -p bootroom-core --lib serial_in_roundtrip` (and siblings) | ❌ Wave 0 (write in `bootroom-core/src/lib.rs` `#[cfg(test)] mod tests`) |
| WS-01 | `/ws` accepts an upgrade, emits Hello, accepts SerialIn | integration | `cargo test -p bootroom --test ws_roundtrip ws_handshake_emits_hello` | ❌ Wave 0 |
| WS-01 (header regression) | WS upgrade response carries COOP+COEP | integration | `cargo test -p bootroom --test ws_roundtrip ws_upgrade_response_carries_coop_coep` | ❌ Wave 0 (Pitfall #4 mitigation) |
| WS-02 | Single-writer funnel — no byte interleaving between two enqueue sources | unit (JS) + integration (Rust-side observation) | Browser unit test of `Funnel` invariant (no test runner in Phase 2 — see "Wave 0 Gaps" for the manual test plan); integration test exercises the path end-to-end via headless smoke | ❌ Wave 0 (JS-side: documented manual test; Rust-side: end-to-end smoke) |
| WS-03 | Inter-character pacing on `SerialIn` decoded by funnel | manual + log inspection | Browser test: open DevTools, send a 10-byte SerialIn with `?pacing=50`, observe time-delta between `slave.write` calls in console.trace | manual-only Phase 2 |
| SERV-06 | `--no-open` suppresses browser auto-open; default opens | integration (subprocess) + manual | `cargo test -p bootroom --test serve_no_open` (mock `open::that_detached` via test feature or verify `bootroom serve --no-open --kernel /tmp/k --port 0` returns without crashing and does not block) | ❌ Wave 0 |
| UI-02 | xterm renders live serial via xterm-pty slave | manual + headless smoke | Open page; observe boot serial; OR Spike B's chromiumoxide harness with assert on `\d+ bytes captured` | manual-only Phase 2 (covered by Spike B for headless) |
| UI-03 | Keystrokes reach guest | manual | Open page, log into the guest shell, type commands, observe responses | manual-only Phase 2 |
| UI-04 | Clear empties terminal; Copy populates clipboard | manual | Click Clear, observe empty terminal; click Copy, paste, verify | manual-only Phase 2 |
| UI-06 | Status pill cycles Idle→Loading→Running→Halted | manual + console | Page load: observe initial IDLE; refresh and observe LOADING → RUNNING → (kernel exit) HALTED | manual-only Phase 2 |
| UI-08 | Launch reloads + boots fresh | manual | Modify `--kernel` artifact mid-session; click Launch; observe new boot | manual-only Phase 2 |
| UI-09 | Reset reloads + boots same kernel | manual | Click Reset; observe page reload + boot | manual-only Phase 2 |

### Sampling Rate

- **Per task commit:** `cargo test -p bootroom-core --lib && cargo test -p bootroom --lib` (fast)
- **Per wave merge:** `cargo test --workspace` + manual headed-browser smoke (open page, verify all four UI flows)
- **Phase gate:** Full suite green + manual headed-browser smoke against NORN kernel + Spike B harness re-run on Phase-2 build (smoke that headless path still works pre-Phase-4)

### Wave 0 Gaps

The following files do NOT exist yet and must be created before they can be used as gates:

- [ ] `crates/bootroom-core/src/lib.rs` — replace skeleton with `WsMessage` / `GuestState` + inline `#[cfg(test)]` round-trip tests
- [ ] `crates/bootroom/src/ws.rs` — new module
- [ ] `crates/bootroom/tests/ws_roundtrip.rs` — three tests: hello, serial_in pass-through, COOP/COEP upgrade headers
- [ ] `crates/bootroom/tests/serve_no_open.rs` — covers SERV-06 `--no-open` behaviour (use `--port 0` to avoid collision; verify the subprocess starts and prints the URL line)
- [ ] `crates/bootroom/web/funnel.js` — Funnel + bytesToB64 / b64ToBytes / keyEventToBytes helpers
- [ ] Browser-side unit test stub for `Funnel` — Phase 2 has no JS test runner; cover with `node --check` syntax validation + a manual test plan documented inline in `funnel.js` doc comments. Phase 3 may revisit.

No framework install required (`cargo test` only). No external runners.

## Project Constraints (from CLAUDE.md)

These directives are inherited from `./CLAUDE.md` and `~/.claude/CLAUDE.md`; the planner MUST honor them and the executor MUST NOT contradict them.

- **Tech stack: Rust only.** Single static binary, embeds static assets via `include_dir!`. No Node.js or Python required to run bootroom.
- **No npm-based frontend toolchain.** Vanilla JS + ES modules + vendored libs only. `funnel.js` is a vanilla ES module — no transpiler, no bundler. `import { ... } from './funnel.js'` is the contract.
- **MIT OR Apache-2.0 dual license.** Already locked. All new files: `// SPDX-License-Identifier: MIT OR Apache-2.0` if you choose to add an SPDX header, otherwise rely on Cargo metadata.
- **MSRV 1.85** — verified for all chosen versions of new deps (open 5.x, base64 0.22, tokio-tungstenite 0.29, futures-util 0.3 — all support 1.85).
- **No --break-system-packages.** N/A; we use cargo only.
- **Don't pollute home directory with temp files.** N/A; tests use `tempfile`.
- **GSD workflow enforcement.** Phase 2 work goes through `/gsd-plan-phase 2` → plans → execute. Direct edits forbidden.
- **No emojis in code or written documentation.**
- **CLAUDE.md (./CLAUDE.md) project constraints:**
  - "single static binary, embeds static assets via `include_dir!`" — Phase 2 doesn't change this; `funnel.js` is embedded alongside `app.js`.
  - "Web UI is vanilla JS + HTML (no build step)" — `funnel.js` MUST be a plain ES module import.
  - "Command surface — minimal: the user must never need >1 long-form command to do common tasks. Subcommands are short verbs (`serve`, `run`, `init`)" — Phase 2 adds only `--no-open` to `serve`; no new subcommands.
  - "Repo external-callable" — `--no-open` and `/ws` are runtime concerns; they don't depend on repo layout.

## Security Domain

> `security_enforcement` is unset in `.planning/config.json`, so default = enabled.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | bootroom is local-dev-only; loopback bind; explicit non-goal in PROJECT.md. |
| V3 Session Management | no | No sessions; one WS connection per page; per-handler-task state. |
| V4 Access Control | partial | Loopback bind already enforced in Phase 1. WS endpoint inherits the same `--host 127.0.0.1` default. |
| V5 Input Validation | yes | `WsMessage` parsing via serde with strict tag matching; unknown frames are rejected with a tracing warning, never panicked on. |
| V6 Cryptography | no | base64 is encoding, not crypto. No keys, no signatures in Phase 2. |
| V12 File and Resources | partial | No new file-serving paths in Phase 2. Phase 1's path-traversal protection on `/assets/{*path}` covers all asset routes; `/ws` is not file-backed. |
| V14 Configuration | yes | COOP/COEP must continue to apply to the WS upgrade response — Pitfall #4 has the regression test. |

### Known Threat Patterns for axum-WebSocket + xterm-pty + browser

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Cross-Site WebSocket Hijacking (CSWSH): a malicious page from another origin opens a WS to our `/ws` because WS upgrades don't enforce SOP | Spoofing | Loopback bind makes this require local foothold. axum's `WebSocketUpgrade` does NOT auto-check `Origin` header; if Phase 4+ ever binds non-loopback, add an origin allow-list. Out of Phase 2 scope (bind is loopback). |
| WS frame flooding from a misbehaving client causing the mpsc queue to grow unbounded | Denial of Service | mpsc channel bounded at capacity 32 (Code Examples §1 / Pattern 1). When the channel is full, sends back-pressure and frames are dropped — acceptable for a loopback dev tool. |
| Malformed JSON in a WS Text frame causes server panic | Denial of Service | `serde_json::from_str` returns Result; we `match` and log warnings. No panic possible. |
| Browser auto-opens a URL controlled by user-supplied data | Injection | The URL is constructed server-side from `listener.local_addr()`; no user data is interpolated. `open::that_detached` doesn't execute a shell — it spawns a process. Safe. |
| base64-decoded payload triggers excessive memory allocation | Resource exhaustion | Phase 2 server does NOT decode SerialIn (pass-through observer only). When Phase 4 decodes, cap input size. |
| COOP/COEP stripped from `/ws` upgrade response via router refactor | Spoofing (downgrade cross-origin isolation) | Regression test `ws_upgrade_response_carries_coop_coep` in Code Examples §1. |
| WS sending `Launch` / `Reset` as a way to remotely trigger page reload of a connected user | Tampering / unauthorized control | Loopback bind + single-user dev tool means no remote attacker. Phase 2 logs only — doesn't broadcast Launch/Reset to other connections (no other connections in scope). |

## Sources

### Primary (HIGH confidence)

- `crates/bootroom/web/vendor/xterm-pty.js` (in-tree, vendored 0.12.0) — inspected the minified source to confirm `slave.read()` returns `number[]`, `slave.write` accepts string or number[], `master.activate` auto-subscribes to `xterm.onData`, the `_onReadable` emitter fires on every `ldisc.onWriteToUpper`.
- `crates/bootroom-core/src/lib.rs` (current) — empty skeleton, Phase 2 lands `WsMessage` + `GuestState` here.
- `crates/bootroom/src/server.rs` (current) — existing router, the `/ws` route goes here.
- `crates/bootroom/tests/common/mod.rs` (current) — `TestServer` harness Phase 2 tests reuse.
- `.planning/phases/02-websocket-live-serial/02-CONTEXT.md` — locked decisions (this RESEARCH treats them as inviolable).
- `.planning/phases/02-websocket-live-serial/02-UI-SPEC.md` — UI deltas (already approved).
- `.planning/phases/01-walking-skeleton/01-RESEARCH.md` — Phase 1 verified versions; reused.
- `.planning/phases/01-walking-skeleton/01-REVIEW.md` — Phase 1 patterns to preserve (IpAddr parsing, tri-state path traversal, error-kind discrimination, FS-error narrow catch).
- `.planning/research/PITFALLS.md` #9 — concurrent serial writes (the load-bearing pitfall this phase addresses).
- crates.io API (queried 2026-05-18) — verified `open` 5.3.5, `tokio-tungstenite` 0.29.0, `futures-util` 0.3.32, `base64` 0.22.1.
- [axum 0.8 testing-websockets example](https://github.com/tokio-rs/axum/blob/main/examples/testing-websockets/src/main.rs) — verified the test pattern, dependencies, version pins.
- [axum 0.8 testing-websockets Cargo.toml](https://github.com/tokio-rs/axum/blob/main/examples/testing-websockets/Cargo.toml) — verified `tokio-tungstenite = "0.29"`.

### Secondary (MEDIUM confidence)

- [docs.rs/axum/latest/axum/extract/ws/](https://docs.rs/axum/latest/axum/extract/ws/) — `WebSocketUpgrade`, `Message`, split semantics.
- [github.com/mame/xterm-pty (README)](https://github.com/mame/xterm-pty) — slave/master API as documented (verified against the vendored source).
- [docs.rs/open](https://docs.rs/open/latest/open/) — `that_detached` semantics, Linux/Wayland behaviour.
- [github.com/snapview/tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) — `connect_async` integration-test pattern.
- [github.com/tokio-rs/axum/pull/3078](https://github.com/tokio-rs/axum/pull/3078) — `Utf8Bytes`/`Bytes` change in axum 0.8 + tokio-tungstenite 0.26.
- [MDN — Cross-Origin-Embedder-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy) — header semantics; confirms applying COEP universally is safe.
- [serde.rs enum representations](https://serde.rs/enum-representations.html) — `#[serde(tag = "type")]` for tagged JSON.

### Tertiary (LOW confidence — verify during execution)

- The exact xterm.js `attachCustomKeyEventHandler` return-value-suppression semantics across xterm.js 5.3.0 (well-documented but the integration test in Pitfall #1 is the authoritative validation).
- Browser auto-open behaviour under non-Hyprland Wayland compositors (sway, KDE) — `open` crate is supposed to handle these but is not verified per-compositor in this environment.

## Metadata

**Confidence breakdown:**
- WS protocol design (`WsMessage` enum, JSON+base64): HIGH — locked in `<decisions>`, verified pattern from serde docs.
- axum 0.8 WS handler pattern: HIGH — official example available; project already runs axum 0.8.9.
- xterm-pty slave API: HIGH — inspected the vendored source byte-for-byte for the API surface.
- Single-writer funnel design: HIGH-MEDIUM — the master-addon double-subscription trap is verified by source inspection; the suppression mechanism (return false from `attachCustomKeyEventHandler`) is documented but Phase-2 integration test is the authoritative validation.
- Browser auto-open: HIGH — environment verified; `open` crate is stable and widely-used.
- COOP/COEP on WS upgrade: MEDIUM-HIGH — should work because axum's WS upgrade is a normal HTTP response and tower middleware applies; regression test pins it.
- Test plumbing (tokio-tungstenite): HIGH — matches the official axum example version-for-version.

**Research date:** 2026-05-18
**Valid until:** 2026-06-18 (30 days; the axum/tokio-tungstenite/open ecosystem is stable enough at these versions that nothing should drift in a month).
