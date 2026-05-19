//! WCH-01: a burst of rapid writes to the kernel collapses to exactly
//! ONE `KernelChanged` broadcast within ~1s (debounce verified).

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::{io::Write, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn burst_collapses_to_one_event() {
    let (state, _tmp, kernel_path, _cfg) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");

    // Give the watcher its OS thread a moment to attach.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5 rapid writes — append a byte each time and close immediately.
    for _ in 0..5 {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&kernel_path)
            .expect("open append");
        f.write_all(&[0u8]).expect("append byte");
        // No sleep between writes — we want them all inside one debounce window.
    }

    // Wait long enough for debounce (300ms) + size-stability (100ms) +
    // generous slack to ensure any second event would have fired too.
    tokio::time::sleep(Duration::from_millis(1_000)).await;

    // Drain the receiver and count KernelChanged frames.
    let mut kernel_changed_count = 0;
    loop {
        match rx.try_recv() {
            Ok(WsMessage::KernelChanged { .. }) => kernel_changed_count += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }

    assert_eq!(
        kernel_changed_count, 1,
        "burst of 5 writes within debounce window must collapse to exactly 1 KernelChanged"
    );
}
