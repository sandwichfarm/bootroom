//! Spike B driver — answers: can chromiumoxide drive headless Chromium
//! against the bootroom server, observe SAB, and exercise qemu-wasm boot?
//!
//! Always writes `SPIKE-B-RESULT.md` (even on failure). The result file
//! is the authoritative artifact Phase 4 consumes.

use anyhow::{Context, Result, anyhow};
use bootroom::{AppState, build_router};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

const RESULT_PATH: &str = "crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md";

#[derive(Debug, Default)]
struct Observations {
    coi: Option<bool>,
    sab: Option<bool>,
    user_agent: String,
    pill_state: String,
    terminal_chars: i64,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();

    let kernel = match parse_kernel_arg() {
        Ok(k) => k,
        Err(e) => {
            // Even argument-parsing failure produces a verdict file so Phase 4
            // never sees a missing artifact.
            let obs = Observations {
                error: Some(format!("argument error: {e}")),
                ..Default::default()
            };
            write_result(
                &obs,
                "red",
                "deferred",
                &format!("Spike could not start: {e}"),
                Path::new("(none)"),
            )?;
            return Err(e);
        }
    };

    if !kernel.exists() {
        let msg = format!("--kernel: file not found at {}", kernel.display());
        let obs = Observations {
            error: Some(msg.clone()),
            ..Default::default()
        };
        write_result(
            &obs,
            "red",
            "deferred",
            &format!("Fixture missing: {msg}"),
            &kernel,
        )?;
        anyhow::bail!(msg);
    }

    let result = run_spike(&kernel).await;

    match result {
        Ok((obs, verdict, chosen_path, rationale)) => {
            write_result(&obs, verdict, chosen_path, &rationale, &kernel)?;
            println!(
                "Wrote {RESULT_PATH} (verdict: {verdict}, chosen_path: {chosen_path})"
            );
            Ok(())
        }
        Err((obs, err)) => {
            let rationale = format!(
                "Spike encountered an unrecoverable error before completing observations: {err}. \
                 Phase 4 should NOT plan against chromiumoxide without re-running this spike. \
                 Consider Playwright subprocess fallback (its headless SAB story is the most-tested)."
            );
            write_result(&obs, "red", "playwright-subprocess", &rationale, &kernel)?;
            println!(
                "Wrote {RESULT_PATH} (verdict: red, chosen_path: playwright-subprocess; error: {err})"
            );
            // Do not bail — Phase 4 needs the file. The verdict captures the failure.
            Ok(())
        }
    }
}

async fn run_spike(
    kernel: &Path,
) -> std::result::Result<(Observations, &'static str, &'static str, String), (Observations, String)>
{
    let mut obs = Observations::default();

    // 1. Spawn bootroom server on ephemeral port.
    let state = Arc::new(AppState::new(kernel.to_path_buf(), None));
    let app = build_router(state);
    let listener = match TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(l) => l,
        Err(e) => return Err((obs, format!("TcpListener::bind failed: {e}"))),
    };
    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(e) => return Err((obs, format!("local_addr failed: {e}"))),
    };
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tracing::info!("bootroom test server: http://{}", addr);
    eprintln!("[spike-b] server up at http://{}", addr);

    // 2. Launch chromiumoxide.
    // Note: `new_headless_mode()` sets the internal `HeadlessMode::New` flag;
    // chromiumoxide 0.9.1 then appends `--headless=new` itself when launching
    // the child Chromium. The `HeadlessMode` enum itself isn't re-exported in
    // 0.9.1, so the builder method is the only public entry point.
    let config = match BrowserConfig::builder()
        .chrome_executable(PathBuf::from("/usr/bin/chromium"))
        .new_headless_mode()
        .no_sandbox()
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            server_handle.abort();
            return Err((
                obs,
                format!(
                    "BrowserConfig::build failed: {e}. The chromiumoxide 0.9.1 API may have shifted; \
                     see https://docs.rs/chromiumoxide/0.9.1/chromiumoxide/browser/struct.BrowserConfig.html"
                ),
            ));
        }
    };

    let (mut browser, mut handler) = match Browser::launch(config).await {
        Ok(b) => b,
        Err(e) => {
            server_handle.abort();
            return Err((
                obs,
                format!(
                    "Browser::launch failed: {e}. Likely causes: Chromium missing at /usr/bin/chromium, \
                     or chromiumoxide 0.9 API incompatibility."
                ),
            ));
        }
    };
    let handler_task = tokio::spawn(async move {
        while let Some(_event) = handler.next().await {
            // Drain the CDP event stream so commands don't deadlock.
        }
    });

    // 3. Navigate.
    let url = format!("http://{addr}/");
    let nav_result: std::result::Result<(), String> = async {
        let page = browser
            .new_page(url.as_str())
            .await
            .map_err(|e| format!("new_page failed: {e}"))?;
        page.wait_for_navigation()
            .await
            .map_err(|e| format!("wait_for_navigation failed: {e}"))?;

        // 4. Capture SAB / COI / UA observations.
        obs.coi = eval_bool(&page, "self.crossOriginIsolated").await;
        obs.sab = eval_bool(&page, "typeof SharedArrayBuffer !== 'undefined'").await;
        obs.user_agent = eval_string(&page, "navigator.userAgent")
            .await
            .unwrap_or_else(|| "(unavailable)".to_string());

        // 5. Poll for pill state + terminal bytes (up to 15s).
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            obs.pill_state = eval_string(
                &page,
                "document.getElementById('status')?.dataset?.state || 'UNKNOWN'",
            )
            .await
            .unwrap_or_else(|| "EVAL_ERR".to_string());

            obs.terminal_chars = eval_i64(
                &page,
                "(document.querySelector('.xterm-rows')?.innerText?.length) \
                 || (document.querySelector('.xterm-screen')?.innerText?.length) \
                 || (document.getElementById('terminal')?.innerText?.length) \
                 || 0",
            )
            .await
            .unwrap_or(0);

            if obs.pill_state == "RUNNING" && obs.terminal_chars > 0 {
                // Give the kernel ~1.5 s more to dump its boot banner so the
                // observation reflects steady-state serial throughput, not the
                // first character that happened to arrive.
                tokio::time::sleep(Duration::from_millis(1_500)).await;
                obs.terminal_chars = eval_i64(
                    &page,
                    "(document.querySelector('.xterm-rows')?.innerText?.length) \
                     || (document.querySelector('.xterm-screen')?.innerText?.length) \
                     || (document.getElementById('terminal')?.innerText?.length) \
                     || 0",
                )
                .await
                .unwrap_or(obs.terminal_chars);
                break;
            }
            if obs.pill_state == "HALTED" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }

        Ok(())
    }
    .await;

    // Cleanup browser and handler before returning, even on error.
    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();
    server_handle.abort();

    if let Err(e) = nav_result {
        obs.error = Some(e.clone());
        return Err((obs, e));
    }

    let (verdict, chosen_path, rationale) = compute_verdict(
        obs.coi.unwrap_or(false),
        obs.sab.unwrap_or(false),
        &obs.pill_state,
        obs.terminal_chars,
    );

    Ok((obs, verdict, chosen_path, rationale))
}

