//! `--log-file` JSONL transcript shape (RUN-08).
//!
//! Each event is one line: a tagged-JSON object with a stable `"type"`
//! discriminator. The six event types — `scenario_start`,
//! `action_send`, `serial_chunk`, `assertion_result`,
//! `scenario_result`, `transcript_overflow` — are the canonical
//! surface that `bootroom run` writes (Rust side) and that
//! `web/scenario.js` builds for the `WsMessage::ScenarioResult.transcript`
//! field (browser side).
//!
//! Timestamps are ISO 8601 UTC with `Z` suffix per 04-RESEARCH Open
//! Question 3 — machine-parseable, no DST ambiguity.
//!
//! `transcript_overflow` is browser-side only; emitted by 04-08 when
//! the cumulative `serial_chunk` `bytes_b64` payload exceeds 5 MB. The
//! Rust side never emits this variant but MUST deserialize it
//! (`run_cmd::persist_transcript` round-trips the browser-built
//! transcript verbatim).
//!
//! Format stability: this shape is the contract for a future v2
//! `--report-format=junit` shim. Adding fields is safe (additive
//! deserialization); renaming or removing is a breaking change.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// One line in a `--log-file` JSONL transcript.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum TranscriptEvent {
    #[serde(rename = "scenario_start")]
    ScenarioStart {
        ts: String,
        scenario: String,
        kernel: String,
    },
    #[serde(rename = "action_send")]
    ActionSend {
        ts: String,
        action: String,
        bytes_b64: String,
    },
    #[serde(rename = "serial_chunk")]
    SerialChunk {
        ts: String,
        action: String,
        bytes_b64: String,
    },
    #[serde(rename = "assertion_result")]
    AssertionResult {
        ts: String,
        action: String,
        // "contains" or "regex"; not the enum to keep the JSONL shape decoupled
        // from `bootroom_core::config::AssertionKind`.
        kind: String,
        pattern: String,
        // "pass" or "fail".
        verdict: String,
    },
    #[serde(rename = "scenario_result")]
    ScenarioResult {
        ts: String,
        // "pass" | "fail" | "timeout" | "error".
        verdict: String,
        // Opaque per-action verdict list; produced by the browser engine
        // (04-08) and round-tripped verbatim by the Rust side.
        actions: serde_json::Value,
    },
    #[serde(rename = "transcript_overflow")]
    TranscriptOverflow {
        ts: String,
        // u64 is fine here — the browser engine emits a JS Number
        // (double-precision); 5 MB fits trivially in either type.
        bytes_truncated_estimate: u64,
    },
}

/// Atomic-line JSONL writer.
pub struct TranscriptWriter<W: Write> {
    w: W,
}

impl<W: Write> TranscriptWriter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }

    /// Serialize `event` as one JSON object terminated by `\n` and
    /// write in a single `write_all` call. Atomicity matters when
    /// the underlying writer is a file shared with another tool
    /// (e.g., a tail-following JSONL parser): a half-line is
    /// indistinguishable from a corrupt event.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if serialization fails (mapped to
    /// `InvalidData`) or the underlying writer errors.
    pub fn write_event(&mut self, event: &TranscriptEvent) -> io::Result<()> {
        let mut line = serde_json::to_vec(event)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push(b'\n');
        self.w.write_all(&line)
    }
}

