---
phase: 05-diagnostics-doctor
plan: 06
subsystem: cli
tags: [clap, doc-strings, regression-test, cli-help]

# Dependency graph
requires:
  - phase: 05-diagnostics-doctor/03
    provides: Cmd::Doctor(DoctorArgs) variant added to the CLI surface
  - phase: 04-scenario-engine-headless-run
    provides: Cmd::Run(RunArgs) variant and CommonArgs flatten
provides:
  - Polished, audited /// doc-strings on every Cmd variant in cli.rs
  - Regression test pinning the exact five-subcommand surface (CLI-01)
affects: [docs-onboarding, ci-cli-regression, future-subcommand-additions]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Inverse pin alongside forward pin in the same regression test: catches deletion, rename, AND surprise addition with one assertion suite."
    - "Parse clap's `Commands:` block by splitting first whitespace-delimited token off each indented row, rather than position-anchoring against clap's exact indentation. Survives minor-version clap formatter retuning."

key-files:
  created: []
  modified:
    - crates/bootroom/src/cli.rs
    - crates/bootroom/tests/cli_subcommands.rs

key-decisions:
  - "Cmd::Run first-line doc: drop plan-internal jargon ('Plan 04-07 fills in the driver body...'), keep the exit-code legend in long_about. First line is the elevator pitch on `bootroom --help`; status/internal-roadmap detail belongs in long_about."
  - "Did not rewrite Cmd::Serve's long_about even though it contains 'Pitfall #9 mitigation' jargon — the plan audit is on the FIRST LINE only (constraint 1 + 3). long_about jargon is a Phase-6 cleanup if it ever matters."
  - "Did not add a separate RED commit for Task 2 even though tdd=\"true\". The test is a regression pin for an already-shipped surface (post-05-03 + 04-03); there is no production-side change to drive. RED→GREEN→REFACTOR collapses to a single GREEN commit when the implementation already exists."

patterns-established:
  - "Pattern: Five-subcommand inverse pin. Future plans that add a sixth user-facing subcommand must update the `expected` array in `top_level_help_lists_exactly_five_subcommands` AND update CLI-01's contract — the test will fail-loud otherwise, which is the desired forcing function."

requirements-completed: [CLI-01]

# Metrics
duration: ~25min
completed: 2026-05-19
---

# Phase 05 Plan 06: CLI doc-string audit + --help regression test Summary

**Cmd::Run first-line doc rewritten to drop plan-internal jargon; regression test pins the exact five-subcommand surface (serve, run, check, init, doctor) so deletions, renames, and surprise additions all fail CI.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2 (both `type="auto"`; Task 2 also `tdd="true"`)
- **Files modified:** 2
- **New tests:** 1 (cli_subcommands grew from 7 → 8)
- **Lib unit tests:** 121 passing, unchanged from pre-plan baseline.

## Accomplishments

- **Cmd::Run doc cleanup.** First-line was 142 chars and leaked plan-internal references ("Plan 04-07 fills in the driver body; Plan 04-03 lands the surface so 04-04..06 build cleanly"). Rewrote to a 76-char declarative sentence: "Run a scenario headlessly under Chromium and exit 0/1 on serial assertions." Long_about now carries the 0/1/2/3 exit-code legend, which is the operationally useful piece for `bootroom run --help`.
- **DoctorArgs::config first-line cleanup.** Was 95 chars on a single line ("Path to bootroom.toml; default = ./bootroom.toml. Missing file is informational, not a failure."). Split into a ≤80-char first line plus a long-help paragraph describing the not-a-failure semantics.
- **Regression test landed.** `top_level_help_lists_exactly_five_subcommands` parses the `Commands:` block out of `bootroom --help`, forward-pins all five names + their documented order, and inverse-pins that no surprise sixth subcommand appears.
- **No other doc-strings touched.** Cmd::Serve / Cmd::Check / Cmd::Init / Cmd::Doctor all already met constraints 1 + 3 of the audit (≤80 char first line, declarative, no jargon).

## Final Cmd-Variant First-Line Doc-Strings (per plan output spec)

| Variant | First-line text | Visible chars |
|---|---|---|
| `Cmd::Serve` | `Start the local HTTP server and serve the qemu-wasm UI.` | 55 |
| `Cmd::Run` | `Run a scenario headlessly under Chromium and exit 0/1 on serial assertions.` | 76 |
| `Cmd::Check` | `Parse and validate bootroom.toml without starting the server.` | 62 |
| `Cmd::Init` | `Write a starter bootroom.toml to the current directory.` | 55 |
| `Cmd::Doctor` | `Run preflight checks (version, browser, headers, config).` | 57 |

All ≤80 chars. All declarative. No internal jargon.

## *Args Fields That Gained a NEW Doc-String

