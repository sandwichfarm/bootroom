//! Dump the first chunk of NORN serial output verbatim.
//!
//! Companion to `spike-b` proper: the main spike captures only a
//! character count (the verdict rubric only needs `count > 0`), but
//! Phase-4 plan 04-11 needs the EXACT bytes of the boot banner so the
//! e2e fixture `tests/fixtures/boot_smoke.toml` can `contains`-assert
//! against a real observation rather than an invented string.
//!
//! Usage:
//!
//!     cargo run -p spike-b --example dump_banner -- --kernel <fixture>
//!
//! Prints the first ~512 chars of `document.querySelector('.xterm-rows').innerText`
//! observed within 30 s, JSON-encoded so escape sequences are obvious.

use anyhow::{Result, anyhow};
use bootroom::{AppState, build_router};
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let kernel = parse_kernel_arg()?;
    if !kernel.exists() {
        anyhow::bail!("--kernel: file not found at {}", kernel.display());
    }

    let state = Arc::new(AppState::new_for_test(kernel.clone(), None));
    let app = build_router(state);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    eprintln!("[dump_banner] server at http://{addr}");

    let config = BrowserConfig::builder()
        .chrome_executable(PathBuf::from("/usr/bin/chromium"))
        .new_headless_mode()
        .no_sandbox()
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .build()
        .map_err(|e| anyhow!("BrowserConfig::build: {e}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;
    let handler_task = tokio::spawn(async move {
        while let Some(_event) = handler.next().await {}
    });

    let page = browser.new_page(format!("http://{addr}/")).await?;
    page.wait_for_navigation().await?;

    // Poll for serial output up to 30s, return the longest non-empty
    // snapshot of the xterm rows we see.
    let mut best = String::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let v = page
            .evaluate(
                "(document.querySelector('.xterm-rows')?.innerText) \
                 || (document.querySelector('.xterm-screen')?.innerText) \
                 || (document.getElementById('terminal')?.innerText) \
                 || ''",
            )
            .await
            .ok()
            .and_then(|r| r.into_value::<String>().ok())
            .unwrap_or_default();
        if v.len() > best.len() {
            best = v;
        }
        // Keep polling — the banner often grows past the first sample.
        if best.len() >= 256 {
            // Give it a small grace window for late bytes, then stop.
            tokio::time::sleep(Duration::from_millis(2_000)).await;
            let v = page
                .evaluate(
                    "(document.querySelector('.xterm-rows')?.innerText) \
                     || (document.querySelector('.xterm-screen')?.innerText) \
                     || (document.getElementById('terminal')?.innerText) \
                     || ''",
                )
                .await
                .ok()
                .and_then(|r| r.into_value::<String>().ok())
                .unwrap_or_default();
            if v.len() > best.len() {
                best = v;
            }
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let _ = browser.close().await;
    let _ = browser.wait().await;
    handler_task.abort();
    server_handle.abort();

    println!("--- serial snapshot ({} chars) ---", best.len());
    println!("{best}");
    println!("--- JSON-escaped (for fixture lifting) ---");
    println!("{}", serde_json::to_string(&best)?);

    Ok(())
}

fn parse_kernel_arg() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut kernel = None;
    while let Some(arg) = args.next() {
        if arg == "--kernel" {
            kernel = args.next().map(PathBuf::from);
        }
    }
    kernel.ok_or_else(|| anyhow!("usage: dump_banner --kernel <path>"))
}
