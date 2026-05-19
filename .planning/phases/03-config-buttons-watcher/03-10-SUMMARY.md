---
phase: 03-config-buttons-watcher
plan: 10
subsystem: web/funnel
tags: [funnel, lock-primitive, observer, ACT-04, browser-js]
requires: [crates/bootroom/web/funnel.js (Phase 2 baseline)]
provides:
  - "Funnel.lockInput()/unlockInput() idempotent input-lock methods"
  - "Funnel.locked: boolean public flag"
  - "setLockObserver(cb) module-level export — single-observer registry with try/catch isolation"
affects:
  - crates/bootroom/web/app.js (Plan 11 wires xterm.onData + action-btn caller-side guards; observer drives BUSY pill + .action-btn[disabled])
tech-stack:
  added: []
  patterns:
    - "Module-level observer registry with defensive non-function fallback + try/catch isolation (mirrors WR-05 writeFromLower try/catch in #drain)"
    - "Idempotent state-transition methods (early-return on no-op, observer NOT re-fired)"
key-files:
  created: []
  modified:
    - "crates/bootroom/web/funnel.js (+109 LOC: locked flag, lockInput/unlockInput, setLockObserver, _notifyLockObserver, doc paragraph, 6th manual-test case)"
decisions:
  - "enqueue + #drain unchanged: caller-side enforcement (per 03-CONTEXT.md `<decisions>` Funnel input lock primitive) so server-initiated SerialIn frames don't self-block when the scenario engine holds the lock"
  - "Single observer (not a Set) — app.js is the sole expected consumer; replace-on-register is the locked API shape"
  - "Defensive non-function -> no-op fallback in setLockObserver (avoids TypeError inside lockInput call site)"
  - "Observer-throws wrapped in try/catch with console.warn — lock state change still takes effect (T-03-10-01 mitigation)"
metrics:
  duration: "~10 min"
  completed: "2026-05-19"
---

# Phase 3 Plan 10: Funnel Input-Lock Primitive Summary

`Funnel` now exposes the lock API surface Phase 4's scenario engine expects: an
idempotent `lockInput()`/`unlockInput()` pair, a public `locked: boolean` flag,
and a module-level `setLockObserver(cb)` export for app.js (Plan 11) to drive
the BUSY pill state and disable `.action-btn` elements. The lock ships UNUSED
inside the funnel itself — `enqueue` and `#drain` are byte-identical to Phase 2.

## The Three Additions

### 1. `this.locked: boolean` flag (default `false`)

Added at the end of the constructor body, after `this.draining = false;`. JSDoc
notes the observer-by-callers contract and links to the locked decision in
`03-CONTEXT.md`.

### 2. `lockInput()` / `unlockInput()` instance methods

Placed between `enqueue` and `#drain` to keep the public API contiguous. Both
are idempotent: early-return on the no-op path so the observer is NOT re-fired
when the lock state is already at the requested value. The lockInput JSDoc
explicitly notes the caller-side enforcement semantics.

### 3. Module-level observer registry + `setLockObserver` export

Placed after the `Funnel` class, before `bytesToB64`:

- `let _lockObserver = () => {};` — module-private, default no-op so methods are
  safe to call before any observer is registered.
- `export function setLockObserver(cb)` — defensive non-function fallback to no-op.
- `function _notifyLockObserver(value)` — try/catch around the observer call so
  an observer throw cannot block the state transition (T-03-10-01 mitigation;
  mirrors the WR-05 `writeFromLower` try/catch in `#drain`).

## Locked Decision: Why `enqueue` Does NOT Short-Circuit on `this.locked`