async fn eval_bool(page: &chromiumoxide::Page, expr: &str) -> Option<bool> {
    match page.evaluate(expr).await {
        Ok(v) => v.into_value::<bool>().ok(),
        Err(_) => None,
    }
}

async fn eval_string(page: &chromiumoxide::Page, expr: &str) -> Option<String> {
    match page.evaluate(expr).await {
        Ok(v) => v.into_value::<String>().ok(),
        Err(_) => None,
    }
}

async fn eval_i64(page: &chromiumoxide::Page, expr: &str) -> Option<i64> {
    match page.evaluate(expr).await {
        Ok(v) => v.into_value::<i64>().ok(),
        Err(_) => None,
    }
}

fn parse_kernel_arg() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut kernel = None;
    while let Some(arg) = args.next() {
        if arg == "--kernel" {
            kernel = args.next().map(PathBuf::from);
        }
    }
    kernel.ok_or_else(|| anyhow!("usage: spike-b --kernel <path>"))
}

fn compute_verdict(
    coi: bool,
    sab: bool,
    pill: &str,
    term_chars: i64,
) -> (&'static str, &'static str, String) {
    if !coi || !sab {
        return (
            "red",
            "playwright-subprocess",
            "crossOriginIsolated/SAB not available in headless Chromium with the documented \
             COOP/COEP setup. Fall back to Playwright (its headless SAB config is the most-tested \
             in the industry)."
                .into(),
        );
    }
    if pill == "RUNNING" && term_chars > 0 {
        return (
            "green",
            "chromiumoxide",
            "Full success: COI=true, SAB=true, qemu-wasm reached RUNNING, serial bytes observed in xterm."
                .into(),
        );
    }
    if pill == "HALTED" && term_chars > 0 {
        return (
            "amber",
            "chromiumoxide",
            "Partial: SAB worked, qemu-wasm executed and produced serial output, but guest halted \
             (likely fixture/kernel issue, not bootroom). Phase 4 should proceed with chromiumoxide."
                .into(),
        );
    }
    if pill == "HALTED" {
        return (
            "amber",
            "chromiumoxide",
            "Partial: SAB worked, qemu-wasm reached onAbort/onExit but no serial bytes (fixture \
             likely panicked early or argv mismatch). Phase 4 viable but needs a better fixture to \
             fully validate."
                .into(),
        );
    }
    (
        "amber",
        "chromiumoxide",
        format!(
            "Timeout: SAB available but qemu-wasm did not reach RUNNING/HALTED within 15s. \
             Final pill state: {pill}, terminal chars: {term_chars}. Re-run with a real kernel \
             fixture before going green."
        ),
    )
}

