---
phase: 03-config-buttons-watcher
plan: 01
subsystem: config
tags: [toml, serde, bootroom-core, schema, escape-decoder, cli-overrides]

# Dependency graph
requires:
  - phase: 02-websocket-live-serial
    provides: bootroom-core::WsMessage + GuestState (unchanged surface; 03-01 lands alongside, not on top)
provides:
  - bootroom-core::config module (Config, Action, Scenario, Assertion, AssertionKind)
  - bootroom-core::config::LoadedConfig (validated, byte-decoded, override-merged view)
  - bootroom-core::config::ResolvedAction (post-decode value type for /api/config projection)
  - bootroom-core::config::CliAction (--action flag input shape)
  - bootroom-core::config::LoadError (typed error w/ optional 1-based line+col)
  - bootroom-core::config::parse_str + offset_to_line_col (Unicode-scalar columns)
  - bootroom-core::escape module (decode_bytes_escape + EscapeError)
  - Workspace dependency declarations for toml, notify, notify-debouncer-full
affects:
  - 03-03 (CLI parser will consume CliAction)
  - 03-04 (bootroom check + bootroom init reuse LoadedConfig::load_from_str)
  - 03-05 (AppState carries LoadedConfig)
  - 03-06 (watcher reuses LoadedConfig for hot reload)
  - 03-07 (/api/config projects ResolvedAction list)
  - 04-* (Phase 4 scenario engine consumes Scenario + Assertion as-is)

# Tech tracking
tech-stack:
  added:
    - "toml = 1.1 (workspace dep) — winnow-backed TOML 1.0 parser w/ span-aware errors"
    - "notify = 8 + notify-debouncer-full = 0.7 (workspace dep declarations; not yet consumed by any crate — Plan 03-06 wires the watcher in crates/bootroom)"
  patterns:
    - "Schema-first parser: one #[serde(deny_unknown_fields)] type tree owned by bootroom-core; every downstream consumer projects from it (Pitfall #5 mitigation)."
    - "Byte-escape decoding centralized in bootroom-core::escape so the TOML config and the --action CLI flag share one decoder (Pitfall #5)."
    - "LoadError carries optional (line, col) so the watcher + /api/config can surface a precise underline; toml::de::Error::span() -> offset_to_line_col() with chars().count() not bytes()."
    - "CLI override merge = dedupe-replace by label (existing TOML entry kept at its index; new label appended; last --action wins among CLI-only collisions)."

key-files:
  created:
    - crates/bootroom-core/src/escape.rs
    - crates/bootroom-core/src/config.rs
    - .planning/phases/03-config-buttons-watcher/03-01-SUMMARY.md
  modified:
    - crates/bootroom-core/src/lib.rs (added pub mod + re-exports for config + escape)
    - crates/bootroom-core/Cargo.toml (added toml runtime dep)
    - Cargo.toml (added notify, notify-debouncer-full, toml workspace deps — already in HEAD before this plan executed)
    - crates/bootroom/src/ws.rs (handle_wire match arm extended to cover the 3 new server-owned WsMessage variants that 03-02 added in parallel)

key-decisions:
  - "AssertionKind serde lowercase: TOML reads kind = \"contains\" | \"regex\" — friendlier than UpperCamelCase in operator-authored files."
  - "LoadError is a struct with a private LoadErrorKind enum + public predicates (is_schema_version_mismatch + actual_version) so the public surface is small and additive."
  - "decode_bytes_escape's unknown-escape variant accepts non-ASCII follow-up bytes by mapping them to '?' (rather than crashing on `as char` for >=0x80) — keeps the parser panic-free on hostile input."
  - "CLI override semantics: replace-in-place by label clears group + description (operator's ad-hoc action has no UI metadata)."
  - "VALID_TOML test fixture uses TOML literal string ('reboot\\r') not basic string (\"reboot\\r\") — TOML's own escape pass would otherwise consume the backslash and confuse the unit test."

