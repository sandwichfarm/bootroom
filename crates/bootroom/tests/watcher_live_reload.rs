//! CFG-10: editing `bootroom.toml` triggers a `ConfigUpdate` broadcast
//! with the JSON projection, and updates `AppState.loaded_config` in
//! place. An invalid edit triggers `ConfigInvalid` and leaves the
//! last-known-good config untouched.

mod common;

use bootroom::watcher::spawn_watcher;
use bootroom_core::WsMessage;
use std::time::Duration;

const TWO_ACTION_TOML: &str = "schema_version = 1\n\n\
    [[action]]\n\
    label = \"reboot\"\n\
    bytes = 'reboot\\r'\n\n\
    [[action]]\n\
    label = \"halt\"\n\
    bytes = 'halt\\r'\n";

const INVALID_TOML: &str = "schema_version = 1\n\n[[action]]\nlable = \"x\"\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn toml_reload() {
    let (state, _tmp, _kernel, cfg_path) = common::spawn_watcher_test_setup();
    let mut rx = state.ws_broadcast.subscribe();

    spawn_watcher(state.clone()).expect("spawn_watcher");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Initial state: one action ("reboot") loaded by spawn_watcher_test_setup.
    {
        let cfg = state.loaded_config.read().await;
        assert_eq!(cfg.actions().len(), 1);
        assert_eq!(cfg.actions()[0].label, "reboot");
    }

    // Phase 1: rewrite with 2 valid actions -> expect ConfigUpdate +
    // AppState.loaded_config gains the new action.
    std::fs::write(&cfg_path, TWO_ACTION_TOML).expect("write valid");

    // Receive until we see a ConfigUpdate (the watcher may also emit
    // KernelChanged events if the parent dir gets touched; we filter).
    let update = recv_until_config(&mut rx, Duration::from_secs(2))
        .await
        .expect("ConfigUpdate within 2s");

    match update {
        WsMessage::ConfigUpdate { config } => {
            let actions = config["actions"].as_array().expect("actions array");
            assert_eq!(actions.len(), 2, "projection must contain both actions");
            let labels: Vec<&str> = actions
                .iter()
                .map(|a| a["label"].as_str().expect("label"))
                .collect();
            assert_eq!(labels, vec!["reboot", "halt"]);
        }
        other => panic!("expected ConfigUpdate, got: {other:?}"),
    }

    // AppState.loaded_config updated in place.
    {
        let cfg = state.loaded_config.read().await;
        assert_eq!(cfg.actions().len(), 2);
        assert_eq!(cfg.actions()[1].label, "halt");
    }

    // Phase 2: rewrite with an INVALID TOML (typo: `lable`) -> expect
    // ConfigInvalid AND AppState.loaded_config STAYS at the 2-action state.
    std::fs::write(&cfg_path, INVALID_TOML).expect("write invalid");

    let invalid = recv_until_config(&mut rx, Duration::from_secs(2))
        .await
        .expect("ConfigInvalid within 2s");

    match invalid {
        WsMessage::ConfigInvalid { error, .. } => {
            assert!(
                error.to_lowercase().contains("unknown field") || error.contains("lable"),
                "error should mention the typo'd field; got: {error}"
            );
        }
        other => panic!("expected ConfigInvalid, got: {other:?}"),
    }

    // Last-known-good preserved (still 2 actions).
    {
        let cfg = state.loaded_config.read().await;
        assert_eq!(
            cfg.actions().len(),
            2,
            "ConfigInvalid must NOT mutate loaded_config"
        );
    }
}

/// Helper: drain until a `ConfigUpdate` OR `ConfigInvalid` arrives, or
/// the timeout expires. Skips unrelated frames (e.g. `KernelChanged`
/// triggered by parent-dir noise — writing to bootroom.toml in the
/// tempdir can fire parent-dir events that the kernel-demux branch
/// quietly ignores, but other tests in the same dir might race).
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
            Ok(Ok(_other)) => {} // skip KernelChanged / unrelated frames
            Ok(Err(_lagged_or_closed)) => {}
            Err(_elapsed) => return None,
        }
    }
    None
}
