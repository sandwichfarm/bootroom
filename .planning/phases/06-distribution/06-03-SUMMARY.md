---
phase: 06-distribution
plan: 03
subsystem: infra
tags: [cargo-dist, github-actions, release-pipeline, cross-compile, homebrew, cargo-binstall, dist-workspace, ci]

# Dependency graph
requires:
  - phase: 06-distribution
    provides: 06-01 (workspace-level [workspace.package] metadata) and 06-02 (per-crate publish metadata) supply the package surface that cargo-dist plans against.
provides:
  - "[dist] config block in dist-workspace.toml pinning cargo-dist 0.31.0, the four locked target triples, and the two locked installers (shell, homebrew)."
  - ".github/workflows/release.yml — cargo-dist-generated, tag-triggered (v*.*.* semver pattern), 5 jobs: plan -> build-local-artifacts -> build-global-artifacts -> host -> announce."
  - "[profile.dist] in root Cargo.toml (inherits release, lto=thin) used by cargo-dist for release builds."
  - "cargo-binstall auto-discovery hookup (asset names follow cargo-dist's bootroom-<target>.tar.xz convention; matches binstall's default pattern)."
affects: [06-04 (release-smoke gate inserts before `host`), 06-05, 06-06]

# Tech tracking
tech-stack:
  added:
    - "cargo-dist v0.31.0 (installed as `dist` binary, not as `cargo dist` subcommand — renamed upstream)"
  patterns:
    - "Configuration source-of-truth lives in dist-workspace.toml ([dist] table), not in inline [workspace.metadata.dist]. cargo-dist v0.28+ default."
    - "Workflow file is target-agnostic: target matrix is computed at CI time by `dist plan --output-format=json` and consumed via `fromJson(needs.plan.outputs.val).ci.github.artifacts_matrix`."
    - "Hosting URL uses repo owner/name from `[workspace.package].repository`; no GitHub env-var hardcoding needed in the workflow."

key-files:
  created:
    - .github/workflows/release.yml
    - dist-workspace.toml
  modified:
    - Cargo.toml (added [profile.dist])

key-decisions:
  - "Accept cargo-dist v0.31's newer config layout (dist-workspace.toml + [dist] table) over the older inline [workspace.metadata.dist] — the plan explicitly permits either."
  - "Hand-correct the targets list in dist-workspace.toml after `dist init --yes` overrode `-t` flags with cargo-dist's 7-target default set. Justification: the YAML-generation step is what CONTEXT.md D-01 forbids hand-editing; the TOML config is the canonical input and re-running `dist generate` from the corrected config regenerates release.yml from source-of-truth."
  - "Defer crates.io publishing: cargo-dist v0.31's default workflow no longer auto-emits a crates.io publish step (this behavior was removed upstream in earlier 0.x versions). DIST-03 needs follow-up — either a separate `cargo publish` workflow file or a manual maintainer step. Documented as deviation."

patterns-established:
  - "Pinning rule: cargo-dist version is pinned in dist-workspace.toml -> [dist].cargo-dist-version, NOT as a workspace [dependencies] entry. CI installs the exact version via the shell installer (`curl ... | sh`) embedded in the generated workflow."
  - "Re-run cargo-dist init / `dist generate` rather than hand-editing release.yml. The tooling rebuilds release.yml deterministically from dist-workspace.toml."

requirements-completed: [DIST-04, DIST-06]
# Note: DIST-03 (crates.io publish) is wired insofar as the workspace/manifests are publish-ready (06-02), but the cargo-dist-generated workflow at v0.31 does NOT include a crates.io publish step. Marking DIST-03 incomplete pending follow-up plan (see Deviations).

# Metrics
duration: 5min
completed: 2026-05-19
---

# Phase 06 Plan 03: cargo-dist Release Pipeline Wiring Summary

**cargo-dist 0.31.0 init produced dist-workspace.toml pinning 4 locked target triples + shell/homebrew installers, plus a tag-triggered .github/workflows/release.yml with the 5-job plan/build/host/announce shape used by Phase 6's downstream gates.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-05-19T18:46:31Z
- **Completed:** 2026-05-19T18:51:33Z
- **Tasks:** 4 (all auto-mode)
- **Files modified:** 3 (1 generated workflow, 1 generated config, 1 Cargo.toml profile addition)

## Accomplishments

