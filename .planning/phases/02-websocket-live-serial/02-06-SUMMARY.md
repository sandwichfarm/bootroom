---
phase: 02-websocket-live-serial
plan: 06
subsystem: web-ui
tags: [websocket, xterm, xterm-pty, funnel, status-pill, es-module, browser]
requires:
  - phase: 02-websocket-live-serial
    provides: WsMessage enum (02-01), /ws axum handler (02-02), Funnel + bytesToB64 + b64ToBytes + keyEventToBytes (02-04), DOM contract btn-launch/btn-reset/btn-clear/btn-copy/status (02-05)
provides:
  - browser-side WS /ws client (Hello / SerialIn / SerialOut / State / Launch / Reset)
  - funnel-mounted xterm keystroke handler (sole writer to slave.write during normal byte flow — WS-02)
  - SerialOut mirror (guest serial -> WS server, batched per readable burst)
  - 4-state pill state machine (IDLE -> LOADING -> RUNNING -> HALTED) with WS State{} authority override
  - LAUNCH/RESET (best-effort WS notify + page reload) / CLEAR (xterm.clear) / COPY (clipboard with COPIED/COPY FAILED flash) handlers
  - ?pacing=N URL param overriding 15ms default for WS-arriving SerialIn (WS-03)
affects: [03-headless-driver, 04-scenarios, 05-action-buttons]

tech-stack:
  added: []  # no new deps; reuses funnel.js from 02-04, WebSocket from browser, xterm 5.3.0 / xterm-pty 0.12.0 already vendored
  patterns:
    - "attachCustomKeyEventHandler returning false to suppress xterm default master.onData dispatch (Pitfall #1 mitigation)"
    - "Single Funnel instance as the sole slave.write writer during normal byte flow (WS-02 by construction); error-path [bootroom] diagnostics are the documented out-of-band exception"
    - "Pattern 5 status-pill derivation (recomputePillLocal): RUNNING requires BOTH runtimeInitialized AND firstSerialOutSeen; explicit triggers set IDLE and HALTED"
    - "Server State{} frames override local pill machine via serverStateAuthority; onExit/onAbort clear authority so local lifecycle wins again"
    - "ws.send wrapped in try/catch with WebSocket.OPEN readyState guard; reconnect via naive setTimeout(connectWs, 1000) per <deferred> (T-02-25 accept)"
    - "JSON.parse per-frame in try/catch; handleWsFrame switch with default console.debug — malformed frames do not break onmessage (T-02-24)"
    - "rAF before location.reload to let the WS Launch/Reset frame flush before navigation tears down the document"

key-files:
  created: []
  modified:
    - crates/bootroom/web/app.js

key-decisions:
  - "attachCustomKeyEventHandler must STAY (kept from Phase 1), but with semantics inverted: Phase 1 returned false to no-op; Phase 2 returns false to suppress xterm default AND manually routes the KeyboardEvent bytes through the funnel. This single-line decision is the WS-02 single-writer guarantee."
  - "master addon stays loaded (xterm.loadAddon(master)) — it owns the OUTPUT path (guest serial -> ldisc -> xterm.write). Only the INPUT path is suppressed by the return-false handler. Verified against 02-RESEARCH.md A8."
  - "Pill RUNNING requires both runtimeInitialized AND firstSerialOutSeen (raised the bar vs. Phase 1 which used crossOriginIsolated as the trigger). Avoids the false-positive 'RUNNING' state during the silent post-runtime-init / pre-first-serial window."
  - "Server State{state:'Running'} (PascalCase per GuestState serde) is uppercased to 'RUNNING' before setPill — bridges the wire format and the CSS [data-state=...] convention without introducing a translation table."
  - "LAUNCH and RESET share the same reload mechanism (D-02 decision); kept visually distinct via index.html copy but behaviorally identical in app.js."
  - "Naive 1s setTimeout WS reconnect (no exponential backoff) accepted per <deferred>; acceptable on loopback dev tool. Plan 03+ may revisit."
  - "Single-file structure preserved (525 LOC total, 297 non-comment LOC — under the plan's 350-LOC factor-out threshold). Comments are dense by design — they carry the Pitfall #1 / Pattern 5 / out-of-band-exception rationale future readers need."

