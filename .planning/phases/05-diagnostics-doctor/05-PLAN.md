---
phase: 05-diagnostics-doctor
type: overview
mode: mvp
plan_count: 6
waves: 3
---

# Phase 5: Diagnostics & Doctor — Plan Set Overview

## Phase Goal

**As a** user (or a confused CI job), **I want to** run `bootroom doctor` and get a single-screen preflight report (version, embedded qemu-wasm rev, detected browser, COOP/COEP self-check, config validity), **so that** I can verify my install before filing bugs and close the documented CLI surface (`serve, run, check, init, doctor`).

## Requirements Coverage

- **CLI-01** — finalized top-level subcommand surface (`serve, run, check, init, doctor`, `--version`, `--help`). Covered by 05-03 (variant + dispatch) and 05-06 (about-text audit + presence regression test).
- **DOC-01** — `bootroom doctor` reports version, qemu-wasm rev, browser, COOP/COEP self-check, config validity. Covered by 05-01 (git SHA capture), 05-02 (qemu-wasm-rev.txt), 05-04 (doctor body + formatters), 05-05 (integration tests).

## Multi-Source Coverage Audit

| Source | Item | Covered by |
|---|---|---|
| GOAL | One-screen preflight report on demand | 05-04 (formatters, body), 05-05 (subprocess gate) |
| GOAL | Closing CLI surface (`serve, run, check, init, doctor`) | 05-03, 05-06 |
| REQ | CLI-01 (short verbs + --help + --version) | 05-03, 05-06 |
| REQ | DOC-01 (5 checks + exit-code contract) | 05-01, 05-02, 05-04, 05-05 |
| RESEARCH | `tower::ServiceExt::oneshot` for headers self-check (Pattern 4, Variant A) | 05-04 |
| RESEARCH | `tower = util` feature promoted dev-dep → dep (Pitfall 6) | 05-03 (Cargo.toml bump) |
| RESEARCH | `discover_chromium` private → `pub(crate)` (Pitfall 7) | 05-03 |
| RESEARCH | `build.rs` git SHA with graceful `"unknown"` fallback (Pattern 3, Pitfall 1) | 05-01 |
| RESEARCH | `embed::QEMU.get_file("qemu-wasm-rev.txt")` Option degradation (Pattern 5, Pitfall 2) | 05-02 + 05-04 |
| RESEARCH | ASCII glyphs (Pattern 1 — overrides CONTEXT unicode) | 05-04 |
| RESEARCH | JSON `schema_version: 1` pinned in tests (Pitfall 5) | 05-05 |
| RESEARCH | Stderr summary on overall=fail (CONTEXT decision; Pitfall 8) | 05-04 + 05-05 |
| RESEARCH | clap doc-string first-line audit (Pitfall 9) | 05-06 |
| CONTEXT | D-DOC-01 — Human + JSON output, exit 0/1 | 05-04, 05-05 |
| CONTEXT | D-DOC-02 — Reuse `discover_chromium`; browser-missing does NOT fail overall | 05-04 |
| CONTEXT | D-DOC-03 — In-process axum + `build_router` self-check | 05-04 |
| CONTEXT | D-DOC-04 — Version = `CARGO_PKG_VERSION` + `BOOTROOM_GIT_SHA` build.rs capture | 05-01, 05-04 |
| CONTEXT | D-DOC-05 — qemu-wasm rev read from embedded file, `"unknown"` fallback | 05-02, 05-04 |
| CONTEXT | D-DOC-06 — Config validity via `LoadedConfig::load_from_str`; no-config = info | 05-04 |
| CONTEXT | D-DOC-07 — CLI surface check #6 (kept separate per Research Open Q2) | 05-04, 05-05 |
| CONTEXT | D-CLI-01 — `Cmd::Doctor` appended last; Serve order preserved | 05-03 |
| CONTEXT | D-CLI-02 — `DoctorArgs { config, format }`; no CommonArgs flatten | 05-03 |
| CONTEXT | D-CLI-03 — Audit `///` first lines for all subcommands | 05-06 |

