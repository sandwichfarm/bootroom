//! `bootroom.toml` schema + load-time validation.
//!
//! This module is the single source of truth for the Phase-3 config
//! surface (Pitfall #5: schema drift). Every downstream consumer —
//! `/api/config`, `bootroom check`, the live-reload watcher, and the
//! Phase-4 scenario engine — projects from these types.
//!
//! Pure: no I/O, no async, no panic on operator input. All errors
//! surface through [`LoadError`] with optional 1-based `line`/`col`
//! span coordinates suitable for UI underlining.
//!
//! See `.planning/phases/03-config-buttons-watcher/03-CONTEXT.md`
//! decision D-01 for the canonical schema shape.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::escape::decode_bytes_escape;

// --- TOML schema (validated by serde with deny_unknown_fields) -----------

/// Root `bootroom.toml` document.
///
/// `schema_version` is required (no `#[serde(default)]`) and locked to
/// the integer `1` for the Phase-3 surface; incompatible versions
/// produce a typed [`LoadError`] via [`LoadedConfig::load_from_str`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default, rename = "action")]
    pub actions: Vec<Action>,
    #[serde(default, rename = "scenario")]
    pub scenarios: Vec<Scenario>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub label: String,
    pub bytes: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub actions: Vec<String>,
    #[serde(default, rename = "assert")]
    pub assertions: Vec<Assertion>,
    #[serde(default = "default_scenario_timeout")]
    pub timeout_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub kind: AssertionKind,
    pub pattern: String,
    pub after: String,
    #[serde(default = "default_assertion_timeout")]
    pub timeout_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssertionKind {
    Contains,
    Regex,
}

fn default_scenario_timeout() -> u64 {
    30_000
}

fn default_assertion_timeout() -> u64 {
    5_000
}

// --- Load errors --------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoadErrorKind {
    Parse,
    SchemaMismatch { actual: u32 },
    DecodeBytes,
    UnknownActionRef,
    DuplicateAction,
}

/// Failure raised while parsing or validating a `bootroom.toml`.
///
/// `line` and `col` are 1-based; both are `None` for errors that don't
/// originate at a specific TOML span (cross-validation errors, CLI
/// override duplicates, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub message: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    kind: LoadErrorKind,
}

impl LoadError {
    fn schema_mismatch(actual: u32) -> Self {
        LoadError {
            message: format!(
                "schema_version {actual} not supported; bootroom requires schema_version = 1"
            ),
            line: None,
            col: None,
            kind: LoadErrorKind::SchemaMismatch { actual },
        }
    }

    fn duplicate_action(label: &str) -> Self {
        LoadError {
            message: format!("duplicate action label '{label}'"),
            line: None,
            col: None,
            kind: LoadErrorKind::DuplicateAction,
        }
    }

    fn unknown_action_ref(scenario: &str, action: &str) -> Self {
        LoadError {
            message: format!(
                "scenario '{scenario}' references unknown action '{action}'"
            ),
            line: None,
            col: None,
            kind: LoadErrorKind::UnknownActionRef,
        }
    }

    /// True when the failure was a `schema_version` mismatch.
    #[must_use]
    pub fn is_schema_version_mismatch(&self) -> bool {
        matches!(self.kind, LoadErrorKind::SchemaMismatch { .. })
    }

