---
phase: "03"
verified: 2026-05-19T10:23:49Z
status: human_needed
verified_count: 15
total_count: 19
score: "15/19 must-haves verified by code+tests; 4 require headed-browser smoke"
re_verification: false
human_verification:
  - test: "ACT-02 — button click writes bytes to guest serial"
    expected: "Click a TOML-defined action button (e.g. 'reboot'); the configured byte sequence appears in the xterm and the guest responds (e.g. shell echoes 'reboot')."
    why_human: "Requires real qemu-wasm guest running in a browser; qemu-wasm assets are not yet built (blocked by Phase 1 plan 01-02). Code path is wired (delegated click → funnel.enqueue → ldisc.writeFromLower) and unit-verified in isolation; end-to-end correctness only observable from a live browser."
  - test: "ACT-04 — manual typing disabled while scenario running"
    expected: "In DevTools: setLockObserver(v=>console.log('lock',v)); funnel.lockInput() — pill flips to BUSY, all .action-btn become disabled, xterm keystrokes are silently dropped; funnel.unlockInput() restores."
    why_human: "API surface and observer are unit-tested; the visible BUSY pill + button-disabled state requires a running browser. Phase 4 will be the first real caller of lockInput()."
  - test: "WCH-05 + UI — non-intrusive 'fresher build available' banner"
    expected: "While `bootroom serve` is running, `touch <kernel-path>` (or `make` in kernel repo); a banner appears between header and terminal with 'Kernel rebuilt — [LAUNCH] [×]'; clicking × dismisses; next change re-shows it; never auto-reloads."
    why_human: "Visual UI behavior + banner-priority ladder (iso > config-invalid > kernel-fresh); requires browser to observe."
  - test: "CFG-10 + UI — config-invalid red banner clears on fix"
    expected: "Write a broken bootroom.toml; red error banner appears with line/col; last-known-good action buttons remain rendered (UI-SPEC Interaction Contract 5); fix the file; red banner clears and action buttons re-render in TOML order."
    why_human: "Visual UI behavior across the live-reload cycle; requires browser. Underlying WS frames (ConfigUpdate / ConfigInvalid) and watcher fan-out are integration-tested."
---

# Phase 3: Config, Buttons, Watcher — Verification Report

**Phase Goal:** A user authors a `bootroom.toml`, sees grouped action buttons appear in the UI, clicks them to drive the guest, and runs `make` in their kernel repo to get a non-intrusive 'fresher build available' banner.

**Verified:** 2026-05-19T10:23:49Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

The Rust + JS surface for the phase goal is complete and the test suite is green. All 19 requirements (CFG-01..10, ACT-01..04, WCH-01..05) are implemented in code with integration coverage; 4 of them (ACT-02, ACT-04, WCH-05, CFG-10) carry an explicit visual/manual sign-off per the phase's own Validation Strategy (`03-VALIDATION.md` §Manual-Only Verifications). The end-to-end "click button → guest responds" loop cannot be exercised here because qemu-wasm assets remain blocked by Phase 1 plan 01-02 (artifact docker build not yet run); the plan executor noted this in 03-11-SUMMARY and the ROADMAP entry ("headed-smoke checkpoint deferred to next interactive session").

### Roadmap Success Criteria

