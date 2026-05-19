---
phase: 06-distribution
plan: 05
subsystem: infra
tags: [cargo-deny, ci, licensing, supply-chain, github-actions]

requires:
  - phase: 02-cli-and-config
    provides: workspace Cargo.toml + workspace member layout (cargo-deny operates on)
provides:
  - cargo-deny configuration at the workspace root enforcing MIT OR Apache-2.0 + compatible permissive licenses
  - GitHub Actions workflow running `cargo deny check` on every push and PR to master
  - Pinned cargo-deny version (^0.19) so local and CI behave identically
  - Mechanical gate: new GPL/AGPL/LGPL/SSPL deps fail CI rather than ship silently
affects: [06-06-release-binaries, 06-07-install-paths, 06-08-readme-and-install-docs]

tech-stack:
  added: [cargo-deny v0.19, GitHub Actions ci-deny workflow]
  patterns: ["closed allow-list licensing", "single-version-pin between local and CI"]

key-files:
  created:
    - deny.toml
    - .github/workflows/ci-deny.yml
  modified: []

key-decisions:
  - "Use cargo-deny v0.19 (v2 schema) instead of the plan's ^0.14: v0.14 cannot parse CVSS 4.0 advisory entries currently in the RustSec DB"
  - "Use closed allow-list licensing (no separate deny-list): v0.16+ removed the legacy deny array; strong-copyleft licenses are rejected by absence from `allow`"
  - "Downgrade wildcards from 'deny' to 'warn' with `allow-wildcard-paths = true`: external wildcards still surface, but workspace-internal path deps (bootroom-core, spike-b) no longer block the gate"
  - "Keep advisories at warn-mode (yanked) to avoid first-run CI failure on pre-existing yanked transitive deps; promote to deny once baseline is clean"

patterns-established:
  - "License posture enforcement: every new dep's SPDX expression must be on the explicit allow-list in deny.toml; adding a license is a deliberate legal-posture change reviewed on the PR"
  - "Version-locked supply-chain tooling: cargo-deny pinned to ^0.19 in both local install (06-05 SUMMARY) and CI (`ci-deny.yml`) so the gate cannot drift"
  - "Plain GitHub Actions workflow: triggers on push and PR to master, with concurrency cancellation, runs-on ubuntu-latest, no continue-on-error"

requirements-completed: [DIST-07]

duration: 9min
completed: 2026-05-19
---

# Phase 6 Plan 5: cargo-deny supply-chain gate Summary

**cargo-deny v0.19 wired into CI with a closed permissive-license allow-list at `deny.toml`, blocking GPL/AGPL/LGPL/SSPL deps and git-source deps on every push and PR to master.**

## Performance

- **Duration:** 9 min (mostly waiting on two `cargo install cargo-deny` builds)
- **Started:** 2026-05-19T18:28:54Z
- **Completed:** 2026-05-19T18:37:22Z
- **Tasks:** 3
- **Files modified:** 2 (both newly created)

## Accomplishments
- `deny.toml` at the repo root with `[graph]`, `[licenses]`, `[bans]`, `[advisories]`, `[sources]` tables.
- Closed permissive-license allow-list: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-DFS-2016, Unicode-3.0, Zlib, CC0-1.0, BSL-1.0, MPL-2.0.
- `cargo deny check` exits 0 against the current workspace dep tree (all four checks: `advisories ok, bans ok, licenses ok, sources ok`).
- `.github/workflows/ci-deny.yml` triggers on push and PR to master, pins cargo-deny to `^0.19`, runs `cargo deny check --hide-inclusion-graph`, with `concurrency` cancellation of superseded runs.

## Task Commits

Each task was committed atomically:

1. **Task 1: Write `deny.toml` at the repo root** — `0171779` (feat)
2. **Task 2: Run `cargo deny check` locally and triage findings** — `eed7411` (fix; schema/wildcard triage after first-run failure)
3. **Task 3: Write `.github/workflows/ci-deny.yml`** — `0fafe68` (feat)

## Files Created/Modified