**No unplanned source items.** All locked decisions land in at least one plan; no Deferred Ideas (JUnit/GH report formats, `--quick`/`--exhaustive`, self-update, multi-arch, doctor-in-browser, `BOOTROOM_CHROMIUM_ARGS`) appear in any plan.

## Glyph Convention Deviation

CONTEXT.md `<specifics>` mentions unicode `✓ / ✗ / –`. Phase 4 `crates/bootroom/src/verbose.rs` already established ASCII-only glyphs (`GLYPH_PASS = "+ "`, `GLYPH_FAIL = "- "`) with documented rationale (Windows console rendering, CI log grep stability). Per Research Open Q1 and the orchestrator's open-question resolution, **Phase 5 uses ASCII** to match Phase 4 precedent. Add `GLYPH_INFO = "~ "` to `verbose.rs` and reuse all three from `doctor_cmd.rs`. The JSON schema is unaffected (status values `"pass" | "fail" | "info"` are textual, not glyphs).

## Dependency Graph & Waves

Three waves total; all six plans have explicit `depends_on` lists driving the wave assignment.

```
Wave 1 (parallel — three independent prep plans, zero file overlap):
  05-01  build.rs BOOTROOM_GIT_SHA capture
         files_modified: crates/bootroom/build.rs,
                         crates/bootroom/tests/doctor_subcommand.rs (scaffold)
         depends_on: []

  05-02  Makefile qemu-assets writes qemu-wasm-rev.txt + sentinel
         files_modified: Makefile,
                         crates/bootroom/assets/qemu/qemu-wasm-rev.txt
         depends_on: []

  05-03  Cmd::Doctor variant + Cargo.toml tower::util + discover_chromium pub(crate) + doctor_cmd stub
         files_modified: crates/bootroom/Cargo.toml,
                         crates/bootroom/src/cli.rs,
                         crates/bootroom/src/lib.rs,
                         crates/bootroom/src/main.rs,
                         crates/bootroom/src/run_cmd.rs,
                         crates/bootroom/src/doctor_cmd.rs
         depends_on: []
         (No strict dependency on 05-01/05-02 — 05-03 stubs doctor_cmd::run
          and does not exercise the env var or embedded file.)

Wave 2 (doctor body — composes all three Wave 1 outputs):
  05-04  doctor_cmd.rs full implementation
         files_modified: crates/bootroom/src/doctor_cmd.rs,
                         crates/bootroom/src/verbose.rs
         depends_on: 05-01 (env!(BOOTROOM_GIT_SHA)),
                     05-02 (embed::QEMU.get_file("qemu-wasm-rev.txt")),
                     05-03 (DoctorArgs/OutputFormat, tower::util in deps,
                            discover_chromium pub(crate), doctor_cmd module declared)

Wave 3 (verification — two parallel plans with zero file overlap):
  05-05  Doctor integration tests
         files_modified: crates/bootroom/tests/doctor_subcommand.rs,
                         crates/bootroom/tests/doctor_human_format.rs,
                         crates/bootroom/tests/doctor_json_schema.rs,
                         crates/bootroom/tests/doctor_headers_check.rs,
                         crates/bootroom/tests/doctor_exit_codes.rs
         depends_on: 05-04

  05-06  CLI doc-string audit + --help regression test
         files_modified: crates/bootroom/src/cli.rs (doc-strings only),
                         crates/bootroom/tests/cli_subcommands.rs
         depends_on: 05-03
```

**Wave 1 file overlap check:** Plans 05-01, 05-02, 05-03 share zero files in `files_modified`. The only edge case is that 05-01 also adds `crates/bootroom/tests/doctor_subcommand.rs` as a Wave-0 scaffold; 05-05 later extends this same file. Sequencing 05-05 in Wave 3 (after 05-04) ensures the extension happens after the scaffold and after the doctor body lands — no concurrent writes.

**Wave 3 file overlap check:** 05-05 touches `crates/bootroom/tests/*` (five new files + one extension to doctor_subcommand.rs). 05-06 touches `crates/bootroom/src/cli.rs` and `crates/bootroom/tests/cli_subcommands.rs`. Zero overlap.

