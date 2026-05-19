//! `bootroom doctor` — preflight diagnostics for CI and operator self-service.
//!
//! Plan 05-04 fills in the full check body. Six checks run in fixed order:
//! `version`, `qemu_wasm_rev`, `browser`, `headers`, `config`, `cli_surface`.
//! Output is either human (default) or JSON (`--format json`, stable
//! schema_version=1).
//!
//! Failure semantics:
//! - `CheckStatus::Pass` and `CheckStatus::Info` do NOT contribute to exit-1.
//! - `CheckStatus::Fail` makes the overall result fail (exit 1).
//! - A missing browser is `Info`, not `Fail` (D-DOC-02 + Research Pitfall 4).
//! - A missing `bootroom.toml` is `Info`, not `Fail` — only a parse error fails.
//! - On overall fail, a single-line summary is also written to stderr for
//!   CI grep convenience.
//!
//! All glyphs are ASCII (`+`, `-`, `~`) per Research Open Q1 — this overrides
//! the unicode mention in CONTEXT.md (documented in 05-PLAN.md).

// `coop`/`coep` are header acronyms (one-letter difference is intentional),
// and the function names `format_human` / `format_json` are operator-facing
// public API. The pedantic linter flags both — silence at module scope.
#![allow(
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::must_use_candidate
)]

use crate::cli::{DoctorArgs, OutputFormat};
use std::path::Path;
use std::process::ExitCode;

// -------- Public data shapes --------

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

/// Top-level JSON report shape (Research §"JSON Output Schema (formal)").
///
/// `schema_version` is the stability contract — downstream CI tooling can
/// safely pin against this value. Future schema bumps will increment this
/// integer.
#[derive(Debug, Clone, serde::Serialize)]
struct Report<'a> {
    schema_version: u32,
    version: &'a str,
    git_sha: &'a str,
    checks: &'a [Check],
    overall: &'static str,
}

// -------- Entrypoint --------

