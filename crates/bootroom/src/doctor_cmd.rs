//! `bootroom doctor` — preflight diagnostics for CI and operator self-service.
//!
//! Plan 05-04 fills in the real check body. Six checks run in fixed order:
//! version, qemu_wasm_rev, browser, headers, config, cli_surface. Output
//! is either human (default) or JSON (`--format json`, stable schema_version=1).
//!
//! Failure semantics:
//! - `CheckStatus::Pass` and `CheckStatus::Info` do NOT contribute to exit-1.
//! - `CheckStatus::Fail` makes the overall result fail (exit 1).
//! - A missing browser is `Info`, not `Fail` (D-DOC-02 + Research Pitfall 4).
//! - A missing `bootroom.toml` is `Info`, not `Fail` — only a parse error fails.
//! - On overall fail, a single-line summary is also written to stderr for
//!   CI grep convenience.

use crate::cli::{DoctorArgs, OutputFormat};
use std::process::ExitCode;

/// Status of a single doctor check.
///
/// Serializes to the lowercased form (`"pass"`, `"fail"`, `"info"`) per the
/// stable JSON schema contract (Research §"JSON Output Schema (formal)").
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Info,
}

/// A single doctor check result.
///
/// `name` is one of the fixed identifiers: `version`, `qemu_wasm_rev`,
/// `browser`, `headers`, `config`, `cli_surface`. The check ORDER in the
/// rendered output is the contract that 05-05's integration tests pin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

/// Run the bootroom doctor preflight checks and return an exit code.
///
/// Stub for Plan 05-03 — always returns `ExitCode::SUCCESS`. The full
/// check body lands in Plan 05-04 (the next task in this plan replaces
/// this body).
pub async fn run(_args: DoctorArgs) -> ExitCode {
    // Body lands in 05-04 Task 2.
    let _ = OutputFormat::Human; // keep the import live for Task 2 wiring.
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON shape contract (Research §"JSON Output Schema (formal)"):
    /// the per-check object MUST serialize with exactly the keys
    /// `name`, `status`, `detail`, and the status MUST be a lowercase
    /// string. This test is the TDD anchor for the JSON schema.
    #[test]
    fn check_serializes_with_lowercase_status() {
        let c = Check {
            name: "version".to_string(),
            status: CheckStatus::Pass,
            detail: "bootroom 0.1.0 (abc1234)".to_string(),
        };
        let v: serde_json::Value =
            serde_json::to_value(&c).expect("Check serializes as JSON");
        // Must have exactly these three keys.
        let obj = v.as_object().expect("Check is a JSON object");
        let keys: std::collections::BTreeSet<&str> =
            obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["name", "status", "detail"].into_iter().collect();
        assert_eq!(keys, expected, "Check JSON keys are name/status/detail");
        assert_eq!(v["name"], "version");
        assert_eq!(v["status"], "pass");
        assert_eq!(v["detail"], "bootroom 0.1.0 (abc1234)");
    }

    #[test]
    fn check_status_fail_serializes_lowercase() {
        let v = serde_json::to_value(CheckStatus::Fail).expect("status serializes");
        assert_eq!(v, serde_json::json!("fail"));
    }

    #[test]
    fn check_status_info_serializes_lowercase() {
        let v = serde_json::to_value(CheckStatus::Info).expect("status serializes");
        assert_eq!(v, serde_json::json!("info"));
    }
}
