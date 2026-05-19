---
phase: 04-scenario-engine-headless-run
plan: 06
subsystem: testing
tags: [transcript, jsonl, serde, verbose-formatter, ascii-glyphs, rust]

requires:
  - phase: 04-scenario-engine-headless-run
    provides: WsMessage::ScenarioResult variant (04-01) — TranscriptEvent is the canonical shape for its `transcript` field.
provides:
  - bootroom::transcript module — TranscriptEvent enum (6 variants), TranscriptWriter (atomic-line JSONL), to_jsonl helper.
  - bootroom::verbose module — VerboseFormatter (progress_action / assertion_verdict / final_summary) + non_verbose_failure_line, ASCII-only glyphs.
  - Cross-language contract pin for TranscriptOverflow (browser engine 04-08 emits, Rust side round-trips verbatim).
affects: [04-07, 04-08, 04-10, REP-01]

tech-stack:
  added: []  # No new crates; uses serde / serde_json already in bootroom.
  patterns:
    - "Tagged-JSON enum via #[serde(tag = \"type\")] with stable rename strings as the wire contract."
    - "Atomic-line JSONL via single write_all(serialized + b'\\n') — no partial-line risk for tail-following parsers."
    - "ASCII-only stderr formatter — high-bit byte audit run in CI grep gate (Open Q4)."
    - "Debug-formatted ({:?}) pattern strings in stderr lines — escapes backslashes so CI grep tooling sees JSON-compatible literals."

key-files:
  created:
    - crates/bootroom/src/transcript.rs
    - crates/bootroom/src/verbose.rs
  modified:
    - crates/bootroom/src/lib.rs

key-decisions:
  - "TranscriptEvent enum is decoupled from bootroom_core::config::AssertionKind — `kind` and `verdict` are plain Strings on the wire to keep the JSONL shape stable independent of internal refactors."
  - "non_verbose_failure_line uses a hyphen ' - ' instead of 04-CONTEXT's em-dash, per Open Q4 (cross-platform CI ASCII-only)."
  - "TranscriptOverflow.bytes_truncated_estimate is u64 (browser emits a JS Number; 5 MB fits losslessly in either type)."
  - "Doc-comment em-dashes also scrubbed from verbose.rs so the high-bit byte grep gate audits the entire file, not just string literals."

patterns-established:
  - "Pattern: serde-tagged enum with rename = snake_case as the cross-language wire contract."
  - "Pattern: TDD with RED-phase stub modules so test failures show as compile errors against placeholder shapes — forces the schema to land in one focused GREEN commit."

requirements-completed: [RUN-08, RUN-09]

duration: ~25min
completed: 2026-05-19
---

# Phase 04 Plan 06: Transcript Shape + Verbose Formatter Summary

**JSONL transcript event types (six variants including TranscriptOverflow), atomic-line writer, and ASCII-only verbose stderr formatter — both wired into `bootroom::` and pinned by 19 unit tests.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files created:** 2
- **Files modified:** 1
- **Unit tests added:** 19 (10 transcript + 9 verbose)

## Accomplishments

- `bootroom::transcript` module defines the six canonical event variants — `scenario_start`, `action_send`, `serial_chunk`, `assertion_result`, `scenario_result`, `transcript_overflow` — as a `#[serde(tag = "type")]` enum.
- `TranscriptWriter::write_event` writes one JSON object per line via a single `write_all` call (atomic-line guarantee for tail-following JSONL parsers).
- Cross-language contract pinned: `transcript_overflow_event_deserializes_from_browser_json` deserializes the exact wire shape `web/scenario.js` (04-08) will emit.
- `bootroom::verbose` module emits byte-exact ASCII stderr lines for action progress, per-assertion verdicts, the final scenario summary, and the non-verbose failure line.
- Open Q4 (cross-platform CI) enforced via a perl high-bit-byte audit in the Task 3 grep gate; verbose.rs contains zero bytes >= 0x80.

## Task Commits

1. **Task 1 RED: failing transcript tests** — `5680436` (test)
2. **Task 1 GREEN: TranscriptEvent + TranscriptWriter impl** — `9b6b023` (feat)
3. **Task 2 RED: failing verbose tests** — `0f85dd1` (test)
4. **Task 2 GREEN: VerboseFormatter impl** — `29dc5c9` (feat)
5. **Task 3: scrub em-dash from doc comments** — `ac37b62` (refactor)

## Files Created/Modified

- `crates/bootroom/src/transcript.rs` (created) — `TranscriptEvent` enum (6 variants), `TranscriptWriter<W>` atomic-line writer, `to_jsonl(&[Event]) -> String` helper, 10 unit tests.
- `crates/bootroom/src/verbose.rs` (created) — `VerboseFormatter<W>` with `progress_action` / `assertion_verdict` / `final_summary`, `GLYPH_ACTION` / `GLYPH_PASS` / `GLYPH_FAIL` constants, `non_verbose_failure_line<W>` free function, 9 unit tests.
- `crates/bootroom/src/lib.rs` (modified) — `pub mod transcript;` and `pub mod verbose;` registered alphabetically.

## Exact Wire Shapes Verified

Round-tripped from the implementation (one example per variant):

