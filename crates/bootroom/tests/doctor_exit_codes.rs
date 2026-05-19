//! `bootroom doctor` exit-code contract pin.
//!
//! Separate from `doctor_subcommand.rs` on purpose: when a future change
//! flips exit-code semantics, this file failing in isolation is the
//! clearest signal possible. The tests here assert ONLY on
//! `output.status.code()` — no stdout/stderr coupling.
//!
//! Plan 05-05 — DOC-01.

use std::process::{Command, ExitStatus};

fn bootroom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

/// Reject signal-termination outcomes (`status.code() == None`) — those
/// are a separate failure mode worth distinguishing from a "real" exit
/// code mismatch.
fn assert_exit(status: ExitStatus, expected: i32, ctx: &str) {
    match status.code() {
        Some(c) => assert_eq!(c, expected, "{ctx}: expected exit {expected}, got {c}"),
        None => panic!("{ctx}: process was signal-terminated, not exited"),
    }
}

/// On a green tree (canonical `build_router` headers pass, no broken
/// config), `bootroom doctor` must exit 0. This is the canonical
/// exit-zero pin — intentionally redundant with the stdout-coupled
/// `doctor_bare_exits_zero_on_green_build` test in `doctor_subcommand.rs`.
#[test]
fn doctor_exits_zero_when_all_checks_pass_or_info() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bootroom_bin())
        .arg("doctor")
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor should succeed");
    assert_exit(out.status, 0, "bare doctor in clean tempdir");
}

/// A broken `--config` path must trip exit 1. Canonical exit-one pin.
#[test]
fn doctor_exits_one_when_config_parse_fails() {
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
    assert_exit(out.status, 1, "doctor with broken --config");
}

/// `--format json` must not change the exit-code contract — green tree
/// remains exit 0, regardless of formatter selection.
#[test]
fn doctor_exits_zero_with_format_json_on_green_build() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bootroom_bin())
        .args(["doctor", "--format", "json"])
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor --format json should succeed");
    assert_exit(out.status, 0, "bare doctor --format json in clean tempdir");
}

/// Likewise, broken `--config` with `--format json` still exits 1. The
/// formatter is independent of the result; both wire formats must agree
/// on the exit-code contract.
#[test]
fn doctor_exits_one_with_format_json_on_broken_config() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let bad_toml = tmp.path().join("bad.toml");
    std::fs::write(&bad_toml, "this is not valid toml [[[\n")
        .expect("write broken toml fixture");
    let out = Command::new(bootroom_bin())
        .args(["doctor", "--format", "json", "--config"])
        .arg(&bad_toml)
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor --format json --config <bad> should succeed");
    assert_exit(out.status, 1, "doctor --format json with broken --config");
}