## Plan Index

| Plan | Wave | Title | Tasks | Autonomous |
|------|------|-------|-------|------------|
| 05-01 | 1 | build.rs git SHA capture | 2 | yes |
| 05-02 | 1 | Makefile + embedded qemu-wasm-rev.txt | 2 | yes |
| 05-03 | 1 | CLI surface — `Cmd::Doctor` variant, deps, visibility | 3 | yes |
| 05-04 | 2 | `doctor_cmd.rs` implementation + formatters | 3 | yes |
| 05-05 | 3 | Doctor integration tests (subprocess + schema) | 3 | yes |
| 05-06 | 3 | CLI about-text audit + --help regression test | 2 | yes |

Total: 6 plans / 15 tasks. All autonomous (no checkpoints). Doctor is fast (~100ms target) and entirely Rust — no headed-browser verification needed for Phase 5.

## Success Criteria (phase-level, goal-backward)

**Truths** (user perspective):
1. Running `bootroom --help` lists exactly `serve, run, check, init, doctor` and renders a clean about line for each.
2. Running `bootroom doctor` (with no flags) on a green install prints six section-grouped check lines and exits 0.
3. Running `bootroom doctor --format json` emits a stable JSON document with `schema_version: 1`, `version`, `git_sha`, `checks[]`, `overall`.
4. Running `bootroom doctor` with an intentionally broken `bootroom.toml` exits 1 and writes a one-line summary to stderr naming the failed check(s).
5. Running `bootroom doctor` on a machine without chromium prints `- browser …` (status=Info) but still exits 0 (browser is informational; does not set overall=fail).
6. The reported version includes the package version AND a short git SHA when built from a git checkout, or `"unknown"` when built from a crates.io tarball.

**Artifacts** (must exist by end of phase):
- `crates/bootroom/src/doctor_cmd.rs`
- `crates/bootroom/src/cli.rs` extended with `Cmd::Doctor(DoctorArgs)`, `DoctorArgs`, `OutputFormat`
- `crates/bootroom/build.rs` extended with `BOOTROOM_GIT_SHA` capture
- `crates/bootroom/assets/qemu/qemu-wasm-rev.txt` (committed; sentinel `"unknown"` until next `make qemu-assets` run)
- `crates/bootroom/tests/doctor_subcommand.rs`, `doctor_human_format.rs`, `doctor_json_schema.rs`, `doctor_headers_check.rs`, `doctor_exit_codes.rs`
- `crates/bootroom/src/verbose.rs` extended with `GLYPH_INFO`
- `Makefile` `qemu-assets` recipe writes `qemu-wasm-rev.txt`

**Key links** (where this is most likely to break):
- `build.rs` → `BOOTROOM_GIT_SHA` env → `env!("BOOTROOM_GIT_SHA")` in `doctor_cmd.rs::check_version`
- `Makefile qemu-assets` → `assets/qemu/qemu-wasm-rev.txt` → `embed::QEMU.get_file("qemu-wasm-rev.txt")` in `doctor_cmd.rs::check_qemu_rev`
- `cli.rs::Cmd::Doctor` → `main.rs` dispatch arm → `bootroom::doctor_cmd::run(args).await`
- `build_router(state) + ServiceExt::oneshot` → COOP/COEP headers → `doctor_cmd.rs::check_headers`
- `run_cmd::discover_chromium` visibility (must become `pub(crate)`) → `doctor_cmd.rs::check_browser`
- `tower::ServiceExt::oneshot` requires `tower = { features = ["util"] }` in `[dependencies]`, NOT `[dev-dependencies]`

## Out of Scope (Deferred per CONTEXT.md)

- JUnit / GitHub Actions report formats (REP-01/02 — v2)
- `--quick` / `--exhaustive` profiles
- Self-update / version probe against crates.io
- Multi-arch guest support (TGT-01 — v2)
- Doctor running inside the headless browser
- `BOOTROOM_CHROMIUM_ARGS` reporting (Research Open Q3 — deferred to v2)

## Next Steps

Execute: `/gsd-execute-phase 05`
