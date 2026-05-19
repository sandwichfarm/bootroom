---
phase: 06-distribution
reviewed: 2026-05-19T21:10:00Z
depth: standard
files_reviewed: 14
files_reviewed_list:
  - LICENSE-MIT
  - LICENSE-APACHE
  - Cargo.toml
  - crates/bootroom/Cargo.toml
  - crates/bootroom-core/Cargo.toml
  - dist-workspace.toml
  - deny.toml
  - .github/workflows/release.yml
  - .github/workflows/release-smoke.yml
  - .github/workflows/ci-deny.yml
  - Makefile
  - README.md
  - crates/bootroom/tests/install_smoke.rs
  - crates/bootroom/src/doctor_cmd.rs
findings:
  critical: 4
  warning: 5
  info: 4
  total: 13
status: issues_found
---

# Phase 6: Code Review Report

**Reviewed:** 2026-05-19T21:10:00Z
**Depth:** standard
**Files Reviewed:** 14
**Status:** issues_found

## Summary

Phase 6 wires the publish/release pipeline, license posture, and the
pre-publish smoke gate. The license files, deny.toml, and the include
allow-list metadata are clean. However the publish round-trip is
**non-functional** in its current shape: four independent blocker-grade
defects mean a `v0.1.0` tag would either fail at packaging time, fail at
install-smoke time, or ship a broken binary because the smoke does not
actually gate the GitHub Release. Each blocker was reproduced locally
where possible (e.g. `cargo package -p bootroom` and the shell glob).

The four blockers, in upstream-first order:

1. **No version requirement on the `bootroom-core` workspace dep**, so
   `cargo package -p bootroom` (release-smoke step 1) errors out before
   Docker ever runs.
2. **Shell-glob bug** in the release-smoke Docker step (`bootroom-*`
   matches both crate dirs) so `cargo install --path` receives two
   arguments and fails.
3. **install_smoke's `path_independence_qemu_wasm_rev_present`** test
   asserts `status == "pass"` on a check that is hard-coded to
   `CheckStatus::Info` in `doctor_cmd.rs::check_qemu_rev`.
4. **`custom-release-smoke` does not block `host`** in the generated
   release.yml — `host` runs the `gh release create` step in parallel
   with the smoke, so a failing smoke does not prevent the GitHub
   Release that `cargo binstall` and the curl installer consume.

These four together mean DIST-03 (crates.io publish + smoke) and DIST-05
(path-independence enforcement) are not actually enforced; a regression
in either today's surface would not be caught.

## Critical Issues

### CR-01: bootroom-core workspace dep has no version — `cargo package -p bootroom` fails before Docker runs

**File:** `Cargo.toml:39` (and `crates/bootroom/Cargo.toml:42`)
**Issue:** The workspace dependency declaration is path-only:
```toml
bootroom-core = { path = "crates/bootroom-core" }
```
and `crates/bootroom/Cargo.toml` consumes it as
`bootroom-core.workspace = true`. `cargo package -p bootroom
--allow-dirty --no-verify` errors out:
```
all dependencies must have a version requirement specified when packaging.
dependency `bootroom-core` does not specify a version
```
This is reproducible against cargo 1.90.0 (the MSRV-pinned toolchain).
`--no-verify` skips the build verify step but the manifest dep-spec
check runs unconditionally. The release-smoke workflow's first step
(`cargo package (bootroom-core + bootroom)`) will therefore fail on
every tag push and block the Docker install gate from ever running.
This also means `cargo publish -p bootroom` cannot succeed in any
follow-up plan that wires DIST-03's crates.io publish step.

The 06-02 SUMMARY's captured `cargo package --list -p bootroom` output
predates the workspace-dep-without-version state and was not re-run as
part of the final verification.

**Fix:**
```toml
# Cargo.toml
[workspace.dependencies]
bootroom-core = { path = "crates/bootroom-core", version = "0.1.0" }
```
Or pin the version in the consumer manifest:
```toml
# crates/bootroom/Cargo.toml
bootroom-core = { workspace = true, version = "0.1.0" }
```
Add a smoke step that runs `cargo package -p bootroom --no-verify`
(without `--allow-dirty`, so it also catches uncommitted-state
regressions) on every PR to fail loudly the next time the version pin
drifts.

