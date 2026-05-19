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

// ----- Plan 04-10 additions: pin the `run` subcommand's surface ------
//
// These tests are subprocess-level regression pins for the CLI-02
// shared-flatten contract (`--kernel`, `--config`, `--verbose` visible
// on both `serve` and `run`) and the run-only `--scenario` /
// `--log-file` flags. They live alongside the Phase-3 help tests so
// every CLI surface change runs through the same harness.

#[test]
fn run_subcommand_help_mentions_shared_flags() {
    let out = Command::new(bin())
        .args(["run", "--help"])
        .output()
        .expect("run bootroom run --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    for needle in ["--kernel", "--config", "--verbose", "--scenario", "--log-file"] {
        assert!(
            help.contains(needle),
            "`bootroom run --help` must mention `{needle}`. Got:\n{help}"
        );
    }
}

#[test]
fn serve_subcommand_help_still_mentions_shared_flags() {
    // Regression pin for 04-03's CLI-02 flatten: --kernel/--config/--verbose
    // must remain visible on `serve --help` after the migration into
    // CommonArgs.
    let out = Command::new(bin())
        .args(["serve", "--help"])
        .output()
        .expect("run bootroom serve --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    for needle in ["--kernel", "--config", "--verbose"] {
        assert!(
            help.contains(needle),
            "`bootroom serve --help` must mention `{needle}`. Got:\n{help}"
        );
    }
}

#[test]
fn top_level_help_lists_run_subcommand() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run bootroom --help");
    assert!(out.status.success(), "got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);
    for sub in ["serve", "run", "check", "init"] {
        assert!(
            help.contains(sub),
            "`bootroom --help` must mention `{sub}`. Got:\n{help}"
        );
    }
}

// ----- Plan 05-06 addition: pin the EXACT five-subcommand surface -----
//
// CLI-01 contract: `bootroom --help` lists exactly five user-facing
// subcommands in the documented order (serve, run, check, init,
// doctor). The auto-added `help` is the only allowed sixth entry.
//
// This single test catches three regression classes at once:
// - Subcommand deletion          (a known name disappears)
// - Subcommand rename            (a known name vanishes; surprise name appears)
// - Subcommand surprise addition (e.g. a cfg-gated `dev` slips in)
//
// Implementation note: we don't position-anchor against clap's exact
// indentation (clap is allowed to retune its formatter between minor
// releases). Instead we parse the "Commands:" block, split off the
// first whitespace-delimited token from each indented line, and
// compare against the documented order.

#[test]
fn top_level_help_lists_exactly_five_subcommands() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run bootroom --help");
    assert!(out.status.success(), "--help should exit 0, got {:?}", out.status);
    let help = String::from_utf8_lossy(&out.stdout);

    // Extract the "Commands:" block: the lines between "Commands:" and
    // the next blank line. Clap renders each row as
    //   "  <name>  <about>"
    // — two leading spaces, name, then more whitespace, then the
    // about-text. We split on whitespace and take the first token.
    let cmd_block: Vec<&str> = help
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("Commands:"))
        .skip(1) // skip the "Commands:" header line itself
        .take_while(|l| !l.trim().is_empty())
        .collect();
    assert!(
        !cmd_block.is_empty(),
        "could not find a `Commands:` block in --help output. Full output:\n{help}"
    );

    let mut listed: Vec<String> = Vec::new();
    for line in &cmd_block {
        // Skip continuation lines (clap wraps long abouts onto extra
        // lines that are indented further than the name column).
        // A real command row starts with exactly two spaces; a
        // continuation line starts with many more.
        if !line.starts_with("  ") {
            continue;
        }
        // Some clap versions indent continuation rows with >2 spaces
        // but not all do — guard by requiring the first token to look
        // like a command name (lowercase alpha, length ≥ 2).
        let token = line.split_whitespace().next().unwrap_or("");
        if token.len() >= 2 && token.chars().all(|c| c.is_ascii_lowercase()) {
            listed.push(token.to_string());
        }
    }

    let expected = ["serve", "run", "check", "init", "doctor"];

    // Forward pin: every documented subcommand appears in order.
    let mut positions: Vec<usize> = Vec::with_capacity(expected.len());
    for name in expected {
        let idx = listed.iter().position(|n| n == name).unwrap_or_else(|| {
            panic!(
                "documented subcommand `{name}` missing from --help.\n\
                 Listed commands: {listed:?}\n\
                 Full --help:\n{help}"
            )
        });
        positions.push(idx);
    }
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(
        positions, sorted,
        "subcommands appear out of order. Expected serve < run < check < init < doctor.\n\
         Found positions {positions:?} for {expected:?} in listed {listed:?}"
    );

    // Inverse pin: every listed name must be one of the five known
    // subcommands plus clap's auto-added `help`. A surprise sixth
    // command (e.g. accidentally cfg-gated `dev` or `bench`) fires CI.
    let known: std::collections::HashSet<&str> =
        ["serve", "run", "check", "init", "doctor", "help"]
            .into_iter()
            .collect();
    for name in &listed {
        assert!(
            known.contains(name.as_str()),
            "surprise subcommand `{name}` appeared in --help — it is not part of the \
             CLI-01 contract (serve, run, check, init, doctor + clap's auto `help`). \
             Either add it to the documented surface or remove it. Listed: {listed:?}"
        );
    }
}