/// Run the bootroom doctor preflight checks and return an exit code.
///
/// Exit code:
/// - `0` on overall pass (Info-only or all-Pass results, including a missing
///   browser).
/// - `1` if any required check is `Fail` (headers, config parse error).
pub async fn run(args: DoctorArgs) -> ExitCode {
    let checks = vec![
        check_version(),
        check_qemu_rev(),
        check_browser().await,
        check_headers().await,
        check_config(args.config.as_deref()),
        check_cli_surface(),
    ];
    let overall_failed = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
    match args.format {
        OutputFormat::Human => {
            println!("{}", format_human(&checks, overall_failed));
            if overall_failed {
                let failed_names: Vec<&str> = checks
                    .iter()
                    .filter(|c| matches!(c.status, CheckStatus::Fail))
                    .map(|c| c.name.as_str())
                    .collect();
                eprintln!(
                    "bootroom doctor: {}/{} checks failed ({})",
                    failed_names.len(),
                    checks.len(),
                    failed_names.join(", "),
                );
            }
        }
        OutputFormat::Json => {
            println!("{}", format_json(&checks, overall_failed));
        }
    }
    if overall_failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

// -------- Individual checks --------

/// Check 1: bootroom version + git SHA (compile-time constants).
///
/// `BOOTROOM_GIT_SHA` is captured by `build.rs` (Plan 05-01). When the
/// build host has no git or no repo, the value is the literal `"unknown"`.
fn check_version() -> Check {
    let version = env!("CARGO_PKG_VERSION");
    let sha = env!("BOOTROOM_GIT_SHA");
    Check {
        name: "version".to_string(),
        status: CheckStatus::Info,
        detail: format!("bootroom {version} ({sha})"),
    }
}

/// Check 2: qemu-wasm revision embedded at compile time.
///
/// The sentinel value `"unknown"` is returned when the file is missing or
/// `make qemu-assets` has not been run on a real qemu-wasm checkout. The
/// sentinel itself is committed by Plan 05-02 to keep the doctor's check
/// total-render shape stable.
fn check_qemu_rev() -> Check {
    let rev = crate::embed::QEMU
        .get_file("qemu-wasm-rev.txt")
        .and_then(|f| f.contents_utf8())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    Check {
        name: "qemu_wasm_rev".to_string(),
        status: CheckStatus::Info,
        detail: format!("qemu-wasm rev {rev}"),
    }
}

/// Check 3: locate a chromium binary for `bootroom run`.
///
/// A missing browser is INFORMATIONAL (status=Info), never a failure —
/// `bootroom serve` does not need chromium, and CI runners that only
/// exercise the server (or run their own driver) are still healthy.
///
/// WR-01: this function is `async` and uses `tokio::process::Command` so
/// the `--version` probe does not block a tokio worker. A hung or
/// slow-to-start chromium would otherwise freeze the executor for the
/// duration of the probe (Doctor advertises a ~100 ms target, and the
/// in-process router self-check that follows shares the same executor).
async fn check_browser() -> Check {
    match crate::run_cmd::discover_chromium() {
        Ok(path) => {
            // Probe `--version` to confirm the binary is a real browser
            // and capture the human-friendly version string. A failed
            // probe downgrades to Info (NOT Fail) — the binary exists
            // but is uncooperative; not a CI gating concern.
            let probe = tokio::process::Command::new(&path)
                .arg("--version")
                .output()
                .await;
            match probe {
                Ok(out) if out.status.success() => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let version_line =
                        stdout.lines().next().unwrap_or("").trim().to_string();
                    let detail = if version_line.is_empty() {
                        format!("{}", path.display())
                    } else {
                        format!("{} ({version_line})", path.display())
                    };
                    Check {
                        name: "browser".to_string(),
                        status: CheckStatus::Pass,
                        detail,
                    }
                }
                _ => Check {
                    name: "browser".to_string(),
                    status: CheckStatus::Info,
                    detail: format!("{} (--version probe failed)", path.display()),
                },
            }
        }
        Err(_) => Check {
            name: "browser".to_string(),
            status: CheckStatus::Info,
            detail: "not found on PATH; install for `bootroom run`".to_string(),
        },
    }
}

/// Check 4: COOP/COEP headers via in-process router self-check.
///
/// Uses `tower::ServiceExt::oneshot` against the canonical `build_router`
/// so a header regression in tower-http middleware (or in the layer
/// stack ordering) trips immediately. Bounded by router logic — no
/// network, no kernel I/O.
///
/// Exposed `pub` so integration tests in `tests/doctor_headers_check.rs`
/// can call this directly (Option A in 05-05-PLAN.md) — the load-bearing
/// regression test for Phase-1's COOP/COEP middleware lives outside this
/// crate.
pub async fn check_headers() -> Check {
    use std::sync::Arc;

    // `new_for_test` tolerates a non-existent kernel path (state.rs:157).
    // We pass a placeholder under temp_dir so we never accidentally
    // canonicalize-touch some real artifact. WR-03: scope the placeholder
    // path with the current pid so parallel test runners (and any other
    // concurrent doctor invocation on a shared host) get distinct names
    // and do not race on a fixed filename.
    let kernel = std::env::temp_dir()
        .join(format!("bootroom-doctor-noop-{}", std::process::id()));
    let state = Arc::new(crate::AppState::new_for_test(kernel, None));
    let app = crate::build_router(state);
    check_headers_against_router(app).await
}

