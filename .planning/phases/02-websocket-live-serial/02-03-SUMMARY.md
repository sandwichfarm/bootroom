---
phase: 02-websocket-live-serial
plan: 03
subsystem: cli
tags: [open, browser, auto-open, clap, axum-ws, base64, futures-util, tokio-tungstenite]

# Dependency graph
requires:
  - phase: 01-walking-skeleton
    provides: ServeArgs struct + server::run startup-line contract (D-04)
provides:
  - "--no-open CLI flag on `bootroom serve`"
  - "open::that_detached auto-open call gated on !args.no_open"
  - "UI-SPEC stderr fallback line on launcher failure (`Could not open browser automatically — open the URL above manually.`)"
  - "Phase 2 Cargo dep graph fully wired (axum ws feature + open + base64 + futures-util + tokio-tungstenite dev-dep) so 02-02 needs zero Cargo.toml edits"
  - "SERV-06 subprocess integration test (CARGO_BIN_EXE_bootroom + ChildGuard drop-guard)"
affects: [02-02-ws-handler, 02-04, 02-05, 02-06, phase-04-headless-run]

# Tech tracking
tech-stack:
  added: [open=5.3.5, base64=0.22.1, futures-util=0.3.32, tokio-tungstenite=0.29.0 (dev-only)]
  patterns: [
    "Subprocess integration test via CARGO_BIN_EXE_<binname> + ChildGuard RAII",
    "Auto-open gated behind explicit opt-out flag; never fatal to server",
    "stdout owns user-facing data lines; stderr owns diagnostics (UI-SPEC contract)"
  ]

key-files:
  created:
    - "crates/bootroom/tests/serve_no_open.rs"
  modified:
    - "Cargo.toml (workspace.dependencies)"
    - "crates/bootroom/Cargo.toml (axum ws feature + new deps)"
    - "crates/bootroom/src/cli.rs (no_open field)"
    - "crates/bootroom/src/server.rs (open::that_detached call site)"

key-decisions:
  - "Use open::that_detached (not open::that) — detached spawn cannot block parent on misconfigured launchers (T-02-05 mitigation, 02-RESEARCH.md anti-pattern guidance)"
  - "Failure is non-fatal: log warn + emit UI-SPEC stderr line + keep serving. Auto-open is a convenience, never a correctness requirement"
  - "URL constructed from listener.local_addr() (a SocketAddr) — no user-controlled string interpolation reaches the launcher (T-02-04 mitigation)"
  - "Wire ALL Phase 2 Cargo deps in this plan (not just `open`) so 02-02 is pure Rust source"
  - "Subprocess test, not in-process: --no-open must exercise the real clap binary path, not the test harness's direct call into build_router"
  - "Test ChildGuard mirrors WR-06 from 01-REVIEW.md: spawned resources abort on drop in BOTH success and failure paths"

patterns-established:
  - "Subprocess integration test pattern: env!(\"CARGO_BIN_EXE_bootroom\") + ChildGuard + mpsc reader threads with recv_timeout"
  - "stdout/stderr split discipline: data → stdout, diagnostics → stderr (so users piping stdout get a clean URL line)"

requirements-completed: [SERV-06]

# Metrics
duration: 4m 9s
completed: 2026-05-18
---

# Phase 2 Plan 3: --no-open + auto-open wiring Summary

**`bootroom serve` opens the harness URL in the user's default browser by default (via `open::that_detached`), with `--no-open` opt-out for CI / headless use; Phase 2 Cargo deps fully wired.**

## Performance

- **Duration:** 4m 9s
- **Started:** 2026-05-18T10:01:55Z
- **Completed:** 2026-05-18T10:06:04Z
- **Tasks:** 2
- **Files modified:** 5 (1 created, 4 edited)

## Accomplishments

- `--no-open` boolean flag on `ServeArgs`; default off means auto-open is the friendly default.
- `open::that_detached(url)` called after `listener.local_addr()` and before `axum::serve`, gated on `!args.no_open`. Launcher failures log a `tracing::warn`, emit the exact UI-SPEC stderr line, and the server keeps serving — never fatal.
- Phase 2 workspace dependencies landed in one shot: `open = "5"`, `base64 = "0.22"`, `futures-util = "0.3.32"`, dev-only `tokio-tungstenite = "0.29"`. `axum` carries the `ws` feature in the `bootroom` crate. Plan 02-02 needs zero Cargo.toml edits.
- Subprocess integration test exercises the real `bootroom` binary with `--port 0` (ephemeral port, no CI collision), confirms the canonical startup line lands within 5 s, and asserts the auto-open fallback line is absent when `--no-open` is set.

## Task Commits

Each task was committed atomically; task 2 followed RED → GREEN.

1. **Task 1: Wire workspace + bootroom Cargo deltas** — `1cd583f` (feat)
2. **Task 2 RED: failing subprocess test** — `8e9e4fe` (test)
3. **Task 2 GREEN: --no-open flag + open::that_detached call** — `c68672f` (feat)

Plan metadata commit will follow this SUMMARY.

## Files Created/Modified

