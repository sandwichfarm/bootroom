---
spike: B
verdict: green
chosen_path: chromiumoxide
date: 2026-05-17
chromium_user_agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/148.0.0.0 Safari/537.36
---

## Question

Can `chromiumoxide` drive `--headless=new` Chromium against a bare axum + COOP/COEP server,
observe `crossOriginIsolated === true`, and successfully execute the qemu-wasm boot path
with serial bytes flowing?

## Method

1. Spawned in-process bootroom server (`bootroom::build_router`) on ephemeral 127.0.0.1 port.
2. Launched headless Chromium via chromiumoxide 0.9.1 (`--headless=new`, `--disable-gpu`,
   `--no-sandbox`, `--disable-dev-shm-usage`, executable `/usr/bin/chromium`).
3. Navigated to the server root, waited for `wait_for_navigation()`.
4. Evaluated `self.crossOriginIsolated` and `typeof SharedArrayBuffer !== 'undefined'`.
5. Polled `#status` `data-state` and terminal `innerText.length` every 250 ms for up to 15 s.
6. Captured user-agent and computed verdict via the rubric in `src/main.rs::compute_verdict`.

Kernel fixture: `crates/bootroom/spikes/spike-b/fixtures/Image`

## Observations

| Observable | Value |
|------------|-------|
| `self.crossOriginIsolated` | `true` |
| `typeof SharedArrayBuffer !== 'undefined'` | `true` |
| Final `#status data-state` | `RUNNING` |
| Terminal char count (approx) | `49` |
| Chromium user-agent | `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/148.0.0.0 Safari/537.36` |

## Decision

**Verdict: green**
**Chosen path for Phase 4: chromiumoxide**

Full success: COI=true, SAB=true, qemu-wasm reached RUNNING, serial bytes observed in xterm.

> Phase 4 plans against chromiumoxide. No additional spike work required.

## Follow-ups

- If `amber`/`red`: re-run with a known-good RISC-V kernel fixture to disambiguate
  fixture-vs-bootroom failure modes.
- Phase 4 planning consumes `chosen_path` directly. If `playwright-subprocess`, vendor a
  Node-based Playwright driver (see 01-RESEARCH.md "Stack Patterns by Variant" → headless
  fallback). A working Playwright path was independently proven during plan 01-07's smoke
  testing using `/usr/lib/node_modules/playwright` with `executablePath: '/usr/bin/chromium'`.
- Spike A (plan 01-09) is independent of this verdict; runs next.
