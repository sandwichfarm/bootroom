//! `bootroom doctor` — preflight diagnostics for CI and operator self-service.
//!
//! Plan 05-03 lands only the stub so the CLI surface is callable end-to-end.
//! Plan 05-04 fills in the real check body (version, browser discovery,
//! COOP/COEP headers via tower's `oneshot`, optional config parse).

use crate::cli::DoctorArgs;
use std::process::ExitCode;

/// Run the bootroom doctor preflight checks and return an exit code.
///
/// Stub for Plan 05-03 — always returns `ExitCode::SUCCESS`. The full
/// check body (Plan 05-04) wires real preflight checks and exits 1 on
/// any failure.
pub async fn run(_args: DoctorArgs) -> ExitCode {
    // Body lands in 05-04.
    ExitCode::SUCCESS
}
