//! `bootroom run` — headless CI driver (RUN-01..10).
//!
//! Architecture (04-RESEARCH "System Architecture Diagram"):
//!   1. Parse + validate config; resolve scenario name -> exit 2 on miss.
//!   2. Discover Chromium binary (Pitfall #6 verification by --version).
//!   3. Bind axum server on 127.0.0.1:0; share `Arc<AppState>` with the
//!      pre-installed oneshot.
//!   4. Launch Chromium with Spike B's flags + `$BOOTROOM_CHROMIUM_ARGS`.
//!   5. Navigate to `http://<bound>/?scenario=<name>`.
//!   6. COI self-check via `Runtime.evaluate` (RUN-10) -> exit 3 on fail.
//!   7. Await the oneshot with timeout = `scenario.timeout_ms + 30_000`
//!      (Pitfall #8); diagnose "no serial output" vs "result missing".
//!   8. Persist JSONL transcript (`--log-file`) and stderr summary
//!      (`--verbose` / non-verbose).
//!   9. Tear down Chromium reliably -- Spike B explicit cleanup sequence,
//!      runs at EVERY post-launch exit path (no Drop guard, no
//!      `browser.clone()` -- `chromiumoxide::Browser` is not `Clone`).
//!  10. Translate verdict -> exit code 0/1.

use bootroom_core::{WsMessage, config::LoadedConfig};
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};

use crate::{
    cli::RunArgs,
    server::build_router,
    state::AppState,
    transcript::{TranscriptEvent, TranscriptWriter},
    verbose::{VerboseFormatter, non_verbose_failure_line},
};

/// Internal error variants. The `run` wrapper maps these to the two
/// non-verdict exit codes (2 / 3).
#[derive(Debug)]
enum ExitReason {
    /// Config/CLI error -- exit 2.
    ConfigError(String),
    /// Startup error (Chromium discovery, SAB unavailable, outer
    /// timeout, listener bind) -- exit 3.
    StartupError(String),
}

/// Run the `run` subcommand.
///
/// Maps an `ExitReason` to its process exit code; the verdict path is
/// translated inside `run_inner` so the success branch returns
/// `ExitCode::from(0)` or `ExitCode::from(1)` directly.
pub async fn run(args: RunArgs) -> ExitCode {
    match run_inner(args).await {
        Ok(exit) => exit,
        Err(ExitReason::ConfigError(msg)) => {
            eprintln!("bootroom run: {msg}");
            ExitCode::from(2)
        }
        Err(ExitReason::StartupError(msg)) => {
            eprintln!("bootroom run: {msg}");
            ExitCode::from(3)
        }
    }
}

/// Translate a verdict string to the binary CI exit code. Any non-`"pass"`
/// verdict (`"fail"`, `"timeout"`, `"error"`) collapses to exit 1 -- the
/// distinction lives in the JSONL transcript / verbose stderr, not in
/// the exit code.
fn verdict_to_exit(verdict: &str) -> u8 {
    u8::from(verdict != "pass")
}

