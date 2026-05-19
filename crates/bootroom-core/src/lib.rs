//! bootroom-core: pure types and protocol definitions.
//!
//! Phase 2 adds `WsMessage` (the `/ws` protocol enum) and `GuestState`
//! (status pill states). Phase 4's headless `bootroom run` driver reuses
//! the same `WsMessage` enum unchanged.

#![cfg_attr(not(test), deny(unsafe_code))]

use serde::{Deserialize, Serialize};

pub mod escape;
pub use escape::{decode_bytes_escape, EscapeError};

/// Wire-level message exchanged over the `/ws` endpoint.
///
/// Externally tagged via `#[serde(tag = "type")]`, producing JSON of the form
/// `{"type": "SerialIn", "data": "..."}`. Byte payloads (`SerialIn`,
/// `SerialOut`) are base64-encoded so the protocol stays JSON-only on the
/// wire — see `02-CONTEXT.md` decision "/ws message protocol — tagged JSON
/// only".
///
/// Note: `#[serde(deny_unknown_fields)]` is intentionally NOT applied —
/// Phase 4 may add variants additively and older clients should ignore
/// unknown fields gracefully (02-RESEARCH.md Open Question 3).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Host -> guest. Bytes injected into guest stdin. `data` is base64.
    SerialIn { data: String },
    /// Guest -> host. Bytes the guest emitted on serial. `data` is base64.
    /// Browser emits these for the server to log; server may forward in
    /// Phase 4 headless mode.
    SerialOut { data: String },
    /// Server -> client. Authoritative guest status pill state. When the
    /// `/ws` connection is live this overrides the browser's local view.
    State { state: GuestState },
    /// Client -> server. Asks the server (and observers) to log a Launch
    /// action; the browser then page-reloads to re-instantiate qemu-wasm.
    Launch,
    /// Client -> server. Asks the server (and observers) to log a Reset
    /// action; in Phase 2 this is identical to `Launch` from the
    /// browser's perspective.
    Reset,
    /// Server -> client on connect. `version` is the server's
    /// `CARGO_PKG_VERSION`. Mismatched clients log a warning but proceed.
    Hello { version: String },
    /// Server -> client. Watcher detected a kernel rebuild. `ok=true` means
    /// size-stability and ELF magic both passed; `ok=false` carries `reason`
    /// (e.g., `"not ELF"`). The browser shows a non-intrusive banner; Launch
    /// is user-initiated. WCH-05.
    KernelChanged {
        ok: bool,
        mtime: i64,
        size: u64,
        sha256_prefix: String,
        reason: Option<String>,
    },
    /// Server -> client. `bootroom.toml` was edited and re-parsed
    /// successfully. `config` is the same JSON projection `/api/config`
    /// returns. CFG-10.
    ConfigUpdate { config: serde_json::Value },
    /// Server -> client. `bootroom.toml` was edited but re-parse failed.
    /// The last-known-good config remains active. `line`/`col` are 1-based
    /// when the error has a TOML span. CFG-10.
    ConfigInvalid {
        error: String,
        line: Option<u32>,
        col: Option<u32>,
    },
}

/// Status pill state machine. Default serde representation: bare string
/// variant (`"Idle" | "Loading" | "Running" | "Halted"`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestState {
    /// Initial render — before xterm + qemu init.
    Idle,
    /// xterm mounted, qemu-wasm Module not yet `onRuntimeInitialized`.
    Loading,
    /// `onRuntimeInitialized` fired AND first `SerialOut` byte seen —
    /// the guest is actually executing.
    Running,
    /// `Module.onExit` / `onAbort`, OR server pushed `State { Halted }`.
    Halted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_in_roundtrip() {
        let m = WsMessage::SerialIn {
            data: "aGVsbG8=".into(),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"type":"SerialIn","data":"aGVsbG8="}"#);
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn unit_variant_serializes_as_object_with_only_type() {
        let launch = serde_json::to_string(&WsMessage::Launch).unwrap();
        assert_eq!(launch, r#"{"type":"Launch"}"#);
        let reset = serde_json::to_string(&WsMessage::Reset).unwrap();
        assert_eq!(reset, r#"{"type":"Reset"}"#);

        let back_launch: WsMessage = serde_json::from_str(&launch).unwrap();
        assert_eq!(back_launch, WsMessage::Launch);
        let back_reset: WsMessage = serde_json::from_str(&reset).unwrap();
        assert_eq!(back_reset, WsMessage::Reset);
    }

    #[test]
    fn state_message_contains_nested_state() {
        let m = WsMessage::State {
            state: GuestState::Running,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"type":"State","state":"Running"}"#);
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn hello_message_carries_version_string() {
        let m = WsMessage::Hello {
            version: "0.1.0".into(),
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains(r#""version":"0.1.0""#), "got: {s}");
        assert!(s.contains(r#""type":"Hello""#), "got: {s}");
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn guest_state_serializes_as_bare_string() {
        let s = serde_json::to_string(&GuestState::Halted).unwrap();
        assert_eq!(s, r#""Halted""#);
    }

    #[test]
    fn wsmessage_implements_required_derives() {
        let m = WsMessage::SerialIn {
            data: "Zm9v".into(),
        };
        let cloned = m.clone();
        assert_eq!(m, cloned);
    }

    #[test]
    fn kernel_changed_ok_true_roundtrip() {
        let m = WsMessage::KernelChanged {
            ok: true,
            mtime: 1_715_000_000,
            size: 12_345_678,
            sha256_prefix: "abc123def456".into(),
            reason: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
        // Spot-check wire shape — type tag + field presence.
        assert!(s.contains(r#""type":"KernelChanged""#), "got: {s}");
        assert!(s.contains(r#""ok":true"#), "got: {s}");
        assert!(s.contains(r#""reason":null"#), "got: {s}");
    }

    #[test]
    fn kernel_changed_ok_false_with_reason_roundtrip() {
        let m = WsMessage::KernelChanged {
            ok: false,
            mtime: 0,
            size: 0,
            sha256_prefix: String::new(),
            reason: Some("not ELF".into()),
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
        assert!(s.contains(r#""reason":"not ELF""#), "got: {s}");
    }

    #[test]
    fn config_update_carries_opaque_value() {
        let m = WsMessage::ConfigUpdate {
            config: serde_json::json!({ "schema_version": 1, "actions": [] }),
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
        assert!(s.contains(r#""type":"ConfigUpdate""#), "got: {s}");
    }

    #[test]
    fn config_invalid_with_and_without_span() {
        let with_span = WsMessage::ConfigInvalid {
            error: "unknown field 'lable'".into(),
            line: Some(12),
            col: Some(1),
        };
        let s = serde_json::to_string(&with_span).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, with_span);
        assert!(s.contains(r#""line":12"#), "got: {s}");
        assert!(s.contains(r#""col":1"#), "got: {s}");

        let without_span = WsMessage::ConfigInvalid {
            error: "permission denied".into(),
            line: None,
            col: None,
        };
        let s = serde_json::to_string(&without_span).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, without_span);
        assert!(s.contains(r#""line":null"#), "got: {s}");
        assert!(s.contains(r#""col":null"#), "got: {s}");
    }

    #[test]
    fn large_mtime_survives_i64() {
        // Pitfall #8 (03-RESEARCH): millennium-scale Unix epoch survives i64.
        let m = WsMessage::KernelChanged {
            ok: true,
            mtime: 9_999_999_999_999_i64,
            size: u64::MAX,
            sha256_prefix: "deadbeefcafe".into(),
            reason: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: WsMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }
}