| # | Success Criterion | Status | Evidence |
|---|------|--------|----------|
| 1 | `bootroom.toml` parsed from CWD by default; `--config` override; all structs use `deny_unknown_fields`; `schema_version` required; typos/incompatible versions produce loud, locating errors. | VERIFIED | `crates/bootroom-core/src/config.rs:30,40,51,62` `#[serde(deny_unknown_fields)]` on Config/Action/Scenario/Assertion; `schema_version: u32` required; `LoadedConfig::load_from_str_with_overrides` rejects `schema_version != 1` with typed error (line 299); unit tests `schema_version_rejected`, `deny_unknown_fields_with_span`, `offset_to_line_col_*` pass. CWD default in `check_cmd.rs:33` and `server.rs` startup. |
| 2 | Action buttons render from `/api/config`, stable insertion order; click writes bytes to guest serial; CLI `--action "label=<bytes>"` repeatable, appends/overrides. | VERIFIED (code) / HUMAN_NEEDED (live guest write — ACT-02) | `api_config.rs:26` returns JSON projection (Plan 07); `web/app.js:229-303` `renderActionButtons` preserves Map-based first-seen group order; delegated click handler (line 274-289) calls `funnel.enqueue(b64ToBytes(b64), {pacingMs:15})`. CLI parser in `cli.rs:113-135` (`parse_cli_action`), repeatable via `Vec<CliAction>`. Tests: `api_config_endpoint` (4 async), `serve_with_cli_action` (2 async), `cli::tests::parse_cli_action_*` (5 unit). Live byte-to-guest write requires browser (ACT-02 in Human Verification). |
| 3 | Scenarios are typed; load-time validation rejects refs to unknown actions with locating errors; `bootroom check` validates without serving; `bootroom init` writes minimal example. | VERIFIED | Scenario/Assertion types (`config.rs:51-100`); validation `LoadedConfig` returns `scenario_unknown_action_ref` style errors (unit test passes). `check_cmd.rs:32 pub fn run` + 8 integration tests in `check_subcommand.rs`. `init_cmd.rs:72 pub fn run` with `EXAMPLE` const + `OpenOptions::create_new(true)` (WR-04 fix) + 6 integration tests in `init_subcommand.rs`. |
| 4 | Editing `bootroom.toml` while `serve` is running updates UI in place (no restart). | VERIFIED (server side) / HUMAN_NEEDED (UI visual — CFG-10) | `watcher.rs:269` watches config parent dir non-recursive (CR-03 fix for atomic-rename); broadcasts `ConfigUpdate { config }` or `ConfigInvalid { error, line, col }` on parse. Integration test `watcher_live_reload::toml_reload` covers both success+invalid+recovery paths; `watcher_config_atomic_rename` covers atomic-rename. `app.js:599-633` handles both WS frame types and re-renders. Browser visual check left to human. |
| 5 | Kernel watcher: `notify-debouncer-full` ~300ms debounce, parent-dir watch, size-stability gate, ELF magic check, non-intrusive banner — never auto-reloads. Manual serial typing disabled during scenario, re-enabled on completion. | VERIFIED (mechanics) / HUMAN_NEEDED (UI banner — WCH-05; lock-pill — ACT-04) | `watcher.rs`: `new_debouncer` with 300ms (line 193+), `RecursiveMode::NonRecursive` on `kernel_parent` (line 270), `SIZE_STABILITY_WINDOW = 100ms` (line 86, gate at line 312), `ELF_MAGIC = [0x7f,'E','L','F']` (line 74, check at 327-340). KernelChanged broadcast; UI never auto-reloads — `app.js:636-651` only sets banner state. Tests: `watcher_debounce`, `watcher_atomic_rename`, `watcher_size_stability`, `watcher_elf_magic`, `watcher_ws_frame`. Lock primitive: `funnel.js:123-138` lockInput/unlockInput + setLockObserver, caller-side guards in `app.js:284, 412, 419`. Phase 4 is first real caller; current behavior validated via console (ACT-04). |

**Score:** 5/5 roadmap success criteria implemented; 4 of them carry residual visual sign-offs per the phase's own validation contract.

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| **CFG-01** — `bootroom.toml` from CWD, `--config` override | VERIFIED | `cli.rs:43-89` ServeArgs.config; `check_cmd.rs:33` default `PathBuf::from("bootroom.toml")`; integration test `config_loading.rs`. |
| **CFG-02** — labeled, grouped action with bytes | VERIFIED | `config.rs:40-50` Action struct (label/bytes/group/description); `actions_roundtrip` unit test. |
| **CFG-03** — scenarios w/ action refs + assertions + timeout | VERIFIED | `config.rs:51-100` Scenario + Assertion + AssertionKind; `scenarios_parse` unit test. |
| **CFG-04** — `schema_version` required, mismatch fails | VERIFIED | `config.rs:299-300` schema_mismatch error; `schema_version_rejected` unit test. |
| **CFG-05** — `deny_unknown_fields` w/ line:col | VERIFIED | All 4 config structs annotated; `deny_unknown_fields_with_span`, `offset_to_line_col_*` tests. |
| **CFG-06** — scenario→action ref validated | VERIFIED | `config.rs:352` `actions_by_label.contains_key` check; `scenario_unknown_action_ref` unit test. |
| **CFG-07** — `bootroom check` subcommand | VERIFIED | `check_cmd.rs` + `Cmd::Check(args)` dispatch (`main.rs:23`); 8 integration tests in `check_subcommand.rs`. |
| **CFG-08** — `bootroom init` writes example | VERIFIED | `init_cmd.rs` + `Cmd::Init` dispatch; `EXAMPLE` const ~30 lines; `create_new(true)` atomic write (WR-04); 6 integration tests in `init_subcommand.rs`. |
| **CFG-09** — action button order stable from TOML | VERIFIED | `actions_insertion_order_preserved` unit test; Map<groupLabel,…> in `app.js:229` preserves first-seen order; `api_config_endpoint` order test. |
| **CFG-10** — live TOML reload updates UI | VERIFIED (server) / HUMAN_NEEDED (UI visual) | Watcher broadcasts ConfigUpdate; `app.js:599-616` re-renders. `watcher_live_reload` + `watcher_config_atomic_rename` integration tests. Visual confirmation deferred to browser. |
| **ACT-01** — buttons from `/api/config` grouped | VERIFIED | `api_config.rs:26` handler; `app.js:229-269` renderActionButtons groups via Map; integration test `api_config_endpoint::shape_includes_base64_bytes`. |
| **ACT-02** — button click writes to guest serial | HUMAN_NEEDED | Code path wired (delegated click → `funnel.enqueue` → existing Phase-2 ldisc.writeFromLower path), but live guest-write loop requires qemu-wasm in a browser (blocked by 01-02 assets). |
| **ACT-03** — CLI `--action` repeatable, append/override | VERIFIED | `cli.rs:113 parse_cli_action`; `Vec<CliAction>` via clap; merge logic in `LoadedConfig::load_from_str_with_overrides`; unit tests `cli_override_appends_new_action`, `cli_override_replaces_existing_action_bytes`, `last_cli_action_wins_for_same_label`; integration tests in `serve_with_cli_action.rs`. |
| **ACT-04** — typing disabled during scenario | VERIFIED (API+guards) / HUMAN_NEEDED (BUSY pill visual) | `funnel.js:123-138` lockInput/unlockInput + idempotent + `setLockObserver`; caller-side guards in `app.js:284, 412, 419`; BUSY pill at `app.js:895 setPill('BUSY')`. Visual confirmation deferred. |
| **WCH-01** — `notify-debouncer-full` 300ms debounce | VERIFIED | `watcher.rs:193 new_debouncer(Duration::from_millis(300),…)`; `watcher_debounce` integration test. |
| **WCH-02** — atomic-rename safe (parent-dir watch) | VERIFIED | `watcher.rs:270` watches `kernel_parent` non-recursive; `watcher_atomic_rename` integration test. |
| **WCH-03** — size-stability across debounce ticks | VERIFIED | `SIZE_STABILITY_WINDOW = 100ms` (line 86); two-sample gate at lines 305-323; `watcher_size_stability` integration test. |
| **WCH-04** — ELF magic sniff, non-ELF warns | VERIFIED | `ELF_MAGIC` constant (line 74); read_exact(4) + compare at 327-340; non-ELF emits `KernelChanged { ok:false, reason:"not ELF" }`; `watcher_elf_magic` integration test. |
| **WCH-05** — non-intrusive fresh-build banner | VERIFIED (frame) / HUMAN_NEEDED (visual) | `watcher.rs:373` broadcasts KernelChanged with ok/mtime/size/sha256_prefix; `app.js:636-651` handles frame, sets bannerState, never reloads; `index.html:52 #fresh-banner`. `watcher_ws_frame` integration test. Visual sign-off deferred. |

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `crates/bootroom/src/watcher.rs:287` | `std::thread::sleep(SIZE_STABILITY_WINDOW)` on debouncer thread | INFO | Deferred per WR-02; 100ms latency documented in module header. Out of v1 perf scope. |
| `crates/bootroom/web/app.js` | Frame-field narrowing (`ok === true`, `typeof === 'string'`) | INFO | Deferred per WR-05; defensive low-priority polish (v1 polish pass). |
| `crates/bootroom/src/watcher.rs:287-300` | No backoff cap on persistent size-instability | INFO | Deferred per WR-09; mitigated in practice (make finishes link in <1s). |
| `crates/bootroom/web/app.js` | TODO/FIXME/PLACEHOLDER markers | NONE | grep found no unresolved debt markers in Phase-3 files. |