---

### CR-02: Shell glob `/work/bootroom-*` expands to two paths — `cargo install --path` fails

**File:** `.github/workflows/release-smoke.yml:85, 107, 135`
**Issue:** Three Docker steps run:
```sh
tar -xzf /workspace/target/package/bootroom-core-*.crate -C /work
tar -xzf /workspace/target/package/bootroom-*.crate -C /work
cargo install --locked --path /work/bootroom-* --root /usr/local
```
After both tars, `/work/` contains BOTH `bootroom-0.1.0/` and
`bootroom-core-0.1.0/`. The glob `bootroom-*` in `--path /work/bootroom-*`
expands (verified locally) to:
```
/work/bootroom-0.1.0 /work/bootroom-core-0.1.0
```
`cargo install --path` accepts exactly one path argument; passing two
will fail with an arg-parse or "multiple paths" error. Even in the
unlikely event Cargo silently accepted both, the install target would
be non-deterministic (depends on glob sort order, which is
locale-sensitive).

Also: `tar -xzf /workspace/target/package/bootroom-*.crate` matches
BOTH `bootroom-0.1.0.crate` and `bootroom-core-0.1.0.crate`. The second
tar is a redundant extraction of bootroom-core into `/work`, which is
harmless but obscures intent (the explicit `bootroom-core-*.crate` tar
on the line above already covers it). Both can be tightened to a
literal version pin or an explicit directory variable.

**Fix:**
```sh
BOOTROOM_VERSION=$(jq -r '.crates[].version' /workspace/target/package/bootroom-0.1.0/.cargo_vcs_info.json 2>/dev/null || echo "0.1.0")
tar -xzf /workspace/target/package/bootroom-core-${BOOTROOM_VERSION}.crate -C /work
tar -xzf /workspace/target/package/bootroom-${BOOTROOM_VERSION}.crate -C /work
cargo install --locked --path /work/bootroom-${BOOTROOM_VERSION} --root /usr/local
```
Or simpler: extract the bootroom crate into a known sub-path and pin
that sub-path literally. Apply to all three Docker steps (lines 70-91,
96-111, 124-141).

---

### CR-03: `path_independence_qemu_wasm_rev_present` asserts `status == "pass"` on a check that is hard-coded to `Info`

**File:** `crates/bootroom/tests/install_smoke.rs:116-120` + `crates/bootroom/src/doctor_cmd.rs:140-152`
**Issue:** `check_qemu_rev` always returns `CheckStatus::Info`:
```rust
fn check_qemu_rev() -> Check {
    let rev = crate::embed::QEMU.get_file("qemu-wasm-rev.txt")
        // ...
        .unwrap_or("unknown");
    Check { name: "qemu_wasm_rev".to_string(), status: CheckStatus::Info, detail: ... }
}
```
`CheckStatus` serializes as lowercase (`"pass" | "fail" | "info"`). The
install-smoke test then asserts:
```rust
assert_eq!(rev_status, "pass", "...");
```
This is structurally unreachable today. The release-smoke "DIST-05
path-independence check" step (release-smoke.yml:124-141) re-runs ONLY
this test, so it will fail every tag push. Even if CR-04 is also
fixed so the smoke actually gates the publish, this test would
permanently block every release.

The 06-08 SUMMARY's decision note ("top-level `qemu_wasm_rev` JSON check
is wrapped in `if let Some(...)`") addressed the top-level field but
not the in-checks-array status assertion.

**Fix:** Either
(a) change the assertion to match doctor's documented contract:
```rust
assert_eq!(
    rev_status, "info",
    "qemu_wasm_rev status must be info ..."
);
// And assert the detail string contains a non-"unknown" rev:
let detail = rev_check.get("detail").and_then(|v| v.as_str()).unwrap_or("");
assert!(
    detail.contains("rev ") && !detail.contains("rev unknown"),
    "qemu_wasm_rev detail indicates the file was not embedded: {detail}"
);
```
(b) promote `check_qemu_rev` to return `Pass` when the file is non-empty
and non-"unknown", and `Fail` when the embedded file resolves to
"unknown". This is a behavior change to `doctor_cmd.rs` and would also
need an update to the doctor schema docs + the 05-phase tests pinning
the Info status (`doctor_cmd.rs:802-805`). (a) is the lower-risk fix.