    /// The actual `schema_version` value encountered, when this error is
    /// a schema mismatch. `None` for all other variants.
    #[must_use]
    pub fn actual_version(&self) -> Option<u32> {
        match self.kind {
            LoadErrorKind::SchemaMismatch { actual } => Some(actual),
            _ => None,
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.col) {
            (Some(l), Some(c)) => write!(f, "{} (line {l}, col {c})", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for LoadError {}

// --- Parser primitives --------------------------------------------------

/// Parse a `bootroom.toml` string into the raw [`Config`] AST.
///
/// Performs only TOML syntax and `deny_unknown_fields` validation;
/// schema-version checking, byte-escape decoding, scenario cross-validation,
/// and CLI override merging live in [`LoadedConfig::load_from_str`] /
/// [`LoadedConfig::load_from_str_with_overrides`].
///
/// # Errors
///
/// Returns [`LoadError`] for any TOML syntax error or unknown field; the
/// `line` / `col` fields carry 1-based span coordinates when the underlying
/// `toml` crate reports them.
pub fn parse_str(input: &str) -> Result<Config, LoadError> {
    match toml::from_str::<Config>(input) {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            let (line, col) = e
                .span()
                .and_then(|range| offset_to_line_col(input, range.start))
                .map_or((None, None), |(l, c)| (Some(l), Some(c)));
            Err(LoadError {
                message: e.message().to_string(),
                line,
                col,
                kind: LoadErrorKind::Parse,
            })
        }
    }
}

/// Convert a 0-based byte offset within `input` into a 1-based (line, col)
/// pair suitable for `vim`/`code` editor jump-to-line.
///
/// Returns `None` if `byte_off > input.len()`. Columns count Unicode scalars
/// (`char`s), not bytes, so multi-byte UTF-8 sequences contribute a single
/// column each.
#[must_use]
pub fn offset_to_line_col(input: &str, byte_off: usize) -> Option<(u32, u32)> {
    if byte_off > input.len() {
        return None;
    }
    let prefix = &input[..byte_off];
    let line = u32::try_from(prefix.bytes().filter(|&b| b == b'\n').count())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    // Column = 1 + chars since the last newline (or start of input).
    let last_nl = prefix.rfind('\n');
    let col_slice = match last_nl {
        Some(i) => &prefix[i + 1..],
        None => prefix,
    };
    let col = u32::try_from(col_slice.chars().count())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    Some((line, col))
}

// --- Resolved (post-merge, byte-decoded) views --------------------------

/// One Action after byte-escape decoding and CLI-override merging.
///
/// `bytes_decoded` is the raw byte sequence written to guest stdin when
/// the button is clicked; the original TOML `bytes` string is not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub label: String,
    pub bytes_decoded: Vec<u8>,
    pub group: Option<String>,
    pub description: Option<String>,
}

/// An ad-hoc Action introduced via `--action label=<bytes>` on the CLI.
///
/// `bytes` is post-`decode_bytes_escape` (the CLI parser is responsible
/// for decoding). No UI metadata: the operator gave us a label and a byte
/// sequence; group/description default to `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliAction {
    pub label: String,
    pub bytes: Vec<u8>,
}

/// A fully-validated `bootroom.toml` ready for projection to `/api/config`,
/// `bootroom check`, the watcher, or the Phase-4 scenario engine.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    actions: Vec<ResolvedAction>,
    scenarios: Vec<Scenario>,
    actions_by_label: HashMap<String, usize>,
}

impl LoadedConfig {
    /// Parse + validate without any CLI overrides.
    ///
    /// # Errors
    ///
    /// Same as [`LoadedConfig::load_from_str_with_overrides`].
    pub fn load_from_str(s: &str) -> Result<Self, LoadError> {
        Self::load_from_str_with_overrides(s, &[])
    }

    /// Parse + validate, then merge `cli` overrides into the action list.
    ///
    /// Override semantics (CONTEXT D-02):
    /// - Existing label is replaced in place; group/description cleared.
    /// - New label appends to the end.
    /// - Among CLI-only collisions, the last `--action label=...` wins.
    /// - Source-TOML duplicate labels are rejected after the merge.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] for:
    /// - any error from [`parse_str`]
    /// - `schema_version != 1`
    /// - malformed `\xNN` / `\q` etc. in any `Action.bytes`
    /// - duplicate action labels in the merged set
    /// - any [`Scenario::actions`] entry that doesn't resolve to a
    ///   known label
    pub fn load_from_str_with_overrides(
        s: &str,
        cli: &[CliAction],
    ) -> Result<Self, LoadError> {
        let cfg = parse_str(s)?;
        Self::from_config(cfg, cli)
    }

