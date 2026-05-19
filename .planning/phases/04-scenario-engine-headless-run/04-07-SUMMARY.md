---
phase: 04-scenario-engine-headless-run
plan: 07
subsystem: testing
tags: [chromiumoxide, headless-chrome, axum, oneshot, cdp, rfc3339, exit-codes]

requires:
  - phase: 04-scenario-engine-headless-run
    provides: |
      RunArgs / Cmd::Run surface (04-03 stub),
      AppState scenario_result_tx + install_scenario_oneshot (04-04),
      ws.rs handoff of WsMessage::ScenarioResult (04-05),
      TranscriptEvent + TranscriptWriter + VerboseFormatter (04-06),
      regex compile-check at config load (04-02),
      browser-side scenario engine (04-08),
      ?scenario= URL query wiring (04-09)
provides:
  - "bootroom run --kernel ... --scenario ... drives headless Chromium against an in-process axum server, awaits the ScenarioResult oneshot, and exits 0/1/2/3"
  - "Inline RFC 3339 / Z UTC timestamp helper (no time/chrono dep) — Howard Hinnant civil_from_days over std::time::SystemTime"
  - "Chromium discovery probe ($CHROMIUM → /usr/bin/chromium → which chromium) verified via --version (Pitfall #6)"
  - "COI / SharedArrayBuffer self-check via CDP Runtime.evaluate before the scenario kicks off (RUN-10)"
  - "Explicit Spike-B cleanup sequence at every post-launch exit path — no BrowserGuard, no browser.clone() (chromiumoxide::Browser is not Clone)"
affects: [04-10, 04-11, 06]

tech-stack:
  added: [chromiumoxide-0.9.1, futures-0.3, regex-1]
  patterns:
    - "Inner async block captures result, unconditional cleanup runs, then result is consumed via ?"
    - "Subprocess-discovery diagnostic that lists every candidate tried with its failure mode"
    - "Inline RFC 3339 formatter via Howard Hinnant civil_from_days — keeps the dep tree minimal"

key-files:
  created: []
  modified:
    - "Cargo.toml — chromiumoxide + futures added to [workspace.dependencies]"
    - "crates/bootroom/Cargo.toml — chromiumoxide + futures + regex added to [dependencies]"
    - "crates/bootroom/src/run_cmd.rs — full headless driver replacing the 04-03 stub"

key-decisions:
  - "Explicit Spike-B cleanup sequence (close → wait → handler.abort → server.abort) — no Drop wrapper; chromiumoxide::Browser is not Clone"
  - "No time / chrono dep added — RFC 3339 helper is ~30 LoC over std::time::SystemTime"
  - "server::run preflight NOT extracted into a shared helper — duplication is small, callers are serve-specific (host/port/no_open) vs run-specific (scenario/log-file), clarity beats DRY for two callers"
  - "discover_chromium refactored to accept a slice of candidates so tests can exercise the all-missing diagnostic without depending on /usr/bin/chromium presence on the developer's box"
  - "Howard Hinnant civil_from_days short names (doe, doy, mp, yoe) retained under #[allow(clippy::similar_names)] — renaming them obscures auditability against the published reference"

patterns-established:
  - "Capture-then-cleanup: post-launch errors flow through an inner async block whose Result is bound to a local; the cleanup sequence runs unconditionally; the local is consumed AFTER cleanup. Avoids Drop guards and Browser.clone() in a non-Clone API"
  - "Three-step subprocess discovery with per-candidate --version probe and aggregate diagnostic listing every candidate tried"
  - "Inline date formatters via Howard Hinnant civil_from_days for projects with a strict minimal-deps stance"

requirements-completed: [RUN-01, RUN-02, RUN-03, RUN-06, RUN-10]

duration: 8min
completed: 2026-05-19
---

# Phase 04 Plan 07: Headless Run Driver — Composition Summary

**`bootroom run --kernel … --scenario …` now drives headless Chromium against an in-process axum, COI-checks the page, awaits the ScenarioResult oneshot, persists a JSONL transcript, and exits 0/1/2/3 — replacing the Plan 04-03 stub.**

## Performance

