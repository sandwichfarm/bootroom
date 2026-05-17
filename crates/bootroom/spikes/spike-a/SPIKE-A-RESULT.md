---
spike: A
verdict: green
chosen_path: module-fs-write
date: 2026-05-17
qemu_wasm_sha: 0ef7b4e2814b231705d8371dd7997f5b72e70baf
chromium_version: "Chromium 148.0.7778.167 Arch Linux"
---

## Question

Can the browser swap the kernel bytes in qemu-wasm's `Module.FS` (e.g.
`Module.FS.writeFile('/pack/Image', bytes)`) at runtime and trigger a guest
reboot WITHOUT re-running emscripten's `file_packager.py`?

## Method

Spike A's question was answered **during plan 01-07's manual smoke** rather
than via a separate interactive run. The 01-07 hotfix (commit `04a31fa`)
established the substitution mechanism that production has used on every
page load since:

1. Plan 01-06 first attempted to write `/pack/Image` from a `Module.preRun`
   callback. emscripten's `addOnPreRun` uses `unshift`, which reversed the
   FIFO order — our writer landed BEFORE the data-pack extractor, then the
   extractor collided on `FS.mayCreate` (errno 20 / EEXIST in musl).
2. Plan 01-07's investigation traced the EEXIST and moved the kernel
   injection from `Module.preRun` → `Module.onRuntimeInitialized`, which
   fires AFTER the data pack has populated `/pack/` and BEFORE `callMain`.
3. Because this emscripten build does not expose `Module.FS` publicly, the
   write uses the wrapper pair
   `Module.FS_unlink('/pack/Image') + Module.FS_createDataFile('/pack', 'Image', bytes, …)`.
4. The user's real NORN kernel boots successfully through this path — both
   in the headed Chromium smoke run (manual, plan 01-07) and the
   chromiumoxide headless run (plan 01-08, verdict green for Spike B).

The complementary Spike A scaffolding under `spikes/spike-a/web/` provides
a `swap.js` DevTools probe for any future re-investigation (e.g. after a
qemu-wasm submodule bump per Pitfall 8).

## Observations

- `Module.FS_createDataFile('/pack', 'Image', newBytes, …)`: **succeeded**.
- Read-back confirms new bytes in place: **yes** (the NORN kernel boots
  and prints its boot banner; trivially, the bytes loaded by QEMU's
  `-kernel /pack/Image` argv path are the bytes we wrote).
- Reset-like Module exports enumerated: **not exhaustively probed at
  runtime**; this Phase 1 verdict relies on the page-reload path (which is
  guaranteed because the production injection runs on every page load).
  The swap.js probe ships pre-wired for future enumeration.
- Calling reset exports caused variant B boot in place: **not attempted**
  in Phase 1. This is the deferred Phase 2 open question.
- Full page reload picks up new kernel bytes: **yes** (this IS the
  production flow — every page load runs the injection sequence and the
  kernel that the running server is configured to serve gets booted).
- Console errors during swap: **none** (post the 04a31fa fix). The errno 20
  EEXIST that triggered the investigation is documented as the root cause
  fixed by moving from `preRun` to `onRuntimeInitialized`.

Spike A's underlying mechanism question is answered affirmatively by the
fact that production app.js demonstrates the swap working on every page
load against a real RISC-V kernel. The narrow remaining question is
whether the swap can be done WITHOUT a full page reload — that's a Phase 2
follow-up captured below.

## Decision

**Verdict: green**
**Chosen path: module-fs-write**

The Module.FS swap mechanism works. Phase 2's Launch button should use the
same pattern that ships in production today: fetch the new kernel bytes,
write them into `/pack/Image` via the `FS_unlink` + `FS_createDataFile`
wrapper pair, then `location.reload()` so the existing `bootGuest()` flow
re-runs and the QEMU `-kernel` argv picks up the new bytes naturally.

This avoids the `pack-rebuild` fallback (which would have required
shipping a Node-based `file_packager.py` invocation alongside the
otherwise-Rust-only bootroom binary) — a real win for the project's
"single static binary, no Node dependency" constraint.

The in-place swap (no page reload, reuse the existing qemu-wasm Worker,
trigger a CPU reset to re-read `/pack/Image`) remains an open Phase 2
optimisation. Recording it as a follow-up rather than blocking on it: a
full reload is the conservative default and preserves the simpler Launch
button surface area.

## Follow-ups

- **Pitfall 8 — submodule SHA gating:** this verdict is valid for
  qemu-wasm submodule SHA `0ef7b4e2814b231705d8371dd7997f5b72e70baf`. If
  the submodule is bumped, re-run the swap.js probe against the new build
  before shipping any Phase 2 reload behaviour. Even minor emscripten
  flag changes can move `Module.FS` exposure or rename the wrapper
  functions.
- **Phase 2 plan input:** consume `chosen_path: module-fs-write` directly
  in the Launch button design (UI-08) and Reset button design (UI-09).
  Launch = full page reload after kernel byte injection (matches Phase 1
  behaviour). Reset = same, against the currently-loaded `--kernel`.
- **Phase 2 optimisation candidate (in-place reset):** investigate
  whether `Module._qemu_system_reset` (or equivalent) is exported on this
  qemu-wasm build. If so, Launch can swap kernel bytes and trigger CPU
  reset without losing the xterm scrollback. swap.js already enumerates
  reset-like exports — that enumeration output is the entry point.
- **Reset-export enumeration deferred:** the `swap.js` probe enumerates
  reset-like Module exports but was not run interactively in Phase 1.
  Phase 2's in-place-reset spike should run swap.js once against the
  production page, capture the enumerated list, and decide whether
  in-place reset is reachable.
