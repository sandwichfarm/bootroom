---
phase: 04-scenario-engine-headless-run
plan: 11
subsystem: testing
tags: [e2e, integration-test, ignore-tagged, chromium, qemu-wasm, norn, transcript-jsonl]

# Dependency graph
requires:
  - phase: 04-scenario-engine-headless-run
    provides: "`bootroom run` driver (04-07), browser scenario engine (04-08), `?scenario=` URL wiring (04-09)"
provides:
  - "Phase-4 e2e gate: `cargo test -p bootroom --test run_smoke_norn_kernel -- --ignored` exercises the full headless path against the real NORN kernel fixture"
  - "`crates/bootroom/tests/fixtures/boot_smoke.toml` — committed TOML fixture asserting a verbatim NORN banner observation"
  - "`crates/bootroom/spikes/spike-b/examples/dump_banner.rs` — companion helper that extracts the first ~512 chars of NORN serial output for future banner refresh"
affects: [phase-verification, ci-integration, future-norn-kernel-revisions]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "`#[ignore = \"<reason>\"]`-tagged integration tests for slow/environment-dependent e2e runs; self-skip with clear `[skip]` diagnostic when prerequisites are missing"
    - "Test-level timeout via spawn-in-thread + `mpsc::recv_timeout` — fails the test rather than hanging cargo's outer runner"
    - "Banner lifting: extract verbatim observation via a companion spike helper, lift into TOML fixture, document refresh procedure inline"

key-files:
  created:
    - crates/bootroom/tests/run_smoke_norn_kernel.rs
    - crates/bootroom/tests/fixtures/boot_smoke.toml
    - crates/bootroom/spikes/spike-b/examples/dump_banner.rs
  modified: []

key-decisions:
  - "Banner choice: `[NORN ALLOC] size = 1048576` — 24 ASCII chars, deterministic (compile-time heap size), one of the earliest NORN-emitted lines."
  - "Added a `spike-b/examples/dump_banner.rs` companion helper rather than modifying the canonical spike binary, so the Spike-B verdict artifact stays untouched and future banner refreshes have first-class tooling."
  - "Used `#[ignore = \"...\"]` form (with reason) instead of bare `#[ignore]` to satisfy clippy::ignore_without_reason without per-test `#[allow]`."

patterns-established:
  - "Pattern: e2e gates use the cargo-test runner with `#[ignore]` so they DON'T run by default (no chromium dependency on every commit) but are trivially invoked with `-- --ignored` for phase-verify."
  - "Pattern: fixtures that depend on environmental binaries (NORN kernel image) are gitignored with a `.gitkeep` placeholder; tests self-skip when the binary is absent."

requirements-completed: [RUN-01, RUN-02, RUN-03, RUN-04, RUN-05, RUN-06, RUN-07, RUN-08, RUN-10]

# Metrics
duration: 10min
completed: 2026-05-19
---

# Phase 4 Plan 11: Scenario-Engine Headless Run — e2e gate against NORN Summary

**`#[ignore]`-tagged integration test that drives `bootroom run` against the real Spike-B NORN kernel and asserts exit 0 + `scenario_result.verdict=pass` + no orphan Chromium processes — validating RUN-01..RUN-08 and RUN-10 as a single integrated path.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-19T15:36:07Z
- **Completed:** 2026-05-19T15:46:00Z (approx)
- **Tasks:** 4 (1 verification-only)
- **Files created:** 3

## Accomplishments

- Phase-4 e2e gate lands and PASSES on the dev host: `cargo test -p bootroom --test run_smoke_norn_kernel -- --ignored` exits 0 in ~1.4 s after running the full headless Chromium + qemu-wasm + NORN-kernel boot.
- TOML fixture (`boot_smoke.toml`) asserts on a verbatim NORN banner observation, with inline documentation of how to refresh it after a kernel update.
- `dump_banner` spike example provides first-class tooling for future banner refreshes — no more "invent a banner" anti-pattern.
- All 4 grep gates in Task 4 pass without modification (the doc-comment mention of `#[ignore]` happens to satisfy the literal grep even after the canonical attribute became `#[ignore = "..."]`).

## Task Commits

1. **Task 1: Lift the NORN banner observation** — `8ce87b6` (chore) — added `spike-b/examples/dump_banner.rs` and used it to capture verbatim serial output. Banner chosen: `[NORN ALLOC] size = 1048576`.
2. **Task 2: Write the TOML fixture** — `966179a` (feat) — `tests/fixtures/boot_smoke.toml` with the lifted banner and `bootroom check` verification.
3. **Task 3: Write the `#[ignore]`-tagged e2e test** — `be9467f` (test) — `tests/run_smoke_norn_kernel.rs`. Verified both default-run-skips and `--ignored` passes.
4. **Task 4: Grep gates** — verification-only, no commit. All 5 grep patterns in the plan's `<verify>` block pass against the file as-is.

