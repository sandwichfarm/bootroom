//! Human-format contract pins for `bootroom doctor`.
//!
//! Runs the doctor as a subprocess (the actual operator surface) and
//! pins the five `##` section headers, ASCII-only glyphs, the trailing
//! `Overall: pass|fail` line, and the banner line shape. These guards
//! catch unicode pollution (Research Open Q1) and accidental section
//! removal.
//!
//! Plan 05-05 — DOC-01.

use std::process::Command;

fn bootroom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootroom")
}

fn run_doctor_human() -> String {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let out = Command::new(bootroom_bin())
        .arg("doctor")
        .current_dir(tmp.path())
        .output()
        .expect("running bootroom doctor should succeed");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("doctor stdout is UTF-8")
}

/// All five `## …` section headers must appear in stdout. Order is the
/// implementation's responsibility; this test pins presence only so a
/// future re-ordering doesn't trip the wrong wire.
#[test]
fn human_format_contains_all_section_headers() {
    let stdout = run_doctor_human();
    for header in [
        "## Version",
        "## Browser",
        "## Server headers",
        "## Config",
        "## CLI surface",
    ] {
        assert!(
            stdout.contains(header),
            "human output missing section header `{header}`; full stdout:\n{stdout}"
        );
    }
}

/// Regression guard against unicode glyph drift — Research Open Q1
/// fixed the format on ASCII (`+`, `-`, `~`). If any of the unicode
/// candidates (✓, ✗, en-dash, em-dash) sneak back in, fail loudly.
#[test]
fn human_format_uses_ascii_glyphs_not_unicode() {
    let stdout = run_doctor_human();
    for non_ascii in ['\u{2713}', '\u{2717}', '\u{2013}', '\u{2014}'] {
        assert!(
            !stdout.contains(non_ascii),
            "unicode glyph U+{:04X} leaked into human output; full stdout:\n{stdout}",
            non_ascii as u32
        );
    }
    // Positive assertion: at least one ASCII status prefix must appear.
    // On this build tree we always emit some lines so at least one of
    // `+ `, `- `, `~ ` must be present.
    let has_ascii_prefix = stdout.lines().any(|l| {
        l.starts_with("+ ") || l.starts_with("- ") || l.starts_with("~ ")
    });
    assert!(
        has_ascii_prefix,
        "expected at least one ASCII status glyph line; full stdout:\n{stdout}"
    );
}

/// The very last non-empty line of stdout must match the documented
/// `Overall: (pass|fail)` shape so CI grep recipes are stable.
#[test]
fn human_format_has_overall_line() {
    let stdout = run_doctor_human();
    let last_non_empty = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .expect("at least one non-empty line in doctor stdout");
    assert!(
        last_non_empty == "Overall: pass" || last_non_empty == "Overall: fail",
        "final non-empty line must be `Overall: pass` or `Overall: fail`; got: {last_non_empty:?}\nfull:\n{stdout}"
    );
}

/// First line of stdout is the banner. Research §"Human Output Format"
/// originally specified an em-dash; the implementer chose ASCII `-` to
/// keep the rule "no unicode anywhere". Accept either form by checking
/// the stable substring `bootroom doctor`.
#[test]
fn human_format_banner_line_present() {
    let stdout = run_doctor_human();
    let first_line = stdout.lines().next().expect("doctor stdout is non-empty");
    assert!(
        first_line.contains("bootroom doctor"),
        "first line must contain `bootroom doctor` banner; got: {first_line:?}\nfull:\n{stdout}"
    );
    assert!(
        first_line.contains("preflight"),
        "banner must mention `preflight`; got: {first_line:?}"
    );
}