patterns-established:
  - "Module layout: bootroom-core/{escape.rs, config.rs}; lib.rs is now a thin re-export hub. New shared parsers should land as siblings here."
  - "Test fixtures inline as `const VALID_TOML: &str` to keep deterministic-no-I/O contract intact."

requirements-completed: [CFG-02, CFG-03, CFG-04, CFG-05, CFG-06, ACT-03]

# Metrics
duration: ~25min
completed: 2026-05-19
---

# Phase 3 Plan 1: bootroom-core config + escape modules + workspace dep declarations Summary

**One shared TOML schema (Config/Action/Scenario/Assertion) + one shared byte-escape decoder land in `bootroom-core`, with deny_unknown_fields span-aware errors and CLI override merge semantics — every downstream Phase-3 consumer now projects from a single parser.**

## Performance

- **Duration:** ~25 minutes
- **Tasks:** 3 (Task 1 workspace deps was already in HEAD via a prior commit — only verified)
- **Files created:** 3 (`escape.rs`, `config.rs`, `03-01-SUMMARY.md`)
- **Files modified:** 3 (`lib.rs`, `Cargo.toml` for bootroom-core, `ws.rs` for the variant-coverage fix)
- **Tests passing:** 34 in `cargo test -p bootroom-core --lib` (was 6 at plan start)
- **Clippy:** clean under `-D warnings` for `--lib --tests` on bootroom-core and the whole workspace

## Accomplishments

- `bootroom-core::escape` ships `decode_bytes_escape` + `EscapeError` with 11 unit tests covering `\r \n \t \0 \\ \xNN` plus typed errors with byte offsets for `TrailingBackslash`, `UnknownEscape`, `ShortHex`, `BadHex`.
- `bootroom-core::config` ships the full Phase-3 schema (Config / Action / Scenario / Assertion / AssertionKind) with `#[serde(deny_unknown_fields)]` on every struct, plus the validated `LoadedConfig` + `ResolvedAction` projection and CLI-override merge semantics.
- 11 named unit tests cover CFG-02..06 + ACT-03 (override merge half) + CFG-09 prerequisite (insertion-order preservation) + the Unicode-column `offset_to_line_col` detail + source-TOML duplicate-label rejection.
- `LoadError` exposes typed predicates (`is_schema_version_mismatch`, `actual_version`) so callers don't have to string-match on the message.
- Pitfall #5 (TOML schema drift) + #8 (silent unknown fields) structurally mitigated — one parser shared by `/api/config`, `bootroom check`, the watcher, and Phase 4.

## Task Commits

Each task was committed atomically (Task 1 was already in HEAD via commit 06b9253 from a prior 03-09 session that opportunistically declared the Phase-3 workspace deps when adding Phase-3 CSS):

1. **Task 1: Workspace dependency declarations (toml + notify + notify-debouncer-full)** — already in HEAD (`06b9253`); verified the three lines are present in `[workspace.dependencies]` and that `cargo metadata` + `cargo tree -p bootroom-core` show `toml v1.1.x` resolved correctly. `crates/bootroom-core/Cargo.toml` also already had `toml.workspace = true`.
2. **Task 2: `decode_bytes_escape` + `EscapeError`** — `ba8b78f` (feat)
3. **Task 3: `Config` / `LoadedConfig` + `parse_str` + override merge** — `47b7d90` (feat). Also extends `crates/bootroom/src/ws.rs` `handle_wire` match arm to cover the three new server-owned `WsMessage` variants (`KernelChanged`, `ConfigUpdate`, `ConfigInvalid`) that landed in parallel via plan 03-02.

**Plan metadata:** this SUMMARY.md (separate commit).

## Files Created/Modified

