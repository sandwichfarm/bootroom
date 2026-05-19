//! 04-10 — RUN-09 pin: --verbose stderr line shape (ASCII-only,
//! prefix-glyph contract from 04-06).
//!
//! Two paths exist depending on whether the test host has a working
//! Chromium under /usr/bin/chromium (the second discovery candidate
//! hard-coded in run_cmd::discover_chromium):
//!
//!   (A) Chromium IS available -> the scenario runs to completion. With
//!       --verbose, stderr ends with the `final_summary` line:
//!           "+ scenario smoke: pass"
//!       or its fail-glyph variant. Without --verbose and on a pass
//!       verdict, stderr is silent (run_cmd only emits the failure line
//!       on non-pass).
//!
//!   (B) Chromium IS NOT available -> the driver exits 3 at the
//!       discovery step. stderr carries the StartupError diagnostic
//!       prefixed by `bootroom run: `.
//!
//! Both paths share two invariants we pin here:
//!
//!   1. stderr is ASCII-only (RUN-09 cross-platform CI mandate).
//!   2. --verbose ON a non-pass verdict OR a startup error produces a
//!      glyph-prefixed line (`+ `, `- `, `> `) or `bootroom run: `;
//!      i.e. there is no UTF-8 sigil drift from the formatter.

use std::process::Command;
use tempfile::TempDir;

fn build_fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let kernel = tmp.path().join("Image");
    std::fs::write(&kernel, b"x").expect("kernel");
    let cfg = tmp.path().join("bootroom.toml");
    std::fs::write(
        &cfg,
        b"\
schema_version = 1

[[action]]
label = \"noop\"
bytes = ''

[[scenario]]
name = \"smoke\"
actions = [\"noop\"]
timeout_ms = 1000
",
    )
    .expect("toml");
    (tmp, kernel, cfg)
}

/// Does a stderr buffer contain only ASCII bytes (< 0x80)? RUN-09's
/// cross-platform CI mandate forbids UTF-8 sigils in the verbose
/// formatter.
fn is_ascii(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b < 0x80)
}

#[test]
fn run_with_verbose_emits_ascii_stderr() {
    let (_tmp, kernel, cfg) = build_fixture();
    let exe = env!("CARGO_BIN_EXE_bootroom");

    // Force a chromium-missing scenario via $CHROMIUM=/nonexistent and an
    // empty PATH. On hosts where /usr/bin/chromium is installed and works
    // (which AGENTS.md flags as the default for this Arch dev box) the
    // hard-coded second candidate still wins and the scenario runs to
    // completion; that path also exercises the verbose final_summary.
    let empty_path = TempDir::new().expect("empty PATH dir");
    let out = Command::new(exe)
        .env("CHROMIUM", "/nonexistent")
        .env("PATH", empty_path.path())
        .args([
            "run",
            "--kernel",
            kernel.to_str().unwrap(),
            "--config",
            cfg.to_str().unwrap(),
            "--scenario",
            "smoke",
            "--verbose",
        ])
        .output()
        .expect("spawn");

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Invariant 1: ASCII-only.
    assert!(
        is_ascii(&out.stderr),
        "stderr contains non-ASCII bytes (RUN-09 violation): {:?}",
        &out.stderr
    );

    // Invariant 2: stderr carries one of the recognized prefixes.
    //   - `bootroom run: ` (StartupError / ConfigError diagnostic, exit 2/3)
    //   - `+ scenario `   (final_summary pass)
    //   - `- scenario `   (final_summary non-pass)
    //   - `> action: `    (progress line; verbose-only)
    let has_recognized_prefix = stderr
        .lines()
        .any(|l| {
            l.starts_with("bootroom run: ")
                || l.starts_with("+ scenario ")
                || l.starts_with("- scenario ")
                || l.starts_with("> action: ")
        });
    assert!(
        has_recognized_prefix,
        "--verbose stderr must contain at least one recognized prefix \
         (`bootroom run: `, `+ scenario `, `- scenario `, `> action: `); got:\n{stderr}\nexit: {:?}",
        out.status.code()
    );
}

#[test]
fn run_without_verbose_keeps_stderr_ascii_and_quiet_on_pass() {
    let (_tmp, kernel, cfg) = build_fixture();
    let exe = env!("CARGO_BIN_EXE_bootroom");

    let empty_path = TempDir::new().expect("empty PATH dir");
    let out = Command::new(exe)
        .env("CHROMIUM", "/nonexistent")
        .env("PATH", empty_path.path())
        .args([
            "run",
            "--kernel",
            kernel.to_str().unwrap(),
            "--config",
            cfg.to_str().unwrap(),
            "--scenario",
            "smoke",
        ])
        .output()
        .expect("spawn");

    let stderr = String::from_utf8_lossy(&out.stderr);

    // ASCII-only on every path.
    assert!(
        is_ascii(&out.stderr),
        "stderr contains non-ASCII bytes (RUN-09 violation): {:?}",
        &out.stderr
    );

    // Two acceptable shapes:
    //   - Pass verdict + no --verbose -> stderr is empty (silent).
    //   - Startup/config error -> stderr contains `bootroom run: ...`.
    //   - Failed verdict + no --verbose -> stderr has a one-line summary
    //     matching `bootroom run: scenario smoke FAILED ...`.
    let exit = out.status.code();
    if exit == Some(0) {
        // Pass path: stderr should be silent (no failure line emitted).
        // We do NOT assert empty because tracing may emit INFO/WARN lines
        // depending on RUST_LOG — instead we assert no failure glyph
        // leaked into stderr.
        assert!(
            !stderr.contains("FAILED"),
            "pass-verdict run must not emit a FAILED line; got:\n{stderr}"
        );
    } else {
        // Non-zero exit: stderr must carry a diagnostic.
        let has_diag = stderr.contains("bootroom run: ");
        assert!(
            has_diag,
            "non-pass run must emit a `bootroom run: ` diagnostic on stderr; exit={exit:?}, stderr:\n{stderr}"
        );
    }
}
