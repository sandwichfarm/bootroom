---
phase: 05-diagnostics-doctor
plan: 04
subsystem: cli
tags: [doctor, preflight, diagnostics, ci]
requires:
  - 05-01  # BOOTROOM_GIT_SHA build-time capture
  - 05-02  # qemu-wasm-rev sentinel
  - 05-03  # doctor CLI scaffold + discover_chromium visibility raise
provides:
  - doctor.run                            # public async entrypoint
  - doctor.Check                          # per-check JSON row
  - doctor.CheckStatus                    # pass | fail | info
  - doctor.format_human                   # human report formatter
  - doctor.format_json                    # JSON report formatter (schema_version=1)
  - verbose.GLYPH_INFO                    # ~ glyph for informational rows
affects:
  - crates/bootroom/src/doctor_cmd.rs
  - crates/bootroom/src/verbose.rs
tech-stack:
  added: []
  patterns:
    - tower-oneshot-against-build_router  # in-process header self-check (Pattern 4 Variant A)
    - include_dir-file-read-with-sentinel-fallback  # Pattern 5 + Pitfall 2
    - ascii-glyphs-via-verbose-module     # reuses Phase-4 GLYPH_PASS/FAIL/INFO
key-files:
  created: []
  modified:
    - crates/bootroom/src/doctor_cmd.rs   # 5-line stub -> 768-line full body
    - crates/bootroom/src/verbose.rs      # +6 lines for GLYPH_INFO
decisions:
  - "Use ASCII glyphs (+, -, ~) per Research Open Q1 — overrides CONTEXT.md's unicode hint."
  - "Browser-missing emits status=Info (never Fail) per D-DOC-02 — does not contribute to overall=fail."
  - "Config-missing is Info; only parse error is Fail (matches `bootroom check`'s NotFound vs parse-error semantic split)."
  - "JSON schema_version=1 pinned as a top-level integer; CI tooling can stable-pin against this."
  - "Stderr summary line is additive on overall=fail (not a replacement for stdout)."
metrics:
  duration: 18m
  completed: 2026-05-19
---

# Phase 5 Plan 4: doctor-fill-in-checks Summary

`bootroom doctor` runs six preflight checks in fixed order (version,
qemu_wasm_rev, browser, headers, config, cli_surface) and emits either a
human-format report or a stable JSON document (`schema_version=1`). Exit
0 on overall pass; exit 1 on any required-check Fail, with an additive
single-line stderr summary for CI grep.

## Tasks Completed

| # | Task                                                      | Commit  | Files                                                      |
|---|-----------------------------------------------------------|---------|------------------------------------------------------------|
| 1 | GLYPH_INFO + Check/CheckStatus types (TDD)                | f57d051 | crates/bootroom/src/verbose.rs, doctor_cmd.rs              |
| 2 | Six checks + two formatters + 19 in-module tests          | 9e1f4b3 | crates/bootroom/src/doctor_cmd.rs                          |
| 3 | Manual smoke + regression sweep (no-edit verification)    | -       | -                                                          |

## Implementation Notes

### File metrics

- `doctor_cmd.rs`: **768 LOC** total, of which **448 production** and **320 tests**.
  - The plan target was ~400 LOC; the production half is 448 LOC (within
    rounding of the target) and the rest is mandatory in-module tests
    requested by Task 2 (schema pin + 7 other contract tests). Splitting
    formatters or checks into sibling files would fragment the surface
    and break the "one file per subcommand body" Phase-4 convention.
- `verbose.rs`: +6 lines for `pub const GLYPH_INFO: &str = "~ "` and its
  doc comment.

### Human-format column-alignment

- **Width: `HUMAN_NAME_WIDTH = 14`** (one space wider than the widest
  current check name `qemu_wasm_rev` = 13 chars).
- Rendered via `format!("{}{:<14} {}", glyph, name, detail)`.
- This is the contract that 05-05's integration tests should pin
  against.

### Header self-check (`check_headers`)

- Uses `tower::ServiceExt::oneshot` against the canonical
  `crate::build_router(state)`. The `state` is a placeholder
  `AppState::new_for_test(kernel, None)` — `new_for_test` tolerates a
  non-existent kernel path (state.rs:157 documented assumption A4).
- Asserts `cross-origin-opener-policy: same-origin` AND
  `cross-origin-embedder-policy: require-corp` on a GET `/` response.
- Failure modes are surfaced as `CheckStatus::Fail` with the actual
  observed values in the `detail` (e.g. `expected COOP=..., got COOP=None`).
- This is the load-bearing schema-drift mitigation from Pitfall 5:
  if a future change to the layer stack drops a header, the doctor's
  self-check fails immediately on a CI run.

### Browser check (`check_browser`)

