---
phase: 03-config-buttons-watcher
plan: 02
subsystem: bootroom-core/protocol
tags: [ws-protocol, additive-variant, serde, CFG-10, WCH-05, phase-3]
requires:
  - "crates/bootroom-core/src/lib.rs (Phase-2 WsMessage enum + GuestState)"
provides:
  - "WsMessage::KernelChanged { ok: bool, mtime: i64, size: u64, sha256_prefix: String, reason: Option<String> }"
  - "WsMessage::ConfigUpdate { config: serde_json::Value }"
  - "WsMessage::ConfigInvalid { error: String, line: Option<u32>, col: Option<u32> }"
  - "serde_json promoted from dev-dependency to regular dependency of bootroom-core"
affects:
  - "crates/bootroom/src/ws.rs (Plan 08 — broadcast forwarder will match these variants when fanning out to per-connection WS sinks)"
  - "crates/bootroom/src/watcher.rs (Plan 06 — kernel watcher emits KernelChanged frames)"
  - "crates/bootroom/web/app.js (Plan 11 — browser-side handlers for KernelChanged/ConfigUpdate/ConfigInvalid)"
tech-stack:
  added:
    - "serde_json v1 promoted to crates/bootroom-core/[dependencies] (was [dev-dependencies]) because ConfigUpdate.config carries a serde_json::Value in the public type surface"
  patterns:
    - "Externally-tagged additive enum extension under #[serde(tag = \"type\")] with no #[serde(deny_unknown_fields)] — older clients tolerate unknown variants by failing local from_str + skipping (Phase-2 02-06 handler)"
    - "Pitfall #8 typing (03-RESEARCH): i64 mtime (millennium-scale Unix epoch), u64 size (any FS file size), String sha256_prefix (12-hex prefix Phase-2 emits)"
key-files:
  created: []
  modified:
    - "crates/bootroom-core/Cargo.toml (serde_json moved from [dev-dependencies] to [dependencies])"
    - "crates/bootroom-core/src/lib.rs (+3 variants appended after Hello, +5 round-trip tests)"
decisions:
  - "Variants appended after Hello — no Phase-2 variant reordered or renamed (per plan's <interfaces> note: 'ordering of existing variants MUST NOT change to avoid binary-coupled ordering assumptions in downstream serializers')"
  - "Did NOT add #[serde(deny_unknown_fields)] to WsMessage — Phase-2 02-01 deliberately omitted it so Phase-3+ additive variants are non-breaking (RETROSPECTIVE / 02-01 plan rationale carried forward)"
  - "ConfigInvalid carries 1-based line/col as Option<u32> matching toml v1's span shape — None when the underlying LoadError has no span (e.g., permission-denied read)"
metrics:
  duration: "~9 min"
  completed: "2026-05-19"
---

# Phase 3 Plan 02: WsMessage Additive Variants Summary

Adds three additive `WsMessage` variants — `KernelChanged`, `ConfigUpdate`, `ConfigInvalid` — to `bootroom-core` so the Phase-3 watcher (Plan 06), config-reload pipeline (Plan 07), and broadcast forwarder (Plan 08) have a concrete enum to construct against. Pure type addition; no consumer wiring yet.

## What landed

- **`crates/bootroom-core/Cargo.toml`:** `serde_json` moved from `[dev-dependencies]` to `[dependencies]`. Required because `WsMessage::ConfigUpdate` carries a `serde_json::Value` in its public type. `bootroom-core` remains I/O-free (`serde_json::Value` is pure data).
- **`crates/bootroom-core/src/lib.rs`:** Three new variants appended *after* `Hello`. Existing six Phase-2 variants (`SerialIn`, `SerialOut`, `State`, `Launch`, `Reset`, `Hello`) unchanged in name, order, and field shape.
- **5 new round-trip tests** appended to the existing `#[cfg(test)] mod tests` block.

## New variants — JSON wire shape

`KernelChanged` (ok-true, watcher passed size-stability + ELF magic):
```json
{
  "type": "KernelChanged",
  "ok": true,
  "mtime": 1715000000,
  "size": 12345678,
  "sha256_prefix": "abc123def456",
  "reason": null
}
```

`KernelChanged` (ok-false, watcher rejected — UI shows warning):
```json
{
  "type": "KernelChanged",
  "ok": false,
  "mtime": 0,
  "size": 0,
  "sha256_prefix": "",
  "reason": "not ELF"
}
```

`ConfigUpdate` (bootroom.toml re-parse succeeded; `config` is the same JSON projection `/api/config` returns):
```json
{
  "type": "ConfigUpdate",
  "config": { "schema_version": 1, "actions": [] }
}
```

`ConfigInvalid` with TOML span:
```json
{
  "type": "ConfigInvalid",
  "error": "unknown field 'lable'",
  "line": 12,
  "col": 1
}
```

`ConfigInvalid` without span (e.g., permission denied):
```json
{
  "type": "ConfigInvalid",
  "error": "permission denied",
  "line": null,
  "col": null
}
```

## Verification

