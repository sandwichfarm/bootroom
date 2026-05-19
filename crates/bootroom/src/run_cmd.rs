//! `bootroom run` — headless CI driver (RUN-01..10).
//!
//! This file is INTENTIONALLY a stub until Plan 04-07 lands the full
//! chromiumoxide driver. Building the CLI surface (04-03) before the
//! driver lets 04-04 / 04-05 / 04-06 wire dependencies into a
//! buildable codebase rather than blocking on the driver landing.
//!
//! The stub returns `ExitCode::from(3)` (startup error) with a stderr
//! diagnostic pointing at 04-07. Integration tests in 04-10 verify
//! the real driver replaces this body.

use crate::cli::RunArgs;
use std::process::ExitCode;

/// Run the `run` subcommand.
///
/// This is a stub in Plan 04-03. The real driver lands in Plan 04-07:
/// chromium discovery, in-process axum server, chromiumoxide launch,
/// COI self-check, oneshot await, transcript persistence, exit-code
/// translation. See
/// `.planning/phases/04-scenario-engine-headless-run/04-07-PLAN.md`.
pub async fn run(args: RunArgs) -> ExitCode {
    let _ = args; // Pin the signature for 04-07.
    eprintln!(
        "bootroom run: not implemented yet (Plan 04-03 stub; full driver lands in 04-07)"
    );
    ExitCode::from(3)
}
