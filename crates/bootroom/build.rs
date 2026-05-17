//! Build script for the bootroom binary.
//!
//! Phase 1: validates that the qemu-wasm artifacts (built once via
//! `make qemu-assets` and committed to git per CONTEXT.md D-02) are
//! present. Does NOT invoke docker — that would be Pitfall 9.
//!
//! Escape hatch: set `BOOTROOM_SKIP_QEMU_ASSET_CHECK=1` to bypass the
//! presence check. Intended for short-lived dev scenarios (e.g. iterating
//! on unrelated crate code before the qemu artifacts have been built).
//! Do NOT enable in CI or release builds — the resulting binary will
//! crash at runtime when it tries to load the missing assets.

use std::path::Path;

const REQUIRED: &[&str] = &[
    "assets/qemu/qemu-system-riscv64.wasm",
    "assets/qemu/qemu-system-riscv64.worker.js",
    "assets/qemu/qemu-system-riscv64.data",
    "assets/qemu/out.js",
    "assets/qemu/load.js",
    "assets/qemu/module.js",
];

fn main() {
    // WR-04: emit one rerun-if-changed per required file. The previous
    // directory-only watch (`cargo:rerun-if-changed=assets/qemu`) only
    // fires on entry add/remove on most filesystems; editing a file in
    // place (e.g. updating module.js, or `cp`-overwriting the .wasm
    // without `rm` first) did NOT trigger a rebuild and the binary
    // ended up with stale embedded bytes.
    for rel in REQUIRED {
        println!("cargo:rerun-if-changed={rel}");
    }
    // Keep the directory-level watch too so adding NEW files (e.g. a
    // future module-side helper script) re-triggers without having to
    // edit REQUIRED first.
    println!("cargo:rerun-if-changed=assets/qemu");
    // Also watch the embedded web/ tree: include_dir!("…/web") captures
    // its contents at compile time but cargo otherwise only invalidates
    // on Cargo.toml/package changes. Today web/ files are part of the
    // crate package so the practical impact is small, but the watch
    // makes the dependency explicit.
    println!("cargo:rerun-if-changed=web");
    // Re-run whenever the escape-hatch env var flips.
    println!("cargo:rerun-if-env-changed=BOOTROOM_SKIP_QEMU_ASSET_CHECK");

    if std::env::var("BOOTROOM_SKIP_QEMU_ASSET_CHECK").is_ok() {
        println!(
            "cargo:warning=BOOTROOM_SKIP_QEMU_ASSET_CHECK is set; skipping qemu-wasm asset presence check. The resulting binary will NOT work until 'make qemu-assets' is run."
        );
        return;
    }

    for rel in REQUIRED {
        let p = Path::new(rel);
        if !p.exists() {
            // Emit the exact required error message per plan 01-02.
            // We print to stderr and exit non-zero; cargo surfaces both.
            // (Older cargo versions silently swallow `cargo::error=`; this
            // form is portable across the MSRV-1.85+ toolchain range.)
            eprintln!(
                "error: qemu-wasm assets missing. Run 'make qemu-assets' from the repo root."
            );
            eprintln!("error: (missing file: {rel})");
            eprintln!(
                "error: to bypass for dev work on unrelated code, set BOOTROOM_SKIP_QEMU_ASSET_CHECK=1"
            );
            std::process::exit(1);
        }
    }
}
