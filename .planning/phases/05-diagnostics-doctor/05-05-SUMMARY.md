---
phase: 05-diagnostics-doctor
plan: 05
subsystem: testing
tags: [doctor, integration-tests, json-schema, contract-pin, coop, coep, tdd]

# Dependency graph
requires:
  - phase: 05-diagnostics-doctor/05-04
    provides: full doctor_cmd body (six checks, two formatters, in-process header self-check)
  - phase: 05-diagnostics-doctor/05-01
    provides: BOOTROOM_GIT_SHA compile-time env var
  - phase: 05-diagnostics-doctor/05-03
    provides: Cmd::Doctor + DoctorArgs CLI surface
provides:
  - subprocess integration test surface for `bootroom doctor` (5 test files, 21 tests)
  - JSON schema v1 contract pin (top-level keys, schema_version, check names, status enum, overall enum)
  - human-format contract pin (section headers, ASCII glyphs, Overall: line, banner)
  - in-process COOP/COEP regression guard via direct check_headers() call
  - exit-code dispatch pin (0 on green, 1 on broken config) decoupled from stdout
affects: [phase-06+, ci-policy, schema-versioning]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - subprocess integration tests via CARGO_BIN_EXE_bootroom + tempfile::TempDir
    - exact-membership BTreeSet assertions for schema key pins (no "at-least" sets)
    - in-process pub-fn re-export (Option A) for load-bearing library entry points
    - test files split one-concern-per-file for localized failure diagnosis

key-files:
  created:
    - crates/bootroom/tests/doctor_exit_codes.rs
    - crates/bootroom/tests/doctor_human_format.rs
    - crates/bootroom/tests/doctor_json_schema.rs
    - crates/bootroom/tests/doctor_headers_check.rs
  modified:
    - crates/bootroom/tests/doctor_subcommand.rs
    - crates/bootroom/src/doctor_cmd.rs

key-decisions:
  - "Option A chosen for headers self-check — exposed doctor_cmd::check_headers as pub fn so integration tests outside the crate call it directly (no JSON-scrape indirection)"
  - "All schema/section/glyph pins use exact-membership BTreeSet equality, not subset checks — additions trip a deliberate decision"
  - "Banner line tolerates ASCII dash OR em-dash; implementer chose ASCII to honour the no-unicode-anywhere rule"
  - "All tempdirs allocated via tempfile::tempdir() — auto-cleanup on Drop; zero writes outside std::env::temp_dir()"

patterns-established:
  - "Per-test tempdir + Command::current_dir() — keeps test runner CWD clean and prevents accidental ./bootroom.toml pickup"
  - "Exit-code tests assert via status.code() = Some(N) and reject None (signal termination is a distinct failure mode)"
  - "JSON schema pins are exact key sets — adding a sixth top-level key requires bumping schema_version in lockstep with the test"

requirements-completed: [DOC-01]

# Metrics
duration: 6min
completed: 2026-05-19
---

# Phase 5 Plan 5: Doctor integration test surface Summary

**Subprocess contract pins for `bootroom doctor`: 21 new tests across 5 files lock human format, JSON v1 schema, exit-code dispatch, and the in-process COOP/COEP self-check against regression.**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-05-19T17:25:57Z
- **Completed:** 2026-05-19T17:31:44Z
- **Tasks:** 3
- **Files modified:** 6 (5 tests + 1 src visibility change)

## Accomplishments

- **Subprocess contract pin** — `bootroom --help` lists `doctor`; `doctor --help` advertises `--config` + `--format`; bare `doctor` exits 0 with `Overall: pass`; `--format json` emits valid v1-schema JSON; broken `--config` exits 1 with `bootroom doctor: …` summary on stderr.
- **JSON v1 schema lock** — exact top-level key set, `schema_version == 1`, exactly six check names, `status ∈ {pass, fail, info}`, `overall ∈ {pass, fail}`, `version == CARGO_PKG_VERSION`, `git_sha` shape pinned.
- **Human format lock** — five `##` section headers, ASCII glyphs only (U+2713/U+2717/U+2013/U+2014 all forbidden), `Overall: pass|fail` final line, banner contains `bootroom doctor` + `preflight`.
- **In-process headers self-check** — `bootroom::doctor_cmd::check_headers().await` exposed `pub` (Option A); regression in `tower-http` COOP/COEP middleware trips this test without a browser.
- **Exit-code dispatch pin** — canonical 0-on-green / 1-on-broken-config decoupled from stdout in `doctor_exit_codes.rs`, plus parity assertion across `human` and `json` formatters.

## Task Commits

1. **Task 1: Extend doctor_subcommand.rs + add doctor_exit_codes.rs** — `0dbbde9` (test)
2. **Task 2: doctor_human_format.rs + doctor_json_schema.rs** — `be2589a` (test)
3. **Task 3: doctor_headers_check.rs (Option A: pub check_headers)** — `5a5170f` (test + minor `pub` visibility flip in `doctor_cmd.rs`)

## Files Created/Modified

