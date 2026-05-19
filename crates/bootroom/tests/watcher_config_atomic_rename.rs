//! CR-03 regression: an atomic-rename save of `bootroom.toml` (write
//! tempfile -> rename over target) must fire `ConfigUpdate`. inotify
//! watches are attached to the inode; before the fix, watching the
//! config file directly caused the watch to follow the orphaned old
//! inode after one rename, silently missing every subsequent edit.
//!
//! Editors that save this way include vim with `:set writebackup`,
//! VS Code, JetBrains IDEs, `git checkout`, and any `make`-driven
//! config regenerator. The fix watches the config's PARENT dir and
//! demuxes by basename equality (same pattern as the kernel watch).

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::time::Duration;

const REPLACEMENT_TOML: &str = "schema_version = 1\n\n\
    [[action]]\n\
    label = \"halt\"\n\
    bytes = 'halt\\r'\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atomic_rename_save_fires_config_update() {
    let (state, tmp, _kernel, cfg_path) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Round 1: atomic-rename save. Write a sibling tempfile, then
    // rename it over the canonical config path — same pattern editors
    // use.
    let sibling = tmp.path().join("bootroom.toml.tmp");
    std::fs::write(&sibling, REPLACEMENT_TOML).expect("write sibling");
    std::fs::rename(&sibling, &cfg_path).expect("rename atomically");

    let msg = recv_until_config(&mut rx, Duration::from_secs(2))
        .await
        .expect("ConfigUpdate within 2s for round-1 rename save");
    match msg {
        WsMessage::ConfigUpdate { config } => {
            let actions = config["actions"].as_array().expect("actions array");
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0]["label"].as_str(), Some("halt"));
        }
        other => panic!("expected ConfigUpdate, got: {other:?}"),
    }

    // Round 2: do it AGAIN. This is the load-bearing assertion — under
    // the pre-fix file-watch behavior, the inode that the watcher
    // attached to was orphaned by round 1, so round 2 fires no event.
    let sibling2 = tmp.path().join("bootroom.toml.tmp2");
    std::fs::write(&sibling2, common::INITIAL_TOML).expect("write sibling 2");
    std::fs::rename(&sibling2, &cfg_path).expect("rename 2 atomically");

    let msg2 = recv_until_config(&mut rx, Duration::from_secs(2))
        .await
        .expect("ConfigUpdate within 2s for round-2 rename save (CR-03 regression)");
    match msg2 {
        WsMessage::ConfigUpdate { config } => {
            let actions = config["actions"].as_array().expect("actions array");
            assert_eq!(actions.len(), 1);
            // INITIAL_TOML has the single action "reboot".
            assert_eq!(actions[0]["label"].as_str(), Some("reboot"));
        }
        other => panic!("expected ConfigUpdate round 2, got: {other:?}"),
    }
}

/// Drain the broadcast channel until a config-shaped frame arrives or
/// the deadline expires; skip unrelated noise (KernelChanged etc.).
async fn recv_until_config(
    rx: &mut tokio::sync::broadcast::Receiver<WsMessage>,
    deadline: Duration,
) -> Option<WsMessage> {
    let start = tokio::time::Instant::now();
    while start.elapsed() < deadline {
        let remaining = deadline.saturating_sub(start.elapsed());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(msg @ (WsMessage::ConfigUpdate { .. } | WsMessage::ConfigInvalid { .. }))) => {
                return Some(msg);
            }
            Ok(Ok(_other)) => {}
            Ok(Err(_)) => {}
            Err(_) => return None,
        }
    }
    None
}
