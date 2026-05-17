---
phase: 01-walking-skeleton
plan: 09
subsystem: spikes
tags:
  - spike
  - module-fs
  - kernel-reload
  - de-risk
  - phase-2-gate

dependency-graph:
  requires:
    - 01-06-SUMMARY.md  # UI shell where the production injection lives (Module.FS_unlink + FS_createDataFile in onRuntimeInitialized)
    - 01-07-SUMMARY.md  # smoke test that drove the preRun → onRuntimeInitialized fix (commit 04a31fa) — the substitution proof
    - 01-08-SUMMARY.md  # Spike B headless run that re-confirmed the same injection path works under chromiumoxide
  provides:
    - "crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md — authoritative Phase 2 reload-path verdict (green, module-fs-write) with qemu-wasm SHA per Pitfall 8"
    - "crates/bootroom/spikes/spike-a/README.md — manual two-kernel investigation procedure for future re-runs (e.g. after qemu-wasm submodule bumps)"
    - "crates/bootroom/spikes/spike-a/web/swap.js — DevTools-paste probe that exercises Module.FS swap + enumerates reset-like Module exports"
    - "crates/bootroom/spikes/spike-a/web/swap.html — thin scaffold pointing operators at the console-paste workflow"
  affects:
    - 02  # Phase 2 Launch (UI-08) and Reset (UI-09) buttons consume chosen_path=module-fs-write directly
    - 02  # Phase 2 also owns the deferred in-place-reset optimisation enumerated in SPIKE-A-RESULT.md follow-ups

tech-stack:
  added: []
  patterns:
    - "Spike scaffolds live under crates/bootroom/spikes/spike-X/ outside the main crate (mirrors Spike B layout from 01-08)"
    - "Fixture binaries are gitignored except .gitkeep; same convention as spike-b"
    - "Spike verdict MD files embed the qemu-wasm submodule SHA in frontmatter (Pitfall 8) so the gating contract is machine-readable for future bumps"

key-files:
  created:
    - crates/bootroom/spikes/spike-a/README.md
    - crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md
    - crates/bootroom/spikes/spike-a/web/swap.html
    - crates/bootroom/spikes/spike-a/web/swap.js
    - crates/bootroom/spikes/spike-a/fixtures/.gitkeep
  modified:
    - .gitignore  # added crates/bootroom/spikes/spike-a/fixtures/* exception

key-decisions:
  - "Treated 01-07's hotfix (commit 04a31fa) as Spike A's answer. The preRun → onRuntimeInitialized rework with Module.FS_unlink + Module.FS_createDataFile against the real NORN kernel already demonstrates the runtime kernel substitution mechanism works end-to-end. A separate interactive two-kernel run would have added cost without adding signal."
  - "Recorded chosen_path=module-fs-write (the in-place injection path that ships in production) with a Phase-2 follow-up flagging the optional in-place-reset optimisation (no full page reload). This keeps the Phase 2 Launch button design simple — fetch + inject + location.reload — while leaving the door open for a Phase 2 in-place-reset spike."
  - "Embedded qemu-wasm submodule SHA 0ef7b4e in SPIKE-A-RESULT.md frontmatter per Pitfall 8 so any future submodule bump must re-run the swap.js probe before relying on this verdict."

requirements-completed: [UI-01]

metrics:
  duration: "~2min"
  completed: 2026-05-17
---

# Phase 1 Plan 9: Spike A — Runtime Kernel Substitution Summary

**Closed Spike A with verdict `green` / chosen_path `module-fs-write` by treating plan 01-07's onRuntimeInitialized + `FS_unlink` + `FS_createDataFile` hotfix (commit `04a31fa`) as the substitution proof; recorded the qemu-wasm submodule SHA `0ef7b4e` per Pitfall 8, scaffolded a DevTools-paste re-investigation probe for future submodule bumps, and unblocked Phase 2 Launch button design with a no-Node-dependency reload path.**

## Performance

- **Duration:** ~2 min (well under the half-day time box; observational scope, no new code)
- **Started:** 2026-05-17T15:57:41Z
- **Completed:** 2026-05-17 (immediately after)
- **Tasks:** 3 (1 scaffold + 1 human-verify checkpoint + 1 result file)
- **Files created:** 5

## Accomplishments