/// Run the COOP/COEP header check against an arbitrary `axum::Router`.
///
/// Factored out of [`check_headers`] (WR-04) so unit tests can pin the
/// Fail-detail wording without spinning up a full `AppState`. Production
/// callers use [`check_headers`] which wires in the canonical
/// `build_router` against a placeholder kernel.
pub async fn check_headers_against_router(app: axum::Router) -> Check {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    let resp = match app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("/ request builds"),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Check {
                name: "headers".to_string(),
                status: CheckStatus::Fail,
                detail: format!("router oneshot failed: {e}"),
            };
        }
    };
    let coop = resp
        .headers()
        .get("cross-origin-opener-policy")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let coep = resp
        .headers()
        .get("cross-origin-embedder-policy")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if coop.as_deref() == Some("same-origin") && coep.as_deref() == Some("require-corp") {
        Check {
            name: "headers".to_string(),
            status: CheckStatus::Pass,
            detail: "COOP=same-origin, COEP=require-corp on /".to_string(),
        }
    } else {
        Check {
            name: "headers".to_string(),
            status: CheckStatus::Fail,
            detail: format!(
                "expected COOP=same-origin, COEP=require-corp; got COOP={coop:?}, COEP={coep:?}"
            ),
        }
    }
}

/// Check 5: config file presence and parse validity.
///
/// Resolution rules:
/// - If `args.config` is `Some(p)`, read `p` (or info on NotFound).
/// - Otherwise, look for `./bootroom.toml` in CWD (or info on NotFound).
/// - Parse via the canonical `LoadedConfig::load_from_str` so doctor's
///   acceptance shape matches `bootroom check` exactly.
fn check_config(config: Option<&Path>) -> Check {
    use bootroom_core::config::LoadedConfig;

    let (path_buf, explicit) = match config {
        Some(p) => (p.to_path_buf(), true),
        None => (std::path::PathBuf::from("bootroom.toml"), false),
    };

    let bytes = match std::fs::read_to_string(&path_buf) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let detail = if explicit {
                format!("no bootroom.toml at {}", path_buf.display())
            } else {
                "no bootroom.toml in CWD (use --config to specify)".to_string()
            };
            return Check {
                name: "config".to_string(),
                status: CheckStatus::Info,
                detail,
            };
        }
        Err(e) => {
            // WR-02: distinguish common io::ErrorKind variants so the
            // operator-facing detail string points at the actual root
            // cause rather than collapsing every non-NotFound failure
            // into one generic Fail. PermissionDenied and "this is a
            // directory" are the most common usage-error shapes; the
            // catch-all preserves the raw error rendering.
            let kind_hint = match e.kind() {
                std::io::ErrorKind::PermissionDenied => " (permission denied)",
                std::io::ErrorKind::IsADirectory => " (is a directory, not a file)",
                _ => "",
            };
            return Check {
                name: "config".to_string(),
                status: CheckStatus::Fail,
                detail: format!("{}: {e}{kind_hint}", path_buf.display()),
            };
        }
    };

    match LoadedConfig::load_from_str(&bytes) {
        Ok(loaded) => Check {
            name: "config".to_string(),
            status: CheckStatus::Pass,
            detail: format!(
                "{}: {} actions, {} scenarios",
                path_buf.display(),
                loaded.actions().len(),
                loaded.scenarios().len(),
            ),
        },
        Err(e) => {
            let detail = match (e.line, e.col) {
                (Some(l), Some(c)) => {
                    format!("{}:{l}:{c}: {}", path_buf.display(), e.message)
                }
                _ => format!("{}: {}", path_buf.display(), e.message),
            };
            Check {
                name: "config".to_string(),
                status: CheckStatus::Fail,
                detail,
            }
        }
    }
}

/// Check 6: CLI subcommand surface (hardcoded for v1).
///
/// 05-05's integration test pins this string against the subcommand list
/// rendered by `bootroom --help`, so any new subcommand will trip the
/// pin and force this constant to be updated in lockstep.
fn check_cli_surface() -> Check {
    Check {
        name: "cli_surface".to_string(),
        status: CheckStatus::Info,
        detail: "serve, run, check, init, doctor".to_string(),
    }
}

// -------- Formatters --------

