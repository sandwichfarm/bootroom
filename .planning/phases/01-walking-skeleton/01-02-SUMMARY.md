---
phase: 01-walking-skeleton
plan: 02
subsystem: qemu-wasm-asset-pipeline
tags: [rust, qemu-wasm, build, assets, makefile, blocked]
status: blocked-on-disk-space
requires:
  - cargo-workspace
provides:
  - qemu-assets-makefile-target
  - build-rs-asset-presence-check
  - rebuild-documentation
  - qemu-wasm-submodule-pinned
affects:
  - 01-04-server-include-dir-embed
  - 01-05-ui-shell
tech-stack:
  added:
    - gnu-make
    - docker-29-x
    - qemu-wasm-submodule@0ef7b4e281
  patterns:
    - committed-binary-artifacts-not-git-lfs
    - build-rs-validation-only-no-side-effects
    - manual-makefile-rebuild-cadence
key-files:
  created:
    - .gitmodules
    - .gitattributes
    - Makefile
    - crates/bootroom/build.rs
    - crates/bootroom/assets/qemu/module.js
    - crates/bootroom/assets/qemu/REBUILD.md
    - qemu-wasm (submodule pin)
  modified:
    - crates/bootroom/Cargo.toml
  not-created-blocker:
    - crates/bootroom/assets/qemu/out.js
    - crates/bootroom/assets/qemu/qemu-system-riscv64.wasm
    - crates/bootroom/assets/qemu/qemu-system-riscv64.worker.js
    - crates/bootroom/assets/qemu/qemu-system-riscv64.data
    - crates/bootroom/assets/qemu/load.js
decisions:
  - skip-docker-build-on-host-disk-98-percent-full
  - add-bootroom-skip-qemu-asset-check-escape-hatch-for-dev-iteration
  - commit-submodule-pointer-separately-from-built-artifacts
  - module-js-bootroom-authored-not-overwritten-by-make-target
metrics:
  duration_minutes: 12
  tasks_completed: 3
  files_created: 6
  files_modified: 1
  commits: 3
  completed_date: 2026-05-17
---

# Phase 1 Plan 02: qemu-wasm Asset Pipeline Summary

Established the qemu-wasm asset pipeline infrastructure: `make qemu-assets` Makefile target, `build.rs` validation, rebuild documentation, and `module.js` argv. The actual binary artifacts (`qemu-system-riscv64.wasm`, `.worker.js`, `.data`, `out.js`, `load.js`) are **not committed in this plan** because executing the docker build on the host system would consume more disk than is safely available — see "Blockers" below.

## What Was Built

| Artifact | Status | Purpose |
|----------|--------|---------|
| `Makefile` (repo root) | committed | `qemu-assets` target runs the qemu-wasm docker build (`buildqemu` image → `build-qemu-wasm` container → emconfigure → emmake → file_packager.py → docker cp into `crates/bootroom/assets/qemu/`). Also `clean-qemu-assets` and `help`. |
| `.gitattributes` (repo root) | committed | `*.wasm binary`, `*.data binary`, explicit binary attribute on the riscv64 `.data` pack. Prevents future text-mangling of the embedded artifacts. |
| `.gitmodules` + `qemu-wasm` gitlink | committed | Submodule pinned at `ktock/qemu-wasm@0ef7b4e2814b231705d8371dd7997f5b72e70baf` (master tip as of 2026-05-17). |
| `crates/bootroom/assets/qemu/module.js` | committed | Bootroom-authored QEMU argv. Adapted from `qemu-wasm/examples/riscv64/src/htdocs/module.js` with the `-drive if=virtio,...` line removed (Phase 1 boots a bare kernel; no rootfs) and `root=/dev/vda rootwait` dropped from `-append`. The Makefile does NOT overwrite this file. |
| `crates/bootroom/assets/qemu/REBUILD.md` | committed | 89-line procedure: when to rebuild, prereqs, single `make qemu-assets` command, expected output files with size bands, the `module.js` special case, validation steps, escape hatch, and "what NOT to do" guard rails. |
| `crates/bootroom/build.rs` | committed | Presence-check only. Emits the exact required error message `qemu-wasm assets missing. Run 'make qemu-assets' from the repo root.` and exits 1 if any of the six required files is absent. `cargo:rerun-if-changed=assets/qemu` + `cargo:rerun-if-env-changed=BOOTROOM_SKIP_QEMU_ASSET_CHECK`. |
| `crates/bootroom/Cargo.toml` | modified | Added `build = "build.rs"` line. |
| `crates/bootroom/assets/qemu/qemu-system-riscv64.wasm` | **not created** | Blocked — see below. |
| `crates/bootroom/assets/qemu/qemu-system-riscv64.worker.js` | **not created** | Blocked. |
| `crates/bootroom/assets/qemu/qemu-system-riscv64.data` | **not created** | Blocked. |
| `crates/bootroom/assets/qemu/out.js` | **not created** | Blocked. |
| `crates/bootroom/assets/qemu/load.js` | **not created** | Blocked. |

