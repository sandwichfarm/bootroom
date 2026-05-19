---
phase: 05-diagnostics-doctor
plan: 03
subsystem: cli
tags: [cli, clap, doctor, scaffolding]
dependency_graph:
  requires:
    - 04-* (CommonArgs, Cmd::Run, run_cmd::discover_chromium private fn)
  provides:
    - Cmd::Doctor variant
    - DoctorArgs { config, format } struct
    - OutputFormat { Human, Json } enum
    - bootroom::doctor_cmd::run async stub
    - pub(crate) visibility on run_cmd::discover_chromium
    - tower::ServiceExt (util feature) callable from non-test code
  affects:
    - 05-04 (fills in doctor_cmd::run body using all of the above)
    - 05-05 / 05-06 (consume the stable CLI surface for tests)
tech-stack:
  added: []
  patterns:
    - "clap::ValueEnum derive for stable, restricted --format vocabulary"
    - "Async stub command handler — signature stable across plan boundary"
key-files:
  created:
    - crates/bootroom/src/doctor_cmd.rs
  modified:
    - crates/bootroom/Cargo.toml
    - crates/bootroom/src/cli.rs
    - crates/bootroom/src/lib.rs
    - crates/bootroom/src/main.rs
    - crates/bootroom/src/run_cmd.rs
decisions:
  - "OutputFormat lives in cli.rs (not doctor_cmd.rs) because it is a CLI-grammar type that clap derives ValueEnum on — keeps doctor_cmd.rs free of clap dependencies."
  - "Doctor variant appended LAST (after Init) to preserve the Phase-1/Phase-4 Cmd variant order pinned by tests/cli_subcommands.rs."
  - "tower `util` feature added to `[dependencies]` (not removed from `[dev-dependencies]`) — cargo unifies feature sets, the duplicated declaration is harmless, and leaving the dev-dep line documents tests' needs explicitly."
  - "Stub `run` is `async fn` even though its body does not await — the upcoming 05-04 body awaits `ServiceExt::oneshot(...)`, so locking the async signature now avoids a churn commit later."
metrics:
  duration_minutes: 5
  completed: 2026-05-19
---

# Phase 5 Plan 03: Doctor CLI Scaffold Summary

Landed the full CLI surface for `bootroom doctor` (variant, args struct, output-format enum, dispatch arm, stub command) plus the two Phase-5 enabling-mutations identified during research (`tower` util feature promoted to normal dependency; `run_cmd::discover_chromium` visibility raised to `pub(crate)`) so Plan 05-04 can fill in the real check body without further plumbing work.

## What Changed

### Files modified

| File | Change |
|------|--------|
| `crates/bootroom/Cargo.toml` | Promoted `tower` with `features = ["util"]` from `[dev-dependencies]` only to `[dependencies]` (left the dev-dep line in place; cargo unifies feature sets). |
| `crates/bootroom/src/cli.rs` | Added `Cmd::Doctor(DoctorArgs)` variant (line 50, appended last), `pub struct DoctorArgs` (line 151), `pub enum OutputFormat { Human, Json }` (line 162) deriving `clap::ValueEnum`. Extended `use clap::{Args, Parser, Subcommand, ValueEnum};`. Added two unit tests (`doctor_subcommand_parses_with_format_flag`, `doctor_subcommand_defaults_to_human_and_no_config`). |
| `crates/bootroom/src/doctor_cmd.rs` | **Created.** Contains `pub async fn run(_args: DoctorArgs) -> ExitCode { ExitCode::SUCCESS }` stub. Body lands in Plan 05-04. |
| `crates/bootroom/src/lib.rs` | Added `pub mod doctor_cmd;` alongside the other command modules. Did NOT add `DoctorArgs` to the re-export line — main.rs reaches it via `crate::cli::DoctorArgs` through `Cmd::Doctor(args)`. |
| `crates/bootroom/src/main.rs` | Added dispatch arm `Cmd::Doctor(args) => Ok(bootroom::doctor_cmd::run(args).await)` AFTER `Cmd::Init`. |
| `crates/bootroom/src/run_cmd.rs` | Changed `fn discover_chromium(...)` to `pub(crate) fn discover_chromium(...)`. Body unchanged. Sibling helpers (`which_via_path_env`, `discover_chromium_with_candidates`) intentionally left private. |

### Insertion line numbers (cli.rs)

- `Cmd::Doctor(DoctorArgs)` — line 50 (last variant of the `Cmd` enum)
- `pub struct DoctorArgs` — line 151
- `pub enum OutputFormat` — line 162
- Plan 05-03 test block marker — line 552

## tower::ServiceExt resolves in non-test code

Verified via `cargo metadata`: the bootroom package now declares two `tower` dependencies — one `kind: None` (normal) and one `kind: 'dev'`, both with `features: ['util']`. The normal-kind entry is what allows non-test code in `doctor_cmd.rs` (Plan 05-04) to write `use tower::ServiceExt;` without compile errors. Confirmed by inspection:

```json
{'name': 'tower', 'kind': None, 'features': ['util']}
{'name': 'tower', 'kind': 'dev', 'features': ['util']}
```

