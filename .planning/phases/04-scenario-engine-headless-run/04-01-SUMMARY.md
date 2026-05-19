---
phase: 04-scenario-engine-headless-run
plan: 01
subsystem: bootroom-core/protocol
tags: [ws-protocol, additive-variant, serde, scenario-engine, RUN-01, RUN-03, RUN-08, phase-4]

# Dependency graph
requires:
  - phase: 02-websocket-live-serial
    provides: "WsMessage enum + #[serde(tag = \"type\")] additive policy (no deny_unknown_fields)"
  - phase: 03-config-buttons-watcher
    provides: "WsMessage::{KernelChanged, ConfigUpdate, ConfigInvalid} + serde_json promoted to regular dep of bootroom-core"
provides:
  - "WsMessage::ScenarioStart { scenario: String }"
  - "WsMessage::ScenarioAbort { reason: String }"
  - "WsMessage::ScenarioResult { verdict: String, scenario: String, started_at: String, ended_at: String, actions: serde_json::Value, transcript: serde_json::Value, error: Option<String> }"
  - "Per-variant roundtrip tests (5 new) verifying additive-variant policy still holds under Phase-4 extension"
affects:
  - "crates/bootroom/src/ws.rs (plan 04-05 — handle_wire wiring of ScenarioResult to AppState oneshot; this plan stubs the match arms)"
  - "crates/bootroom/src/state.rs (plan 04-04 — AppState gains oneshot::Sender<ScenarioResult> field)"
  - "crates/bootroom/src/main.rs (plans 04-07/04-08 — `bootroom run` driver awaits ScenarioResult and translates verdict to exit code)"
  - "crates/bootroom/web/scenario.js (plan 04-08 — browser scenario engine emits ScenarioStart at kickoff, ScenarioResult at termination)"

# Tech tracking
tech-stack:
  added: []  # No new dependencies. serde_json::Value was already in [dependencies] from plan 03-02.
  patterns:
    - "Additive WsMessage extension under externally-tagged enum (Phase-2 02-01 policy, Phase-3 03-02 reaffirmed, Phase-4 04-01 extended)"
    - "Opaque serde_json::Value for browser-built event payloads (actions / transcript) — keeps bootroom-core schema-version-independent of browser engine event shapes"
    - "Reserved variant pattern: ScenarioStart and ScenarioAbort ship as wire-typed but un-consumed; honors 04-RESEARCH Open Question 1 (ship the wire shape now, leave server-side handling for future plans)"

key-files:
  created:
    - ".planning/phases/04-scenario-engine-headless-run/04-01-SUMMARY.md (this file)"
  modified:
    - "crates/bootroom-core/src/lib.rs (+3 variants appended after ConfigInvalid; +5 roundtrip tests appended after large_mtime_survives_i64; enum-level doc gains a Phase-4 additive-variant note)"
    - "crates/bootroom/src/ws.rs (handle_wire match: ScenarioAbort joins the server-owned warn arm; ScenarioStart + ScenarioResult get a debug-log no-op arm with a comment pointing at plans 04-05/04-07 for full wiring)"

key-decisions:
  - "Variants appended AFTER ConfigInvalid (last Phase-3 variant) — no Phase-2/3 variant reordered or renamed, preserving binary-stable wire ordering for downstream serializers"
  - "Did NOT add #[serde(deny_unknown_fields)] to WsMessage — Phase-2 policy carried through Phase 3 and reaffirmed here so future plans can add variants without breaking old clients"
  - "ScenarioResult.actions and .transcript typed as serde_json::Value (not nested structs) — the browser engine in plan 04-08 builds the payloads; server only forwards bytes to --log-file (04-06). Nested structs would force schema-version coupling on every event-shape change."
  - "ScenarioResult.started_at / .ended_at typed as String (ISO 8601 with Z suffix) — matches 04-RESEARCH Open Q3 'UTC for machine-parseable logs'. Concrete chrono::DateTime would add a transitive dep without serializer benefit."
  - "ScenarioStart is reserved (no consumer in Phase 4) — ships as a valid serializable variant so plans like --watch v2 can use it without a re-extension to the enum"
  - "Out-of-scope ws.rs match arms (Rule 3 auto-fix): treat ScenarioAbort as server-owned (client emitting it is misbehaving); treat ScenarioStart + ScenarioResult as debug-log placeholders that plans 04-05/04-07 will replace with real consumers"

patterns-established:
  - "Reserved-variant pattern: an enum member ships with full serde derive + roundtrip test but no server-side consumer. Documented in the variant's doc comment as 'reserved' with a forward-pointer to the plan that wires it."
  - "Opaque-JSON forward-compat pattern: cross-crate payloads that browser-builds use serde_json::Value rather than coupling to nested-struct schemas. Lets the browser ship event-shape additions without recompiling bootroom-core."

