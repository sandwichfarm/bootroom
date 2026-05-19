---
phase: 4
name: Scenario Engine + Headless `run`
gathered: 2026-05-19
status: Ready for planning
mode: smart-discuss
---

# Phase 4: Scenario Engine + Headless `run` — Context

<domain>
## Phase Boundary

**Goal:** A kernel CI job runs `bootroom run --kernel build/Image --scenario boot_smoke`, gets a 0/1 exit code from serial-output assertions, and a full transcript on failure — using the exact same embedded assets and WS protocol as `serve` mode.

**In scope (Phase 4):**
- Browser-side scenario engine (executes the ordered action list, owns per-action serial buffers, evaluates assertions)
- Additive `WsMessage` variants: at minimum `ScenarioStart` (browser→server, for log/verbose), `ScenarioResult` (browser→server with per-action verdicts + transcript), `ScenarioAbort` (server→browser for cancellation if needed)
- `bootroom run` CLI: `--kernel <path>`, `--config <path>`, `--scenario <name>`, `--verbose`, `--log-file <path>`; shared flags via clap `#[flatten]` (CLI-02)
- In-process axum server bound to `127.0.0.1:0` (ephemeral port), launched by `run` then connected to via chromiumoxide
- Chromium discovery: `$CHROMIUM` env override → `/usr/bin/chromium` → `which chromium` → fail with hint
- Chromium launch flags: `--headless=new`, `--no-sandbox`, `--disable-dev-shm-usage` + Spike B's set; user override via `$BOOTROOM_CHROMIUM_ARGS`
- `crossOriginIsolated` self-check (RUN-10): JS eval `self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'`; abort with structured error
- Assertion engine: substring (`contains`) and Rust-`regex` (`regex`); ANSI escapes stripped; line-buffered (`\r?\n`)
- Per-action serial buffer reset by default (configurable via per-scenario carry-over flag — schema already in place, surface via Phase 3 types unchanged)
- Per-action and per-scenario timeouts with structured failures (RUN-06)
- Exit codes: 0 pass, 1 scenario fail, 2 config/CLI error, 3 startup error (SAB/chromium missing/timeout)
- `--log-file` JSONL: per-line event (timestamp, type, payload) — actions sent, serial chunks, assertion verdicts, final result
- `--verbose` → stderr: per-action progress + assertion verdict + final summary. Default silent on success; one-line failure summary on stderr.
- ACT-04 funnel lock: scenario engine calls `funnel.lockInput()` on start and `funnel.unlockInput()` on completion (primitive already shipped in Phase 3 plan 03-10)
- Reuses the existing browser app — same `index.html`, same `app.js`, same `funnel.js`. Run mode is detected via URL query (`?scenario=<name>`) or server-pushed frame on connect.

**Out of scope (later phases):**
- `bootroom doctor` — Phase 5
- Crates.io publish + cargo-dist + release binaries — Phase 6
- JUnit XML / GitHub Actions annotations — v2 (REP-01, REP-02)
- `--watch` re-run loop — v2 (AUTH-01)
- Screenshot / replay capture — v2
- Snapshot/save-state actions — blocked on upstream qemu-wasm

**Phase 4 requirements (from ROADMAP.md):** RUN-01..10, CLI-02

</domain>

<decisions>
## Implementation Decisions

### Scenario Engine Architecture

- Engine executes **browser-side** in `app.js` (or a new sibling module, e.g. `web/scenario.js`). Matches the architecture decision from prior phases: "scenarios run client-side; server is exit-code translator."
- **Per-action serial buffer** stored browser-side as a `Map<actionLabel, Uint8Array>` (or a growable buffer); resets on action start; accumulates SerialOut bytes during the action's match window.
- **Results return via a new `ScenarioResult` WsMessage variant** (browser → server): includes overall verdict (pass/fail), per-action verdicts, per-assertion verdicts, and the full transcript. Additive variant — no existing wire shape changes.
- `bootroom run` exits **on `ScenarioResult` receipt**: server translates verdict to exit code, persists transcript to `--log-file` if set.

