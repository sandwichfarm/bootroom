---
phase: 06-distribution
plan: 02
subsystem: distribution
tags: [cargo-publish, include-allow-list, publish-metadata, crates-io, dist-05]
dependency_graph:
  requires:
    - "06-01 (LICENSE-MIT and LICENSE-APACHE at workspace root, license SPDX expression in workspace package)"
  provides:
    - "Explicit `[package].include` allow-list pinning every file in the bootroom publish tarball"
    - "Explicit `[package].include` allow-list pinning every file in the bootroom-core publish tarball"
    - "Full crates.io publish metadata (homepage, documentation, readme, keywords, categories, publish=true)"
    - "In-crate symlinks (LICENSE-MIT, LICENSE-APACHE, README.md) that preserve workspace-root single-source-of-truth while making the files accessible to cargo's package allow-list"
  affects:
    - "06-03 (cargo-dist can now compute a deterministic workspace tarball without silent file drops)"
    - "DIST-05 (embedded asset path-independence is mechanically enforced — every build.rs REQUIRED entry is allow-listed)"
tech_stack:
  added: []
  patterns:
    - "Cargo `[package].include` allow-list mode (gitignore-style globs, anchored with leading `/`)"
    - "In-crate symlink pattern for workspace-root files (LICENSE/README) — Cargo silently ignores `../` traversal in include patterns; symlinks are the canonical workaround"
    - "Anchored include patterns (`/LICENSE-MIT` not `LICENSE-MIT`) to prevent accidental matches against same-named files deeper in the tree (e.g. `spikes/spike-a/README.md`)"
key_files:
  created:
    - "crates/bootroom/LICENSE-MIT (symlink -> ../../LICENSE-MIT)"
    - "crates/bootroom/LICENSE-APACHE (symlink -> ../../LICENSE-APACHE)"
    - "crates/bootroom/README.md (symlink -> ../../README.md)"
    - "crates/bootroom-core/LICENSE-MIT (symlink -> ../../LICENSE-MIT)"
    - "crates/bootroom-core/LICENSE-APACHE (symlink -> ../../LICENSE-APACHE)"
    - "crates/bootroom-core/README.md (symlink -> ../../README.md)"
  modified:
    - "crates/bootroom/Cargo.toml (added include allow-list + 6 publish-metadata keys)"
    - "crates/bootroom-core/Cargo.toml (added include allow-list + 6 publish-metadata keys)"
decisions:
  - "Used in-crate symlinks for LICENSE-MIT, LICENSE-APACHE, and README.md instead of `../../` paths in include — cargo 1.90 silently drops `../`-traversing include entries. Symlinks preserve the workspace-root single-source-of-truth (06-01 invariant) while satisfying cargo's in-package allow-list rule."
  - "Anchored include patterns with leading `/` (e.g. `/LICENSE-MIT`) to prevent gitignore-style at-any-depth matching from silently slurping `spikes/spike-a/README.md` into the bootroom tarball. Without anchoring, the bare pattern `README.md` matched two extra files."
  - "Kept the README in-crate symlink rather than dropping `readme` from include — the `readme` key alone gets the file into the tarball, but having it in the include list documents intent and makes the allow-list self-consistent (each tarball file traces back to an include entry)."
  - "Listed each of the 7 qemu-wasm asset files individually rather than a glob like `assets/qemu/**` — mirrors build.rs's REQUIRED constant exactly, and prevents stray files (e.g. `REBUILD.md` which IS present in `crates/bootroom/assets/qemu/`) from leaking into the published tarball."
metrics:
  duration_seconds: 222
  duration_human: "3 min 42 sec"
  tasks_completed: "3 / 3"
  files_modified: 2
  files_created: 6
  commits: 3
  completed_date: "2026-05-19"
---

# Phase 6 Plan 2: Cargo Package Metadata + Include Allow-List Summary

Pinned the publish surface of both crates with explicit `[package].include` allow-lists and finished the crates.io publish metadata — `cargo publish` cannot silently drop a qemu-wasm artifact or a license file, and `cargo package --list` confirms a precise file set with no leakage of tests/, spikes/, or stray asset directory entries.

## What was done

### Task 1: bootroom Cargo.toml — include + publish metadata

Added to `crates/bootroom/Cargo.toml` `[package]` block:

```toml
homepage = "https://github.com/sandwich-farm/bootroom"
documentation = "https://docs.rs/bootroom"
readme = "README.md"
keywords = ["qemu", "wasm", "riscv", "test-harness", "kernel"]
categories = ["development-tools::testing", "wasm", "command-line-utilities"]
publish = true
include = [
    "src/**/*.rs",
    "build.rs",
    "web/**/*",
    "assets/qemu/qemu-system-riscv64.wasm",
    "assets/qemu/qemu-system-riscv64.worker.js",
    "assets/qemu/qemu-system-riscv64.data",
    "assets/qemu/out.js",
    "assets/qemu/load.js",
    "assets/qemu/module.js",
    "assets/qemu/qemu-wasm-rev.txt",
    "/LICENSE-MIT",
    "/LICENSE-APACHE",
    "/README.md",
]
```

### Task 2: bootroom-core Cargo.toml — include + publish metadata

Added to `crates/bootroom-core/Cargo.toml` `[package]` block:

```toml
homepage = "https://github.com/sandwich-farm/bootroom"
documentation = "https://docs.rs/bootroom-core"
readme = "README.md"
keywords = ["bootroom", "qemu", "wasm", "riscv", "protocol"]
categories = ["development-tools::testing", "wasm", "data-structures"]
publish = true
include = [
    "src/**/*.rs",
    "/LICENSE-MIT",
    "/LICENSE-APACHE",
    "/README.md",
]
```

### Task 3: Verified cargo package --list output

Output captured below — both tarballs contain exactly the expected file set; no stray entries.

## cargo-package-list

### `cargo package --list -p bootroom --allow-dirty --no-verify`

```
.cargo_vcs_info.json
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
assets/qemu/load.js
assets/qemu/module.js
assets/qemu/out.js
assets/qemu/qemu-system-riscv64.data
assets/qemu/qemu-system-riscv64.wasm
assets/qemu/qemu-system-riscv64.worker.js
assets/qemu/qemu-wasm-rev.txt
build.rs
src/api_config.rs
src/assets.rs
src/check_cmd.rs
src/cli.rs
src/doctor_cmd.rs
src/embed.rs
src/headers.rs
src/init_cmd.rs
src/kernel_info.rs
src/kernel_stream.rs
src/lib.rs
src/main.rs
src/run_cmd.rs
src/server.rs
src/state.rs
src/transcript.rs
src/verbose.rs
src/watcher.rs
src/ws.rs
web/app.js
web/funnel.js
web/index.html
web/scenario.js
web/style.css
web/vendor/LICENSES.md
web/vendor/VERSIONS.md
web/vendor/xterm-pty.js
web/vendor/xterm.css
web/vendor/xterm.js
```

Verification table:

| Plan requirement | Status |
|---|---|
| All 7 REQUIRED qemu-wasm assets present | OK (load.js, module.js, out.js, qemu-system-riscv64.{wasm,worker.js,data}, qemu-wasm-rev.txt) |
| Full `web/**/*` tree present | OK (10 files: app.js, funnel.js, index.html, scenario.js, style.css, vendor/{LICENSES.md, VERSIONS.md, xterm-pty.js, xterm.css, xterm.js}) |
| build.rs present | OK |
| Entire `src/` tree present | OK (20 .rs files) |
| LICENSE-MIT, LICENSE-APACHE, README.md present | OK (all 3) |
| No `tests/` entries | OK (verified by grep — none) |
| No `spikes/` entries | OK (anchored `/README.md` prevented `spikes/spike-a/README.md` from leaking) |
| No `target/` entries | OK |
| `assets/qemu/REBUILD.md` excluded | OK (allow-list lists each asset individually) |

### `cargo package --list -p bootroom-core --allow-dirty --no-verify`

```
.cargo_vcs_info.json
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/config.rs
src/escape.rs
src/lib.rs
```

Verification table:

| Plan requirement | Status |
|---|---|
| `src/**/*.rs` present | OK (lib.rs, config.rs, escape.rs) |
| LICENSE-MIT, LICENSE-APACHE, README.md present | OK |
| No `tests/` entries | OK |
| No `assets/` entries (correct — bootroom-core has no assets) | OK |
| No stray files | OK (10 total entries; the 4 cargo-generated + 3 src + 3 root files) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Cargo silently drops `../` paths from include**

