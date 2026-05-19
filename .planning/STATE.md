---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: "Plan 03-03 complete (CLI subcommand skeleton: Cmd::{Serve,Check,Init} + --config/--action). Plans complete: 01, 02, 03, 09, 10. 03-05 in flight (state.rs/server.rs/tests/common). Next non-blocking plans: 04 (real check/init handlers), 06, 07, 08, 11."
last_updated: "2026-05-19T09:15:06.112Z"
progress:
  total_phases: 6
  completed_phases: 2
  total_plans: 26
  completed_plans: 20
  percent: 77
---

# State: bootroom

**Project:** bootroom — web-based test harness for RISC-V kernels via qemu-wasm
**Mode:** mvp
**Initialized:** 2026-05-17

## Project Reference

**Core Value:** Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library. If everything else fails, that one path must stay friction-free.

**Current Focus:** Phase 03 — Config, Buttons, Watcher

## Current Position

Phase: 03 (Config, Buttons, Watcher) — EXECUTING
Plan: 10 of 11

- **Phase:** 2 — WebSocket + Live Serial — COMPLETE
- **Plan:** 02-06 complete — `crates/bootroom/web/app.js` refactored end-to-end: imports `Funnel`, `bytesToB64`, `b64ToBytes`, `keyEventToBytes` from `./funnel.js`; constructs one `Funnel(slave)` as sole writer to `slave.write` during normal byte flow (WS-02); installs intercepting `attachCustomKeyEventHandler` returning `false` to suppress xterm's default `master.onData` dispatch (Pitfall #1 mitigation) and route bytes through funnel with `pacingMs: 0`. WS `/ws` lifecycle: `connectWs()` parses Hello (info terminal write), SerialIn (b64 -> funnel.enqueue with configurable `pacingMs`), State (uppercased to override local pill via `serverStateAuthority`); naive 1s reconnect on close (T-02-25 accept). SerialOut mirror via `slave.onReadable` -> `ws.send({type:'SerialOut',data:<b64>})` when WS open — also the trigger for LOADING -> RUNNING. 4-state pill machine (Pattern 5): IDLE (explicit at startup) -> LOADING (after `xterm.open`) -> RUNNING (`runtimeInitialized && firstSerialOutSeen` via `recomputePillLocal`) -> HALTED (Module.onExit/onAbort, clearing `serverStateAuthority`). LAUNCH/RESET = best-effort WS send + `requestAnimationFrame` + `window.location.reload()` (D-02 identical mechanism, visually distinct). CLEAR = `xterm.clear()`. COPY = selection-or-`xterm.buffer.active.translateToString(true, 0, length)` (Pitfall #5) with COPIED/COPY FAILED 1500ms flash + `[bootroom] Copy failed` terminal diagnostic on failure (T-02-29 audit trail). `?pacing=N` URL param clamped to `>= 0`, default 15ms (WS-03). 525 LOC total / 297 non-comment LOC — under plan's 350-LOC factor-out threshold; single-file preserved per Phase 1 norm. Phase 1 surface preserved unchanged: `humanBytes` (WR-07), `isoLocal`, `loadKernelInfo`, vendor-globals guard (WR-01), `FS_unlink` ENOENT-only catch (WR-08), `FS_createDataFile` swap, `Module.TTY.stream_ops.poll` patch, resize handler + rAF fits. All 12 grep gates + `node --check` + `cargo test --workspace` green on first commit. Commit 6b77b30. UI-02/03/04/06/08/09 + WS-02/03 satisfied.
- **Status:** Executing Phase 03
- **Progress:** [███████░░░] 69%

## Performance Metrics

- Phases complete: 0/6
- v1 requirements complete: 0/59
- Validated requirements: 0
- Open spikes: 0 (Phase-1 Spike A retired — verdict green, chosen_path module-fs-write; Spike B retired — verdict green, chosen_path chromiumoxide)

## Accumulated Context

### Decisions

Carried from `PROJECT.md` Key Decisions:

- Project + binary name = `bootroom` (kernel-agnostic)
- Repo `norn-web` → `bootroom` rename pending (Phase 1 action item)
- Language = Rust 1.85+ / edition 2024
- Action model = serial/stdin injection via xterm-pty `slave.write`
- Kernel discovery = watch path (default) + `--kernel` override
- Config format = TOML
- UI = vanilla JS + HTML, embedded via `include_dir!`
- CI mode = `bootroom run --scenario …` with exit codes
- Distribution = `cargo install` + `cargo-dist` prebuilt binaries
- License = MIT OR Apache-2.0
- [Phase ?]: 01-02: Skipped docker build due to host disk constraint; added BOOTROOM_SKIP_QEMU_ASSET_CHECK escape hatch in build.rs so dev work on unrelated Phase 1 plans is unblocked.
- [Phase ?]: 01-03: Vendored xterm@5.3.0 (unscoped) and xterm-pty@0.12.0 as UMD bundles; pinned via SHA-256 in vendor/VERSIONS.md with MIT licenses captured verbatim in vendor/LICENSES.md
- [Phase 1]: 01-04: axum 0.8 + tower-http 0.6 + clap derive; default bind 127.0.0.1:8765; COOP/COEP via SetResponseHeaderLayer::overriding on every response (verified on 404 + 501 paths); --kernel existence validated at startup (V5); non-loopback --host emits tracing::warn (V4 partial); library exposes build_router/AppState/ServeArgs for plan 01-07 integration tests
- [Phase 1]: 01-05: Streaming SHA-256 (constant memory) for /api/kernel/info; tokio_util ReaderStream + Body::from_stream for /kernel; mime_guess::from_path with octet-stream fallback resolves .wasm -> application/wasm; V12 path-traversal protection layered (reject `..` segments + canonicalize-and-confirm-descendant); tokio gains "io-util" workspace feature for AsyncReadExt::read
- [Phase 1]: 01-06: Phase 1 UI shell (index.html / app.js / style.css) ships with inline non-module SAB probe BEFORE any module script (Pitfall #4 mitigated); xterm.js + xterm-pty wired per qemu-wasm reference with `attachCustomKeyEventHandler(() => false)` marking input as Phase-1 no-op; kernel bytes fetched up-front and written into Module.FS via synchronous preRun closure (the pendingKernel fallback — bypasses any qemu-wasm-build async-preRun dependency); status pill driven by Module.onRuntimeInitialized / onExit / onAbort; UI-SPEC palette declared once in :root, zero hex outside that block; /assets/qemu/load.js retained (180 lines of emscripten data-pack preload glue, required for /pack/ mount); FitAddon NOT vendored — resize handler is a placeholder, Phase 2 swaps it
- [Phase 1]: 01-09: Spike A closed with verdict green / chosen_path module-fs-write. Production app.js (FS_unlink + FS_createDataFile in onRuntimeInitialized, commit 04a31fa from 01-07) is the proof; the substitution mechanism works on every page load against the real NORN kernel. qemu-wasm submodule SHA 0ef7b4e recorded in SPIKE-A-RESULT.md frontmatter per Pitfall 8. Phase 2 Launch button = fetch + FS_unlink + FS_createDataFile + location.reload (no Node dep). In-place reset (no full page reload) deferred as optional Phase 2 optimisation.
- [Phase 2]: 02-01: `WsMessage` (serde-tagged enum, six variants: SerialIn/SerialOut/State/Launch/Reset/Hello) and `GuestState` (Idle/Loading/Running/Halted, derives Copy) defined in `bootroom-core/src/lib.rs`. `#[serde(tag = "type")]` produces `{"type":"<variant>",...}` wire shape; no `#[serde(deny_unknown_fields)]` so Phase 4 can extend additively (RESEARCH Open Q3). serde wired to `[dependencies]`, serde_json to `[dev-dependencies]` (runtime callers pass strings). Six round-trip tests pin the wire format. TDD RED (a926c45) → GREEN (1d704d7). One clippy `doc_markdown` deviation auto-fixed (quoted `SerialOut` in a doc comment). Plan 02 (axum /ws handler) and Phase 4 headless `run` both import this enum unchanged — WS-04 single source of truth established.
- [Phase 2]: 02-03: --no-open flag added to ServeArgs (default false = auto-open is on); open::that_detached called after listener bind, gated on !args.no_open. Detached spawn variant prevents misconfigured launchers from blocking the parent. Failure logs tracing::warn + emits UI-SPEC stderr line and keeps serving (never fatal). URL is format!("http://{bound}") so no user-controlled string reaches the launcher (T-02-04). All Phase 2 Cargo deps wired together: open=5.3.5, base64=0.22.1, futures-util=0.3.32, dev-only tokio-tungstenite=0.29.0; axum gains ws feature on bootroom crate (plan 02-02 needs zero Cargo.toml edits). SERV-06 subprocess test uses CARGO_BIN_EXE_bootroom + ChildGuard RAII + mpsc reader threads (mirrors WR-06). TDD RED 8e9e4fe -> GREEN c68672f; one clippy needless_continue auto-fix folded into GREEN.
- [Phase 2]: 02-04: Used ES #drain private method syntax in funnel.js — enforces single-drain invariant via language syntax rather than convention
- [Phase 2]: 02-05: Button hover uses inset box-shadow in --fg-muted; accent reserved for focus-visible ring only (no new hex values; palette purity preserved at count 10)
- [Phase 2]: 02-02: /ws axum handler shipped — Pattern 1 (split socket + bounded mpsc capacity 32 per T-02-15) lives in crates/bootroom/src/ws.rs; route wired via axum::routing::any in build_router. Server is a pass-through observer in Phase 2 — sends Hello { version: env!(CARGO_PKG_VERSION) } as the first frame; SerialIn / SerialOut log to trace; Launch / Reset log to info; client-sent State/Hello log warn + continue (T-02-17). Malformed JSON / unexpected binary log + continue (T-02-16). handle_wire kept async with #[allow(clippy::unused_async)] + comment so Phase 4's tx.send().await reply path lands without signature churn. Integration tests in crates/bootroom/tests/ws_roundtrip.rs pin: (1) Hello version match, (2) SerialIn pass-through round-trip, (3) COOP/COEP-on-/ws regression (Pitfall #4 / T-02-18 — reqwest::get("/ws") returns 400 with both headers intact). Commits 206844e (feat) + 8537e3b (test). WS-01 + WS-04 server side both satisfied.
- [Phase 2]: 02-06: WS lifecycle wired in app.js — funnel-mounted xterm input via attachCustomKeyEventHandler returning false (Pitfall #1 fix), SerialOut mirror via slave.onReadable, 4-state pill machine (IDLE/LOADING/RUNNING/HALTED with server State authority + onExit clearing authority), LAUNCH/RESET identical reload (D-02), CLEAR=xterm.clear, COPY=selection-or-full-buffer with flash+diagnostic on failure (T-02-29), ?pacing=N URL param overrides 15ms WS-SerialIn default (WS-03). 525 LOC / 297 non-comment, under 350-LOC factor-out threshold. All grep gates + node --check + cargo test --workspace green on first commit. Phase 2 complete.
- [Phase ?]: [Phase 3]: 03-10: Funnel lock primitive shipped (ACT-04) — Funnel.locked flag + idempotent lockInput/unlockInput methods + module-level setLockObserver export (with non-function-fallback + try/catch isolation per T-03-10-01). enqueue + #drain unchanged: server-initiated SerialIn must keep flowing during scenario lock, so enforcement is at the caller (Plan 11 wires xterm.onData + .action-btn guards and the BUSY pill observer). 6th DevTools manual-test case added to funnel.js for UI-SPEC Interaction Contract 9. Commits 57bff45 + 18ff166 (corrective revert of pre-staged style.css).
- [Phase 3]: 03-01: bootroom-core gains the canonical TOML schema + escape decoder (executed retroactively after 03-09/03-10/03-02 due to out-of-order parallel work). escape.rs (decode_bytes_escape + EscapeError) handles \r\n\t\0\\\xNN with byte-offset error positions (11 tests). config.rs ships Config/Action/Scenario/Assertion/AssertionKind with #[serde(deny_unknown_fields)] on every struct, LoadedConfig + ResolvedAction projection, CliAction (--action runtime value), LoadError struct with private kind enum + public predicates is_schema_version_mismatch/actual_version, parse_str + offset_to_line_col using prefix.chars().count() for Unicode-scalar columns (matches vim/code jump-to-line). CLI override merge = dedupe-replace by label: existing TOML entry kept at its index, new label appended, last --action wins among CLI-only collisions, group+description cleared when shadowing existing TOML action. 11 unit tests cover CFG-02..06 + ACT-03 override semantics + duplicate-label rejection. Also fixed cross-plan blocker in crates/bootroom/src/ws.rs handle_wire match arm to cover 3 new server-owned WsMessage variants (KernelChanged/ConfigUpdate/ConfigInvalid) added by parallel 03-02 — same warn-and-continue posture as State/Hello. workspace deps (toml=1.1, notify=8, notify-debouncer-full=0.7) already in HEAD via 06b9253. 34 tests green; cargo clippy --workspace --lib --tests -- -D warnings clean. Commits ba8b78f (escape) + 47b7d90 (config + ws.rs fix). CFG-02..06 + ACT-03 unit-test surface satisfied; Pitfall #5/#8 structurally mitigated (one parser shared by all downstream consumers).
- [Phase ?]: [Phase 3]: 03-03: CLI subcommand surface landed — Cmd::{Serve, Check, Init} with Serve first variant (Pitfall #9), ServeArgs extended with --config + repeatable --action (clap value_parser=parse_cli_action, delegating to bootroom_core::decode_bytes_escape — one parser, zero CLI-vs-TOML drift). main.rs dispatches Check->exit(2)/Init->exit(1) stubs; Plan 04 replaces. New tests/cli_subcommands.rs pins 6 help-text + stub-exit assertions (avoids tests/common/mod.rs to dodge Plan 03-05 collision). Pitfall #9 cleared — tests/serve_no_open.rs still green. Commits 15acae4 (cli) + ee76ad5 (dispatch+tests).

### Architecture (from research)

- One process, one WebSocket, two modes (`serve` and `run`) sharing identical embedded assets and `/ws` protocol.
- `bootroom-core` library (pure types + scenario engine, no I/O); `bootroom` binary (clap dispatch, axum app, watcher, headless driver).
- xterm-pty `slave` is *the* byte boundary for both action injection (`slave.write`) and serial capture (`slave.onReadable`).
- Scenarios run client-side (low latency); server is exit-code translator.

### Retired Spikes (Phase 1)

- **Spike A:** CLOSED 01-09 — verdict GREEN, chosen_path `module-fs-write`. Production app.js's onRuntimeInitialized injection (commit 04a31fa) is the proof. qemu-wasm SHA 0ef7b4e recorded per Pitfall 8. Phase 2 Launch button = fetch + FS_unlink + FS_createDataFile + location.reload (no Node dep). In-place reset deferred as Phase 2 optimisation.
- **Spike B:** CLOSED 01-08 — verdict GREEN, chosen_path `chromiumoxide`. headless=new + SAB + qemu-wasm boots the NORN kernel end-to-end. Phase 4 driver locked.

### Todos

- Approve roadmap → run `/gsd-plan-phase 1` to decompose Phase 1 into plans.
- Vendor `xterm.js` and `xterm-pty` (qemu-wasm demo uses CDN — must be self-hosted).
- Define `make qemu-assets` target that runs the qemu-wasm docker build and caches output (don't drive from `build.rs`).
- Add `--assets-dir <path>` runtime override from day one (dev iteration without `cargo build`).

### Blockers

Spikes A and B are de-risking activities for Phase 1, not external blockers.

- 01-02: docker build for qemu-wasm artifacts not run; host disk at 98% (12G free). User must free 10G+ then run 'make qemu-assets' to populate crates/bootroom/assets/qemu/.

### Key Pitfalls to Watch (top 3)

1. **COOP/COEP plumbing** must apply to *every* subresource (HTML, JS, WASM, worker, data) — single missing header silently breaks SAB. Verify at boot.
2. **Headless Chromium + SAB + qemu-wasm** is the single biggest unknown. Spike B retires this in Phase 1.
3. **Serial-output assertions** flake without line-buffering, ANSI stripping, per-action buffer reset, and explicit timeouts. Bake the conventions into the scenario engine (Phase 4) before users author scenarios.

## Session Continuity

- **Last session:** 2026-05-19T09:15:06.104Z
- **Stopped at:** Plan 03-03 complete (CLI subcommand skeleton: Cmd::{Serve,Check,Init} + --config/--action). Plans complete: 01, 02, 03, 09, 10. 03-05 in flight (state.rs/server.rs/tests/common). Next non-blocking plans: 04 (real check/init handlers), 06, 07, 08, 11.
- **Next session:** Run Phase 2 verifier (`/gsd-verify-phase 02`) for outer-loop check, then plan Phase 3 via `/gsd-plan-phase 3`. Phase 3 is the headless `bootroom run --scenario …` driver (chromiumoxide-based per Spike B verdict); Phase 2's WS protocol round-trip and SerialOut mirror give Phase 3 a ready-made assertion-capture hook.
- **Context to reload for Phase 3 planning:** `.planning/ROADMAP.md` (Phase 3 scope), `.planning/phases/01-foundation/01-08-SUMMARY.md` + `SPIKE-B-RESULT.md` (chromiumoxide verdict + headless boot proof), `crates/bootroom/spikes/spike-b/` (working spike code), `crates/bootroom-core/src/lib.rs` (`WsMessage` + `GuestState` — Phase 3 reuses the enum unchanged), `.planning/phases/02-websocket-live-serial/02-06-SUMMARY.md` (browser-side WS lifecycle + SerialOut mirror Phase 3 will consume server-side).

---
*State initialized: 2026-05-17 via gsd-roadmapper*