- `crates/bootroom-core/src/escape.rs` — `decode_bytes_escape` byte-walker + `EscapeError` enum with byte-position carriers. 11 inline tests.
- `crates/bootroom-core/src/config.rs` — TOML schema types + `LoadedConfig` + `LoadError` + `parse_str` + `offset_to_line_col` + `CliAction` + `ResolvedAction`. 11 inline tests.
- `crates/bootroom-core/src/lib.rs` — added `pub mod escape;` and `pub mod config;` plus a flat re-export hub for the public symbols.
- `crates/bootroom-core/Cargo.toml` — `toml.workspace = true` runtime dep (was already in this state from a prior commit due to the parallel 03-02 landing serde_json into `[dependencies]`).
- `crates/bootroom/src/ws.rs` — extended the `handle_wire` non-exhaustive-match arm to include `KernelChanged | ConfigUpdate | ConfigInvalid` alongside `State | Hello`. Same warn-and-continue posture (these are server-owned variants; a client sending them is a protocol error).

## Decisions Made

- **Module split:** `escape.rs` separated from `config.rs` because the `--action` CLI flag (Plan 03-03) needs `decode_bytes_escape` without dragging in the TOML schema.
- **TOML literal string in the test fixture:** `bytes = 'reboot\r'` rather than `bytes = "reboot\r"` so the backslash survives TOML's own escape pass and the `decode_bytes_escape` call gets the same `\r` two-character sequence an operator would actually type into a basic-string-quoted config field. (TOML's basic-string `\r` is interpreted by the TOML parser as a literal CR byte; downstream code wouldn't see the backslash.)
- **`LoadError` shape:** struct + private kind enum + public predicates (vs an exposed enum) so we can add error variants additively without breaking match-exhaustive callers.
- **CLI override clears UI metadata:** when a `--action label=X` shadows an existing TOML action, group/description are dropped — operators ad-hoc actions don't have UI metadata to inherit. Tested in `cli_override_replaces_existing_action_bytes`.
- **Workspace deps not yet consumed by any crate:** `notify` and `notify-debouncer-full` are declared in `[workspace.dependencies]` but no crate references them. They land here so Plan 03-06 can simply add `notify = { workspace = true }` to `crates/bootroom/Cargo.toml`. `cargo tree --workspace --depth 1` accordingly does NOT show them — this matches the plan's stated intent ("bootroom-core stays I/O-free").

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Pre-existing WIP tests in `lib.rs` referencing 03-02 variants kept the verification step from compiling**

- **Found during:** Task 2 verification.
- **Issue:** `crates/bootroom-core/src/lib.rs` had unstaged tests for `WsMessage::KernelChanged`, `ConfigUpdate`, `ConfigInvalid` — variants that didn't exist yet, blocking `cargo test -p bootroom-core --lib` with E0599.
- **Fix:** While I was investigating, the user/automation in parallel committed plan 03-02 (commits `ec9edf8` + `e927fd4`), which added the three new variants AND their tests properly. The blocker self-resolved.
- **Files modified:** none by me.
- **Verification:** `cargo test -p bootroom-core --lib` shows the three 03-02 tests plus 03-01's 22 new tests, all green.

**2. [Rule 3 - Blocking] `handle_wire` in `crates/bootroom/src/ws.rs` did not cover the 3 new `WsMessage` variants**

- **Found during:** Task 3 verification (`cargo build --workspace`).
- **Issue:** Plan 03-02 added `KernelChanged`, `ConfigUpdate`, `ConfigInvalid` to `WsMessage` but didn't extend the `handle_wire` match in `crates/bootroom/src/ws.rs`; the build broke with E0004 (non-exhaustive patterns) the moment my own changes touched `lib.rs`. This is a pre-existing latent error in 03-02 that surfaced once any other build of `bootroom` was triggered.
- **Fix:** Added the three new variants to the existing `State | Hello => warn` arm — same posture (these are server-owned variants, so a client sending them is a protocol error: log and continue, do not disconnect).
- **Files modified:** `crates/bootroom/src/ws.rs`.
- **Verification:** `cargo build --workspace` clean; `cargo clippy --workspace --lib --tests -- -D warnings` clean.
- **Committed in:** `47b7d90` (folded into Task 3 commit).

**3. [Rule 1 - Bug] Initial test assertion compared `cfg.actions[0].bytes` against a Rust-side escape that conflicted with TOML's own escape pass**

