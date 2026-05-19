//! `--log-file` JSONL transcript shape (RUN-08).
//!
//! Stub module — RED phase. Tests below pin the canonical wire shape
//! for the six event types and the writer's atomic-line guarantee.
//! Implementation lands in the GREEN commit.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// One line in a `--log-file` JSONL transcript. RED-phase placeholder.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TranscriptEvent {
    Placeholder,
}

/// Atomic-line JSONL writer. RED-phase placeholder.
pub struct TranscriptWriter<W: Write> {
    _w: std::marker::PhantomData<W>,
}

impl<W: Write> TranscriptWriter<W> {
    pub fn new(_w: W) -> Self {
        Self {
            _w: std::marker::PhantomData,
        }
    }

    /// # Errors
    /// Always returns `Unsupported` in the RED phase.
    pub fn write_event(&mut self, _event: &TranscriptEvent) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "not implemented"))
    }
}

#[must_use]
pub fn to_jsonl(_events: &[TranscriptEvent]) -> String {
    String::new()
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
        // Variant doesn't exist yet — compile error in RED.
        let e = TranscriptEvent::ScenarioStart {
            ts: TS.into(),
            scenario: "boot_smoke".into(),
            kernel: "/tmp/Image".into(),
        };
        let s = assert_roundtrip(&e);
        assert!(s.contains(r#""type":"scenario_start""#), "got: {s}");
    }

    #[test]
    fn transcript_overflow_event_deserializes_from_browser_json() {
        let wire = r#"{"ts":"2026-05-19T14:32:01.123Z","type":"transcript_overflow","bytes_truncated_estimate":5000000}"#;
        let parsed: TranscriptEvent = serde_json::from_str(wire).expect("deserialize browser JSON");
        match parsed {
            TranscriptEvent::TranscriptOverflow {
                bytes_truncated_estimate,
                ..
            } => {
                assert_eq!(bytes_truncated_estimate, 5_000_000);
            }
            other => panic!("expected TranscriptOverflow, got {other:?}"),
        }
    }
}
