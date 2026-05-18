//! bootroom-core: pure types and protocol definitions.
//!
//! Phase 2 adds `WsMessage` (the `/ws` protocol enum) and `GuestState`
//! (status pill states). Phase 4's headless `bootroom run` driver reuses
//! the same `WsMessage` enum unchanged.

#![cfg_attr(not(test), deny(unsafe_code))]

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