#[allow(clippy::too_many_lines)]
async fn run_inner(args: RunArgs) -> Result<ExitCode, ExitReason> {
    // 1. Load + validate config.
    let cfg_path = args
        .common
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("bootroom.toml"));
    let cfg_content = std::fs::read_to_string(&cfg_path).map_err(|e| {
        ExitReason::ConfigError(format!("--config: {} ({e})", cfg_path.display()))
    })?;
    let loaded = LoadedConfig::load_from_str(&cfg_content).map_err(|e| {
        ExitReason::ConfigError(format!("{}: {}", cfg_path.display(), e.message))
    })?;

    // 2. Resolve scenario name (defense-in-depth: browser side validates again).
    let scenario = loaded
        .scenarios()
        .iter()
        .find(|s| s.name == args.scenario)
        .ok_or_else(|| {
            ExitReason::ConfigError(format!("unknown scenario '{}'", args.scenario))
        })?
        .clone();

    // 3. Validate --kernel exists.
    if !args.common.kernel.is_file() {
        return Err(ExitReason::ConfigError(format!(
            "--kernel: file not found at {}",
            args.common.kernel.display()
        )));
    }

    // 4. Canonicalize paths (lift from server::run; see Phase 3).
    let kernel_canon = std::fs::canonicalize(&args.common.kernel).map_err(|e| {
        ExitReason::ConfigError(format!("--kernel canonicalize: {e}"))
    })?;
    let cfg_canon = std::fs::canonicalize(&cfg_path).map_err(|e| {
        ExitReason::ConfigError(format!("--config canonicalize: {e}"))
    })?;

    // 5. Bind axum listener on 127.0.0.1:0.
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
        ExitReason::StartupError(format!("failed to bind 127.0.0.1:0: {e}"))
    })?;
    let bound = listener
        .local_addr()
        .map_err(|e| ExitReason::StartupError(format!("local_addr: {e}")))?;

    // 6. Build AppState with the bound address in allowed_origins.
    let state = AppState::new(
        args.common.kernel.clone(),
        kernel_canon,
        /* assets_dir: */ None,
        cfg_path,
        cfg_canon,
        loaded,
        vec![format!("http://{bound}")],
    );
    // run-mode never opens the watcher; intentional (kernel-rebuild
    // banners are a serve-mode concern).
    let state = Arc::new(state);

    // 7. Install the oneshot BEFORE Chromium navigates.
    let result_rx = state.install_scenario_oneshot().await;

    // 8. Spawn the axum server in the background.
    let app = build_router(state.clone());
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // 9. Discover and launch Chromium.
    let chromium = match discover_chromium() {
        Ok(p) => p,
        Err(msg) => {
            // server_task is the only thing live before Chromium --
            // abort it directly, no Chromium cleanup needed.
            server_task.abort();
            return Err(ExitReason::StartupError(msg));
        }
    };
    let extra_args = std::env::var("BOOTROOM_CHROMIUM_ARGS")
        .ok()
        .map(|s| {
            s.split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (mut browser, handler_task) = match launch_chromium(chromium, extra_args).await
    {
        Ok(pair) => pair,
        Err(msg) => {
            server_task.abort();
            return Err(ExitReason::StartupError(msg));
        }
    };

    // 10-12. ALL post-launch work goes in an inner async block. Its result
    // is captured BEFORE cleanup runs. Spike B shape (see
    // `spikes/spike-b/src/main.rs:175-237` -- `nav_result` is captured,
    // then cleanup runs at lines 240-243 unconditionally, then
    // `nav_result` is consumed). NO Drop guard, NO `browser.clone()`.
    let page_work: Result<WsMessage, ExitReason> = async {
        // 10. Navigate.
        let url = format!("http://{bound}/?scenario={}", args.scenario);
        let page = browser
            .new_page(url.as_str())
            .await
            .map_err(|e| ExitReason::StartupError(format!("new_page: {e}")))?;
        page.wait_for_navigation().await.map_err(|e| {
            ExitReason::StartupError(format!("wait_for_navigation: {e}"))
        })?;

        // 11. COI self-check (RUN-10).
        coi_self_check(&page).await.map_err(ExitReason::StartupError)?;

        // 12. Await ScenarioResult with outer timeout (Pitfall #8).
        let outer_timeout_ms = scenario.timeout_ms + 30_000;
        let recv = timeout(Duration::from_millis(outer_timeout_ms), result_rx)
            .await
            .map_err(|_| {
                ExitReason::StartupError(
                    "ScenarioResult not received within scenario.timeout_ms + 30s. \
                     Check that Chromium can boot the kernel and that COOP/COEP are \
                     correctly served."
                        .into(),
                )
            })?
            .map_err(|e| {
                ExitReason::StartupError(format!("ScenarioResult oneshot closed: {e}"))
            })?;
        Ok(recv)
    }
    .await;

    // 13. UNCONDITIONAL CLEANUP -- lifted verbatim from Spike B
    //     (`spikes/spike-b/src/main.rs:240-243`). Runs on success AND
    //     on every error-return from the inner block.
    //
    //     Order matters: close -> wait -> handler.abort -> server.abort.
    //     `Browser` is NOT Clone; consume `browser` here directly.
    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();
    server_task.abort();

    // 14. NOW consume the captured result. The `?` here propagates any
    //     ExitReason from the inner block; cleanup has already run.
    let result = page_work?;
    let WsMessage::ScenarioResult {
        verdict,
        transcript: tx,
        ..
    } = result.clone()
    else {
        return Err(ExitReason::StartupError(format!(
            "expected WsMessage::ScenarioResult, got {result:?}"
        )));
    };

    // 15. Persist JSONL (if --log-file).
    if let Some(log_path) = args.log_file.as_ref() {
        if let Err(e) = persist_transcript(log_path, &args, &scenario, &tx) {
            tracing::warn!(error = %e, "failed to write --log-file");
            // Non-fatal: continue to exit-code translation.
        }
    }

    // 16. Verbose / non-verbose stderr.
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    if args.common.verbose {
        let mut vf = VerboseFormatter::new(&mut stderr);
        let _ = vf.final_summary(&verdict, &args.scenario);
    } else if verdict != "pass" {
        let _ = non_verbose_failure_line(&mut stderr, &args.scenario, &verdict);
    }

    // 17. Translate verdict -> exit code.
    Ok(ExitCode::from(verdict_to_exit(&verdict)))
}

/// Discover the Chromium binary. Tries three candidates in order:
/// `$CHROMIUM` env var, `/usr/bin/chromium`, and `which chromium`.
///
/// Each candidate is verified by invoking `--version` (Pitfall #6: an
/// existence check is insufficient -- a non-Chromium binary at the
/// path would otherwise be picked up silently).
fn discover_chromium() -> Result<PathBuf, String> {
    let which_out = Command::new("which")
        .arg("chromium")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let candidates: Vec<(String, String)> = vec![
        (
            "$CHROMIUM".into(),
            std::env::var("CHROMIUM").unwrap_or_default(),
        ),
        ("/usr/bin/chromium".into(), "/usr/bin/chromium".into()),
        ("`which chromium`".into(), which_out),
    ];
    discover_chromium_with_candidates(&candidates)
}

/// Same as [`discover_chromium`] but accepts an externally-built
/// `(label, path)` candidate list -- used by tests to exercise the
/// all-missing diagnostic without depending on whether
/// `/usr/bin/chromium` exists on the developer's box.
fn discover_chromium_with_candidates(
    candidates: &[(String, String)],
) -> Result<PathBuf, String> {
    let mut errors: Vec<String> = Vec::new();
    for (label, path_str) in candidates {
        if path_str.is_empty() {
            errors.push(format!("{label}: not set"));
            continue;
        }
        let p = PathBuf::from(path_str);
        // Pitfall #6: existence check is insufficient; verify by invoking --version.
        match Command::new(&p).arg("--version").output() {
            Ok(out) if out.status.success() => return Ok(p),
            Ok(out) => errors.push(format!(
                "{label} ({path_str}): exited {} on --version",
                out.status
            )),
            Err(e) => errors.push(format!("{label} ({path_str}): {e}")),
        }
    }
    Err(format!(
        "no working Chromium binary found:\n  {}\nSet $CHROMIUM to the chromium binary path.",
        errors.join("\n  ")
    ))
}

/// Launch headless Chromium with Spike B's flags + any operator-supplied
/// extras. Returns the browser handle and the spawned event-drain task
/// so the caller can abort it during cleanup.
async fn launch_chromium(
    exe: PathBuf,
    extra_args: Vec<String>,
) -> Result<(Browser, JoinHandle<()>), String> {
    let mut builder = BrowserConfig::builder()
        .chrome_executable(exe)
        .new_headless_mode()
        .no_sandbox()
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage");
    for a in extra_args {
        builder = builder.arg(a);
    }
    let config = builder.build().map_err(|e| {
        format!(
            "BrowserConfig::build failed: {e}. \
             Set $CHROMIUM to override the chromium binary path."
        )
    })?;
    let (browser, mut handler) = Browser::launch(config).await.map_err(|e| {
        format!(
            "Browser::launch failed: {e}. \
             Check $CHROMIUM and that the binary is executable."
        )
    })?;
    let handler_task = tokio::spawn(async move {
        // Drain the CDP event stream so commands don't deadlock.
        while let Some(_event) = handler.next().await {}
    });
    Ok((browser, handler_task))
}

/// COI self-check (RUN-10): the headless page MUST report
/// `crossOriginIsolated === true` AND `typeof SharedArrayBuffer !== 'undefined'`.
/// A `false` reading means COOP/COEP headers are missing -- qemu-wasm
/// cannot run without SAB, so we fail fast with a diagnostic that
/// points at the only realistic cause.
async fn coi_self_check(page: &Page) -> Result<(), String> {
    let coi: bool = page
        .evaluate(
            "self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'",
        )
        .await
        .map_err(|e| format!("COI self-check eval failed: {e}"))?
        .into_value::<bool>()
        .unwrap_or(false);
    if !coi {
        return Err(coi_self_check_diagnostic().into());
    }
    Ok(())
}

/// Static diagnostic message emitted when the COI self-check fails.
/// Extracted into a helper so tests can pin the wording without
/// spinning up a real headless Chromium.
fn coi_self_check_diagnostic() -> &'static str {
    "crossOriginIsolated/SAB unavailable in headless Chromium. \
     This usually means COOP/COEP headers are missing from the bootroom server's \
     responses. Check `/` headers via `curl -I`; see the iso-banner in index.html for \
     the in-browser hint. (Phase 1 self-test: ensure Chromium >= 118 and --headless=new.)"
}

