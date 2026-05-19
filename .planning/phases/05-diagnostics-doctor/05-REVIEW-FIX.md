---
phase: 05-diagnostics-doctor
fixed_at: 2026-05-19T19:55:00Z
review_path: .planning/phases/05-diagnostics-doctor/05-REVIEW.md
iteration: 1
findings_in_scope: 7
fixed: 7
skipped: 0
status: all_fixed
---

# Phase 5: Code Review Fix Report

**Fixed at:** 2026-05-19T19:55:00Z
**Source review:** .planning/phases/05-diagnostics-doctor/05-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 7 (1 BLOCKER + 1 CRITICAL + 5 WARNING)
- Fixed: 7
- Skipped: 0
- INFO findings (4) intentionally out of scope per fix request

This run was a resumption of a prior interrupted fix session — the
preceding agent's worktree, two commits, and an uncommitted WR-04/WR-05
working tree were recovered via the recovery sentinel at
`.review-fix-recovery-pending.json` and the orphan worktree at
`/tmp/sv-05-reviewfix-OEhpbG`. The recovered uncommitted changes were
verified (`cargo test --workspace` green, BL-01 verification command
green), then committed as a single WR-04 + WR-05 cluster.

## Fixed Issues

### BL-01: `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` now fails to compile

**Files modified:** `crates/bootroom/build.rs`
**Commit:** `baa335e`
**Applied fix:** Hoisted the `BOOTROOM_GIT_SHA` capture above the
`BOOTROOM_SKIP_QEMU_ASSET_CHECK` early-return so the env var is always
emitted. The compile-time `env!()` lookups in `doctor_cmd.rs` now
resolve under the documented dev escape hatch.

**Verification:**
`BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo check -p bootroom` succeeds with
only the expected `cargo:warning` line; previously failed at
compile-time with the `env!` lookup.

### CR-01: `qemu-wasm-rev.txt` is not on the per-file `rerun-if-changed` list

**Files modified:** `crates/bootroom/build.rs`
**Commit:** `baa335e` (clustered with BL-01 — both touch `build.rs` in
the same minimal-context window and are operationally co-located)
**Applied fix:** Added `assets/qemu/qemu-wasm-rev.txt` to the `REQUIRED`
array. The per-file `rerun-if-changed` directive now fires when
`make qemu-assets` rewrites the file in place, re-triggering the
`include_dir!` embed.

### WR-01: `check_browser` runs `Command::output()` synchronously in an async function

**Files modified:** `crates/bootroom/src/doctor_cmd.rs`, `Cargo.toml`
**Commit:** `98ba0a7`
**Applied fix:** Switched `check_browser` to `tokio::process::Command`
and awaited `.output().await` so the `--version` probe no longer blocks
a tokio worker. The workspace `tokio` dep gained the `process` feature
to enable this. A hung Chromium binary now races against the doctor's
overall timeout rather than freezing the executor indefinitely.

### WR-02: `check_config` swallows reachable I/O errors as Fail

**Files modified:** `crates/bootroom/src/doctor_cmd.rs`
**Commit:** `98ba0a7`
**Applied fix:** Matched on `io::ErrorKind::PermissionDenied` and
`io::ErrorKind::IsADirectory` in the non-`NotFound` arm and appended a
short hint to the detail string. Operators now see "is a directory, not
a file" instead of a generic Fail when they accidentally pass
`--config .`.

### WR-03: Unit tests share hard-coded `/tmp/bootroom-doctor-*` paths

**Files modified:** `crates/bootroom/src/doctor_cmd.rs`
**Commit:** `98ba0a7`
**Applied fix:** Replaced four hard-coded shared `/tmp/bootroom-doctor-*`
paths with `tempfile::tempdir()` (already a dev-dep) in unit tests.
The `check_headers` placeholder kernel path was switched to a
pid-scoped `bootroom-doctor-noop-{pid}` name so parallel cargo test
runners get distinct paths without paying `tempdir` overhead on every
call.

### WR-04: No negative test for `check_headers` Fail-detail wording

**Files modified:** `crates/bootroom/src/doctor_cmd.rs`
**Commit:** `20eb467`
**Applied fix:** Factored the COOP/COEP assertion block out of
`check_headers` into a new `check_headers_against_router(app:
axum::Router) -> Check` helper. Added a `#[tokio::test]` that builds a
bare `Router::new().route("/", get(|| async { "ok" }))` (no COOP/COEP
middleware), calls the new helper, and pins the Fail-detail wording:
`expected COOP=same-origin, COEP=require-corp ... got COOP=None ...
COEP=None`. A future refactor that drops the cross-origin-isolation
layer will now trip a specific, named test rather than relying on the
green-path assertion alone.

### WR-05: `format_human` silently drops checks with unknown names

**Files modified:** `crates/bootroom/src/doctor_cmd.rs`
**Commit:** `20eb467` (clustered with WR-04 — same file, same area)
**Applied fix:** Implemented option (a) from the review — added a
catch-all `## Other` section to `format_human`. A `KNOWN_NAMES`
constant pins the six recognized check names; any check whose name is
not in that set renders via the shared `render_check_line` helper into
a `## Other` block before the `Overall:` line. A check that contributes
to `overall_failed` can no longer vanish from the rendered report.

Two regression tests pin behavior:
- A check named `future_check` surfaces under `## Other` with its
  detail visible.
- A report whose checks are all known names omits the `## Other`
  section entirely (no empty section).

## Verification

- `cargo test --workspace`: 124 lib tests + integration tests passing
  (full run green; only pre-existing clippy pedantic warnings in
  `tests/doctor_subcommand.rs` unrelated to these findings remain).
- `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo check -p bootroom`: succeeds
  (BL-01 specific verification per the fix request).
- Per-fix verification followed the 3-tier strategy: file re-read for
  every change (Tier 1) and `cargo check` / `cargo test` as Tier 2
  syntax/semantic check.

## Skipped Issues

None. All seven in-scope findings (BL-01, CR-01, WR-01, WR-02, WR-03,
WR-04, WR-05) were fixed and committed.

The four INFO-tier findings (IN-01 unicode hygiene, IN-02 unrelated
context note, IN-03 lifetime tightening on `Report.git_sha`,
IN-04 redundant `tower` dep stanza) were intentionally out of scope
per the fix request and remain in REVIEW.md for a separate pass.

---

_Fixed: 2026-05-19T19:55:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
