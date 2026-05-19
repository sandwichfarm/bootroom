---
phase: 06-distribution
fixed_at: 2026-05-19T21:50:00Z
review_path: .planning/phases/06-distribution/06-REVIEW.md
iteration: 1
findings_in_scope: 9
fixed: 9
skipped: 0
status: all_fixed
---

# Phase 6: Code Review Fix Report

**Fixed at:** 2026-05-19T21:50:00Z
**Source review:** `.planning/phases/06-distribution/06-REVIEW.md`
**Iteration:** 1

**Summary:**
- Findings in scope: 9 (4 CRITICAL + 5 WARNING; 4 INFO skipped per scope)
- Fixed: 9
- Skipped: 0
- Workspace tests: `cargo test --workspace` -> 124+ tests across all binaries, all green
- `cargo package -p bootroom-core --no-verify` -> green (post-fix)
- `cargo package -p bootroom --no-verify` -> manifest-validation green (verify
  build still requires bootroom-core on crates.io; documented in WR-05 commit)
- `cargo deny check licenses advisories` -> ok (post-fix)

## Fixed Issues

### CR-01: bootroom-core workspace dep has no version

**Files modified:** `Cargo.toml`
**Commit:** `ce2ad38`
**Applied fix:** Changed `bootroom-core = { path = "crates/bootroom-core" }` to
`bootroom-core = { path = "crates/bootroom-core", version = "0.1.0" }` in the
workspace dependency table. `cargo package -p bootroom --no-verify` now passes
the manifest-validation step that previously errored with "all dependencies
must have a version requirement specified when packaging."

### CR-02: Shell glob `/work/bootroom-*` matches both crate dirs

**Files modified:** `.github/workflows/release-smoke.yml`
**Commit:** `14bc1bb` (clustered with WR-05)
**Applied fix:** Replaced every shell-glob `bootroom-*` / `bootroom-core-*`
reference with an explicit `${BOOTROOM_VERSION}` pin resolved up front via
`cargo metadata --no-deps --format-version 1 | jq -r ...`. The variable is
passed into the Docker container via `-e BOOTROOM_VERSION=...` and used to
construct each `tar -xzf .../bootroom-${BOOTROOM_VERSION}.crate` and
`cargo install --path /work/bootroom-${BOOTROOM_VERSION}` invocation. Applied
to all three Docker steps (smoke, install_smoke tests, DIST-05 path-
independence check). Also dropped the redundant `cd /tmp` inside the first
Docker block (the `-w /tmp` flag already sets the CWD — IN-02 freebie).

### CR-03: `path_independence_qemu_wasm_rev_present` asserts wrong status

**Files modified:** `crates/bootroom/tests/install_smoke.rs`
**Commit:** `1e46306`
**Applied fix:** Option (a) from the review — changed the assertion from
`rev_status == "pass"` to `rev_status == "info"` to match the documented
contract of `doctor_cmd.rs::check_qemu_rev`, which is hard-coded to return
`CheckStatus::Info`. Added a separate assertion on the check's `detail` string
to enforce the actual DIST-05 signal (the detail must not contain the degraded
sentinel `"rev unknown"`), preserving the regression-detection intent of the
test without touching doctor_cmd's contract. Updated the file-level rustdoc
to match.

### CR-04: `custom-release-smoke` does not gate `host`

**Files modified:** `.github/workflows/release.yml`, `dist-workspace.toml`
**Commit:** `40e643c`
**Applied fix:** Hand-edited `release.yml` to add `custom-release-smoke` to
`host.needs:` and to require its success (`needs.custom-release-smoke.result
== 'skipped' || ... == 'success'`) in the `host.if:` predicate. cargo-dist
v0.31's `host-jobs` hook only wires the smoke into `announce` (operationally
inert), so the `gh release create` step inside `host` was previously running
in parallel with the smoke; a broken binary could have reached `cargo
binstall` and the curl installer despite a smoke failure. Updated the
`host-jobs` comment in `dist-workspace.toml` to warn future maintainers that
the next `dist generate` run will undo this edit and must be re-applied. This
also implicitly addresses IN-03 (the comment-vs-reality gap in dist-workspace.toml).

### WR-01: README install matrix predates crates.io publish

