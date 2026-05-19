//! `--verbose` stderr formatter (RUN-09).
//!
//! Stub module — RED phase. Tests below pin the byte-exact stderr
//! line shape for action progress, assertion verdicts, the final
//! scenario summary, and the non-verbose failure line.

use std::io::{self, Write};

pub const GLYPH_ACTION: &str = "> ";
pub const GLYPH_PASS: &str = "+ ";
pub const GLYPH_FAIL: &str = "- ";

pub struct VerboseFormatter<W: Write> {
    _w: std::marker::PhantomData<W>,
}

impl<W: Write> VerboseFormatter<W> {
    pub fn new(_w: W) -> Self {
        Self {
            _w: std::marker::PhantomData,
        }
    }

    /// # Errors
    /// Always `Unsupported` in the RED phase.
    pub fn progress_action(&mut self, _action: &str) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "not implemented"))
    }

    /// # Errors
    /// Always `Unsupported` in the RED phase.
    pub fn assertion_verdict(
        &mut self,
        _kind: &str,
        _pattern: &str,
        _passed: bool,
    ) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "not implemented"))
    }

    /// # Errors
    /// Always `Unsupported` in the RED phase.
    pub fn final_summary(&mut self, _verdict: &str, _scenario: &str) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "not implemented"))
    }
}

/// # Errors
/// Always `Unsupported` in the RED phase.
pub fn non_verbose_failure_line<W: Write>(
    _w: &mut W,
    _scenario: &str,
    _reason: &str,
) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "not implemented"))
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
