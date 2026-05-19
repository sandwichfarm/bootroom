---
phase: 4
name: Scenario Engine + Headless `run`
researched: 2026-05-19
domain: browser-side scenario engine + chromiumoxide headless driver + assertion evaluator + CLI/exit-code surface
confidence: HIGH for the runtime stack (every piece is already shipped in-repo or proven by Spike B); MEDIUM for ANSI/line-buffer edge cases (specifics depend on real kernel output); MEDIUM for ScenarioResult outer-timeout tuning.
---

# Phase 4: Scenario Engine + Headless `run` — Research

## Summary

Phase 4 lands the second mode of the dual-mode harness: the same axum server, the same embedded assets, the same WS protocol, driven by a headless Chromium under chromiumoxide and exited 0/1 against serial-output assertions. Spike B [VERIFIED: `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` — verdict `green`, COI=true, SAB=true, RUNNING + 49 chars of serial in 15s] retired the single biggest unknown (`--headless=new` SAB reliability), so the driver design is mechanical rather than exploratory: spawn `AppState` + `build_router` on `127.0.0.1:0`, launch Chromium pointed at the bound URL with a `?scenario=<name>` query, await a `ScenarioResult` frame on the WS broadcast (or via a dedicated oneshot), translate the verdict to exit code.

The center of gravity is the **browser-side scenario engine** in a new `web/scenario.js`. It owns the action loop (resolves action labels via `/api/config`, enqueues each through the existing `Funnel` at `pacingMs: 15`, accumulates per-action serial buffers from `master.onWrite`), evaluates assertions (ANSI-stripped, line-buffered, substring or compiled regex), and emits a single `ScenarioResult` WS frame with the full transcript. The Rust side is a thin "exit-code translator": it doesn't need to understand assertions — only `pass | fail | timeout | error` plus the transcript JSONL events to persist.