- **Duration:** ~8 min (wall, including build warmup for chromiumoxide cold compile)
- **Started:** 2026-05-19T15:25:37Z
- **Completed:** 2026-05-19T15:32:34Z
- **Tasks:** 4 (1 dep promotion, 1 driver implement, 1 verify-only, 1 grep-gates)
- **Files modified:** 3

## Accomplishments

- Replaced the Plan 04-03 stub in `crates/bootroom/src/run_cmd.rs` with the full driver: config + scenario validation, listener bind, oneshot install, Chromium discovery + launch, navigation to `?scenario=<name>`, COI self-check, outer-timeout await, transcript persistence, verbose/non-verbose stderr, exit-code translation.
- Wired `chromiumoxide = 0.9.1` (no default features), `futures = 0.3`, and `regex = 1` into `crates/bootroom`. No `time` / `chrono` was added — the RFC 3339 helper is inline (~30 LoC, Howard Hinnant civil_from_days over `std::time::SystemTime`).
- Verified end-to-end on a host with `/usr/bin/chromium`: empty `boot_smoke` scenario exits 0, writes a 2-line JSONL transcript (`scenario_start` + `scenario_result`), and emits `+ scenario boot_smoke: pass` under `--verbose`.

## Task Commits

1. **Task 1: Promote chromiumoxide + futures + regex to bootroom deps** — `3d8a4ee` (chore)
2. **Task 2: Implement run_cmd::run end-to-end** — `609969e` (feat)
   - **Follow-up clippy clean** — `ff1984d` (refactor)
3. **Task 3: Refactor server::run to share preflight** — _no commit; decision rule applied (see Deviations)_
4. **Task 4: Grep gates** — _no commit; validates Tasks 1+2; all 12 gates emit OK_

## Files Created/Modified

- `Cargo.toml` — added `chromiumoxide = { version = "0.9.1", default-features = false }` and `futures = "0.3"` to `[workspace.dependencies]`.
- `crates/bootroom/Cargo.toml` — added `chromiumoxide`, `futures`, `regex` to `[dependencies]`.
- `crates/bootroom/src/run_cmd.rs` — full driver replacing the 04-03 stub. 575 LoC including 8 unit tests.

## Verification

### Exit-code observations (real binary)

| Invocation                                             | Expected | Observed |
| ------------------------------------------------------ | -------- | -------- |
| `run --kernel /nonexistent --config <good> --scenario boot_smoke` | 2        | **2**    |
| `run --kernel <ok> --config <good> --scenario fake`    | 2        | **2**    |
| `run --kernel <ok> --config /nonexistent.toml --scenario boot_smoke` | 2 | **2** |
| `run --kernel <ok> --config <good> --scenario boot_smoke` (empty scenario, real chromium) | 0 | **0** |

