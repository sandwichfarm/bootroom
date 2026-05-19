//! Wave-0 scaffold for Phase 5 doctor subcommand integration tests.
//!
//! This file pins the contract that `crates/bootroom/build.rs` exposes
//! `BOOTROOM_GIT_SHA` as a compile-time env var via
//! `cargo:rustc-env=BOOTROOM_GIT_SHA=...`. Plan 05-04 consumes this via
//! `env!("BOOTROOM_GIT_SHA")` in the doctor `version` check. Plan 05-05
//! appends CLI shape / exit-code / stderr-summary tests to this file.

use std::process::Command;

const SHA: &str = env!("BOOTROOM_GIT_SHA");

fn bootroom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

#[test]
fn git_sha_env_is_set() {
    // The macro resolves at compile time; this asserts the value is a
    // non-empty &str at runtime.
    assert!(!SHA.is_empty(), "BOOTROOM_GIT_SHA must not be empty");
}

#[test]
fn git_sha_env_has_no_whitespace() {
    assert!(
        !SHA.chars().any(|c| c.is_whitespace()),
        "BOOTROOM_GIT_SHA must not contain whitespace, got {SHA:?}"
    );
}

#[test]
fn git_sha_env_shape_is_short_sha_or_unknown() {
    if SHA == "unknown" {
        return;
    }
    let len_ok = (7..=40).contains(&SHA.len());
    let hex_ok = SHA.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    assert!(
        len_ok && hex_ok,
        "BOOTROOM_GIT_SHA must be \"unknown\" or [0-9a-f]{{7,40}}, got {SHA:?}"
    );
}

// ----- 05-05 additions: subprocess contract pins -----

/// Top-level `--help` must list `doctor` so operators discover the subcommand
/// in the standard place. Asserts only that the word `doctor` appears
/// alongside the rendered about-text (which mentions `preflight`).
#[test]
fn top_level_help_lists_doctor() {
    let out = Command::new(bootroom_bin())
        .arg("--help")
        .output()
        .expect("running bootroom --help should succeed");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Find the subcommands block by looking for a line containing "doctor"
    // alongside some piece of the rendered about-text. clap renders each
    // subcommand on its own line with the about-text alongside; we accept
    // either `Run` (which starts the about-text) or `preflight`.
    let has_doctor_listing = stdout.lines().any(|l| {
        l.contains("doctor") && (l.contains("Run") || l.contains("preflight"))
    });
    assert!(
        has_doctor_listing,
        "expected --help to list `doctor` with its about-text; got:\n{stdout}"
    );
}

/// `bootroom doctor --help` must advertise the two flags operators rely on.
#[test]
fn doctor_help_mentions_flags() {
    let out = Command::new(bootroom_bin())
        .args(["doctor", "--help"])
        .output()
        .expect("running bootroom doctor --help should succeed");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--config"),
        "doctor --help must mention --config; got:\n{stdout}"
    );
    assert!(
        stdout.contains("--format"),
        "doctor --help must mention --format; got:\n{stdout}"
    );
}

/// Running `bootroom doctor` with no flags from a clean tempdir on this
/// build tree must exit 0 (headers Pass via in-process router, config Info
/// because no `bootroom.toml` is present) and stdout must contain the
/// `Overall: pass` final line.
#[test]
fn doctor_bare_exits_zero_on_green_build() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bootroom_bin())
        .arg("doctor")
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("Overall: pass"),
        "stdout must contain `Overall: pass`; got:\n{stdout}"
    );
}

/// `bootroom doctor --format json` must emit valid JSON whose top-level
/// shape matches the v1 schema (`schema_version=1` + exactly five keys).
/// This is the Pitfall 5 mitigation — any drift in the wire shape forces
/// a deliberate `schema_version` bump.
#[test]
fn doctor_format_json_emits_valid_schema() {
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
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor JSON parses as serde_json::Value");
    assert_eq!(
        value["schema_version"],
        serde_json::json!(1),
        "schema_version must be 1 (got {:?})",
        value.get("schema_version")
    );
    let obj = value.as_object().expect("top-level is a JSON object");
    let keys: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let expected: std::collections::BTreeSet<&str> =
        ["schema_version", "version", "git_sha", "checks", "overall"]
            .into_iter()
            .collect();
    assert_eq!(
        keys, expected,
        "top-level JSON keys must be exactly the v1 schema set"
    );
}

/// On overall fail, doctor MUST write a single-line summary to stderr so
/// operators can `bootroom doctor 2>&1 | grep ...` in CI. Pitfall 8 pin.
#[test]
fn doctor_failure_writes_stderr_summary() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let bad_toml = tmp.path().join("bad.toml");
    std::fs::write(&bad_toml, "this is not valid toml [[[\n")
        .expect("write broken toml fixture");
    let out = Command::new(bootroom_bin())
        .args(["doctor", "--config"])
        .arg(&bad_toml)
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor --config <bad> should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 on broken config; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stderr.contains("bootroom doctor:"),
        "stderr must contain `bootroom doctor:` prefix; got:\n{stderr}"
    );
    assert!(
        stderr.contains("config"),
        "stderr must mention `config`; got:\n{stderr}"
    );
}