**0.** All existing `pub` fields with `#[arg(...)]` already had `///` doc-strings before this plan started (audit constraint #4 was already satisfied across `CommonArgs`, `ServeArgs`, `RunArgs`, `CheckArgs`, `InitArgs`, `DoctorArgs`). The only field-level edit was a REWRITING of `DoctorArgs::config` for first-line length — not the addition of a new doc-string.

## Archived `bootroom --help` Output (post-audit)

```
Web-based test harness for RISC-V kernels via qemu-wasm.

Usage: bootroom <COMMAND>

Commands:
  serve   Start the local HTTP server and serve the qemu-wasm UI
  run     Run a scenario headlessly under Chromium and exit 0/1 on serial assertions
  check   Parse and validate bootroom.toml without starting the server
  init    Write a starter bootroom.toml to the current directory
  doctor  Run preflight checks (version, browser, headers, config)
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Note: clap strips the trailing period from each first-line `///` doc when rendering the one-line Commands row. This is expected clap-derive behaviour; the source-level doc-strings keep the trailing period for grammatical correctness in long_about views.

## Test Count Delta — `cargo test -p bootroom --test cli_subcommands`

| | Before plan | After plan |
|---|---|---|
| Total tests | 7 | 8 |
| New | — | `top_level_help_lists_exactly_five_subcommands` |

Plus the cli.rs `#[cfg(test)] mod tests` unit tests stayed at 121 passing, unchanged. No existing test was removed or modified.

## Task Commits

1. **Task 1: Doc-string audit on Cmd variants and *Args structs in cli.rs** — `4f6af6d` (docs)
2. **Task 2: Add five-subcommand regression test in cli_subcommands.rs** — `b2cdcf8` (test)

## Files Created/Modified

- `crates/bootroom/src/cli.rs` — Rewrote Cmd::Run first-line doc (drop plan-internal jargon, ≤80 chars). Split DoctorArgs::config doc into a ≤80-char first line + long-help paragraph.
- `crates/bootroom/tests/cli_subcommands.rs` — Added `top_level_help_lists_exactly_five_subcommands`: forward-pins five names in order, inverse-pins against surprise subcommands.

## Decisions Made

See `key-decisions` in frontmatter. The non-obvious ones:

- **TDD cycle collapsed for Task 2.** Task 2 is `tdd="true"` but the production code (the five Cmd variants) already exists. A test asserting their presence passes on first run. The TDD execution guard says "if RED passes unexpectedly, STOP" — but the plan explicitly frames Task 2 as a REGRESSION PIN of an already-shipped surface, which is a known degenerate case. Committed as a single `test(...)` GREEN commit; no separate RED commit because no production-side change drove the test.
- **Did not edit long_about jargon.** The audit (constraints 1 + 3 of Task 1) is scoped to FIRST-LINE text only. Cmd::Serve's long_about contains "MUST be the first variant — preserves help-text ordering and the Phase-2 subprocess test invocation shape (Pitfall #9 mitigation)" which IS internal-implementation chatter, but it does not render on `bootroom --help` (top-level) — only on `bootroom serve --help` (subcommand-specific). Leaving it for a possible future Phase-6 cleanup.

## Deviations from Plan

None — plan executed exactly as written.

The plan's `<interfaces>` block proposed an alternate Cmd::Serve first-line ("Serve the harness over HTTP for a real browser tab.") with a "or similar — preserve Phase-1 intent" guard. The existing Phase-1 first-line ("Start the local HTTP server and serve the qemu-wasm UI.") is 55 chars, declarative, and jargon-free; per the plan's "leave alone if it already meets constraints" rule, no edit was made.

## Issues Encountered

None.

## Next Phase Readiness

- CLI-01 contract is pinned in CI. Any future plan that touches the five-subcommand surface (adds, removes, renames) will fire the regression test with a diagnostic naming the offending subcommand.
- Onboarding-quality: `bootroom --help` now renders five clean one-liner rows. First user-visible interaction with the binary is unembarrassing.
- No blockers for Plan 05-07 (the remaining plan in phase 5).

## Self-Check: PASSED

Verified post-write:

- `crates/bootroom/src/cli.rs` — modified, doc-string edits present (Cmd::Run + DoctorArgs::config).
- `crates/bootroom/tests/cli_subcommands.rs` — modified, new test function `top_level_help_lists_exactly_five_subcommands` present (1 occurrence).
- Commit `4f6af6d` exists in `git log`.
- Commit `b2cdcf8` exists in `git log`.
- `cargo build -p bootroom` succeeds.
- `cargo test -p bootroom --test cli_subcommands` — 8/8 passing.
- `cargo test -p bootroom --lib` — 121/121 passing.

---
*Phase: 05-diagnostics-doctor*
*Plan: 06*
*Completed: 2026-05-19*