patterns-established:
  - "Plan 02-06 establishes the WS frame dispatcher template (switch on frame.type with per-branch try/catch + default console.debug) that Phase 4's headless driver can mirror on the Rust side."
  - "Pill state machine as derived-from-flags + explicit-trigger hybrid (recomputePillLocal for derivation; setPill('IDLE'/'HALTED') for explicit triggers) — clean, no enum"

requirements-completed: [UI-02, UI-03, UI-04, UI-06, UI-08, UI-09, WS-02, WS-03]

duration: ~3 min
completed: 2026-05-18
---

# Phase 2 Plan 6: WS Lifecycle + Funnel Input + Button Handlers + Pill Machine Summary

**Phase 2's interactive layer is live: keystrokes flow funnel -> slave (single-writer per WS-02), guest serial mirrors to /ws as SerialOut frames, WS-pushed State{} frames override the local 4-state pill machine, and LAUNCH/RESET/CLEAR/COPY all work — the browser session now binds together plans 01 (enum), 02 (server), 04 (funnel), and 05 (DOM).**

## Performance

- **Duration:** ~3 min (525 LOC single-file refactor with dense rationale comments; 297 non-comment LOC)
- **Started:** 2026-05-18T10:31:14Z
- **Completed:** 2026-05-18T10:34:27Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- **WS-02 enforced** — xterm input intercepted via `attachCustomKeyEventHandler` returning `false` (suppresses xterm's default master.onData dispatch — Pitfall #1 mitigation); bytes routed through the single `Funnel(slave)` with `pacingMs: 0` for user typing.
- **WS /ws lifecycle wired** — connectWs() parses frame.type and dispatches Hello (info), SerialIn (b64ToBytes -> funnel.enqueue with configurable pacingMs), and State (overrides local pill). Malformed JSON and unknown frame types are logged but never break onmessage. onclose schedules a 1s reconnect (T-02-25 accept per <deferred>).
- **SerialOut mirror** — slave.onReadable -> slave.read() -> bytesToB64 -> ws.send({type:'SerialOut',data}) when WS open; this is also the trigger for the LOADING -> RUNNING pill transition.
- **4-state pill machine** — IDLE (explicit at startup, defensive) -> LOADING (right after xterm.open) -> RUNNING (runtimeInitialized AND firstSerialOutSeen, via recomputePillLocal) -> HALTED (Module.onExit/onAbort). Server-pushed State{} frames set serverStateAuthority which overrides local derivation; onExit/onAbort clear authority so local lifecycle wins again.
- **LAUNCH/RESET** — best-effort WS notify + disable header buttons + rAF + window.location.reload(). Identical mechanism per D-02 decision; D-02 just keeps them visually distinct.
- **CLEAR** — xterm.clear() with no WS traffic.
- **COPY** — selection-or-full-buffer via `xterm.buffer.active.translateToString(true, 0, length)` (per Pitfall #5); flashes `COPIED` / `COPY FAILED` for 1500ms; failures also write `[bootroom] Copy failed: <reason>` to the terminal (T-02-29 audit trail).
- **WS-03 pacing** — `?pacing=N` URL param clamped to `>= 0`, default 15ms, applied to WS-arriving SerialIn enqueues only (user typing always uses pacingMs: 0).

## Task Commits

1. **Task 1: Refactor app.js — funnel-mounted input, SerialOut mirror, button handlers, WS lifecycle, status pill state machine** — `6b77b30` (feat)

**Plan metadata:** _committed at phase-final step below_

## Files Created/Modified

- `crates/bootroom/web/app.js` — full Phase 2 interactive layer, 264 net-lines-added (525 total). Top-level changes: import block from funnel.js; setPill('IDLE') defensive call; new Funnel(slave); intercepting attachCustomKeyEventHandler (return false); pill machine flags + recomputePillLocal; pacingMs from URLSearchParams; connectWs + handleWsFrame; slave.onReadable -> WS SerialOut mirror; 4 button handlers; Module.onRuntimeInitialized sets runtimeInitialized=true and calls recomputePillLocal instead of the Phase 1 crossOriginIsolated check; Module.onExit/onAbort clear serverStateAuthority before setPill('HALTED'); bootGuest() flow preserved otherwise; humanBytes/isoLocal/loadKernelInfo/vendor-globals-guard/FS swap/PTY poll patch/resize handler all unchanged.

## Decisions Made

See `key-decisions` in frontmatter. Headline:

- The Phase 1 `attachCustomKeyEventHandler(() => false)` is REPLACED, not removed — the suppression mechanism stays (Pitfall #1 requires it) but now also enqueues the bytes through the funnel. This is THE load-bearing decision for the WS-02 single-writer guarantee.
- Pill RUNNING requires BOTH runtimeInitialized AND firstSerialOutSeen (raised the bar from Phase 1's crossOriginIsolated trigger), which avoids the false-positive "RUNNING" state during the silent post-runtime-init / pre-first-serial window.
- GuestState's PascalCase wire format ("Running") is uppercased once at the State frame boundary to match the CSS [data-state="RUNNING"] convention — no translation table needed elsewhere.

## Deviations from Plan

None — plan executed exactly as written.

The plan's grep gates all pass on first commit:

| Gate | Result |
|---|---|
| `import.*funnel.js` | PASS |
| `new Funnel(slave)` | PASS |
| `new WebSocket` | PASS |
| `type: 'Launch'` | PASS |
| `type: 'Reset'` | PASS |
| `type: 'SerialOut'` | PASS |
| `xterm.clear()` | PASS |
| `navigator.clipboard.writeText` | PASS |
| `recomputePillLocal` | PASS |
| `serverStateAuthority` | PASS |
| `() => false` is GONE | PASS |
| `return false` present (Pitfall #1) | PASS |
| `node --check crates/bootroom/web/app.js` | exits 0 |
| `cargo build --workspace` | green |
| `cargo test --workspace` | 9 tests pass (3 ws_roundtrip + 6 bootroom_core) |

File length: 525 total LOC (297 non-comment) — under the plan's 350-LOC threshold for considering a multi-file split. Single-file structure preserved per the Phase 1 norm.

## Issues Encountered

None.

## Known Stubs

None. Plan 02-06 closes Phase 2's known-stub list (plan 05 documented the four buttons as plan-boundary-deferred stubs; this plan wires them).

## Threat Flags

None — no new security-relevant surface introduced beyond what 02-01 / 02-02 / 02-04 already covered. The threat model in 02-06-PLAN.md (T-02-22 through T-02-29) is fully addressed: handleWsFrame's per-branch try/catch and default-arm console.debug cover T-02-24; the COPY-failed audit-trail terminal write covers T-02-29; the funnel's `#drain` private-method-syntax invariant covers T-02-26; loopback single-origin + no postMessage receiver covers T-02-22; T-02-23/25/27/28 are explicit accepts per <deferred> / loopback trust model.

## User Setup Required

None — no external service configuration required. Loopback-only.

## Next Phase Readiness

- **Phase 2 complete.** All 6 plans (01 enum, 02 server, 03 auto-open, 04 funnel, 05 DOM, 06 wiring) merged on master.
- **Manual smoke checklist remains** (per 02-VALIDATION.md, deferred to phase gate, not blocking plan-close):
  1. Boot NORN kernel; observe IDLE -> LOADING -> RUNNING transitions (UI-06).
  2. Type into terminal; observe responses (UI-03).
  3. Click CLEAR; observe terminal empty (UI-04).
  4. Click COPY; paste elsewhere; verify content (UI-04).
  5. Click LAUNCH; observe reload + boot (UI-08).
  6. Click RESET; observe reload + boot (UI-09).
  7. With `?pacing=50`, inject a 10-byte SerialIn via DevTools; observe ~450ms drain (WS-03).
  8. Re-run `cargo run -p bootroom-spike-b ...` to confirm no regression in the Phase 1 headless boot path.
- **Phase 3 (headless driver) entry conditions met:** WS protocol round-trips end-to-end (server + browser both speak the same enum); the browser-side SerialOut mirror gives Phase 3 a ready-made hook for assertion capture; the LAUNCH frame is a logged-only no-op today but provides the protocol slot Phase 3 will use to trigger headless launches without page reload.

## Self-Check: PASSED

Files exist:
- `crates/bootroom/web/app.js` — FOUND (modified)
- `.planning/phases/02-websocket-live-serial/02-06-SUMMARY.md` — FOUND (this file)

Commits exist:
- `6b77b30` feat(02-06): wire WS lifecycle + funnel input + button handlers + pill machine in app.js — FOUND

---
*Phase: 02-websocket-live-serial*
*Completed: 2026-05-18*