**Files modified:** `README.md`
**Commit:** `4d164e3` (clustered with WR-02..WR-04)
**Applied fix:** Added a "Pre-1.0 status" note above the install matrix
pointing users at the `cargo install --locked --git
https://github.com/sandwich-farm/bootroom bootroom` fallback (and `make
install` from a clone) until bootroom is on crates.io and a tagged GitHub
Release exists.

### WR-02: actions/upload-artifact@v6 and actions/download-artifact@v7 do not exist

**Files modified:** `.github/workflows/release.yml`
**Commit:** `4d164e3`
**Applied fix:** Replaced all 11 references (6 `upload-artifact@v6` + 5
`download-artifact@v7`) with `@v4`. `actions/checkout@v6` was left as-is (not
flagged by review). cargo-dist will regenerate these to the broken versions on
next `dist generate`; the dist-workspace.toml host-jobs comment (added in the
CR-04 commit) now warns about the broader re-apply-by-hand pattern.

### WR-03: LICENSE-APACHE appendix placeholder unfilled

**Files modified:** `LICENSE-APACHE`
**Commit:** `4d164e3`
**Applied fix:** Replaced `Copyright [yyyy] [name of copyright owner]` with
`Copyright 2026 sandwich` in the canonical Apache 2.0 appendix. No NOTICE file
needed (the appendix is the canonical attribution surface).

### WR-04: deny.toml missing `version = 2` and `yanked = "warn"` posture

**Files modified:** `deny.toml`
**Commit:** `4d164e3`
**Applied fix:** Added explicit `version = 2` to both `[licenses]` and
`[advisories]` tables to defend against a future cargo-deny default flip.
Promoted `[advisories].yanked` from `"warn"` to `"deny"` per the review's
recommendation; verified locally with `cargo deny check licenses advisories`
which reported `advisories ok, licenses ok` (the only warnings are
unencountered allowed licenses, which are expected on a deps tree that does
not happen to include MPL-2.0 / Unicode-DFS-2016 / Zlib crates).

### WR-05: `cargo package --allow-dirty --no-verify` hides regressions

**Files modified:** `.github/workflows/release-smoke.yml`
**Commit:** `14bc1bb` (clustered with CR-02)
**Applied fix:** Removed both `--allow-dirty` and `--no-verify` flags from the
two `cargo package` invocations in the smoke. Added an in-file comment noting
the catch-22: the bootroom verify build will fail until bootroom-core is
first published to crates.io (because the packaged tarball strips the path-
dep and the verify build only sees the `version = "0.1.0"` requirement). This
is the WR-01 manual-publish gap surfacing in the smoke; documented for the
maintainer doing the first publish.

## Skipped Issues

None. All 9 in-scope findings were fixed. The 4 INFO findings (IN-01..IN-04)
were out of scope per the directive; IN-02 was incidentally addressed inside
the CR-02 fix (redundant `cd /tmp` removed), and IN-03 was incidentally
addressed inside the CR-04 fix (dist-workspace.toml comment corrected).

## Notes for Verifier

- **CR-04 is a hand-edit to a cargo-dist-generated workflow.** Any future
  `dist generate --mode ci` run will silently revert the `host.needs:` and
  `host.if:` edits. The dist-workspace.toml `host-jobs` comment now warns
  about this; consider tracking an upstream cargo-dist issue to expose a
  `pre-host-jobs` / `gate-jobs` config knob that wires into `host` rather
  than `announce`.
- **WR-02 is also a hand-edit to a cargo-dist-generated workflow** for the
  same reason. Bumping cargo-dist past v0.31 may fix this upstream — verify
  before next `dist generate`.
- **WR-05 + CR-01 interact:** the smoke now exercises `cargo package`'s verify
  build, which compiles the published tarball against deps resolved from
  crates.io. Until bootroom-core is published once (manually), the bootroom
  verify build will fail. This is the WR-01 gap surfaced operationally.
- All fix commits were made on the temp branch `gsd-reviewfix/06-2834008`
  inside the isolated worktree `/tmp/sv-06-reviewfix-yYd8iH`. The cleanup
  tail fast-forwards `master` to capture them.

---

_Fixed: 2026-05-19T21:50:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