---

### CR-04: `custom-release-smoke` does not gate `host` — the smoke does not block the GitHub Release

**File:** `.github/workflows/release.yml:215-219, 281-289`
**Issue:** The `host` job (which runs `gh release create` at L279) lists
its `needs:` as `[plan, build-local-artifacts, build-global-artifacts]`.
`custom-release-smoke` is NOT in that list. It is only a dependency of
`announce` (L291-295). cargo-dist's `host-jobs` config inserted the
smoke job and its `announce`-gate, but did NOT insert a `needs:` edge
from `host` to `custom-release-smoke`.

Consequence: when a tag is pushed, `host` and `custom-release-smoke`
run in parallel. If the smoke fails, the `gh release create` step has
already executed and the prebuilt artifacts have been published to the
GitHub Release that `cargo binstall` (DIST-06) and the curl installer
(`bootroom-installer.sh`) consume. Only `announce` is skipped — which
does nothing operational (review L291-307: it does a checkout and
exits).

The 06-06 SUMMARY (L45) acknowledges this ("Accept that
custom-release-smoke runs in parallel with host") and rationalizes
that it's fine because cargo-dist v0.31 doesn't auto-publish to
crates.io. That rationale ignores the GitHub-Release-artifacts surface,
which is exactly the DIST-04 + DIST-06 install path. **A broken
binary will reach `cargo binstall` users despite a smoke failure.**

**Fix:** Either
(a) hand-add `custom-release-smoke` to `host.needs:` in release.yml
(violates the "no hand-editing" rule from 06-03 SUMMARY but is
mechanically simple), or
(b) move the smoke into a `pre-host-job`/`publish-job` slot that
cargo-dist's generator wires before `host` — check cargo-dist v0.31
config keys (`publish-jobs`?) and re-run `dist generate --mode ci`
to regenerate release.yml.
(c) If neither cargo-dist hook gates `host`, add a small explicit
`gate` job between `build-*` and `host`:
```yaml
gate:
  needs: [plan, build-local-artifacts, build-global-artifacts, custom-release-smoke]
  if: ${{ needs.custom-release-smoke.result == 'success' }}
  runs-on: ubuntu-latest
  steps:
    - run: echo gate green
host:
  needs: [plan, build-local-artifacts, build-global-artifacts, gate]
```
Verify with a deliberately-failing smoke (e.g. `exit 1` injected in the
Docker step) that `gh release create` does NOT execute.

## Warnings

### WR-01: README documents `cargo install --locked bootroom` as the primary install path, but bootroom is not yet published to crates.io

**File:** `README.md:25-29`
**Issue:** The install matrix presents `cargo install --locked bootroom`
as the primary command. Per 06-03 SUMMARY (Deviation #3 +
requirements-completed note at L44), DIST-03 is INCOMPLETE: cargo-dist
v0.31 does not emit a `cargo publish` step and Phase 6 did not add a
separate publish workflow. Until someone manually runs `cargo publish`
or a follow-up plan wires the publish step, end users running the
documented primary command will receive `error: could not find
'bootroom' in registry 'crates-io'`. The secondary (`cargo binstall`)
and tertiary (curl installer) paths depend on a GitHub Release
existing, which presupposes a tag-triggered release.yml run completing
green — also a future state.

This is a documentation-vs-reality gap, not a bug in the code itself,
but it directly contradicts the phase goal ("kernel project on any
supported platform installs `bootroom` in one step").

**Fix:** Either land the missing publish step (separate follow-up plan)
or amend README to caveat that the install matrix becomes available
after the first published release. Minimum:
```markdown
> **Status:** bootroom is pre-1.0. The install matrix below becomes
> active after the first tagged release. Until then, build from source
> (`make install` from a clone).
```

---

### WR-02: `actions/upload-artifact@v6` / `actions/download-artifact@v7` are version skews from cargo-dist's generator

**File:** `.github/workflows/release.yml:69, 85, 134, 161, 184, 191, 207, 232, 240, 253, 260`
**Issue:** The cargo-dist-generated workflow uses
`actions/upload-artifact@v6` and `actions/download-artifact@v7`. As of
the project's current date (2026-05-19), the latest stable versions
are v4 (upload) and v4 (download); v6/v7 are not published releases
from the actions/upload-artifact and actions/download-artifact repos.
This will cause every release run to fail at the "Setup Action" step
with `Unable to resolve action`.

Verify against `https://github.com/actions/upload-artifact/releases`
and `https://github.com/actions/download-artifact/releases`. If
cargo-dist v0.31 truly emits v6/v7 references, that's a cargo-dist
upstream bug to file; locally pin to v4 in the meantime (which means
hand-editing release.yml, violating the no-hand-edit invariant — file
the upstream issue first).

**Fix:**
```yaml
- uses: actions/upload-artifact@v4
- uses: actions/download-artifact@v4
```
If cargo-dist regenerates these on next `dist generate`, also pin
cargo-dist to a fixed-up version or override via a small post-generate
patch step.

---

### WR-03: LICENSE-APACHE leaves the boilerplate `Copyright [yyyy] [name of copyright owner]` placeholder unfilled

**File:** `LICENSE-APACHE` (tail of file)
**Issue:** The file ends with the canonical Apache 2.0 boilerplate
"APPENDIX: How to apply the Apache License to your work" which
contains `Copyright [yyyy] [name of copyright owner]`. Apache's
canonical template documents this is meant as instructions, not a
field-to-fill-in inside the LICENSE itself — but for projects of any
notability the convention is to either (a) replace it with a real
copyright line for the project, or (b) add a sibling `NOTICE` file
naming the copyright owner. LICENSE-MIT does fill in the copyright
("Copyright (c) 2026 sandwich ...").

This is a minor licensing-hygiene concern: a strict license-bot may
flag the unfilled `[yyyy] [name]` as a missing attribution, and SPDX
scanners running on the published .crate may report the file as
"Apache-2.0 with template placeholders".

**Fix:** Either add a NOTICE file at the repo root naming the project
authors, or append a real copyright line below the appendix (e.g.
`Copyright 2026 sandwich <sandwich.farm@protonmail.com>`).

---

### WR-04: deny.toml is missing an explicit `version = 2` declaration and `private` block

**File:** `deny.toml:38, 110`
**Issue:** cargo-deny v0.16+ defaults to schema v2 (the file's comment
at L17 acknowledges this), but the v2 schema documentation recommends
declaring `version = 2` explicitly at the top of `[licenses]` and
`[advisories]` to defend against a future v3 default flip. The file
also has no `[bans].deny` entries enforcing the project's deferred
threats (e.g. banning `openssl-sys` in favor of `rustls`, which the
README does not yet promise but the codebase implies).

Less critical: `[advisories].yanked = "warn"` lets a yanked transitive
dep ship in a release. The comment notes this is intentional
("first-run CI from failing"), but Phase 6 is the right place to
promote to `deny` since the baseline cargo-deny run is presumably
already green.

**Fix:**
```toml
[licenses]
version = 2
# ...

[advisories]
version = 2
yanked = "deny"
# ...
```
Run `cargo deny check` after the change to confirm no transitive
yanked deps remain.

---

### WR-05: `cargo package` smoke uses `--allow-dirty --no-verify`, hiding two classes of regression

**File:** `.github/workflows/release-smoke.yml:56-58`
**Issue:**
```sh
cargo package -p bootroom-core --allow-dirty --no-verify
cargo package -p bootroom --allow-dirty --no-verify
```
- `--allow-dirty` lets the workflow run on an uncommitted-state
  checkout, defeating the "what gets published is what's committed"
  invariant. On a tag push the working tree is clean, so this flag is
  unnecessary; on a `workflow_dispatch` manual re-run it would hide a
  dirty state.
- `--no-verify` skips the in-package `cargo build` verify step. This
  is the only place that exercises the path-stripped
  `bootroom-core = { workspace = true }` dep after Cargo rewrites it
  to a registry version on publish (per the cargo docs message
  reproduced under CR-01). Skipping verify means the smoke does NOT
  catch the case where a workspace-level dep cannot be resolved from
  crates.io (e.g. because bootroom-core hasn't been published yet — see
  WR-01 / CR-01).

**Fix:**
```sh
cargo package -p bootroom-core
cargo package -p bootroom
```
If verify fails because bootroom-core is not on crates.io, the right
fix is to publish bootroom-core first or use a local registry shim
(per 06-03's "release-smoke uses the freshly-built local artifacts —
not a hot crates.io round trip" from CONTEXT.md L113). Either way,
hiding the failure with `--no-verify` is the wrong choice.

## Info

### IN-01: release.yml triggers on `pull_request` and runs the full 4-target build matrix on every PR

**File:** `.github/workflows/release.yml:42-45`
**Issue:** The `on:` trigger fires on `pull_request`. Combined with the
build-matrix expansion (4 targets via cargo-dist), every PR runs a
full cross-platform build matrix. That is a meaningful CI-minute
cost for a project that already has separate CI workflows (ci-deny,
etc.). The trigger is correct in the sense that `publishing:
${{ !github.event.pull_request }}` correctly gates the publish path,
but the build cost is not zero.

**Fix:** Consider scoping the PR trigger to a smaller surface (e.g.
`dist plan` only, no `dist build`) or restricting to PRs that touch
`dist-workspace.toml`, `Cargo.toml`, or the workflows themselves.
cargo-dist exposes a `pr-run-mode` config key — set it to `plan` to
skip the build matrix on PRs entirely.

---

### IN-02: `cd /tmp` after `cargo install` is no-op — the shell is already at WORKDIR `/tmp`

**File:** `.github/workflows/release-smoke.yml:86`
**Issue:**
```sh
docker run ... -w /tmp ...
sh -euxc '
  ...
  cargo install ...
  cd /tmp                          # <- no-op, WORKDIR is already /tmp
  bootroom --version
  ...
```
The `-w /tmp` flag on `docker run` already sets the container CWD to
`/tmp`. The `cd /tmp` inside the script is redundant. Harmless, but
suggests intent drift — someone might assume CWD has changed earlier
in the script (it hasn't) and reach for that as a path-independence
signal. Either delete the `cd` or move the WORKDIR setup into the
shell script so the intent is collocated.

**Fix:**
```sh
# Remove the redundant `cd /tmp` at line 86.
```

---

### IN-03: `dist-workspace.toml` comment refers to `host-jobs` semantics that the generated workflow does not enforce

**File:** `dist-workspace.toml:20-26`
**Issue:** The comment claims:
> cargo-dist will inject a `needs:` reference into the generated
> release.yml so the `host` job (which performs `gh release create`)
> only proceeds when the smoke is green.

The generated workflow does NOT do this — see CR-04. The comment is
aspirational rather than descriptive. Either fix the wiring (CR-04)
or update the comment to match reality:
```toml
# The smoke gates the `announce` job, not `host`. cargo-dist v0.31's
# host-jobs hook wires `custom-release-smoke` -> `announce`; `host`
# (gh release create) runs in parallel with the smoke. To gate the
# GitHub Release, add an explicit `gate` job between build and host
# (see 06-REVIEW.md CR-04).
```

---

### IN-04: Makefile `help` and `release` targets do not mention the `cargo-dist` install prerequisite or the manual-publish gap

**File:** `Makefile:19-25, 92-104`
**Issue:** `make help` says:
```
release           cargo dist build --artifacts=all (local cross-platform smoke; NOT a publish)
```
The doc comment above the target (L92-102) does note the cargo-dist
install requirement, but `make help` itself does not — a user running
`make help` then `make release` for the first time gets a `cargo:
no such subcommand: dist` error with no guidance. Phase 6 also did
not wire a `make publish` target despite documenting the install
matrix as user-facing surface — a maintainer running a release has no
single-command path beyond a manual `git tag` + push.

**Fix:** Augment `make help` to:
```
release           cargo dist build --artifacts=all (requires cargo install cargo-dist; NOT a publish)
```
and consider adding `make tag VERSION=...` that does
`git tag vX.Y.Z && git push origin vX.Y.Z` after verifying the
working tree is clean and Cargo.toml matches.

---

_Reviewed: 2026-05-19T21:10:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
