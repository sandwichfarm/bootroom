//! Release-smoke install verification.
//!
//! This test is `#[ignore]` so it does not run as part of the normal
//! `cargo test` suite. The release-smoke GitHub Actions workflow
//! (.github/workflows/release-smoke.yml) runs it explicitly via
//! `cargo test --test install_smoke -- --ignored` AFTER `cargo install
//! --locked --path crates/bootroom` has placed the binary on PATH inside
//! a Docker container.
//!
//! Required state for this test to pass:
//! - `bootroom` binary on PATH (via cargo install during the smoke job).
//! - CWD is intentionally NOT the source tree — typically `/` or `/root`
//!   inside the Docker container. This exercises DIST-05's path-
//!   independence requirement (assets embedded via include_dir!).
//! - Optionally, `BOOTROOM_INSTALL_SMOKE_BIN` env var can override the
//!   binary path (for local debugging without rebuilding the container).

use std::process::Command;

fn bootroom_bin() -> String {
    std::env::var("BOOTROOM_INSTALL_SMOKE_BIN").unwrap_or_else(|_| "bootroom".to_string())
}

#[test]
#[ignore]
fn doctor_overall_is_pass_after_cargo_install() {
    let bin = bootroom_bin();

    // Run from CWD that is NOT the source tree. The release-smoke workflow
    // ensures this; locally, run from /tmp or pass --current-dir.
    let output = Command::new(&bin)
        .args(["doctor", "--format", "json"])
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke `{} doctor`: {}", bin, e));

    assert!(
        output.status.success(),
        "bootroom doctor exited non-zero: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout =
        String::from_utf8(output.stdout).expect("bootroom doctor stdout must be UTF-8");

    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "bootroom doctor --format json produced non-JSON: {}\nstdout:\n{}",
            e, stdout
        )
    });

    let overall = parsed
        .get("overall")
        .and_then(|v| v.as_str())
        .expect("bootroom doctor JSON missing `overall` field");

    assert_eq!(
        overall, "pass",
        "bootroom doctor overall status was {:?}, expected \"pass\". Full JSON:\n{}",
        overall, stdout
    );
}

#[test]
#[ignore]
fn doctor_runs_from_tmp_cwd() {
    // DIST-05 path-independence check: change CWD to /tmp before invoking.
    // (06-08 expands this with explicit DIST-05 coverage; this is the
    // smaller in-test variant for the smoke gate's wave.)
    let bin = bootroom_bin();

    let output = Command::new(&bin)
        .args(["doctor", "--format", "json"])
        .current_dir("/tmp")
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke `{} doctor` from /tmp: {}", bin, e));

    assert!(
        output.status.success(),
        "bootroom doctor exited non-zero when run from /tmp: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
