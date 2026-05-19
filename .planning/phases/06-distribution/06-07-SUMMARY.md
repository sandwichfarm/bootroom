---
phase: 06-distribution
plan: 07
subsystem: docs
tags: [readme, install, quickstart, badges, cargo-dist, distribution]

requires:
  - phase: 06-distribution
    provides: cargo-dist release pipeline (06-03), shell-installer URL shape (06-04), release-smoke pre-publish gate (06-06)
  - phase: 05
    provides: bootroom doctor preflight subcommand
  - phase: 01
    provides: bootroom serve --kernel canonical first-run command

provides:
  - Top-of-file README install matrix covering all three locked install paths
  - Quickstart anchored on `bootroom doctor` followed by `bootroom serve --kernel <path>`
  - NORN first-consumer link
  - Badge row (license, crates.io, docs.rs, release workflow)
  - Relocation of the legacy `cargo build --release` flow into a `## Source builds and contribution` section

affects: [07-release, future docs work, NORN onboarding]

tech-stack:
  added: []
  patterns:
    - "Three-tier install matrix presentation: primary (cargo install) -> secondary (cargo binstall) -> tertiary (shell installer)"
    - "Hardened curl flags (`--proto =https --tlsv1.2 -sSf`) for the shell-installer one-liner per cargo-dist guidance"

key-files:
  created:
    - .planning/phases/06-distribution/06-07-SUMMARY.md
  modified:
    - README.md

key-decisions:
  - "Use a single 3-row markdown table for the install matrix (not three separate code blocks) — keeps the matrix scannable in one glance and matches CONTEXT.md D-07's 'three install paths in one section' framing."
  - "Backslash-escape the pipe in the shell-installer command so the GitHub markdown table renders correctly. The whole command stays inside backticks for shell readability."
  - "Document NORN at the URL `https://github.com/sandwich-farm/nostros` based on PROJECT.md's `~/Develop/nostros` reference. Flagged as a placeholder for the user to confirm — if NORN's canonical GitHub URL differs, a single search-and-replace fixes it."
  - "Preserve the existing `## Status` section that follows `## License` (it predates this plan and is harmless)."
  - "Source-build flow moved into `## Source builds and contribution` rather than deleted — preserves the contribution path for hackers on bootroom itself."

patterns-established:
  - "README terseness rule: install matrix + quickstart + first-consumer link + source-build path; depth lives in `.planning/`"
  - "Badge ordering: License -> crates.io -> docs.rs -> release workflow"

requirements-completed: [DIST-02, DIST-03, DIST-04, DIST-06]

duration: ~10min
completed: 2026-05-19
---

# Phase 06 Plan 07: README install matrix + quickstart Summary

**README top-of-file rewrite delivering install matrix (cargo install / cargo binstall / hardened curl-installer), `bootroom doctor` -> `bootroom serve --kernel` quickstart, NORN consumer link, and full badge row — replacing the pre-Phase-6 `cargo build --release` placeholder quickstart.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-19T20:55:00Z (approx)
- **Completed:** 2026-05-19T21:05:00Z (approx)
- **Tasks:** 1/1
- **Files modified:** 1 (README.md)

## Accomplishments
- Install matrix documents all three locked install paths in primary-secondary-tertiary order.
- Quickstart walks the user through `bootroom doctor` followed by `bootroom serve --kernel <path>` — the canonical first-run flow.
- Badge row adds crates.io, docs.rs, and release-workflow badges next to the existing license badge.
- NORN is linked as the first downstream consumer at the top of the README.
- Hardened curl flags (`--proto =https --tlsv1.2 -sSf`) baked into the shell-installer command per cargo-dist hardening guidance — mitigates T-06-07-01.
- Stale standalone `cargo build --release` quickstart removed and folded into `## Source builds and contribution` for hackers working on bootroom itself.

## Task Commits

1. **Task 1: Rewrite README.md top-of-file section with install matrix + quickstart** — `a4ba229` (docs)

## Files Created/Modified
- `README.md` — Rewritten body between the H1 + tagline + repo-rename note (preserved) and the existing `## License` / `## Status` sections (preserved). Adds badges, "First consumer" line, `## Install`, `## Quickstart`, and `## Source builds and contribution` sections.
- `.planning/phases/06-distribution/06-07-SUMMARY.md` — This summary.

## Decisions Made
- **3-row markdown table for the install matrix** instead of three separate code blocks, to match the "one glance" framing in CONTEXT.md D-07.
- **Pipe-escape inside backticks** for the shell-installer command's `| sh` so it renders as a literal pipe in a GitHub-flavored markdown table cell.
- **NORN URL placeholder** at `https://github.com/sandwich-farm/nostros` based on PROJECT.md's `~/Develop/nostros` directory reference. Flagged below for user confirmation.
- **Preserve `## Status` section** that exists in the current README between `## License` and EOF — it predates this plan, is informative, and removing it was not part of the plan's mandate.

## Deviations from Plan

**None — plan executed exactly as written.**

The plan's reference README block did not include the existing `## Status` section that lives in the current file. I preserved it verbatim rather than deleting it (deletion would have been a silent scope addition; the plan did not list it for removal).

## Issues Encountered

**1. Absolute-path drift on first Write call**
- **Issue:** The first `Write` call used an absolute path that pointed at the main repo's `README.md` (`/home/sandwich/Develop/norn-web/README.md`) rather than the worktree's `README.md` (`/home/sandwich/Develop/norn-web/.claude/worktrees/agent-a3c505fa4ce04040a/README.md`). This is the worktree-absolute-path safety failure mode called out in the executor checklist.
- **Detection:** `git status` reported a clean tree even though the file had "just been written" — `md5sum` confirmed the worktree file was unchanged and the main repo file had the new content.
- **Recovery:** Reverted the main repo's README with `git checkout -- README.md` (run from the main repo, not the worktree), then re-issued the Write call against the worktree-rooted absolute path. Verified with `git status` + `git diff --stat` that the change now lived inside the worktree before staging.
- **Impact:** No commit was made against the main repo (the revert happened before any staging). No data was lost — the original main-repo README content is identical to what's at HEAD on master.

## Placeholders Left for User Confirmation

- **NORN GitHub URL** — currently set to `https://github.com/sandwich-farm/nostros`. If the canonical URL is different (different org, different repo name), a single search-and-replace in `README.md` will fix it. The URL appears twice in the markdown: once in the "First consumer" line.
- **crates.io / docs.rs badges** will render as 404s until the first `bootroom` crate publish lands. This is intentional and helpful — visible 404s signal "release not yet shipped" during the transition window.
- **Release workflow badge** points at `https://github.com/sandwich-farm/bootroom/actions/workflows/release.yml`. If the GitHub remote is still under a different org/name at publication time, this URL must be updated along with `[workspace.package].repository` in `Cargo.toml`.

## User Setup Required

None — README edits only, no external service configuration.

## Threat Flags

None — this plan only edits documentation. The hardened curl flags (`--proto =https --tlsv1.2 -sSf`) directly address T-06-07-01 from the plan's threat model.

## Next Phase Readiness

- All Phase 6 wave-4 plans are now complete: 06-04 (release pipeline), 06-06 (release-smoke gate), and 06-07 (README polish).
- The README now matches the user-facing surface that the upcoming 07-release work will exercise.
- No blockers for the next phase.

## Self-Check: PASSED

- README.md exists and contains the required strings.
- Commit `a4ba229` exists in the worktree branch (`worktree-agent-a3c505fa4ce04040a`).
- Verification gate passed (see Task 1 verify command).

---
*Phase: 06-distribution*
*Plan: 07*
*Completed: 2026-05-19*