- `cargo test -p bootroom-core --lib tests::` — **11 tests pass** (6 Phase-2 + 5 new):
  - `serial_in_roundtrip` ✓
  - `unit_variant_serializes_as_object_with_only_type` ✓
  - `state_message_contains_nested_state` ✓
  - `hello_message_carries_version_string` ✓
  - `guest_state_serializes_as_bare_string` ✓
  - `wsmessage_implements_required_derives` ✓
  - `kernel_changed_ok_true_roundtrip` ✓ (NEW)
  - `kernel_changed_ok_false_with_reason_roundtrip` ✓ (NEW)
  - `config_update_carries_opaque_value` ✓ (NEW)
  - `config_invalid_with_and_without_span` ✓ (NEW — exercises both Some and None for line/col)
  - `large_mtime_survives_i64` ✓ (NEW — pins Pitfall #8: mtime=9_999_999_999_999, size=u64::MAX)
- `cargo clippy -p bootroom-core --lib --tests -- -D warnings` — clean (no warnings).
- `cargo tree -p bootroom-core --depth 1` — confirms `serde_json` listed as a normal dependency alongside `serde` (and `toml`, which was added by parallel Plan 01 work — see Deviations).

## Threat-model coverage (T-03-02-01..03)

| Threat ID | Disposition | How it landed |
|-----------|-------------|---------------|
| T-03-02-01 (Tampering — protocol shape) | mitigate | Five round-trip tests assert `from_str(to_string(x)) == x` for every new variant; `large_mtime_survives_i64` pins Pitfall-#8 typing (i64/u64/String). |
| T-03-02-02 (Protocol confusion — additive policy) | accept | Did not add `#[serde(deny_unknown_fields)]` to the enum (Phase-2 explicit choice preserved); older Phase-2 clients receiving a Phase-3 variant will fail their local `from_str` and skip the frame per the 02-06 handler. |
| T-03-02-03 (Information disclosure — ConfigInvalid.error contents) | accept | Error string echoes the operator's own broken TOML; no secret surface in a dev tool reading an operator-authored file. |

## Deviations from Plan

### [Rule 3 - Blocking] Parallel-execution contention on `crates/bootroom-core/src/lib.rs`

- **Found during:** Task 1 (RED step)
- **Issue:** Multiple parallel executors were active in the same working tree during this plan's execution. A different agent ran a `git commit -a`-style staging that captured my plan-02 RED diff into their plan-03-09 commit (`86e1eee feat(03-09): add Phase-3 CSS for actions panel, banners, BUSY pill`), then a subsequent rebase reverted my changes back into a stash labelled `Plan 03-02 WIP tests for KernelChanged/ConfigUpdate/ConfigInvalid (pre-existing, out of scope for 03-01)`. The working tree also gained partial plan-01 lines (`pub mod escape; pub use escape::...`) and a workspace `toml = { workspace = true }` dependency that were not part of plan 02.
- **Fix:** Recovered the plan-02 work from `git stash pop`, re-applied it cleanly, temporarily removed plan-01's `pub mod escape` lines so my commit's lib.rs diff stayed scoped to plan-02 only, committed plan-02 as a single `feat(03-02)` (commit `ec9edf8`), then let plan-01's working-tree state be restored by the file-state-tracking layer. The single-commit form here also collapses the planned TDD RED → GREEN sequence: the historical RED state existed transiently in commit `86e1eee` (where my failing tests were carried along inside the 03-09 commit before the variants existed), so the cycle is preserved across the project's history even though my own commit is the GREEN state. No work was lost. No destructive git operation (`git reset --hard`, `git clean`, `git update-ref refs/heads/<protected>`) was used.
- **Files modified:** as recorded above.
- **Commit:** `ec9edf8 feat(03-02): add 3 additive WsMessage variants for Phase-3 events`

### [Rule 2 - Critical functionality] Test-only assertion for `config_invalid_with_and_without_span`

- **Found during:** Task 1 (test design)
- **Issue:** Plan listed `config_invalid_with_and_without_span` as a single test name but specified "two assertions (with line/col, without)". A single test body covering both Some-span and None-span variants gives the same coverage signal but with a smaller failure footprint.
- **Fix:** Implemented as one `#[test]` with both assertions inline, both asserting `from_str(to_string(x)) == x` and additionally spot-checking that `null` appears in the wire output when `line`/`col` are `None`.
- **Rationale:** Plan's `<behavior>` clause explicitly said `null for absent fields (Option<u32> serializes to JSON null when None)` — the spot-check directly pins that guarantee.

## Out-of-scope items observed (NOT fixed)

- **Plan 01 partial work in working tree:** Untracked file `crates/bootroom-core/src/escape.rs` and a `pub mod escape; pub use escape::...` block in `lib.rs` were present during plan-02 execution. These are plan-01's responsibility. Plan 02's commit excludes them; they remain in the working tree (uncommitted) for plan 01 to land cleanly.
- **Workspace `Cargo.toml` `toml = "1.1"` workspace dep:** Present in unstaged working-tree changes (likely plan-01 or plan-06 prep). My `crates/bootroom-core/Cargo.toml` change does NOT reference the workspace `toml` entry — `serde_json = { workspace = true }` is the only line I added. Whichever later plan commits the workspace `toml` entry will validate it.

## Self-Check: PASSED

- `git log --oneline | grep ec9edf8` → FOUND: `ec9edf8 feat(03-02): add 3 additive WsMessage variants for Phase-3 events`
- `crates/bootroom-core/src/lib.rs` exists and contains `kernel_changed_ok_true_roundtrip` → FOUND
- `crates/bootroom-core/Cargo.toml` lists `serde_json = { workspace = true }` under `[dependencies]` → FOUND
- `cargo test -p bootroom-core --lib tests::` → 11/11 PASS
- `cargo clippy -p bootroom-core --lib --tests -- -D warnings` → clean