fn write_result(
    obs: &Observations,
    verdict: &str,
    chosen_path: &str,
    rationale: &str,
    kernel: &Path,
) -> Result<()> {
    let body = format_result_md(obs, verdict, chosen_path, rationale, kernel);
    if let Some(parent) = Path::new(RESULT_PATH).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(RESULT_PATH, body).with_context(|| format!("write {RESULT_PATH}"))
}

fn format_result_md(
    obs: &Observations,
    verdict: &str,
    chosen_path: &str,
    rationale: &str,
    kernel: &Path,
) -> String {
    let date = today_utc_string();
    let coi = obs
        .coi
        .map_or_else(|| "(unobserved)".to_string(), |v| v.to_string());
    let sab = obs
        .sab
        .map_or_else(|| "(unobserved)".to_string(), |v| v.to_string());
    let ua = if obs.user_agent.is_empty() {
        "(unobserved)".to_string()
    } else {
        obs.user_agent.clone()
    };
    let error_section = obs.error.as_ref().map_or_else(
        String::new,
        |e| format!("\n## Error\n\n```\n{e}\n```\n"),
    );

    let phase4_hint = match verdict {
        "red" => "\n> Phase 4 should NOT plan against chromiumoxide. Vendor a Playwright \
                  subprocess driver (Node-based) invoked from the bootroom binary as a child \
                  process. The driver Cargo crate stays minimal; Playwright's wider-tested \
                  headless SAB story compensates for the Node dependency on CI runners. \
                  See 01-RESEARCH.md \"Stack Patterns by Variant\" headless fallback.\n",
        "amber" => "\n> Phase 4 plans against chromiumoxide. The Phase 4 plan MUST include an \
                    additional 'real-kernel headless smoke' task before committing.\n",
        "green" => "\n> Phase 4 plans against chromiumoxide. No additional spike work required.\n",
        _ => "",
    };

    format!(
        r#"---
spike: B
verdict: {verdict}
chosen_path: {chosen_path}
date: {date}
chromium_user_agent: {ua}
---

## Question

Can `chromiumoxide` drive `--headless=new` Chromium against a bare axum + COOP/COEP server,
observe `crossOriginIsolated === true`, and successfully execute the qemu-wasm boot path
with serial bytes flowing?

## Method

1. Spawned in-process bootroom server (`bootroom::build_router`) on ephemeral 127.0.0.1 port.
2. Launched headless Chromium via chromiumoxide 0.9.1 (`--headless=new`, `--disable-gpu`,
   `--no-sandbox`, `--disable-dev-shm-usage`, executable `/usr/bin/chromium`).
3. Navigated to the server root, waited for `wait_for_navigation()`.
4. Evaluated `self.crossOriginIsolated` and `typeof SharedArrayBuffer !== 'undefined'`.
5. Polled `#status` `data-state` and terminal `innerText.length` every 250 ms for up to 15 s.
6. Captured user-agent and computed verdict via the rubric in `src/main.rs::compute_verdict`.

Kernel fixture: `{kernel_path}`

## Observations

| Observable | Value |
|------------|-------|
| `self.crossOriginIsolated` | `{coi}` |
| `typeof SharedArrayBuffer !== 'undefined'` | `{sab}` |
| Final `#status data-state` | `{pill}` |
| Terminal char count (approx) | `{term_chars}` |
| Chromium user-agent | `{ua}` |

## Decision

**Verdict: {verdict}**
**Chosen path for Phase 4: {chosen_path}**

{rationale}
{phase4_hint}
## Follow-ups

- If `amber`/`red`: re-run with a known-good RISC-V kernel fixture to disambiguate
  fixture-vs-bootroom failure modes.
- Phase 4 planning consumes `chosen_path` directly. If `playwright-subprocess`, vendor a
  Node-based Playwright driver (see 01-RESEARCH.md "Stack Patterns by Variant" → headless
  fallback). A working Playwright path was independently proven during plan 01-07's smoke
  testing using `/usr/lib/node_modules/playwright` with `executablePath: '/usr/bin/chromium'`.
- Spike A (plan 01-09) is independent of this verdict; runs next.
{error_section}"#,
        verdict = verdict,
        chosen_path = chosen_path,
        date = date,
        ua = ua,
        kernel_path = kernel.display(),
        coi = coi,
        sab = sab,
        pill = if obs.pill_state.is_empty() {
            "(unobserved)"
        } else {
            obs.pill_state.as_str()
        },
        term_chars = obs.terminal_chars,
        rationale = rationale,
        phase4_hint = phase4_hint,
        error_section = error_section,
    )
}

/// Best-effort `YYYY-MM-DD` from `SystemTime::now()` without pulling chrono.
/// Civil-from-days algorithm by Howard Hinnant (public domain).
fn today_utc_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
