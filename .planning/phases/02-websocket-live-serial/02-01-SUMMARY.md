---
phase: 02-websocket-live-serial
plan: 01
subsystem: api
tags: [serde, serde_json, websocket-protocol, tagged-enum, base64, types-library]

requires:
  - phase: 01-walking-skeleton
    provides: empty bootroom-core skeleton crate, workspace serde pin (1.0.228)
provides:
  - WsMessage + GuestState enum definitions in bootroom-core
  - serde-tagged JSON wire format ({"type":"<variant>",...}) for /ws
  - serde + serde_json wiring in bootroom-core Cargo.toml
  - Six round-trip unit tests proving wire-shape stability
affects:
  - phase 02 plan 02 (axum /ws handler imports WsMessage)
  - phase 02 plan 03-06 (browser client speaks the same JSON)
  - phase 04 headless run (re-imports WsMessage unchanged)

tech-stack:
  added:
    - serde = "1.0.228" (workspace dep, now used by bootroom-core)
    - serde_json = "1" (dev-dep only — runtime callers pass strings)
  patterns:
    - "#[serde(tag = \"type\")] for externally-keyed tagged enums"
    - "Default serde representation for C-like enums (bare string variant)"
    - "Pure-types library with zero runtime I/O"

key-files:
  created: []
  modified:
    - crates/bootroom-core/src/lib.rs
    - crates/bootroom-core/Cargo.toml

key-decisions:
  - "WsMessage uses #[serde(tag = \"type\")] — the locked /ws wire format"
  - "GuestState uses default serde repr (bare string), additionally derives Copy"
  - "No #[serde(deny_unknown_fields)] — leaves room for additive Phase 4 variants (RESEARCH Open Q3)"
  - "serde_json kept in [dev-dependencies] only — runtime callers pass already-serialized strings"

patterns-established:
  - "Pattern 2 from 02-RESEARCH.md: WsMessage + GuestState in bootroom-core (single source of truth)"
  - "TDD RED-GREEN at the plan level: failing-test commit, then implementation commit"

requirements-completed: [WS-04]

duration: 6min
completed: 2026-05-18
---

# Phase 2 Plan 1: bootroom-core WsMessage + GuestState Summary

**Pure-types `WsMessage` (serde-tagged) + `GuestState` (string-variant) protocol enums in `bootroom-core`, with six round-trip serde tests pinning the locked `/ws` wire format.**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-05-18T (commit a926c45 RED phase)
- **Completed:** 2026-05-18T (commit 1d704d7 GREEN phase)
- **Tasks:** 1 (TDD: RED + GREEN)
- **Files modified:** 2

## Accomplishments

- `WsMessage` enum defined exactly per the locked `02-CONTEXT.md` `<decisions>` block — six variants: `SerialIn`, `SerialOut`, `State`, `Launch`, `Reset`, `Hello`.
- `GuestState` enum with `Idle | Loading | Running | Halted` and required `Copy` derive.
- `#[serde(tag = "type")]` produces `{"type":"<variant>",...}` JSON; unit variants serialize as `{"type":"Launch"}` (not bare strings, not `{}`).
- Per-variant doc comments record direction (host->guest, server->client, etc.) so downstream consumers don't have to re-derive intent from `02-CONTEXT.md`.
- Six unit tests committed as failing first (RED), then implementation made them pass (GREEN) — TDD gate sequence visible in `git log`.

## Task Commits

1. **Task 1 RED: failing tests for WsMessage + GuestState** — `a926c45` (test)
2. **Task 1 GREEN: implement WsMessage + GuestState enums** — `1d704d7` (feat)

No REFACTOR commit needed — file is 130 LOC including doc comments and tests; lib code itself is ~55 LOC, well under the 120-LOC soft cap.

## Files Created/Modified

- `crates/bootroom-core/src/lib.rs` — replaced empty skeleton with `WsMessage` + `GuestState` enums and six `#[cfg(test)]` round-trip tests.
- `crates/bootroom-core/Cargo.toml` — added `serde = { workspace = true }` to `[dependencies]` and `serde_json = { workspace = true }` to `[dev-dependencies]`.

## Decisions Made

None beyond what `02-CONTEXT.md` already locked. The implementation matches `02-RESEARCH.md` Pattern 2 verbatim (lines 429–467), modulo per-variant doc comments which I expanded inline to make direction explicit at the call site.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Lint] Quoted `SerialOut` in doc comment to satisfy `clippy::doc_markdown`**
- **Found during:** Task 1 GREEN verification (`cargo clippy --all-targets -- -D warnings`)
- **Issue:** clippy's `doc_markdown` lint (enabled via `pedantic` warn-level in `[lints.clippy]`) flagged the bare `SerialOut` identifier in a doc comment on `GuestState::Running`.
- **Fix:** Wrapped `SerialOut` in backticks.
- **Files modified:** `crates/bootroom-core/src/lib.rs`
- **Verification:** `cargo clippy -p bootroom-core --all-targets -- -D warnings` exits 0.
- **Committed in:** `1d704d7` (part of GREEN commit — caught before commit).

---

**Total deviations:** 1 auto-fixed (1 lint)
**Impact on plan:** Cosmetic doc fix only. No scope change, no API change.

## Issues Encountered

None. The plan was a near-verbatim copy from `02-RESEARCH.md` Pattern 2; the only surprise was the `doc_markdown` lint on the inline doc comment, which is a workspace-wide `pedantic` lint inherited from `[lints.clippy]`.

## TDD Gate Compliance

- RED gate: `a926c45` (`test(02-01): ...`) — six tests committed, compilation failed because `WsMessage` / `GuestState` / `serde_json` were not yet in scope (26 compile errors, exactly the expected "type does not exist" failures).
- GREEN gate: `1d704d7` (`feat(02-01): ...`) — `cargo test -p bootroom-core --lib` shows `6 passed; 0 failed`.
- REFACTOR gate: skipped — implementation is already minimal and idiomatic.

## Verification

- `cargo test -p bootroom-core --lib` — 6 passed, 0 failed.
- `cargo clippy -p bootroom-core --all-targets -- -D warnings` — exit 0.
- `cargo fmt -p bootroom-core -- --check` — exit 0.
- `cargo test --workspace` — full workspace green, no regressions.

## Next Phase Readiness

- Plan 02 (`/ws` handler) can now `use bootroom_core::{WsMessage, GuestState};` and compile.
- Phase 4 headless `bootroom run` driver imports the same enum unchanged (single source of truth — exactly the WS-04 design intent).
- No blockers.

## Self-Check: PASSED

- FOUND: `crates/bootroom-core/src/lib.rs` (130 lines, contains `pub enum WsMessage` and `pub enum GuestState`)
- FOUND: `crates/bootroom-core/Cargo.toml` (with `serde` dep + `serde_json` dev-dep)
- FOUND commit `a926c45` (RED — failing tests)
- FOUND commit `1d704d7` (GREEN — implementation)

---
*Phase: 02-websocket-live-serial*
*Completed: 2026-05-18*