Both `cargo build -p bootroom` and `cargo test -p bootroom --lib` succeed against the new graph.

## `bootroom --help` verbatim subcommand block

(For Plan 05-06's regex pin.)

```
Web-based test harness for RISC-V kernels via qemu-wasm.

Usage: bootroom <COMMAND>

Commands:
  serve   Start the local HTTP server and serve the qemu-wasm UI
  run     Run a scenario headlessly under Chromium and exit with a CI-style status code (RUN-01..10). Plan 04-07 fills in the driver body; Plan 04-03 lands the surface so 04-04..06 build cleanly
  check   Parse and validate bootroom.toml without starting the server
  init    Write a starter bootroom.toml to the current directory
  doctor  Run preflight checks (version, browser, headers, config)
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Subcommand order: `serve, run, check, init, doctor, help` — `doctor` is the fifth and final user subcommand (clap auto-appends `help`), so the Phase-1/Phase-4 cli_subcommands test pins for the first four positions remain stable.

## `bootroom doctor` exit-code matrix

| Invocation | Exit code | Notes |
|---|---|---|
| `bootroom doctor` | 0 | Stub returns `ExitCode::SUCCESS`. Body lands in 05-04. |
| `bootroom doctor --help` | 0 | clap renders help; mentions `--config <PATH>` and `--format <human\|json>`. |
| `bootroom doctor --format json` | 0 | Stub honors the flag without crashing. |
| `bootroom doctor --format invalid` | 2 | Clap usage error from the `ValueEnum` gate (T-05-03-01 mitigation verified). |
| `bootroom doctor --config /tmp/x.toml` | 0 | Path parsed and stored in `DoctorArgs::config`; no file access in the stub. |

## Test count pre/post

| Test surface | Pre | Post |
|---|---|---|
| `cargo test -p bootroom --lib` | 100 passed | **102 passed** (+2 new doctor tests) |
| `cargo test -p bootroom` (all targets) | clean | clean (all integration suites still pass, including `cli_subcommands.rs` 7/7) |

No regressions; no test deletions.

## Verification

- [x] `cargo build -p bootroom` succeeds.
- [x] `cargo test -p bootroom --lib cli::tests::doctor_` — 2 passed.
- [x] `cargo test -p bootroom --lib` — 102 passed, 0 failed.
- [x] `cargo test -p bootroom` (integration suites) — all green; `tests/cli_subcommands.rs` 7/7 unaffected by the variant append.
- [x] `bootroom --help` lists five user subcommands in order `serve, run, check, init, doctor`.
- [x] `bootroom doctor` exits 0; `bootroom doctor --help` exits 0; `bootroom doctor --format json` exits 0; `bootroom doctor --format invalid` exits 2 (clap usage error).
- [x] `run_cmd::discover_chromium` is `pub(crate)` and still called from `run_cmd::run_inner` (line 156) with no changes.
- [x] `tower::ServiceExt` resolves in non-test code (verified via `cargo metadata` graph; tested implicitly by the still-green build).

## Deviations from Plan

None — plan executed exactly as written. The Cargo.toml edit landed in line 37 (the original `tower.workspace = true`), and the dev-dependency line at 51 was left as-is per the plan's instruction to preserve it. Existing test file `tests/cli_subcommands.rs` did NOT need editing — pins are positional (first four variants) and the new `doctor` variant was appended last, so the existing assertions still hold.

## Commits

| Hash | Subject |
|---|---|
| `9f0909d` | chore(05-03): enable tower util feature; promote discover_chromium pub(crate) |
| `7376b2c` | feat(05-03): add Cmd::Doctor(DoctorArgs) + OutputFormat to CLI surface |
| `8b3de9d` | feat(05-03): wire doctor_cmd stub end-to-end |

Three task-scoped commits matching the plan's three `<task>` blocks 1-for-1.

## Known Stubs

| Stub | File | Line | Reason |
|---|---|---|---|
| `doctor_cmd::run` returns `ExitCode::SUCCESS` unconditionally | `crates/bootroom/src/doctor_cmd.rs` | 17 | Intentional — Plan 05-04 fills in the real body (version/browser/header/config checks). The async signature, args type, and exit-code type are locked in this plan so 05-04 only changes the function body. The success criterion explicitly allows the stub. |

## Self-Check: PASSED

- [x] `crates/bootroom/src/doctor_cmd.rs` — FOUND.
- [x] `crates/bootroom/Cargo.toml` — modified, `features = ["util"]` present in `[dependencies]` block.
- [x] `crates/bootroom/src/cli.rs` — three new declarations present at lines 50, 151, 162.
- [x] `crates/bootroom/src/lib.rs` — `pub mod doctor_cmd;` line present.
- [x] `crates/bootroom/src/main.rs` — `Cmd::Doctor(args)` dispatch arm present after `Cmd::Init`.
- [x] `crates/bootroom/src/run_cmd.rs` — `pub(crate) fn discover_chromium` present at line 396.
- [x] Commit `9f0909d` — FOUND in git log.
- [x] Commit `7376b2c` — FOUND in git log.
- [x] Commit `8b3de9d` — FOUND in git log (HEAD).
