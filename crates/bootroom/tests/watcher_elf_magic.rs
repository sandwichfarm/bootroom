//! WCH-04: non-ELF content in the kernel path produces
//! `KernelChanged { ok: false, reason: Some("not ELF") }`. The browser
//! shows a warning banner; the kernel is never accidentally "live".

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_elf_yields_ok_false() {
    let (state, _tmp, kernel_path, _cfg) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Operator accidentally points --kernel at a shell script.
    let bad = b"#!/bin/sh\necho hi\n";
    std::fs::write(&kernel_path, bad).expect("write non-ELF");

    let msg = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("KernelChanged within 2s")
        .expect("broadcast recv");

    match msg {
        WsMessage::KernelChanged {
            ok,
            reason,
            size,
            ..
        } => {
            assert!(!ok, "non-ELF must yield ok=false");
            assert_eq!(reason.as_deref(), Some("not ELF"));
            assert_eq!(
                usize::try_from(size).unwrap(),
                bad.len(),
                "size in non-ELF frame must still reflect the file size"
            );
        }
        other => panic!("expected KernelChanged, got: {other:?}"),
    }
}