    fn from_config(cfg: Config, cli: &[CliAction]) -> Result<Self, LoadError> {
        if cfg.schema_version != 1 {
            return Err(LoadError::schema_mismatch(cfg.schema_version));
        }

        // Decode every TOML action's bytes first (preserves insertion order).
        let mut actions: Vec<ResolvedAction> = Vec::with_capacity(cfg.actions.len());
        for a in cfg.actions {
            let bytes_decoded = decode_bytes_escape(&a.bytes).map_err(|e| LoadError {
                message: format!("action '{}': {e}", a.label),
                line: None,
                col: None,
                kind: LoadErrorKind::DecodeBytes,
            })?;
            actions.push(ResolvedAction {
                label: a.label,
                bytes_decoded,
                group: a.group,
                description: a.description,
            });
        }

        // Merge CLI overrides. Dedupe-replace by label: last `--action` wins
        // among CLI-only collisions; existing TOML labels are replaced in
        // place and have their UI metadata (group/description) cleared.
        for c in cli {
            if let Some(existing) = actions.iter_mut().find(|x| x.label == c.label) {
                existing.bytes_decoded.clone_from(&c.bytes);
                existing.group = None;
                existing.description = None;
            } else {
                actions.push(ResolvedAction {
                    label: c.label.clone(),
                    bytes_decoded: c.bytes.clone(),
                    group: None,
                    description: None,
                });
            }
        }

        // Post-merge label uniqueness check. This only trips when the source
        // TOML itself declared duplicate labels; CLI overrides go through
        // the dedupe-replace branch above and cannot land here.
        let mut actions_by_label: HashMap<String, usize> = HashMap::new();
        for (i, a) in actions.iter().enumerate() {
            if actions_by_label.insert(a.label.clone(), i).is_some() {
                return Err(LoadError::duplicate_action(&a.label));
            }
        }

        // Scenario cross-validation: every referenced action label must
        // resolve to an entry in the merged action map.
        for s in &cfg.scenarios {
            for refed in &s.actions {
                if !actions_by_label.contains_key(refed) {
                    return Err(LoadError::unknown_action_ref(&s.name, refed));
                }
            }
        }

        Ok(LoadedConfig {
            actions,
            scenarios: cfg.scenarios,
            actions_by_label,
        })
    }

    /// All merged actions in TOML insertion order (new CLI actions appended).
    #[must_use]
    pub fn actions(&self) -> &[ResolvedAction] {
        &self.actions
    }

    #[must_use]
    pub fn scenarios(&self) -> &[Scenario] {
        &self.scenarios
    }

    #[must_use]
    pub fn action_by_label(&self, label: &str) -> Option<&ResolvedAction> {
        self.actions_by_label
            .get(label)
            .and_then(|&i| self.actions.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: `bytes` uses a TOML literal string (single-quoted) so the
    // backslash survives TOML's own escape pass — the byte-level escape
    // decoding happens inside `decode_bytes_escape`.
    const VALID_TOML: &str = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = 'reboot\r'
group = "system"
description = "Soft reboot via init"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]
timeout_ms = 10000

  [[scenario.assert]]
  kind = "contains"
  pattern = "Booting"
  after = "reboot"
  timeout_ms = 2000
"#;

    // --- CFG-02: parse a valid TOML round-trip ---------------------------

    #[test]
    fn actions_roundtrip() {
        let cfg = parse_str(VALID_TOML).expect("parse VALID_TOML");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.actions.len(), 1);
        let a = &cfg.actions[0];
        assert_eq!(a.label, "reboot");
        assert_eq!(a.bytes, "reboot\\r", "raw TOML literal string (pre escape-decode)");
        assert_eq!(a.group.as_deref(), Some("system"));
        assert_eq!(a.description.as_deref(), Some("Soft reboot via init"));

        let loaded = LoadedConfig::load_from_str(VALID_TOML).expect("load VALID_TOML");
        let resolved = &loaded.actions()[0];
        assert_eq!(resolved.label, "reboot");
        assert_eq!(
            resolved.bytes_decoded,
            vec![b'r', b'e', b'b', b'o', b'o', b't', 0x0d]
        );
    }

    // --- CFG-03: scenarios parse with assertions ------------------------

    #[test]
    fn scenarios_parse() {
        let loaded = LoadedConfig::load_from_str(VALID_TOML).expect("load");
        let scenarios = loaded.scenarios();
        assert_eq!(scenarios.len(), 1);
        let s = &scenarios[0];
        assert_eq!(s.name, "boot_smoke");
        assert_eq!(s.actions, vec!["reboot".to_string()]);
        assert_eq!(s.timeout_ms, 10_000);
        assert_eq!(s.assertions.len(), 1);
        let a = &s.assertions[0];
        assert_eq!(a.kind, AssertionKind::Contains);
        assert_eq!(a.pattern, "Booting");
        assert_eq!(a.after, "reboot");
        assert_eq!(a.timeout_ms, 2_000);
    }

    // --- CFG-04: schema_version mismatch --------------------------------

    #[test]
    fn schema_version_rejected() {
        for bad in [0u32, 2u32, 99u32] {
            let s = format!("schema_version = {bad}\n");
            let err = LoadedConfig::load_from_str(&s).expect_err("expected mismatch");
            assert!(err.is_schema_version_mismatch(), "actual: {err:?}");
            assert_eq!(err.actual_version(), Some(bad));
        }
        // Sanity: schema_version = 1 with no actions/scenarios parses fine.
        LoadedConfig::load_from_str("schema_version = 1\n").expect("schema_version=1 ok");
    }

    // --- CFG-05: deny_unknown_fields with span --------------------------