## Files Created/Modified

- `crates/bootroom/tests/run_smoke_norn_kernel.rs` (created) — the e2e gate. `#[ignore]`-tagged, self-skips when chromium or NORN fixture is absent, spawns `bootroom run` in a worker thread with `mpsc::recv_timeout(90s)`, asserts exit 0 + transcript scenario_start + transcript scenario_result(verdict=pass) + no orphan chromium.
- `crates/bootroom/tests/fixtures/boot_smoke.toml` (created) — TOML fixture: schema_version=1, one no-op `boot` action, one scenario `boot_smoke` with a `contains` assertion against the NORN banner. Refresh procedure documented inline.
- `crates/bootroom/spikes/spike-b/examples/dump_banner.rs` (created) — companion helper that uses the same headless-Chromium path as spike-b but prints the first ~512 chars of NORN serial output (plus a JSON-escaped form) for verbatim fixture lifting.

## Verbatim Observations

### NORN banner observed during dump_banner extraction

The first 1,340 chars of NORN serial output captured during a real run included:

```
[NORN ALLOC] heap = 0x00000000802ba000 .. 0x00000000803ba000
[NORN ALLOC] size = 1048576
[NORN ISA] base=rv64 extensions=i,m,a,f,d,c,h,zicbom,zicboz,zicntr,zicsr,zifencei,zihint
ntl,zihintpause,zihpm,zawrs,zfa,zca,zcd,zba,zbb,zbc,zbs,sstc,svadu
[NORN ISA] all pinned extensions present
[NORN PMP] region count = 16 [sentinel:qemu-virt-default]
[NORN CAP] sizeof Capability=0x0000000000000010 CdtNode=0x0000000000000010 CSpace=0x0000
000000000008 bytes
[NORN CAP] kinds=10 forbidden=3
...
[NORN] Phase 1 boot  hart=0x0000000000000000
[NORN] Hello from S-mode at 0x80200000
[NORN] Phase 1 OK  spawning net + relay + demo-relay-passing (Plan 04-08)
[NORN SCHED] seed 0x0000000000000003 tasks, bootstrapping task 0
[NORN SCHED] preempt loo[NORN RELAY] task started
p active
[NORN TRAP PF] InstructionPageFault scause=0x000000000000000c sepc=0x0000000080263b5c st
val=0x0000000080263b5c
```

`[NORN ALLOC] size = 1048576` was chosen as the assertion pattern because:
- 24 ASCII chars (well above the >=4-char floor specced in the plan).
- Deterministic across builds (the heap size is a compile-time constant in NORN's allocator setup).
- Appears very early in boot (after the heap allocator initializes — well before any of the kernel logic that could vary by build).
- Plain ASCII, no risk of xterm 80-column wrap clobbering it (line is much shorter than 80 chars).

### Final scenario_result event from the e2e run

```json
{"type":"scenario_result","ts":"2026-05-19T15:43:15.934Z","verdict":"pass","actions":[{"assertions":[{"kind":"contains","pattern":"[NORN ALLOC] size = 1048576","verdict":"pass"}],"label":"boot","verdict":"pass"}]}
```

Exit code from `bootroom run`: 0.

Time-to-pass on this host: ~1.2 s (Chromium spawn + qemu-wasm boot + banner emission + assertion + transcript flush). The 90-second test-level timeout has ample headroom.

### Orphan-process check

Post-exit `pgrep -f "chromium.*--headless"` returns empty — RUN-10 holds; the `BrowserGuard` Drop impl from 04-07 is working.

## Decisions Made

1. **Banner choice rationale** — see the `[NORN ALLOC] size = 1048576` block above. The plan explicitly prohibited inventing a banner; running `dump_banner` against the fixture provided real, verbatim ground truth.

2. **dump_banner as `examples/` rather than modifying spike-b/main.rs** — the canonical Spike-B binary's purpose is to compute a Phase-1 verdict (`SPIKE-B-RESULT.md`), not to be a banner-extraction tool. Adding the extraction as an `examples/dump_banner.rs` keeps the spike artifact stable while giving Phase 4+ a first-class refresh path.

3. **`#[ignore = "..."]` instead of bare `#[ignore]`** — clippy::ignore_without_reason is in the project's pedantic lint set. Using the reason form is more informative AND lint-clean. The plan's Task-4 grep `'#\[ignore\]'` still matches because the doc comment at line 3 contains the literal substring.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 — Missing tooling] Added `dump_banner` example to spike-b**
- **Found during:** Task 1 (Lift the NORN banner observation)
- **Issue:** The plan said to either (a) lift the banner from `SPIKE-B-RESULT.md` or (b) re-run Spike B to regenerate it. But Spike B's verdict file stores only a CHARACTER COUNT, not the bytes themselves — so neither (a) nor (b) actually exposes the verbatim banner. Without instrumentation, the task would have been marked BLOCKED.
- **Fix:** Added `crates/bootroom/spikes/spike-b/examples/dump_banner.rs` — a small companion helper that reuses the same bootroom-server + chromiumoxide path but prints the first ~512 chars of `document.querySelector('.xterm-rows').innerText` verbatim (plus a JSON-escaped form for direct fixture lifting). This is permanent infrastructure for future banner refreshes after NORN kernel updates.
- **Files added:** `crates/bootroom/spikes/spike-b/examples/dump_banner.rs`
- **Verification:** Ran `cargo run -p spike-b --example dump_banner -- --kernel <fixture>` and captured 1340 chars of serial output including the chosen banner.
- **Committed in:** `8ce87b6` (Task 1 commit)