### Headless Driver (chromiumoxide)

- **Single in-process model:** `bootroom run` spins up the same axum server (in-process) bound to `127.0.0.1:0`, launches chromiumoxide-driven Chromium pointing at that ephemeral URL, awaits `ScenarioResult` WS frame, then shuts down. Matches Spike B's proven pattern.
- **Chromium discovery:** `$CHROMIUM` env → `/usr/bin/chromium` → PATH probe (`which chromium`) → exit 3 with hint message listing the searched candidates.
- **COI self-check (RUN-10):** Before launching the scenario, evaluate `self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'` via CDP `Runtime.evaluate`. Failure → exit 3 with the same fix-hint message the UI banner displays.
- **Launch flags:** `--headless=new`, `--no-sandbox`, `--disable-dev-shm-usage`, plus Spike B's working set; appendable via `$BOOTROOM_CHROMIUM_ARGS`.

### Assertion Matching Semantics

- **ANSI stripping (RUN-05):** Buffer is decoded UTF-8-lossy, then `\x1b\[[0-9;]*[A-Za-z]` is regex-stripped before matching. Cursor moves, color codes, OSC sequences all gone. Storage is still raw bytes; stripping happens at match time.
- **Line buffering (RUN-05):** Match operates on lines accumulated up to the latest `\r?\n` boundary. Partial trailing lines are only considered at action-timeout fire (so a kernel printing `login: ` without a newline still gets matched after timeout fires — that path explicitly tagged in the engine).
- **Regex flavor:** Rust `regex` crate (already a candidate workspace dep). `^`/`$` anchored when authors include them; multiline mode is OFF by default. Patterns compile-checked at config load (Phase 3 — extend `Assertion::validate()` to compile-check regex patterns).
- **`after` semantics:**
  - `after = "<action_label>"` → only that action's serial buffer is searched.
  - `after = "any"` → union of all per-action buffers since scenario start (line-ordered as they arrived).

### CLI Surface & Output

- **Flags:** `--kernel <path>`, `--config <path>`, `--scenario <name>`, `--verbose`, `--log-file <path>`. Shared `--kernel`/`--config`/`--verbose` via clap `#[flatten]` (CLI-02). `--host`/`--port` deliberately not exposed (the ephemeral 127.0.0.1:0 bind is an implementation detail).
- **Exit codes:** `0` pass, `1` scenario fail (any assertion or timeout), `2` config/CLI error (invalid TOML, unknown scenario, bad flag), `3` startup error (SAB self-check failed, Chromium missing, ScenarioResult not received within outer timeout).
- **`--log-file` format:** JSONL — one event per line. Event types: `scenario_start`, `action_send`, `serial_chunk`, `assertion_result`, `scenario_result`. Each event has `ts` (ISO 8601), `type`, and a type-specific payload. Machine-parseable for downstream report tooling (a v2 `--report-format=junit` flag can consume this directly).
- **`--verbose` (stderr):** Per-action progress (`▶ action: reboot`), assertion verdicts (`✓ assert: contains "login: "`), final summary. Non-verbose: silent on success; single-line `bootroom run: scenario boot_smoke FAILED — assertion 'login: ' not found after action reboot` on failure. stdout is reserved for the transcript when `--log-file -` is passed (future, not Phase 4).

### Claude's Discretion

All other implementation details are at Claude's discretion — module layout, exact JSON event payloads, internal struct shapes, test-fixture choice, and order of plan execution.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets

- `bootroom-core::config` — `Config`, `Action`, `Scenario`, `Assertion`, `AssertionKind`, `LoadedConfig`, `parse_str`, span-aware `LoadError`. Phase 4 extends `Assertion::validate()` to compile-check regex patterns at load time; otherwise unchanged.
- `bootroom-core::WsMessage` — additive enum with explicit precedent of additive variants (`KernelChanged`, `ConfigUpdate`, `ConfigInvalid` added in Phase 3). `ScenarioResult` / `ScenarioStart` / `ScenarioAbort` follow the same pattern; no `deny_unknown_fields`, old clients ignore unknowns.
- `crates/bootroom/src/server.rs` — `run(args)` already returns the bound listener and an Arc<AppState>; trivial to call from a `run` subcommand and grab the ephemeral port back.
- `crates/bootroom/src/ws.rs` — split socket + bounded mpsc + Phase-3 broadcast forwarder. The reader side already routes JSON parse errors with warn+continue; new variants drop in without touching the wire path.
- `crates/bootroom/src/state.rs` — `AppState` already carries `kernel_canon`, `loaded_config`, `ws_broadcast`. Phase 4 adds an optional `scenario_result_tx: Option<oneshot::Sender<ScenarioResult>>` (or equivalent) so the `run` command can await scenario completion.
- `crates/bootroom/web/funnel.js` — `Funnel` with `lockInput()`/`unlockInput()` and `setLockObserver(cb)` from Phase 3 plan 03-10. Scenario engine calls `funnel.lockInput()` on start and `funnel.unlockInput()` on completion.
- `crates/bootroom/web/app.js` — already wires `handleWsFrame` with try/catch per-variant. Adding the scenario-mode branch is additive.
- `crates/bootroom/spikes/spike-b/` — chromiumoxide 0.9.1 reference driver. Patterns lifted: launch flags, COI eval, polling loop. Not a `cargo tree -p bootroom` dependency yet (Spike B is its own workspace member); Phase 4 promotes chromiumoxide into `crates/bootroom`'s `[dependencies]`.

### Established Patterns

- **Span-aware errors:** `LoadError` with optional `line`/`col` is the project's convention. Engine errors that originate at scenario-config time get the same treatment.
- **WS protocol additivity:** All Phase 2/3 wire additions are pure variant inserts; client tolerates unknowns by `warn+continue`. Phase 4 follows.
- **Single-source-of-truth schemas:** Decoder / validator / projector live in `bootroom-core`. Anything Phase 4 surfaces (e.g., scenario events) lives there too — `bootroom` and `bootroom-core` cannot drift.
- **Subprocess tests:** Pattern lifted from `tests/cli_subcommands.rs` + `tests/serve_no_open.rs`. Use `CARGO_BIN_EXE_bootroom` and a child-guard RAII.
- **Tracing:** `tracing::info!` for state transitions; `tracing::warn!` for recoverable bumps; `tracing::error!` only when about to exit non-zero.

### Integration Points

- New `Cmd::Run(RunArgs)` variant in `crates/bootroom/src/cli.rs`; dispatched from `main.rs` (alongside `Serve`/`Check`/`Init`).
- New file `crates/bootroom/src/run_cmd.rs` (driver: spin up server in-process, drive chromiumoxide, await result, translate exit code).
- New file `crates/bootroom/web/scenario.js` (engine: action sequencer, per-action buffers, assertion evaluator, ScenarioResult emitter).
- Extend `WsMessage` enum in `crates/bootroom-core/src/lib.rs` with the new variants + roundtrip tests.
- Extend `app.js` to detect run-mode and import the scenario module on first ScenarioStart frame (or `?scenario=` URL param).

</code_context>

<specifics>
## Specific Ideas

- The same `bootroom run` invocation should work against the real NORN kernel fixture used in Spike B (`spike-b/fixtures/Image`) so Phase 4 verification has a credible green-path target.
- Pitfall protection: when chromium launch fails (binary missing, sandbox issue), the error message must point at `$CHROMIUM` env override before anything else — kernel CI runners use a wide variety of chromium installs.

</specifics>

<deferred>
## Deferred Ideas

- `--report-format=junit` and `--report-format=github` (REP-01/02) — v2.
- `--watch` re-run loop (AUTH-01) — v2.
- Per-action keyboard shortcuts (AUTH-02) — v2.
- Screenshot button (AUTH-03) — v2.
- Record-and-replay (AUTH-04) — v2.
- Snapshot/save-state actions — blocked on upstream qemu-wasm.
- Playwright subprocess fallback — Spike B retired this concern (chromiumoxide green); not implemented in Phase 4. If later CI runner images break, Phase 5/6 may reintroduce it.

</deferred>
