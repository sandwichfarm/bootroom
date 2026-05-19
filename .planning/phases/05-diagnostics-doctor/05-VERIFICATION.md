---
phase: 05-diagnostics-doctor
verified: 2026-05-19T17:50:29Z
status: passed
score: 6/6 must-haves verified
overrides_applied: 0
---

# Phase 5: Diagnostics & Doctor Verification Report

**Phase Goal:** `bootroom doctor` preflight (version, qemu-wasm rev, browser, COOP/COEP self-check, config) + finalize CLI surface (`serve, run, check, init, doctor`).
**Verified:** 2026-05-19T17:50:29Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `bootroom --help` lists exactly `serve, run, check, init, doctor` with clean about lines | VERIFIED | Live binary `--help` output enumerates the five subcommands in the documented order with one-line abouts; `tests/cli_subcommands.rs::top_level_help_lists_exactly_five_subcommands` pins both the forward (every documented name present, in order) and inverse (no surprise names beyond `help`) contract. |
| 2 | `bootroom doctor` (no flags, green build) prints six section-grouped lines and exits 0 | VERIFIED | Live run in an empty `/tmp/bootroom-verify-empty` produced six rendered checks under `## Version` / `## Browser` / `## Server headers` / `## Config` / `## CLI surface` and a final `Overall: pass`, exit 0. `tests/doctor_subcommand.rs::doctor_bare_exits_zero_on_green_build` and `tests/doctor_exit_codes.rs::doctor_exits_zero_when_all_checks_pass_or_info` lock the behavior. |
| 3 | `bootroom doctor --format json` emits stable schema with `schema_version: 1`, `version`, `git_sha`, `checks[]`, `overall` | VERIFIED | Live JSON output shows the exact five top-level keys and `"schema_version": 1`. `tests/doctor_subcommand.rs::doctor_format_json_emits_valid_schema` and `tests/doctor_json_schema.rs` pin the wire shape; `format_json_top_level_keys_pinned` / `format_json_schema_version_is_one` unit tests double-pin. |
| 4 | `bootroom doctor` with broken `bootroom.toml` exits 1 and writes a one-line stderr summary naming failed checks | VERIFIED | Live run with broken `bad.toml` produced `Overall: fail` on stdout, `bootroom doctor: 1/6 checks failed (config)` on stderr, exit 1. `tests/doctor_subcommand.rs::doctor_failure_writes_stderr_summary` and `tests/doctor_exit_codes.rs::doctor_exits_one_when_config_parse_fails` pin both axes. |
| 5 | Doctor on a machine without chromium reports `~ browser …` (Info) but exits 0 | VERIFIED | `doctor_cmd.rs::check_browser` returns `CheckStatus::Info` on both `Err` (not found) and probe-failure paths (lines 192-204). The overall failure check at line 89 filters only on `Fail`, so `Info` cannot contribute. Unit test `browser_status_info_does_not_set_overall_fail` (line 732-740) pins this invariant. |
| 6 | Version reports package version + short git SHA, or `"unknown"` from a tarball | VERIFIED | Live run reports `bootroom 0.1.0 (3c94005)`. `build.rs` lines 69-77 chain `git rev-parse --short HEAD` through `.ok().filter().and_then().map().filter().unwrap_or_else(|| "unknown")` — no `.unwrap()` is reachable. `tests/doctor_subcommand.rs::git_sha_env_shape_is_short_sha_or_unknown` pins the format contract. |

