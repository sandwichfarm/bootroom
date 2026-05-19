---
phase: 05-diagnostics-doctor
plan: 01
subsystem: build
tags: [build-script, env-var, version, git-sha, doctor, wave-0]
requires:
  - crates/bootroom/build.rs (Phase 1 REQUIRED-file presence check)
provides:
  - "cargo:rustc-env=BOOTROOM_GIT_SHA emitted on every build"
  - "env!(BOOTROOM_GIT_SHA) consumable at runtime by future doctor `version` check (05-04)"
  - "Wave-0 test scaffold at crates/bootroom/tests/doctor_subcommand.rs for 05-05 to extend"
affects:
  - 05-04 (doctor `version` check will env!() this)
  - 05-05 (extends doctor_subcommand.rs with CLI shape tests)
tech-stack:
  added: []
  patterns:
    - "Research Pattern 3: .ok().filter().and_then().filter().unwrap_or_else() chain for fail-safe Command::output() capture"
key-files:
  modified:
    - crates/bootroom/build.rs
  created:
    - crates/bootroom/tests/doctor_subcommand.rs
decisions:
  - "Capture git SHA at build time (not runtime) so `cargo install bootroom` from crates.io tarballs (which strip .git/) still emit a meaningful value — degraded to \"unknown\""
  - "Use std::process::Command + safe-chain, no new crate dep (git2/gix would add 30+ transitive deps for one shell-out)"
  - "Watch both .git/HEAD and .git/refs — HEAD covers commit/checkout, refs covers branch updates that don't move HEAD"
metrics:
  duration: 2m37s
  completed: 2026-05-19
  tasks_completed: 2
  files_created: 1
  files_modified: 1
---

# Phase 05 Plan 01: BOOTROOM_GIT_SHA Build-Time Capture Summary

**One-liner:** Extended `crates/bootroom/build.rs` to emit `cargo:rustc-env=BOOTROOM_GIT_SHA=<short-sha>` via `std::process::Command` with a safe `.ok().filter().and_then().filter().unwrap_or_else("unknown")` chain, and wave-0'd a 3-test scaffold pinning the contract so plan 05-04 can `env!("BOOTROOM_GIT_SHA")` without surprise.

## Goal

Capture `git rev-parse --short HEAD` into a compile-time env var so the future `bootroom doctor` `version` check (CONTEXT.md D-DOC-04) can render `bootroom <CARGO_PKG_VERSION> (<sha>)` even from `cargo install`-sourced binaries that ship without `.git/`. Wave-0 a tiny integration test that proves the env var is exposed end-to-end.

## Tasks Completed

| Task | Name                                                              | Commit  | Files                                          |
| ---- | ----------------------------------------------------------------- | ------- | ---------------------------------------------- |
| 1    | Wave-0 test scaffold — pin env!(BOOTROOM_GIT_SHA) is exposed      | c6da1d9 | crates/bootroom/tests/doctor_subcommand.rs (created) |
| 2    | Extend build.rs to capture BOOTROOM_GIT_SHA with "unknown" fallback | 5daa19e | crates/bootroom/build.rs (modified)            |

## TDD Cycle

- **RED (task 1, commit c6da1d9):** Created `tests/doctor_subcommand.rs` with three `git_sha_*` tests. `cargo test --test doctor_subcommand git_sha` failed at compile time with:
  ```
  error: environment variable `BOOTROOM_GIT_SHA` not defined at compile time
   --> crates/bootroom/tests/doctor_subcommand.rs:9:19
    |
  9 | const SHA: &str = env!("BOOTROOM_GIT_SHA");
    |                   ^^^^^^^^^^^^^^^^^^^^^^^^
  ```
  This is the documented RED state — the failure message explicitly names `BOOTROOM_GIT_SHA`, proving the test is sensitive to the contract we're about to add.

- **GREEN (task 2, commit 5daa19e):** Appended the git-SHA capture block to `build.rs`. All three tests passed on first compile:
  ```
  running 3 tests
  test git_sha_env_has_no_whitespace ... ok
  test git_sha_env_is_set ... ok
  test git_sha_env_shape_is_short_sha_or_unknown ... ok

  test result: ok. 3 passed; 0 failed
  ```

- **REFACTOR:** Not needed — the appended block is the minimum sufficient form (Pattern 3 verbatim).

## The Exact git-Invocation Chain Shape

For 05-04 (and any future reader) to reference:

