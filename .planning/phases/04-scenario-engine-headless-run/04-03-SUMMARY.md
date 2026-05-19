---
phase: 04-scenario-engine-headless-run
plan: 03
subsystem: cli
tags: [clap, cli, flatten, subcommand, run]

# Dependency graph
requires:
  - phase: 04-scenario-engine-headless-run
    provides: ScenarioStart/Abort/Result WsMessage variants (04-01)
provides:
  - "CommonArgs struct (--kernel, --config, --verbose) flattened into ServeArgs + RunArgs"
  - "Cmd::Run(RunArgs) enum variant with --scenario (req) + --log-file (opt)"
  - "bootroom::run_cmd module with async stub returning ExitCode::from(3) for 04-04..07 to build against"
affects: [04-04, 04-05, 04-06, 04-07]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "clap #[command(flatten)] for shared subcommand args"
    - "stub-then-replace pattern for blocking dependency layers"

key-files:
  created:
    - crates/bootroom/src/run_cmd.rs
  modified:
    - crates/bootroom/src/cli.rs
    - crates/bootroom/src/main.rs
    - crates/bootroom/src/lib.rs
    - crates/bootroom/src/server.rs

key-decisions:
  - "Stub run_cmd::run returns ExitCode::from(3) with stderr diagnostic pointing at 04-07 — keeps the workspace buildable while 04-07 lands the driver."
  - "Async signature pinned on the stub (#[allow(clippy::unused_async)]) so 04-07 doesn't need to change the call-site contract."
  - "Cmd variant order: Serve, Run, Check, Init — preserves Pitfall #9 help-text ordering."

patterns-established:
  - "CommonArgs flatten: subcommands sharing flags carry them through `#[command(flatten)] pub common: CommonArgs` rather than duplicating arg declarations."
  - "Server::run reads `args.common.kernel` / `args.common.config` — same path will apply to future shared-flag subcommands."

requirements-completed: [CLI-02, RUN-01, RUN-08, RUN-09]

# Metrics
duration: 8min
completed: 2026-05-19
---

# Phase 4 Plan 3: scenario-engine-headless-run / CommonArgs + Cmd::Run surface Summary

**Extracted `CommonArgs` (--kernel/--config/--verbose) shared via clap flatten across `serve` and a new `run` subcommand; landed `Cmd::Run(RunArgs)` with stub dispatch so 04-04..06 can build against the surface while 04-07 writes the chromiumoxide driver.**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-05-19T14:04:50Z
- **Completed:** 2026-05-19T14:12:29Z
- **Tasks:** 3 (Task 1 followed TDD RED→GREEN; Task 2 wired dispatch; Task 3 was a grep-gates verification + clippy cleanup)
- **Files modified:** 4 + 1 created

## Accomplishments

- `CommonArgs { kernel, config, verbose }` ships as the shared flag carrier; `ServeArgs` + `RunArgs` flatten it.
- `Cmd` enum gains `Run(RunArgs)` in the second slot (after `Serve`) — `--scenario <NAME>` required, `--log-file <PATH>` optional.
- `bootroom::run_cmd::run(args)` lives as a stub that exits 3 with a stderr pointer to 04-07; `main.rs` dispatches `Cmd::Run` to it.
- Phase 2/3 invocation grammar pinned by `cli_serve_args_phase2_compat` + new `cli_serve_args_phase3_compat_via_flatten` — the only thing that changed is field-access paths inside the test bodies (`args.kernel` → `args.common.kernel`).
- `cargo clippy -p bootroom --lib --tests --no-deps -- -D warnings` is now clean.

## Task Commits

1. **Task 1 RED — failing tests** — `42188b6` (test)
2. **Task 1 GREEN — extract CommonArgs + add Cmd::Run(RunArgs)** — `df31ee2` (feat)
3. **Task 2 — wire Cmd::Run dispatch + stub bootroom::run_cmd** — `f886a99` (feat)
4. **Task 3 — clippy clean** — `2a28273` (chore)