- **Found during:** Task 3 verification (`cargo test -p bootroom-core --lib`).
- **Issue:** The first iteration of `actions_roundtrip` asserted `a.bytes == "reboot\\r"` against a TOML basic string `bytes = "reboot\r"`, which TOML parses to a 7-character string with an actual CR byte — so the assertion was reading 7 bytes against an 8-character Rust literal.
- **Fix:** Switched the test fixture to a TOML literal string `bytes = 'reboot\r'`. The intent — "bytes_decoded sees the backslash sequence and turns it into a CR byte" — is preserved and now actually tested.
- **Files modified:** `crates/bootroom-core/src/config.rs`.
- **Verification:** `actions_roundtrip` green.
- **Committed in:** `47b7d90` (folded into Task 3 commit).

**4. [Rule 1 - Clippy] Four `clippy::pedantic` lints in `config.rs`**

- **Found during:** Task 3 verification (`cargo clippy ... -D warnings`).
- **Issue:** `needless_pass_by_value` (×3) on `LoadError::duplicate_action(label: String)` and `LoadError::unknown_action_ref(scenario: String, action: String)`, plus one `assigning_clones` on `existing.bytes_decoded = c.bytes.clone();`.
- **Fix:** Took `&str` for the constructor args (still `format!`-friendly), and used `.clone_from(&c.bytes)` for the in-place override write.
- **Files modified:** `crates/bootroom-core/src/config.rs`.
- **Verification:** `cargo clippy --workspace --lib --tests -- -D warnings` clean.
- **Committed in:** `47b7d90` (folded into Task 3 commit).

---

**Total deviations:** 4 auto-fixed (1 self-resolved by parallel work, 3 from the executor: 1 cross-plan blocker fix, 1 test-fixture bug, 1 clippy cleanup).
**Impact on plan:** No scope creep. All four are correctness/lint fixes folded into the task they were discovered in. The cross-plan `handle_wire` fix is a one-line additive change to an existing match arm — it does not change protocol semantics.

## Issues Encountered

- Plan 03-01's stated verification command `cargo tree --workspace --depth 1 | grep -E 'notify v8|notify-debouncer-full v0.7'` returns no matches because the `notify`/`notify-debouncer-full` workspace declarations are not yet consumed by any crate. This matches the plan's own intent ("bootroom-core stays I/O-free") and is documented in this SUMMARY's Decisions section.

## User Setup Required

None — bootroom-core remains I/O-free, no external services or env vars touched.

## Next Phase Readiness

- Plan 03-03 (CLI refactor: `Cmd::{Serve,Check,Init}` + `--config` + `--action`) can import `CliAction` and `decode_bytes_escape` directly from `bootroom_core`.
- Plan 03-04 (`bootroom check` + `bootroom init`) reuses `LoadedConfig::load_from_str` — error messages already render with `(line N, col M)` suffix via the `Display` impl on `LoadError`.
- Plan 03-05 (AppState extension) can stash `LoadedConfig` and project to JSON via the soon-to-be-defined `to_api_json` helper (Plan 03-07).
- Plan 03-06 (watcher) can pull `notify` + `notify-debouncer-full` from `[workspace.dependencies]` directly via `crates/bootroom/Cargo.toml`.

## Self-Check

Created files exist:
- `crates/bootroom-core/src/escape.rs` — FOUND
- `crates/bootroom-core/src/config.rs` — FOUND
- `.planning/phases/03-config-buttons-watcher/03-01-SUMMARY.md` — FOUND (this file)

Commits exist (verified via `git log --oneline`):
- `ba8b78f` — FOUND (Task 2: escape module)
- `47b7d90` — FOUND (Task 3: config module + ws.rs match-arm fix)
- Task 1 was already in HEAD via `06b9253` — FOUND

`cargo test -p bootroom-core --lib`: 34 passed, 0 failed.
`cargo clippy --workspace --lib --tests -- -D warnings`: clean.

## Self-Check: PASSED

---
*Phase: 03-config-buttons-watcher*
*Completed: 2026-05-19*