requirements-completed: [RUN-01, RUN-03, RUN-08]

# Metrics
duration: ~5min
completed: 2026-05-19
---

# Phase 4 Plan 01: Scenario Engine Headless Run — Wire Protocol Additions Summary

**Three additive WsMessage variants (ScenarioStart, ScenarioAbort, ScenarioResult) extend the /ws protocol for Phase 4's scenario engine and `bootroom run` driver; pure type addition with no consumer wiring yet.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-05-19T13:56:26Z
- **Completed:** 2026-05-19T14:01:45Z
- **Tasks:** 2 (1 TDD task + 1 grep-gate verification task)
- **Files modified:** 2 (crates/bootroom-core/src/lib.rs, crates/bootroom/src/ws.rs)

## Accomplishments

- WsMessage gains 3 additive variants (12 total variants: 9 Phase-2/3 + 3 Phase-4)
- 5 new roundtrip tests pass; 11 prior WsMessage tests still pass; full workspace test suite passes (47 in bootroom + 39 in bootroom-core + escape/config submodule tests)
- `cargo clippy --workspace --all-targets -- -D warnings` reports no new warnings
- No new dependencies added — `serde_json::Value` was already a regular dependency from plan 03-02
- Phase-2/3 additive-variant policy preserved: no `#[serde(deny_unknown_fields)]` on WsMessage, no variant reordered or renamed

## Task Commits

Each task was committed atomically (TDD task 1 → 2 commits: RED then GREEN; task 2 → 0 commits, read-only verification):

1. **Task 1 RED: failing roundtrip tests for new variants** — `b952e86` (test)
2. **Task 1 GREEN: add ScenarioStart/Abort/Result variants + handle_wire match-arm fix** — `883d88e` (feat)
3. **Task 2: grep-gate verification (variant count = 12, deny_unknown_fields absent from real source, all 12 variants present)** — verified, no commit (read-only check)

## Files Created/Modified

- `crates/bootroom-core/src/lib.rs` — WsMessage extended with three new variants (`ScenarioStart`, `ScenarioAbort`, `ScenarioResult`) appended after `ConfigInvalid`; enum-level doc updated with Phase-4 additive note; +5 round-trip tests in the `tests` module
- `crates/bootroom/src/ws.rs` — `handle_wire` match arm extended (Rule-3 blocking fix) so the workspace compiles: `ScenarioAbort` joins the server-owned warn arm; `ScenarioStart` / `ScenarioResult` get a debug-log no-op arm with a forward-pointer comment to plans 04-05/04-07
- `.planning/phases/04-scenario-engine-headless-run/04-01-SUMMARY.md` — this file

## Decisions Made

See `key-decisions` in frontmatter for full list. Key points:

- **No `deny_unknown_fields`:** Phase-2 policy preserved through Phase 4. Documented in the enum-level doc comment so future planners do not "fix" the omission.
- **Opaque-JSON for actions/transcript:** Avoids a schema-version coupling between `bootroom-core` and the browser engine. Browser builds the JSON; server forwards bytes.
- **Reserved-variant pattern:** `ScenarioStart` ships now even though no Phase 4 code path emits it, so a future server-driven re-run (`--watch`, v2) has the wire shape ready without re-extending the enum.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] Extended `handle_wire` match in `crates/bootroom/src/ws.rs` so the workspace compiles**
- **Found during:** Task 1 verification (`cargo test --workspace --no-fail-fast`)
- **Issue:** Adding three variants to a `#[non_exhaustive]`-free enum broke the exhaustive `match wire { ... }` in `handle_wire` (E0004). The workspace would not compile until the consumer added arms.
- **Fix:** Added `ScenarioAbort` to the existing server-owned warn arm (a client emitting it is misbehaving, identical posture to `State`/`Hello`/`KernelChanged`/`ConfigUpdate`/`ConfigInvalid`). Added a separate debug-log arm for `ScenarioStart` and `ScenarioResult` with an explicit comment that the real wiring (oneshot park, `--log-file` forwarding) lands in plans 04-05 and 04-07.
- **Files modified:** crates/bootroom/src/ws.rs
- **Verification:** `cargo test --workspace --no-fail-fast` and `cargo clippy --workspace --all-targets -- -D warnings` both pass.
- **Committed in:** `883d88e` (Task 1 GREEN commit)

