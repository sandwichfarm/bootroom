//! WCH-02: atomic-rename safety. `make` typically writes a sibling
//! tempfile and renames over the target; the rename event lands on
//! the parent dir, which the watcher watches. The resulting
//! `KernelChanged` carries the FINAL bytes.

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tempfile_rename_fires_kernel_changed() {
    let (state, tmp, kernel_path, _cfg) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Write a NEW 64-byte ELF stub to a sibling tempfile, then rename
    // atomically over the kernel path — same pattern as `make`.
    let sibling = tmp.path().join("Image.tmp");
    std::fs::write(&sibling, common::ELF_STUB_64).expect("write sibling");
    std::fs::rename(&sibling, &kernel_path).expect("rename atomically");

    // Wait for the watcher to debounce + run gates.
    let deadline = Duration::from_secs(2);
    let recv = tokio::time::timeout(deadline, rx.recv()).await;
    let msg = recv
        .expect("KernelChanged within 2s")
        .expect("broadcast recv");

    match msg {
        WsMessage::KernelChanged {
            ok,
            size,
            reason,
            sha256_prefix,
            ..
        } => {
            assert!(ok, "atomic rename of valid ELF stub must yield ok=true; reason: {reason:?}");
            assert_eq!(size, 64, "must reflect the renamed-in bytes");
            assert_eq!(sha256_prefix.len(), 12, "sha256_prefix = 12 hex chars");
            assert!(reason.is_none(), "ok=true frame must have reason=None");
        }
        other => panic!("expected KernelChanged, got: {other:?}"),
    }
}
