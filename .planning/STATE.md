---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: Completed 01-05-PLAN.md (API + asset handlers)
last_updated: "2026-05-17T14:15:04.955Z"
progress:
  total_phases: 6
  completed_phases: 0
  total_plans: 9
  completed_plans: 5
  percent: 56
---

# State: bootroom

**Project:** bootroom — web-based test harness for RISC-V kernels via qemu-wasm
**Mode:** mvp
**Initialized:** 2026-05-17

## Project Reference

**Core Value:** Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library. If everything else fails, that one path must stay friction-free.

**Current Focus:** Phase 1 — Walking Skeleton

## Current Position

Phase: 1 (Walking Skeleton) — EXECUTING
Plan: 5 of 9 complete (next: 01-06 author web/index.html + app.js + style.css)

- **Phase:** 1 — Walking Skeleton
- **Plan:** 01-05 complete — four real route handlers wired into build_router; GET /api/kernel/info, /kernel, and /assets/{*path} return live responses; V12 path-traversal protection on --assets-dir; .wasm served with application/wasm
- **Status:** Executing Phase 1
- **Progress:** [██████░░░░] 56%

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
- [Phase ?]: 01-02: Skipped docker build due to host disk constraint; added BOOTROOM_SKIP_QEMU_ASSET_CHECK escape hatch in build.rs so dev work on unrelated Phase 1 plans is unblocked.
- [Phase ?]: 01-03: Vendored xterm@5.3.0 (unscoped) and xterm-pty@0.12.0 as UMD bundles; pinned via SHA-256 in vendor/VERSIONS.md with MIT licenses captured verbatim in vendor/LICENSES.md
- [Phase 1]: 01-04: axum 0.8 + tower-http 0.6 + clap derive; default bind 127.0.0.1:8765; COOP/COEP via SetResponseHeaderLayer::overriding on every response (verified on 404 + 501 paths); --kernel existence validated at startup (V5); non-loopback --host emits tracing::warn (V4 partial); library exposes build_router/AppState/ServeArgs for plan 01-07 integration tests
- [Phase 1]: 01-05: Streaming SHA-256 (constant memory) for /api/kernel/info; tokio_util ReaderStream + Body::from_stream for /kernel; mime_guess::from_path with octet-stream fallback resolves .wasm -> application/wasm; V12 path-traversal protection layered (reject `..` segments + canonicalize-and-confirm-descendant); tokio gains "io-util" workspace feature for AsyncReadExt::read

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

Spikes A and B are de-risking activities for Phase 1, not external blockers.

- 01-02: docker build for qemu-wasm artifacts not run; host disk at 98% (12G free). User must free 10G+ then run 'make qemu-assets' to populate crates/bootroom/assets/qemu/.

### Key Pitfalls to Watch (top 3)

1. **COOP/COEP plumbing** must apply to *every* subresource (HTML, JS, WASM, worker, data) — single missing header silently breaks SAB. Verify at boot.
2. **Headless Chromium + SAB + qemu-wasm** is the single biggest unknown. Spike B retires this in Phase 1.
3. **Serial-output assertions** flake without line-buffering, ANSI stripping, per-action buffer reset, and explicit timeouts. Bake the conventions into the scenario engine (Phase 4) before users author scenarios.

## Session Continuity

- **Last session:** 2026-05-17T14:15:04.947Z
- **Stopped at:** Completed 01-05-PLAN.md (API + asset handlers)
- **Next session:** Execute 01-06 (author crates/bootroom/web/index.html + app.js + style.css so GET / starts returning 200)
- **Context to reload:** `PROJECT.md`, `REQUIREMENTS.md`, `ROADMAP.md`, `research/SUMMARY.md`, `research/ARCHITECTURE.md`, `research/PITFALLS.md` (top 8 pitfalls), `.planning/phases/01-walking-skeleton/01-05-SUMMARY.md`.

---
*State initialized: 2026-05-17 via gsd-roadmapper*