No BLOCKER anti-patterns. The three deferred items are all explicitly accepted in 03-REVIEW.md "Fixes Applied → Deferred (rationale)".

### Test Suite

`cargo test --workspace`: **47 lib (bootroom) + 34 lib (bootroom-core) + integration tests across ~22 files, all passing, 0 failures, 0 ignored.**

`cargo clippy --workspace --all-targets`: clean, no warnings.

### Human Verification Required

See frontmatter `human_verification` for the 4 explicit visual/interactive sign-offs:

1. **ACT-02** — button click writes bytes to guest serial (requires qemu-wasm guest)
2. **ACT-04** — BUSY pill + disabled action buttons during `funnel.lockInput()`
3. **WCH-05 visual** — fresh-build banner appears, dismisses, re-shows; never auto-reloads
4. **CFG-10 visual** — red config-invalid banner appears + auto-clears on fix; last-known-good buttons preserved

These items are pre-declared as manual checks in `03-VALIDATION.md §Manual-Only Verifications` and acknowledged in the ROADMAP entry as "headed-smoke checkpoint deferred to next interactive session (autonomous mode + qemu-wasm assets blocked by Phase-1 01-02)." All underlying code paths, WS frames, and DOM/CSS containers are unit/integration-verified.

### Notes on REQUIREMENTS.md Traceability Table

The traceability table at `.planning/REQUIREMENTS.md:173-191` still lists many Phase-3 items as "Pending" because it was last updated on 2026-05-17 before Phase 3 plans executed. The Phase-3 plan-level SUMMARY frontmatter (`requirements_completed`/`requirements_satisfied`/`requirements`) collectively covers all 19 requirements. The table is out of sync with the code; updating it is a planning-doc bookkeeping task (suggest doing it as part of `gsd-state-update`), not a code-level gap.

### Gaps Summary

No code gaps. All 19 requirements have implementation + tests; clippy is clean; full test suite is green. The four `human_needed` items are visual/interactive checks that the phase plan and ROADMAP explicitly designate as manual sign-offs, blocked on Phase-1 qemu-wasm assets. Status is `human_needed` (not `passed`) because the goal — "a user clicks buttons to drive the guest" — has an inherently end-to-end component that cannot be exercised in an automated headless verification while qemu-wasm assets are missing.

---

_Verified: 2026-05-19T10:23:49Z_
_Verifier: Claude (gsd-verifier)_
