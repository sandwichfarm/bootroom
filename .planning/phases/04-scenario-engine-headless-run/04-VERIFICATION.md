---
phase: 04-scenario-engine-headless-run
verified: 2026-05-19T00:00:00Z
status: passed
score: 5/5 must-haves verified
overrides_applied: 0
---

# Phase 4: Scenario Engine + Headless `run` Verification Report

**Phase Goal:** A kernel CI job runs `bootroom run --kernel build/Image --scenario boot_smoke`, gets a 0/1 exit code from serial-output assertions, and a full transcript on failure — using the exact same embedded assets and WS protocol as `serve` mode.

**Verified:** 2026-05-19
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

Phase 4 is a composition phase. Every success criterion has direct codebase evidence and is exercised end-to-end against the real NORN kernel by `tests/run_smoke_norn_kernel.rs -- --ignored`, which executed in 1.29s on this host during verification.

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `bootroom run` drives headless Chromium against same embedded assets + same `/ws`; no separate CI code path | VERIFIED | `run_cmd.rs:150` calls the SAME `build_router(state)` used by `serve` (`server.rs:171`). `run_uses_same_router.rs` test pins this contract and passes. `run_cmd.rs:174` launches via `chromiumoxide::Browser::launch`. |
| 2 | Assertions support substring + anchored regex; ANSI stripped; line-buffered (`\r?\n`) | VERIFIED | `scenario.js:193-218` implements `evaluate()` with ANSI strip regex `/\x1b\[[0-9;]*[A-Za-z]/g`, line-buffer to last `\r?\n` with BL-01 fix (empty matchTarget when no newline yet pre-timeout). `bootroom-core/config.rs:430-452` compile-checks `kind="regex"` patterns at load via the `regex` crate. RUN-04/05 satisfied. |
| 3 | Per-action + per-scenario timeouts with structured failures; per-action buffer reset by default | VERIFIED | `scenario.js:248-256` derives per-action timeout via MAX(assertion.timeout_ms) with `??` nullish fallback (WR-08). `scenario.js:469` per-scenario budget with `??`. `Promise.race` at `scenario.js:617-636` with timer-handle clearTimeout (WR-03 fix). Per-action `Map<label, Uint8Array[]>` reset at action start (`scenario.js:489-493`). `run_cmd.rs:211` outer timeout = `scenario.timeout_ms + 30_000` (Pitfall #8). |
| 4 | crossOriginIsolated startup self-check; abort early with clear message if SAB unavailable; exit 0 pass / non-zero fail | VERIFIED | `run_cmd.rs:504-534` `coi_self_check()` calls CDP `Runtime.evaluate("self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'")` with WR-05 distinguishing eval-failure vs non-bool vs false. Returns ExitReason::StartupError → exit 3. `verdict_to_exit` at `run_cmd.rs:79-81` collapses any non-`"pass"` verdict to exit 1; pass = exit 0. |
| 5 | `--log-file` JSONL transcript; `--verbose` to stderr; shared flags via clap `#[flatten]` | VERIFIED | `transcript.rs:30-76` defines six tagged `TranscriptEvent` variants including `transcript_overflow`. `TranscriptWriter::write_event` line-atomic write at `transcript.rs:98-100`. `verbose.rs` ASCII-glyph formatter (`> `, `+ `, `- `). `cli.rs:53-66` `CommonArgs` shared via `#[command(flatten)]` across `ServeArgs` and `RunArgs`. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/bootroom-core/src/lib.rs` | 3 additive `WsMessage` variants (`ScenarioStart`, `ScenarioResult`, `ScenarioAbort`) | VERIFIED | Lines 87, 92, 112; roundtrip tests at 290-388. |
| `crates/bootroom-core/src/config.rs` | Regex compile-check at load + `Assertion.after` resolution check + `regex` direct dep | VERIFIED | Lines 430-452 implement compile + after-resolution; tests at 730-870. |
| `crates/bootroom/src/cli.rs` | `CommonArgs` flatten + `Cmd::Run(RunArgs)` + `--scenario` + `--log-file` | VERIFIED | Lines 38-128; 14 unit tests including `cli_help_lists_shared_flags_on_run`. |
| `crates/bootroom/src/state.rs` | `scenario_result_tx: Arc<AsyncMutex<Option<oneshot::Sender<WsMessage>>>>` with install/take | VERIFIED | Lines 102, 134, 216-244; install-replaces-prior test at 341-385. |
| `crates/bootroom/src/ws.rs` | Handler arm forwards `ScenarioResult` to oneshot; `ScenarioAbort` warn-arm | VERIFIED | Lines 287-326; integration tests at 395-470 cover serve mode, run mode, duplicate frame, and abort. |
| `crates/bootroom/src/transcript.rs` | Six-variant `TranscriptEvent` enum + atomic-line writer | VERIFIED | 306 lines; lines 30-76; tests for serialization stability and cross-language overflow deserialization. |
| `crates/bootroom/src/verbose.rs` | ASCII-glyph stderr formatter + non-verbose failure line | VERIFIED | 199 lines; lines 12-84. |
| `crates/bootroom/src/run_cmd.rs` | Driver: chromium discovery, axum bind, chromiumoxide launch, COI self-check, oneshot await with outer timeout, exit translation, transcript persist, explicit teardown | VERIFIED | 825 lines; all WR fixes applied (WR-02 percent-encode, WR-04 shutdown timeout, WR-05 non-bool distinction, WR-06 shell-tokenize, WR-07 PATH-walk). Inline RFC 3339 helper, no `time`/`chrono` dep. Reuses `build_router(state)`. |
| `crates/bootroom/web/scenario.js` | Browser-side sequencer: per-action Map + flat buffer for `after="any"`, ANSI strip, line-buffered evaluate, funnel lock, ws flush poll, onWrite disposable, 5 MB cap | VERIFIED | 810 lines; BL-01 fix at 193-218 (no pre-timeout match against partial line); BL-02 fix at 400-419 (flat buffer always appends, even between actions); WR-01 fix patches `bytes_truncated_estimate` at scenario end (line 658); WR-03 fix clears timeout handle (line 636); WR-08 fix uses `??` (lines 256, 469). |
| `crates/bootroom/web/app.js` | URL `?scenario=` detection + dynamic import after Hello + config load | VERIFIED | Lines 337-394 (`maybeRunScenarioFromUrlQuery`); line 670 wires it after `initialConfigLoad()` resolves. |
| `crates/bootroom/tests/run_smoke_norn_kernel.rs` | `#[ignore]`-tagged e2e gate against real NORN kernel | VERIFIED | Test ran in 1.29s; exit 0; `scenario_result.verdict=pass`. |
| `crates/bootroom/tests/fixtures/boot_smoke.toml` | Fixture with `boot_smoke` scenario + NORN banner contains-assertion | VERIFIED | Banner `[NORN ALLOC] size = 1048576` lifted verbatim via `dump_banner.rs`. |

All artifacts pass exists + substantive + wired checks.

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `run_cmd::run_inner` | `bootroom::server::build_router` | direct call at run_cmd.rs:150 | WIRED | Same router as `serve`; pinned by `run_uses_same_router.rs` integration test. |
| `run_cmd::run_inner` | `AppState::install_scenario_oneshot` | run_cmd.rs:147 | WIRED | Tx parked on AppState before Chromium navigates. |
| `ws::handle_wire` (ScenarioResult arm) | `AppState::take_scenario_result_tx` → oneshot.send | ws.rs:287-303 | WIRED | Verified by `ws_scenario_result_handoff.rs` (5 scenarios). |
| `run_cmd::run_inner` | oneshot recv with timeout | tokio::time::timeout at run_cmd.rs:212 | WIRED | Outer timeout = scenario.timeout_ms + 30_000ms. |
| `run_cmd::run_inner` | `coi_self_check` via CDP Runtime.evaluate | run_cmd.rs:208, 504-534 | WIRED | Raw eval result distinguishes 3 failure modes; ExitReason::StartupError → exit 3. |
| `app.js` Hello handler | `scenario.js::runScenario` dynamic import | app.js:670, 372-374 | WIRED | Dynamic `import('./scenario.js')` only when `?scenario=<name>` query present. |
| `scenario.js` | `ws.send(ScenarioResult)` + bufferedAmount poll | scenario.js:223-238 | WIRED | Pitfall #2 mitigation in place. |
| `scenario.js` | `master.onWrite` Disposable cleanup | scenario.js:409, 642 | WIRED | Pitfall #4 mitigation; `finally` block disposes the subscription. |
| `scenario.js` | `funnel.lockInput()`/`unlockInput()` | scenario.js:472, 648 | WIRED | Phase-3 load-bearing primitive used as planned. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace tests green | `cargo test --workspace` | ~175 tests pass; 1 ignored (the e2e gate) | PASS |
| E2E gate against real NORN kernel | `cargo test -p bootroom --test run_smoke_norn_kernel -- --ignored` | 1 passed in 1.29s | PASS |
| Phase-4 integration tests green | `cargo test -p bootroom --test run_uses_same_router --test run_log_file_jsonl --test run_verbose_stderr --test run_subcommand_exit_codes --test ws_scenario_result_handoff` | 9 tests pass | PASS |

### Requirements Coverage

| Requirement | Description | Status | Evidence |
|-------------|-------------|--------|----------|
| RUN-01 | `bootroom run` exits 0 on pass / non-zero on fail | SATISFIED | `verdict_to_exit` in run_cmd.rs; exit-code tests in `run_subcommand_exit_codes.rs`; e2e gate observes exit 0 on real kernel. |
| RUN-02 | Chromiumoxide drives Chromium | SATISFIED | `chromiumoxide::Browser::launch` at run_cmd.rs:174-181; `chromiumoxide = "0.9.1"` workspace dep. |
| RUN-03 | Same embedded assets + same WS protocol as serve | SATISFIED | `build_router(state)` reuse; `run_uses_same_router.rs` regression test. |
| RUN-04 | Substring + regex per-action serial buffers | SATISFIED | `scenario.js:212` substring `includes`; `scenario.js:216` `_compiled.test`; Rust compile-check at config.rs:450. |
| RUN-05 | ANSI strip + line-buffered match | SATISFIED | `scenario.js:194-209` with BL-01 fix preventing pre-timeout match against partial line. |
| RUN-06 | Per-action + per-scenario timeouts; structured failures | SATISFIED | scenario.js:248-256 (per-action MAX), 469 (per-scenario), `Promise.race` at 617-636; verdict values `"fail"`/`"timeout"`/`"error"` in `WsMessage::ScenarioResult.verdict`. |
| RUN-07 | Per-action buffer reset by default | SATISFIED | scenario.js:489-493 resets `buffers.set(label, [])` at every action start. |
| RUN-08 | `--log-file` JSONL transcript | SATISFIED | `transcript.rs` six-variant enum + atomic writer; `run_log_file_jsonl.rs` test. |
| RUN-09 | `--verbose` stderr progress | SATISFIED | `verbose.rs` ASCII glyphs; `run_verbose_stderr.rs` test. |
| RUN-10 | crossOriginIsolated self-check; abort with clear message | SATISFIED | run_cmd.rs:504-544 with WR-05 distinguished diagnostics; ExitReason::StartupError → exit 3. |
| CLI-02 | Shared flags via clap `#[flatten]` | SATISFIED | `cli.rs:53-66` CommonArgs; flattened into ServeArgs (line 70-71) and RunArgs (line 112-113); `cli_help_lists_shared_flags_on_run` test pins help-text rendering. |

All 11 requirements satisfied. No orphaned requirements.

### Anti-Patterns Found

No blocker or warning anti-patterns. The 10 modified Phase-4 files (`run_cmd.rs`, `state.rs`, `transcript.rs`, `verbose.rs`, `ws.rs`, `cli.rs`, `lib.rs`, `config.rs`, `scenario.js`, `app.js`) contain:

- No `TBD`, `FIXME`, or `XXX` markers.
- No `placeholder` / `coming soon` / `not yet implemented` strings.
- No `return null` stubs in production code paths.

The one match for `placeholder` is in `state.rs:143` — a doc-comment describing the test helper's behavior, not a production stub.

All 2 BLOCKER and 8 WARNING findings from `04-REVIEW.md` are addressed with traceable in-source comments:

- **BL-01** — scenario.js:197-209 (`atTimeout=false && lastNl===-1 → matchTarget=''`)
- **BL-02** — scenario.js:400-419 (flat buffer always appends; per-action gated on currentLabel)
- **WR-01** — scenario.js:377, 651-658 (track + patch `bytes_truncated_estimate` at scenario end)
- **WR-02** — run_cmd.rs:189-198, 303-318 (`encode_query_component` percent-encoder)
- **WR-03** — scenario.js:606, 617-636 (`clearTimeout` in finally)
- **WR-04** — run_cmd.rs:73, 244-255 (`SHUTDOWN_BUDGET=5s` timeout on close+wait)
- **WR-05** — run_cmd.rs:504-534 (raw → into_value typed error vs false vs eval-fail)
- **WR-06** — run_cmd.rs:166-173, 338-387 (`shell_tokenize` with single/double quote + backslash escape)
- **WR-07** — run_cmd.rs:416-432 (pure-Rust `which_via_path_env`; no external `which` shell-out)
- **WR-08** — scenario.js:256, 469 (`??` nullish fallback so `timeout_ms=0` not silently overridden)

### Human Verification Required

None. Phase 4 produces runnable code (CLI binary, integration tests, e2e gate) with comprehensive automated coverage. The single `#[ignore]`-tagged e2e test exercises the full headless path against the real NORN kernel and was executed during verification (passed in 1.29s).

### Gaps Summary

No gaps. The phase goal is observably true in the codebase:

1. **Same code path as serve** — pinned by `run_uses_same_router.rs` integration test against `build_router(state)`.
2. **Headless CI exit code** — `verdict_to_exit` translation; observed exit 0 from e2e gate on green path.
3. **Full transcript on failure** — JSONL writer + browser-side transcript building with 5 MB cap and overflow signaling.
4. **Substring + regex assertions** — both engines (Rust regex for compile-check, JS RegExp for runtime match) covered; intersection feature subset documented in config.rs.
5. **COI self-check before scenario kickoff** — explicit CDP eval gating Chromium navigation.

All 10 post-review fixes (2 BL + 8 WR) are present in the committed code with traceable comments.

---

_Verified: 2026-05-19_
_Verifier: Claude (gsd-verifier)_
