---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Completed 02-05-PLAN.md (LAUNCH/RESET/CLEAR/COPY markup + IDLE pill CSS — palette-pure)
last_updated: "2026-05-18T10:28:20.566Z"
progress:
  total_phases: 6
  completed_phases: 1
  total_plans: 15
  completed_plans: 14
  percent: 93
---

# State: bootroom

**Project:** bootroom — web-based test harness for RISC-V kernels via qemu-wasm
**Mode:** mvp
**Initialized:** 2026-05-17

## Project Reference

**Core Value:** Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library. If everything else fails, that one path must stay friction-free.

**Current Focus:** Phase 2 — WebSocket + Live Serial

## Current Position

Phase: 2 (WebSocket + Live Serial) — EXECUTING
Plan: 5 of 6 complete (01 done, 02 done, 03 done, 04 done, 05 done; remaining: 02-06 WS lifecycle wiring)

- **Phase:** 2 — WebSocket + Live Serial
- **Plan:** 02-02 complete — `crates/bootroom/src/ws.rs` (134 LOC) implements Pattern 1: `WebSocketUpgrade.on_upgrade(socket → split → bounded mpsc::channel::<WsMessage>(32) → spawned writer task)`, with `handle_wire` dispatching each variant to tracing (SerialIn/SerialOut → trace, Launch/Reset → info, client-sent State/Hello → warn + continue per T-02-17). Server emits `Hello { version: env!(CARGO_PKG_VERSION) }` as the first frame on every connection. Route registered as `.route("/ws", axum::routing::any(crate::ws::ws_handler))` — `any` over `get` per 02-RESEARCH.md Pattern 1 to leave room for future protocol negotiation. Malformed JSON / unexpected binary frames log + continue; only Close or recv error breaks the reader loop (T-02-16). Three integration tests in `crates/bootroom/tests/ws_roundtrip.rs` pin the contract: `ws_handshake_emits_hello` (first frame parses as Hello with matching CARGO_PKG_VERSION), `ws_client_serial_in_is_logged_not_echoed` (SerialIn + Close round-trip without server panic), and `ws_upgrade_response_carries_coop_coep` (Pitfall #4 / T-02-18 regression — COOP same-origin + COEP require-corp survive on a raw GET to /ws). One clippy `unused_async` deviation auto-fixed inline (handle_wire kept async for Phase 4 forward compatibility). cargo test --workspace green. Commits 206844e + 8537e3b. WS-01 + WS-04 server side satisfied.
- **Status:** Executing Phase 2
- **Progress:** [██████████] 100% (Phase 2 plans 1-5 complete; plan 6 remaining)

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

- **Last session:** 2026-05-18T10:28:20.558Z (then plan 02-02 completed 2026-05-18T10:26:40Z)
- **Stopped at:** Completed 02-02-PLAN.md (/ws axum handler + 3 integration tests incl. COOP/COEP-on-/ws regression)
- **Next session:** Execute Phase 2's final plan: 02-06 (WS lifecycle wiring in `web/app.js` — imports `funnel.js` from 02-04, queries the DOM contract from 02-05, connects to /ws shipped here, parses Hello + State frames, routes Launch/Reset clicks to WS frames before page reload).
- **Context to reload:** `02-CONTEXT.md` (locked decisions), `02-UI-SPEC.md` (button/pill state machine for plan 06), `.planning/phases/02-websocket-live-serial/02-02-SUMMARY.md` (server-side /ws contract this plan consumes), `.planning/phases/02-websocket-live-serial/02-05-SUMMARY.md` (DOM contract), `crates/bootroom/web/index.html`, `crates/bootroom/web/app.js`, `crates/bootroom/web/funnel.js`, `crates/bootroom-core/src/lib.rs`.

---
*State initialized: 2026-05-17 via gsd-roadmapper*