/// Persist the browser-built transcript to a `--log-file` path. Always
/// writes a `scenario_start` preamble (server-emitted); subsequent
/// events are deserialized from the browser-built `transcript` JSON
/// array and re-serialized via `TranscriptWriter` to enforce
/// line-atomicity and canonical key order.
fn persist_transcript(
    path: &Path,
    args: &RunArgs,
    scenario: &bootroom_core::config::Scenario,
    browser_transcript: &serde_json::Value,
) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let buf = std::io::BufWriter::new(file);
    let mut w = TranscriptWriter::new(buf);
    // Server-emitted preamble -- always written, regardless of what
    // the browser-built transcript contains. Open Q3: UTC + Z suffix.
    let now = utc_now_iso8601_z();
    w.write_event(&TranscriptEvent::ScenarioStart {
        ts: now,
        scenario: scenario.name.clone(),
        kernel: args.common.kernel.display().to_string(),
    })?;
    // Append every event from the browser-built transcript verbatim.
    if let serde_json::Value::Array(events) = browser_transcript {
        for ev_json in events {
            match serde_json::from_value::<TranscriptEvent>(ev_json.clone()) {
                Ok(ev) => w.write_event(&ev)?,
                Err(e) => {
                    tracing::warn!(error = %e, "skipping unknown transcript event");
                }
            }
        }
    }
    Ok(())
}

