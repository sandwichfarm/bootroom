//! Integration tests for `bootroom check` — CFG-07.
//!
//! Spawns the compiled binary as a subprocess and asserts on exit code,
//! stdout, and stderr per the Phase 3 Copywriting Contract:
//!
//! - exit 0 ok       → `<file>: ok (N actions, M scenarios)` on stdout
//! - exit 1 parse    → `<file>[:line:col]: <message>` on stderr
//! - exit 2 not-found → `<file>: file not found` on stderr
//! - exit 3 schema   → `<file>: schema_version mismatch (expected 1, got N)` on stderr

use std::io::Write;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

/// The 25-line example TOML from RESEARCH Example B — also embedded in
/// `init_cmd::EXAMPLE`. We inline a copy here so this test file is
/// independent of Task 2 ordering; `init_subcommand.rs` cross-validates
/// that the two stay in sync via `init_output_parses_with_check`.
const VALID_EXAMPLE: &str = r#"# bootroom.toml — bootroom test harness configuration.
# https://github.com/sandwich-farm/bootroom

schema_version = 1

# Action buttons appear in the UI in the order declared below.
# `bytes` accepts C-style escapes: \r \n \t \0 \\ \xNN.
[[action]]
label = "reboot"
bytes = "reboot\r"
group = "Boot"
description = "Send reboot command to the guest shell"

[[action]]
label = "ctrlc"
bytes = "\x03"
group = "Diagnostics"
description = "Send Ctrl-C to the foreground process"

# Scenarios are scripted action sequences with assertions.
# Phase 3 ships scenario *definitions*; the engine that runs them
# lands in Phase 4.
[[scenario]]
name = "boot_smoke"
actions = ["reboot"]
timeout_ms = 30000

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "reboot"
  timeout_ms = 5000
"#;

fn write_toml_tempfile(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .prefix("bootroom-check-")
        .suffix(".toml")
        .tempfile()
        .expect("create tempfile");
    f.write_all(content.as_bytes()).expect("write tempfile");
    f.flush().expect("flush tempfile");
    f
}

#[test]
fn check_valid_example_exits_zero() {
    let tmp = write_toml_tempfile(VALID_EXAMPLE);
    let path = tmp.path();
    let out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(path)
        .output()
        .expect("run bootroom check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stdout={stdout} stderr={stderr}"
    );
    let expected = format!("{}: ok (2 actions, 1 scenarios)", path.display());
    assert!(
        stdout.contains(&expected),
        "expected stdout to contain {expected:?}; got: {stdout}"
    );
}

#[test]
fn check_unknown_field_exits_one_with_span() {
    // Place an unknown top-level field at line 3 col 1.
    let bad = "schema_version = 1\n\nunknown_field = 1\n";
    let tmp = write_toml_tempfile(bad);
    let path = tmp.path();
    let out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(path)
        .output()
        .expect("run bootroom check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1; stderr={stderr}"
    );
    let prefix = format!("{}:3:1:", path.display());
    assert!(
        stderr.contains(&prefix),
        "expected stderr to begin with {prefix:?}; got: {stderr}"
    );
    assert!(
        stderr.contains("unknown") || stderr.contains("field"),
        "expected stderr to mention unknown/field; got: {stderr}"
    );
}

#[test]
fn check_schema_version_mismatch_exits_three() {
    let bad = "schema_version = 2\n";
    let tmp = write_toml_tempfile(bad);
    let path = tmp.path();
    let out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(path)
        .output()
        .expect("run bootroom check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "expected exit 3; stderr={stderr}"
    );
    let expected = format!(
        "{}: schema_version mismatch (expected 1, got 2)",
        path.display()
    );
    assert!(
        stderr.contains(&expected),
        "expected stderr to contain {expected:?}; got: {stderr}"
    );
}

#[test]
fn check_missing_config_path_exits_two() {
    let path = "/nonexistent/bootroom-does-not-exist-xyz.toml";
    let out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(path)
        .output()
        .expect("run bootroom check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2; stderr={stderr}"
    );
    let expected = format!("{path}: file not found");
    assert!(
        stderr.contains(&expected),
        "expected stderr to contain {expected:?}; got: {stderr}"
    );
}

#[test]
fn check_default_path_in_empty_cwd_exits_two() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bin())
        .arg("check")
        .current_dir(tmp.path())
        .output()
        .expect("run bootroom check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2; stderr={stderr}"
    );
    assert!(
        stderr.contains("bootroom.toml: file not found"),
        "expected stderr to contain `bootroom.toml: file not found`; got: {stderr}"
    );
}

#[test]
fn check_scenario_unknown_action_exits_one() {
    let bad = r#"schema_version = 1

[[action]]
label = "reboot"
bytes = "reboot\r"

[[scenario]]
name = "boot_smoke"
actions = ["missing"]
timeout_ms = 1000
"#;
    let tmp = write_toml_tempfile(bad);
    let path = tmp.path();
    let out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(path)
        .output()
        .expect("run bootroom check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1; stderr={stderr}"
    );
    // The bootroom-core LoadError message uses single quotes; the
    // Copywriting Contract calls for double quotes but the core library
    // owns the format. Document deviation: we accept the single-quote
    // form here and let the Copywriting Contract follow what's actually
    // emitted (also less brittle).
    let expected = format!(
        "{}: scenario 'boot_smoke' references unknown action 'missing'",
        path.display()
    );
    assert!(
        stderr.contains(&expected),
        "expected stderr to contain {expected:?}; got: {stderr}"
    );
}