- Delegates discovery to `crate::run_cmd::discover_chromium` (raised
  to `pub(crate)` in Plan 05-03).
- On `Ok(path)`: probes `<path> --version`; on success captures the
  first stdout line as the version string. On failed subprocess
  invocation OR non-zero exit, downgrades to `Info` with a
  `(--version probe failed)` suffix — **never** `Fail`.
- On `Err(_)`: returns `Info` with detail
  `"not found on PATH; install for \`bootroom run\`"`.
- This is the D-DOC-02 + Pitfall 4 contract: a missing browser is
  not a CI failure (the server still runs).

### Config check (`check_config`)

- Resolution mirrors `bootroom check`: explicit `--config <path>` wins;
  otherwise look for `./bootroom.toml` in CWD.
- `NotFound` -> `Info` with the appropriate detail string
  (different message for explicit vs default path).
- Any other `read_to_string` failure (permission denied, etc.) ->
  `Fail` with the IO error message.
- `LoadedConfig::load_from_str` Err -> `Fail` with `path:line:col: <msg>`
  (or `path: <msg>` if the parser did not report a span).
- `Ok` -> `Pass` with detail `"path: N actions, M scenarios"`.

### Exit-code semantics

| overall_failed | stdout                     | stderr                                              | exit |
|----------------|----------------------------|-----------------------------------------------------|------|
| `false`        | full human / JSON report   | (none)                                              | 0    |
| `true`         | full human / JSON report   | `bootroom doctor: N/6 checks failed (<names>)`      | 1    |

The stderr line writes ONLY in human mode; JSON consumers can read
`overall` from the document. (The plan's interface block specifies this
behavior in `<tasks>` Task 2 step 9 / Pitfall 8.)

## Sample Outputs

### `bootroom doctor` (human, all-pass)

```
bootroom doctor - preflight checks

## Version
~ version        bootroom 0.1.0 (9e1f4b3)
~ qemu_wasm_rev  qemu-wasm rev unknown

## Browser
+ browser        /usr/bin/chromium (Chromium 148.0.7778.167 Arch Linux)

## Server headers
+ headers        COOP=same-origin, COEP=require-corp on /

## Config
~ config         no bootroom.toml in CWD (use --config to specify)

## CLI surface
~ cli_surface    serve, run, check, init, doctor

Overall: pass
```

Exit code: 0.

### `bootroom doctor --format json` (all-pass)

```json
{
  "schema_version": 1,
  "version": "0.1.0",
  "git_sha": "9e1f4b3",
  "checks": [
    {
      "name": "version",
      "status": "info",
      "detail": "bootroom 0.1.0 (9e1f4b3)"
    },
    {
      "name": "qemu_wasm_rev",
      "status": "info",
      "detail": "qemu-wasm rev unknown"
    },
    {
      "name": "browser",
      "status": "pass",
      "detail": "/usr/bin/chromium (Chromium 148.0.7778.167 Arch Linux)"
    },
    {
      "name": "headers",
      "status": "pass",
      "detail": "COOP=same-origin, COEP=require-corp on /"
    },
    {
      "name": "config",
      "status": "info",
      "detail": "no bootroom.toml in CWD (use --config to specify)"
    },
    {
      "name": "cli_surface",
      "status": "info",
      "detail": "serve, run, check, init, doctor"
    }
  ],
  "overall": "pass"
}
```

### `bootroom doctor --config <broken.toml>` (overall=fail)

stdout (full report rendered as usual, with one `- config` row):

```
...
## Config
- config         /tmp/bootroom-doctor-test/bad.toml:1:6: key with no value, expected `=`

## CLI surface
~ cli_surface    serve, run, check, init, doctor

Overall: fail
```

stderr (single line, additive):

```
bootroom doctor: 1/6 checks failed (config)
```

Exit code: 1.

## Verification Evidence

### Smoke timing

- `./target/debug/bootroom doctor` (debug build, cold cache):
  **39 ms wall-clock**. Well under the 1-second target. No Chromium
  launch in the happy path — only a `--version` probe (sub-200 ms).

### Test count

- Pre-Phase-5 baseline (this worktree, `cargo test -p bootroom`): **174 passed**.
- Post Plan 05-04: **193 passed** (+19 new `doctor_cmd::tests::*`).
- Workspace-wide: **241 passed, 0 failed**.

### In-module tests added (19 total)

