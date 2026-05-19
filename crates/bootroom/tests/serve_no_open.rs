//! SERV-06 subprocess integration test for the `--no-open` flag.
//!
//! Unlike most bootroom integration tests (which use the in-process
//! `common::spawn` harness against `build_router`), this test must drive the
//! actual `bootroom` binary to exercise clap's parsing of `--no-open` and the
//! `server::run` code path that decides whether to call `open::that_detached`.
//!
//! What is verified:
//!   1. `bootroom serve --kernel <tmp> --port 0 --no-open` boots, prints the
//!      canonical startup line (`Serving bootroom on http://...`), and keeps
//!      running. With `--no-open` set, the server MUST NOT print the
//!      UI-SPEC fallback line about being unable to open a browser.
//!   2. `bootroom serve --help` lists `--no-open` in its options.
//!
//! What is intentionally NOT verified here: that auto-open actually launches
//! a GUI browser when `--no-open` is absent. That is covered by manual smoke
//! steps in 02-VALIDATION.md — a subprocess that launches a real GUI app is
//! unstable in CI and would require a fake browser shim.
//!
//! WR-06 lesson: own the child process via a drop guard so test failures
//! (including panics) cannot leak a running `bootroom` subprocess that holds
//! a listener port. See `ChildGuard` below.

mod common;

use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

/// RAII guard around a spawned `Child` — kills + waits on drop so panicking
/// tests cannot leak a running `bootroom` subprocess (mirrors WR-06 in
/// 01-REVIEW.md: every spawned resource must be aborted on drop, not just
/// in the happy path).
struct ChildGuard {
    child: Option<std::process::Child>,
}

impl ChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort cleanup; ignore errors (the child may already be dead).
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn serve_no_open_returns_listener_without_launching_browser() {
    // `env!("CARGO_BIN_EXE_bootroom")` is set by cargo for integration tests
    // and resolves to the freshly-built `bootroom` binary path.
    let bin = env!("CARGO_BIN_EXE_bootroom");
    let kernel = common::write_kernel_tempfile(b"fake-kernel");
    let kernel_path = kernel
        .path()
        .to_str()
        .expect("kernel tempfile path utf-8")
        .to_owned();

    // Phase 3: `bootroom serve` now requires a readable `bootroom.toml`
    // at startup (or `--config <PATH>`). Supply a trivial valid config
    // so this test exercises only the --no-open codepath.
    let mut cfg = tempfile::NamedTempFile::new().expect("config tempfile");
    {
        use std::io::Write;
        cfg.write_all(b"schema_version = 1\n")
            .expect("write config");
    }
    let cfg_path = cfg
        .path()
        .to_str()
        .expect("config tempfile path utf-8")
        .to_owned();

    let mut child = Command::new(bin)
        .args([
            "serve",
            "--kernel",
            &kernel_path,
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--no-open",
            "--config",
            &cfg_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bootroom serve --no-open");

    // Move stdout out of the child handle BEFORE wrapping it in ChildGuard;
    // ChildGuard's `kill()` doesn't need stdout, and we need to read it on a
    // background thread without holding the guard's borrow.
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let guard = ChildGuard::new(child);

    // Reader thread: forward every stdout line on a channel so the test
    // thread can wait with a timeout. A blocking read on the main thread
    // would have no way to time out.
    let (tx, rx) = mpsc::channel::<String>();
    let tx_clone = tx.clone();
    let stdout_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            // Best-effort: if the test already returned, the receiver is gone.
            if tx_clone.send(line).is_err() {
                break;
            }
        }
    });

    // Stderr reader: collect lines so we can assert the UI-SPEC failure
    // message is NOT present when --no-open is set.
    let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if stderr_tx.send(line).is_err() {
                break;
            }
        }
    });

    // Wait up to 5s for the startup line.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut startup_line: Option<String> = None;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(line) => {
                if line.starts_with("Serving bootroom on http://127.0.0.1:")
                    && line.ends_with(" (Ctrl-C to stop)")
                {
                    startup_line = Some(line);
                    break;
                }
                // Non-matching line — keep draining; the startup banner may
                // be preceded by tracing-subscriber init output.
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Loop condition re-checks the deadline; nothing to do here.
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let line = startup_line.expect(
        "bootroom serve --no-open did not print the canonical startup line within 5s",
    );

    // Sanity-check the bound URL has a non-zero port (OS picked one because
    // we passed --port 0).
    let port_str = line
        .strip_prefix("Serving bootroom on http://127.0.0.1:")
        .and_then(|s| s.strip_suffix(" (Ctrl-C to stop)"))
        .expect("startup line shape");
    let port: u16 = port_str
        .parse()
        .unwrap_or_else(|_| panic!("ephemeral port should parse as u16, got {port_str:?}"));
    assert_ne!(port, 0, "OS should assign a non-zero ephemeral port");

    // Drop the guard to terminate the child. Drain stdout/stderr reader
    // threads so we can inspect everything the child emitted before exit.
    drop(guard);
    drop(tx);
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    // With --no-open, the server must not have attempted to open a browser,
    // so the UI-SPEC fallback line must be absent on stderr.
    let stderr_lines: Vec<String> = stderr_rx.try_iter().collect();
    let saw_fallback = stderr_lines
        .iter()
        .any(|l| l.contains("Could not open browser automatically"));
    assert!(
        !saw_fallback,
        "--no-open must skip the open call, but stderr contained the auto-open \
         failure line. stderr lines = {stderr_lines:?}"
    );
}

#[test]
fn serve_help_lists_no_open_flag() {
    let bin = env!("CARGO_BIN_EXE_bootroom");
    let output = Command::new(bin)
        .args(["serve", "--help"])
        .output()
        .expect("run bootroom serve --help");

    assert!(
        output.status.success(),
        "bootroom serve --help should exit 0, got {:?}",
        output.status
    );
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--no-open"),
        "`bootroom serve --help` output must mention --no-open. Got:\n{help}"
    );
}