- **Found during:** Task 3 (initial `cargo package --list` showed LICENSE-MIT and LICENSE-APACHE missing)
- **Issue:** The plan specified `../../LICENSE-MIT`, `../../LICENSE-APACHE`, `../../README.md` in each crate's `include` list. Cargo 1.90 silently ignores include entries that traverse outside the package directory — no warning, no error, the files just don't appear in the tarball. The plan author had not anticipated this Cargo constraint (which is documented but easy to miss). README.md happened to appear in the tarball anyway because the `readme = "../../README.md"` key has special handling that copies the file independently of the include list, but no equivalent mechanism exists for licenses.
- **Fix:** Created in-crate symlinks (`crates/bootroom/LICENSE-MIT -> ../../LICENSE-MIT` etc.) for both crates, so the files are accessible from within each crate's manifest directory while preserving the workspace-root single-source-of-truth (06-01's invariant). Updated the include patterns to `/LICENSE-MIT`, `/LICENSE-APACHE`, `/README.md` (anchored to the manifest root) and `readme = "README.md"`. Cargo follows symlinks when packaging, so the tarball receives the full file contents at the basename, not a symlink.
- **Files modified:** `crates/bootroom/Cargo.toml`, `crates/bootroom-core/Cargo.toml`
- **Files created:** `crates/bootroom/LICENSE-MIT`, `crates/bootroom/LICENSE-APACHE`, `crates/bootroom/README.md`, `crates/bootroom-core/LICENSE-MIT`, `crates/bootroom-core/LICENSE-APACHE`, `crates/bootroom-core/README.md` (all symlinks)
- **Commit:** `cdc18ab`

**2. [Rule 2 - Critical correctness] Unanchored gitignore patterns matched files outside intent**

- **Found during:** Task 3 (after the symlink fix, `cargo package --list -p bootroom` initially included `spikes/spike-a/README.md`)
- **Issue:** Cargo's `include` uses gitignore-style globs. A pattern like `README.md` (no leading `/`) matches a file with that basename at ANY directory depth — including `spikes/spike-a/README.md`. spike-a is not a workspace member, so its files would have leaked into the published bootroom tarball.
- **Fix:** Anchored the workspace-root file patterns with a leading `/`: `/LICENSE-MIT`, `/LICENSE-APACHE`, `/README.md`. After this change, the patterns match only the symlinks at the manifest root.
- **Files modified:** `crates/bootroom/Cargo.toml`, `crates/bootroom-core/Cargo.toml`
- **Commit:** `cdc18ab` (combined with deviation 1)

Both deviations were resolved without modifying the plan's intent — the goal was an exact allow-list with the LICENSE and README files in the tarball, which is now achieved.

## Notes on metadata key choices (per-key justification, not in the manifest comments)

- **`homepage`** — references the canonical `https://github.com/sandwich-farm/bootroom` URL (matches `[workspace.package].repository`). If the GitHub repo is renamed before publish, a single Cargo.toml edit at rename time corrects both keys; threat T-06-02-04 explicitly accepts this risk.
- **`documentation`** — explicit `docs.rs/<crate>` URL. crates.io auto-generates docs.rs links, but setting the key explicitly is the documented convention.
- **`readme = "README.md"`** — points to the in-crate symlink, which resolves to the workspace-root README.
- **`keywords`** (bootroom): 5 lowercase ASCII tokens within crates.io's documented constraints (max 5, max 20 chars each, `[a-z0-9_-]`). Chosen for discoverability — leads with the WHAT (`qemu`, `wasm`, `riscv`) then the INTENT (`test-harness`, `kernel`).
- **`keywords`** (bootroom-core): leads with `bootroom` to make the relationship to the main crate discoverable in crates.io search.
- **`categories`**: all listed slugs (`development-tools::testing`, `wasm`, `command-line-utilities`, `data-structures`) are valid crates.io categories as of 2026.
- **`publish = true`** — explicit; default is also `true`, but the explicit form makes any future `publish = false` regression visible in diffs.
- **`include`** — listed each of the 7 qemu-wasm assets individually rather than using `assets/qemu/**`. This mirrors `crates/bootroom/build.rs`'s REQUIRED constant exactly, and ensures stray files in `assets/qemu/` (currently `REBUILD.md`, possibly others later) stay out of the tarball.

## Threat surface scan

No new threat-relevant surface introduced beyond what the plan's threat model already covers.

## Self-Check: PASSED

Files asserted in this SUMMARY:
- `crates/bootroom/Cargo.toml` — FOUND (modified, contains `include = [`)
- `crates/bootroom-core/Cargo.toml` — FOUND (modified, contains `include = [`)
- `crates/bootroom/LICENSE-MIT` — FOUND (symlink)
- `crates/bootroom/LICENSE-APACHE` — FOUND (symlink)
- `crates/bootroom/README.md` — FOUND (symlink)
- `crates/bootroom-core/LICENSE-MIT` — FOUND (symlink)
- `crates/bootroom-core/LICENSE-APACHE` — FOUND (symlink)
- `crates/bootroom-core/README.md` — FOUND (symlink)

Commits asserted:
- `2c772ee` — FOUND
- `92d9220` — FOUND
- `cdc18ab` — FOUND