    #[test]
    fn deny_unknown_fields_with_span() {
        let bad = "schema_version = 1\n[[action]]\nlable = \"x\"\n";
        let err = LoadedConfig::load_from_str(bad).expect_err("typo should fail");
        assert!(
            err.message.to_lowercase().contains("unknown field")
                || err.message.contains("lable"),
            "message did not mention unknown field: {}",
            err.message
        );
        assert_eq!(err.line, Some(3), "line; full err: {err:?}");
        assert_eq!(err.col, Some(1), "col; full err: {err:?}");
    }

    // --- CFG-06: scenario references unknown action --------------------

    #[test]
    fn scenario_unknown_action_ref() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["missing_one"]
"#;
        let err = LoadedConfig::load_from_str(s).expect_err("unknown ref should fail");
        assert!(
            err.message.contains("boot_smoke"),
            "must name scenario; got: {}",
            err.message
        );
        assert!(
            err.message.contains("missing_one"),
            "must name missing action; got: {}",
            err.message
        );
    }

    // --- ACT-03: CLI override merge semantics ---------------------------

    #[test]
    fn cli_override_replaces_existing_action_bytes() {
        let cli = vec![CliAction {
            label: "reboot".into(),
            bytes: vec![0x03],
        }];
        let loaded =
            LoadedConfig::load_from_str_with_overrides(VALID_TOML, &cli).expect("ok");
        let actions = loaded.actions();
        assert_eq!(actions.len(), 1);
        let a = &actions[0];
        assert_eq!(a.label, "reboot");
        assert_eq!(a.bytes_decoded, vec![0x03]);
        // CLI override clears UI metadata even when replacing existing.
        assert!(a.group.is_none(), "group should be cleared on override");
        assert!(
            a.description.is_none(),
            "description should be cleared on override"
        );
    }

    #[test]
    fn cli_override_appends_new_action() {
        let cli = vec![CliAction {
            label: "newone".into(),
            bytes: vec![0x41],
        }];
        let loaded =
            LoadedConfig::load_from_str_with_overrides(VALID_TOML, &cli).expect("ok");
        let actions = loaded.actions();
        assert_eq!(actions.len(), 2);
        // Original first, new appended.
        assert_eq!(actions[0].label, "reboot");
        assert_eq!(actions[1].label, "newone");
        assert_eq!(actions[1].bytes_decoded, vec![0x41]);
        assert!(actions[1].group.is_none());
        assert!(actions[1].description.is_none());
    }

    #[test]
    fn last_cli_action_wins_for_same_label() {
        // No "x" action in the TOML; two CLI overrides for "x".
        let toml = "schema_version = 1\n";
        let cli = vec![
            CliAction {
                label: "x".into(),
                bytes: vec![1],
            },
            CliAction {
                label: "x".into(),
                bytes: vec![2],
            },
        ];
        let loaded = LoadedConfig::load_from_str_with_overrides(toml, &cli).expect("ok");
        let actions = loaded.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "x");
        assert_eq!(
            actions[0].bytes_decoded,
            vec![2],
            "last --action x= should win"
        );
    }

    // --- CFG-09 prerequisite: insertion order preserved -----------------

    #[test]
    fn actions_insertion_order_preserved() {
        let s = r#"
schema_version = 1

[[action]]
label = "alpha"
bytes = "a"

[[action]]
label = "beta"
bytes = "b"

[[action]]
label = "gamma"
bytes = "c"
"#;
        let loaded = LoadedConfig::load_from_str(s).expect("ok");
        let labels: Vec<&str> = loaded.actions().iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["alpha", "beta", "gamma"]);
    }

    // --- offset_to_line_col: ASCII + Unicode ----------------------------

    #[test]
    fn offset_to_line_col_basic() {
        assert_eq!(offset_to_line_col("a\nb\nc", 4), Some((3, 1)));
        assert_eq!(offset_to_line_col("", 0), Some((1, 1)));
        assert_eq!(offset_to_line_col("abc", 100), None);
    }

    #[test]
    fn offset_to_line_col_handles_unicode_columns() {
        // "aé\nx" — 'é' is 2 bytes (0xc3 0xa9). 'x' starts at byte 4.
        // That's line 2, col 1. With prefix.len() the col would be wrong.
        let s = "aé\nx";
        assert_eq!(offset_to_line_col(s, 4), Some((2, 1)));
    }

    // --- Source-TOML duplicate labels rejected --------------------------

    // --- Phase 4 RUN-04: regex compile-check at load -------------------

    #[test]
    fn regex_assertion_valid_pattern_loads_ok() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "regex"
  pattern = 'Booting[a-z]+'
  after = "reboot"
