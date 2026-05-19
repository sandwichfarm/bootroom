---
phase: 04-scenario-engine-headless-run
plan: 05
subsystem: infra
tags: [rust, axum, tokio, websocket, oneshot, scenario-engine]

# Dependency graph
requires:
  - phase: 04-scenario-engine-headless-run
    provides: WsMessage::ScenarioResult / ScenarioStart / ScenarioAbort variants (04-01)
  - phase: 04-scenario-engine-headless-run
    provides: AppState.scenario_result_tx slot + install/take helpers (04-04)
provides:
  - handle_wire arms for the three new scenario variants
  - ScenarioResult -> oneshot forwarding (load-bearing handoff for RUN-01)
  - Integration test pinning the same-router contract (RUN-03)
affects: [04-07 run_cmd driver, 04-08 browser scenario engine]

# Tech tracking
tech-stack:
  added: []  # No new crates; existing tokio-tungstenite + futures-util.
  patterns:
    - "Take-once oneshot handoff via Arc<AsyncMutex<Option<Sender>>>"
    - "Warn-and-continue for protocol-error frames (consistent with Phase-3 posture)"

key-files:
  created:
    - crates/bootroom/tests/ws_scenario_result_handoff.rs
  modified:
    - crates/bootroom/src/ws.rs

key-decisions:
  - "Use match guard `ws @ WsMessage::ScenarioResult { .. }` so the entire frame is forwarded by value to the oneshot — preserves the byte-exact transcript without re-cloning."
  - "Drop `#[allow(clippy::unused_async)]` from handle_wire and use the real `state` parameter; the function now actually awaits on `state.take_scenario_result_tx()`."
  - "Duplicate ScenarioResult in run mode warns-and-continues rather than disconnecting — same posture as Phase-3 protocol-error frames; the operator's WS stays usable after a run completes."

patterns-established:
  - "Integration tests that need the WS handshake's Origin gate use `state.allowed_origins = vec![format!(\"http://{bound}\")]` after `new_for_test` and before `Arc::new`."

requirements-completed: [RUN-01, RUN-03]

# Metrics
duration: 60min
completed: 2026-05-19
---

# Phase 04 Plan 05: scenario-engine-headless-run — handle_wire scenario arms Summary

**`ws.rs::handle_wire` now forwards `WsMessage::ScenarioResult` frames out of the WS reader thread to the `oneshot::Sender` installed on `AppState` by `bootroom run`, closing the browser -> server handoff with byte-exact delivery and a real-WS integration test as the regression gate.**

## Performance

- **Duration:** 60 min
- **Started:** 2026-05-19T14:20:17Z
- **Completed:** 2026-05-19T15:20:28Z (approx)
- **Tasks:** 3 (Task 3 is a verification step; produced no commit)
- **Files modified:** 1 source + 1 new test = 2 files

## Accomplishments

- Three new `handle_wire` match arms: `ScenarioStart` (no-op debug log), `ScenarioResult` (oneshot handoff), `ScenarioAbort` (warn — server-owned).
- `state` parameter is now load-bearing in `handle_wire`; `#[allow(clippy::unused_async)]` removed.
- Five new `tokio::test` unit tests inside `ws::tests` cover all three arms in both serve and run modes plus the duplicate-frame branch (slot already taken).
- One new integration test (`ws_scenario_result_handoff::scenario_result_frame_lands_on_oneshot`) drives a real WS roundtrip on `127.0.0.1:0` and asserts the oneshot receiver yields the verdict byte-for-byte under a 5 s timeout.
- All eight `ws::tests` unit tests pass (3 existing `truncate_for_log_*` + 5 new arm tests). Phase-3 `ws_roundtrip` regression suite (5 tests) still green.

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing handle_wire arms** — `d7a0bed` (test)
2. **Task 1 GREEN: wire handle_wire to forward ScenarioResult** — `90654f3` (feat)
3. **Task 2: integration test — real WS roundtrip** — `dfb9164` (test)