- `Cargo.toml` — added `open`, `base64`, `futures-util`, `tokio-tungstenite` to `[workspace.dependencies]`
- `crates/bootroom/Cargo.toml` — enabled axum `ws` feature; added `open` / `base64` / `futures-util` direct deps; added `tokio-tungstenite` + `futures-util` dev-deps
- `crates/bootroom/src/cli.rs` — new `no_open: bool` field on `ServeArgs` (clap `#[arg(long)]`, default false)
- `crates/bootroom/src/server.rs` — auto-open block between the startup-line `println!` and `axum::serve`; matches Ok/Err, logs accordingly, emits the UI-SPEC stderr fallback on failure
- `crates/bootroom/tests/serve_no_open.rs` — new subprocess test file: 2 tests (canonical startup line within 5 s with `--no-open`; `--help` lists `--no-open`), `ChildGuard` RAII type, mpsc-based reader threads with `recv_timeout`

## Decisions Made

- **`that_detached`, not `that`:** the detached variant spawns the launcher process detached from `bootroom`, so a hung or misconfigured `xdg-open` / `open` / `start` cannot block the server's event loop. Matches 02-RESEARCH.md "Anti-Patterns to Avoid."
- **Failure non-fatal:** auto-open is a UX nicety, not a correctness gate. A user without `xdg-open` (minimal Docker image, headless server they forgot to add `--no-open` to) must still get a working harness URL. We log `warn` + emit the UI-SPEC line + carry on.
- **stderr for the diagnostic, stdout for the URL:** scripts piping `bootroom serve | grep http` for the URL must see exactly one clean line. The "Could not open browser automatically — open the URL above manually." fallback therefore goes to stderr.
- **Wire all Phase 2 deps now:** `02-CONTEXT.md` declares the dep set; landing them together means plan 02-02 (the WebSocket handler) is pure Rust source.
- **Subprocess test, not in-process:** the in-process `common::spawn` harness skips clap entirely; only a real binary launch can prove `--no-open` parses correctly. The cost (a real `bootroom` process) is mitigated by `--port 0` (no port collisions) and `ChildGuard` (no leaked processes).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy `needless_continue` lint in subprocess test**

- **Found during:** Task 2 GREEN (post-implementation `cargo clippy -p bootroom --tests -- -D warnings`)
- **Issue:** `Err(mpsc::RecvTimeoutError::Timeout) => continue,` is the last arm of a `match` inside a `while` loop — `continue` is redundant since the loop reaches its end naturally. Clippy pedantic rejects under `-D warnings`.
- **Fix:** Replaced the bare `continue` with an empty block body + comment explaining intent (the loop condition re-checks the deadline; nothing else to do on timeout). No behavioural change.
- **Files modified:** `crates/bootroom/tests/serve_no_open.rs`
- **Verification:** `cargo clippy -p bootroom --tests -- -D warnings` clean; `cargo test --workspace` still passes.
- **Committed in:** `c68672f` (folded into the GREEN commit since it was the same logical change set; documented here for transparency)

---

**Total deviations:** 1 auto-fixed (1 lint bug)
**Impact on plan:** Zero scope creep. Single one-line lint fix; behavioural test logic unchanged.

## Issues Encountered

None — both tasks executed exactly as written, with the one clippy lint above as the only post-implementation surprise.

## TDD Gate Compliance

Task 2 used `tdd="true"` and followed the RED → GREEN sequence:

- **RED:** `8e9e4fe test(02-03)` — subprocess test added, compiles, both tests fail because clap rejects `--no-open` as an unknown argument.
- **GREEN:** `c68672f feat(02-03)` — `--no-open` flag + `open::that_detached` call land; both tests pass; full workspace suite green; clippy clean.
- **REFACTOR:** not required; implementation matched the planned shape on first pass.

Task 1 was a dep-graph wiring step (`tdd="true"` but with no behaviour change to test). Verification was `cargo build -p bootroom --tests` (succeeded) + `cargo tree` confirming axum's `ws` feature flowed through and the four new deps resolved as expected.

## User Setup Required

None — auto-open is a runtime convenience, no env vars or external config needed.

## Next Phase Readiness

- `bootroom-core` has the WS protocol enums (from 02-01).
- `bootroom` crate has the `ws` feature, `futures-util`, `base64`, and the `tokio-tungstenite` dev-dep ready for plan **02-02** (the `/ws` axum handler + WebSocket client integration tests).
- Auto-open behaviour and `--no-open` opt-out are in place; no further CLI surface changes needed for the rest of Phase 2.
- SERV-06 automated half is satisfied; manual verification (real browser opens when `--no-open` is absent) is queued for 02-VALIDATION.md per the plan.

## Self-Check: PASSED

- File exists: `.planning/phases/02-websocket-live-serial/02-03-SUMMARY.md`
- File exists: `crates/bootroom/tests/serve_no_open.rs`
- Commit present: `1cd583f` (Task 1, Cargo deps)
- Commit present: `8e9e4fe` (Task 2 RED)
- Commit present: `c68672f` (Task 2 GREEN)
- Symbol present: `pub no_open: bool` in `crates/bootroom/src/cli.rs`
- Symbol present: `open::that_detached` in `crates/bootroom/src/server.rs`
- Feature wired: `axum = { workspace = true, features = ["ws"] }` in `crates/bootroom/Cargo.toml`

---
*Phase: 02-websocket-live-serial*
*Completed: 2026-05-18*
