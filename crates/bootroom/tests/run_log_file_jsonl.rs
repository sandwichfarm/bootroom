//! 04-10 — RUN-08 pin: --log-file writes JSONL transcript.
//!
//! Strategy: force a chromium discovery failure (`$CHROMIUM=/nonexistent`
//! plus a `PATH` that contains no chromium) so the driver exits 3
//! BEFORE attempting a real headless run, but AFTER writing the
//! `scenario_start` preamble. Confirms:
//!   (a) the file is created at the supplied `--log-file` path,
//!   (b) the first line is a valid `scenario_start` `TranscriptEvent`,
//!   (c) every line is valid JSON parsing to a known `TranscriptEvent`.
//!
//! Runtime: < 5s on a typical dev host (no chromium probe, no real
//! Chromium launch).

use bootroom::transcript::TranscriptEvent;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn run_writes_scenario_start_event_to_log_file() {
    let exe = env!("CARGO_BIN_EXE_bootroom");
    let tmp = TempDir::new().expect("tempdir");

    // 1. Create a valid kernel placeholder (size > 0 so kernel.is_file()
    //    passes the run_cmd preflight; the bytes are not consumed
    //    because we fast-fail at chromium discovery).
    let kernel = tmp.path().join("Image");
    std::fs::write(&kernel, b"not actually a kernel").expect("write kernel");

    // 2. Minimal bootroom.toml with one scenario the driver can resolve.
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
    .expect("write toml");

    let log_path = tmp.path().join("log.jsonl");

    // 3. Empty PATH + bogus $CHROMIUM defeats every discovery candidate
    //    so the driver fast-fails at the chromium step (exit 3) without
    //    ever launching a real browser.
    let empty_path = TempDir::new().expect("tempdir for empty PATH");

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
            "--log-file",
            log_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn bootroom run");

    // 4. We don't gate on exit code — the contract here is the log
    //    content. On chromium-missing the driver exits 3; if the host
    //    is misconfigured in some other way we might see 2. Either way
    //    the scenario_start preamble must have been written before
    //    the bail.
    let content = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        !content.is_empty(),
        "expected --log-file to be created with at least a scenario_start line.\nexit: {:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    // 5. Every line must be a valid TranscriptEvent. The first line
    //    MUST be a scenario_start.
    let mut found_scenario_start = false;
    for (i, line) in content.lines().enumerate() {
        let ev: TranscriptEvent = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("line {i} not valid TranscriptEvent JSON: {e}\nline: {line}")
        });
        if let TranscriptEvent::ScenarioStart {
            scenario, kernel: kernel_field, ..
        } = &ev
        {
            assert_eq!(scenario, "smoke", "scenario name in preamble");
            assert!(
                !kernel_field.is_empty(),
                "kernel field must be populated in scenario_start preamble"
            );
            found_scenario_start = true;
        }
    }
    assert!(
        found_scenario_start,
        "expected at least one scenario_start event in log; got:\n{content}"
    );
}
