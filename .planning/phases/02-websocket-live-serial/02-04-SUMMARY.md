---
phase: 02-websocket-live-serial
plan: 04
subsystem: ui
tags: [vanilla-js, es-modules, xterm-pty, base64, pty, single-writer-invariant]

# Dependency graph
requires:
  - phase: 01-walking-skeleton
    provides: xterm + xterm-pty wiring in web/app.js, include_dir!-embedded web/ assets
provides:
  - "web/funnel.js — Funnel singleton enforcing WS-02 single-writer invariant"
  - "bytesToB64 / b64ToBytes helpers (Pitfall #3 Latin-1-safe, chunked for large bursts)"
  - "keyEventToBytes covering Enter / Backspace / Tab / Escape / arrows / Home / End / PageUp / PageDown / Delete / Ctrl-letter / printable"
  - "Per-byte pacing primitive for WS-03 (delays BETWEEN bytes, not before first)"
affects: [02-06-app-wiring, 04-headless-run, scenario-engine]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ES module private method syntax (`#drain`) for invariant-protected drain loop"
    - "Single-writer funnel pattern: enqueue appends, draining flag gates pump start"
    - "Chunked String.fromCharCode (CHUNK = 0x8000) for >125k-byte base64 encoding"
    - "Inline JSDoc manual test plan in lieu of JS runner (Phase 2 has no npm)"

key-files:
  created:
    - "crates/bootroom/web/funnel.js"
  modified: []

key-decisions:
  - "Used `#drain` ECMAScript private method syntax (browsers and Node 18+) instead of underscore convention — enforces no-external-call invariant via syntax."
  - "Manual test plan embedded as JSDoc trailer rather than a separate doc — keeps verification co-located with implementation, satisfies 02-VALIDATION.md no-runner gap acknowledgement."
  - "Did not register the funnel into app.js yet — plan 02-06 owns the wiring. Plan 02-04 ships the standalone module so plans 02-05 (server) and 02-06 (client wiring) can land independently."

patterns-established:
  - "WS-02 mitigation pattern: every byte to slave.write flows through a singleton with a draining flag"
  - "Latin-1-safe base64 via chunked String.fromCharCode loop (avoids btoa() Pitfall #3)"
  - "Vanilla ES module with no imports — fully standalone, embedded via existing include_dir! macro"

requirements-completed: [WS-02, WS-03]

# Metrics
duration: ~10min
completed: 2026-05-18
---

# Phase 2 Plan 4: web/funnel.js — Single-Writer Browser Funnel Summary

**Vanilla ES module exporting a Funnel singleton (sole writer to xterm-pty slave), Latin-1-safe base64 helpers, and a KeyboardEvent-to-bytes translator covering kernel-REPL keys — the load-bearing WS-02 mitigation for Phase 2.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-18T10:02:00Z (approx)
- **Completed:** 2026-05-18T10:12:09Z
- **Tasks:** 1
- **Files modified:** 1 (created)

## Accomplishments

- Created `crates/bootroom/web/funnel.js` (192 LOC) — valid ES module, no dependencies, no DOM coupling
- Implemented `Funnel` class with the `draining`-flag pattern from 02-RESEARCH.md Pitfall #7 (single drain loop guaranteed even under concurrent enqueue)
- Implemented `bytesToB64` / `b64ToBytes` with chunked `String.fromCharCode.apply` (CHUNK = 0x8000) — handles bytes ≥ 0x80 and large bursts without hitting V8's argument-count limit
- Implemented `keyEventToBytes` covering Enter (0x0d), Backspace (0x7f), Tab (0x09), Escape (0x1b), arrows, Home/End/PageUp/PageDown/Delete, Ctrl-letter, printable UTF-8
- Embedded an inline JSDoc manual test plan covering all five behaviors that 02-VALIDATION.md flags for headed-browser smoke during the wave-2 merge

## Task Commits

1. **Task 1: Create web/funnel.js with Funnel + base64 + keyEventToBytes + manual test plan** — `cb40545` (feat)

**Plan metadata:** (this SUMMARY commit, hash to follow)

## Files Created/Modified

- `crates/bootroom/web/funnel.js` (created, 192 LOC) — Funnel singleton class, bytesToB64, b64ToBytes, keyEventToBytes, plus module-level JSDoc explaining the WS-02 invariant + Pitfall #1 (master double-subscribe) trap and an inline manual test plan

## Decisions Made

- **Used ES `#drain` private method syntax** instead of an underscore convention. Modern browsers (the only ones that run qemu-wasm) all support this; the syntax enforces the "no external callers of the drain loop" invariant at the language level rather than by comment.
- **Embedded the manual test plan in the file itself** as a JSDoc trailer block. 02-VALIDATION.md explicitly accepted "no JS runner in Phase 2"; co-locating the verification steps with the code being verified means a future reader cannot miss them.
- **No DOM coupling, no xterm coupling, no import statements.** `Funnel` takes a `slave`-like object via constructor (the only contract is `slave.write(number[]|string)`), so the module is unit-testable with a stub if a JS runner is later introduced.
- **Did not wire the funnel into `app.js`.** That belongs to plan 02-06 (WS lifecycle + xterm.attachCustomKeyEventHandler). Shipping the module standalone lets plans 05 and 06 land in parallel in wave 2.

## Deviations from Plan

None - plan executed exactly as written. The implementation was copied verbatim from 02-RESEARCH.md Code Examples §2 with the JSDoc headers and inline manual test plan added per the action description.

## Issues Encountered

None.

## Verification Run

```
$ node --check crates/bootroom/web/funnel.js   # exit 0 — valid ES module
$ grep -c "^export " crates/bootroom/web/funnel.js
4                                                # Funnel, bytesToB64, b64ToBytes, keyEventToBytes
$ grep -q "MANUAL TEST PLAN" funnel.js          # found
$ grep -q "draining" funnel.js                  # Pitfall #7 pattern present
$ grep -q "0x8000" funnel.js                    # chunked base64 loop present
$ wc -l funnel.js                                # 192
$ cargo build --workspace                        # Finished in 6.04s
```

All automated verification commands from the plan's `<verify>` block pass. File length (192 LOC) is slightly above the 90-140 target due to the comprehensive WS-02 JSDoc header and the five-block manual test plan that the action explicitly required — net code is well within target.

## User Setup Required

None - no external service configuration required.

## Self-Check: PASSED

- File `crates/bootroom/web/funnel.js` exists (verified by `node --check` exit 0).
- Commit `cb40545` exists in `git log` (verified by `git rev-parse --short HEAD` after commit).

## Next Phase Readiness

- Plan 02-06 can `import { Funnel, bytesToB64, b64ToBytes, keyEventToBytes } from './funnel.js'` directly.
- No blocking issues. Plan 02-05 (server-side WS) and plan 02-06 (client wiring) can both proceed in wave 2.
- Phase 4's headless `bootroom run` will reuse `keyEventToBytes` and the base64 helpers when feeding scenario steps into the funnel via chromiumoxide's CDP-injected events.

---
*Phase: 02-websocket-live-serial*
*Completed: 2026-05-18*