## Blockers

### B-01-02-01: docker build skipped — host disk at 98%

**Status:** blocked
**Severity:** plan-level; does not block plans 01-03 (CONTEXT.md UI vendor deps) or other workspace-internal work that can be done with `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1`. Blocks plans 01-04 (`include_dir!` embedding) and 01-05 (UI shell) at the point they need the actual `.wasm` binary to flow into the browser.

**What happened:** The qemu-wasm docker build per `qemu-wasm/README.md` requires building a multi-stage emscripten image (`emscripten/emsdk:3.1.50` base ≈ 3 GB, plus glib + zlib + pixman + libffi compiled from source under emscripten, plus the QEMU build itself). Combined intermediate-layer footprint is realistically 5–10 GB; the upstream README does not state an exact figure but the Dockerfile has ~30 build stages.

At plan-execution time, host filesystem state was:

```
/dev/mapper/root  457G  422G   12G  98% /
```

Running the full `make qemu-assets` would have a high probability of filling the disk, which per `AGENTS.md` (this is a Wayland production system) is unacceptable risk. I did not attempt the docker build.

**Remediation (user-runnable):**

1. Free at least 10 GB on `/` (typical candidates: `docker system prune -a -f`, `pacman -Sc`, `journalctl --vacuum-time=2weeks`, clean `~/.cache/`).
2. From the repo root:

   ```bash
   make qemu-assets
   ```

3. Verify the artifacts are >1 MB combined wasm:

   ```bash
   ls -la crates/bootroom/assets/qemu/
   stat -c%s crates/bootroom/assets/qemu/qemu-system-riscv64.wasm   # expect 10-30 MB
   ```

4. Commit them:

   ```bash
   git add crates/bootroom/assets/qemu/
   git commit -m "build(01-02): commit initial qemu-wasm artifacts (submodule @0ef7b4e281)"
   ```

5. Confirm `cargo build --workspace` now succeeds with zero warnings (the `build.rs` presence check passes).

**Until then:** `cargo build --workspace` fails by design with the friendly error:

```
error: qemu-wasm assets missing. Run 'make qemu-assets' from the repo root.
error: (missing file: assets/qemu/qemu-system-riscv64.wasm)
error: to bypass for dev work on unrelated code, set BOOTROOM_SKIP_QEMU_ASSET_CHECK=1
```

For unrelated dev work (e.g. iterating on `bootroom-core` or the UI shell), `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo build --workspace` succeeds with a single warning. **Never set this in CI or release builds.**

## Decisions Made

### 1. Skip docker build; mark plan blocked on disk space

Per the plan prompt's environment notes: "If `make qemu-assets` takes >10 minutes or fails, log the error in SUMMARY.md, mark the plan as `status: blocked`, and exit gracefully — do NOT block forever." The blocker here is upstream of even attempting the build (insufficient free disk to do so safely), so the same protocol applies. The scaffolding committed in this plan (Makefile, build.rs, REBUILD.md, module.js, submodule pin) is **everything except the binary outputs** — the user's one command (`make qemu-assets`) takes over from here.