/// Convenience: serialize a slice of events to a `String`.
///
/// Used by tests; production code constructs a `TranscriptWriter`
/// over `BufWriter<File>` and calls `write_event` event-by-event.
///
/// # Panics
///
/// Panics only if `serde_json::to_string` fails on a `TranscriptEvent`,
/// which is unreachable: every field is a plain `String`, `u64`, or
/// `serde_json::Value` — all infallibly serializable.
#[must_use]
pub fn to_jsonl(events: &[TranscriptEvent]) -> String {
    let mut out = String::new();
    for e in events {
        let s = serde_json::to_string(e)
            .expect("TranscriptEvent serialize cannot fail (no unsupported types)");
        out.push_str(&s);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-05-19T14:32:01.123Z";

    fn assert_roundtrip(event: &TranscriptEvent) -> String {
        let s = serde_json::to_string(event).expect("serialize");
        let back: TranscriptEvent = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(&back, event, "roundtrip must preserve the value");
        s
    }

    #[test]
    fn scenario_start_event_roundtrip() {
        let e = TranscriptEvent::ScenarioStart {
            ts: TS.into(),
            scenario: "boot_smoke".into(),
            kernel: "/tmp/Image".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"scenario_start""#), "got: {s}");
        assert!(s.contains(r#""scenario":"boot_smoke""#), "got: {s}");
        assert!(s.contains(r#""kernel":"/tmp/Image""#), "got: {s}");
        assert!(s.contains(r#""ts":"2026-05-19T14:32:01.123Z""#), "got: {s}");
    }

    #[test]
    fn action_send_event_roundtrip() {
        let e = TranscriptEvent::ActionSend {
            ts: TS.into(),
            action: "reboot".into(),
            bytes_b64: "cmVib290DQ==".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"action_send""#), "got: {s}");
        assert!(s.contains(r#""action":"reboot""#), "got: {s}");
        assert!(s.contains(r#""bytes_b64":"cmVib290DQ==""#), "got: {s}");
    }

    #[test]
    fn serial_chunk_event_roundtrip() {
        let e = TranscriptEvent::SerialChunk {
            ts: TS.into(),
            action: "reboot".into(),
            bytes_b64: "WyAg".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"serial_chunk""#), "got: {s}");
        assert!(s.contains(r#""action":"reboot""#), "got: {s}");
        assert!(s.contains(r#""bytes_b64":"WyAg""#), "got: {s}");
    }

    #[test]
    fn assertion_result_event_roundtrip_pass() {
        let e = TranscriptEvent::AssertionResult {
            ts: TS.into(),
            action: "reboot".into(),
            kind: "contains".into(),
            pattern: "login: ".into(),
            verdict: "pass".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"assertion_result""#), "got: {s}");
        assert!(s.contains(r#""verdict":"pass""#), "got: {s}");
        assert!(s.contains(r#""kind":"contains""#), "got: {s}");
    }

    #[test]
    fn assertion_result_event_roundtrip_fail() {
        let e = TranscriptEvent::AssertionResult {
            ts: TS.into(),
            action: "reboot".into(),
            kind: "regex".into(),
            pattern: "Booting\\s+".into(),
            verdict: "fail".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"assertion_result""#), "got: {s}");
        assert!(s.contains(r#""verdict":"fail""#), "got: {s}");
        assert!(s.contains(r#""kind":"regex""#), "got: {s}");
    }

    #[test]
    fn scenario_result_event_roundtrip() {
        let e = TranscriptEvent::ScenarioResult {
            ts: TS.into(),
            verdict: "pass".into(),
            actions: serde_json::json!([{"label":"reboot","verdict":"pass"}]),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"scenario_result""#), "got: {s}");
        assert!(s.contains(r#""verdict":"pass""#), "got: {s}");
        assert!(s.contains(r#""actions":"#), "got: {s}");
    }

    #[test]
    fn transcript_overflow_event_roundtrip() {
        let e = TranscriptEvent::TranscriptOverflow {
            ts: TS.into(),
            bytes_truncated_estimate: 5_000_000,
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"transcript_overflow""#), "got: {s}");
        assert!(
            s.contains(r#""bytes_truncated_estimate":5000000"#),
            "got: {s}"
        );
    }

    #[test]
    fn transcript_overflow_event_deserializes_from_browser_json() {
        // This is the EXACT shape `web/scenario.js` (04-08) emits.
        let wire = r#"{"ts":"2026-05-19T14:32:01.123Z","type":"transcript_overflow","bytes_truncated_estimate":5000000}"#;
        let parsed: TranscriptEvent = serde_json::from_str(wire).expect("deserialize browser JSON");
        match parsed {
            TranscriptEvent::TranscriptOverflow {
                ts,
                bytes_truncated_estimate,
            } => {
                assert_eq!(ts, "2026-05-19T14:32:01.123Z");
                assert_eq!(bytes_truncated_estimate, 5_000_000);
            }
            other => panic!("expected TranscriptOverflow, got {other:?}"),
        }
    }

    #[test]
    fn transcript_writer_writes_one_line_per_event() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = TranscriptWriter::new(&mut buf);
            w.write_event(&TranscriptEvent::ScenarioStart {
                ts: TS.into(),
                scenario: "boot_smoke".into(),
                kernel: "/tmp/Image".into(),
            })
            .unwrap();
            w.write_event(&TranscriptEvent::ActionSend {
                ts: TS.into(),
                action: "reboot".into(),
                bytes_b64: "cmVib290DQ==".into(),
            })
            .unwrap();
            w.write_event(&TranscriptEvent::ScenarioResult {
                ts: TS.into(),
                verdict: "pass".into(),
                actions: serde_json::json!([]),
            })
            .unwrap();
        }
        let s = String::from_utf8(buf).expect("utf8");
        assert_eq!(s.matches('\n').count(), 3, "three newline terminators: {s}");
        assert_eq!(s.lines().count(), 3, "three lines: {s}");
        for line in s.lines() {
            let _: TranscriptEvent =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid line {line:?}: {e}"));
        }
    }

    #[test]
    fn to_jsonl_concatenates_with_trailing_newline() {
        let events = vec![
            TranscriptEvent::ScenarioStart {
                ts: TS.into(),
                scenario: "s".into(),
                kernel: "/k".into(),
            },
            TranscriptEvent::ScenarioResult {
                ts: TS.into(),
                verdict: "pass".into(),
                actions: serde_json::json!([]),
            },
        ];
        let s = to_jsonl(&events);
        assert!(s.ends_with('\n'), "trailing newline: {s:?}");
        assert_eq!(s.lines().count(), 2);
    }
}
