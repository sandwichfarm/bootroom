//! Byte-string escape decoder shared by the TOML config parser and the
//! `--action` CLI surface.
//!
//! Accepts these escape forms (everything else is rejected with a typed
//! error carrying a byte offset):
//!
//! - `\r` -> 0x0d
//! - `\n` -> 0x0a
//! - `\t` -> 0x09
//! - `\0` -> 0x00
//! - `\\` -> 0x5c (literal backslash)
//! - `\xNN` -> the byte with hex value `NN` (two hex digits required)
//!
//! Non-ASCII input bytes pass through verbatim — strings are walked at the
//! byte level (`str::as_bytes()`), so UTF-8-encoded characters land in the
//! output buffer unchanged.
//!
//! No `unsafe`, no panic path on operator-supplied input: every error
//! variant carries the byte offset where the problem was detected so the
//! UI can underline the bad character.

use std::fmt;

/// Errors produced by [`decode_bytes_escape`].
///
/// `pos` is a 0-based byte offset into the input string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeError {
    /// The input ended with a lone backslash, e.g. `"abc\\"`.
    TrailingBackslash { pos: usize },
    /// Backslash followed by an unknown character, e.g. `"\\q"`.
    UnknownEscape { pos: usize, ch: char },
    /// `\xNN` escape with fewer than two hex digits available, e.g. `"\\x4"`.
    ShortHex { pos: usize },
    /// `\xNN` escape with a non-hex digit, e.g. `"\\xZ1"`.
    BadHex { pos: usize },
}

impl fmt::Display for EscapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EscapeError::TrailingBackslash { pos } => {
                write!(f, "trailing backslash at byte {pos}")
            }
            EscapeError::UnknownEscape { pos, ch } => {
                write!(f, "unknown escape '\\{ch}' at byte {pos}")
            }
            EscapeError::ShortHex { pos } => {
                write!(
                    f,
                    "truncated \\xNN escape at byte {pos} (need two hex digits)"
                )
            }
            EscapeError::BadHex { pos } => {
                write!(f, "invalid hex digit at byte {pos}")
            }
        }
    }
}

impl std::error::Error for EscapeError {}

/// Decode a string containing escape sequences into a byte vector.
///
/// See module-level docs for the accepted escape set. Non-escape input
/// bytes are passed through unchanged, so UTF-8 multi-byte sequences
/// survive verbatim.
///
/// # Errors
///
/// Returns [`EscapeError`] on a malformed escape; the variant carries the
/// 0-based byte offset of the offending character.
pub fn decode_bytes_escape(s: &str) -> Result<Vec<u8>, EscapeError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        // Backslash — consume one escape.
        if i + 1 >= bytes.len() {
            return Err(EscapeError::TrailingBackslash { pos: i });
        }
        let next = bytes[i + 1];
        match next {
            b'r' => {
                out.push(0x0d);
                i += 2;
            }
            b'n' => {
                out.push(0x0a);
                i += 2;
            }
            b't' => {
                out.push(0x09);
                i += 2;
            }
            b'0' => {
                out.push(0x00);
                i += 2;
            }
            b'\\' => {
                out.push(b'\\');
                i += 2;
            }
            b'x' => {
                // Need two hex digits after the 'x'.
                if i + 3 >= bytes.len() {
                    return Err(EscapeError::ShortHex { pos: i });
                }
                let h1 = bytes[i + 2];
                let h2 = bytes[i + 3];
                let v1 = hex_digit(h1).ok_or(EscapeError::BadHex { pos: i + 2 })?;
                let v2 = hex_digit(h2).ok_or(EscapeError::BadHex { pos: i + 3 })?;
                out.push((v1 << 4) | v2);
                i += 4;
            }
            _ => {
                // Map non-ASCII follow-up bytes to '?' (Pattern 4 detail).
                let ch = if next < 0x80 { next as char } else { '?' };
                return Err(EscapeError::UnknownEscape { pos: i + 1, ch });
            }
        }
    }
    Ok(out)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_carriage_return() {
        assert_eq!(
            decode_bytes_escape("reboot\\r").unwrap(),
            vec![b'r', b'e', b'b', b'o', b'o', b't', 0x0d]
        );
    }

    #[test]
    fn decodes_repeated_hex_escapes() {
        assert_eq!(
            decode_bytes_escape("\\x03\\x03\\x03").unwrap(),
            vec![0x03, 0x03, 0x03]
        );
    }

    #[test]
    fn decodes_simple_escape_set() {
        assert_eq!(
            decode_bytes_escape("\\n\\t\\0\\\\").unwrap(),
            vec![0x0a, 0x09, 0x00, b'\\']
        );
    }

    #[test]
    fn bad_hex_reports_position() {
        // "abc\xZ1" — the backslash is at index 3, 'Z' at index 5.
        let err = decode_bytes_escape("abc\\xZ1").unwrap_err();
        assert_eq!(err, EscapeError::BadHex { pos: 5 });
    }

    #[test]
    fn trailing_backslash_reports_position() {
        let err = decode_bytes_escape("abc\\").unwrap_err();
        assert_eq!(err, EscapeError::TrailingBackslash { pos: 3 });
    }

    #[test]
    fn short_hex_reports_position() {
        // "\x4" — only one hex digit; backslash at index 0.
        let err = decode_bytes_escape("\\x4").unwrap_err();
        assert_eq!(err, EscapeError::ShortHex { pos: 0 });
    }

    #[test]
    fn unknown_escape_reports_position_and_char() {
        let err = decode_bytes_escape("\\q").unwrap_err();
        assert_eq!(err, EscapeError::UnknownEscape { pos: 1, ch: 'q' });
    }

    #[test]
    fn utf8_passes_through_byte_for_byte() {
        // 'é' is UTF-8 0xc3 0xa9.
        assert_eq!(decode_bytes_escape("é").unwrap(), vec![0xc3, 0xa9]);
    }

    #[test]
    fn non_ascii_after_backslash_maps_to_question_mark() {
        // "\é" — backslash at 0, then UTF-8 lead byte 0xc3 at index 1.
        let err = decode_bytes_escape("\\é").unwrap_err();
        assert_eq!(err, EscapeError::UnknownEscape { pos: 1, ch: '?' });
    }

    #[test]
    fn escape_error_displays_and_implements_error_trait() {
        let e = EscapeError::TrailingBackslash { pos: 3 };
        assert_eq!(format!("{e}"), "trailing backslash at byte 3");
        let _: &dyn std::error::Error = &e;
        // Clone + Eq present (used above in assert_eq!).
        let _cloned = e.clone();
    }

    #[test]
    fn upper_and_lower_hex_digits_accepted() {
        assert_eq!(decode_bytes_escape("\\xAB").unwrap(), vec![0xab]);
        assert_eq!(decode_bytes_escape("\\xab").unwrap(), vec![0xab]);
        assert_eq!(decode_bytes_escape("\\xFf").unwrap(), vec![0xff]);
    }
}
