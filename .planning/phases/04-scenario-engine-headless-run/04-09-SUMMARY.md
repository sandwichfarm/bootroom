---
phase: 04-scenario-engine-headless-run
plan: 09
subsystem: web/browser
tags: [run-mode, scenario-engine, url-query, dynamic-import]
dependency-graph:
  requires:
    - 04-01  # WsMessage::ScenarioResult variant (the error-frame inline path)
    - 04-08  # scenario.js engine module (runScenario export)
  provides:
    - url-query-run-mode  # ?scenario=<name> entry point in app.js
  affects:
    - crates/bootroom/web/app.js
tech-stack:
  added: []
  patterns:
    - dynamic-import-for-lazy-loading
    - module-scope-singleton-references (funnel/master/ws shared with engine)
    - urlsearchparams-once-at-module-load
key-files:
  created: []
  modified:
    - crates/bootroom/web/app.js
decisions:
  - "Unknown-scenario error frame is constructed inline in app.js rather than relying on scenario.js's defense-in-depth path. Two reasons: (1) the engine never has to load if we already know the lookup will fail — saves a fetch+parse on the unhappy path; (2) the inline path keeps the `app.js → engine` contract one-way (app.js calls runScenario with a valid scenario or not at all)."
  - "Re-fetch /api/config inside the helper rather than threading the config object out of initialConfigLoad. The fetch is browser-HTTP-cached after the first call (same URL, same session) so the cost is one parse, not a network round-trip. Threading would require restructuring initialConfigLoad's return contract for a single caller."
  - "Engine receives the SAME module-scope `funnel`, `master`, `ws` locals app.js uses. No duplicate sockets, no duplicate xterm-pty instances. `ws` is captured at the call site (not via a getter), so a reconnect mid-scenario presents the engine with a stale reference — accepted: 04-08's send-guards (ws.readyState === OPEN) handle this gracefully and the outer-timeout backstop in run_cmd covers the dead-socket case."
metrics:
  duration: ~12min
  completed: 2026-05-19
---

# Phase 04 Plan 09: scenario-engine-headless-run — Wire ?scenario= URL Query

URL-query run-mode detection plus dynamic `import('./scenario.js')` integration in `app.js`, closing the loop between `bootroom run` (Chromium navigation) and the browser engine.

## What Was Built

`crates/bootroom/web/app.js` now reads `URLSearchParams.get('scenario')` once at module load (joining the existing `?pacing` parse), defines `maybeRunScenarioFromUrlQuery()`, and chains it onto the existing `initialConfigLoad()` promise inside the WS `Hello` handler. Serve mode (no `?scenario=`) takes no new code paths — the helper returns early, `scenario.js` is never fetched.

When `?scenario=<name>` is set:

1. After `initialConfigLoad()` resolves (so the buttons panel is already populated), the helper runs.
2. Re-fetches `/api/config` (HTTP-cached, cheap) and looks up `scenarios.find(s => s.name === scenarioName)`.
3. **If found:** dynamically `import('./scenario.js')` and `await runScenario(scenario, config.actions, { ws, funnel, master })`. The engine takes over from there.
4. **If not found OR import/run throws:** constructs and sends a `ScenarioResult{verdict:'error', error: '...'}` frame inline so `run_cmd`'s exit-code translation (04-05) produces exit 1 immediately, without waiting for the outer timeout.

### Files

**Modified**

- `crates/bootroom/web/app.js` (+94 / -1)
  - Added `const scenarioName = urlParams.get('scenario');` next to the existing `?pacing` parse (around line 513).
  - Added `async function maybeRunScenarioFromUrlQuery()` (after `initialConfigLoad`, around line 317).
  - Replaced the Hello branch's `initialConfigLoad().catch(...)` with `initialConfigLoad().then(maybeRunScenarioFromUrlQuery).catch(...)`.

## How It Connects

- **Upstream (Rust side):** `bootroom run` (04-07) launches Chromium and navigates to `http://127.0.0.1:<port>/?scenario=<name>`. That URL hits the existing static handler and serves `index.html`, which loads `app.js`. This plan is what turns the query string into a scenario run.
- **Downstream (browser side):** The engine module from 04-08 (`scenario.js`, 732 LOC) is the sole consumer of the `runScenario` import. No other call site exists in the codebase.

## Commits

| Hash      | Type | Description                                                |
| --------- | ---- | ---------------------------------------------------------- |
| `193c33c` | feat | wire ?scenario= URL query to dynamic scenario.js import    |

## Deviations from Plan

None — plan executed exactly as written. The plan's Step 2 helper body was adopted verbatim with one cosmetic change: the two error-frame constructions both compute `new Date().toISOString()` into a single `nowIso` local so `started_at` and `ended_at` are byte-identical strings (matches what tests would assert against).

## Verification

All five grep gates from the plan emit `OK`:

```
node --check crates/bootroom/web/app.js              # passes
urlParams.get('scenario')                            # present
async function maybeRunScenarioFromUrlQuery          # present
import('./scenario.js')                              # present (line 373)
.then(maybeRunScenarioFromUrlQuery)                  # present (Hello branch)
urlParams.get('pacing')                              # still present (no Phase 2 regression)
```

`grep -nE "(import|from).*scenario\.js" crates/bootroom/web/app.js` confirms the dynamic import is the only entry into `scenario.js` — there is no static `import ... from './scenario.js'` anywhere in `app.js`.

## Known Stubs

None. The helper is feature-complete; the engine it calls into is feature-complete (04-08). End-to-end smoke is gated on 04-10's integration test.

## Self-Check: PASSED

- FOUND: `crates/bootroom/web/app.js` (94 lines added)
- FOUND: commit `193c33c` in git log
- FOUND: `.planning/phases/04-scenario-engine-headless-run/04-09-SUMMARY.md` (this file)