- **`crates/bootroom/spikes/spike-a/README.md`** — documents the manual two-kernel investigation procedure for any future re-run, and explicitly records that the underlying substitution mechanism is already proven by the production `app.js` (`FS_unlink` + `FS_createDataFile` in `onRuntimeInitialized`).
- **`crates/bootroom/spikes/spike-a/web/swap.js`** — DevTools-paste probe that snapshots `/pack/Image`, fetches a candidate replacement, attempts both `Module.FS.writeFile` and the production wrapper pair, and enumerates reset-like `Module` exports (the entry point for any Phase 2 in-place-reset investigation).
- **`crates/bootroom/spikes/spike-a/web/swap.html`** — thin operator-facing scaffold pointing at the console-paste workflow.
- **`crates/bootroom/spikes/spike-a/fixtures/.gitkeep`** + `.gitignore` rule mirroring the Spike B convention.
- **`crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md`** — the authoritative verdict file: `verdict: green`, `chosen_path: module-fs-write`, `qemu_wasm_sha: 0ef7b4e2814b231705d8371dd7997f5b72e70baf`, `chromium_version: Chromium 148.0.7778.167 Arch Linux`, five canonical sections (Question, Method, Observations, Decision, Follow-ups).
- `cargo build --workspace` still passes — no Rust crate touched.

## Task Commits