**Score:** 6/6 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/bootroom/src/doctor_cmd.rs` | Full doctor body (6 checks, 2 formatters, exit-code logic) | VERIFIED | 921 lines; exports `Check`, `CheckStatus`, `run`, `check_headers`, `check_headers_against_router`, `format_human`, `format_json`. All six `check_*` functions present and wired into `run()`. |
| `crates/bootroom/src/cli.rs` extended with `Cmd::Doctor(DoctorArgs)`, `DoctorArgs`, `OutputFormat` | Append `Doctor` last in `Cmd`; `DoctorArgs { config, format }`; `OutputFormat::{Human, Json}` | VERIFIED | `Cmd::Doctor(DoctorArgs)` is the 5th variant after Serve/Run/Check/Init (lines 33-52); `DoctorArgs` with `config: Option<PathBuf>` + `format: OutputFormat` (lines 152-164); `OutputFormat` derives `ValueEnum` (lines 166-170). |
| `crates/bootroom/build.rs` extended with `BOOTROOM_GIT_SHA` capture | Captured on every code path, including the escape hatch | VERIFIED | SHA capture lives at lines 69-78, **above** the `BOOTROOM_SKIP_QEMU_ASSET_CHECK` early return at line 85 (BL-01 fix at commit baa335e). `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo check -p bootroom` succeeds. |
| `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` | Committed sentinel `"unknown"` | VERIFIED | File contains `unknown\n`. Listed in `build.rs::REQUIRED` (line 30) so `make qemu-assets` rewrites retrigger the `include_dir!` embed (CR-01 fix). |
| Doctor integration tests | 5 test files | VERIFIED | `doctor_subcommand.rs` (193 lines, 6 tests), `doctor_human_format.rs`, `doctor_json_schema.rs`, `doctor_headers_check.rs` (40 lines), `doctor_exit_codes.rs` (87 lines, 4 tests) all present and green. |
| `crates/bootroom/src/verbose.rs` extended with `GLYPH_INFO` | `GLYPH_INFO = "~ "` | VERIFIED | Line 19: `pub const GLYPH_INFO: &str = "~ ";` alongside the pre-existing PASS/FAIL glyphs. Used in `doctor_cmd.rs::glyph_for`. |
| `Makefile` `qemu-assets` recipe writes `qemu-wasm-rev.txt` | Recipe Step 5b writes file via `git rev-parse --short HEAD` | VERIFIED | Lines 67-69 print "Step 5b/5: Recording qemu-wasm git rev..." and write to `$(QEMU_OUT_DIR)/qemu-wasm-rev.txt`. `clean-qemu-assets` (line 80) removes it. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `build.rs` | `doctor_cmd.rs::check_version` / `format_json` | `cargo:rustc-env=BOOTROOM_GIT_SHA` → `env!("BOOTROOM_GIT_SHA")` | WIRED | `build.rs:78` emits the env; `doctor_cmd.rs:126` and `:511` consume it; `tests/doctor_subcommand.rs:11` independently re-checks the env at the test binary level (so a regression where the env vanishes from the test build also trips). |
| `Makefile qemu-assets` | `doctor_cmd.rs::check_qemu_rev` | `assets/qemu/qemu-wasm-rev.txt` → `embed::QEMU.get_file("qemu-wasm-rev.txt")` | WIRED | Makefile writes the file; `build.rs::REQUIRED` watches it; `include_dir!` in `src/embed.rs` embeds the qemu assets dir; `doctor_cmd.rs:141-146` reads via `.get_file().and_then(contents_utf8).map(trim).filter(!empty).unwrap_or("unknown")`. Falls back to `"unknown"` end-to-end (live run confirms). |
| `cli.rs::Cmd::Doctor` | `doctor_cmd::run(args).await` | `main.rs` match arm | WIRED | `main.rs:26`: `Cmd::Doctor(args) => Ok(bootroom::doctor_cmd::run(args).await)`. Live `bootroom doctor` invocation exits with the expected code on both green and red paths. |
| `build_router(state) + ServiceExt::oneshot` | `doctor_cmd::check_headers` | In-process axum `Router` + `tower::ServiceExt` | WIRED | `doctor_cmd.rs:218-232` builds the canonical router with `AppState::new_for_test` and a pid-scoped placeholder kernel (WR-03), then delegates to `check_headers_against_router`. Live run reports `+ headers COOP=same-origin, COEP=require-corp on /`. Negative test `check_headers_fails_on_bare_router_with_specific_detail` (doctor_cmd.rs:764-795) pins the Fail wording (WR-04). |
| `run_cmd::discover_chromium` visibility | `doctor_cmd::check_browser` | `pub(crate) fn discover_chromium()` | WIRED | `run_cmd.rs:396`: `pub(crate) fn discover_chromium() -> Result<PathBuf, String>`; `doctor_cmd.rs:166` calls it. Live run resolved `/usr/bin/chromium` and reports `Chromium 148.0.7778.167 Arch Linux`. |
| `tower = { features = ["util"] }` in `[dependencies]` | `doctor_cmd::check_headers_against_router` use of `ServiceExt::oneshot` | Cargo.toml | WIRED | Build succeeds without `#[cfg(test)]`-gated tower; the `use tower::ServiceExt;` in `doctor_cmd.rs:242` compiles outside `#[cfg(test)]`. |

### Data-Flow Trace (Level 4)

Doctor is a CLI that renders dynamic data at runtime — `version`, `git_sha`, embedded qemu rev, discovered browser path/version, COOP/COEP probe result, and config-load outcome. Traced each source against its rendered output:

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| `doctor_cmd::check_version` | `version`, `sha` | `env!("CARGO_PKG_VERSION")`, `env!("BOOTROOM_GIT_SHA")` (from build.rs git invocation) | Yes — live run shows `bootroom 0.1.0 (3c94005)` matching `git rev-parse --short HEAD` | FLOWING |
| `doctor_cmd::check_qemu_rev` | `rev` | `embed::QEMU.get_file("qemu-wasm-rev.txt")` → file currently contains sentinel `"unknown"` | Yes — sentinel is real data; live run prints `qemu-wasm rev unknown` exactly as expected when `make qemu-assets` has not been run | FLOWING |
| `doctor_cmd::check_browser` | `path`, `version_line` | `run_cmd::discover_chromium()` + `tokio::process::Command::new(&path).arg("--version").output().await` | Yes — live run resolved chromium path and captured the actual `chromium --version` stdout | FLOWING |
| `doctor_cmd::check_headers_against_router` | `coop`, `coep` | `app.oneshot(Request::builder().uri("/")...)` → real response headers | Yes — live run reports `COOP=same-origin, COEP=require-corp on /` matching Phase-1 middleware contract | FLOWING |
| `doctor_cmd::check_config` | `bytes`, `loaded`/`e` | `std::fs::read_to_string` + `LoadedConfig::load_from_str` | Yes — live run with broken TOML reports the actual TOML parser error `bad.toml:1:6: key with no value, expected ` | FLOWING |
| `doctor_cmd::check_cli_surface` | hardcoded string | Hardcoded `"serve, run, check, init, doctor"` (intentional per `tests/cli_subcommands.rs` cross-pin) | Yes — paired with the exact-five-subcommand test ensuring the string and clap surface cannot drift | FLOWING |