**2. [Rule 1 — Lint bug] Switched `#[ignore]` to `#[ignore = "..."]`**
- **Found during:** Task 3 (clippy gate after writing the test)
- **Issue:** Project's pedantic clippy lints include `ignore_without_reason`, which rejects bare `#[ignore]`. Plan's Task 3 code sample uses the bare form.
- **Fix:** Use `#[ignore = "requires /usr/bin/chromium + the NORN kernel fixture; run with --ignored on a configured host"]`. The plan's Task-4 grep gate `'#\[ignore\]'` still passes because the file's module doc comment (line 3) contains the literal substring `\`#[ignore]\`-tagged` (in backticks).
- **Files modified:** `crates/bootroom/tests/run_smoke_norn_kernel.rs` (in same commit as task creation)
- **Verification:** `cargo clippy -p bootroom --tests -- -D warnings` exits 0; Task-4 grep gates all pass.
- **Committed in:** `be9467f` (Task 3 commit)

**3. [Rule 1 — Doc-style] Wrapped `scenario_result_tx` in backticks in module doc comment**
- **Found during:** Task 3 (clippy gate)
- **Issue:** `doc_markdown` lint flagged `scenario_result_tx` as missing backticks.
- **Fix:** `RUN-06 \`scenario_result_tx\` handoff`.
- **Files modified:** `crates/bootroom/tests/run_smoke_norn_kernel.rs` (same commit)
- **Committed in:** `be9467f` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 missing tooling, 2 lint fixes).
**Impact on plan:** All deviations necessary to make the gate land on a clippy-clean codebase. Deviation #1 is the most consequential — it converts "BLOCKED, surface to operator" into "PASSED with permanent banner-refresh tooling for future plans."

## Issues Encountered

- Initial worktree branch was behind master and didn't have prerequisite Phase-4 plans' code. Resolved by `git merge master --no-edit` at the start of execution.
- The Spike-B verdict file documents the test by character count rather than byte content, so a direct lift wasn't possible — see deviation #1.

## User Setup Required

None — the test is `#[ignore]`-tagged by design. It runs only when an operator (or the GSD phase-verify gate) passes `-- --ignored` explicitly on a host with `/usr/bin/chromium` installed and the NORN kernel fixture present at `crates/bootroom/spikes/spike-b/fixtures/Image`. Both prerequisites self-check with a clear `[skip]` diagnostic if missing.

## Next Phase Readiness

- Phase 4 is functionally complete. RUN-01..RUN-08 and RUN-10 all hold against the real NORN kernel. RUN-09 (CI integration) is the canonical follow-on if/when that gets scoped.
- The `dump_banner` helper is reusable for any future kernel target — not NORN-specific.
- The `tests/fixtures/` directory now exists; future fixture-driven integration tests should follow the same layout.

## Self-Check: PASSED

- `crates/bootroom/tests/run_smoke_norn_kernel.rs` — FOUND.
- `crates/bootroom/tests/fixtures/boot_smoke.toml` — FOUND.
- `crates/bootroom/spikes/spike-b/examples/dump_banner.rs` — FOUND.
- Commit `8ce87b6` — FOUND.
- Commit `966179a` — FOUND.
- Commit `be9467f` — FOUND.
- Default `cargo test -p bootroom --test run_smoke_norn_kernel` — 1 ignored, 0 failed.
- `cargo test -p bootroom --test run_smoke_norn_kernel -- --ignored` — 1 passed in 1.4 s.
- `cargo clippy -p bootroom --all-targets -- -D warnings` — clean.

---
*Phase: 04-scenario-engine-headless-run*
*Completed: 2026-05-19*
