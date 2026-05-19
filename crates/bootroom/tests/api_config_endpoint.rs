//! Plan 03-07: `/api/config` HTTP shape contract.
//!
//! Verifies (per the plan's `must_haves`):
//! - JSON shape: `schema_version`, `actions[]`, `scenarios[]` at the top level.
//! - Each action carries `label`, `bytes_b64`, `group`, `description`;
//!   base64-decoding `bytes_b64` yields the escape-decoded byte sequence.
//! - Action insertion order is preserved (CFG-09).
//! - COOP/COEP middleware applies to `/api/config` (regression check).
//! - Empty config returns empty arrays (not absent fields).

mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bootroom_core::config::LoadedConfig;
use serde_json::Value;

/// ACT-01: action surface is base64-decodable bytes on the wire.
#[tokio::test]
async fn shape_includes_base64_bytes() {
    let toml = "schema_version = 1\n\n\
        [[action]]\n\
        label = \"reboot\"\n\
        bytes = \"reboot\\r\"\n\
        group = \"system\"\n\
        description = \"Soft reboot\"\n";
    let loaded = LoadedConfig::load_from_str(toml).expect("load");

    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let resp = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("http get");
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .expect("content-type")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        ct.starts_with("application/json"),
        "content-type must be JSON, got {ct}"
    );

    let json: Value = resp.json().await.expect("json");
    assert_eq!(json["schema_version"].as_u64(), Some(1));
    let actions = json["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1);
    let a0 = &actions[0];
    assert_eq!(a0["label"].as_str(), Some("reboot"));
    assert_eq!(a0["group"].as_str(), Some("system"));
    assert_eq!(a0["description"].as_str(), Some("Soft reboot"));

    let b64 = a0["bytes_b64"].as_str().expect("bytes_b64");
    let decoded = STANDARD.decode(b64).expect("decode b64");
    // "reboot\r" = [b'r', b'e', b'b', b'o', b'o', b't', 0x0d]
    assert_eq!(decoded, b"reboot\r");

    // scenarios present and empty (we declared none).
    assert!(
        json["scenarios"]
            .as_array()
            .expect("scenarios array")
            .is_empty()
    );
}

/// CFG-09: action insertion order is preserved through the wire.
#[tokio::test]
async fn order_preserved() {
    let toml = "schema_version = 1\n\n\
        [[action]]\n\
        label = \"zebra\"\n\
        bytes = \"z\"\n\n\
        [[action]]\n\
        label = \"alpha\"\n\
        bytes = \"a\"\n\n\
        [[action]]\n\
        label = \"middle\"\n\
        bytes = \"m\"\n\n\
        [[action]]\n\
        label = \"last\"\n\
        bytes = \"l\"\n";
    let loaded = LoadedConfig::load_from_str(toml).expect("load");

    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let json: Value = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    let actions = json["actions"].as_array().expect("actions array");
    let labels: Vec<&str> = actions
        .iter()
        .map(|a| a["label"].as_str().expect("label"))
        .collect();
    assert_eq!(
        labels,
        vec!["zebra", "alpha", "middle", "last"],
        "TOML insertion order must survive the wire (CFG-09)"
    );
}

/// Regression: COOP/COEP middleware applies to `/api/config` (threat
/// T-03-07-05 — confused deputy mitigation; the layer model auto-applies
/// to new routes but the safety net catches accidental .layer-vs-.route
/// reordering).
#[tokio::test]
async fn coop_coep_present() {
    let loaded = LoadedConfig::load_from_str("schema_version = 1\n").expect("load");
    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let resp = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("cross-origin-opener-policy")
            .expect("coop header")
            .to_str()
            .unwrap(),
        "same-origin"
    );
    assert_eq!(
        resp.headers()
            .get("cross-origin-embedder-policy")
            .expect("coep header")
            .to_str()
            .unwrap(),
        "require-corp"
    );
}

/// A minimal config (no actions, no scenarios) projects to empty *arrays*,
/// not absent fields. The browser relies on `actions.length === 0` rather
/// than `typeof actions === 'undefined'`.
#[tokio::test]
async fn empty_config_returns_empty_arrays() {
    let loaded = LoadedConfig::load_from_str("schema_version = 1\n").expect("load");
    let kernel = common::write_kernel_tempfile(b"k");
    let server = common::spawn_with_loaded(kernel.path().to_path_buf(), loaded).await;

    let json: Value = reqwest::get(format!("{}/api/config", server.base_url))
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert_eq!(json["schema_version"].as_u64(), Some(1));
    let actions = json["actions"].as_array().expect("actions must be present array");
    assert!(actions.is_empty());
    let scenarios = json["scenarios"].as_array().expect("scenarios must be present array");
    assert!(scenarios.is_empty());
}
