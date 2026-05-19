//! `bootroom init` — onboarding subcommand (CFG-08).
//!
//! Writes a 25-line starter `bootroom.toml` into the current working
//! directory. Refuses to overwrite an existing file unless `--force` is
//! passed. The example exercises every Phase-3 schema field so that
//! operators have a working reference for `[[action]]` (with grouping
//! plus escape-encoded `bytes`) and `[[scenario]]` (with one
//! `[[scenario.assert]]` block).
//!
//! Exit codes:
//!
//! | Code | Meaning                                              |
//! |------|------------------------------------------------------|
//! | 0    | File written; stdout: `Wrote ./bootroom.toml`        |
//! | 1    | File exists and `--force` not passed; or write error |
//!
//! `init` -> `check` cross-validation lives in
//! `tests/init_subcommand.rs::init_output_parses_with_check` — the
//! `EXAMPLE` constant below is the single source of truth.

use crate::cli::InitArgs;
use std::path::PathBuf;
use std::process::ExitCode;

/// The 25-line example TOML written by `bootroom init`.
///
/// Held as a raw string literal (not `include_str!`) per RESEARCH
/// Pattern 8 anti-pattern note — the example is small enough to inline
/// and the raw-string syntax avoids the dual-decoding hazard (we want
/// the *characters* `\r` and `\x03` to appear in the file, then TOML's
/// own escape decoder handles them at load time).
pub const EXAMPLE: &str = r#"# bootroom.toml — bootroom test harness configuration.
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

/// Run the `init` subcommand.
///
/// Returns an `ExitCode` rather than calling `std::process::exit` so
/// integration tests can also call this function directly without
/// terminating the test runner.
#[must_use]
pub fn run(args: &InitArgs) -> ExitCode {
    let path = PathBuf::from("bootroom.toml");

    if path.exists() && !args.force {
        eprintln!("bootroom.toml already exists; pass --force to overwrite.");
        return ExitCode::from(1);
    }

    match std::fs::write(&path, EXAMPLE) {
        Ok(()) => {
            println!("Wrote ./bootroom.toml");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("failed to write {}: {e}", path.display());
            ExitCode::from(1)
        }
    }
}
