//! Plan 03-07: integration half of ACT-03 — CLI `--action` overrides
//! land in the `/api/config` payload.
//!
//! Composes the merge logic from Plan 03-01 (`LoadedConfig::load_from_str_with_overrides`)
//! with the projection from Plan 03-06 (`watcher::project_loaded_to_json`)
//! through the HTTP surface from Plan 03-07 (`api_config` handler).

mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bootroom_core::config::{CliAction, LoadedConfig};
use serde_json::Value;

/// CLI override of an existing TOML action: same label, different bytes.
/// `/api/config` MUST surface the CLI bytes (group/description cleared per
/// CONTEXT D-02 override semantics).
#[tokio::test]
async fn cli_action_overrides_config_in_api_config() {
    let toml = "schema_version = 1\n\n\
        [[action]]\n\
        label = \"reboot\"\n\
        bytes = \"reboot\\r\"\n\
        group = \"system\"\n\
        description = \"original\"\n";
    let cli = vec![CliAction {
        label: "reboot".into(),
        bytes: vec![0x03], // Ctrl-C
    }];
    let loaded = LoadedConfig::load_from_str_with_overrides(toml, &cli).expect("load+merge");

    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let json: Value = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("get")
        .json()
        .await
        .expect("json");

    let actions = json["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "override is in-place; count unchanged");
    let a0 = &actions[0];
    assert_eq!(a0["label"].as_str(), Some("reboot"));
    let decoded = STANDARD
        .decode(a0["bytes_b64"].as_str().expect("bytes_b64"))
        .expect("decode b64");
    assert_eq!(decoded, vec![0x03], "CLI bytes must win");
    // CONTEXT D-02: group/description cleared when CLI shadows an
    // existing TOML action.
    assert!(
        a0["group"].is_null(),
        "group must be cleared when CLI overrides; got {:?}",
        a0["group"]
    );
    assert!(
        a0["description"].is_null(),
        "description must be cleared when CLI overrides; got {:?}",
        a0["description"]
    );
}

/// CLI add of a new label: appends to the end, original TOML action
/// preserved in its original position.
#[tokio::test]
async fn cli_action_appends_new_action() {
    let toml = "schema_version = 1\n\n\
        [[action]]\n\
        label = \"x\"\n\
        bytes = \"x\"\n";
    let cli = vec![CliAction {
        label: "y".into(),
        bytes: vec![0xff],
    }];
    let loaded = LoadedConfig::load_from_str_with_overrides(toml, &cli).expect("load+merge");

    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let json: Value = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("get")
        .json()
        .await
        .expect("json");

    let actions = json["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0]["label"].as_str(), Some("x"));
    let x_bytes = STANDARD
        .decode(actions[0]["bytes_b64"].as_str().unwrap())
        .expect("decode x");
    assert_eq!(x_bytes, b"x");

    assert_eq!(actions[1]["label"].as_str(), Some("y"));
    let y_bytes = STANDARD
        .decode(actions[1]["bytes_b64"].as_str().unwrap())
        .expect("decode y");
    assert_eq!(y_bytes, vec![0xff]);
    // New CLI-only actions carry no UI metadata.
    assert!(actions[1]["group"].is_null());
    assert!(actions[1]["description"].is_null());
}
