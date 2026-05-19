---
phase: 06-distribution
plan: 04
subsystem: infra
tags: [makefile, cargo, cargo-dist, distribution, operator-tooling]

requires:
  - phase: 06-distribution
    provides: cargo-dist 0.31+ release pipeline (06-03), workspace `[package].include` allow-list (06-02)
provides:
  - "`make install` operator-facing wrapper for `cargo install --locked --path crates/bootroom` (DIST-02 satisfied)"
  - "`make release` operator-facing wrapper for `cargo dist build --artifacts=all` (local pre-tag smoke)"
  - "`make help` lists all four targets (qemu-assets, clean-qemu-assets, install, release) with aligned column"
affects:
  - documentation (README install instructions can now point at `make install`)
  - release workflow (operators have a local smoke step that matches the CI release.yml artifact set)

tech-stack:
  added: []
  patterns:
    - "Makefile-as-operator-sugar: thin wrappers over canonical cargo invocations, no logic"

key-files:
  created: []
  modified:
    - "Makefile"

key-decisions:
  - "Recipe lines copy the canonical cargo invocations verbatim (no variables, no expansion) — the verification gates grep for exact strings, and any drift would silently break reproducibility."
  - "`make release` calls `cargo dist build`, NOT `dist build`, even though cargo-dist 0.31+ ships the binary as `dist`. This matches the plan frontmatter's locked recipe and the verification gate. (Note: `cargo dist` is a shim cargo provides for any `cargo-*` binary on PATH — confirmed `/home/sandwich/.cargo/bin/dist` is present.)"
  - "`install` and `release` are independent — no `release: install` chain. CONTEXT.md decisions don't pair them, and the operator workflow doesn't either (`release` is a pre-tag smoke; `install` is a one-shot personal install)."
  - "Default goal stays `help` (first declared target). Bare `make` continues to print help, which is the discoverability story for this Makefile."

patterns-established:
  - "Operator targets carry inline comments explaining purpose, prerequisites, and what they do NOT do (e.g. `release` explicitly does not publish)."

requirements-completed: [DIST-02]

duration: 4min
completed: 2026-05-19
---

# Phase 06 Plan 04: Makefile install/release targets Summary

**`make install` (DIST-02) and `make release` operator wrappers appended to the workspace Makefile with aligned help text.**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-05-19T18:52:00Z (approx)
- **Completed:** 2026-05-19T18:56:25Z
- **Tasks:** 2 (1 modify + 1 verify)
- **Files modified:** 1 (Makefile)

## Accomplishments

- DIST-02 satisfied: `make install` runs `cargo install --locked --path crates/bootroom`, dropping the binary into `~/.cargo/bin` from a clean checkout in one command.
- `make release` provides a local pre-tag smoke that exercises the same artifact set the GitHub Actions release workflow (`.github/workflows/release.yml` from 06-03) emits on tag push.
- `make help` (and bare `make`) lists all four targets in a single aligned 17-char column.
- `.PHONY` declaration extended; no existing recipe altered.

## Task Commits

1. **Task 1: Append `install` and `release` targets to the workspace-root Makefile** — `c056627` (feat)
2. **Task 2: Smoke-test `make install --dry-run` and confirm `make` (bare) still prints help** — read-only verification; outputs captured below; no separate commit

## Files Created/Modified

- `Makefile` — Extended `.PHONY` line, added two new echo lines to the `help:` recipe, appended `install:` and `release:` targets (with explanatory comment blocks) after `clean-qemu-assets`.

## Verbatim Makefile additions

`.PHONY` line:
```
.PHONY: qemu-assets clean-qemu-assets help install release
```

Two new `help:` echo lines (appended to the existing block):
```
	@echo "  install           cargo install --locked --path crates/bootroom (local user install)"
	@echo "  release           cargo dist build --artifacts=all (local cross-platform smoke; NOT a publish)"
```

`install:` recipe (appended after `clean-qemu-assets:`):
```
# DIST-02: single-command local install. Uses --locked so the workspace
# Cargo.lock is honored exactly; mismatched transitive deps would otherwise
# break reproducibility for kernel-CI consumers. Installs to ~/.cargo/bin
# by default (or whatever CARGO_INSTALL_ROOT points at).
install:
	cargo install --locked --path crates/bootroom
```

`release:` recipe:
```
# Local cross-platform release smoke. Runs cargo-dist's build pipeline against
# the locked four targets; produces the same artifact set the GitHub Actions
# release workflow (.github/workflows/release.yml) emits on a `v*` tag push.
# Useful BEFORE pushing a tag to catch packaging regressions locally.
#
# Requires cargo-dist installed (`cargo install cargo-dist --locked`) and
# cross-toolchains (cargo-zigbuild handles the linux musl cross; macOS hosts
# may need additional setup for linux targets — see cargo-dist docs).
#
# This target does NOT publish to crates.io or to GitHub Releases — those
# only happen via the on-tag CI workflow.
release:
	cargo dist build --artifacts=all
```

## Captured dry-run outputs (Task 2)

`make -n install`:
```
cargo install --locked --path crates/bootroom
```

`make -n release`:
```
cargo dist build --artifacts=all
```

`make help`:
```
bootroom Makefile targets:
  qemu-assets       Rebuild qemu-wasm artifacts (requires docker; 10-30 minutes)
  clean-qemu-assets Remove generated qemu artifacts from crates/bootroom/assets/qemu
  install           cargo install --locked --path crates/bootroom (local user install)
  release           cargo dist build --artifacts=all (local cross-platform smoke; NOT a publish)
```

Bare `make` (no args) — identical to `make help`, confirming the default goal is still `help` (it remains the first declared target in the file).

`make -p` target enumeration (filtered):
```
clean-qemu-assets:
help:
install:
qemu-assets:
release:
```

**NOT exercised:** `make install` and `make release` were NOT actually run. Both would be heavy operations (cargo install rebuilds the workspace + writes `~/.cargo/bin`; cargo-dist invokes cross-toolchains). Per the plan, dry-run verification only.

## Decisions Made

See `key-decisions` in frontmatter. The two notable judgment calls:

1. **Recipe string == plan literal.** The orchestrator's `<context>` block mentioned that cargo-dist 0.31+ ships its binary as `dist` (not `cargo dist`), and `/home/sandwich/.cargo/bin/dist` confirms that. However, the plan's frontmatter `must_haves`, `key_links`, and verification gate explicitly require `cargo dist build --artifacts=all` as a literal string. `cargo dist` is also a working invocation because cargo dispatches `cargo <name>` to any `cargo-<name>` binary on PATH, and `dist` is a cargo-dist subcommand. Followed the plan literally — both invocations are equivalent on this host.

2. **No commit for Task 2.** Task 2 is pure read-only verification; the plan explicitly states "no files modified" and the action is to capture output for the SUMMARY. The outputs live in this SUMMARY, which is committed as the plan metadata commit.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required for this plan. Operators who want to actually use `make release` need cargo-dist installed (`cargo install cargo-dist --locked`); the recipe's leading comment documents this.

## Next Phase Readiness

- DIST-02 closed. Phase 06 wave 4 unblocked.
- `make release` is available for pre-tag local smoke; integrates with 06-03's `release.yml` CI workflow (same artifact set).
- No blockers.

## Self-Check: PASSED

- File `Makefile` modified — confirmed via `git diff HEAD~1 HEAD -- Makefile`.
- Commit `c056627` exists — confirmed via `git log --oneline | grep c056627`.
- All five Task 1 verification grep gates pass.
- All four Task 2 verification gates pass.

---
*Phase: 06-distribution*
*Completed: 2026-05-19*