_Task 3 ("Grep gates") is a verification step; it produces no file changes and emits `OK` against the post-Task-1 source. No commit needed._

## Files Created/Modified

- `crates/bootroom/src/ws.rs` — `handle_wire` gains three new arms; `_state` -> `state`; five new async unit tests appended to `mod tests`.
- `crates/bootroom/tests/ws_scenario_result_handoff.rs` — NEW. End-to-end roundtrip pinning the same-router contract (RUN-03) plus oneshot delivery (04-04, 04-05).

## Decisions Made

- **Match guard `ws @ WsMessage::ScenarioResult { .. }`** instead of destructuring and re-constructing the variant — preserves the original `WsMessage` value by-binding, avoiding any field copy and keeping the wire shape exact for downstream JSONL transcript writers in 04-06.
- **`take_scenario_result_tx().await` is called inside the arm** (not earlier in the function) so the mutex is held for the minimum window — only the slot read, then released before `tx.send(...)` runs. No deadlock surface across the oneshot send.
- **Duplicate-frame branch warns instead of erroring.** The operator's `/ws` connection should survive a misbehaving client; `bootroom run` is the only consumer of the first frame and shuts down immediately after, so a duplicate after the slot is taken is by definition late and ignorable.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] clippy::doc_markdown errors on new test doc comments**

- **Found during:** Task 1 (post-GREEN clippy check before commit)
- **Issue:** Five new `///` doc comments on the test functions referenced bare PascalCase variant names (`ScenarioResult`, `ScenarioStart`, `ScenarioAbort`) that clippy's `doc_markdown` lint requires to be backtick-quoted. The workspace lints elevate clippy::pedantic to warnings and CI fails on `-D warnings`, so this would have been a CI block.
- **Fix:** Wrapped each bare type name in backticks inside the doc comments (`ScenarioResult` -> `` `ScenarioResult` ``, etc.). No behavior change.
- **Files modified:** `crates/bootroom/src/ws.rs`
- **Verification:** `cargo clippy -p bootroom --lib --tests -- -D warnings` finishes clean.
- **Committed in:** `90654f3` (part of the Task 1 GREEN commit — the fix landed before the commit was written).

---

**Total deviations:** 1 auto-fixed (1 bug — clippy strict-lint compliance).
**Impact on plan:** None on behavior. Pre-empts a CI failure that the plan didn't anticipate (the workspace's `clippy::pedantic = warn` + `-D warnings` posture treats doc-markdown gaps as hard errors).

## Issues Encountered

- During the RED-phase test run, two earlier cargo-test invocations were left orphaned from prior background polling and contended on the build lock, leading to one process hanging for 25+ minutes on the test-binary process. Cleaned up by killing the orphaned processes; the next cargo invocation completed in ~15 s with the expected RED signal (one hung-on-rx.await test + one assertion failure). No code impact.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- `04-07` (`run_cmd` driver) can now `state.install_scenario_oneshot().await` before launching Chromium and `tokio::time::timeout(outer_timeout, rx.recv()).await` to receive the verdict — the WS reader will deliver byte-for-byte.
- `04-08` (browser scenario engine) was merged in Wave 2 and emits `ScenarioResult` JSON on the WS; this plan completes its server-side consumer. End-to-end (browser -> ws.rs -> oneshot -> run_cmd) is one plan away from working.

## Self-Check: PASSED

- `crates/bootroom/src/ws.rs` exists and contains the three new arms + `take_scenario_result_tx` call (verified by Task 3 grep gates).
- `crates/bootroom/tests/ws_scenario_result_handoff.rs` exists and `cargo test -p bootroom --test ws_scenario_result_handoff` reports 1 test passing.
- Commits exist on the worktree branch:
  - `d7a0bed` — test(04-05): add failing handle_wire arms
  - `90654f3` — feat(04-05): wire handle_wire to forward ScenarioResult to oneshot
  - `dfb9164` — test(04-05): integration — ScenarioResult roundtrip through real WS

---
*Phase: 04-scenario-engine-headless-run*
*Completed: 2026-05-19*