1. `check_serializes_with_lowercase_status` — Check JSON shape
2. `check_status_fail_serializes_lowercase` — enum tag pin
3. `check_status_info_serializes_lowercase` — enum tag pin
4. `format_json_schema_version_is_one` — schema_version pin
5. `format_json_top_level_keys_pinned` — top-level key set pin
6. `format_json_overall_pass_when_no_fails` — overall=pass semantic
7. `format_json_overall_fail_when_any_fail` — overall=fail semantic
8. `format_human_contains_section_headers` — section header pin
9. `format_human_uses_ascii_glyphs` — Open Q1 ASCII contract
10. `format_human_overall_fail_string` — final line pin
11. `browser_status_info_does_not_set_overall_fail` — D-DOC-02
12. `check_headers_passes_against_build_router` — self-check (Pitfall 5)
13. `check_qemu_rev_reads_embedded_file` — embed read contract
14. `check_config_missing_is_info_not_fail` — Info semantic
15. `check_config_missing_default_is_info` — default-path semantic
16. `check_config_broken_toml_is_fail` — Fail semantic
17. `check_config_valid_toml_is_pass` — Pass semantic
18. `check_version_detail_shape` — `bootroom X.Y.Z (<sha>)` pin
19. `check_cli_surface_lists_all_subcommands` — exact-string pin

### Clippy

`cargo clippy -p bootroom --all-targets` warning count:

- Pre-Plan-05-04 baseline (origin/master): **4 warnings** (pre-existing).
- Post Plan 05-04: **3 warnings**, all in pre-existing files
  (`cli.rs:156` doc-markdown, `tests/doctor_subcommand.rs`
  always-false expression + redundant closure).
- **No new clippy warnings introduced by this plan.**

### Manual smoke scenarios

| Scenario                                  | stdout shape | exit | stderr |
|-------------------------------------------|--------------|------|--------|
| A. `bootroom doctor` (green build)        | full human   | 0    | (none) |
| B. `bootroom doctor --format json`        | valid JSON, schema_version=1, 6 checks | 0 | (none) |
| C. `bootroom doctor --config <broken.toml>` | full human report with `- config` row | 1 | `bootroom doctor: 1/6 checks failed (config)` |

All three scenarios behaved exactly as specified.

### ASCII-glyph contract (Open Q1)

`grep -P '[\x{2713}\x{2717}\x{2013}]' crates/bootroom/src/doctor_cmd.rs | grep -v '^//'`
returns no matches — em-dashes appear only in module-level doc
comments, never in rendered output strings.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Replaced em-dash with hyphen in the human-format header**

- **Found during:** Task 2 GREEN step.
- **Issue:** Plan interface block showed `bootroom doctor — preflight
  checks` with an em-dash (U+2014) in the rendered output. The Open Q1
  ASCII-only contract (also asserted by my own
  `format_human_uses_ascii_glyphs` test) forbids U+2013/U+2014 in
  rendered output. The test failed on the first run.
- **Fix:** Rendered header changed to `bootroom doctor - preflight
  checks` (ASCII hyphen). Em-dashes remain only in source-code
  comments / doc strings (the test does not inspect those).
- **Files modified:** `crates/bootroom/src/doctor_cmd.rs`.
- **Commit:** 9e1f4b3 (within the Task 2 patch).

**2. [Rule 2 - Hygiene] Suppressed module-level pedantic-clippy noise**

- **Found during:** post-Task-2 clippy run.
- **Issue:** `clippy::pedantic` is enabled at workspace level; my
  additions triggered `similar_names` (`coop`/`coep` differ by one
  letter — intentional), `doc_markdown` (acronyms COOP/COEP are not
  in backticks), and `must_use_candidate` (every formatter would
  need `#[must_use]`).
- **Fix:** Added `#![allow(clippy::similar_names, clippy::doc_markdown,
  clippy::must_use_candidate)]` at module scope with a comment
  explaining the rationale. Also added a `# Panics` section to
  `format_json` and switched the test `vec![Check { .. }]` to an array
  literal (the genuinely useful clippy hints).
- **Net effect:** clippy warning count DROPPED from 4 (baseline) to
  3 (post-plan). No regressions.
- **Files modified:** `crates/bootroom/src/doctor_cmd.rs`.
- **Commit:** 9e1f4b3.

No other deviations. Plan executed exactly as written.

## Self-Check: PASSED

- File `crates/bootroom/src/doctor_cmd.rs` exists (768 LOC).
- File `crates/bootroom/src/verbose.rs` exists, contains `GLYPH_INFO`.
- File `.planning/phases/05-diagnostics-doctor/05-04-SUMMARY.md` exists (this file).
- Commit `f57d051` (Task 1) present in `git log`.
- Commit `9e1f4b3` (Task 2) present in `git log`.
- `bootroom doctor` exits 0 on a clean checkout; `bootroom doctor
  --config /tmp/bootroom-doctor-test/bad.toml` exits 1 with the
  expected stderr summary.
- `cargo test -p bootroom`: 193 passed, 0 failed.
- `cargo test --workspace`: 241 passed, 0 failed.
- `cargo clippy -p bootroom --all-targets`: 3 warnings (all pre-existing).
