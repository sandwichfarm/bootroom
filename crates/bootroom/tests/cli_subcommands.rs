//! CFG-07/CFG-08 scaffolding: CLI surface for `serve`, `check`, `init`.
//!
//! These tests pin the user-visible CLI shape that Plans 04 (real
//! check/init bodies), 06 (config-aware serve), and 07 (watcher) will
//! depend on. Per Pitfall #9, the Phase-2 subprocess test
//! `tests/serve_no_open.rs` must keep passing unchanged; that
//! verification lives in its own file and is run as part of the plan
//! verifier.
//!
//! Implementation note: this file does NOT include `mod common;` —
//! the only thing we need is `CARGO_BIN_EXE_bootroom`, which cargo
//! exports automatically for every integration test. Avoiding the
//! shared `common` module also avoids a write conflict with the
//! parallel 03-05 plan that owns `tests/common/mod.rs`.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

#[test]
fn cli_help_lists_three_subcommands() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run bootroom --help");
    assert!(out.status.success(), "--help should exit 0, got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    for sub in ["serve", "check", "init"] {
        assert!(
            help.contains(sub),
            "`bootroom --help` must mention `{sub}`. Got:\n{help}"
        );
    }
}

#[test]
fn cli_serve_help_includes_config_and_action() {
    let out = Command::new(bin())
        .args(["serve", "--help"])
        .output()
        .expect("run bootroom serve --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    for needle in ["--config", "--action", "LABEL=BYTES"] {
        assert!(
            help.contains(needle),
            "`bootroom serve --help` must mention `{needle}`. Got:\n{help}"
        );
    }
}

#[test]
fn cli_check_help_includes_config() {
    let out = Command::new(bin())
        .args(["check", "--help"])
        .output()
        .expect("run bootroom check --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--config"),
        "`bootroom check --help` must mention --config. Got:\n{help}"
    );
}

#[test]
fn cli_init_help_includes_force() {
    let out = Command::new(bin())
        .args(["init", "--help"])
        .output()
        .expect("run bootroom init --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--force"),
        "`bootroom init --help` must mention --force. Got:\n{help}"
    );
}

// The Plan-03 placeholder stub tests `cli_check_stub_exits_nonzero` and
// `cli_init_stub_exits_nonzero` were RETIRED by Plan 03-04. Their
// real-behavior replacements live in `tests/check_subcommand.rs` and
// `tests/init_subcommand.rs` respectively.
