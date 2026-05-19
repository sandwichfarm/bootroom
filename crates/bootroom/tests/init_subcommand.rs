//! Integration tests for `bootroom init` — CFG-08.
//!
//! Spawns the compiled binary as a subprocess with `current_dir` set to
//! a per-test `tempdir()` so the test runner's CWD stays clean and the
//! tests cannot accidentally clobber a developer's real `bootroom.toml`.

use std::fs;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

#[test]
fn init_writes_example_to_empty_cwd() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bin())
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .expect("run bootroom init");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("Wrote ./bootroom.toml"),
        "expected stdout to contain `Wrote ./bootroom.toml`; got: {stdout}"
    );
    let target = tmp.path().join("bootroom.toml");
    assert!(target.exists(), "bootroom.toml should exist at {target:?}");
    let meta = fs::metadata(&target).expect("metadata");
    assert!(
        meta.len() > 200,
        "expected example > 200 bytes; got {} bytes",
        meta.len()
    );
}

#[test]
fn init_refuses_overwrite_without_force() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let target = tmp.path().join("bootroom.toml");
    fs::write(&target, "PREEXISTING\n").expect("seed file");
    let pre = fs::read_to_string(&target).expect("read pre");

    let out = Command::new(bin())
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .expect("run bootroom init");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1; stderr={stderr}"
    );
    assert!(
        stderr.contains("already exists; pass --force"),
        "expected stderr to contain `already exists; pass --force`; got: {stderr}"
    );
    let post = fs::read_to_string(&target).expect("read post");
    assert_eq!(pre, post, "file content must be unchanged");
    assert!(post.contains("PREEXISTING"));
}

#[test]
fn init_force_overwrites() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let target = tmp.path().join("bootroom.toml");
    fs::write(&target, "PREEXISTING\n").expect("seed file");

    let out = Command::new(bin())
        .arg("init")
        .arg("--force")
        .current_dir(tmp.path())
        .output()
        .expect("run bootroom init --force");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("Wrote ./bootroom.toml"),
        "expected stdout to contain `Wrote ./bootroom.toml`; got: {stdout}"
    );
    let post = fs::read_to_string(&target).expect("read post");
    assert_eq!(
        post,
        bootroom::init_cmd::EXAMPLE,
        "post-content must equal EXAMPLE"
    );
    assert!(!post.contains("PREEXISTING"));
}

#[test]
fn init_output_parses_with_check() {
    // End-to-end CFG-07 + CFG-08 hand-off: `init` writes the example,
    // `check` then parses it cleanly. De-risks any escape-sequence
    // rendering mistake in the inline EXAMPLE const.
    let tmp = tempfile::tempdir().expect("create tempdir");
    let init_out = Command::new(bin())
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .expect("run bootroom init");
    assert_eq!(init_out.status.code(), Some(0), "init must succeed first");

    let target = tmp.path().join("bootroom.toml");
    let check_out = Command::new(bin())
        .arg("check")
        .arg("--config")
        .arg(&target)
        .output()
        .expect("run bootroom check");
    let stdout = String::from_utf8_lossy(&check_out.stdout);
    let stderr = String::from_utf8_lossy(&check_out.stderr);
    assert_eq!(
        check_out.status.code(),
        Some(0),
        "expected check to exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("ok (2 actions, 1 scenarios)"),
        "expected `ok (2 actions, 1 scenarios)`; got: {stdout}"
    );
}

#[test]
fn inline_example_matches_check_test_expectation() {
    // Sanity-check the inline EXAMPLE constant: it must contain
    // `schema_version = 1` and fit within the documented ~25-line
    // budget (allow a small amount of flex for header comments).
    let ex = bootroom::init_cmd::EXAMPLE;
    assert!(
        ex.contains("schema_version = 1"),
        "EXAMPLE must declare schema_version = 1"
    );
    let lines = ex.lines().count();
    assert!(
        lines <= 32,
        "EXAMPLE should be ~25 lines; got {lines}"
    );
}