**2. [Plan-doc cross-check] `deny_unknown_fields` grep gate clarified to filter doc comments**
- **Found during:** Task 2 verification
- **Issue:** The plan's Task 2 verify-script `DUF=$(grep -c 'deny_unknown_fields' "$F")` did not implement the filter the surrounding prose mandates ("Filter out comments before counting"). The Phase-2 enum-level doc contains a single intentional mention (`/// Note: #[serde(deny_unknown_fields)] is intentionally NOT applied`); a literal grep would fail the gate.
- **Fix:** Applied the documented filter when running the gate: `grep -v '^\s*//' | grep -c 'deny_unknown_fields'`. Real-source occurrences: 0. Doc-comment mention preserved.
- **Files modified:** None — verification-script convention only.
- **Verification:** Real-source count is 0; all 3 Task-2 gates pass.
- **Committed in:** N/A (no source change).

**3. [Plan-doc cross-check] `ScenarioResult` mention count: the plan asks for ≥ 6, baseline was 5 → added Phase-4 doc note to enum-level docstring**
- **Found during:** Task 1 done-criterion check
- **Issue:** `grep -c 'ScenarioResult' crates/bootroom-core/src/lib.rs` returned 5 after the GREEN commit (variant decl + 3 test-construction sites + the `"type":"ScenarioResult"` literal assertion). The plan's done-criterion is `≥ 6` ("variant declaration + doc + 3 dedicated tests + the `enum`-level reference"). Functionally fine; documentation thin.
- **Fix:** Added a Phase-4 additive-variant note to the WsMessage enum-level doc comment explicitly naming `ScenarioStart`, `ScenarioAbort`, and `ScenarioResult`. Count is now 7 (≥ 6).
- **Files modified:** crates/bootroom-core/src/lib.rs (enum-level doc, no code change)
- **Verification:** `grep -c 'ScenarioResult' crates/bootroom-core/src/lib.rs` returns 7.
- **Committed in:** `883d88e` (folded into the Task 1 GREEN commit).

---

**Total deviations:** 3 (1 blocking auto-fix, 2 plan-doc cross-checks)
**Impact on plan:** Auto-fix 1 is essential — without it the workspace does not compile, blocking the wave's downstream plans. Cross-checks 2 and 3 close documented gaps in the plan's gate scripts but do not change the source-code outcome the plan asks for. No scope creep: no Phase 4 consumer wiring was started (plans 04-04/04-05/04-07/04-08 retain their full scope).

## Issues Encountered

None — the workspace-compile breakage was anticipated by the deviation rules (Rule 3) and resolved in the same commit as the GREEN gate.

## Known Stubs

The new `ScenarioStart` and `ScenarioResult` match arms in `crates/bootroom/src/ws.rs` are intentionally inert (debug-log only) and will be replaced by:

- Plan 04-05: `ScenarioResult` arm — fetch the awaiting `oneshot::Sender<ScenarioResult>` off `AppState` and send the result to the `bootroom run` driver
- Plan 04-07: `ScenarioResult` arm — forward the transcript bytes to `--log-file` (JSONL)
- (`ScenarioStart` is `ServerOwned`-style reserved per the plan's Open Question 1 disposition and may stay inert beyond Phase 4)

This is not a stub-blocking-the-goal — the plan's goal is "wire-protocol additions, no consumer wiring yet" — so it is documented but not gating completion.

## Threat Flags

None — this plan introduces no new external input surface. All three new variants are constructed server-side or by the trusted browser engine (which lands in plan 04-08), and the consumer arms in `handle_wire` log-and-continue per the Phase-2/3 recovery posture (see threat register T-04-01-01 / T-04-01-02 in 04-01-PLAN.md).

## Next Phase Readiness

- **Plan 04-02** (next in wave) can proceed: `WsMessage::ScenarioResult { verdict, scenario, started_at, ended_at, actions, transcript, error }` is constructible by name from any crate that depends on `bootroom-core`.
- **Plan 04-04** can add the `oneshot::Sender<ScenarioResult>` field to `AppState` without further changes to `bootroom-core`.
- **Plan 04-05** can replace the `handle_wire` debug-log arm with the real oneshot-park consumer.
- **Plan 04-08** can construct `ScenarioStart` (kickoff) and `ScenarioResult` (termination) from the browser engine and `JSON.stringify` them onto the `/ws` socket — the wire shape is fully wired through serde.

## Self-Check: PASSED

Verification of claims:

- `crates/bootroom-core/src/lib.rs`: exists. `grep -c 'ScenarioResult' …` returns 7 (≥ 6 ✓). Variant count gate returns 12 ✓. Real-source `deny_unknown_fields` count is 0 ✓.
- `crates/bootroom/src/ws.rs`: exists. Match-arm extension committed in `883d88e`.
- Commit `b952e86`: found in `git log` (TDD RED).
- Commit `883d88e`: found in `git log` (TDD GREEN).
- `cargo test -p bootroom-core --lib`: 39 passed (was 34 before this plan; +5 new tests as planned).
- `cargo test --workspace --no-fail-fast`: all suites pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

---
*Phase: 04-scenario-engine-headless-run*
*Plan: 01*
*Completed: 2026-05-19*