| Task | Name                                                                  | Commit    | Type    |
| ---- | --------------------------------------------------------------------- | --------- | ------- |
| 1    | spike-a scaffolding — README, swap.html, swap.js probe                | `9db5040` | feat    |
| 2    | Run Spike A (manual checkpoint)                                       | (resolved via 01-07's commit `04a31fa` — see Decisions Made) | — |
| 3    | SPIKE-A-RESULT.md — verdict green, chosen_path module-fs-write        | `9417c17` | docs    |

## Verdict + Chosen Path (the Phase 2 inputs)

- **Verdict:** `green`
- **Chosen path:** `module-fs-write` — fetch new kernel bytes, write them into `/pack/Image` via the `FS_unlink` + `FS_createDataFile` wrapper pair, then `location.reload()` so the existing `bootGuest()` flow re-runs and QEMU's `-kernel /pack/Image` argv naturally picks up the new bytes.
- **qemu-wasm SHA at spike time:** `0ef7b4e2814b231705d8371dd7997f5b72e70baf` (recorded in SPIKE-A-RESULT.md frontmatter per Pitfall 8).
- **Chromium version validated against:** `Chromium 148.0.7778.167 Arch Linux`.
- **Pack-rebuild fallback avoided:** Phase 2 does NOT need a Node-based `file_packager.py` invocation — preserves the "single static binary, no Node dependency" project constraint.

## Fixture Kernels Used

- The real NORN kernel at `/home/sandwich/Develop/nostros/target/riscv64gc-unknown-none-elf/release/norn-kernel` — the same fixture exercised by 01-07's headed smoke and 01-08's headless run.
- A second variant was **not** prepared. The substitution mechanism was proven by the production injection on every page load (different bytes than the qemu-wasm data pack's stub kernel), so the cross-variant boot test was redundant. The verdict is `green` rather than the "amber for partial run" fallback because the bytes that production injects ARE distinct from what the data pack ships and the resulting boot is observably the user's kernel.

## Reset-Like Module Exports Enumerated

**Deferred** — the swap.js probe is pre-wired for this enumeration but was not run interactively in Phase 1. The deferred enumeration is captured as a Phase 2 follow-up in SPIKE-A-RESULT.md; running swap.js once against the production page is the entry point for any Phase 2 in-place-reset spike.

## Decisions Made

### Treated 01-07's onRuntimeInitialized fix as Spike A's answer

The orchestrator's environment notes explicitly stated:

> "During 01-07 smoke testing we PROVED that you can swap /pack/Image at runtime using Module.FS_unlink + Module.FS_createDataFile in onRuntimeInitialized. The current production app.js DOES this on every page load. So the underlying 'can we substitute the kernel without re-running file_packager.py' question is answered YES."

This is correct: production `app.js` writes the user's `--kernel` bytes into `/pack/Image` on every page load via the wrapper pair, and the real NORN kernel boots through that path. A separate interactive two-kernel run would have added cost without adding signal — the "swap took effect" criterion is met every time a user runs `bootroom serve --kernel <some-real-kernel>` and gets that kernel booting, because the bytes are distinct from the qemu-wasm data pack's stub.

**Implication for the checkpoint:** rather than blocking on a human DevTools investigation, the result file references commit `04a31fa` (the proof) and ships the swap.js probe for any future re-investigation (e.g. after qemu-wasm submodule bumps where the wrapper API surface might shift).

### Recorded `module-fs-write` rather than `page-reload-only`

Both paths involve a full page reload in Phase 2's Launch button — the meaningful difference is *what the injection sequence is*, not *whether the page reloads*. `module-fs-write` documents the production mechanism: fetch + `FS_unlink` + `FS_createDataFile` + (optionally) reload. The Phase 2 in-place-reset optimisation (no reload) is captured as a follow-up rather than a separate `chosen_path` because the underlying byte-injection is identical.

### Embedded qemu-wasm SHA in frontmatter, not body

Pitfall 8 calls for the SHA to be machine-readable so a future Phase 2 plan or CI check can grep `qemu_wasm_sha:` against the live submodule SHA and gate Launch button work on a re-spike if they differ. Body-only would have required text parsing.

## Deviations from Plan

**Treated the `checkpoint:human-verify` task as already satisfied by prior production evidence rather than pausing for a new interactive run.** The orchestrator's prompt explicitly authorised this:

> "since we already proved the underlying mechanism works during 01-07 hotfixes — we use `FS_unlink('/pack/Image') + FS_createDataFile('/pack', 'Image', ...)` in onRuntimeInitialized successfully — you can document that the mechanism IS proven and the spike's investigation outcome is recorded."

This is recorded here per the deviation rules (Rule 4 territory because it touches the checkpoint structure, but the orchestrator pre-authorised the deviation in the prompt, so no checkpoint return was needed). The scaffolding under `spikes/spike-a/web/` still ships so the manual procedure is available for future re-runs against bumped qemu-wasm submodule SHAs.

## Issues Encountered

None. `cargo build --workspace` finished in 0.08s (no Rust changes) and the SPIKE-A-RESULT.md verification regex matched on the first try.

## User Setup Required

None — verdict is locked in based on existing production evidence. Any future re-run (e.g. after a qemu-wasm submodule bump) requires the user to:

1. Build or obtain a second RISC-V kernel image distinct from the first.
2. Drop both under `crates/bootroom/spikes/spike-a/fixtures/Image-A` and `Image-B` (gitignored).
3. Follow the procedure in `crates/bootroom/spikes/spike-a/README.md`.

## Next Phase Readiness

- **Phase 1 final close:** This is Phase 1 plan 9 of 9 — last plan in the phase. ROADMAP.md should now mark Phase 1 complete (`/gsd-progress` will handle the bookkeeping).
- **Phase 2 Launch button (UI-08):** Has its authoritative input. Implementation pattern = fetch new kernel + `FS_unlink('/pack/Image')` + `FS_createDataFile('/pack', 'Image', bytes, true, true, true)` + `location.reload()`. The `/ws` work in Phase 2 is the orthogonal piece (serial input wiring) — Launch button is a thin re-invocation of the same injection path.
- **Phase 2 Reset button (UI-09):** Same pattern as Launch, re-injecting the currently-loaded `--kernel` rather than a new one.
- **Phase 2 in-place-reset spike (optional optimisation):** Captured in SPIKE-A-RESULT.md follow-ups. Entry point = run swap.js once against the production page, capture the reset-export enumeration, decide whether to invoke `Module._qemu_system_reset` (or equivalent) instead of `location.reload()`.

## Self-Check: PASSED

Verified files exist on disk:
- FOUND: `crates/bootroom/spikes/spike-a/README.md`
- FOUND: `crates/bootroom/spikes/spike-a/web/swap.html`
- FOUND: `crates/bootroom/spikes/spike-a/web/swap.js`
- FOUND: `crates/bootroom/spikes/spike-a/fixtures/.gitkeep`
- FOUND: `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md`

Verified commits exist in git log:
- FOUND: `9db5040` — `feat(01-09): spike-a scaffolding — README, swap.html, swap.js probe`
- FOUND: `9417c17` — `docs(01-09): SPIKE-A-RESULT.md — verdict green, chosen_path module-fs-write`

Verified verdict file frontmatter:
- `verdict: green` ✓
- `chosen_path: module-fs-write` ✓
- `qemu_wasm_sha: 0ef7b4e2814b231705d8371dd7997f5b72e70baf` ✓
- Five canonical `## ` sections present ✓

Verified `cargo build --workspace` still passes (0.08s, no Rust changes).

---
*Phase: 01-walking-skeleton*
*Completed: 2026-05-17*
