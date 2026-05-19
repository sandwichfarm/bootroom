//! Plan 03-07: CFG-01 end-to-end — config path resolution semantics.
//!
//! These are SUBPROCESS tests because the behavior under test
//! (default-CWD lookup of `bootroom.toml`, `--config` override of the
//! default, and missing-file fatality at startup) lives in the
//! `bootroom serve` binary entrypoint, not the library. The in-process
//! `build_router` test harness cannot exercise the CWD-default branch.
//!
//! Pattern mirrors `tests/serve_no_open.rs`: spawn the binary, give it
//! a few hundred ms to bind, then drop a `ChildGuard` to terminate it.

mod common;

use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

/// RAII child guard — mirrors `serve_no_open::ChildGuard`. Kept inline
/// here so this test file is self-contained (no shared trait module).
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
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// CFG-01: with no `--config`, `bootroom serve` looks for `bootroom.toml`
/// in the current working directory. A tempdir with a valid `bootroom.toml`
/// must launch the server successfully.
#[test]
fn default_path_is_cwd_bootroom_toml() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg_path = tmp.path().join("bootroom.toml");
    std::fs::write(&cfg_path, "schema_version = 1\n").expect("write cfg");

    let kernel = common::write_kernel_tempfile(b"k");

    let child = Command::new(bin())
        .args([
            "serve",
            "--kernel",
            kernel.path().to_str().unwrap(),
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--no-open",
        ])
        .current_dir(tmp.path()) // <- CWD = tempdir with bootroom.toml
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bootroom serve");

    let mut guard = ChildGuard::new(child);

    // Allow startup. If the server fails at startup (e.g. config not
    // found) the child exits early; try_wait surfaces that.
    thread::sleep(Duration::from_millis(500));
    let exited_early = guard
        .child
        .as_mut()
        .expect("child")
        .try_wait()
        .expect("try_wait");
    assert!(
        exited_early.is_none(),
        "bootroom serve must still be running 500ms in (CWD bootroom.toml default \
         path resolved); got exit status {exited_early:?}"
    );
    // Drop guard kills the child.
}

/// CFG-01: `--config <path>` overrides the CWD-default lookup. The
/// tempdir has NO `bootroom.toml` in it, but we point `--config` at a
/// config file elsewhere — the server must start regardless.
#[test]
fn config_flag_overrides_cwd_default() {
    let tmp = tempfile::tempdir().expect("tempdir"); // empty, no bootroom.toml inside.
    let mut external_cfg = tempfile::NamedTempFile::new().expect("ext cfg tempfile");
    external_cfg
        .write_all(b"schema_version = 1\n")
        .expect("write ext cfg");

    let kernel = common::write_kernel_tempfile(b"k");

    let child = Command::new(bin())
        .args([
            "serve",
            "--kernel",
            kernel.path().to_str().unwrap(),
            "--config",
            external_cfg.path().to_str().unwrap(),
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--no-open",
        ])
        .current_dir(tmp.path()) // No bootroom.toml here.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bootroom serve --config");

    let mut guard = ChildGuard::new(child);

    thread::sleep(Duration::from_millis(500));
    let exited_early = guard
        .child
        .as_mut()
        .expect("child")
        .try_wait()
        .expect("try_wait");
    assert!(
        exited_early.is_none(),
        "--config must win over CWD default even when CWD has no bootroom.toml; \
         got early exit status {exited_early:?}"
    );
}

/// CFG-01 negative: a missing `--config` file is FATAL at startup
/// (CONTEXT decision "Config live-reload": initial-load failure is fatal).
/// The exit code must be non-zero and stderr must mention the offending path.
#[test]
fn missing_config_file_fails_startup() {
    let kernel = common::write_kernel_tempfile(b"k");
    let bogus = "/this/path/does/not/exist/bootroom-test-missing.toml";

    let output = Command::new(bin())
        .args([
            "serve",
            "--kernel",
            kernel.path().to_str().unwrap(),
            "--config",
            bogus,
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--no-open",
        ])
        .output()
        .expect("run bootroom serve --config /nonexistent");

    assert!(
        !output.status.success(),
        "missing --config file must fail at startup; got status {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(bogus) || stderr.contains("not found") || stderr.contains("--config"),
        "stderr must reference the bad config path; got: {stderr}"
    );
}