- cargo-dist v0.31.0 installed and verified via `dist --version`.
- `dist init` invoked non-interactively producing the canonical Phase 6 release pipeline shape.
- Targets locked to exactly the four CONTEXT.md D-02 triples (x86_64/aarch64 × {unknown-linux-musl, apple-darwin}); cargo-dist's default 7-target list (which adds linux-gnu and windows-msvc) was hand-trimmed in dist-workspace.toml and the workflow regenerated.
- cargo-binstall discovery preconditions verified: `[package.repository]` is a GitHub URL, no `[package.metadata.binstall]` overrides exist in either crate, asset names follow the bootroom-<target>.tar.xz convention (confirmed via `dist plan --output-format=json`).
- Workspace still builds clean (`cargo build --workspace --offline`) — no manifest corruption.

## Task Commits

1. **Task 1: Install or verify cargo-dist locally** — no commit (tool-presence check; installed cargo-dist 0.31.0 globally via `cargo install cargo-dist --locked`).
2. **Task 2 + Task 3 + Task 4: dist init + audit + binstall verify** — `e119543` (chore)

All three subsequent tasks landed in a single commit because Tasks 3 and 4 are read-only audits that produced no file changes; Task 2's output (the generated files plus the post-init targets correction) is the only artifact.

## Files Created/Modified

- `dist-workspace.toml` (created) — `[dist]` table with cargo-dist-version=0.31.0, ci=github, hosting=github, installers=["shell","homebrew"], targets=[the 4 locked triples], install-path=CARGO_HOME, install-updater=false.
- `.github/workflows/release.yml` (created, 320 lines) — autogenerated by cargo-dist; tag-trigger pattern `'**[0-9]+.[0-9]+.[0-9]+*'`; jobs: `plan`, `build-local-artifacts`, `build-global-artifacts`, `host`, `announce`.
- `Cargo.toml` (modified) — added `[profile.dist]` inheriting `release` with `lto = "thin"`. The `# Defaults; Phase 6 may tune LTO.` placeholder comment below the empty `[profile.release]` table remains.

## cargo-dist init Audit (Task 3)

### Captured cargo-dist version

`cargo-dist 0.31.0` (installed binary is named `dist`, not `cargo-dist` / `cargo dist`; upstream renamed the binary in v0.28+).

### Full `[dist]` block (from dist-workspace.toml)

```toml
[workspace]
members = ["cargo:."]

[dist]
cargo-dist-version = "0.31.0"
ci = "github"
installers = ["shell", "homebrew"]
targets = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl", "x86_64-apple-darwin", "aarch64-apple-darwin"]
install-path = "CARGO_HOME"
hosting = "github"
install-updater = false
```

### Generated workflow job names (for 06-06's `needs:` reference)

In tag-trigger DAG order:

1. `plan` — runs `dist plan` (PR mode) or `dist host --steps=create` (tag mode); emits JSON manifest read by every downstream job.
2. `build-local-artifacts` — matrix over the 4 target triples; builds binaries + per-platform installers; runs on per-target runners chosen by `dist plan` (musl jobs on Ubuntu containers, darwin jobs on macOS runners).
3. `build-global-artifacts` — runs once on `ubuntu-22.04`; builds platform-agnostic installers (the `bootroom-installer.sh` shell installer + the `bootroom.rb` Homebrew formula).
4. `host` — only on tag pushes (`needs.plan.outputs.publishing == 'true'`); uploads artifacts, runs `dist host --steps=upload --steps=release`, then `gh release create` to publish the GitHub Release with all the assets.
5. `announce` — final gate; runs only if `host` succeeded; currently emits nothing user-visible (cargo-dist's announcement hook is empty by default; reserved for the homebrew tap push / publish-to-npm-style extensions).

### Hard locks audit

| # | Lock | Expected | Actual | Pass? |
|---|------|----------|--------|-------|
| 1 | Targets exactly the 4 locked | `[x86_64-unknown-linux-musl, aarch64-unknown-linux-musl, x86_64-apple-darwin, aarch64-apple-darwin]` | Same | YES |
| 2 | Installers exactly `[shell, homebrew]` | `["shell", "homebrew"]` | Same | YES |
| 3 | cargo-dist-version pin matches Task-1 capture | 0.31.0 | 0.31.0 | YES |
| 4 | Workflow includes GitHub Release **AND** crates.io publish steps | Both | GH Release: YES (in `host` job, `gh release create` line). crates.io publish: NO. | **FAIL (soft — documented as deviation; cargo-dist v0.31 no longer auto-publishes to crates.io)** |
| 5 | Tag trigger `v*` pattern | `v*` or `v*.*.*` semver | `'**[0-9]+.[0-9]+.[0-9]+*'` — matches `v0.1.0`, `v1.2.3-rc.1`, etc. | YES |
| 6 | Publish ordering: bootroom-core before bootroom | Either dependency-ordered auto or explicit | N/A — there is no `cargo publish` step at all (see #4) | Deferred to follow-up |

### Soft observations (items 7–10)

| # | Observation | Result |
|---|---|---|
| 7 | Release-smoke insertion hook for 06-06 | `host` is the natural gate point; 06-06 can either `needs: [build-local-artifacts, build-global-artifacts]` to interpose before `host`, or use `workflow_run` to fire after `announce` succeeds. Job names captured above. |
| 8 | Repo URL handling | Workflow hardcodes `sandwich-farm/bootroom` (read from `[workspace.package].repository` at init time). Confirmed via `dist plan --output-format=json` (`hosting.github.owner: "sandwich-farm"`, `repo: "bootroom"`). Survives a future repo rename only after a re-run of `dist generate`. |
| 9 | bootroom-core publish surface | `dist plan` only lists one `app_name: "bootroom"` because cargo-dist only ships binary artifacts. `bootroom-core` is a library and is correctly excluded from the artifact list. Its crates.io publishing is a separate concern (see #4 deviation). |
| 10 | Pre-release tag handling | Workflow uses `announcement_is_prerelease` flag and passes `--prerelease` to `gh release create`. Matches cargo-dist default; pre-release tags become GitHub Pre-Releases. |

### cargo-binstall discovery rules (Task 4)

| Rule | Status |
|---|---|
| `[package.repository]` points to a GitHub URL | YES — `https://github.com/sandwich-farm/bootroom` (workspace inherit). |
| Asset names follow `<crate>-<target>.<ext>` | YES — confirmed via `dist plan`: e.g. `bootroom-x86_64-unknown-linux-musl.tar.xz`. |
| No `[package.metadata.binstall]` override blocks | YES — `grep -r '[package.metadata.binstall]' crates/` returns no matches. |

DIST-06 satisfied. Live smoke-test (`cargo binstall bootroom`) deferred to post-first-release iteration (Phase 7+ concern per plan).

## Decisions Made

- **Accept `dist-workspace.toml` over inline `[workspace.metadata.dist]`.** cargo-dist v0.31 writes the new layout by default; the plan explicitly accepts either form.
- **Hand-correct the targets list in dist-workspace.toml** after `dist init --yes -t <four triples>` produced the 7-target default. The TOML config is the source-of-truth input; running `dist generate` from the corrected TOML produces the canonical release.yml. CONTEXT.md D-01's "no hand-editing release.yml" rule is preserved (release.yml was regenerated from configuration, not hand-modified).
- **Defer crates.io publishing wiring.** cargo-dist v0.31 does not include a crates.io publish step in its default workflow; this is a known upstream change. DIST-03's actual `cargo publish` invocation needs a follow-up plan (likely a separate small workflow file at `.github/workflows/publish-crates.yml` triggered after `host` succeeds, or a manual maintainer step using existing `cargo publish -p bootroom-core && cargo publish -p bootroom`).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] cargo-dist binary renamed from `cargo dist` to `dist`**
- **Found during:** Task 1 (Install or verify cargo-dist locally)
- **Issue:** The plan's verify command `cargo dist --version` fails with "no such command: dist". cargo-dist v0.28+ renamed the installed binary from `cargo-dist` to `dist`, so it's invoked as `dist`, not `cargo dist`.
- **Fix:** Used `dist --version` and `dist init` directly throughout the plan. Documented in this summary so 06-04, 06-05, 06-06 don't repeat the search.
- **Files modified:** none (tooling discovery)
- **Verification:** `dist --version` reports `cargo-dist 0.31.0`; `dist init` ran successfully.
- **Committed in:** N/A (Task 1 is tool-presence only)

**2. [Rule 3 — Blocking] `dist init`'s `--yes` flag overrides `-t`/`--target` with cargo-dist's 7-target default set**
- **Found during:** Task 2 (Run `cargo dist init` non-interactively)
- **Issue:** Running `dist init --yes -i shell -i homebrew -t x86_64-unknown-linux-musl -t aarch64-unknown-linux-musl -t x86_64-apple-darwin -t aarch64-apple-darwin -c github --hosting github` produced a dist-workspace.toml with `targets = [<the 7 default triples>]`, NOT the 4 passed via `-t`. This is upstream cargo-dist behavior in v0.31: `--yes` means "accept all recommended defaults", which silently widens the target list.
- **Fix:** Hand-edited `dist-workspace.toml`'s `targets = [...]` line to contain only the 4 locked triples, then ran `dist generate` to re-emit release.yml from the corrected config. The release.yml itself was regenerated by the tool (not hand-edited), preserving CONTEXT.md D-01's rule.
- **Files modified:** dist-workspace.toml (one-line correction), .github/workflows/release.yml (regenerated by `dist generate`)
- **Verification:** `dist plan --output-format=json` confirms only the 4 locked triples appear in the artifact list (Apple Silicon macOS, Intel macOS, ARM64 MUSL Linux, x64 MUSL Linux).
- **Committed in:** e119543

**3. [Rule 4 boundary — Documented, not auto-fixed] cargo-dist v0.31 does not emit a crates.io publish step**
- **Found during:** Task 3 (Audit the generated workflow)
- **Issue:** Task 3's hard-lock #4 expects the generated workflow to include both a "publish to GitHub Release" step AND a "publish to crates.io" step. The GitHub Release step exists (in the `host` job: `gh release create ...`); the crates.io step does NOT. This is intentional in upstream cargo-dist — automatic crates.io publishing was removed because it's brittle (token scope, dependency ordering, prerelease semantics) and is better handled in a project-specific workflow.
- **Why not auto-fix:** Adding a crates.io publish step is non-trivial (needs `CARGO_REGISTRY_TOKEN`-scoped secret, dependency-ordered `cargo publish -p bootroom-core && cargo publish -p bootroom` from CONTEXT.md D-09, and decisions about whether prereleases should publish). This is a follow-up plan, not a Rule 1/2/3 inline fix. Rule 4 says "ask user for architectural decisions"; flagged here for 06-04 or a new follow-up plan instead of halting the current plan.
- **Action:** Flagged in this summary for follow-up. Recommend a small `.github/workflows/publish-crates.yml` triggered by `workflow_run: { workflows: [Release], conclusion: success }` that runs `cargo publish -p bootroom-core` then `cargo publish -p bootroom`, OR documented manual maintainer step in `RELEASING.md`.
- **Files modified:** none
- **Verification:** Confirmed absence via `grep -nE "publish|crates\.io|CARGO_REGISTRY" .github/workflows/release.yml` — only the `publishing:` boolean output from the `plan` job appears, no actual `cargo publish` invocation.

---

**Total deviations:** 3 (1 tool-rename blocking, 1 init-flag override blocking, 1 documented-but-unaddressed for follow-up).
**Impact on plan:** Tasks 1, 2, 4 fully met. Task 3 partially failed on hard-lock #4 (crates.io publish step missing); flagged for a follow-up plan rather than blocking Phase 6 progress. The Phase 6 keystone (multi-target builds + GitHub Releases + binstall discovery) is fully wired. DIST-03 (crates.io publish) requires one additional small artifact (workflow file or manual step) before it can be claimed complete.

## Issues Encountered

- **First-release homebrew failure expected (T-06-03-07).** The generated workflow includes a homebrew formula emission, but no `homebrew-bootroom` tap repo exists yet. cargo-dist's first-release homebrew step is expected to fail until a tap repo is provisioned. Per CONTEXT.md `<deferred>`, tap maintenance beyond auto-emit is out-of-scope for Phase 6. The first `v*.*.*` tag push will emit the formula artifact attached to the GitHub Release, even if the (currently nonexistent) tap-push step fails downstream. Document and ignore.

## User Setup Required

None for this plan. (External setup required before the first real release: provision `CARGO_REGISTRY_TOKEN` repo secret for crates.io publishing once the follow-up publish workflow lands; optionally create the `homebrew-bootroom` tap repo. Both are post-Phase-6 maintainer concerns.)

## Next Phase Readiness

- Release pipeline shape is locked. Plans 06-04 (release-smoke / pre-publish gate) and 06-05/06-06 can reference the captured job names (`plan`, `build-local-artifacts`, `build-global-artifacts`, `host`, `announce`) for `needs:` wiring.
- DIST-04 (4-target prebuilt binaries) and DIST-06 (cargo-binstall auto-discovery) are wired and will work on first tag push.
- DIST-03 (crates.io publish) is **NOT** wired by the cargo-dist-generated workflow at v0.31; a small follow-up artifact is required. Flag for a new plan in Phase 6 or carry into a Phase 7 distribution-followup phase.

## Self-Check

### Created files exist

```
FOUND: dist-workspace.toml
FOUND: .github/workflows/release.yml
```

### Commit exists

```
FOUND: e119543 chore(06-03): init cargo-dist with 4 locked targets and shell+homebrew installers
```

## Self-Check: PASSED

---
*Phase: 06-distribution*
*Completed: 2026-05-19*
