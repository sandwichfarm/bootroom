//! `bootroom check` — CI preflight subcommand (CFG-07).
//!
//! Reads a `bootroom.toml` from disk, parses + validates via
//! [`bootroom_core::config::LoadedConfig::load_from_str`], and reports
//! the outcome on stdout / stderr with operator-friendly exit codes.
//!
//! Exit-code matrix (Phase 3 CONTEXT `<specifics>`):
//!
//! | Code | Meaning                                | Output                                                          |
//! |------|----------------------------------------|-----------------------------------------------------------------|
//! | 0    | TOML parses + cross-validates          | stdout: `<file>: ok (N actions, M scenarios)`                   |
//! | 1    | TOML parse error or validation failure | stderr: `<file>[:line:col]: <message>`                          |
//! | 2    | Config file not found / read error     | stderr: `<file>: file not found` (`NotFound`) or `<file>: <io>` |
//! | 3    | `schema_version` mismatch              | stderr: `<file>: schema_version mismatch (expected 1, got N)`   |
//!
//! The schema-mismatch case is formatted from the typed
//! [`bootroom_core::config::LoadError`] predicates rather than re-using
//! the underlying `message` string so the operator-facing wording
//! stays under this crate's control (Copywriting Contract).

use crate::cli::CheckArgs;
use bootroom_core::config::LoadedConfig;
use std::path::PathBuf;
use std::process::ExitCode;

/// Run the `check` subcommand.
///
/// Returns an `ExitCode` rather than calling `std::process::exit` so
/// integration tests can also call this function directly without
/// terminating the test runner.
#[must_use]
pub fn run(args: CheckArgs) -> ExitCode {
    let path = args.config.unwrap_or_else(|| PathBuf::from("bootroom.toml"));

    let bytes = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("{}: file not found", path.display());
            return ExitCode::from(2);
        }
        Err(e) => {
            // Any other I/O error (permission denied, etc.) — same
            // "couldn't get the file" bucket. Exit 2 is the closest
            // match to the file-not-found semantic.
            eprintln!("{}: {e}", path.display());
            return ExitCode::from(2);
        }
    };

    match LoadedConfig::load_from_str(&bytes) {
        Ok(loaded) => {
            println!(
                "{}: ok ({} actions, {} scenarios)",
                path.display(),
                loaded.actions().len(),
                loaded.scenarios().len()
            );
            ExitCode::SUCCESS
        }
        Err(e) if e.is_schema_version_mismatch() => {
            eprintln!(
                "{}: schema_version mismatch (expected 1, got {})",
                path.display(),
                e.actual_version().unwrap_or(0)
            );
            ExitCode::from(3)
        }
        Err(e) => {
            match (e.line, e.col) {
                (Some(l), Some(c)) => {
                    eprintln!("{}:{l}:{c}: {}", path.display(), e.message);
                }
                _ => {
                    eprintln!("{}: {}", path.display(), e.message);
                }
            }
            ExitCode::from(1)
        }
    }
}
