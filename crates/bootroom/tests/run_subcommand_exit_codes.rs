//! 04-10 — RUN-01 pin: the 0/1/2/3 exit-code table for `bootroom run`.
//!
//! The 0/1 split (pass vs. fail verdict) is covered by 04-11 because it
//! requires a real Chromium + a real kernel that actually boots. This
//! file pins the 2/3 paths:
//!
//!   * Exit 2 — config / CLI error caught BEFORE Chromium launch:
//!       - missing kernel
//!       - unknown scenario
//!       - missing config file
//!   * Exit 3 — startup error: no working Chromium binary.
//!     This test self-skips on hosts where `/usr/bin/chromium --version`
//!     succeeds, because `discover_chromium`'s second candidate is hard
//!     coded to `/usr/bin/chromium` and is not influenced by `$PATH` or
//!     `$CHROMIUM`. On those hosts the exit-3 path is structurally
//!     unreachable; 04-11 covers the green-path end-to-end.

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

#[test]
fn exit_2_when_kernel_missing() {
    let exe = env!("CARGO_BIN_EXE_bootroom");
    let out = Command::new(exe)
        .args([
            "run",
            "--kernel",
            "/nonexistent/Image",
            "--scenario",
            "anything",
        ])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 (config error on missing kernel); stderr was:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exit_2_when_scenario_unknown() {
    let (_tmp, k, c) = build_fixture();
    let exe = env!("CARGO_BIN_EXE_bootroom");
    let out = Command::new(exe)
        .args([
            "run",
            "--kernel",
            k.to_str().unwrap(),
            "--config",
            c.to_str().unwrap(),
            "--scenario",
            "does_not_exist",
        ])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr was:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown scenario") || stderr.contains("does_not_exist"),
        "diagnostic should mention the unknown scenario name; got:\n{stderr}"
    );
}

#[test]
fn exit_2_when_config_missing() {
    let (_tmp, k, _c) = build_fixture();
    let exe = env!("CARGO_BIN_EXE_bootroom");
    let out = Command::new(exe)
        .args([
            "run",
            "--kernel",
            k.to_str().unwrap(),
            "--config",
            "/nonexistent/bootroom.toml",
            "--scenario",
            "smoke",
        ])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr was:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exit_3_when_chromium_missing() {
    let (_tmp, k, c) = build_fixture();
    let exe = env!("CARGO_BIN_EXE_bootroom");

    // run_cmd::discover_chromium's second candidate is the hard-coded
    // path /usr/bin/chromium — neither $PATH nor $CHROMIUM can mask
    // it. If that binary works on this host, the exit-3 path is
    // structurally unreachable and we self-skip. 04-11 covers the
    // green path end-to-end.
    let chromium_works = std::process::Command::new("/usr/bin/chromium")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if chromium_works {
        eprintln!(
            "[skip] /usr/bin/chromium works on this host; exit-3 path is unreachable here"
        );
        return;
    }

    let empty_path = TempDir::new().expect("tempdir for empty PATH");
    let out = Command::new(exe)
        .env("CHROMIUM", "/nonexistent")
        .env("PATH", empty_path.path())
        .args([
            "run",
            "--kernel",
            k.to_str().unwrap(),
            "--config",
            c.to_str().unwrap(),
            "--scenario",
            "smoke",
        ])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(3),
        "expected exit 3 (startup error); stderr was:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Chromium") || stderr.contains("chromium"),
        "diagnostic should mention chromium; got:\n{stderr}"
    );
}
