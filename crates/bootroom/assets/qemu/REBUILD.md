# Rebuilding the qemu-wasm artifacts

The files in this directory are produced by the qemu-wasm submodule's docker
build. They are committed to git so end users do NOT need docker installed
to `cargo build` bootroom. Per CONTEXT.md decision D-02.

## When to rebuild

- You bumped the `qemu-wasm` submodule pinned commit in `.gitmodules`.
- Upstream qemu-wasm fixed a bug you want.
- You changed the QEMU argv in `module.js` and want to verify boot.

## Prerequisites

- `docker` 20.10+ (verified 2026-05-17 on docker 29.4.3).
- `make` (GNU Make 4.x).
- The `qemu-wasm` git submodule checked out: `git submodule update --init --recursive`.
- Free disk: at least 10 GB for the docker build context, emsdk image
  (~3 GB), and intermediate layers.
- Time: 15–30 minutes on first run (cold emsdk + glib + pixman + ffi compile);
  ~5 minutes on warm builds.

## Procedure

From the repo root:

    make qemu-assets

This runs the qemu-wasm/examples/riscv64 docker build, packages the
preload pack (kernel `Image` + opensbi BIOS), then copies the output
files into `crates/bootroom/assets/qemu/`. After it completes:

    git status crates/bootroom/assets/qemu/
    git add crates/bootroom/assets/qemu/
    git commit -m "build: bump qemu-wasm artifacts to <submodule-sha>"

The submodule SHA is `git -C qemu-wasm rev-parse HEAD`.

## What ends up committed

| File                            | Approx size | What it is |
|---------------------------------|-------------|------------|
| `qemu-system-riscv64.wasm`      | 10–30 MB    | The QEMU RISC-V system emulator compiled to WebAssembly. |
| `qemu-system-riscv64.worker.js` | <100 KB     | Emscripten pthread worker shim. |
| `qemu-system-riscv64.data`      | 5–20 MB     | Emscripten preload pack containing the `/pack/` filesystem with the `Image` slot. |
| `out.js`                        | 100–500 KB  | Emscripten JS glue + `initEmscriptenModule` default export. |
| `load.js`                       | <5 KB       | Classic script emitted by `file_packager.py` that primes the page before the module loads. |
| `module.js`                     | <2 KB       | **bootroom-authored** QEMU argv (NOT generated; `make qemu-assets` does not overwrite it). |

## Special handling: `module.js`

`module.js` is bootroom-authored, not produced by the docker build. The
Makefile copies the other five files; `module.js` stays put. If you need
to change the QEMU argv (e.g., add a `-drive` for a rootfs), edit
`module.js` directly and commit. The current Phase 1 argv drops the
reference example's `-drive if=virtio,format=raw,file=/pack/rootfs.bin`
because Phase 1 boots a bare kernel only.

## Validation

After commit, sanity check on a clean checkout:

    cargo clean
    cargo build --workspace

If you forgot to commit any file, `build.rs` will tell you which one with
a friendly error:

    error: qemu-wasm assets missing. Run 'make qemu-assets' from the repo root.
    error: (missing file: assets/qemu/qemu-system-riscv64.wasm)

## Escape hatch for unrelated dev work

If you need to `cargo build` against unrelated changes before the docker
build has finished, set the env var documented inside `build.rs`:

    BOOTROOM_SKIP_QEMU_ASSET_CHECK=1 cargo build --workspace

The resulting binary will not run — it has no qemu artifacts to embed —
but compilation succeeds. Never set this in CI or release builds.

## What NOT to do

- Do not run docker from `build.rs`. (Pitfall 9.)
- Do not check in any docker intermediate state.
- Do not switch this directory to Git LFS without team consensus —
  `cargo install bootroom` users would need git-lfs installed.
- Do not compress the wasm file. The browser caches it; the binary
  compresses fine at the OS layer if needed for distribution.
