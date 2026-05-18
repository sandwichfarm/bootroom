//! bootroom-core: pure types and protocol definitions.
//!
//! Phase 2 adds `WsMessage` (the `/ws` protocol enum) and `GuestState`
//! (status pill states). Phase 4's headless `bootroom run` driver reuses
//! the same `WsMessage` enum unchanged.

#![cfg_attr(not(test), deny(unsafe_code))]

use serde::{Deserialize, Serialize};

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
}