- `crates/bootroom/tests/doctor_subcommand.rs` — extended with 5 subprocess contract tests (top-level help, doctor --help, bare exit-zero, JSON schema, stderr summary).
- `crates/bootroom/tests/doctor_exit_codes.rs` — 4 tests; exit-code dispatch decoupled from stdout, parity across `human`/`json` formatters.
- `crates/bootroom/tests/doctor_human_format.rs` — 4 tests; section headers, ASCII-glyph guard, Overall: line, banner.
- `crates/bootroom/tests/doctor_json_schema.rs` — 7 tests; key set, schema_version, check names, status/overall enums, version, git_sha shape.
- `crates/bootroom/tests/doctor_headers_check.rs` — 1 `#[tokio::test(flavor = "multi_thread")]`; direct `check_headers().await` against `build_router`.
- `crates/bootroom/src/doctor_cmd.rs` — `check_headers` visibility flipped from private `async fn` to `pub async fn` (Option A enabler).

## Decisions Made

- **Option A (preferred in plan) chosen for the headers self-check.** `bootroom::doctor_cmd::check_headers` is now `pub` and called in-process from `tests/doctor_headers_check.rs`. Rationale: load-bearing regression test deserves a direct call, not a JSON scrape. The visibility flip is two characters and `Check`/`CheckStatus` were already `pub`.
- **Exact-membership pins, not subset.** Both the JSON top-level key set and the six check names use `BTreeSet` equality. A future addition trips the test and forces a schema_version decision — that is the explicit Pitfall-5 mitigation.
- **No `#[ignore]` tests.** Plan allowed an `#[ignore]` for the optional `doctor_exit_code_unaffected_by_browser_missing` case; CI image has chromium reliably, and the unit-test suite already covers the browser=Info-not-Fail invariant in `doctor_cmd::tests::browser_status_info_does_not_set_overall_fail`, so I omitted that test rather than ignore it.
- **Banner accepts ASCII dash (current implementation) — the em-dash assertion in the plan was relaxed to "contains `bootroom doctor` + `preflight`" because the 05-04 implementer chose ASCII for the banner consistent with the no-unicode rule.**

### JSON top-level key set pinned (verbatim from `tests/doctor_json_schema.rs`)

```rust
let expected: BTreeSet<&str> =
    ["checks", "git_sha", "overall", "schema_version", "version"]
        .into_iter()
        .collect();
```

(Identical pin also present in `tests/doctor_subcommand.rs::doctor_format_json_emits_valid_schema` as the subprocess-level contract anchor.)

## Deviations from Plan

None - plan executed exactly as written, with two minor scope-trims documented under Decisions Made:

- Optional `doctor_exit_code_unaffected_by_browser_missing` test omitted (covered by the existing unit test, no `#[ignore]` introduced).
- Banner assertion relaxed to substring match (em-dash vs ASCII dash difference, captured explicitly in the test file's docstring).

Neither trim weakens the contract surface; both are documented in test source so a future reader can re-add them deliberately if the surrounding constraints change.

## Issues Encountered

- **Worktree was at pre-phase-5 commit (`1a224cf`) at agent spawn.** Merged `master` (HEAD `22891ff`) into the worktree branch as a fast-forward-merging step before reading the plan; otherwise neither the plan file nor the `doctor_cmd.rs` source from 05-04 existed in this checkout. The merge is a single commit and includes all of phase 4 + phase 5 waves 1–3.

## Verification

- `cargo test -p bootroom --test doctor_subcommand` → **8 passed** (3 pre-existing git_sha + 5 new)
- `cargo test -p bootroom --test doctor_exit_codes` → **4 passed**
- `cargo test -p bootroom --test doctor_human_format` → **4 passed**
- `cargo test -p bootroom --test doctor_json_schema` → **7 passed**
- `cargo test -p bootroom --test doctor_headers_check` → **1 passed**
- `cargo test -p bootroom` total passed: **214** (baseline 193, delta **+21**, plan's floor of ≥12 met).
- `cargo test --workspace` → green.
- `cargo clippy --tests` on the four new files: **zero warnings** introduced by 05-05 code (pre-existing pedantic warnings in 05-01 scaffold and elsewhere are out of scope per Rule 4).
- **No test writes outside `std::env::temp_dir()`** — every test allocates via `tempfile::tempdir()` and writes into the resulting `TempDir` whose `Drop` impl cleans up.

## Next Phase Readiness

- DOC-01 requirement complete. The doctor's external contract is now locked: human format, JSON v1 schema, exit-code dispatch, and the COOP/COEP self-check all have dedicated regression guards.
- Plan 05-06 (final phase plan) can build on a fully-pinned doctor surface.
- Schema v2 path is documented: any future change to `checks` array shape or top-level keys will trip `json_top_level_keys_are_exactly_five` / `json_checks_names_are_the_six_known` and force the implementer to either bump `schema_version` or update both code and pins together.

## Self-Check: PASSED

All five test files exist:
- `crates/bootroom/tests/doctor_subcommand.rs` — FOUND
- `crates/bootroom/tests/doctor_exit_codes.rs` — FOUND
- `crates/bootroom/tests/doctor_human_format.rs` — FOUND
- `crates/bootroom/tests/doctor_json_schema.rs` — FOUND
- `crates/bootroom/tests/doctor_headers_check.rs` — FOUND

All three task commits exist in `git log`:
- `0dbbde9` — FOUND
- `be2589a` — FOUND
- `5a5170f` — FOUND

---
*Phase: 05-diagnostics-doctor*
*Completed: 2026-05-19*