```jsonl
{"type":"scenario_start","ts":"2026-05-19T14:32:01.123Z","scenario":"boot_smoke","kernel":"/tmp/Image"}
{"type":"action_send","ts":"2026-05-19T14:32:01.123Z","action":"reboot","bytes_b64":"cmVib290DQ=="}
{"type":"serial_chunk","ts":"2026-05-19T14:32:01.123Z","action":"reboot","bytes_b64":"WyAg"}
{"type":"assertion_result","ts":"2026-05-19T14:32:01.123Z","action":"reboot","kind":"contains","pattern":"login: ","verdict":"pass"}
{"type":"scenario_result","ts":"2026-05-19T14:32:01.123Z","verdict":"pass","actions":[{"label":"reboot","verdict":"pass"}]}
{"type":"transcript_overflow","ts":"2026-05-19T14:32:01.123Z","bytes_truncated_estimate":5000000}
```

The `type` discriminator appears first in serde's default field order. Downstream consumers (04-07 `persist_transcript`, 04-08 browser engine, future REP-01 JUnit shim) must rely on the field NAMES, not the order — but the order is stable across serde 1.x.

## Exact Verbose Stderr Lines Verified

Byte-exact unit-test assertions:

```text
> action: reboot
+ assert: contains "login: "
- assert: regex "Booting\\s+"
+ scenario boot_smoke: pass
- scenario boot_smoke: fail
- scenario boot_smoke: timeout
- scenario boot_smoke: error
bootroom run: scenario boot_smoke FAILED - assertion 'login: ' not found after action reboot
```

The regex line shows the on-the-wire bytes: a double backslash (`\\`) because Rust's `Debug` formatter (`{:?}`) escapes the single literal backslash in the input pattern `Booting\s+`. This is deliberate — JS/Python observers reading the line as JSON-like text see the same byte sequence they would see if they had pulled the pattern out of the JSONL transcript.

## Decisions Made

- **kind/verdict as plain `String` on the wire** rather than reusing `bootroom_core::config::AssertionKind`. The JSONL shape is a long-lived contract; internal enum refactors must not break downstream JUnit-shim consumers.
- **u64 for `bytes_truncated_estimate`** even though the JS Number is the actual source. u64 round-trips losslessly for any value <= 2^53, which is 9 petabytes; 5 MB is trivially safe.
- **High-bit byte audit on the whole file**, not just string literals. The Task 3 grep gate runs `perl -ne 'while (/[^\x00-\x7F]/g)'` over `verbose.rs`; this caught two em-dashes in doc comments that would otherwise have passed a literal-only check. Refactor commit `ac37b62` scrubs them.

## Deviations from Plan

None — plan executed exactly as written. The em-dash scrub in commit `ac37b62` is explicitly the work Task 3's grep gate is designed to enforce; the gate fired, I fixed, gate passed. Not a deviation, just the gate doing its job.

## Issues Encountered

- **Task 3 grep gate caught two em-dashes in `verbose.rs` doc comments** on first run. Fixed by replacing `—` with `--` in two comment lines; gate re-ran clean. No code-path change.

## Verification Pass

- `cargo build --workspace` — succeeds (10.57s cold, 36.66s with workspace).
- `cargo test -p bootroom --lib transcript::tests` — **10 passed** (>= 9 required).
- `cargo test -p bootroom --lib verbose::tests` — **9 passed** (>= 8 required).
- `cargo clippy -p bootroom --lib --no-deps --tests -- -D warnings` — clean.
- Task 3 grep gate (six `serde(rename = ...)` checks, three glyph constant checks, bytes_truncated_estimate presence, high-bit byte audit) — **OK**.

## Next Plan Readiness

- 04-07 (run_cmd) can `use bootroom::transcript::{TranscriptEvent, TranscriptWriter};` and `use bootroom::verbose::{VerboseFormatter, non_verbose_failure_line};` without further refactoring.
- `persist_transcript` (04-07 Step 8) will round-trip the browser-built transcript including `transcript_overflow` events losslessly — pinned by `transcript_overflow_event_deserializes_from_browser_json`.
- 04-08 (browser engine) has a stable target for its `web/scenario.js` JSON output shape.
- 04-10 (stderr line tests) has byte-exact reference lines to grep for in its end-to-end runs.

## Self-Check

- `[ -f crates/bootroom/src/transcript.rs ]` — **FOUND**
- `[ -f crates/bootroom/src/verbose.rs ]` — **FOUND**
- `git log | grep -q 5680436` — **FOUND** (transcript RED)
- `git log | grep -q 9b6b023` — **FOUND** (transcript GREEN)
- `git log | grep -q 0f85dd1` — **FOUND** (verbose RED)
- `git log | grep -q 29dc5c9` — **FOUND** (verbose GREEN)
- `git log | grep -q ac37b62` — **FOUND** (Task 3 refactor)

## TDD Gate Compliance

Both modules followed RED → GREEN per plan:

- Transcript: `test(04-06)` at `5680436` (RED, placeholder enum fails to compile against the new variants in the tests), `feat(04-06)` at `9b6b023` (GREEN, 10/10 tests pass).
- Verbose: `test(04-06)` at `0f85dd1` (RED, all 9 tests fail with `Unsupported` from stub methods), `feat(04-06)` at `29dc5c9` (GREEN, 9/9 tests pass).
- Refactor: `refactor(04-06)` at `ac37b62` (Task 3 grep-gate fix; tests still pass after).

**Self-Check: PASSED**

---
*Phase: 04-scenario-engine-headless-run*
*Completed: 2026-05-19*
