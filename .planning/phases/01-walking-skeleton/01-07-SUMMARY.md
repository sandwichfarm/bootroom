---
phase: 01
plan: 07
slug: integration-tests-and-smoke
status: complete
requirements:
  - DIST-01
  - SERV-01
  - SERV-02
  - SERV-03
  - SERV-04
  - SERV-05
  - UI-01
  - UI-05
  - UI-07
  - CLI-03
date: 2026-05-17
---

# Plan 01-07 — Integration Tests + Manual Headed-Browser Smoke

## Outcome

All 10 Phase 1 requirements are now exercised by automated tests AND a real headed-browser smoke against the live NORN kernel.

## Tasks

| # | Task | Commit | Status |
|---|------|--------|--------|
| 1 | Common test helper + dev-deps | `bc38d77` | ✅ |
| 2 | SERV-01, SERV-02, SERV-05 integration tests (6 tests) | `e2cbe20` | ✅ |
| 3 | SERV-03, SERV-04, UI-07 integration tests (8 tests) | `38eb9d3` | ✅ |
| 4 | Full suite + include_dir leak check + CLI-03 | (verify-only) | ✅ |
| 5 | Headed-browser human-verify checkpoint | (manual + headless reproduce) | ✅ |

## Verification — automated

- `cargo test --workspace` → 30 tests pass (16 lib unit + 14 integration), 0 failures.
- `include_dir!` build-host path leak: only standard rustc panic-source paths in `target/release/bootroom`; embedded asset paths do NOT leak. Non-blocking; Phase 2 could add `--remap-path-prefix` if size or privacy matters.
- `CLI-03`: `bootroom serve --kernel /tmp/fixture` is a complete invocation; binds, prints URL, stays up. No multi-line invocation needed.
- Binary size: 54 MB at `target/release/bootroom` (41.7 MB qemu wasm + ~11 MB Rust debug strings).

## Verification — manual headed-browser smoke

Run live against the user's actual NORN kernel:
```
./target/release/bootroom serve --kernel /home/sandwich/Develop/nostros/target/riscv64gc-unknown-none-elf/release/norn-kernel
```

Two real defects surfaced and were fixed before the smoke passed:

1. **emscripten asset-path resolution** — `load.js` fetched `qemu-system-riscv64.data` as a bare relative URL, resolving to `/qemu-system-riscv64.data` (404) instead of `/assets/qemu/...`. **Fix:** inline `Module.locateFile = (p) => '/assets/qemu/' + p` in `index.html` BEFORE `load.js` (commit `04a31fa`).
2. **`/pack/Image` EEXIST collision** — our preRun callback wrote `/pack/Image` first because emscripten's `addOnPreRun` uses `unshift` (reversing FIFO). The data pack then hit `FS.mayCreate` → errno 20 (EEXIST in musl). **Fix:** move kernel injection from `Module.preRun` → `Module.onRuntimeInitialized`, using `FS_unlink + FS_createDataFile` since Module.FS isn't publicly exposed on this build (commit `04a31fa`).

Plus a UX polish via `/gsd-quick`:
3. **Viewport / scrollbars** — terminal didn't fill the viewport; long kernel paths produced per-`dd` horizontal scrollbars; body grew past 100vh. Fixed via CSS (`height: 100vh + overflow: hidden`, ellipsis on `.kinfo dd`, force xterm to fill `#terminal`) and a real `fitTerminalToContainer` that resizes xterm's cell grid to the container (commit `82d14f0`).

After fixes, automated headless probe at 1280x800 viewport:
- Header: 41 px tall
- Terminal: 759 px tall, full width
- Header + Terminal = 800 px (exact viewport fit)
- No scrollbars on `body` or `html` (axis-checked)
- NORN kernel boot banner streams into xterm: `[NORN ISA] base=rv64 …`, `[NORN PMP] region count = 16`, `[NORN PAGING] sv39 active satp=…`, `[NORN SCHED] timebase=… Hz`, etc.

User confirmed the visual smoke before requesting `/gsd-progress`.

## Notable decisions

- **Spike B (01-08) not yet run** — UI-01's "boots qemu-system-riscv64.wasm" is verified manually via real browser + headless playwright. The automated `chromiumoxide` story lands in plan 01-08.
- **Build-host path leak** ruled non-blocking; left as a Phase-2 polish opportunity.

## Files modified (cumulative for plan)

- `crates/bootroom/Cargo.toml` (dev-deps + bin sections)
- `crates/bootroom/tests/common/mod.rs`
- `crates/bootroom/tests/serve_binds.rs`
- `crates/bootroom/tests/coop_coep_headers.rs`
- `crates/bootroom/tests/port_host_flags.rs`
- `crates/bootroom/tests/embedded_assets_served.rs`
- `crates/bootroom/tests/assets_dir_override.rs`
- `crates/bootroom/tests/kernel_info_endpoint.rs`
- Plus hotfix commits to `crates/bootroom/web/{index.html,app.js,style.css}` from the smoke test
