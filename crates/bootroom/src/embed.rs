//! Compile-time embeds of the UI (`web/`) and qemu-wasm artifacts
//! (`assets/qemu/`). The `--assets-dir` flag (handled in
//! `assets.rs`, plan 01-05) overrides these at request time.

use include_dir::{Dir, include_dir};

pub static WEB: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/web");
pub static QEMU: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/qemu");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_qemu_dir_has_wasm() {
        // Plan 01-02 guarantees this file is committed; build.rs validates at compile.
        assert!(
            QEMU.get_file("qemu-system-riscv64.wasm").is_some(),
            "qemu-wasm artifact must be committed; if you see this in tests, run 'make qemu-assets'"
        );
    }
}
