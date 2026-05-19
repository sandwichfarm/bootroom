---
phase: 04-scenario-engine-headless-run
plan: 04
subsystem: api
tags: [tokio, oneshot, mutex, axum, appstate, scenario-engine]

# Dependency graph
requires:
  - phase: 03-config-buttons-watcher
    provides: "AppState with kernel_canon, loaded_config, ws_broadcast, allowed_origins fields and the new()/new_for_test()/new_for_test_with_loaded() constructors"
  - phase: 04-scenario-engine-headless-run
    provides: "ScenarioResult variant on WsMessage (04-01) — used as the payload type carried by the oneshot."
provides:
  - "`AppState.scenario_result_tx`: `Arc<AsyncMutex<Option<oneshot::Sender<WsMessage>>>>` slot"
  - "`AppState::install_scenario_oneshot()` — installs a fresh oneshot, returns the receiver (replaces any prior sender)"
  - "`AppState::take_scenario_result_tx()` — take-once Option::take() of the sender"
affects: [04-05-ws-handler-scenario-result, 04-07-run-cmd-driver]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "tokio::sync::Mutex (renamed `AsyncMutex`) — used (not std::sync::Mutex) because WS handler holds it across .await"
    - "Arc<AsyncMutex<Option<oneshot::Sender<_>>>> — canonical shape for take-once handoff to a consume-by-value sender"
    - "Replace-on-install semantics: explicit one-scenario-per-invocation contract documented and unit-tested"

key-files:
  created:
    - ".planning/phases/04-scenario-engine-headless-run/04-04-SUMMARY.md"
  modified:
    - "crates/bootroom/src/state.rs — appended scenario_result_tx field + 2 helpers + 4 tests"

key-decisions:
  - "Initialize the new field unconditionally inside AppState::new instead of extending the public constructor signature. Keeps existing 04-07 and 03-* callers source-compatible; serve mode simply never calls install, run mode installs exactly once."
  - "tokio::sync::Mutex (AsyncMutex) over std::sync::Mutex. The WS handler in 04-05 acquires the lock inside an async function that .awaits before/after the critical section; std::sync::Mutex guards are !Send across await unless you drop them explicitly."
  - "Replace-on-second-install (drop prior sender) rather than reject-on-busy. The contract is one-scenario-per-invocation; race-driven double-install is not a real path in bootroom run, so the simpler semantic wins."

patterns-established:
  - "AsyncMutex import alias: `use tokio::sync::{Mutex as AsyncMutex, broadcast, oneshot};` to disambiguate from std::sync::Mutex."
  - "Take-once handoff: caller .install()s → driver .take()s once → second .take() returns None until next install. Unit-tested via scenario_oneshot_install_then_take_once."

requirements-completed: [RUN-01]

# Metrics
duration: ~10 min
completed: 2026-05-19
---

# Phase 04 Plan 04: scenario_result_tx slot on AppState

**Adds a take-once `oneshot::Sender<WsMessage>` slot to `AppState` so `bootroom run` can install a sender before launching Chromium and the `/ws` handler can fire it on a `ScenarioResult` frame.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-19T13:58:00Z
- **Completed:** 2026-05-19T14:08:44Z
- **Tasks:** 2 (1 TDD code task + 1 grep-gate verification task)
- **Files modified:** 1

## Accomplishments

- `AppState.scenario_result_tx: Arc<AsyncMutex<Option<oneshot::Sender<WsMessage>>>>` added; defaults to `None` (`Arc::new(AsyncMutex::new(None))`) in `AppState::new` without changing the public 7-arg signature.
- `install_scenario_oneshot()` and `take_scenario_result_tx()` async helpers on `AppState` codify the take-once handoff contract.
- Four new `state::tests::scenario_oneshot_*` / `appstate_clone_shares_scenario_oneshot_slot` tests pin the contract (default-None, install + take-once + send/receive roundtrip, replace-on-second-install drops prior sender, clones share one slot via `Arc::ptr_eq`).
- All 9 `state::tests::` pass; full `cargo test --workspace --no-fail-fast` passes; `cargo build --workspace` and `cargo clippy -p bootroom --lib -- -D warnings` clean.

## Task Commits

Each task was committed atomically:

1. **Task 1 RED** — Add failing tests for scenario_result_tx slot — `77f5379` (test)
2. **Task 1 GREEN** — Add scenario_result_tx field + install/take helpers — `4b83ee8` (feat)
3. **Task 2** — Grep gates pin field shape — verification-only (no code change; gates run against the GREEN commit and emit `OK`)

**Plan metadata:** appended by the orchestrator after this SUMMARY commits.

## Files Created/Modified

- `crates/bootroom/src/state.rs` — added `use tokio::sync::{Mutex as AsyncMutex, broadcast, oneshot};` import alias; appended `scenario_result_tx` field with doc-comment explaining `Arc<AsyncMutex<Option<oneshot::Sender<WsMessage>>>>` shape, serve-vs-run-mode contract, and one-scenario-per-invocation semantics; initialized the field to `Arc::new(AsyncMutex::new(None))` in `AppState::new`; added `install_scenario_oneshot()` and `take_scenario_result_tx()` async helpers; appended four `#[tokio::test]` tests.

## Decisions Made

- **Do not extend `AppState::new` signature.** Initialize `scenario_result_tx` to `None` unconditionally. Justification: 04-07 (run) calls `install_scenario_oneshot()` on the constructed state — the constructor doesn't need to know the mode. Avoids touching `server.rs` and every test fixture.
- **`tokio::sync::Mutex` (aliased `AsyncMutex`), not `std::sync::Mutex`.** WS handler (04-05) holds the slot across `.await` points; std guards are `!Send` across awaits without explicit drop, and the lock contention is rare enough that the async overhead is negligible.
- **Replace-on-install, don't reject.** A second `install_scenario_oneshot()` drops the prior sender (the prior receiver yields `RecvError`). Justification: there is no client-driven install path; the only caller is the driver, exactly once. Simpler semantic; unit-tested by `scenario_oneshot_second_install_replaces_first`.
- **`tokio::test` for the new tests.** Required because the helpers are async. Mixed sync/async tests already coexist in this module.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## Test Evidence

```
running 9 tests
test state::tests::appstate_canonicalizes_assets_dir ... ok
test state::tests::appstate_clone_shares_loaded_config ... ok
test state::tests::appstate_canonical_kernel_is_absolute ... ok
test state::tests::appstate_broadcast_subscribe_works ... ok
test state::tests::appstate_new_for_test_has_empty_config ... ok
test state::tests::scenario_oneshot_install_then_take_once ... ok
test state::tests::scenario_oneshot_default_is_none ... ok
test state::tests::appstate_clone_shares_scenario_oneshot_slot ... ok
test state::tests::scenario_oneshot_second_install_replaces_first ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out
```

Grep gates (Task 2):
```
$ grep -v '^\s*//' crates/bootroom/src/state.rs | grep -q 'scenario_result_tx:'         # OK
$ grep -v '^\s*//' crates/bootroom/src/state.rs | grep -q 'Arc<AsyncMutex<Option<oneshot::Sender<WsMessage>>>>'  # OK
$ grep -q 'pub async fn install_scenario_oneshot' crates/bootroom/src/state.rs           # OK
$ grep -q 'pub async fn take_scenario_result_tx' crates/bootroom/src/state.rs            # OK
$ grep -q 'allowed_origins: Vec<String>,' crates/bootroom/src/state.rs                   # OK
OK
```

## Next Phase Readiness

- **04-05** (`/ws` ScenarioResult handler) can now call `state.take_scenario_result_tx().await` directly. In serve mode it gets `None` and warn-logs; in run mode it gets `Some(tx)` and fires `tx.send(result)`.
- **04-07** (`run_cmd::run`) can now call `state.install_scenario_oneshot().await` before launching Chromium and `.await` the returned receiver (with timeout) to learn the scenario verdict.
- No blockers. No further changes to `state.rs` are anticipated in this phase.

## Self-Check: PASSED

- File `crates/bootroom/src/state.rs` exists and contains `scenario_result_tx`, `install_scenario_oneshot`, `take_scenario_result_tx`. — FOUND
- Commit `77f5379` (RED) — FOUND in git log.
- Commit `4b83ee8` (GREEN) — FOUND in git log.

---
*Phase: 04-scenario-engine-headless-run*
*Plan: 04*
*Completed: 2026-05-19*
