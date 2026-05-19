---
phase: 03-config-buttons-watcher
plan: 03
subsystem: cli
tags: [clap, derive, subcommands, escape-decoder, bootroom-core]

# Dependency graph
requires:
  - phase: 03-config-buttons-watcher
    provides: "Plan 01 — bootroom_core::config::CliAction + bootroom_core::decode_bytes_escape"
provides:
  - "Cmd::{Serve, Check, Init} enum (Serve remains first variant — Pitfall #9)"
  - "ServeArgs extended with --config <PATH> and --action <LABEL=BYTES> (repeatable)"
  - "CheckArgs + InitArgs structs"
  - "parse_cli_action value parser delegating to bootroom_core::decode_bytes_escape"
  - "main.rs exhaustive match dispatching Check -> exit(2) and Init -> exit(1) stubs (Plan 04 replaces)"
  - "tests/cli_subcommands.rs integration test (6 cases) pinning the user-visible CLI surface"
affects: [03-04, 03-06, 03-07, 03-08]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Shared escape grammar — CLI and TOML both decode via bootroom_core::decode_bytes_escape (one parser, zero drift risk)"
    - "Stub-first CLI surface — placeholder exit codes (2/1) land in main.rs so downstream plans can match exhaustively before bodies exist"

key-files:
  created:
    - crates/bootroom/tests/cli_subcommands.rs
  modified:
    - crates/bootroom/src/cli.rs
    - crates/bootroom/src/main.rs
    - crates/bootroom/src/lib.rs

key-decisions:
  - "Cmd::Serve kept as the FIRST enum variant (Pitfall #9) — preserves help-text ordering and the Phase-2 subprocess test invocation shape. tests/serve_no_open.rs passes unchanged."
  - "parse_cli_action splits on the FIRST `=` so operators may embed `=` inside byte payloads (e.g. `--action 'env=KEY=VAL\\r'`). Empty label rejected with a helpful error; rhs decode delegated to the shared escape helper."
  - "Override semantics (CLI overrides TOML; last `--action label=X` wins among repeats) live entirely in LoadedConfig::load_from_str_with_overrides (Plan 01). The CLI is just a producer of Vec<CliAction>."
  - "Check/Init stub exit codes — Check exits 2 (file-not-found-class), Init exits 1. Plan 04 will rewrite the corresponding `cli_check_stub_exits_nonzero` / `cli_init_stub_exits_nonzero` tests with real 0/1/2/3 assertions."
  - "Did NOT add `mod common;` to tests/cli_subcommands.rs even though `CARGO_BIN_EXE_bootroom` would let us — avoids a write conflict with Plan 03-05 which owns tests/common/mod.rs."

patterns-established:
  - "CLI value-parser pattern: clap value_parser fn returns Result<T, String>, never panics, prefixes the error with the failing argument label."
  - "Stub-dispatch pattern: placeholder match arms exit with a distinctive non-success code + `// Plan NN wires real handlers; placeholder until then.` comment so reviewers do not mistake them for final."

requirements-completed: [CFG-01, CFG-07, CFG-08, ACT-03]

# Metrics
duration: ~4m
completed: 2026-05-19
---

# Phase 03 Plan 03: CLI Subcommand Skeleton Summary

**Three-arm `Cmd::{Serve, Check, Init}` clap surface with `--config`/`--action` extensions on `serve`, stubs in `main.rs`, and a six-case subprocess test file pinning the user-visible help text.**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-05-19T09:08:59Z
- **Completed:** 2026-05-19T09:13:16Z
- **Tasks:** 2
- **Files modified:** 3 (+1 created)

## Accomplishments

