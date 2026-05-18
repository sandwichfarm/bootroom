---
phase: 2
name: WebSocket + Live Serial
gathered: 2026-05-18
status: Ready for planning
mode: discuss
---

# Phase 2: WebSocket + Live Serial — Context

<domain>
## Phase Boundary

**Goal:** A user types into the browser terminal and sees their keystrokes reach the guest kernel; serial output streams back in real time through the architecture's load-bearing PTY-over-WS substrate.

**In scope (Phase 2):**
- `/ws` endpoint relaying tagged-JSON messages (SerialIn, SerialOut, State, Launch, Reset)
- Free-form keyboard input from xterm reaching the guest (remove the Phase-1 no-op handler; route through a client-side write funnel)
- Launch button → full page reload (re-fetches `/api/kernel/info` + `/kernel`, fresh qemu-wasm instance)
- Reset button → full page reload (same as Launch in Phase 2; in-place reset deferred until use justifies the complexity)
- xterm "clear" and "copy all" controls
- Status pill driven by Module lifecycle events + (when WS is connected) authoritative `State` messages from the server
- Auto-open browser on `bootroom serve`; `--no-open` opts out
- Configurable ~10–20 ms inter-character pacing on injected sequences (client-side throttle)
- `bootroom-core` gets the WS message enum (single source of truth, reused by Phase 4's headless `run`)

**Out of scope (later phases):**
- TOML config + action buttons + scenarios (Phase 3 / 4)
- `bootroom run` headless driver (Phase 4 — reuses the WS protocol shipped here)
- Kernel-path watcher / "fresher build" banner (Phase 3)
- In-place qemu reset without page reload (deferred; Spike A scaffolding stays, future phase may revisit when scrollback loss bites)
- `bootroom doctor` (Phase 5)
- crates.io publish / release binaries (Phase 6)

**Phase 2 requirements (from ROADMAP.md):** SERV-06, UI-02, UI-03, UI-04, UI-06, UI-08, UI-09, WS-01, WS-02, WS-03, WS-04

</domain>

<decisions>
## Implementation Decisions

### /ws message protocol — tagged JSON only

**Decision:** Single serde-tagged enum, JSON-on-the-wire, base64 for byte payloads.

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
- Browser side: bytes from `slave.onReadable()` get `btoa()`'d into `data`; incoming `SerialIn.data` gets `atob()`'d before being enqueued for the slave.
- Accept ~33% wire overhead for base64; the dev/CI traffic volume makes this irrelevant. Binary framing was rejected as premature optimization.
- Endpoint: `GET /ws` upgraded by axum's WebSocket extractor; one connection per page; server-side state is per-connection (no broadcast in Phase 2).

### Launch / Reset — both = full page reload

**Decision:** Both buttons trigger `window.location.reload()`.

- Browser re-instantiates qemu-wasm Worker fresh; re-fetches `/api/kernel/info` + `/kernel`; runs staticInit/preRun/main as on first load.
- `/ws` auto-reconnects after the reload (server is stateless per-connection).
- xterm scrollback resets across reload — accepted cost in Phase 2.
- In-place qemu reset (Spike A's mechanism + a `qemu_system_reset` wasm export) is deferred. Spike A's `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` documents the FS-swap path; a future phase can revisit when scrollback loss actually frustrates users.
- Both buttons send a `Launch` / `Reset` WS message *before* reloading, so the server can log the request (Phase 4 / scenario tooling can observe).

### Keyboard input + client-side write funnel

**Decision:** Client-side funnel; all host→guest writes funnel through a single in-order async writer.

- Remove Phase 1's `xterm.attachCustomKeyEventHandler(() => false)` from `app.js`.
- Replace with a write-funnel module (e.g., `web/funnel.js`): exposes `enqueue(bytes)`; drains via an async loop with configurable pacing (`pacingMs`, default 15ms between bytes for injected sequences, 0ms for user typing).
- xterm `onData` callback calls `funnel.enqueue(bytes, { pacingMs: 0 })` — user typing flows at native speed.
- WS-received `SerialIn` messages call `funnel.enqueue(decoded, { pacingMs: config.pacingMs })` — scenarios pace as configured.
- Funnel writes bytes to `slave.write(...)`. Single sender = no interleaving (satisfies WS-02 by construction).
- SerialOut mirror: xterm-pty's slave `onReadable` → read all available → emit `SerialOut` WS frame (batched per readable burst, not per byte) so CI can observe.
- Config knob: query string param `?pacing=20` overrides; default 15ms. Server doesn't currently care about the pacing value (client-side concern).

### Browser auto-open + status pill source

**Decision:** Use the `open` crate (Rust); default ON; `--no-open` opts out. Status pill: emscripten events provide local truth, WS `State` overrides when connected.

- Add `open = "5"` to workspace deps. Call `open::that_detached(url)` after `axum::serve` is listening. Failure (no browser, headless system) logs a warning and continues serving.
- CLI flag: `--no-open` (bool). Behavior matches Phase 1 today (which printed the URL) only when set.
- Status pill state machine:
  | State | Trigger |
  |---|---|
  | `Idle` | Initial render (before xterm + qemu init) |
  | `Loading` | After xterm mount, before `Module.onRuntimeInitialized` |
  | `Running` | `Module.onRuntimeInitialized` fired AND first SerialOut byte received (proves guest is actually executing, not just initialized) |
  | `Halted` | `Module.onExit` or `Module.onAbort`, OR server pushes `State{state: Halted}` over WS |
- When WS connects and server sends `State`, that becomes authoritative (server has the broader view across reconnects, scenarios, etc.).
- WS `Hello { version }` on connect — client validates version compatibility; mismatch logs a warning but proceeds (Phase 6 may tighten).

</decisions>

<canonical_refs>
## Canonical References

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md` (Phase 2 reqs: SERV-06, UI-02, UI-03, UI-04, UI-06, UI-08, UI-09, WS-01..04)
- `.planning/ROADMAP.md` (Phase 2 section + success criteria)
- `.planning/phases/01-walking-skeleton/01-SKELETON.md` (architecture committed in Phase 1)
- `.planning/phases/01-walking-skeleton/01-CONTEXT.md` (Phase 1 decisions Phase 2 builds on)
- `.planning/phases/01-walking-skeleton/01-UI-SPEC.md` (Phase 2 extends — same palette, +buttons, +clear/copy controls)
- `.planning/phases/01-walking-skeleton/01-RESEARCH.md` (verified versions: axum 0.8 WebSocket, tower-http 0.6)
- `.planning/phases/01-walking-skeleton/spikes/spike-a/SPIKE-A-RESULT.md` (in-place reset deferred; mechanism documented for future)
- `.planning/phases/01-walking-skeleton/01-REVIEW.md` (Phase 1 patterns to preserve: error handling, tracing, path canonicalization)
- `crates/bootroom-core/src/lib.rs` (where the WsMessage enum lands)
- `crates/bootroom/src/server.rs` (axum router; new `/ws` route)
- `crates/bootroom/web/app.js` (entry to extend)
- `crates/bootroom/web/vendor/VERSIONS.md` (xterm.js 5.3.0 + xterm-pty 0.12.0)
- `qemu-wasm/examples/riscv64/src/htdocs/index.html` (reference xterm-pty wiring already followed)

External (fetch on demand):
- axum 0.8 WebSocket docs: https://docs.rs/axum/0.8/axum/extract/ws/
- `open` crate: https://docs.rs/open/5
- xterm.js Terminal.onData: https://xtermjs.org/docs/api/terminal/classes/terminal/#ondata

</canonical_refs>

<code_context>
## Existing Code Insights

After Phase 1 the repo has:
- `crates/bootroom-core/src/lib.rs` — empty skeleton; this is where `WsMessage` and `GuestState` land
- `crates/bootroom/src/server.rs` — axum router; Phase 2 adds a `.route("/ws", get(ws_handler))` and a connection-state struct
- `crates/bootroom/src/cli.rs` — already has `ServeArgs` with `--port`, `--host`, `--assets-dir`; Phase 2 adds `--no-open` and optional `--pacing-ms`
- `crates/bootroom/web/app.js` — has the xterm + xterm-pty mount and the Module.FS swap. Phase 2 modifies in place to add WS, buttons, write funnel
- `crates/bootroom/web/index.html` — has #hdr, #status pill, #terminal. Phase 2 adds Launch + Reset buttons + Clear + Copy controls
- `crates/bootroom/web/style.css` — UI-SPEC palette already in CSS custom properties; new buttons use the same palette
- Test infra at `crates/bootroom/tests/common/mod.rs` — TestServer harness ready for WS integration tests

</code_context>

<specifics>
## Specific Ideas

- WS framing: axum's `WebSocketUpgrade` extractor + `WebSocket::send/recv`. Use a `tokio::sync::mpsc` channel per connection for the server-side write side so the WS task is single-writer.
- WS heartbeat: axum's WebSocket supports pings; rely on tokio-tungstenite's defaults (Phase 2). Tighten only if disconnects become an issue.
- The funnel implementation: `web/funnel.js` exports a singleton. Tests for it as a pure unit (no DOM, no slave).
- Launch/Reset buttons live in the header strip per UI-SPEC's noted Phase-2 design space: small mono icons or text buttons (`LAUNCH` / `RESET`). Position: right of kernel-info, left of status pill.
- Clear / Copy controls: small icons in the terminal frame (top-right of #terminal); Clear calls `xterm.clear()`, Copy reads `xterm.getSelection() || xterm.buffer.active.translateToString()` and writes to clipboard.
- Status pill new state `Idle` lands in this phase (UI-SPEC Phase 1 marked it deferred — Phase 1 had Loading/Running/Halted only). Use the same accent-amber color as Loading initially; switch to neutral grey if it visually conflicts.
- Server-side WS state: per-connection struct `ConnState { sender_to_browser, last_serial_out_at, ... }`. Hold via `Extension(state)` or dedicated `WsState` type.
- Test the WS round-trip: client connects → server sends Hello → client echoes a SerialIn → server emits SerialOut mirror. Integration test using `tokio-tungstenite` as the client driver.
- Add a one-liner `bootroom serve --no-open --kernel /tmp/Image` smoke to docs.

</specifics>

<deferred>
## Deferred Ideas

- **In-place qemu reset** (no page reload) — defer until Phase 3+; Spike A scaffolding remains, future plan can wire `qemu_system_reset` export.
- **TOML config + action buttons** — Phase 3.
- **Kernel-path watcher** — Phase 3.
- **Headless `bootroom run`** — Phase 4 (reuses the WS protocol shipped here).
- **WS reconnect with exponential backoff** — keep simple in Phase 2; if disconnects become a real problem, harden later.
- **Multi-tab support / concurrent WS connections** — out of scope; Phase 2 is local single-user.
- **Authentication on /ws** — bootroom is loopback-only; explicit non-goal in PROJECT.md.
- **Binary WS frames for serial bytes** — JSON-only chosen for Phase 2; revisit only if profiling shows wire overhead actually matters.

</deferred>