Per `.planning/phases/03-config-buttons-watcher/03-CONTEXT.md` `<decisions>`
"Funnel input lock primitive": server-initiated `SerialIn` WS frames (the
scenario engine's own writes to drive the guest during scenario steps) MUST
continue to reach the guest while the lock is held — otherwise the scenario
engine would self-block as soon as it called `lockInput()`. The lock is
therefore enforced at the CALLER:

- `xterm.onData`'s key-event handler checks `funnel.locked` and short-circuits
  before calling `enqueue` (Plan 11).
- `.action-btn` click handlers check `funnel.locked` and short-circuit before
  calling `enqueue` (Plan 11).

The funnel itself remains lock-agnostic. This decision is recorded in the file's
module doc comment in the new "Lock primitive (Phase 3 addition)" paragraph,
and in each method's JSDoc.

## Manual Test Plan — 6th Test Case Added

Per UI-SPEC Interaction Contract 9 (the manual DevTools test for the lock
primitive). The 6th block at the bottom of `funnel.js` walks through:

1. Register an observer that logs `lock changed → <bool>`.
2. `funnel.lockInput()` fires the observer with `true`; `funnel.locked === true`.
3. A second `funnel.lockInput()` does NOT fire the observer (idempotent).
4. `funnel.unlockInput()` fires the observer with `false`; `funnel.locked === false`.
5. A second `funnel.unlockInput()` does NOT fire the observer (idempotent).
6. Notes the post-Plan-11 behavior: pill flips to `BUSY`, `.action-btn` disabled.

## Plan 11 Hand-Off

Plan 11 owns:
- The caller-side guards in `xterm.onData` (the `attachCustomKeyEventHandler`
  callback that already exists in app.js gains a `if (funnel.locked) return false;`
  check before invoking `keyEventToBytes` + `funnel.enqueue`).
- The `.action-btn` click handler's `if (funnel.locked) return;` guard.
- The single observer registration: `setLockObserver(locked => { ... })` that
  flips the status pill to a new `BUSY` state when `locked === true` (and restores
  the prior pill state on `false`) and toggles `.disabled` on every `.action-btn`.
- The `BUSY` pill style additions in `style.css` and the corresponding CSS
  class state (5-state pill machine: IDLE / LOADING / RUNNING / HALTED / BUSY).

## Verification

- `node --check crates/bootroom/web/funnel.js` → exit 0 ✔
- `grep -c 'lockInput'` → 14 ✔
- `grep -c 'unlockInput'` → 9 ✔
- `grep -c 'setLockObserver'` → 5 ✔
- `grep -c 'this.locked = false'` → 2 ✔ (declaration + JSDoc reference; both intentional)
- `cargo build -p bootroom` → clean ✔ (include_dir! re-embeds the updated funnel.js)
- `cargo test -p bootroom --test embedded_assets_served` → 3 passed ✔

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Pre-staged out-of-scope style.css inclusion in the feat commit**
- **Found during:** post-commit deletion check after Task 1's feat commit.
- **Issue:** `git add crates/bootroom/web/style.css`-pre-staged content from a
  prior context (`.action-btn` styling rules — almost certainly intended for
  Plan 11's button-rendering work) was already in the index when `git commit`
  ran, so the 03-10 feat commit accidentally included 93 lines of unrelated
  CSS additions. Plan 03-10's scope is funnel.js only.
- **Fix:** Created a follow-up `fix(03-10)` commit that restores style.css to
  its pre-03-10 HEAD state. The .action-btn additions remain in the working
  tree as an unstaged modification, ready for the appropriate later plan to
  commit.
- **Files modified:** `crates/bootroom/web/style.css` (revert).
- **Commit:** `18ff166`.

### Auth gates encountered

None.

## Known Stubs

None. The lock API is fully implemented; Phase 3 ships it deliberately UNUSED
inside the funnel (the locked design decision). Plan 11 wires the consumer
side. Phase 4 is the first non-Plan-11 consumer.

## Threat Flags

None — no new trust boundary or network surface. STRIDE register in
`03-10-PLAN.md` (T-03-10-01 through -04) covers the JS-only surface; all
dispositions held (one mitigate via try/catch + three accepts per the loopback
dev-tool threat model).

## Commits

- `57bff45` feat(03-10): add Funnel lockInput/unlockInput + setLockObserver (ACT-04)
- `18ff166` fix(03-10): remove style.css from 03-10 commit (out-of-scope)

## Self-Check: PASSED

- File `crates/bootroom/web/funnel.js` exists ✔
- Commit `57bff45` in `git log` ✔
- Commit `18ff166` in `git log` ✔
- All 4 grep gates pass ✔
- `node --check` clean ✔
- `cargo test -p bootroom --test embedded_assets_served` passes ✔
