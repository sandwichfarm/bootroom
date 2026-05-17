---
phase: 1
name: Walking Skeleton
date: 2026-05-17
mode: discuss
---

# Phase 1 Discussion Log

## Gray Areas Presented

1. Repo rename + crate layout
2. qemu-wasm artifact pipeline
3. Spike sequencing and acceptance
4. Phase 1 UI scope + port defaults

User selected: **all four**.

## Area 1 — Repo rename + crate layout

**Options:**
- Rename now + workspace from day 1 (Recommended)
- Defer rename, single crate now
- Rename now, single crate now (split later)
- Defer rename, workspace from day 1

**Selected:** Rename now + workspace from day 1.

**Notes:** Aligns with research recommendation; Phase 4 will need `bootroom-core` for headless `run`. Cleanest separation upfront. Physical directory rename is cosmetic (qemu-wasm submodule is relative).

## Area 2 — qemu-wasm artifact pipeline

**Options:**
- `make qemu-assets` + check artifacts into git (Recommended)
- `make qemu-assets` + gitignored, build-required
- Pre-built tarball download via build.rs
- Build via build.rs every cargo invocation

**Selected:** `make qemu-assets` + check artifacts into git.

**Notes:** Accept ~10–30 MB git repo cost in exchange for clean-checkout `cargo build` working without docker, reproducible builds, and CI runners not needing docker. Vendored xterm.js/xterm-pty also committed (separate dir under `web/vendor/`).

## Area 3 — Spike sequencing and acceptance

**Options:**
- Sequence B → A; both inside Phase 1 (Recommended)
- Parallel A + B as independent plans
- Sequence A → B; both inside Phase 1
- Spike B only in Phase 1; defer Spike A to Phase 2

**Selected:** Sequence B → A; both inside Phase 1.

**Notes:** Spike B is the single biggest risk per research — run first so a red verdict reroutes to Playwright before more chromiumoxide work. Both spikes emit a `SPIKE-X-RESULT.md` with verdict and chosen path; downstream phases consume these files.

## Area 4a — Phase 1 UI scope

**Options:**
- Minimum spec: status pill + header + xterm placeholder + probe (Recommended)
- Bare bones: header + probe only, no xterm
- Maximum spec: also include manual Launch button

**Selected:** Minimum spec.

**Notes:** xterm.js mounts on xterm-pty `slave` so serial *output* renders; keyboard *input* is a no-op (Phase 2 wires it through `/ws`). Satisfies ROADMAP success criterion 4 ("terminal visible, even if not yet interactive").

## Area 4b — Default port + --no-open behavior

**Options:**
- Port 8765, --no-open default-ON in Phase 1 (Recommended)
- Port 8080, --no-open default-ON
- Ephemeral (port 0), --no-open default-ON
- Port 8765, --no-open default-OFF (auto-open in Phase 1)

**Selected:** Port 8765, --no-open default-ON.

**Notes:** Auto-open (SERV-06) lives in Phase 2 per ROADMAP — keep the phase boundary clean. Phase 1 just prints the URL. Port 8765 is uncommon (avoids collision with 3000/8080/8000) and mnemonic for bootroom.

## Deferred Ideas Captured

(See CONTEXT.md `<deferred>` section for full list — all items above are restated there.)

## Claude's Discretion (not asked)

- Exact directory tree under `crates/bootroom/` (web/, web/vendor/, assets/qemu/) — chosen by Claude based on stack conventions.
- COOP/COEP middleware mechanism (single `SetResponseHeaderLayer`) — derived from research.
- Endpoint set for Phase 1 (`/`, `/api/kernel/info`, `/kernel`, `/assets/*`) — derived from minimum-spec UI needs.
- Logging via `tracing` + `tracing-subscriber` with `RUST_LOG` filtering — already locked in CLAUDE.md stack table.