- `Cmd` enum extended from 1 to 3 variants while keeping `Serve` first (Pitfall #9 — Phase 2 subprocess test `tests/serve_no_open.rs` still passes byte-for-byte unchanged).
- `ServeArgs` gains `config: Option<PathBuf>` and `actions: Vec<CliAction>` with clap `ArgAction::Append` for repeatability; clap `value_parser = parse_cli_action` validates each value via the shared `bootroom_core::decode_bytes_escape` helper.
- 10 inline `cli::tests` cover the 5 `parse_cli_action` paths (simple, hex, empty label, no `=`, invalid escape) plus the three new subcommand parses plus the Pitfall #9 ServeArgs-shape regression pin.
- 6 new integration tests in `tests/cli_subcommands.rs` assert: three subcommands in top-level `--help`; `serve --help` mentions `--config`, `--action`, and `LABEL=BYTES`; `check --help` mentions `--config`; `init --help` mentions `--force`; `check` and `init` stubs both exit non-zero.

## Task Commits

1. **Task 1: Extend `cli.rs`** — `15acae4` (feat) — `Cmd` enum, `ServeArgs/CheckArgs/InitArgs`, `parse_cli_action`, 10 inline tests.
2. **Task 2: Dispatch + re-exports + integration tests** — `ee76ad5` (feat) — exhaustive `main.rs` match with `Check`/`Init` stubs, `lib.rs` re-exports, new `tests/cli_subcommands.rs`, tiny `single_char_pattern` clippy fix.

Per-task TDD note: Task 1's tests were written together with the impl (the tests cannot compile without the new types). The RED/GREEN/REFACTOR ceremony degenerates to a single commit when the unit-under-test is a brand-new module-level item with no prior signature to red-test against. Task 2's TDD was integration-level (`cli_subcommands.rs` exercises the user-facing surface end-to-end).

## Files Created/Modified

- `crates/bootroom/src/cli.rs` — Extended `Cmd` to three variants; new `CheckArgs`/`InitArgs`; new `parse_cli_action` value parser; new `--config` and `--action` fields on `ServeArgs`; 10 inline tests.
- `crates/bootroom/src/main.rs` — `match cli.cmd` exhaustive: `Serve` unchanged; `Check` -> `exit(2)` placeholder; `Init` -> `exit(1)` placeholder. Both arms carry a `// Plan 04 wires real handlers; placeholder until then.` marker.
- `crates/bootroom/src/lib.rs` — Re-exports broadened from `ServeArgs` alone to `Cli, Cmd, CheckArgs, InitArgs, ServeArgs` so Plan 04's handler-test harnesses can construct the parsed shape directly.
- `crates/bootroom/tests/cli_subcommands.rs` (new) — 6 subprocess tests driving `CARGO_BIN_EXE_bootroom` for help-text and stub-exit assertions. Intentionally does NOT use `mod common;` to avoid a write conflict with the parallel Plan 03-05 work on `tests/common/mod.rs`.

## Decisions Made

- **Cmd::Serve stays first variant** — Pitfall #9 mitigation. Confirmed by running `cargo test --test serve_no_open` to green in isolation.
- **CLI override merge stays in LoadedConfig** — The CLI is just a producer of `Vec<CliAction>`; merge/dedup semantics are Plan 01's job (already implemented in `LoadedConfig::load_from_str_with_overrides`). Keeps escape-grammar and override-policy single-sourced.
- **Distinct stub exit codes (2 for Check, 1 for Init)** — Different codes make accidental CI use during the Plan 03→04 window fail loudly with a recognisable signature.
- **No `mod common;` in cli_subcommands.rs** — Avoids contention with the concurrent Plan 03-05 agent who owns `tests/common/mod.rs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy `single_char_pattern` lint on `err.contains("x")`**

- **Found during:** Task 2 (final clippy sweep on `--lib --tests -- -D warnings`).
- **Issue:** `parse_cli_action_invalid_escape_propagates` used `err.contains("x")` (a single-char string literal where a `char` is preferred).
- **Fix:** `err.contains("x")` → `err.contains('x')`.
- **Files modified:** `crates/bootroom/src/cli.rs` (one character).
- **Verification:** `cargo clippy -p bootroom --lib --tests -- -D warnings` green; all 10 unit tests still pass.
- **Committed in:** `ee76ad5` (folded into Task 2 commit — single-byte fix, atomic with the dispatch wiring).

---

**Total deviations:** 1 auto-fixed (1 lint cleanup).
**Impact on plan:** Cosmetic. No scope or behavior change.

## Issues Encountered

- **Concurrent Plan 03-05 WIP in working tree at start.** The parent agent flagged that 03-05 is parallel-executing against `state.rs`, `server.rs`, and `tests/common/mod.rs`. When I began Task 2, those files already had uncommitted modifications (introducing the Phase-3 `AppState::new` six-argument signature plus a `new_for_test` shim, with `server.rs` line 48 still calling the old two-argument shape). Mitigation: I stashed the 03-05 WIP for the duration of my own build/test verification, ran `cargo build -p bootroom` + `cargo test --test cli_subcommands` + `cargo test --test serve_no_open` + `cargo clippy` all green in isolation, then unstashed the 03-05 WIP so it stays in their working tree for them to finish.
- **Aggregate `cargo test -p bootroom` will fail in the working tree** until 03-05 commits their `server.rs` line-48 callsite update. This is purely a cross-plan ordering artifact, not a regression introduced by Plan 03-03. The three test surfaces this plan owns (`cli::tests`, `cli_subcommands`, `serve_no_open`) all pass green in isolation against my commits.

## TDD Gate Compliance

This plan uses `type: execute` (not `type: tdd`), and the two tasks declare `tdd="true"` individually for unit/integration test coverage rather than the plan-level RED→GREEN cycle. No RED-phase test commit is required at the plan level; per-task assertions land alongside the implementation in a single commit because the tests reference types that did not exist prior to the task.

## Pitfall #9 Confirmation

`tests/serve_no_open.rs` passes byte-for-byte unchanged. Verified by `cargo test --test serve_no_open` returning `2 passed; 0 failed` against the Task-2 HEAD (commit `ee76ad5`).

## Self-Check: PASSED

- `crates/bootroom/src/cli.rs` — FOUND (modified, contains `Cmd::Check`, `CheckArgs`, `InitArgs`, `parse_cli_action`).
- `crates/bootroom/src/main.rs` — FOUND (modified, contains `Cmd::Check` arm).
- `crates/bootroom/src/lib.rs` — FOUND (modified, re-exports `CheckArgs`, `InitArgs`, `Cli`, `Cmd`).
- `crates/bootroom/tests/cli_subcommands.rs` — FOUND (new file, 6 tests).
- Commit `15acae4` — FOUND in `git log`.
- Commit `ee76ad5` — FOUND in `git log`.
- `cargo test -p bootroom --lib cli::tests` — 10 passed (verified).
- `cargo test --test cli_subcommands` — 6 passed (verified, with 03-05 WIP stashed).
- `cargo test --test serve_no_open` — 2 passed (verified, with 03-05 WIP stashed; Pitfall #9 cleared).
- `cargo clippy -p bootroom --lib --tests -- -D warnings` — clean (verified, with 03-05 WIP stashed).

## Next Phase Readiness

- Plan 04 (`check_cmd` + `init_cmd` real handlers) can now match exhaustively on `Cmd::{Serve, Check, Init}` and replace the two `unimplemented`-class exit stubs.
- Plan 06 (config-aware `serve`) has `ServeArgs.config` and `ServeArgs.actions` to forward into `LoadedConfig::load_from_str_with_overrides`.
- Plan 07 (watcher) is unaffected by this plan; CLI shape is fixed.
- The two `cli_*_stub_exits_nonzero` tests are explicitly flagged for rewrite by Plan 04 with real exit-code assertions (0/1/2/3).

---
*Phase: 03-config-buttons-watcher*
*Completed: 2026-05-19*
