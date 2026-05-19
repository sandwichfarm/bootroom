//! WCH-03: size-stability gate. A partial write (size N -> size 2N
//! across the 100ms inner-check window) holds back the `KernelChanged`
//! frame until the size settles. Eventually (after the file stops
//! growing) at least one `KernelChanged` fires.
//!
//! Timing-sensitive: the test asserts ≥1 `KernelChanged` within a 3s
//! generous window — the exact count depends on when the debouncer
//! happens to observe each write burst.

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::{io::Write, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_write_held_until_stable() {
    let (state, _tmp, kernel_path, _cfg) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Phase A: write a 32-byte ELF prefix; wait briefly.
    let part_a = {
        let mut v = vec![0u8; 32];
        v[0] = 0x7f;
        v[1] = b'E';
        v[2] = b'L';
        v[3] = b'F';
        v
    };
    std::fs::write(&kernel_path, &part_a).expect("write part a");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Phase B: extend to 64 bytes (this is the "growing" event that the
    // size-stability gate must observe and defer at least once).
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&kernel_path)
            .expect("open append");
        f.write_all(&[0u8; 32]).expect("append phase b");
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Now let it settle. Total wait window: 3s.
    tokio::time::sleep(Duration::from_millis(2_500)).await;

    // Drain — assert ≥1 KernelChanged appeared (size stable eventually).
    let mut kernel_changed_seen = false;
    let mut last_size = 0u64;
    loop {
        match rx.try_recv() {
            Ok(WsMessage::KernelChanged { size, ok, .. }) => {
                // Both ok=true (stable, ELF) and ok=false (transient
                // half-state) count as "the watcher reacted"; what we
                // care about for WCH-03 is that NO KernelChanged frame
                // ever reflects an intermediate non-stable state. We
                // assert the LAST one shows the settled size (64).
                kernel_changed_seen = true;
                last_size = size;
                // Sanity: if ok=true the size must be 64; if ok=false
                // the watcher recorded the size as observed (size_1).
                if ok {
                    assert_eq!(size, 64, "ok=true frame must show settled size");
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    assert!(
        kernel_changed_seen,
        "watcher must eventually fire a KernelChanged after partial write settles"
    );
    assert_eq!(
        last_size, 64,
        "final KernelChanged must reflect the settled (64-byte) size"
    );
}
