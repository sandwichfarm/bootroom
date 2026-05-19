//! Release-smoke install verification.
//!
//! These tests are gated by the `ignore` attribute so they do not run as part of the normal
//! `cargo test` suite. The release-smoke GitHub Actions workflow
//! (.github/workflows/release-smoke.yml) runs it explicitly via
//! `cargo test --test install_smoke -- --ignored` AFTER `cargo install
//! --locked --path crates/bootroom` has placed the binary on PATH inside
//! a Docker container.
//!
//! Required state for these tests to pass:
//! - `bootroom` binary on PATH (via cargo install during the smoke job).
//! - CWD is intentionally NOT the source tree — typically `/` or `/tmp`
//!   inside the Docker container. This exercises DIST-05's path-
//!   independence requirement (assets embedded via include_dir!).
//! - Optionally, `BOOTROOM_INSTALL_SMOKE_BIN` env var can override the
//!   binary path (for local debugging without rebuilding the container).
//!
//! Test inventory:
//! - `doctor_overall_is_pass_after_cargo_install` — coarse "binary launches
//!   and reports overall pass" check (default CWD).
//! - `doctor_runs_from_tmp_cwd` — same coarse check, but explicitly from
//!   `/tmp` to exercise path-independence at the exit-status + overall=pass
//!   level.
//! - `path_independence_qemu_wasm_rev_present` — DIST-05 strict gate: from
//!   `/tmp`, asserts the `qemu_wasm_rev` check status is `"info"` (the
//!   documented contract for `check_qemu_rev` in doctor_cmd.rs — the
//!   embedded rev is always informational, never pass/fail) AND that the
//!   check's detail string contains a real revision, not the degraded
//!   `"unknown"` sentinel. This is the runtime backstop for the
//!   `[package].include` allow-list (06-02) and `include_dir!` reachability.

use std::process::Command;

fn bootroom_bin() -> String {
    std::env::var("BOOTROOM_INSTALL_SMOKE_BIN").unwrap_or_else(|_| "bootroom".to_string())
}

/// Run `bootroom doctor --format json` with the given CWD, assert exit 0,
/// parse stdout as JSON, and return the parsed `serde_json::Value`.
fn run_doctor_json_from(cwd: &str) -> serde_json::Value {
    let bin = bootroom_bin();
    let output = Command::new(&bin)
        .args(["doctor", "--format", "json"])
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke `{} doctor` from {}: {}", bin, cwd, e));
    assert!(
        output.status.success(),
        "bootroom doctor exited non-zero (CWD={cwd}): stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout =
        String::from_utf8(output.stdout).expect("bootroom doctor stdout must be UTF-8");
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "bootroom doctor --format json produced non-JSON: {}\nstdout:\n{}",
            e, stdout
        )
    })
}

#[test]
#[ignore]
fn doctor_overall_is_pass_after_cargo_install() {
    // Default CWD; mostly proves the binary launches at all.
    let parsed = run_doctor_json_from(".");
    let overall = parsed
        .get("overall")
        .and_then(|v| v.as_str())
        .expect("overall field missing");
    assert_eq!(overall, "pass", "doctor JSON: {}", parsed);
}

#[test]
#[ignore]
fn doctor_runs_from_tmp_cwd() {
    // DIST-05 path-independence (basic): binary launches and reports pass
    // even when invoked from /tmp.
    let parsed = run_doctor_json_from("/tmp");
    let overall = parsed
        .get("overall")
        .and_then(|v| v.as_str())
        .expect("overall field missing");
    assert_eq!(
        overall, "pass",
        "doctor from /tmp returned non-pass overall: {}",
        parsed
    );
}

#[test]
#[ignore]
fn path_independence_qemu_wasm_rev_present() {
    // DIST-05 strict check: the qemu-wasm-rev embedded file must be reachable
    // when CWD is /tmp. A regression in `[package].include` (06-02) or in
    // include_dir!'s path argument would manifest as an empty/missing
    // qemu_wasm_rev value here, failing this test before the publish job
    // (release-smoke gating).
    let parsed = run_doctor_json_from("/tmp");

    // Locate the qemu_wasm_rev entry in the checks array.
    let checks = parsed
        .get("checks")
        .and_then(|v| v.as_array())
        .expect("checks field missing or not an array");

    let rev_check = checks
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("qemu_wasm_rev"))
        .expect("doctor JSON has no `qemu_wasm_rev` check entry");

    let rev_status = rev_check
        .get("status")
        .and_then(|s| s.as_str())
        .expect("qemu_wasm_rev check has no status");

    // CR-03: doctor's `check_qemu_rev` is documented to always return
    // CheckStatus::Info (the embedded rev is informational; it is never
    // a pass/fail signal on its own). The previous `"pass"` assertion
    // was structurally unreachable and would block every release.
    assert_eq!(
        rev_status, "info",
        "qemu_wasm_rev status was {:?}, expected \"info\" (the documented contract for check_qemu_rev — see crates/bootroom/src/doctor_cmd.rs::check_qemu_rev).\nFull JSON: {}",
        rev_status, parsed
    );

    // The real DIST-05 signal: the embedded qemu-wasm-rev.txt must have
    // shipped inside the published crate. A regression of the
    // `[package].include` allow-list (06-02) or `include_dir!`
    // reachability would degrade the detail string to the literal
    // sentinel "rev unknown".
    let rev_detail = rev_check
        .get("detail")
        .and_then(|d| d.as_str())
        .expect("qemu_wasm_rev check has no detail");
    assert!(
        rev_detail.starts_with("qemu-wasm rev "),
        "qemu_wasm_rev detail has unexpected shape: {:?}",
        rev_detail
    );
    assert!(
        !rev_detail.contains("rev unknown"),
        "qemu_wasm_rev detail is the documented degraded value 'rev unknown', meaning the embedded asset bundle did not ship: {}",
        rev_detail
    );

    // Also confirm the top-level qemu_wasm_rev field (if doctor exposes it
    // separately from the checks array) is non-empty.
    if let Some(rev) = parsed.get("qemu_wasm_rev").and_then(|v| v.as_str()) {
        assert!(
            !rev.is_empty(),
            "qemu_wasm_rev top-level field is empty: {}",
            parsed
        );
        assert_ne!(
            rev, "unknown",
            "qemu_wasm_rev is the documented degraded value 'unknown', meaning the embedded asset bundle did not ship: {}",
            parsed
        );
    }
}