The `$CHROMIUM=/nonexistent → exit 3` branch is exercised by the unit test `discover_chromium_returns_error_when_all_missing`. A subprocess test cannot reliably reproduce the all-missing path on this developer box because `/usr/bin/chromium` is present (the discover walks past `$CHROMIUM` to candidate #2 and succeeds). The unit test pins the diagnostic shape (all three candidate labels appear; `Set $CHROMIUM` hint present).

### Tests

- `cargo test -p bootroom --lib run_cmd::tests` — 8/8 pass
  - `verdict_pass_yields_exit_zero`
  - `verdict_fail_yields_exit_one`
  - `discover_chromium_returns_error_when_all_missing`
  - `coi_self_check_diagnostic_mentions_headers`
  - `utc_now_iso8601_z_format_pin`
  - `format_iso8601_z_epoch`
  - `format_iso8601_z_known_date`
  - `format_iso8601_z_millis_zero_padded`
- `cargo test -p bootroom --tests` — all integration tests pass (`serve_binds`, `serve_no_open`, `ws_roundtrip`, `ws_broadcast_fanout`, `ws_scenario_result_handoff`, `coop_coep_headers`, etc.).
- `cargo test --workspace` — all workspace tests pass.
- `cargo build --workspace` — succeeds.
- `cargo clippy -p bootroom --all-targets -- -D warnings` — clean.

### Grep gates (Task 4)

All 12 gates emit `OK`:

| #   | Type     | Pattern                                                   | Observed |
| --- | -------- | --------------------------------------------------------- | -------- |
| 1   | positive | `chromiumoxide` in `crates/bootroom/Cargo.toml`           | ✓        |
| 2   | positive | `new_headless_mode` + `disable-dev-shm-usage` in run_cmd  | ✓        |
| 3   | positive | `arg("--version")` (Pitfall #6 candidate verification)    | ✓        |
| 4   | positive | `self.crossOriginIsolated` (COI self-check JS)            | ✓        |
| 5   | positive | `?scenario=`                                              | ✓        |
| 6   | positive | `build_router(state` (same router as serve)               | ✓        |
| 7   | positive | `ExitCode::from(2)` + `ExitCode::from(3)`                 | ✓        |
| 8   | positive | `timeout_ms + 30_000` (Pitfall #8 outer-timeout formula)  | ✓        |
| 9   | positive | All four Spike-B cleanup lines                            | ✓        |
| 10  | negative | NO `BrowserGuard`, NO `browser.clone()`                   | ✓        |
| 11  | negative | NO `time =` / `chrono =` dep in either Cargo.toml         | ✓        |
| 12  | positive | `utc_now_iso8601_z` / `format_iso8601_z` + `SystemTime`   | ✓        |

### Cleanup-sequence parity with Spike B

The four-line teardown is byte-identical to `crates/bootroom/spikes/spike-b/src/main.rs:240-243`:

```rust
let _ = browser.close().await;
let _ = browser.wait().await;
handler_task.abort();
server_task.abort();
```

No `BrowserGuard` Drop wrapper, no `browser.clone()` (chromiumoxide::Browser is not Clone), no `Arc<Mutex<Option<Browser>>>` — all forbidden by Task 4 negative gates.

## Decisions Made

- **Explicit cleanup, not RAII.** Earlier plan drafts proposed a `BrowserGuard` Drop wrapper holding `browser.clone()`. That would not compile (Browser is not Clone). Lifted Spike B's verified shape verbatim: capture inner-async-block result into a local, run the four-line cleanup unconditionally, then consume the local with `?`.
- **No `time` / `chrono` dep.** The RFC 3339 helper is ~30 LoC over `std::time::SystemTime` + Howard Hinnant civil_from_days. Matches the project's "single static binary, minimal deps" stance documented in CLAUDE.md. Bytes-compatible with JS `new Date().toISOString()` so server-emitted timestamps interleave cleanly with browser-emitted ones in the JSONL transcript.
- **server::run preflight NOT factored out (Task 3).** The duplication is small and the two callers have substantially different surface (serve owns host/port/no_open/actions/assets_dir; run owns scenario/log-file/verbose). Per the plan's Task 3 decision rule, clarity beats DRY for two callers. Phase 6 may revisit.
- **`discover_chromium_with_candidates` extracted for testability.** Real `discover_chromium` builds the three-candidate list from env / hardcoded path / `which`, then delegates to the slice-taking helper. Tests pass synthetic candidates so the all-missing branch is exercised without depending on `/usr/bin/chromium` being absent.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — clippy pedantic blocking strict CI] Clippy `-D warnings` failures in `format_iso8601_z`**
- **Found during:** Task 2 post-implementation, when running `cargo clippy -p bootroom --all-targets -- -D warnings` to confirm the project's "CI fails on clippy -D warnings" stance from CLAUDE.md.
- **Issue:** Five pedantic-level lints fired on `format_iso8601_z`: `boolean_to_int_using_if` on `verdict_to_exit`, `cast_possible_wrap` on `secs_total / 86_400` u64→i64, `cast_possible_truncation` on `rem_euclid(146_097)` i64→u32, `cast_lossless` on `yoe as i64`, and `similar_names` on `doe` vs `doy`.
- **Fix:** Replaced casts with bounded `try_from` + `unwrap_or` defaults, swapped `if/else 0/1` for `u8::from(bool)`, renamed `d` to `day_of_month` (the easy disambiguation), and added a function-scope `#[allow(clippy::similar_names)]` for `doe`/`doy`/`mp`/`yoe` because those are Howard Hinnant's canonical short names — renaming them would obscure auditability against the published reference algorithm.
- **Files modified:** `crates/bootroom/src/run_cmd.rs`
- **Verification:** `cargo clippy -p bootroom --all-targets -- -D warnings` is clean; all 8 unit tests still pass; all 12 grep gates still pass.
- **Committed in:** `ff1984d` (refactor).

**2. [Rule 3 — blocking discovery test] `discover_chromium` refactored to accept candidate slice**
- **Found during:** Task 2 unit-test authoring.
- **Issue:** The plan's `discover_chromium_returns_error_when_all_missing` test cannot exercise the all-missing diagnostic on a developer box where `/usr/bin/chromium` exists — the function would walk past `$CHROMIUM=` and find the real binary.
- **Fix:** Extracted `discover_chromium_with_candidates(&[(label, path)])` so tests pass synthetic candidates. The public `discover_chromium` builds the canonical three-candidate list and delegates. This matches the plan's Step 10 spike note: "make discover_chromium accept a slice of candidates for tests".
- **Files modified:** `crates/bootroom/src/run_cmd.rs`
- **Verification:** Unit test passes; integration behavior unchanged (production path still uses the canonical three-candidate list).
- **Committed in:** `609969e` (part of the Task 2 feat commit).

---

**Total deviations:** 2 auto-fixed (1 × Rule 1 clippy hygiene, 1 × Rule 3 testability refactor).
**Impact on plan:** Both deviations are within the plan's spirit — Rule 1 unblocks the project's strict-clippy CI posture explicitly stated in CLAUDE.md; Rule 3 implements a test-refactor the plan itself flagged in Task 2 Step 10. No scope creep, no architectural change.

## Issues Encountered

- **Worktree was created from an older master HEAD.** On startup, the agent's branch was at `1a224cf` (before the Phase 4 prerequisites had merged into master). Resolved by fast-forward merging master into the worktree branch — a safe operation that brought in 04-01..04-09 cleanly. No conflict; no per-prereq cherry-pick needed.

## User Setup Required

None — chromium is already present at `/usr/bin/chromium` on this developer box. End users will need a chromium-compatible binary discoverable via `$CHROMIUM`, `/usr/bin/chromium`, or `which chromium` to run `bootroom run` against a real scenario.

## Threat Flags

None — no new security-relevant surface beyond what the plan's threat register already covers. The driver opens a loopback-only listener (matches `serve` mode's posture); Chromium runs with `--no-sandbox` + `--disable-dev-shm-usage` lifted verbatim from Spike B and constrained to localhost.

## Next Phase Readiness

- **04-10 subprocess tests** (`run_log_file_jsonl`, `run_verbose_stderr`, `run_uses_same_router`, `run_subcommand_exit_codes`) can now be authored against the real driver.
- **04-11 NORN fixture test** (`#[ignore]`-tagged) can now be authored; the end-to-end `bootroom run … boot_smoke` path is proven to exit 0 against a real `/usr/bin/chromium`.
- **Phase 6 polish opportunities:** dedupe spike-b's `chromiumoxide` declaration against the new workspace dep; revisit `server::run` preflight extraction if a third subcommand needs to share it.

---

## Self-Check: PASSED

- `crates/bootroom/src/run_cmd.rs` — FOUND
- `crates/bootroom/Cargo.toml` — FOUND
- `Cargo.toml` — FOUND
- Commit `3d8a4ee` (Task 1 dep promotion) — FOUND in `git log`
- Commit `609969e` (Task 2 driver implementation) — FOUND in `git log`
- Commit `ff1984d` (Task 2 clippy clean) — FOUND in `git log`
- All 8 `run_cmd::tests` unit tests — pass
- All 12 grep gates — pass
- `cargo build --workspace` — pass
- `cargo clippy -p bootroom --all-targets -- -D warnings` — clean
- End-to-end `bootroom run … boot_smoke` against real Chromium — exit 0

---
*Phase: 04-scenario-engine-headless-run*
*Plan: 07*
*Completed: 2026-05-19*
