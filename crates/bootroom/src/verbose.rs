//! `--verbose` stderr formatter (RUN-09).
//!
//! All output is ASCII-only per 04-RESEARCH Open Question 4
//! (cross-platform CI; Windows console may not render UTF-8 cleanly).
//! Glyphs: `> ` for action, `+ ` for pass, `- ` for fail.
//!
//! Format pinned by 04-06 unit tests; downstream CI tooling parses
//! these lines with simple prefix matching.

use std::io::{self, Write};

pub const GLYPH_ACTION: &str = "> ";
pub const GLYPH_PASS: &str = "+ ";
pub const GLYPH_FAIL: &str = "- ";
/// Informational glyph for `bootroom doctor` (Plan 05-04). Used when a
/// check produces a status that is neither pass nor fail — e.g. the
/// browser was not discovered (a missing browser is information, not a
/// CI failure).
pub const GLYPH_INFO: &str = "~ ";

pub struct VerboseFormatter<W: Write> {
    w: W,
}

impl<W: Write> VerboseFormatter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }

    /// Write one progress line for an action that is about to run.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error from the underlying writer.
    pub fn progress_action(&mut self, action: &str) -> io::Result<()> {
        writeln!(self.w, "{GLYPH_ACTION}action: {action}")
    }

    /// Write one assertion verdict line. The pattern is included
    /// via Rust's `Debug` formatter (`{:?}`), which wraps in quotes
    /// and escapes backslashes -- matching what JSON/JS observers
    /// expect when grepping CI logs.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error from the underlying writer.
    pub fn assertion_verdict(
        &mut self,
        kind: &str,
        pattern: &str,
        passed: bool,
    ) -> io::Result<()> {
        let glyph = if passed { GLYPH_PASS } else { GLYPH_FAIL };
        writeln!(self.w, "{glyph}assert: {kind} {pattern:?}")
    }

    /// Write the final scenario verdict line. Any non-`"pass"` verdict
    /// (including `"fail"`, `"timeout"`, `"error"`) uses the fail glyph.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error from the underlying writer.
    pub fn final_summary(&mut self, verdict: &str, scenario: &str) -> io::Result<()> {
        let glyph = if verdict == "pass" {
            GLYPH_PASS
        } else {
            GLYPH_FAIL
        };
        writeln!(self.w, "{glyph}scenario {scenario}: {verdict}")
    }
}

/// Non-verbose one-line failure summary written to stderr when
/// `--verbose` is NOT set and the verdict is not `"pass"`.
///
/// 04-CONTEXT writes the failure line with an em-dash; Open Q4's
/// ASCII-only mandate trumps, so we use a plain hyphen with surrounding
/// spaces for cross-platform CI portability.
///
/// # Errors
///
/// Propagates any I/O error from the underlying writer.
pub fn non_verbose_failure_line<W: Write>(
    w: &mut W,
    scenario: &str,
    reason: &str,
) -> io::Result<()> {
    writeln!(w, "bootroom run: scenario {scenario} FAILED - {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture<F>(f: F) -> String
    where
        F: FnOnce(&mut Vec<u8>),
    {
        let mut buf: Vec<u8> = Vec::new();
        f(&mut buf);
        String::from_utf8(buf).expect("utf8")
    }

    #[test]
    fn progress_action_writes_exact_line() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.progress_action("reboot").unwrap();
        });
        assert_eq!(s, "> action: reboot\n");
    }

    #[test]
    fn assertion_verdict_pass_contains() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.assertion_verdict("contains", "login: ", true).unwrap();
        });
        assert_eq!(s, "+ assert: contains \"login: \"\n");
    }

    #[test]
    fn assertion_verdict_fail_regex() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.assertion_verdict("regex", "Booting\\s+", false).unwrap();
        });
        // The actual bytes on the wire: hyphen, space, "assert: regex ",
        // a quote, B,o,o,t,i,n,g, two literal backslashes, s,+, a quote,
        // newline.
        assert_eq!(s, "- assert: regex \"Booting\\\\s+\"\n");
    }

    #[test]
    fn final_summary_pass() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.final_summary("pass", "boot_smoke").unwrap();
        });
        assert_eq!(s, "+ scenario boot_smoke: pass\n");
    }

    #[test]
    fn final_summary_fail() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.final_summary("fail", "boot_smoke").unwrap();
        });
        assert_eq!(s, "- scenario boot_smoke: fail\n");
    }

    #[test]
    fn final_summary_timeout_uses_fail_glyph() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.final_summary("timeout", "boot_smoke").unwrap();
        });
        assert_eq!(s, "- scenario boot_smoke: timeout\n");
    }

    #[test]
    fn final_summary_error_uses_fail_glyph() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(buf);
            vf.final_summary("error", "boot_smoke").unwrap();
        });
        assert_eq!(s, "- scenario boot_smoke: error\n");
    }

    #[test]
    fn non_verbose_failure_line_format() {
        let s = capture(|buf| {
            non_verbose_failure_line(
                buf,
                "boot_smoke",
                "assertion 'login: ' not found after action reboot",
            )
            .unwrap();
        });
        assert_eq!(
            s,
            "bootroom run: scenario boot_smoke FAILED - assertion 'login: ' not found after action reboot\n"
        );
    }

    #[test]
    fn all_output_is_ascii() {
        let s = capture(|buf| {
            let mut vf = VerboseFormatter::new(&mut *buf);
            vf.progress_action("reboot").unwrap();
            vf.assertion_verdict("contains", "login: ", true).unwrap();
            vf.assertion_verdict("regex", "x+", false).unwrap();
            vf.final_summary("pass", "boot_smoke").unwrap();
            vf.final_summary("timeout", "boot_smoke").unwrap();
            non_verbose_failure_line(buf, "boot_smoke", "reason").unwrap();
        });
        for (i, b) in s.as_bytes().iter().enumerate() {
            assert!(
                *b < 0x80,
                "byte {i} = 0x{b:02x} is non-ASCII; Open Q4 violated"
            );
        }
    }
}