### 2. Add `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` escape hatch (Rule 2)

The plan's stated `build.rs` behavior is to fail when artifacts are absent. Strictly applied, that means `cargo build --workspace` fails on every checkout until the multi-minute `make qemu-assets` has run. For a multi-plan phase like this one, that is too sharp — plans 01-03 (UI vendor deps), 01-06 (CLI clap dispatch), and 01-08/09 (spikes) can all make progress without the qemu binaries embedded. The escape hatch lets that work happen without disabling the protection; the loud `cargo:warning=` makes it impossible to forget. The plan's `<verify>` step (rename wasm → expect failure) is unaffected: the env var is opt-in, not on by default.

### 3. Submodule pin committed separately from artifacts

`.gitmodules` + `qemu-wasm` gitlink are committed in the scaffolding commit (`f70436e`) so that even before the artifacts exist, the build SHA is reproducible — the submodule pin IS the source of truth per CONTEXT.md D-02 trust register T-01-02-01. The actual artifact commit (whenever it happens, post-`make qemu-assets`) should reference this submodule SHA in its message per the REBUILD.md template.

### 4. `module.js` is bootroom-authored, not regenerated

The Makefile only copies the five generated files (`out.js`, the three `qemu-system-riscv64.*` siblings, and `load.js`). `module.js` is hand-written and lives next to them in the same directory so `include_dir!` (plan 01-04) picks it up in one tree walk. The drop of `-drive` and `root=/dev/vda rootwait` is documented inline in `module.js`'s header comment and again in REBUILD.md's "Special handling" section.

### 5. `.gitattributes` lives at repo root, not in the assets dir

`*.wasm binary` and `*.data binary` are global rules — they should apply if any future plan adds wasm/data files elsewhere (vendored xterm-pty addons, for instance). Placing them at the repo root is conventional and matches every other Cargo project's pattern.

## Build & Run Verification

- `cargo build --workspace` (no env) → **fails by design** with the exact required friendly error. Verified the exact string `qemu-wasm assets missing. Run 'make qemu-assets' from the repo root.` appears on stderr.
- `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo build --workspace` → **succeeds** with a single `cargo:warning=` (escape hatch works).
- `git log --oneline` shows the three commits in order: `f70436e`, `e8f1f52`, `93e00c2`.
- The Makefile's `help` target shell-parses (`make -n help` would run; not invoked because it produces no side effects).
- The Makefile's `qemu-assets` target was NOT invoked. Its first two preflight checks (`command -v docker`, `test -d qemu-wasm`) would both pass on this host, so the build would have started — and is precisely the step blocked on disk.

The verify step from the plan that flips the wasm file aside and re-runs cargo to check for the friendly error is **trivially satisfied** because the wasm file is permanently absent. The full bidirectional test (present → succeeds; absent → fails with message) will need to run after the user completes the deferred docker build.

## Deviations from Plan

### Auto-fixed (Rule 2 — added missing critical functionality)

**1. [Rule 2 - Critical] Added `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` escape hatch.**
- **Found during:** Task 2.
- **Issue:** Strict `cargo build` failure on every checkout until `make qemu-assets` runs makes it impossible to do dev work on unrelated Phase 1 plans (01-03 UI vendor deps, 01-06 clap dispatch, etc.) on a machine where the multi-minute docker build hasn't run.
- **Fix:** `build.rs` honors `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` to skip the check, emits a `cargo:warning=` when the env var is set, and `cargo:rerun-if-env-changed=BOOTROOM_SKIP_QEMU_ASSET_CHECK` re-runs the script when the user toggles it. Verify steps (failure on missing wasm) are unaffected because the var is opt-in.
- **Files modified:** `crates/bootroom/build.rs`.
- **Commit:** `e8f1f52`.