## Files Created/Modified

- `crates/bootroom/src/cli.rs` — added `CommonArgs`, `RunArgs`, `Cmd::Run`; refactored `ServeArgs` to flatten common; appended 8 new tests, updated 1 (Phase-2 compat) for the new field paths.
- `crates/bootroom/src/server.rs` — field-access migration: `args.kernel` → `args.common.kernel`, `args.config` → `args.common.config`. Three in-file `ServeArgs { ... }` test literals updated to construct `common: CommonArgs { ... }`.
- `crates/bootroom/src/main.rs` — new `Cmd::Run(args) => Ok(bootroom::run_cmd::run(args).await)` arm.
- `crates/bootroom/src/lib.rs` — `pub mod run_cmd;` + re-export `CommonArgs, RunArgs`.
- `crates/bootroom/src/run_cmd.rs` (NEW) — async stub returning `ExitCode::from(3)` with a stderr diagnostic.

## Decisions Made

- **Stub is async with `#[allow(clippy::unused_async)]`.** 04-07 awaits chromiumoxide; pinning the signature now means 04-04..06 don't need to re-thread the call-site when the real driver lands.
- **Exit code `3` for the stub.** Per RUN-09's exit-code translation table, `3` is "driver startup error" — the stub is genuinely in startup before any scenario logic runs. 04-10 integration tests will distinguish the stub's `3` from a real-driver `3` by stderr content.
- **`-v` short flag on `CommonArgs`.** Plan called it out for `run`; making it generic on `CommonArgs` means `serve -v` parses too. Today `serve` ignores `verbose`; the cost is zero and the help-text consistency is worth it.
- **`--action` stays on `ServeArgs`.** The plan's interface section keeps `--action` outside the shared common-args; a new test `cli_parses_run_with_repeated_actions_unsupported` pins that `bootroom run --action ...` errors.

## Field-name migration audit (per plan output section)

The only call sites that read `args.kernel` / `args.config` were:

- `crates/bootroom/src/server.rs::run` — 4 reads of `args.kernel`, 1 read of `args.config`, all migrated to `.common.*`.
- `crates/bootroom/src/server.rs::tests` — 3 `ServeArgs { ... }` literals (config-invalid, missing-assets-dir, missing-kernel) — all updated to wrap kernel/config in `common: CommonArgs { ... }`.
- `crates/bootroom/src/cli.rs::tests::cli_serve_args_phase2_compat` — updated read-side `args.kernel`/`args.config` → `args.common.kernel`/`args.common.config`. **Argv inputs unchanged** (Pitfall #9 regression pin).

No tests under `crates/bootroom/tests/` constructed `ServeArgs` directly (all use `Command::new(CARGO_BIN_EXE_bootroom)`), so the integration test surface needed zero updates.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `cli_help_lists_shared_flags_on_run` was checking the wrong help string**

- **Found during:** Task 1 GREEN verification.
- **Issue:** The plan said to assert against `Cli::command().render_long_help().to_string()`. clap's top-level `render_long_help` does NOT recurse into subcommand args — only the subcommand summary lines render. The assertion failed for `--kernel`/`--config`/`--verbose`/`--scenario`/`--log-file`.
- **Fix:** Render the `run` subcommand's long help via `Cli::command().find_subcommand_mut("run").render_long_help()`. The intent of the test — "the new shared flatten + run-only flags surface in `bootroom run --help`" — is preserved.
- **Files modified:** crates/bootroom/src/cli.rs (test only).
- **Verification:** Test now passes; all 5 flag names present in the rendered help.
- **Committed in:** `df31ee2`.

**2. [Rule 2 - Missing Critical] `--action` on `run` must error, not silently parse**

- **Found during:** Task 1 RED tests.
- **Issue:** The plan's `<behavior>` block did not pin that `bootroom run --action ... --scenario ...` should error. Without a test, a future contributor moving `--action` into `CommonArgs` would silently widen `run`'s surface — `run` doesn't consume actions, so accepting them would be a foot-gun.
- **Fix:** Added `cli_parses_run_with_repeated_actions_unsupported` asserting `--action` on `run` returns `Err`.
- **Files modified:** crates/bootroom/src/cli.rs (test only).
- **Verification:** Test passes with the design `--action` stays on `ServeArgs`.
- **Committed in:** `42188b6` (RED) + `df31ee2` (GREEN).

**3. [Rule 2 - Missing Critical] Clippy `-D warnings` was failing**

- **Found during:** Task 3 verification.
- **Issue:** New doc comment in `--log-file` listed bare snake_case event names (`scenario_start`, `action_send`, etc.) which clippy::doc-markdown flagged. The new `run_cmd::run` stub also tripped `clippy::unused_async` (deliberately async for 04-07).
- **Fix:** Wrapped event names in backticks; added `#[allow(clippy::unused_async)]` with a comment pointing at 04-07 to make the suppression intentional rather than incidental.
- **Files modified:** crates/bootroom/src/cli.rs, crates/bootroom/src/run_cmd.rs.
- **Verification:** `cargo clippy -p bootroom --lib --tests --no-deps -- -D warnings` exits clean.
- **Committed in:** `2a28273`.

---

**Total deviations:** 3 auto-fixed (1 bug in plan, 2 missing critical surface).
**Impact on plan:** No scope creep; all three deviations harden the surface the plan describes. Argv parsing grammar unchanged; only field-access paths and clippy-clean polish.

## Issues Encountered

- The worktree was originally behind `master` (didn't have the phase-04 plan files yet). Merged `master` into `worktree-agent-af21ec233b8d7fa15` at the top of execution to pick up the plan tree; no conflicts, fast-forward-style merge with new files only.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- 04-04 / 04-05 / 04-06 / 04-07 can now `use bootroom::{RunArgs, CommonArgs, run_cmd}` and build against the surface.
- 04-07 needs only to **replace the body** of `crates/bootroom/src/run_cmd.rs::run`; the signature `pub async fn run(args: RunArgs) -> ExitCode` is fixed.
- Integration tests in 04-10 will need to differentiate the stub's `exit 3` from a real-driver `exit 3` — recommend asserting on stderr substring `"Plan 04-03 stub"` until 04-07 lands.

## Self-Check: PASSED

- `crates/bootroom/src/run_cmd.rs` exists: FOUND.
- Commit `42188b6` (test RED): FOUND.
- Commit `df31ee2` (feat: CommonArgs + Cmd::Run): FOUND.
- Commit `f886a99` (feat: dispatch + stub module): FOUND.
- Commit `2a28273` (chore: clippy clean): FOUND.
- `cargo build --workspace`: OK.
- `cargo test -p bootroom --lib cli::tests`: 19 passed / 0 failed.
- `cargo test -p bootroom --test cli_subcommands`: 4 passed / 0 failed.
- `cargo test -p bootroom` (full): all `test result: ok` lines, zero failures.
- `cargo clippy -p bootroom --lib --tests --no-deps -- -D warnings`: clean.
- All 7 grep gates: OK.

## TDD Gate Compliance

Plan was `type: execute` (not plan-level `tdd`), but Task 1 declared `tdd="true"`:

- RED gate: `42188b6 test(04-03): add failing tests for CommonArgs flatten + Cmd::Run` — verified compile failures (E0599/E0609) before GREEN.
- GREEN gate: `df31ee2 feat(04-03): extract CommonArgs + add Cmd::Run(RunArgs)` — 19/19 cli tests pass post-GREEN.
- REFACTOR gate: `2a28273 chore(04-03): clippy clean — backtick event names + pin run_cmd async` — cosmetic only, tests stayed green.

---
*Phase: 04-scenario-engine-headless-run*
*Completed: 2026-05-19*
