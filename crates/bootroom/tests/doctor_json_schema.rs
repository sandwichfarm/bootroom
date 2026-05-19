//! JSON schema v1 pins for `bootroom doctor --format json`.
//!
//! These tests are the single hardest contract guard in Phase 5: any
//! drift in the JSON wire shape forces a deliberate `schema_version`
//! bump (Pitfall 5). Each pin asserts EXACT membership of the relevant
//! key/name/enum set — not "at least these" — so additions trip a test.
//!
//! Plan 05-05 — DOC-01.

use std::collections::BTreeSet;
use std::process::Command;

fn bootroom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

fn run_doctor_json() -> serde_json::Value {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bootroom_bin())
        .args(["doctor", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor --format json should succeed");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("doctor JSON parses as serde_json::Value")
}

/// Top-level key set MUST be exactly five — adding a sixth without
/// bumping `schema_version` is a contract violation.
#[test]
fn json_top_level_keys_are_exactly_five() {
    let v = run_doctor_json();
    let obj = v.as_object().expect("top-level is a JSON object");
    let keys: BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> =
        ["checks", "git_sha", "overall", "schema_version", "version"]
            .into_iter()
            .collect();
    assert_eq!(
        keys, expected,
        "top-level JSON keys must be exactly the v1 schema set"
    );
}

/// `schema_version` MUST be the integer 1. Pitfall 5 pin.
#[test]
fn json_schema_version_is_one() {
    let v = run_doctor_json();
    assert_eq!(
        v["schema_version"],
        serde_json::json!(1),
        "schema_version must be 1; got {:?}",
        v.get("schema_version")
    );
}

/// The six known check names — adding a seventh trips this pin and
/// forces a deliberate `schema_version` decision (do we add it to v1,
/// or bump to v2?).
#[test]
fn json_checks_names_are_the_six_known() {
    let v = run_doctor_json();
    let checks = v["checks"].as_array().expect("checks is an array");
    let mut names: Vec<&str> = checks
        .iter()
        .map(|c| c["name"].as_str().expect("check.name is string"))
        .collect();
    names.sort_unstable();
    let expected = vec![
        "browser",
        "cli_surface",
        "config",
        "headers",
        "qemu_wasm_rev",
        "version",
    ];
    assert_eq!(
        names, expected,
        "check.name set must be exactly the six known v1 identifiers"
    );
}

/// Every `status` must be in the `{pass, fail, info}` enum. Catches any
/// future enum widening without a schema bump.
#[test]
fn json_status_values_are_in_enum() {
    let v = run_doctor_json();
    let checks = v["checks"].as_array().expect("checks is an array");
    for c in checks {
        let status = c["status"].as_str().expect("check.status is string");
        assert!(
            matches!(status, "pass" | "fail" | "info"),
            "unexpected check.status `{status}` for check `{}`",
            c["name"]
        );
    }
}

/// `overall` MUST be exactly `"pass"` or `"fail"` (string), never `null`
/// or absent, never another value.
#[test]
fn json_overall_is_pass_or_fail() {
    let v = run_doctor_json();
    let overall = v["overall"].as_str().expect("overall is a string");
    assert!(
        overall == "pass" || overall == "fail",
        "overall must be `pass` or `fail`; got: {overall:?}"
    );
}

/// `version` MUST equal `CARGO_PKG_VERSION` — drift between Cargo
/// metadata and the doctor's reported version is a silent CI break.
#[test]
fn json_version_is_cargo_pkg_version() {
    let v = run_doctor_json();
    assert_eq!(
        v["version"],
        serde_json::json!(env!("CARGO_PKG_VERSION")),
        "version field must equal CARGO_PKG_VERSION"
    );
}

/// `git_sha` MUST be a non-empty string matching either the
/// `"unknown"` sentinel (build.rs fallback path) or a 7..40-char
/// lowercase hex SHA. Sibling of the 05-01 build-time test.
#[test]
fn json_git_sha_is_set() {
    let v = run_doctor_json();
    let sha = v["git_sha"].as_str().expect("git_sha is a string");
    assert!(!sha.is_empty(), "git_sha must be non-empty");
    if sha == "unknown" {
        return;
    }
    let len_ok = (7..=40).contains(&sha.len());
    let hex_ok = sha.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    assert!(
        len_ok && hex_ok,
        "git_sha must be `unknown` or [0-9a-f]{{7,40}}; got {sha:?}"
    );
}