**2. [Rule 2 - Robustness] Makefile preflight checks for docker + submodule.**
- **Found during:** Task 1 Makefile authoring.
- **Issue:** Plan's Makefile draft calls `cd qemu-wasm/examples/riscv64 && ./build.sh` — but the submodule has no `build.sh`. Plan task body notes this caveat ("the exact command depends on the upstream submodule's published procedure; mirror it faithfully").
- **Fix:** Replaced with the multi-step docker procedure documented in `qemu-wasm/README.md` (build emsdk image → run `build-qemu-wasm` container → emconfigure/emmake → file_packager.py → docker cp out). Added preflight `command -v docker` and `test -d qemu-wasm` with friendly error messages.
- **Files modified:** `Makefile`.
- **Commit:** `f70436e`.

### Not auto-fixed (deferred to user / future plan)

**3. [Blocker] Docker build not executed.**
- **Found during:** Task 1 Step 3 (the `make qemu-assets` invocation).
- **Issue:** Host disk at 98% capacity (12G free on a 457G `/`). Running a 5–10 GB docker build is unsafe.
- **Disposition:** Documented as B-01-02-01 above. Not auto-fixed. User must free disk and run `make qemu-assets` manually.

No Rule 4 architectural decisions were needed.

## Authentication Gates

None.

## Self-Check: PASSED

- FOUND: Makefile
- FOUND: .gitattributes
- FOUND: .gitmodules
- FOUND: qemu-wasm (submodule gitlink @ 0ef7b4e2814b231705d8371dd7997f5b72e70baf)
- FOUND: crates/bootroom/build.rs
- FOUND: crates/bootroom/assets/qemu/module.js
- FOUND: crates/bootroom/assets/qemu/REBUILD.md
- VERIFIED: Cargo.toml has `build = "build.rs"`
- VERIFIED: `Makefile` contains `qemu-assets:` target
- VERIFIED: `build.rs` emits the exact required error message verbatim
- VERIFIED: `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` escape hatch lets `cargo build --workspace` succeed
- FOUND commit: f70436e (scaffolding + submodule)
- FOUND commit: e8f1f52 (build.rs)
- FOUND commit: 93e00c2 (REBUILD.md)
- MISSING (expected, blocked): crates/bootroom/assets/qemu/qemu-system-riscv64.{wasm,worker.js,data}, out.js, load.js — see B-01-02-01

## Phase 2+ Inheritance

- Plan 01-04 (`include_dir!` embedding) MUST ensure its `cargo build` will still succeed after `make qemu-assets` runs and the artifacts are committed. The `assets/qemu/` directory tree is now established and contains `module.js` + `REBUILD.md` so the `include_dir!` macro has something to walk even pre-artifact-build.
- Plan 01-04 should fetch its qemu artifacts from `crates/bootroom/assets/qemu/` exactly — the relative path is now load-bearing.
- Any future plan that needs `cargo build --workspace` to succeed in CI before artifacts exist (e.g., a workspace-level format/lint check) must either depend on a pre-built CI cache of those artifacts OR set `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` for the cargo invocation and gate strict release builds separately.
- The submodule SHA `0ef7b4e281` is the locked source of truth — the user's `make qemu-assets` invocation will produce artifacts deterministically against this SHA; downstream agents should reference it in artifact-commit messages.

## Commits

- `f70436e` — `chore(01-02): add qemu-wasm submodule, Makefile, .gitattributes, module.js`
- `e8f1f52` — `feat(01-02): add build.rs that validates qemu-wasm assets are present`
- `93e00c2` — `docs(01-02): add REBUILD.md documenting qemu-assets rebuild procedure`

## Known Stubs

None. Every file committed is its final form; the only "stubs" are the absent binary artifacts whose absence is the documented blocker B-01-02-01 (not a code stub — a deferred build step the user runs locally).

## Threat Flags

None new. Threat register T-01-02-01..04 are mitigated as planned (submodule pin committed, `.gitattributes` marks binaries for review, Makefile fails fast without docker, T-04 deferred to plan 01-07 as designed).