/// Column-aligned name width for the human formatter. 14 chars is the
/// widest current check name (`qemu_wasm_rev` = 13 chars) plus one
/// space. Recorded in 05-04-SUMMARY.md as the contract value.
const HUMAN_NAME_WIDTH: usize = 14;

fn glyph_for(status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => crate::verbose::GLYPH_PASS,
        CheckStatus::Fail => crate::verbose::GLYPH_FAIL,
        CheckStatus::Info => crate::verbose::GLYPH_INFO,
    }
}

/// Look up a check by name. Returns a placeholder `Info` line when the
/// check is absent (defensive — Task 2 always emits all six, but this
/// keeps the formatter robust if a future refactor drops one).
fn find_check<'a>(checks: &'a [Check], name: &str) -> Option<&'a Check> {
    checks.iter().find(|c| c.name == name)
}

fn render_check_line(c: &Check) -> String {
    format!(
        "{}{:<width$} {}",
        glyph_for(&c.status),
        c.name,
        c.detail,
        width = HUMAN_NAME_WIDTH
    )
}

/// Format the human-format preflight report.
///
/// Section grouping is fixed (matches Research §"Human Output Format"):
/// Version (version + qemu_wasm_rev), Browser, Server headers, Config,
/// CLI surface, followed by a final `Overall: <pass|fail>` line.
pub fn format_human(checks: &[Check], overall_failed: bool) -> String {
    let mut out = String::new();
    out.push_str("bootroom doctor - preflight checks\n\n");

    out.push_str("## Version\n");
    if let Some(c) = find_check(checks, "version") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    if let Some(c) = find_check(checks, "qemu_wasm_rev") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("## Browser\n");
    if let Some(c) = find_check(checks, "browser") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("## Server headers\n");
    if let Some(c) = find_check(checks, "headers") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("## Config\n");
    if let Some(c) = find_check(checks, "config") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("## CLI surface\n");
    if let Some(c) = find_check(checks, "cli_surface") {
        out.push_str(&render_check_line(c));
        out.push('\n');
    }
    out.push('\n');

    // WR-05: catch-all section. Any check whose name does not match one
    // of the six known section templates above renders here instead of
    // silently disappearing. Without this, a future refactor that adds
    // a seventh check would: (a) trip the JSON exact-set membership test
    // (good), but (b) make the new check vanish from the human report
    // even though it still contributes to `overall_failed` — leaving an
    // exit-1 doctor run with no visible failure reason. KNOWN_NAMES is
    // pinned to the six rendered above; adding a new check name to that
    // list (without adding a section template above) is intentional and
    // routes it through "## Other".
    const KNOWN_NAMES: &[&str] = &[
        "version",
        "qemu_wasm_rev",
        "browser",
        "headers",
        "config",
        "cli_surface",
    ];
    let unknown: Vec<&Check> = checks
        .iter()
        .filter(|c| !KNOWN_NAMES.contains(&c.name.as_str()))
        .collect();
    if !unknown.is_empty() {
        out.push_str("## Other\n");
        for c in unknown {
            out.push_str(&render_check_line(c));
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str(if overall_failed {
        "Overall: fail"
    } else {
        "Overall: pass"
    });
    out
}

/// Format the JSON preflight report. `schema_version: 1` is the stability
/// contract. `version` and `git_sha` are top-level so CI tooling can pin
/// without parsing the per-check `detail` string.
///
/// # Panics
///
/// Panics if `serde_json::to_string_pretty` fails on the in-crate
/// `Report` shape — which can only happen if a future schema field
/// derives a non-`Serialize` value, i.e. it is a compile-time concern
/// surfaced at runtime via this `expect`.
pub fn format_json(checks: &[Check], overall_failed: bool) -> String {
    let report = Report {
        schema_version: 1,
        version: env!("CARGO_PKG_VERSION"),
        git_sha: env!("BOOTROOM_GIT_SHA"),
        checks,
        overall: if overall_failed { "fail" } else { "pass" },
    };
    serde_json::to_string_pretty(&report).expect("Report serializes to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- JSON shape contract (Task 1 anchors) -----

    #[test]
    fn check_serializes_with_lowercase_status() {
        let c = Check {
            name: "version".to_string(),
            status: CheckStatus::Pass,
            detail: "bootroom 0.1.0 (abc1234)".to_string(),
        };
        let v: serde_json::Value =
            serde_json::to_value(&c).expect("Check serializes as JSON");
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

    // ----- format_json: schema_version + overall semantics -----

    #[test]
    fn format_json_schema_version_is_one() {
        let s = format_json(&[], false);
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        assert_eq!(v["schema_version"], 1);
    }

    #[test]
    fn format_json_top_level_keys_pinned() {
        let s = format_json(&[], false);
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        let obj = v.as_object().expect("top-level is object");
        let keys: std::collections::BTreeSet<&str> =
            obj.keys().map(String::as_str).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["schema_version", "version", "git_sha", "checks", "overall"]
                .into_iter()
                .collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn format_json_overall_pass_when_no_fails() {
        let checks = vec![
            Check {
                name: "version".to_string(),
                status: CheckStatus::Info,
                detail: "x".to_string(),
            },
            Check {
                name: "headers".to_string(),
                status: CheckStatus::Pass,
                detail: "y".to_string(),
            },
        ];
        let overall_failed = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
        let s = format_json(&checks, overall_failed);
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        assert_eq!(v["overall"], "pass");
    }

    #[test]
    fn format_json_overall_fail_when_any_fail() {
        let checks = vec![
            Check {
                name: "version".to_string(),
                status: CheckStatus::Info,
                detail: "x".to_string(),
            },
            Check {
                name: "headers".to_string(),
                status: CheckStatus::Fail,
                detail: "y".to_string(),
            },
        ];
        let overall_failed = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
        let s = format_json(&checks, overall_failed);
        let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        assert_eq!(v["overall"], "fail");
    }

    // ----- format_human: section headers + glyphs -----

    #[test]
    fn format_human_contains_section_headers() {
        let checks = vec![
            check_version(),
            check_qemu_rev(),
            Check {
                name: "browser".to_string(),
                status: CheckStatus::Info,
                detail: "x".to_string(),
            },
            Check {
                name: "headers".to_string(),
                status: CheckStatus::Pass,
                detail: "y".to_string(),
            },
            Check {
                name: "config".to_string(),
                status: CheckStatus::Info,
                detail: "z".to_string(),
            },
            check_cli_surface(),
        ];
        let s = format_human(&checks, false);
        for h in [
            "## Version",
            "## Browser",
            "## Server headers",
            "## Config",
            "## CLI surface",
            "Overall: pass",
        ] {
            assert!(s.contains(h), "human output missing {h}; full:\n{s}");
        }
    }

    #[test]
    fn format_human_uses_ascii_glyphs() {
        let checks = vec![Check {
            name: "version".to_string(),
            status: CheckStatus::Pass,
            detail: "bootroom 0.0.0 (xx)".to_string(),
        }];
        let s = format_human(&checks, false);
        // Must contain the ASCII pass glyph; must NOT contain unicode
        // check/x marks that CONTEXT.md originally hinted at.
        assert!(s.contains("+ "), "ASCII pass glyph missing");
        for non_ascii in ['\u{2713}', '\u{2717}', '\u{2013}', '\u{2014}'] {
            assert!(
                !s.contains(non_ascii),
                "unicode glyph U+{:04X} leaked into human output:\n{s}",
                non_ascii as u32
            );
        }
    }

    #[test]
    fn format_human_overall_fail_string() {
        let s = format_human(&[], true);
        assert!(s.ends_with("Overall: fail"));
    }

    // ----- WR-05: unknown check names appear under "## Other" -----

    #[test]
    fn format_human_renders_unknown_checks_under_other_section() {
        let checks = vec![
            Check {
                name: "future_check".to_string(),
                status: CheckStatus::Fail,
                detail: "hypothetical seventh check".to_string(),
            },
        ];
        let s = format_human(&checks, true);
        assert!(
            s.contains("## Other"),
            "unknown check must surface under '## Other'; got:\n{s}"
        );
        assert!(
            s.contains("future_check"),
            "unknown check name must appear in rendered output; got:\n{s}"
        );
        assert!(
            s.contains("hypothetical seventh check"),
            "unknown check detail must appear; got:\n{s}"
        );
    }

    #[test]
    fn format_human_omits_other_section_when_all_checks_known() {
        let checks = vec![
            Check {
                name: "version".to_string(),
                status: CheckStatus::Info,
                detail: "x".to_string(),
            },
            Check {
                name: "headers".to_string(),
                status: CheckStatus::Pass,
                detail: "y".to_string(),
            },
        ];
        let s = format_human(&checks, false);
        assert!(
            !s.contains("## Other"),
            "no Other section when every check name is recognized; got:\n{s}"
        );
    }

    // ----- browser=Info does not set overall=fail -----

    #[test]
    fn browser_status_info_does_not_set_overall_fail() {
        let checks = [Check {
            name: "browser".to_string(),
            status: CheckStatus::Info,
            detail: "not found".to_string(),
        }];
        let overall_failed = checks.iter().any(|c| matches!(c.status, CheckStatus::Fail));
        assert!(!overall_failed, "Info must not contribute to overall fail");
    }

    // ----- check_headers: load-bearing self-check (Pitfall 5) -----

    #[tokio::test]
    async fn check_headers_passes_against_build_router() {
        let c = check_headers().await;
        assert_eq!(c.name, "headers");
        assert_eq!(
            c.status,
            CheckStatus::Pass,
            "expected Pass on canonical router; got detail = {}",
            c.detail
        );
        assert!(
            c.detail.contains("same-origin"),
            "detail should mention same-origin; got: {}",
            c.detail
        );
    }

    // ----- WR-04: negative test pins Fail detail wording -----

    #[tokio::test]
    async fn check_headers_fails_on_bare_router_with_specific_detail() {
        // A bare router with no COOP/COEP middleware is the simplest way
        // to simulate "the Phase-1 cross-origin-isolation layer regressed
        // out". Pin the Fail-detail wording so a future refactor that
        // changes the message shape (e.g. drops the `expected …; got …`
        // contract) trips this test.
        use axum::{routing::get, Router};
        let bare = Router::new().route("/", get(|| async { "ok" }));
        let c = check_headers_against_router(bare).await;
        assert_eq!(c.name, "headers");
        assert!(
            matches!(c.status, CheckStatus::Fail),
            "bare router must produce Fail; got {:?} detail={}",
            c.status,
            c.detail
        );
        assert!(
            c.detail.starts_with("expected COOP=same-origin, COEP=require-corp"),
            "Fail detail must start with the expected-COOP/COEP contract; got: {}",
            c.detail
        );
        assert!(
            c.detail.contains("got COOP=None"),
            "Fail detail must report `got COOP=None` when header missing; got: {}",
            c.detail
        );
        assert!(
            c.detail.contains("COEP=None"),
            "Fail detail must report `COEP=None` when header missing; got: {}",
            c.detail
        );
    }

    // ----- check_qemu_rev: embedded file read -----

    #[test]
    fn check_qemu_rev_reads_embedded_file() {
        let c = check_qemu_rev();
        assert_eq!(c.name, "qemu_wasm_rev");
        assert!(
            matches!(c.status, CheckStatus::Info),
            "qemu_wasm_rev is always Info; got {:?}",
            c.status
        );
        assert!(
            c.detail.starts_with("qemu-wasm rev "),
            "detail must start with 'qemu-wasm rev '; got: {}",
            c.detail
        );
        // The captured rev is either the sentinel or a non-empty token.
        let rev = c.detail.trim_start_matches("qemu-wasm rev ").trim();
        assert!(!rev.is_empty(), "rev token must be non-empty");
    }

    // ----- check_config: missing is Info, broken is Fail -----

    #[test]
    fn check_config_missing_is_info_not_fail() {
        // WR-03: use tempfile::tempdir() so the missing-file path is
        // process-isolated. We name a file inside a fresh tempdir that
        // we never create, giving us a guaranteed-non-existent path with
        // no cross-test or cross-runner contention.
        let dir = tempfile::tempdir().expect("mkdir tmp");
        let missing = dir.path().join("no-such-file.toml");
        let c = check_config(Some(&missing));
        assert_eq!(c.name, "config");
        assert!(
            matches!(c.status, CheckStatus::Info),
            "missing config must be Info; got {:?} (detail={})",
            c.status,
            c.detail
        );
        assert!(
            c.detail.contains("no bootroom.toml"),
            "missing-config detail must mention 'no bootroom.toml'; got: {}",
            c.detail
        );
    }

    #[test]
    fn check_config_missing_default_is_info() {
        // With no --config and (likely) no bootroom.toml in CWD, the
        // result must be Info. NOTE: if the test runner is launched
        // from a directory that DOES contain a valid bootroom.toml,
        // the result is Pass — which also must not be Fail. So we
        // assert only the not-Fail property.
        let c = check_config(None);
        assert_eq!(c.name, "config");
        assert!(
            !matches!(c.status, CheckStatus::Fail),
            "default no-config path must not be Fail; detail={}",
            c.detail
        );
    }

    #[test]
    fn check_config_broken_toml_is_fail() {
        // WR-03: tempfile::tempdir() is process-isolated and auto-cleans
        // on drop, eliminating the cross-test / cross-user contention
        // risks of a hard-coded /tmp/bootroom-doctor-* path.
        let dir = tempfile::tempdir().expect("mkdir tmp");
        let p = dir.path().join("bad.toml");
        std::fs::write(&p, "this is not valid toml [[[\n").expect("write bad toml");
        let c = check_config(Some(&p));
        assert_eq!(c.name, "config");
        assert!(
            matches!(c.status, CheckStatus::Fail),
            "broken toml must be Fail; got {:?} detail={}",
            c.status,
            c.detail
        );
    }

    #[test]
    fn check_config_valid_toml_is_pass() {
        // WR-03: tempfile::tempdir() — see check_config_broken_toml_is_fail.
        let dir = tempfile::tempdir().expect("mkdir tmp");
        let p = dir.path().join("good.toml");
        std::fs::write(&p, "schema_version = 1\n").expect("write good toml");
        let c = check_config(Some(&p));
        assert_eq!(c.name, "config");
        assert!(
            matches!(c.status, CheckStatus::Pass),
            "valid toml must be Pass; got {:?} detail={}",
            c.status,
            c.detail
        );
        assert!(c.detail.contains("0 actions"));
        assert!(c.detail.contains("0 scenarios"));
    }

    // ----- check_version + check_cli_surface: shape pins -----

    #[test]
    fn check_version_detail_shape() {
        let c = check_version();
        assert_eq!(c.name, "version");
        assert!(matches!(c.status, CheckStatus::Info));
        // Exact prefix is the 05-05 pin contract.
        assert!(
            c.detail.starts_with("bootroom "),
            "detail must start with 'bootroom '; got: {}",
            c.detail
        );
        assert!(
            c.detail.contains('('),
            "detail must contain the '(<sha>)' segment; got: {}",
            c.detail
        );
    }

    #[test]
    fn check_cli_surface_lists_all_subcommands() {
        let c = check_cli_surface();
        assert_eq!(c.name, "cli_surface");
        assert_eq!(c.detail, "serve, run, check, init, doctor");
    }
}