Three new additive `WsMessage` variants land in `bootroom-core`: `ScenarioStart` (server → browser, optional — see decision below), `ScenarioResult` (browser → server), and `ScenarioAbort` (server → browser, defensive). The existing protocol additivity convention (Phase 3's `ConfigUpdate`/`ConfigInvalid`/`KernelChanged`) makes this a pure variant insert.

**Primary recommendation:** Detect run-mode from the URL query (`?scenario=<name>`) — no server-pushed bootstrap frame needed. The browser already fetches `/api/config` after WS Hello; it can resolve the scenario from that same payload. Use a `oneshot::Sender<ScenarioResult>` parked on `AppState` (filled by the WS handler when a `ScenarioResult` frame arrives, taken by the `run_cmd` driver to translate to exit code). Lift Spike B's launch incantation almost verbatim, with the addition of `$CHROMIUM` discovery and a Runtime-evaluate COI probe BEFORE waiting for the scenario.

## User Constraints (from CONTEXT.md)

### Locked Decisions

**Scenario Engine Architecture**
- Engine executes **browser-side** in `app.js` (or a new sibling module, e.g. `web/scenario.js`).
- Per-action serial buffer stored browser-side as a `Map<actionLabel, Uint8Array>`; resets on action start; accumulates `SerialOut` bytes during the action's match window.
- Results return via a new `ScenarioResult` `WsMessage` variant (browser → server) carrying overall verdict, per-action verdicts, per-assertion verdicts, full transcript. Additive — no existing wire shape changes.
- `bootroom run` exits **on `ScenarioResult` receipt**: server translates verdict to exit code, persists transcript to `--log-file` if set.

**Headless Driver (chromiumoxide)**
- Single in-process model: `bootroom run` spins up the axum server on `127.0.0.1:0`, launches chromiumoxide-driven Chromium pointing at that ephemeral URL, awaits `ScenarioResult`, shuts down.
- Chromium discovery: `$CHROMIUM` env → `/usr/bin/chromium` → PATH probe (`which chromium`) → exit 3 with hint message listing the searched candidates.
- COI self-check (RUN-10): evaluate `self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'` via CDP `Runtime.evaluate` BEFORE the scenario kicks off. Failure → exit 3 with the same fix-hint message the UI banner displays.
- Launch flags: `--headless=new`, `--no-sandbox`, `--disable-dev-shm-usage`, plus Spike B's working set; appendable via `$BOOTROOM_CHROMIUM_ARGS`.

**Assertion Matching Semantics**
- ANSI stripping (RUN-05): regex-strip `\x1b\[[0-9;]*[A-Za-z]` before matching; storage is still raw bytes; stripping happens at match time.
- Line buffering (RUN-05): match operates on lines accumulated up to the latest `\r?\n` boundary. Partial trailing lines are only considered at action-timeout fire.
- Regex flavor: Rust `regex` crate. Multiline mode is OFF by default. Patterns compile-checked at config load (Phase 3 — extend `Assertion::validate()` to compile-check regex patterns).
- `after` semantics: `after = "<action_label>"` → only that action's serial buffer is searched. `after = "any"` → union of all per-action buffers since scenario start (line-ordered as they arrived).

**CLI Surface & Output**
- Flags: `--kernel <path>`, `--config <path>`, `--scenario <name>`, `--verbose`, `--log-file <path>`. Shared `--kernel`/`--config`/`--verbose` via clap `#[flatten]` (CLI-02). `--host`/`--port` deliberately not exposed.
- Exit codes: `0` pass, `1` scenario fail (any assertion or timeout), `2` config/CLI error (invalid TOML, unknown scenario, bad flag), `3` startup error (SAB self-check failed, Chromium missing, ScenarioResult not received within outer timeout).
- `--log-file` format: JSONL — one event per line. Event types: `scenario_start`, `action_send`, `serial_chunk`, `assertion_result`, `scenario_result`. Each event has `ts` (ISO 8601), `type`, type-specific payload.
- `--verbose` (stderr): per-action progress (`▶ action: reboot`), assertion verdicts (`✓ assert: contains "login: "`), final summary. Non-verbose: silent on success; single-line failure summary on stderr.

### Claude's Discretion

All other implementation details: module layout, exact JSON event payloads, internal struct shapes, test-fixture choice, order of plan execution.

### Deferred Ideas (OUT OF SCOPE)

- `--report-format=junit` / `--report-format=github` (REP-01/02) — v2.
- `--watch` re-run loop (AUTH-01) — v2.
- Per-action keyboard shortcuts (AUTH-02) — v2.
- Screenshot button (AUTH-03), record-and-replay (AUTH-04) — v2.
- Snapshot/save-state actions — blocked on upstream qemu-wasm.
- Playwright subprocess fallback — Spike B retired this concern. Not implemented in Phase 4.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| RUN-01 | `bootroom run --kernel <path> --scenario <name>` runs headlessly, exits 0/non-zero. | "Headless `run` Driver" + "Exit-Code Translation Table" below. |
| RUN-02 | Drive headless Chromium via `chromiumoxide` (Playwright fallback documented). | Spike B verdict `green` [VERIFIED: `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md`]; "Chromium Launch Patterns" lifts Spike B's incantation. |
| RUN-03 | Same embedded assets and same WS protocol as `serve` mode — no separate CI code path. | "Reuse Surface" section — `build_router(state)` shared verbatim; only the bootstrap (CLI dispatch) and tail (oneshot await + exit code) differ. |
| RUN-04 | Assertions support substring and anchored regex against per-action serial buffers. | "Assertion Evaluator" section — `AssertionKind::{Contains, Regex}` already in `bootroom-core::config` since Phase 3 [VERIFIED: `crates/bootroom-core/src/config.rs:71-76`]. |
| RUN-05 | ANSI escape sequences stripped before matching; line-buffered (`\r?\n`) output. | "ANSI Stripping + Line Buffering" section — single regex `\x1b\[[0-9;]*[A-Za-z]`. |
| RUN-06 | Per-action and per-scenario timeouts with explicit defaults; structured failures. | "Timeout Machinery" — `default_scenario_timeout = 30_000` ms, `default_assertion_timeout = 5_000` ms already shipped [VERIFIED: `crates/bootroom-core/src/config.rs:78-84`]. |
| RUN-07 | Per-action serial buffer reset by default (configurable carry-over). | "Per-Action Buffer Semantics" — default reset; carry-over via a future per-scenario flag (schema-additive). |
| RUN-08 | `--log-file <path>` writes full transcript (timestamps, action sends, serial output, assertion results). | "JSONL Log Format" section — five event types future-proof for JUnit shim. |
| RUN-09 | `--verbose` prints scenario progress to stderr for CI logs. | "stderr/verbose Output Contract" section. |
| RUN-10 | `crossOriginIsolated` startup self-check; abort early with clear message if SAB unavailable. | "COI Self-Check (RUN-10)" — CDP `Runtime.evaluate` BEFORE scenario kickoff; same hint string as `index.html` iso-banner. |
| CLI-02 | Common flags (`--kernel`, `--config`, `--verbose`) shared via clap `#[flatten]`. | "CLI Surface" section — extract `SharedArgs` struct with these three fields; `ServeArgs` and `RunArgs` each `#[command(flatten)]` it. |

## Project Constraints (from CLAUDE.md)

- **Single-binary, no Node.js runtime:** all assets embedded via `include_dir!`; the `run` driver must NOT shell out to Node. Chromium discovery falls back to system binary; if missing, exit 3 with a clear hint.
- **Vanilla JS / no build step:** `web/scenario.js` ships as a plain ES module imported by `app.js` (or dynamically via `import('./scenario.js')`). No bundler. No npm install.
- **License MIT OR Apache-2.0:** `chromiumoxide` (Apache-2.0/MIT) and `regex` (Apache-2.0/MIT) both clear; `cargo-deny` will pass.
- **GSD workflow:** all edits go through `/gsd-execute-phase`; no direct repo modifications outside the workflow.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Scenario action sequencer | Browser / Client (`web/scenario.js`) | — | Browser owns `Funnel` (the only path to guest stdin) and `master.onWrite` (the only path that observes guest output); colocating the engine eliminates a WS round-trip per action and per serial chunk. |
| Per-action serial buffer accumulation | Browser / Client | — | Same rationale: `master.onWrite` is the source. Server only sees the final `ScenarioResult` JSON. |
| Assertion evaluation (substring / regex / ANSI strip / line buffer) | Browser / Client | — | All inputs live in the browser; pushing bytes to the server to evaluate would just round-trip a serial transcript that's already in memory. |
| Regex compilation pre-flight | Browser / Client (`new RegExp`) + Backend (`bootroom check` validation via Rust `regex` crate at config load) | — | Phase 3 already compiles via Rust at load time (decision); the browser uses JS `RegExp` for runtime match. Mismatch risk is real — see "Pitfall: regex flavor drift". |
| Scenario verdict → exit code translation | API / Backend (`run_cmd::run`) | — | Process-exit is a Rust concern. |
| Headless Chromium launch + COI probe + `ScenarioResult` await | API / Backend (`chromiumoxide`) | Browser (driven via CDP) | Lift Spike B's pattern. |
| JSONL transcript persistence | API / Backend | — | File I/O is naturally on the Rust side; the browser ships the transcript as a JSON payload on the `ScenarioResult` frame. |
| `--verbose` stderr streaming | API / Backend | — | The server already routes WS frames; verbose mode tees `action_send` / `assertion_result` events to stderr as they arrive. |
| Shared `--kernel` / `--config` / `--verbose` flags | API / Backend (clap `#[command(flatten)]`) | — | CLI-02 mechanic. |
| URL-query run-mode detection (`?scenario=`) | Browser / Client | API / Backend (no involvement) | The driver appends `?scenario=<name>` when navigating; the browser parses `URLSearchParams` after WS Hello and triggers the engine. |

## Standard Stack

### Core (additions for Phase 4 only — existing deps unchanged)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| chromiumoxide | 0.9.1 | Headless Chromium CDP driver in `crates/bootroom` (promoted from Spike B) | Spike B proved 0.9.1 against system Chromium 148; the API surprises (no `tokio-runtime` feature, `HeadlessMode` not re-exported, `BrowserConfig` fields all `pub(crate)`) are already documented [VERIFIED: `.planning/phases/01-walking-skeleton/01-08-SUMMARY.md` "chromiumoxide 0.9.x surprises"]. |
| futures | 0.3.x | `StreamExt::next` on the CDP handler stream | Required by chromiumoxide examples; already used in Spike B `Cargo.toml` [VERIFIED: `crates/bootroom/spikes/spike-b/Cargo.toml:18`]. |
| regex | 1.12.x | Compile-check `Assertion { kind: Regex, pattern }` at config load | `regex` is already transitively present in the lockfile [VERIFIED: `grep '^name = "regex"' Cargo.lock` → 1.12.3]; promote to direct dep. Phase 4 only uses the *compile* side at load time — the actual runtime matching is JS `RegExp` in the browser (see "Pitfall: regex flavor drift"). |

**Installation (workspace `Cargo.toml` and `crates/bootroom/Cargo.toml`):**

```toml
# workspace.dependencies
chromiumoxide = { version = "0.9.1", default-features = false }
futures = "0.3"
regex = "1"

# crates/bootroom/Cargo.toml [dependencies]
chromiumoxide.workspace = true
futures.workspace = true
regex.workspace = true

# crates/bootroom-core/Cargo.toml [dependencies]
# (regex used at validate() time for pattern pre-compilation)
regex.workspace = true
```

**Version verification:** `chromiumoxide 0.9.1` is the Spike B-verified version; do not bump in Phase 4 — risk of re-encountering the 0.9.x API surprises on a fresher minor. `regex 1.x` is mature; `^1` is fine.

### No new browser-side libraries

The scenario engine is plain ES module + native `URLSearchParams` + native `RegExp` + the existing `Funnel` + the existing `master.onWrite` subscription pattern. No vendored JS additions.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Browser-side engine | Server-side engine driven via WS | Server-side would require streaming every `SerialOut` chunk to the server, evaluating per-chunk, and sending `SerialIn` ScenarioStart frames back. That's roughly the same wire shape but adds 2× round-trips per action and forces re-implementing ANSI-strip + line-buffer in Rust. The buffer already exists in the browser as `master.onWrite` data; collocating the engine there eliminates the round-trip. CONTEXT locks this. |
| `oneshot::Sender<ScenarioResult>` on `AppState` | `mpsc<WsMessage>` consumed by `run_cmd` | A `oneshot` is the natural shape: one scenario per `run` invocation; the channel fires exactly once. An `mpsc` would work too but `ws.rs` would need to know to forward only the one `ScenarioResult` rather than every frame. |
| URL-query run-mode detection (`?scenario=foo`) | Server-pushed `ScenarioStart` frame on WS connect | Query is **strictly simpler**: the browser already reads `URLSearchParams` for `?pacing=N` [VERIFIED: `crates/bootroom/web/app.js:503`]. A server-push approach requires `AppState` to carry the scenario name and the WS handler to push it just after `Hello`, which is more state and one more wire shape. **Recommend query string; keep `ScenarioStart` reserved as a defensive future option in the enum but unused in Phase 4 unless plan-checker flags a need.** |
| chromiumoxide | Playwright (Node subprocess) | Retired by Spike B verdict `green`. Re-introducing Playwright costs a Node dep on every CI runner — not worth it. |
| Polling for `crossOriginIsolated` after navigation | CDP `Runtime.evaluate` synchronously after `wait_for_navigation()` | Spike B [VERIFIED: `crates/bootroom/spikes/spike-b/src/main.rs:187-189`] already uses `page.evaluate("self.crossOriginIsolated")` after `wait_for_navigation()`. Same pattern. |

## Architecture Patterns

### System Architecture Diagram

```
                  bootroom run --kernel X --scenario boot_smoke
                                  │
                                  ▼
                   ┌──────────────────────────────────┐
                   │ run_cmd::run(args)               │
                   │  - load+validate config (Cmd 1)  │
                   │  - look up scenario by name      │
                   │    → exit 2 if unknown           │
                   │  - bind 127.0.0.1:0 + spawn axum │
                   │  - install oneshot on AppState   │
                   │  - launch chromiumoxide          │
                   │  - probe COI via CDP eval        │
                   │    → exit 3 if SAB unavailable   │
                   └──────────────────────────────────┘
                                  │
            navigate to http://127.0.0.1:<eph>/?scenario=boot_smoke
                                  │
                                  ▼
                   ┌──────────────────────────────────┐
                   │ Browser (headless Chromium)      │
                   │  app.js boots qemu-wasm normally │
                   │  WS Hello → fetch /api/config    │
                   │  detect ?scenario= → dyn-import  │
                   │  ./scenario.js + start engine    │
                   └──────────────────────────────────┘
                                  │
                                  ▼
                   ┌──────────────────────────────────┐
                   │ scenario.js engine               │
                   │  funnel.lockInput()              │
                   │  for each action in seq:         │
                   │    reset per-action buffer       │
                   │    funnel.enqueue(bytes,         │
                   │      {pacingMs: 15})             │
                   │    accumulate master.onWrite     │
                   │    evaluate assertions against   │
                   │      this buffer (after=lbl)     │
                   │      OR against union buffer     │
                   │      (after="any")               │
                   │    per-action timeout fires      │
                   │      → fail + structured payload │
                   │  funnel.unlockInput()            │
                   │  ws.send(ScenarioResult{...})    │
                   └──────────────────────────────────┘
                                  │
                                  ▼
                   ┌──────────────────────────────────┐
                   │ ws.rs handle_wire (extended)     │
                   │  match WsMessage::ScenarioResult │
                   │    → state.scenario_result_tx    │
                   │       .take() & .send(result)    │
                   └──────────────────────────────────┘
                                  │
                                  ▼
                   ┌──────────────────────────────────┐
                   │ run_cmd::run (waiting)           │
                   │  result = oneshot.await          │
                   │  persist JSONL (if --log-file)   │
                   │  print verbose summary (stderr)  │
                   │  translate verdict → exit code   │
                   │  browser.close().await           │
                   │  std::process::exit(code)        │
                   └──────────────────────────────────┘
```

### Pattern 1: URL-query run-mode detection

**What:** `app.js` reads `URLSearchParams` for `?scenario=<name>`. If present, after WS Hello (and after `initialConfigLoad` resolves), dynamically import `./scenario.js` and start the engine with the resolved scenario object.

**When to use:** Always — this is the only run-mode entry point in Phase 4.

**Example:**

```javascript
// inside handleWsFrame, Hello case, after initialConfigLoad():
const params = new URLSearchParams(location.search);
const scenarioName = params.get('scenario');
if (scenarioName) {
  // Resolve from the config we just fetched (cache it on a closure).
  // initialConfigLoad already populated the action buttons — we know /api/config worked.
  const cfg = await fetch('/api/config').then(r => r.json());
  const scenario = (cfg.scenarios || []).find(s => s.name === scenarioName);
  if (!scenario) {
    // Browser cannot exit 2 on its own; emit ScenarioResult with a structured error
    // so the server translates it. The Rust side ALSO validates this up-front, so
    // this branch is defense-in-depth only.
    ws.send(JSON.stringify({
      type: 'ScenarioResult',
      verdict: 'error',
      error: `unknown scenario '${scenarioName}'`,
      transcript: []
    }));
    return;
  }
  const { runScenario } = await import('./scenario.js');
  runScenario(scenario, cfg.actions, { ws, funnel, master });
}
```

### Pattern 2: per-action serial buffer via `master.onWrite`

`master.onWrite` is the only path that observes guest → host bytes [VERIFIED: `crates/bootroom/web/app.js:683-696`]. The Phase 2 SerialOut mirror is already subscribed. Add a second subscriber for the scenario engine: each chunk gets appended to a `Map<actionLabel, Uint8Array[]>` keyed by the **currently-executing action**. Per-action buffer is reset at action start (RUN-07 default).

**Example:**

```javascript
// scenario.js:
const buffers = new Map();   // label -> Uint8Array[]
let currentLabel = null;     // null between actions

const unsub = master.onWrite(([bytes, _ack]) => {
  // _ack is NOT called — Phase 2 SerialOut listener also doesn't call it.
  // xterm.write's own listener (wired by app.js) owns the ack.
  if (currentLabel === null || !bytes || bytes.length === 0) return;
  const chunks = buffers.get(currentLabel) || [];
  chunks.push(new Uint8Array(bytes));
  buffers.set(currentLabel, chunks);
});
```

### Pattern 3: ANSI strip + line-buffered match

**ANSI regex:** `/\x1b\[[0-9;]*[A-Za-z]/g` (matches CSI sequences — covers cursor moves, SGR color codes). Does NOT strip OSC sequences (`\x1b]...\x07`) — kernels rarely emit these on serial. If a kernel does, we surface as a Phase-5 gap, not a Phase-4 blocker.

**Line buffering rule (RUN-05):** concatenate chunks into a UTF-8 string (`TextDecoder('utf-8', {fatal: false})`), strip ANSI, then match against the substring up to the last `\r?\n`. Partial trailing lines are *only* matched at action timeout.

**Example:**

```javascript
function evaluate(buffer, assertion, atTimeout) {
  const raw = new TextDecoder('utf-8', {fatal: false}).decode(buffer);
  const stripped = raw.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
  const lastNl = stripped.lastIndexOf('\n');
  const matchTarget = (atTimeout || lastNl === -1)
    ? stripped                         // include partial trailing line at timeout
    : stripped.slice(0, lastNl + 1);   // line-bounded otherwise
  if (assertion.kind === 'contains') {
    return matchTarget.includes(assertion.pattern);
  }
  // 'regex': compile once at scenario start, not per-evaluate
  return assertion._compiled.test(matchTarget);
}
```

### Pattern 4: oneshot-based scenario completion on `AppState`

**Server-side**, the `AppState` gains an optional `scenario_result_tx: Mutex<Option<oneshot::Sender<ScenarioResult>>>` (a `Mutex<Option<…>>` because `oneshot::Sender` is `!Sync` and consumed by `.send()`).

```rust
// state.rs (additive — no existing tests change shape):
use tokio::sync::Mutex;

pub struct AppState {
    // … existing fields …
    /// Phase 4 RUN-01: when `bootroom run` is the active dispatch, this
    /// holds a oneshot sender awaited by `run_cmd::run`. The WS handler
    /// fills it when it receives a `ScenarioResult` frame. `Mutex<Option<_>>`
    /// because `oneshot::Sender::send` consumes the sender by value.
    pub scenario_result_tx: Mutex<Option<oneshot::Sender<bootroom_core::ScenarioResult>>>,
}
```

The `serve` path leaves `scenario_result_tx = None`; the `run` path installs `Some(tx)` before launching the browser and `rx.await`s.

**Inside `ws.rs::handle_wire`**, a new match arm:

```rust
WsMessage::ScenarioResult { verdict, transcript, actions, error } => {
    let mut guard = state.scenario_result_tx.lock().await;
    if let Some(tx) = guard.take() {
        let _ = tx.send(ScenarioResult { verdict, transcript, actions, error });
    } else {
        // serve-mode received a ScenarioResult — log only.
        tracing::warn!("ScenarioResult received in serve mode; ignoring");
    }
}
```

### Recommended Project Structure (additive)

```
crates/
├── bootroom-core/src/
│   ├── lib.rs              # extend WsMessage with 3 variants (additive)
│   └── config.rs           # extend Assertion::validate() to compile regex
├── bootroom/src/
│   ├── cli.rs              # add Cmd::Run(RunArgs); extract SharedArgs (CLI-02)
│   ├── main.rs             # dispatch Cmd::Run → run_cmd::run
│   ├── run_cmd.rs          # NEW — headless driver
│   ├── state.rs            # add scenario_result_tx field
│   └── ws.rs               # extend handle_wire match for ScenarioResult
└── bootroom/web/
    ├── app.js              # detect ?scenario=, dyn-import scenario.js
    └── scenario.js         # NEW — engine
```

### Anti-Patterns to Avoid

- **Putting the engine in `app.js`.** It's already 921 lines; the engine is its own concern (action sequencer + buffer accumulator + assertion evaluator + result emitter). Keep them separate. CONTEXT permits both ("`app.js` or a new sibling module"); the sibling module is the right call.
- **Forwarding every `SerialOut` over WS to the server for scoring.** The server already has `SerialOut` for logging purposes (Phase 2), but doubling that traffic just to evaluate assertions on the Rust side trades a clean architectural boundary for nothing — the browser has the bytes already.
- **Calling `_ack` from the scenario engine's `master.onWrite` listener.** [VERIFIED: `crates/bootroom/web/app.js:680-682`] — xterm's own listener owns the ack. Multiple listeners observe; only one acks.
- **Re-implementing escape-byte decoding on the browser side.** `/api/config` already projects `bytes_b64` per action [VERIFIED: `crates/bootroom/src/watcher.rs:108-122`]; `b64ToBytes` already exists in `funnel.js`. The engine just looks up `actions.find(a => a.label === ref).bytes_b64` per scenario step.
- **`include_dir!` in tests for `web/scenario.js`.** The asset pipeline already includes everything under `web/`; tests just exercise via headless Chromium against the running server. No special test plumbing needed.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Compiled regex pattern with line-anchors | A custom NFA / a string-search-with-anchor-detection | JS `RegExp` (browser) + Rust `regex` (server validate-only). Pre-compile once per scenario start; cache on the assertion object. | Battle-tested; documented anchor semantics. |
| ANSI escape stripping | Hand-rolled state machine | `String.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '')` | Covers CSI sequences (color codes + cursor moves); the only forms in practice on a kernel serial console. Pure regex; no DFA library. |
| Line-buffering on a streaming buffer | Custom growable byte buffer + last-CR tracker | `Uint8Array[]` concat + `lastIndexOf('\n')` at evaluate time | Simpler than tracking state per-chunk; `evaluate` is called at most a few times per second. |
| Headless browser process management | Hand-rolled `Command::new` + IPC to a custom Chrome harness | `chromiumoxide 0.9.1` | Spike B proved 0.9.1 against system Chromium 148; the API surprises are documented; this is the load-bearing dep. |
| Per-action timeout cancellation | Manual `setTimeout` + cancel state | `Promise.race([actionLoop, timeoutPromise])` | Idiomatic JS; the engine awaits the race result. The losing branch's setTimeout becomes a no-op when its outer promise is no longer awaited; engine logs at action-completion that the timeout did/didn't fire. |
| JSONL writer | Manual `serde_json::to_string` + `\n` per event | Manual `serde_json::to_string` + `\n` per event | Actually, this **is** hand-rolled — but the surface area is one `writeln!(file, "{}", json)` call per event; importing a library would be larger. |
| Exit-code translation | A switch + magic numbers | `enum ExitCode { Pass = 0, ScenarioFail = 1, ConfigError = 2, StartupError = 3 }` + a single mapping function | Keeps the magic numbers in one place; the planner / test file pins them. |
| Chromium binary discovery | A custom search across `$PATH` | Three-step resolution: `$CHROMIUM` env → `/usr/bin/chromium` → `which chromium` (via `std::process::Command` + `which` crate OR plain `Command::new("which").arg("chromium")`). | The `which` crate is 7 KB and stable but adds a dep; the bare `Command::new("which")` works on macOS + Linux. CONTEXT specifies the three-step probe. |
| ScenarioResult struct definitions | Inline JSON `Value` blobs | Concrete `ScenarioResult { verdict: Verdict, actions: Vec<ActionResult>, transcript: Vec<TranscriptEvent>, error: Option<String> }` in `bootroom-core` | Single source of truth (Phase-3 convention); roundtrip-tested next to the existing WsMessage variant tests. |

**Key insight:** Phase 4 is a **composition phase**, not a primitives phase. The hard pieces (single-writer funnel, COI bootstrap, chromiumoxide launch, watcher debounce) all shipped in Phases 1–3 or in Spike B. The new code is glue + a small JS engine + a clap subcommand. Resist the temptation to "improve" pieces that already ship working — they have integration tests behind them.

## Common Pitfalls

### Pitfall 1: Regex flavor drift (server validates, browser executes)

**What goes wrong:** Phase 3's `Assertion::validate()` extension compiles patterns with the Rust `regex` crate (decision in CONTEXT). The browser then executes the same pattern via JS `RegExp`. The two engines have different feature sets:

- Rust `regex` does NOT support backreferences or lookaround.
- JS `RegExp` does support both.
- Anchor semantics differ only with multiline flag (we keep multiline OFF, so `^`/`$` behavior is identical — string start / string end).

**Why it happens:** Developer writes `"contains lookahead (?=foo)"` regex in TOML; Rust rejects it at load → `bootroom check` fails. Good. But the inverse direction is also possible: developer writes a regex JS accepts but Rust rejects — same outcome, load fails cleanly. The actual drift case is **a regex Rust accepts that JS interprets slightly differently** (Unicode handling on `\w`, for example).

**How to avoid:**
- Document explicitly that Phase 4 supports the *intersection* of Rust `regex` and ECMAScript `RegExp`: no backreferences, no lookaround, no `\w`/`\d` reliance on locale.
- In `bootroom check`, validate that the pattern compiles in Rust `regex`. The JS side will accept any pattern Rust accepts (Rust `regex` is the *stricter* engine for these features).
- In `web/scenario.js`, catch `RegExp` construction failures and report as `verdict: error` (defense in depth — this should never fire after Phase 3's validate step).

**Warning signs:** A `bootroom check` pass followed by a runtime regex failure in the browser. The transcript will show `assertion_result { verdict: error, reason: "regex compile failed in browser" }`. Treat as a planner-level miss and add the offending pattern to the docs.

### Pitfall 2: Browser navigation breaks the WS before `ScenarioResult` lands

**What goes wrong:** The scenario engine finishes, builds the `ScenarioResult` payload, calls `ws.send(...)`. The frame is queued in the WebSocket's outbound buffer. **Then** something (a kernel `Module.onExit`, a user click in headed-debug mode, an emscripten worker crash) triggers `location.reload()` or kills the document. The browser tears down the WS connection before the frame leaves the wire. The server's `oneshot::Receiver` never fires; the outer timeout in `run_cmd` eventually trips → exit 3 with a misleading "ScenarioResult not received" message even though the scenario passed.

**Why it happens:** WS-level flush is asynchronous; `ws.send()` returns immediately. Phase 2's Launch button works around this with `requestAnimationFrame` before reload [VERIFIED: `crates/bootroom/web/app.js:716-726`].

**How to avoid:**
- Before scenario start, install a `beforeunload` no-op (or active warning) so unintentional navigations during scenario run are at least logged.
- Use the WebSocket's `bufferedAmount` to poll until 0 after `ws.send(scenarioResultFrame)`, then resolve a `Promise` — only then is the scenario engine "done". The server's oneshot will have fired by that point.
- Alternative: don't reload. Spike A's `chosen_path: module-fs-write` is for `serve` mode Launch; in `run` mode the scenario shouldn't trigger any reload. If a scenario *requires* a kernel re-boot mid-sequence, that's a v2 concern (snapshot/save-state is already deferred).
- Outer timeout in `run_cmd` should be generous enough (scenario `timeout_ms` + 30s buffer) to cover slow WS flushes, but the diagnostic on timeout should distinguish "no ScenarioResult arrived" from "Chromium crashed" (CDP handler stream EOF = crash).

**Warning signs:** Intermittent flaky exits with exit code 3 on a scenario that should pass.

### Pitfall 3: `funnel.lockInput()` self-blocks the scenario

**What goes wrong:** Phase 3 [VERIFIED: `crates/bootroom/web/funnel.js:117-127`] designed the lock to fire `_lockObserver(true)` (which flips the pill to BUSY and disables `.action-btn`s in `app.js`) but `funnel.enqueue` is **lock-agnostic** by design: "server-initiated `SerialIn` frames (i.e. the scenario engine's own writes) must keep flowing during a lock; otherwise the engine would self-block." The scenario engine calls `funnel.enqueue` directly (not via WS round-trip), so the same exemption applies.

**Why it happens:** A future contributor reads the funnel doc, sees "input is locked," and adds a guard inside `funnel.enqueue` itself. This silently breaks the scenario engine.

**How to avoid:**
- Keep the contract documented in `funnel.js` (it already is) and reaffirm it in `scenario.js`: "calls `funnel.enqueue` directly; the lock is for *user-initiated* enqueues only."
- Add a regression test (subprocess test against a small scenario) that proves the lock state doesn't block engine enqueues.

**Warning signs:** A scenario hangs on its first action with no `serial_chunk` events in the transcript — bytes never reach the guest.

### Pitfall 4: `master.onWrite` listener leak across reconnects

**What goes wrong:** If the WS reconnects mid-scenario (network blip, server restart), the engine's `master.onWrite` subscription is *separate* from the WS — it survives. But the engine's `runScenario` function returns early on WS close, leaking the subscription. Next scenario run double-subscribes.

**Why it happens:** xterm-pty's `onWrite` returns a `Disposable` (an object with `.dispose()`). The Phase 2 `SerialOut` mirror doesn't call `.dispose()` either, but it's only subscribed once per page load.

**How to avoid:** In `scenario.js`, capture the disposable at subscription time and call `disposable.dispose()` in a `finally` block. The engine should be designed to run exactly once per page load (matching `bootroom run`'s one-scenario-per-invocation contract); but defending against multi-run is cheap.

**Warning signs:** Per-action buffer entries from a prior scenario showing up under the current scenario's labels.

### Pitfall 5: `after = "any"` ordering ambiguity

**What goes wrong:** CONTEXT specifies `after = "any"` → "union of all per-action buffers since scenario start (line-ordered as they arrived)." But the per-action `Map<label, Uint8Array[]>` shape is keyed; it doesn't preserve cross-action line order. Naively concatenating all values gives action-order, not line-arrival-order.

**Why it happens:** The implementer reaches for the `Map` they already have.

**How to avoid:** Maintain a **secondary** flat append-only `Uint8Array[]` of all chunks alongside the per-action `Map`. When `after = "any"`, evaluate against this flat buffer. When `after = "<label>"`, evaluate against the per-action `Map.get(label)`. Both structures are cheap.

**Warning signs:** Regex matches at unexpected positions; substring assertions fail intermittently when an earlier action's late-arriving serial output races a later action's start.

### Pitfall 6: Chromium binary discovery races with `/usr/bin/chromium` symlinks

**What goes wrong:** `$CHROMIUM` → `/usr/bin/chromium` → `which chromium` is the documented probe order. But on systems where `/usr/bin/chromium` exists as a stale symlink to a missing target, the second step succeeds at the existence check and fails at launch. The third step (`which`) is never tried.

**Why it happens:** Spike B used a literal `/usr/bin/chromium` constant. The Phase 4 driver should `Path::exists()`-check, not just trust the constant.

**How to avoid:** For each candidate, do `Command::new(&candidate).arg("--version").output()` and verify the exit code is zero. The candidate that responds is the one to use. Fall through to the next candidate on any failure (exec error, non-zero exit). If all three fail, exit 3 with a hint message listing every candidate tried + the error per candidate.

**Warning signs:** Exit 3 with `Browser::launch failed` and no hint pointing to `$CHROMIUM` (regression vs. CONTEXT specifics).

### Pitfall 7: `chromiumoxide 0.9.1` API surprises (already documented)

**What goes wrong:** Three surprises from Spike B that re-bite if Phase 4 copies older sample code: (1) `tokio-runtime` feature is gone; (2) `HeadlessMode` enum not re-exported; (3) `BrowserConfig` fields all `pub(crate)`.

**How to avoid:** Lift Spike B's working incantation verbatim [VERIFIED: `crates/bootroom/spikes/spike-b/src/main.rs:135-154`]. Do not rewrite from chromiumoxide docs — they may reflect a different minor.

**Warning signs:** Cargo errors mentioning `HeadlessMode` or `tokio-runtime` feature.

### Pitfall 8: `outer timeout` swallowing real failures

**What goes wrong:** The outer timeout in `run_cmd` is the safety net for "ScenarioResult never arrives." Set too tight (e.g., 30s) it triggers on legitimately slow boots; set too loose (e.g., 5min) it makes CI debugging painful when a scenario hangs.

**How to avoid:** Compute outer timeout from the **scenario's own `timeout_ms`** (already in the config) plus a generous buffer (e.g., `scenario.timeout_ms + 30_000` ms) covering: kernel boot, COI probe wait, WS roundtrip latency, and ScenarioResult send. Distinguish the timeout error: if no `serial_chunk` events were ever observed, blame "kernel never booted"; if events were observed but the result frame is missing, blame "ScenarioResult not received."

**Warning signs:** Exit 3 with a generic timeout message and no actionable hint.

## Runtime State Inventory

Phase 4 is a **greenfield additive phase** — it adds new files (`run_cmd.rs`, `scenario.js`), new variants (3× `WsMessage`), a new field on `AppState`, and new CLI dispatch. **No rename or migration is involved.** No runtime state inventory is required.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — verified by reading `state.rs`, `watcher.rs`, `ws.rs`; no persistent storage exists. | None |
| Live service config | None — verified by reading `cli.rs`; only env-var-like inputs (`RUST_LOG`) and new `$CHROMIUM`/`$BOOTROOM_CHROMIUM_ARGS`. | None |
| OS-registered state | None — bootroom is a one-process binary; no systemd unit, no cron, no Task Scheduler. | None |
| Secrets/env vars | New env vars `$CHROMIUM` (chromium binary path override) and `$BOOTROOM_CHROMIUM_ARGS` (additional flags). Document in `--help` or README; no secrets handled. | None |
| Build artifacts | None — `cargo build --workspace` rebuilds cleanly. | None |

## Code Examples

Verified patterns from in-repo sources.

### Chromium launch (lift from Spike B verbatim, parameterize executable)

```rust
// crates/bootroom/src/run_cmd.rs
// Source: crates/bootroom/spikes/spike-b/src/main.rs:135-154
use chromiumoxide::{Browser, BrowserConfig};
use std::path::PathBuf;

async fn launch_chromium(executable: PathBuf, extra_args: Vec<String>) -> Result<Browser> {
    let mut builder = BrowserConfig::builder()
        .chrome_executable(executable)
        .new_headless_mode()
        .no_sandbox()
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage");
    for arg in extra_args {
        builder = builder.arg(arg);
    }
    let config = builder.build()
        .map_err(|e| anyhow::anyhow!(
            "BrowserConfig::build failed: {e}. \
             Set $CHROMIUM to override the chromium binary path."
        ))?;
    let (browser, mut handler) = Browser::launch(config).await
        .map_err(|e| anyhow::anyhow!(
            "Browser::launch failed: {e}. Check $CHROMIUM and that the binary is executable."
        ))?;
    // Drain the CDP event stream so commands don't deadlock.
    tokio::spawn(async move {
        while let Some(_event) = handler.next().await {}
    });
    Ok(browser)
}
```

### COI self-check via CDP `Runtime.evaluate` (RUN-10)

```rust
// Source: crates/bootroom/spikes/spike-b/src/main.rs:187-189 (eval_bool helper)
async fn coi_self_check(page: &chromiumoxide::Page) -> Result<()> {
    let coi: bool = page
        .evaluate("self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'")
        .await?
        .into_value::<bool>()
        .unwrap_or(false);
    if !coi {
        anyhow::bail!(
            "crossOriginIsolated/SAB unavailable in headless Chromium. \
             This usually means COOP/COEP headers are missing — check the / response, \
             and re-run after confirming Chromium ≥118 (--headless=new). \
             See /assets/web/app.js iso-banner for the in-browser hint."
        );
    }
    Ok(())
}
```

### Shared args with clap `#[command(flatten)]` (CLI-02)

```rust
// crates/bootroom/src/cli.rs (extension)
use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Common flags shared across `serve` and `run`. CLI-02.
#[derive(Debug, Args, Clone)]
pub struct SharedArgs {
    /// Path to the kernel image to load into the guest.
    #[arg(long, value_name = "PATH")]
    pub kernel: PathBuf,

    /// Path to bootroom.toml; default = ./bootroom.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Verbose progress output to stderr (RUN-09; ignored by `serve`).
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

#[derive(Debug, Args, Clone)]
pub struct RunArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    /// Name of the scenario in `bootroom.toml` to execute.
    #[arg(long, value_name = "NAME")]
    pub scenario: String,

    /// Write a full JSONL transcript to this path (RUN-08).
    #[arg(long, value_name = "PATH")]
    pub log_file: Option<PathBuf>,
}
```

`ServeArgs` is refactored to also `#[command(flatten)] shared: SharedArgs` for `--kernel`/`--config`/`--verbose`; its `--host`/`--port`/`--assets-dir`/`--no-open`/`--action` remain serve-only.

### Additive WsMessage variants (Phase-3 convention)

```rust
// crates/bootroom-core/src/lib.rs — append to WsMessage enum
// Convention source: crates/bootroom-core/src/lib.rs:32-75 (Phase 3 additions)

/// Server -> client. Defensive cancellation; Phase 4 does not require it
/// but reserves the variant in case future timeouts need to signal the engine.
ScenarioAbort { reason: String },

/// Browser -> server. Final scenario verdict + full transcript. The server's
/// `bootroom run` driver awaits this frame on a `oneshot::Receiver` and
/// translates `verdict` to a process exit code. Schema:
///
/// - `verdict`: "pass" | "fail" | "timeout" | "error"
/// - `scenario`: the scenario name as run
/// - `started_at` / `ended_at`: ISO 8601 wall-clock timestamps (browser-local)
/// - `actions`: per-action verdicts + per-assertion verdicts
/// - `transcript`: ordered JSONL-style event list (same shape as --log-file)
/// - `error`: optional structured message for verdict="error" or "timeout"
ScenarioResult {
    verdict: String,
    scenario: String,
    started_at: String,
    ended_at: String,
    actions: serde_json::Value,
    transcript: serde_json::Value,
    error: Option<String>,
},

// `ScenarioStart` is RESERVED but NOT shipped in Phase 4 — URL-query
// detection is sufficient. If a future need surfaces (server-driven
// scenario re-execution mid-session), add it then.
```

Roundtrip tests follow the Phase-3 pattern at `crates/bootroom-core/src/lib.rs:159-225`.

### JSONL transcript event shapes (RUN-08)

```json
{"ts":"2026-05-19T14:32:01.123Z","type":"scenario_start","scenario":"boot_smoke","kernel":"/path/to/Image"}
{"ts":"2026-05-19T14:32:01.140Z","type":"action_send","action":"reboot","bytes_b64":"cmVib290DQ=="}
{"ts":"2026-05-19T14:32:02.205Z","type":"serial_chunk","action":"reboot","bytes_b64":"WyAgIDAuMDAwMDAwXSAuLi4="}
{"ts":"2026-05-19T14:32:03.310Z","type":"assertion_result","action":"reboot","kind":"contains","pattern":"login: ","verdict":"pass"}
{"ts":"2026-05-19T14:32:03.311Z","type":"scenario_result","verdict":"pass","actions":[...]}
```

The schema is future-proof for a v2 JUnit shim: every JUnit `<testcase>` maps to one action's assertion-result events; every `<testsuite>` maps to a scenario_result.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Server-side scenario engine driving the browser via WS | Browser-side engine emitting a single `ScenarioResult` over WS | Phase 4 CONTEXT decision | Eliminates per-action WS round-trip; simplifies ANSI/line-buffer logic to one location. |
| `wasmtime` / `wasmer` for headless qemu-wasm | Headless Chromium + chromiumoxide | Locked in Phase 1 research; WASI can't satisfy qemu-wasm's `WebAssembly.Module` + pthreads requirements. | Chromium is the only viable target. |
| `BrowserConfig { headless: ..., ..config }` post-build override | All-builder construction | chromiumoxide 0.9.x (Spike B finding) | Documented; Spike B incantation lifts cleanly. |
| `tokio-runtime` feature flag | Implicit (chromiumoxide is tokio-only as of 0.9.x) | chromiumoxide 0.9.x | Drop the feature flag in Cargo.toml. |

**Deprecated/outdated:**
- `Playwright subprocess fallback`: retired by Spike B verdict (CONTEXT explicitly removes it from Phase 4 scope).
- The earlier-phase placeholder "scenario types only; engine in Phase 4" [VERIFIED: `.planning/phases/03-config-buttons-watcher/03-CONTEXT.md:20`] is now fulfilled.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | JS `RegExp` accepts every pattern Rust `regex` accepts (Rust is the stricter engine for backref/lookaround). | Pitfall 1 | If wrong, a few obscure patterns pass `bootroom check` but fail at runtime — engine reports `verdict: error`; documentation pins supported subset. |
| A2 | `WebSocket.bufferedAmount === 0` polling reliably signals "frame flushed to TCP" across Chromium versions. | Pitfall 2 | If `bufferedAmount` lags actual flush, the outer timeout in `run_cmd` may fire on a scenario that *did* succeed. Mitigated by generous outer-timeout buffer. |
| A3 | `master.onWrite` listeners can coexist without acking competition (only one listener calls `_ack`, others observe). | Pattern 2 | If wrong, multi-listener acking double-drives xterm backpressure and bytes get dropped. Phase 2's SerialOut mirror already validates this is safe [VERIFIED: comment at `app.js:680-682`]. |
| A4 | `chromiumoxide 0.9.1` against system Chromium 148.0.7778.167 stays compatible through Phase 4 timeframe. | Standard Stack | Chromium auto-updates on Arch may break the CDP API. Mitigated by setting `--chromium` (or `$CHROMIUM`) so CI can pin a known-good version. |
| A5 | `URLSearchParams` is universally available in any browser capable of running qemu-wasm (which already requires SAB + COOP/COEP). | Pattern 1 | Negligible — every SAB-capable browser supports `URLSearchParams` (ES2017+). |
| A6 | A scenario does not require a kernel re-boot mid-sequence in Phase 4. | Pitfall 2 | If wrong, the engine needs a `Module.FS_unlink` + reload-aware completion mechanism; out of scope per CONTEXT. |
| A7 | The new `Mutex<Option<oneshot::Sender<…>>>` field on `AppState` is safe to leave `None` for `serve` mode (no consumer ever takes it) and roundtrip-tested for `run` mode. | Pattern 4 | If a future test inadvertently checks `is_some()` in `serve` context, it'll be `None`. The match arm in `handle_wire` is gated on `Some(_)`. |
| A8 | The kernel will emit *some* `master.onWrite` bytes before the per-scenario timeout fires (no scenario starts before `firstSerialOutSeen`). | Timeout Machinery | If a scenario references a kernel that fails to boot at all, the engine fails fast on per-action timeout — verdict "timeout" with `error: "no serial output observed"`. |

**Notes:** All assumptions are either backed by in-repo evidence or low-risk; none require user confirmation before execution. A1 and A8 are the two worth surfacing in `04-DISCUSSION-LOG.md` or the executor's startup notes.

## Open Questions

1. **Should `ScenarioStart` ship as a no-op variant in Phase 4, or be reserved for Phase 5+?**
   - What we know: CONTEXT lists it as "at minimum" alongside `ScenarioResult`; URL-query detection makes it unnecessary.
   - What's unclear: Plan-checker may flag the absence as a deviation from CONTEXT.
   - Recommendation: Ship it as a defined-but-unused variant. Cost is one variant + one roundtrip test (~20 LoC); future use cases (server-driven re-runs, `--watch`) get it for free.

2. **Where to validate `--scenario <name>` exists in the config — Rust or browser?**
   - What we know: Rust has the full `LoadedConfig` at `run_cmd::run` startup; failing early gives exit code 2 cleanly.
   - What's unclear: Should we *also* defend in the browser? CONTEXT doesn't specify.
   - Recommendation: Both. Rust check exits 2 cleanly before launching Chromium (fast path). Browser check is defense-in-depth — if config reloads between Rust's check and the browser's fetch (impossible in `run` mode since the watcher does nothing destructive, but defensive code is cheap), the browser emits `ScenarioResult { verdict: "error", error: "scenario '...' not found" }` and exits 1 cleanly.

3. **Timestamp format for `--log-file` and `ScenarioResult.started_at`: ISO 8601 UTC or local with offset?**
   - What we know: CONTEXT says "ISO 8601" without specifying TZ.
   - What's unclear: CI logs typically prefer UTC; the existing UI uses local-with-offset [VERIFIED: `crates/bootroom/web/app.js:60-78`].
   - Recommendation: UTC for log files (machine-readable, no DST confusion); explicit `Z` suffix. JSON wire format = same.

4. **Should `--verbose` output use unicode glyphs (`▶`, `✓`) on Windows CI?**
   - What we know: Windows console may not render UTF-8 cleanly.
   - What's unclear: Phase 4 runs on Linux + macOS (Phase 6 considers Windows); CI today is Linux-only.
   - Recommendation: ASCII-only glyphs (`>` for action, `+`/`-` for pass/fail) — safer cross-platform; matches `bootroom check` convention.

5. **Per-action timeout default — inherit from `Assertion.timeout_ms` or use a dedicated `Action.timeout_ms`?**
   - What we know: `Assertion.timeout_ms` defaults to 5_000 [VERIFIED: `bootroom-core/src/config.rs:82-84`]; no `Action.timeout_ms` field exists.
   - What's unclear: An action with no assertions has no natural timeout source.
   - Recommendation: The **per-action timeout** = MAX(assertion.timeout_ms across that action's assertions). If the action has zero assertions, default to a hard 5_000 ms. Document this in `run_cmd::run` and the engine.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Chromium binary | RUN-02 headless driver | ✓ | `Chromium 148.0.7778.167 Arch Linux` [VERIFIED: `chromium --version`] | `$CHROMIUM` env override; `which chromium` PATH probe; otherwise exit 3. |
| `cargo` toolchain (1.85+) | All Rust builds | ✓ | Workspace MSRV 1.85; system rustc 1.90 [from 01-RESEARCH.md] | None — fatal if missing. |
| `chromiumoxide 0.9.1` (crate) | RUN-02 driver | ✓ | 0.9.1 in `Cargo.lock` (via Spike B) | None. |
| `regex` crate | Assertion validation (server) | ✓ | 1.12.3 (already transitively present) [VERIFIED: `Cargo.lock`] | None. |
| `tokio` + `oneshot` | Run mode completion | ✓ | Workspace 1.52.3 | None. |
| Node.js | Browser pre-build | ✗ | N/A | None needed — vanilla JS, no build step. (Project constraint.) |
| QEMU-WASM embedded assets | Browser boot | ✓ | Committed under `crates/bootroom/assets/qemu/` per Phase 1 | None — fatal at build time if missing. |

**Missing dependencies with no fallback:** None for the green-path Linux dev environment.

**Missing dependencies with fallback:** Chromium binary may be missing on a fresh CI runner; the `$CHROMIUM` → `/usr/bin/chromium` → `which chromium` chain provides three options; if all fail, exit 3 with a documentation pointer.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust integration tests) + manual headed-browser smoke tests where headless is infeasible |
| Config file | `crates/bootroom/Cargo.toml` `[dev-dependencies]` |
| Quick run command | `cargo test -p bootroom --test cli_subcommands` (fast smoke) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RUN-01 | `bootroom run` exits 0 on pass, non-zero on fail | integration (subprocess) | `cargo test -p bootroom --test run_subcommand_exit_codes` | ❌ Wave 0 |
| RUN-02 | chromiumoxide drives headless Chromium | integration (real fixture, requires `/usr/bin/chromium`) | `cargo test -p bootroom --test run_smoke_norn_kernel -- --ignored` (tagged `#[ignore]` for low-resource CI) | ❌ Wave 0 |
| RUN-03 | Same assets / same `/ws` protocol as `serve` | integration | `cargo test -p bootroom --test run_uses_same_router` | ❌ Wave 0 |
| RUN-04 | Substring + regex assertion kinds | unit (browser pseudo-DOM via headless Chromium) + Rust validate | `cargo test -p bootroom-core --test assertion_validate` + `cargo test -p bootroom --test run_assertion_kinds -- --ignored` | ❌ Wave 0 |
| RUN-05 | ANSI strip + line-buffered match | unit (Rust) for compile; integration for runtime | `cargo test -p bootroom-core --test assertion_validate` (compile-side) + `cargo test -p bootroom --test run_assertion_ansi -- --ignored` | ❌ Wave 0 |
| RUN-06 | Per-action + per-scenario timeouts with structured failures | integration | `cargo test -p bootroom --test run_timeout_shapes -- --ignored` | ❌ Wave 0 |
| RUN-07 | Per-action serial buffer reset by default | integration | `cargo test -p bootroom --test run_per_action_buffer_reset -- --ignored` | ❌ Wave 0 |
| RUN-08 | `--log-file` JSONL transcript | integration (subprocess + temp file) | `cargo test -p bootroom --test run_log_file_jsonl -- --ignored` | ❌ Wave 0 |
| RUN-09 | `--verbose` stderr stream | integration (subprocess; assert stderr lines) | `cargo test -p bootroom --test run_verbose_stderr -- --ignored` | ❌ Wave 0 |
| RUN-10 | COI self-check on startup | integration (Chromium with deliberately-broken COOP/COEP, expected exit 3) | `cargo test -p bootroom --test run_coi_self_check -- --ignored` | ❌ Wave 0 |
| CLI-02 | Shared `--kernel`/`--config`/`--verbose` via `#[flatten]` | unit (clap parse) | `cargo test -p bootroom --test cli_subcommands cli_serve_run_share_kernel_flag` | ⚠ extend existing `cli_subcommands.rs` |

### Sampling Rate

- **Per task commit:** `cargo test -p bootroom --test cli_subcommands` (compile-side; fast)
- **Per wave merge:** `cargo test --workspace` (excludes `#[ignore]` integration tests by default)
- **Phase gate:** `cargo test --workspace -- --ignored` against a real Chromium + real NORN kernel fixture (mirrors Spike B harness). Required pass before `/gsd-verify-work`.

### Wave 0 Gaps

- [ ] `crates/bootroom/tests/run_subcommand_exit_codes.rs` — pin RUN-01 exit-code table (use a fixture scenario; can run with a small kernel or even `#[ignore]`-tagged real-NORN).
- [ ] `crates/bootroom/tests/run_uses_same_router.rs` — verify `build_router(state)` is the same router across `serve` and `run` dispatch (no separate codepath).
- [ ] `crates/bootroom/tests/run_smoke_norn_kernel.rs` — `#[ignore]`-tagged integration: green-path scenario against the Spike B `fixtures/Image`.
- [ ] `crates/bootroom-core/tests/assertion_validate.rs` — regex compile + invalid-pattern rejection.
- [ ] `crates/bootroom/tests/run_log_file_jsonl.rs` — subprocess + JSONL shape pin.
- [ ] `crates/bootroom/tests/run_verbose_stderr.rs` — subprocess + stderr line assertions.
- [ ] `crates/bootroom/tests/run_coi_self_check.rs` — startup error path (deliberately disable COOP/COEP via test-only feature flag? Or skip — RUN-10 may be manual-only).

## Security Domain

Phase 4 widens the attack surface only marginally:

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | bootroom is a local-loopback dev tool; no auth surface. |
| V3 Session Management | no | Same. |
| V4 Access Control | partial | The existing CR-02 Origin allow-list on `/ws` covers `run` mode too — Chromium will send `Origin: http://127.0.0.1:<eph>` matching `allowed_origins`. |
| V5 Input Validation | yes | All untrusted inputs are operator-controlled (scenario name from CLI, scenario steps from `bootroom.toml`). Validation already shipped (`deny_unknown_fields`, `LoadedConfig` cross-validation); Phase 4 extends with regex compile-check. |
| V6 Cryptography | no | No new cryptography. |

### Known Threat Patterns for the Phase-4 Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| `--scenario` value injection into shell / Chromium argv | Tampering | clap parses `--scenario` as a string; the value is used in a URL query param (browser-decoded) and in JSONL events. No shell concatenation. **Risk: low.** |
| Malicious `bootroom.toml` with a pathological regex (catastrophic backtracking) | DoS | Rust `regex` is linear-time by construction — no catastrophic backtracking possible. JS `RegExp` IS susceptible, but the engine has `assertion.timeout_ms` as a natural circuit-breaker; a pathological regex will trip the timeout. **Risk: low.** |
| Untrusted JS in `--assets-dir` override loaded into `run`-mode Chromium | Tampering | `--assets-dir` is operator-supplied — same trust model as `serve`. CR-02 Origin allow-list applies. **Risk: low.** |
| Chromium process not cleaned up on `bootroom run` panic | Resource exhaustion (DoS-self) | Spike B uses `browser.close().await` + `browser.wait().await` + handler `abort()` [VERIFIED: `spike-b/src/main.rs:240-243`]. Lift verbatim; wrap in `Drop` guard if RAII is feasible. **Risk: medium** (needs explicit cleanup in error paths). |
| `$BOOTROOM_CHROMIUM_ARGS` injection | Tampering | env var is operator-controlled (matches dev-tool trust model); appended verbatim to chromium argv. Document as "trusts the same operator as `--kernel`." **Risk: low.** |

## Sources

### Primary (HIGH confidence)

- `crates/bootroom/spikes/spike-b/src/main.rs` (in-tree) — chromiumoxide 0.9.1 launch incantation, `Runtime.evaluate` patterns, the eval helpers.
- `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` — `verdict: green`, COI/SAB confirmed.
- `crates/bootroom-core/src/config.rs` — `Config`, `Action`, `Scenario`, `Assertion`, `AssertionKind`, `LoadedConfig`, span-aware `LoadError`. All needed types already shipped.
- `crates/bootroom-core/src/lib.rs` — `WsMessage` enum and additive-variant convention (`KernelChanged`, `ConfigUpdate`, `ConfigInvalid` already inserted in Phase 3).
- `crates/bootroom/src/cli.rs`, `crates/bootroom/src/main.rs`, `crates/bootroom/src/server.rs`, `crates/bootroom/src/state.rs`, `crates/bootroom/src/ws.rs` — existing dispatch shape; `build_router(state)` reuse path.
- `crates/bootroom/web/funnel.js` — `lockInput`/`unlockInput`/`setLockObserver` already shipped; lock-agnostic enqueue semantics documented inline.
- `crates/bootroom/web/app.js` — `URLSearchParams` precedent (`?pacing=N`), `master.onWrite` SerialOut mirror, WS handler structure for additive variants.
- `crates/bootroom/src/watcher.rs:108-141` — `project_loaded_to_json` shape (the `/api/config` projection the engine consumes).
- `.planning/phases/04-scenario-engine-headless-run/04-CONTEXT.md` — all locked decisions.
- `.planning/phases/01-walking-skeleton/01-08-SUMMARY.md` — chromiumoxide 0.9.x API surprises catalogue.

### Secondary (MEDIUM confidence)

- `Cargo.lock` (in-tree) — `regex 1.12.3` already transitively present.
- `/usr/bin/chromium --version` → `Chromium 148.0.7778.167 Arch Linux` (matches Spike B).

### Tertiary (LOW confidence — Phase 4 validates)

- A1 (Rust `regex` is the stricter superset of JS `RegExp` for the assertion-supported feature subset): based on training knowledge of the two engines, not in-repo verification.
- A2 (`WebSocket.bufferedAmount` is a reliable flush signal in Chromium 148+): based on training knowledge of the standard; not in-repo verified.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every dep is in-repo or Spike-B-proven.
- Architecture: HIGH — additive-variant convention is the third Phase-3 application; the oneshot pattern is canonical tokio.
- Pitfalls: MEDIUM — pitfalls 1 (regex flavor) and 2 (WS flush race) are educated guesses; the rest are direct reads of in-repo files.
- Security: HIGH — the existing CR-02 Origin allow-list covers `run` mode without modification.

**Research date:** 2026-05-19
**Valid until:** 2026-06-18 (30 days; Chromium auto-update is the main drift risk — re-verify against the latest stable if Phase 4 slips past this window).

---

**Synthesis:** Phase 4 is a composition of pieces every one of which already ships somewhere in this workspace. The `chromiumoxide 0.9.1` launch sequence is Spike B verbatim; the `WsMessage` additive-variant convention is the Phase 3 pattern repeated for the third time; `Funnel` already exposes `lockInput`/`unlockInput` waiting for its first caller; `Assertion` types and `default_*_timeout` helpers are pre-decided; `project_loaded_to_json` projects everything the browser needs; `URLSearchParams` is already the precedent for runtime config injection (`?pacing=N`); the `master.onWrite` observer pattern is already double-subscribed (xterm + Phase-2 SerialOut mirror). The new code is a ~300-line `web/scenario.js` engine, a ~250-line `run_cmd.rs` driver, three additive `WsMessage` variants with roundtrip tests, and the clap `#[flatten]` extraction of `SharedArgs`. The dominant risk is **Pitfall 2** (WS flush race on scenario completion) — defended by a `bufferedAmount === 0` poll and a generous outer timeout. Plan against the Spike B fixture (`crates/bootroom/spikes/spike-b/fixtures/Image`) for a credible green-path integration check; the headed-browser smoke pattern from Phase 3 plan 03-11 carries over unchanged.
