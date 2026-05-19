//! WCH-05: validate the full shape of the `KernelChanged` frame for a
//! valid ELF stub — ok=true, mtime > 0, size=64, `sha256_prefix` length
//! == 12 hex chars, reason=None.

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kernel_changed_payload_shape() {
    let (state, _tmp, kernel_path, _cfg) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Overwrite with a fresh 64-byte ELF stub (the setup already wrote
    // one, but no watcher was attached at that point — re-write to trigger).
    std::fs::write(&kernel_path, common::ELF_STUB_64).expect("rewrite stub");

    let msg = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("KernelChanged within 2s")
        .expect("broadcast recv");

    match msg {
        WsMessage::KernelChanged {
            ok,
            mtime,
            size,
            sha256_prefix,
            reason,
        } => {
            assert!(ok, "valid ELF stub must yield ok=true; reason: {reason:?}");
            assert!(mtime > 0, "mtime must be a recent epoch second (got {mtime})");
            assert_eq!(size, 64, "size must reflect the 64-byte stub");
            assert_eq!(
                sha256_prefix.len(),
                12,
                "sha256_prefix is exactly 12 hex chars (first 6 bytes)"
            );
            assert!(
                sha256_prefix.chars().all(|c| c.is_ascii_hexdigit()),
                "sha256_prefix must be hex-ascii, got: {sha256_prefix}"
            );
            assert!(reason.is_none(), "ok=true frame must have reason=None");
        }
        other => panic!("expected KernelChanged, got: {other:?}"),
    }
}