"#;
        LoadedConfig::load_from_str(s).expect("valid regex must load");
    }

    #[test]
    fn regex_assertion_invalid_pattern_rejected() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "regex"
  pattern = 'unclosed['
  after = "reboot"
"#;
        let err = LoadedConfig::load_from_str(s).expect_err("unclosed [ must fail");
        assert!(err.is_invalid_regex(), "is_invalid_regex; full: {err:?}");
        assert!(
            err.message.contains("boot_smoke"),
            "must name scenario; got: {}",
            err.message
        );
        assert!(
            err.message.contains("reboot"),
            "must name after-label; got: {}",
            err.message
        );
        assert!(
            err.message.contains("unclosed["),
            "must include offending pattern; got: {}",
            err.message
        );
    }

    #[test]
    fn regex_assertion_backref_rejected() {
        // Backreferences are JS-only; Rust `regex` rejects them. This is
        // exactly the intersection-subset policy (Pitfall #1 in 04-RESEARCH).
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "regex"
  pattern = '(a)\1'
  after = "reboot"
"#;
        let err = LoadedConfig::load_from_str(s).expect_err("backref must fail");
        assert!(err.is_invalid_regex(), "is_invalid_regex; full: {err:?}");
    }

    #[test]
    fn regex_assertion_lookaround_rejected() {
        // Lookaround is JS-only; Rust `regex` rejects it.
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "regex"
  pattern = '(?=foo)'
  after = "reboot"
"#;
        let err = LoadedConfig::load_from_str(s).expect_err("lookaround must fail");
        assert!(err.is_invalid_regex(), "is_invalid_regex; full: {err:?}");
    }

    #[test]
    fn contains_assertion_with_bracket_loads_ok() {
        // `kind = "contains"` is substring; the pattern is a literal,
        // not a regex. An unclosed `[` is therefore fine.
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "contains"
  pattern = 'unclosed['
  after = "reboot"
"#;
        LoadedConfig::load_from_str(s).expect("contains assertion must load");
    }

    // --- Phase 4 RUN-05: `after`-resolution check at load --------------

    #[test]
    fn assertion_after_resolves_to_scenario_action_loads_ok() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "reboot"
"#;
        LoadedConfig::load_from_str(s).expect("after=reboot is in actions, must load");
    }

    #[test]
    fn assertion_after_any_loads_ok() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "any"
"#;
        LoadedConfig::load_from_str(s).expect("after=any is always legal");
    }

    #[test]
    fn assertion_after_typo_rejected() {
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "rebot"
"#;
        let err = LoadedConfig::load_from_str(s).expect_err("typo must fail");
        assert!(
            err.is_unresolvable_after(),
            "is_unresolvable_after; full: {err:?}"
        );
        assert!(
            err.message.contains("boot_smoke"),
            "must name scenario; got: {}",
            err.message
        );
        assert!(
            err.message.contains("rebot"),
            "must surface offending after value; got: {}",
            err.message
        );
        assert!(
            err.message.contains("reboot"),
            "must list legal action label; got: {}",
            err.message
        );
        assert!(
            err.message.contains("any"),
            "must mention 'any' as a universal-legal value; got: {}",
            err.message
        );
    }

    #[test]
    fn assertion_after_references_action_not_in_scenario_rejected() {
        // "ls" is a top-level action label but is NOT in `boot_smoke`'s
        // `actions` Vec — so the assertion `after = "ls"` is unresolvable
        // inside this scenario even though `ls` is otherwise a known label.
        let s = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[action]]
label = "ls"
bytes = "y"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "ls"
"#;
        let err = LoadedConfig::load_from_str(s)
            .expect_err("after=ls must fail; ls is not in this scenario");
        assert!(
            err.is_unresolvable_after(),
            "is_unresolvable_after; full: {err:?}"
        );
        assert!(
            err.message.contains("boot_smoke"),
            "must name scenario; got: {}",
            err.message
        );
        assert!(
            err.message.contains("ls"),
            "must surface offending after value; got: {}",
            err.message
        );
    }

    #[test]
    fn duplicate_toml_action_labels_rejected() {
        let s = r#"
schema_version = 1

[[action]]
label = "dup"
bytes = "a"

[[action]]
label = "dup"
bytes = "b"
"#;
        let err =
            LoadedConfig::load_from_str(s).expect_err("duplicate labels must fail");
        assert!(
            err.message.contains("dup"),
            "must name the duplicate label; got: {}",
            err.message
        );
    }
}