/// RFC 3339 UTC timestamp with millisecond precision and trailing `Z`,
/// e.g. `"2026-05-19T14:32:01.123Z"`. Byte-compatible with JS
/// `new Date().toISOString()` (which the browser-side scenario engine
/// at `web/scenario.js` uses for its own timestamps). No external dep
/// -- uses only `std::time::SystemTime` to match the project's
/// "single static binary, minimal deps" stance.
fn utc_now_iso8601_z() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_iso8601_z(now.as_secs(), now.subsec_millis())
}

/// Pure formatter for the RFC 3339 UTC + `Z` shape used by
/// `utc_now_iso8601_z`. Split out so tests can pin specific epoch
/// inputs deterministically.
///
/// The civil-from-days conversion uses Howard Hinnant's algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html#civil_from_days>),
/// which is well-known, audited, and handles leap years + the
/// Gregorian calendar correctly across all useful years.
// The civil-from-days algorithm uses Howard Hinnant's canonical short
// variable names (`doe` = day-of-era, `doy` = day-of-year, `mp` =
// month-prime, `yoe` = year-of-era). Renaming them obscures
// auditability against the published reference; allow the
// similar-names lint locally.
#[allow(clippy::similar_names)]
fn format_iso8601_z(secs_total: u64, millis: u32) -> String {
    // Days since 1970-01-01 (epoch). u64::MAX / 86_400 fits in i64 with
    // plenty of headroom (i64::MAX days ~= year 25e15 AD); the cast
    // cannot wrap for any plausible `SystemTime::now()` value.
    let days = i64::try_from(secs_total / 86_400).unwrap_or(i64::MAX);
    // seconds-of-day: 0..86_400, always fits in u32.
    let sod = u32::try_from(secs_total % 86_400).unwrap_or(0);
    let hour = sod / 3_600;
    let minute = (sod % 3_600) / 60;
    let second = sod % 60;

    // civil_from_days (Howard Hinnant). All-integer; no float.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    // rem_euclid(146_097) is bounded to 0..146_097, always fits in u32.
    let doe = u32::try_from(z.rem_euclid(146_097)).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day_of_month = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(m <= 2);

    format!(
        "{y:04}-{m:02}-{day_of_month:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn verdict_pass_yields_exit_zero() {
        assert_eq!(verdict_to_exit("pass"), 0);
    }

    #[test]
    fn verdict_fail_yields_exit_one() {
        assert_eq!(verdict_to_exit("fail"), 1);
        assert_eq!(verdict_to_exit("timeout"), 1);
        assert_eq!(verdict_to_exit("error"), 1);
        // Anything else collapses to 1 -- the wire format is open-ended.
        assert_eq!(verdict_to_exit(""), 1);
        assert_eq!(verdict_to_exit("unknown"), 1);
    }

    #[test]
    fn discover_chromium_returns_error_when_all_missing() {
        let cands = vec![
            ("$CHROMIUM".to_string(), String::new()),
            (
                "/nonexistent-bootroom-test/chromium".to_string(),
                "/nonexistent-bootroom-test/chromium".to_string(),
            ),
            ("`which chromium`".to_string(), String::new()),
        ];
        let err = discover_chromium_with_candidates(&cands)
            .expect_err("all-missing must error");
        assert!(
            err.contains("no working Chromium binary found"),
            "diagnostic must lead with the headline; got: {err}"
        );
        // All three candidate labels MUST appear in the diagnostic so
        // operators see what was tried.
        assert!(err.contains("$CHROMIUM"), "missing $CHROMIUM in: {err}");
        assert!(
            err.contains("/nonexistent-bootroom-test/chromium"),
            "missing /nonexistent-bootroom-test/chromium in: {err}"
        );
        assert!(
            err.contains("`which chromium`"),
            "missing which-chromium label in: {err}"
        );
        // Hint to set $CHROMIUM.
        assert!(
            err.contains("Set $CHROMIUM"),
            "diagnostic must hint at $CHROMIUM override; got: {err}"
        );
    }

    #[test]
    fn coi_self_check_diagnostic_mentions_headers() {
        let diag = coi_self_check_diagnostic();
        assert!(
            diag.contains("COOP/COEP"),
            "diagnostic must mention COOP/COEP so operators know where to look; got: {diag}"
        );
        assert!(
            diag.contains("crossOriginIsolated") || diag.contains("SAB"),
            "diagnostic must mention the failed predicate; got: {diag}"
        );
    }

    #[test]
    fn utc_now_iso8601_z_format_pin() {
        let s = utc_now_iso8601_z();
        let re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$")
            .expect("regex compiles");
        assert!(re.is_match(&s), "format drift: got {s}");
    }

    #[test]
    fn format_iso8601_z_epoch() {
        assert_eq!(format_iso8601_z(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_iso8601_z_known_date() {
        // 2025-05-19T17:20:00Z UTC = 1_747_675_200 seconds since epoch.
        // (Computed: (2025-1970)*365 + leap_days + days_into_2025 ...)
        // Easier: pick the well-known 2021-01-01T00:00:00Z = 1_609_459_200.
        assert_eq!(
            format_iso8601_z(1_609_459_200, 0),
            "2021-01-01T00:00:00.000Z"
        );
        // 2024-02-29T12:34:56Z (leap day) = 1_709_210_096.
        assert_eq!(
            format_iso8601_z(1_709_210_096, 789),
            "2024-02-29T12:34:56.789Z"
        );
    }

    #[test]
    fn format_iso8601_z_millis_zero_padded() {
        // Millis < 100 must zero-pad to three digits.
        assert_eq!(format_iso8601_z(0, 7), "1970-01-01T00:00:00.007Z");
        assert_eq!(format_iso8601_z(0, 42), "1970-01-01T00:00:00.042Z");
    }
}
