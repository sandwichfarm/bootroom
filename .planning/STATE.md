# State: bootroom

**Project:** bootroom — web-based test harness for RISC-V kernels via qemu-wasm
**Mode:** mvp
**Initialized:** 2026-05-17

## Project Reference

**Core Value:** Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library. If everything else fails, that one path must stay friction-free.

**Current Focus:** Phase 1 — Walking Skeleton. Serve qemu-wasm with correct COOP/COEP, boot a NORN kernel in a real browser tab, run the two required Phase-1 spikes (runtime kernel substitution; headless Chromium + SAB + qemu-wasm).

## Current Position

- **Phase:** 1 — Walking Skeleton (not started)
- **Plan:** None — `/gsd-plan-phase 1` is the next step
- **Status:** Roadmap approved, awaiting phase planning
- **Progress:** `[░░░░░░░░░░] 0/6 phases`

## Performance Metrics

- Phases complete: 0/6
- v1 requirements complete: 0/59
- Validated requirements: 0
- Open spikes: 2 (Phase-1 Spike A, Spike B)

## Accumulated Context

### Decisions

Carried from `PROJECT.md` Key Decisions:

- Project + binary name = `bootroom` (kernel-agnostic)
- Repo `norn-web` → `bootroom` rename pending (Phase 1 action item)
- Language = Rust 1.85+ / edition 2024
- Action model = serial/stdin injection via xterm-pty `slave.write`
- Kernel discovery = watch path (default) + `--kernel` override
- Config format = TOML
- UI = vanilla JS + HTML, embedded via `include_dir!`
- CI mode = `bootroom run --scenario …` with exit codes
- Distribution = `cargo install` + `cargo-dist` prebuilt binaries
- License = MIT OR Apache-2.0

### Architecture (from research)

- One process, one WebSocket, two modes (`serve` and `run`) sharing identical embedded assets and `/ws` protocol.
- `bootroom-core` library (pure types + scenario engine, no I/O); `bootroom` binary (clap dispatch, axum app, watcher, headless driver).
- xterm-pty `slave` is *the* byte boundary for both action injection (`slave.write`) and serial capture (`slave.onReadable`).
- Scenarios run client-side (low latency); server is exit-code translator.

### Open Spikes (Phase 1)

- **Spike A:** Confirm runtime kernel substitution into qemu-wasm `Module.FS` (avoid re-running emscripten `file_packager.py` per launch). If intractable: launch-time pack rebuild fallback.
- **Spike B:** Confirm headless Chromium (`--headless=new`) + `SharedArrayBuffer` + COOP/COEP + qemu-wasm boots a fixture kernel end-to-end. If red: switch from `chromiumoxide` to Playwright subprocess (adds Node dep on CI runners).

### Todos

- Approve roadmap → run `/gsd-plan-phase 1` to decompose Phase 1 into plans.
- Vendor `xterm.js` and `xterm-pty` (qemu-wasm demo uses CDN — must be self-hosted).
- Define `make qemu-assets` target that runs the qemu-wasm docker build and caches output (don't drive from `build.rs`).
- Add `--assets-dir <path>` runtime override from day one (dev iteration without `cargo build`).

### Blockers

None. Spikes A and B are de-risking activities for Phase 1, not external blockers.

### Key Pitfalls to Watch (top 3)

1. **COOP/COEP plumbing** must apply to *every* subresource (HTML, JS, WASM, worker, data) — single missing header silently breaks SAB. Verify at boot.
2. **Headless Chromium + SAB + qemu-wasm** is the single biggest unknown. Spike B retires this in Phase 1.
3. **Serial-output assertions** flake without line-buffering, ANSI stripping, per-action buffer reset, and explicit timeouts. Bake the conventions into the scenario engine (Phase 4) before users author scenarios.

## Session Continuity

- **Last session:** 2026-05-17 — Roadmap created (6 phases, 59/59 coverage).
- **Next session:** Run `/gsd-plan-phase 1` to decompose Phase 1 into executable plans. Include both spikes as their own plans.
- **Context to reload:** `PROJECT.md`, `REQUIREMENTS.md`, `ROADMAP.md`, `research/SUMMARY.md`, `research/ARCHITECTURE.md`, `research/PITFALLS.md` (top 8 pitfalls).

---
*State initialized: 2026-05-17 via gsd-roadmapper*