No HOLLOW or DISCONNECTED artifacts. Every rendered value flows from a real source.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `--help` lists five subcommands in documented order | `bootroom --help` | Output enumerates `serve`, `run`, `check`, `init`, `doctor`, `help` in the expected order with rendered abouts | PASS |
| `--version` prints package version | `bootroom --version` | `bootroom 0.1.0` | PASS |
| Doctor exits 0 on green tree | `cd /tmp/empty && bootroom doctor` | All six checks rendered, `Overall: pass`, exit 0 | PASS |
| Doctor exits 1 on broken config | `bootroom doctor --config bad.toml` (where `bad.toml` is invalid TOML) | `Overall: fail` on stdout, `bootroom doctor: 1/6 checks failed (config)` on stderr, exit 1 | PASS |
| JSON output is valid + schema_version=1 | `bootroom doctor --format json` piped through a JSON parser mentally | Top-level keys exactly `schema_version, version, git_sha, checks, overall`; `schema_version: 1`; six check objects each with `name/status/detail` | PASS |
| BOOTROOM_SKIP_QEMU_ASSET_CHECK escape hatch (BL-01 fix) | `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo check -p bootroom` | Compiles cleanly with the expected `cargo:warning` line; no `env!()` failure | PASS |
| Full workspace test suite | `cargo test --workspace --no-fail-fast` | 39 test result lines, all `ok`; 266 individual tests passed across crates | PASS |

### Probe Execution

No probes are declared for this phase (no `scripts/*/tests/probe-*.sh` exist in this repo; PLAN/SUMMARY make no probe references — verification is via `cargo test`, in-binary integration tests, and the live binary runs above).

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| CLI-01 | 05-03, 05-06 | Top-level subcommands are short verbs: `serve, run, init, check, doctor`, `--version`, `--help` | SATISFIED | `tests/cli_subcommands.rs::top_level_help_lists_exactly_five_subcommands` pins exact set + order + inverse (no surprise commands). `-V` and `-h` are clap defaults; `--version` live run returns `bootroom 0.1.0`. |
| DOC-01 | 05-01, 05-02, 05-04, 05-05 | `bootroom doctor` reports version, qemu-wasm rev, browser, COOP/COEP self-check on `/`, config validity | SATISFIED | All six checks present and producing real data (see Data-Flow Trace). Five integration test files in `tests/doctor_*.rs` pin the subprocess contract; unit tests in `doctor_cmd.rs` pin the formatter shapes. Live runs match expected behavior on both pass and fail paths. |

No orphaned requirements: REQUIREMENTS.md lines 202 and 205 map CLI-01 and DOC-01 to Phase 5; both appear in the PLAN frontmatter coverage table.

### Anti-Patterns Found

Scanned `crates/bootroom/src/{cli.rs, doctor_cmd.rs, main.rs, lib.rs, build.rs, run_cmd.rs}`, `Makefile`, `assets/qemu/qemu-wasm-rev.txt`, and the five new test files for debt markers, stub patterns, console-log-only handlers, and hollow returns.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | — | — | — | No debt markers (`TBD`, `FIXME`, `XXX`, `HACK`, `PLACEHOLDER`) found in any phase-5 file. No `return null` / empty `=> {}` stubs. No `console.log`-only handlers. The `"unknown"` sentinel in `qemu-wasm-rev.txt` is intentional and documented as the green-path value until `make qemu-assets` runs. The hardcoded `"serve, run, check, init, doctor"` string in `check_cli_surface` is intentional and cross-pinned by the exact-five-subcommand integration test, so a drift between the rendered surface and the doctor's reported surface trips CI. |

### Human Verification Required

None. Phase 5 produces a Rust CLI with deterministic input/output and an in-process router self-check — no visual rendering, no real-time browser behavior, no external service dependencies that the automated checks cannot exercise.

### Gaps Summary

No gaps. The phase delivered every artifact, every key link is wired, every observable truth is reproducible at the command line, both requirements (CLI-01, DOC-01) are satisfied with multiple layers of test coverage, the full workspace test suite is green (39 suites / 266 tests), and the BL-01 + CR-01 + 5 WARNING findings from `05-REVIEW.md` are all closed and re-verified (`BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo check -p bootroom` succeeds; negative header test, async browser probe, tempdir test isolation, "## Other" catch-all section, and io-kind hints in config checks all present in code).

---

_Verified: 2026-05-19T17:50:29Z_
_Verifier: Claude (gsd-verifier)_