```rust
let sha = std::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()                                          // git missing from PATH -> None
    .filter(|o| o.status.success())                // not a git repo -> non-zero exit -> filtered out
    .and_then(|o| String::from_utf8(o.stdout).ok())// non-UTF-8 stdout (vanishingly rare) -> None
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())                     // empty trim result -> filtered out
    .unwrap_or_else(|| "unknown".to_string());
println!("cargo:rustc-env=BOOTROOM_GIT_SHA={sha}");
println!("cargo:rerun-if-changed=.git/HEAD");
println!("cargo:rerun-if-changed=.git/refs");
```

Each link in the chain neutralizes one failure mode without panicking:

| Failure mode                          | Where in chain it's caught                 | Result        |
| ------------------------------------- | ------------------------------------------ | ------------- |
| `git` binary absent (no PATH entry)   | `.output().ok()` -> `None`                 | `"unknown"`   |
| Running outside a git repo            | `.filter(|o| o.status.success())` -> `None`| `"unknown"`   |
| Detached / missing HEAD               | git itself exits non-zero -> same as above | `"unknown"`   |
| Empty stdout (unusual)                | `.filter(|s| !s.is_empty())` -> `None`     | `"unknown"`   |
| Non-UTF-8 stdout (vanishingly rare)   | `.and_then(String::from_utf8)`              | `"unknown"`   |
| Healthy git checkout                  | All filters pass                            | `"<short-sha>"` |

No `.unwrap()` anywhere on the git path. Per D-DOC-04, the build never aborts because of git's state.

## Sanity Check: Captured SHA vs. `git rev-parse --short HEAD`

At commit time of task 2 (`5daa19e`), the worktree's `git rev-parse --short HEAD` returned `c6da1d9` (the prior commit, since task-2's commit hadn't happened yet at build time). The `git_sha_env_shape_is_short_sha_or_unknown` test passed, confirming the captured value matches the `[0-9a-f]{7,40}` shape. Each subsequent `cargo build` will pick up the new HEAD via `cargo:rerun-if-changed=.git/HEAD`.

## Verification Results

| Check                                                                               | Result | Notes                                                          |
| ----------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------- |
| `cargo test --test doctor_subcommand git_sha` — 3 pass                              | PASS   | All three `git_sha_*` tests green                              |
| `grep -c 'cargo:rustc-env=BOOTROOM_GIT_SHA' build.rs` == 1                           | PASS   | Single emission, no duplicates                                 |
| `grep -v '^#' build.rs \| grep -c 'cargo:rerun-if-changed=.git'` == 2                | PASS   | Both `.git/HEAD` and `.git/refs` watched                       |
| `cargo build -p bootroom` still succeeds (Phase-1 REQUIRED-file presence unchanged) | PASS   | Built in 1.85s, no regression                                  |
| No new transitive crate deps (no `git2`, no `gix`)                                  | PASS   | `Command::new("git")` shells out to system git                 |

## Deviations from Plan

None — plan executed exactly as written. Pattern 3 used verbatim from the plan's `<interfaces>` block. No deviation rules triggered.

## Known Stubs

None.

## Threat Flags

None — no new attack surface introduced. The `BOOTROOM_GIT_SHA` value is intentionally public (rendered in `doctor` output for support requests; no secret material). The trust boundary (`build host → bootroom binary`) was already in place from Phase 1's qemu-asset embedding.

## Success Criteria Status

- [x] `crates/bootroom/build.rs` emits `cargo:rustc-env=BOOTROOM_GIT_SHA=<value>` on every build.
- [x] Value is a short SHA (`[0-9a-f]{7,40}`) on git checkouts, exactly `"unknown"` otherwise.
- [x] No panics, no `.unwrap()` on git invocation paths.
- [x] Three `git_sha_*` tests in `tests/doctor_subcommand.rs` pass.
- [x] Existing Phase-1 REQUIRED-file presence check and `BOOTROOM_SKIP_QEMU_ASSET_CHECK` escape hatch behavior are byte-identical to pre-change behavior on the happy path.

## Self-Check: PASSED

- File `crates/bootroom/build.rs` exists and contains the new git-SHA block.
- File `crates/bootroom/tests/doctor_subcommand.rs` exists with three `git_sha_*` tests.
- Commit `c6da1d9` (RED test scaffold) present in `git log`.
- Commit `5daa19e` (GREEN build.rs extension) present in `git log`.
