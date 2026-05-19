//! 04-11 -- Phase-4 e2e gate.
//!
//! `#[ignore]`-tagged: requires real Chromium + the Spike B NORN kernel
//! fixture. Run via: `cargo test --workspace -- --ignored`.
//!
//! Exit 0 from `bootroom run --kernel <NORN> --scenario boot_smoke`
//! validates RUN-01..RUN-08, RUN-10 as a single integrated path:
//!
//! - RUN-01 `bootroom run` subcommand surface
//! - RUN-02 `--kernel` + `--config` flags
//! - RUN-03 `--scenario <name>` selects from config
//! - RUN-04 in-process server bind + handoff to headless Chromium
//! - RUN-05 chromiumoxide driver composition
//! - RUN-06 `scenario_result_tx` handoff
//! - RUN-07 process exit code matches scenario verdict
//! - RUN-08 `--log-file` JSONL transcript
//! - RUN-10 no orphan Chromium / axum processes after exit
//!
//! The test self-skips (returns OK) when either chromium or the NORN
//! fixture is absent so an operator who manually passes `--ignored`
//! on a misconfigured host sees a clear `[skip]` diagnostic rather
//! than a panic.

use bootroom::transcript::TranscriptEvent;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

fn norn_fixture_path() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("spikes/spike-b/fixtures/Image")
}

fn chromium_works() -> bool {
    Command::new("/usr/bin/chromium")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "requires /usr/bin/chromium + the NORN kernel fixture; run with --ignored on a configured host"]
fn run_norn_boot_smoke_exits_zero() {
    let kernel = norn_fixture_path();
    if !kernel.is_file() {
        eprintln!(
            "[skip] NORN kernel fixture not present at {} -- \
             populate from a NORN build or copy the Spike-B fixture",
            kernel.display()
        );
        return;
    }
    if !chromium_works() {
        eprintln!("[skip] /usr/bin/chromium not present or not working");
        return;
    }

    let tmp = TempDir::new().expect("tempdir");
    let log_path = tmp.path().join("transcript.jsonl");

    // The fixture TOML is committed under crates/bootroom/tests/fixtures.
    let cfg =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/boot_smoke.toml");
    assert!(cfg.is_file(), "fixture {} not present", cfg.display());

    let exe = env!("CARGO_BIN_EXE_bootroom");

    // 90-second test-level timeout: spawn `Command::output()` (which
    // blocks until exit) inside a worker thread; main thread blocks on
    // `recv_timeout`. If the binary hangs we fail the test rather than
    // letting cargo's outer test runner discover it.
    //
    // 90 s = scenario.timeout_ms (30 s) + Pitfall-#8 buffer (30 s) +
    // thread spawn / pgrep / log-read slack (30 s).
    let (tx, rx) = mpsc::channel();
    let exe_owned = exe.to_string();
    let k = kernel.clone();
    let c = cfg.clone();
    let lp = log_path.clone();

    std::thread::spawn(move || {
        let out = Command::new(&exe_owned)
            .args([
                "run",
                "--kernel",
                k.to_str().unwrap(),
                "--config",
                c.to_str().unwrap(),
                "--scenario",
                "boot_smoke",
                "--log-file",
                lp.to_str().unwrap(),
                "--verbose",
            ])
            .output();
        let _ = tx.send(out);
    });

    let out = rx
        .recv_timeout(Duration::from_secs(90))
        .expect("bootroom run did not return within 90s; possible hang in run_cmd")
        .expect("spawn bootroom run");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; got {:?}\n\
         stdout:\n{stdout}\n\
         stderr:\n{stderr}",
        out.status.code()
    );

    let log = std::fs::read_to_string(&log_path).expect("log file must exist");
    let mut found_start = false;
    let mut found_pass_result = false;
    for (i, line) in log.lines().enumerate() {
        let ev: TranscriptEvent = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("line {i} not valid JSON: {e}\nline: {line}"));
        match ev {
            TranscriptEvent::ScenarioStart { .. } => {
                found_start = true;
            }
            TranscriptEvent::ScenarioResult { verdict, .. } if verdict == "pass" => {
                found_pass_result = true;
            }
            _ => {}
        }
    }
    assert!(
        found_start,
        "missing scenario_start event in transcript:\n{log}"
    );
    assert!(
        found_pass_result,
        "missing pass-verdict scenario_result in transcript:\n{log}"
    );

    // RUN-10: no orphan chromium/headless processes after `bootroom run`
    // returns. Cross-checks the BrowserGuard Drop impl from 04-07.
    let orphans = Command::new("pgrep")
        .args(["-f", "chromium.*--headless"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    assert!(
        orphans.is_empty(),
        "orphan chromium processes after bootroom run exit: {orphans}"
    );
}