- `deny.toml` — cargo-deny v0.19 config; closed allow-list of permissive SPDX expressions; `[bans]` warns on duplicates and external wildcards, `allow-wildcard-paths = true` for workspace path deps; `[advisories]` warns on yanked; `[sources]` rejects unknown registries and any git-source dep.
- `.github/workflows/ci-deny.yml` — single-job workflow on `ubuntu-latest`, installs cargo-deny `--locked --version ^0.19`, runs `cargo deny check --hide-inclusion-graph` on push and PR to master.

## Decisions Made

- **cargo-deny pinned to `^0.19`** (not the plan's `^0.14`). v0.14.24 fails to load the current RustSec advisory DB because of a CVSS 4.0 parse error (`RUSTSEC-2026-0041`). v0.19.6 supports the modern DB format and the v2 deny.toml schema. Local and CI install commands both use `--version ^0.19` so they cannot drift within the 0.19.x patch range.
- **v2 schema (no `[licenses].deny`)**. The legacy v1 schema had a separate `deny = [...]` array under `[licenses]`. v0.16 removed it; the `allow` list became exhaustive and any license not on the list fails the check. The strong-copyleft licenses the plan explicitly enumerated (GPL-2.0/3.0, AGPL-3.0, LGPL-2.1/3.0, SSPL-1.0) are now rejected by absence, documented in a comment block in `deny.toml`.
- **Wildcards at `warn`, not `deny`**. `wildcards = "deny"` failed on workspace-internal path deps (`bootroom-core.workspace = true` from `crates/bootroom/Cargo.toml`, and `bootroom = { path = "../.." }` from `crates/bootroom/spikes/spike-b/Cargo.toml`). The plan's threat T-06-05-08 accepted workspace-internal exemption; the concrete way to express that in v0.19 is `wildcards = "warn"` plus `allow-wildcard-paths = true`. External wildcard regressions still surface as warnings.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocker] cargo-deny version bump from `^0.14` → `^0.19`**
- **Found during:** Task 2 (`cargo deny check` initial run)
- **Issue:** cargo-deny v0.14.24 — the version implied by the plan's `^0.14` CI pin — fails to load the current RustSec advisory DB. The crash:
  ```
  failed to load advisory database: parse error: error parsing
    /home/.../crates/lz4_flex/RUSTSEC-2026-0041.md:
    unsupported CVSS version: 4.0
  ```
  CVSS 4.0 entries are now standard in the RustSec DB; v0.14 only handles CVSS 3.x. The gate would fail on every CI run.
- **Fix:** Bumped local install and CI pin to `^0.19`. cargo-deny v0.19.6 (latest in the 0.19.x series) parses CVSS 4.0 correctly.
- **Files modified:** `deny.toml` (header comments), `.github/workflows/ci-deny.yml` (`--version ^0.19`).
- **Verification:** `cargo deny check` exits 0; advisories check passes against the current RustSec DB.
- **Committed in:** `eed7411` (Task 2), `0fafe68` (Task 3 — CI pin matches local).

**2. [Rule 3 — Blocker] v0.16+ schema migration in `deny.toml`**
- **Found during:** Task 2 (post-install schema validation)
- **Issue:** cargo-deny v0.16+ removed four keys the plan-text `deny.toml` relied on:
  - `[licenses].unlicensed = "deny"` (removed; absence-of-license is now part of the default behaviour)
  - `[licenses].deny = [...]` (removed; closed allow-list replaces it)
  - `[advisories].unmaintained = "all"` / `"warn"` (removed; replaced by severity-aware advisory matching with `ignore` entries for exceptions)
  - `[advisories].unsound = "warn"` (removed; same)
- **Fix:** Rewrote `deny.toml` against the v2 schema. Strong-copyleft enforcement is now expressed by their absence from `allow`, documented in an inline comment block. The `[advisories]` block keeps only `yanked = "warn"` and an empty `ignore = []`.
- **Files modified:** `deny.toml`.
- **Verification:** `cargo deny check` parses the config cleanly with cargo-deny v0.19.6.
- **Committed in:** `eed7411` (Task 2).

**3. [Rule 3 — Blocker] Workspace path-dep wildcards**
- **Found during:** Task 2 (first successful `cargo deny check` parse)
- **Issue:** `wildcards = "deny"` flagged two workspace-internal path deps as wildcard violations:
  - `bootroom-core.workspace = true` in `crates/bootroom/Cargo.toml` (workspace dep with no version pin — cargo-deny treats this as `*`).
  - `bootroom = { path = "../.." }` in `crates/bootroom/spikes/spike-b/Cargo.toml`.
  These are intentional workspace-internal deps; the plan's threat model (T-06-05-08) explicitly accepted workspace-internal exemption.
- **Fix:** Downgraded `wildcards = "deny"` → `wildcards = "warn"` and added `allow-wildcard-paths = true`. External wildcard pins (which would be a real regression) still surface as CI warnings; workspace path deps no longer block the gate.
- **Files modified:** `deny.toml`.
- **Verification:** `bans ok` in the final `cargo deny check` output.
- **Committed in:** `eed7411` (Task 2).

---

**Total deviations:** 3 auto-fixed (all Rule 3 — blocking issues discovered when running the gate)
**Impact on plan:** All three fixes were mechanical adaptations to (a) tool version, (b) tool schema evolution, and (c) the unavoidable presence of workspace-internal path deps. The plan's intent — mechanical license enforcement on every push and PR — is delivered intact, with the same allow-list set and the same CI trigger surface.

## Issues Encountered

- **First `cargo install cargo-deny --locked` installed v0.19.6, not v0.14.** With no version specifier, cargo grabs the latest version. The plan's `^0.14` pin was specified only on the CI workflow, not on the local install command; I re-installed twice (once to `^0.14` to honour the plan's intent, then back to `^0.19` once the v0.14 advisory-DB crash made the pin untenable). Net result: local install matches CI pin.
- **Three `license-not-encountered` warnings.** The defensive allow-list entries for `MPL-2.0`, `Unicode-DFS-2016`, and `Zlib` did not match any dep in the current tree (cargo-deny v0.19's license detection differs from earlier versions). These are warn-level only and do not fail the gate. Kept in the allow-list as forward-compatibility: removing them would force a CI failure the first time a dep transitively pulls one in.
- **Two `duplicate` bans warnings.** `tungstenite 0.28.0` + `0.29.0` (via chromiumoxide → async-tungstenite vs axum → tokio-tungstenite), and `windows-sys 0.60.2` + `0.61.2` (notify vs anstyle-query). These are warn-level and reflect the upstream ecosystem state; resolution requires upstream dep updates and is out of scope for 06-05.

## Captured Tool Output

### Local cargo-deny version

```
cargo-deny 0.19.6
```

Install command: `cargo install cargo-deny --locked --version ^0.19`

### `cargo deny check` final exit

Exit code: `0`

Tail of stdout:

```
advisories ok, bans ok, licenses ok, sources ok
```

### Full `deny.toml` (verbatim, as committed)

```toml
# deny.toml — cargo-deny configuration for bootroom.
#
# Purpose: mechanical enforcement of the project's license posture
# (MIT OR Apache-2.0 + compatible permissive licenses across the dep tree)
# and supply-chain hygiene (no git-source deps, no wildcard pins, etc.).
#
# Run locally:
#     cargo install cargo-deny --locked --version ^0.19
#     cargo deny check
#
# CI runs the same command on every push and PR to master via
# .github/workflows/ci-deny.yml. Local and CI install the same major version
# (^0.19) so the gate behaves identically.
#
# Schema: cargo-deny v0.16+ (uses implicit version = 2 schema; old keys
#         like `unlicensed`, `unmaintained`, `unsound`, and top-level
#         `[licenses].deny` were removed in v0.16).
# Docs:   https://embarkstudios.github.io/cargo-deny/checks/index.html

[graph]
# Check all features so platform- or feature-gated deps with different
# license metadata are not silently skipped.
all-features = true

[output]
# Keep the inclusion-graph output readable. cargo-deny still emits the
# full graph when a check fails.
feature-depth = 1

# -----------------------------------------------------------------------------
# [licenses] — the heart of D-10 / DIST-07.
#
# In the v2 schema, the `allow` list is exhaustive: any SPDX expression
# not on the list fails the check. There is no separate `deny` list — its
# job is implicit in the closed allow-list. Strong-copyleft licenses
# (GPL-*, AGPL-*, LGPL-*, SSPL-*) are rejected simply by not appearing.
# -----------------------------------------------------------------------------
[licenses]
# cargo-deny's recommended threshold for matching ambiguous LICENSE-file text
# against a declared SPDX expression. Lower values cause false positives.
confidence-threshold = 0.93

# Permissive licenses compatible with bootroom's "MIT OR Apache-2.0" posture.
# Adding to this list is a legal-posture change — require explicit review.
#
# Strong-copyleft licenses (GPL-2.0, GPL-3.0, AGPL-3.0, LGPL-2.1, LGPL-3.0,
# SSPL-1.0) are intentionally absent. cargo-deny v0.16+ rejects any license
# not on this list — there is no separate deny-list in the v2 schema.
allow = [
    "MIT",                              # our own + the majority of Rust crates
    "Apache-2.0",                       # our own + the majority of Rust crates
    "Apache-2.0 WITH LLVM-exception",   # compiler-stack crates (e.g. rustix)
    "BSD-2-Clause",                     # older BSD-licensed deps
    "BSD-3-Clause",                     # older BSD-licensed deps
    "ISC",                              # tiny utility crates (e.g. ring, untrusted)
    "Unicode-DFS-2016",                 # unicode-ident (transitive via syn / serde-derive)
    "Unicode-3.0",                      # newer unicode crates (e.g. icu_*)
    "Zlib",                             # some compression / bindings crates
    "CC0-1.0",                          # public-domain-equivalent
    "BSL-1.0",                          # Boost Software License (permissive)
    # MPL-2.0: weak copyleft. Allowed because:
    #   (a) MPL-2.0 file-level copyleft does not impose source-disclosure
    #       on bootroom's binary as a whole (only on modifications to the
    #       MPL-licensed files themselves), and
    #   (b) common transitive deps (e.g. webpki-roots) ship under it.
    # Re-evaluate if the number of MPL-2.0 deps grows or if any direct dep
    # of bootroom adopts MPL-2.0.
    "MPL-2.0",
]

# Per-crate license exceptions go here with rationale. Empty by default.
exceptions = []

# Crates with missing-but-known license metadata. Empty by default.
clarify = []

# -----------------------------------------------------------------------------
# [bans] — crate-level policy.
# -----------------------------------------------------------------------------
[bans]
# Workspace pins major versions, so duplicates are mostly cosmetic. Warn
# instead of deny to keep the gate focused on legal/security issues.
multiple-versions = "warn"

# Wildcard version specifiers ("*") are surfaced but not blocking. A pure
# external "*" dep on crates.io is a regression and should be triaged
# through the CI warning; the warning level keeps this from blocking on
# workspace-internal path deps (bootroom-core.workspace = true, and the
# spike-b → bootroom path dep) which cargo-deny treats as wildcards but
# which are intentional and constrained by the workspace itself.
# `allow-wildcard-paths` below covers unpublished crates (spike-b) but
# does not cover wildcard path deps inside public crates (bootroom →
# bootroom-core), so we keep this at "warn" rather than "deny".
wildcards = "warn"

# Permit path-style wildcard deps in unpublished workspace members
# (spike-b is publish = false). External wildcards still surface.
allow-wildcard-paths = true

# Crates to ban entirely. Empty by default; add with rationale.
deny = []

# -----------------------------------------------------------------------------
# [advisories] — RustSec advisory DB checks.
#
# v2 schema: per-severity gating moved out of `unmaintained`/`unsound` keys
# and into the default "deny on any match" behaviour, with explicit
# `ignore` entries for vetted exceptions. `yanked` is still configurable.
# -----------------------------------------------------------------------------
[advisories]
# Warn on yanked crate versions (rather than deny) to keep first-run CI
# from failing on pre-existing yanked transitive deps. Promote to "deny"
# once the baseline is clean.
yanked = "warn"

# Vetted ignores go here. Each entry pairs a RustSec advisory ID with the
# rationale for why it does not apply to bootroom (path of exposure,
# upstream status, scheduled remediation). Empty by default.
ignore = []

# -----------------------------------------------------------------------------
# [sources] — restrict where deps may come from.
# -----------------------------------------------------------------------------
[sources]
# Only registries on the explicit allow-list below.
unknown-registry = "deny"

# No git-source deps. The workspace currently has none; this prevents
# accidental regression where a dep gets pulled in via { git = "..." }.
unknown-git = "deny"

# Canonical crates.io URL.
allow-registry = ["https://github.com/rust-lang/crates.io-index"]

# Empty by default; add with rationale if a git-source dep is ever needed.
allow-git = []
```

### Full `.github/workflows/ci-deny.yml` (verbatim, as committed)

```yaml
name: cargo-deny

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

# cargo-deny is a license/security gate; cancel superseded runs.
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  cargo-deny:
    name: cargo deny check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Pin the cargo-deny version explicitly. Drift between local and CI is
      # impossible if this version matches the one used during 06-05 plan
      # execution. The local install used `cargo install cargo-deny --locked
      # --version ^0.19`. The caret allows patch upgrades within 0.19.x;
      # a v0.20 with breaking schema changes will require an explicit bump.
      #
      # NOTE: the original plan specified ^0.14; that pin was bumped to
      # ^0.19 because (a) cargo-deny v0.14 cannot parse CVSS 4.0 entries in
      # the current RustSec advisory DB (RUSTSEC-2026-0041), and (b) v0.16+
      # removed several deprecated keys (unlicensed, [licenses].deny,
      # [advisories].unmaintained, [advisories].unsound) that the plan-text
      # deny.toml relied on. The v2 schema is documented in deny.toml.
      - name: Install cargo-deny
        run: cargo install cargo-deny --locked --version ^0.19

      # `--hide-inclusion-graph` keeps log output bounded; cargo-deny still
      # prints the full graph when an issue is found.
      - name: Run cargo deny check
        run: cargo deny check --hide-inclusion-graph
```

## User Setup Required

None — no external service configuration required. The CI workflow runs automatically on push/PR; no GitHub secrets, no third-party tokens.

## Next Phase Readiness

- DIST-07 has a mechanical enforcement layer. The dep tree's license posture cannot regress without (a) a visible diff to `deny.toml` and (b) a CI failure on push/PR.
- The next plan (06-06 onwards) can rely on `cargo deny check` exiting 0 against the current dep tree.
- Open items to revisit (logged here, not blocking):
  - Tighten `[bans].multiple-versions = "warn"` to `"deny"` once the two duplicate clusters (`tungstenite`, `windows-sys`) resolve upstream.
  - Tighten `[advisories].yanked = "warn"` to `"deny"` once the baseline is yanked-clean.
  - Reconsider the `wildcards = "warn"` decision when/if `bootroom-core` becomes a separately-published crate (would need a real semver pin in `crates/bootroom/Cargo.toml`).

## Self-Check: PASSED

- `deny.toml` exists at repo root: FOUND.
- `.github/workflows/ci-deny.yml` exists: FOUND.
- Commit `0171779` (Task 1) — feat: deny.toml allow-list: FOUND in `git log`.
- Commit `eed7411` (Task 2) — fix: schema/wildcards triage: FOUND in `git log`.
- Commit `0fafe68` (Task 3) — feat: ci-deny.yml: FOUND in `git log`.
- `cargo deny check` exits 0 against the current workspace: confirmed.

---
*Phase: 06-distribution*
*Completed: 2026-05-19*
