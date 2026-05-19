---
phase: 03-config-buttons-watcher
plan: 05
subsystem: bootroom-server-state
tags: [appstate, config, broadcast, canonicalization, startup-validation]
requirements: [CFG-01, CFG-09, CFG-10, WCH-05]
dependency_graph:
  requires:
    - 03-01  # LoadedConfig + CliAction (consumed by AppState + server::run)
    - 03-02  # WsMessage incl. KernelChanged / ConfigUpdate / ConfigInvalid (broadcast payload)
    - 03-03  # ServeArgs.config + ServeArgs.actions (CLI surface server::run reads)
  provides:
    - "AppState.config_path + AppState.config_path_canon"
    - "AppState.kernel_canon"
    - "AppState.loaded_config: Arc<RwLock<LoadedConfig>>"
    - "AppState.ws_broadcast: broadcast::Sender<WsMessage>"
    - "server::run startup-config-validation preflight"
    - "AppState::new_for_test compatibility shim for Phase-2 tests"
  affects:
    - 03-06  # watcher.rs (writes loaded_config; sends KernelChanged / ConfigUpdate via ws_broadcast)
    - 03-07  # /api/config (reads loaded_config)
    - 03-08  # WS forwarder (subscribes to ws_broadcast)
tech_stack:
  added:
    - "tokio::sync::broadcast (capacity 16 — fan-out to per-conn WS subscribers)"
  patterns:
    - "Single Arc<RwLock<LoadedConfig>> = one writer (Plan 06 watcher) + many readers (Plan 07 /api/config)"
    - "Canonical-path-at-startup (Pitfall #1 mitigation): kernel + config canonicalized once, stored in AppState, watcher compares against absolute form"
    - "Compat-shim test constructor (`new_for_test`) preserves the Phase-2 call signature so tests/common/mod.rs::spawn stays one-line"
key_files:
  created: []
  modified:
    - crates/bootroom/src/state.rs                 # +5 fields, new+new_for_test, 5 unit tests
    - crates/bootroom/src/server.rs                # config-load preflight in run() + 2 unit tests
    - crates/bootroom/src/assets.rs                # test_state -> new_for_test
    - crates/bootroom/src/kernel_info.rs           # test_state -> new_for_test
    - crates/bootroom/src/kernel_stream.rs         # test_state -> new_for_test
    - crates/bootroom/spikes/spike-b/src/main.rs   # spike call site -> new_for_test
    - crates/bootroom/tests/common/mod.rs          # spawn helper -> new_for_test
    - crates/bootroom/tests/serve_no_open.rs       # +--config <tmp> for new startup contract
decisions:
  - "Broadcast capacity = 16 (CONTEXT <specifics>) — Lagged(n) is the slow-consumer signal Plan 08 handles."
  - "Initial-load config failure is FATAL — no deferred ConfigInvalid frame at startup. Live-reload failures emit ConfigInvalid; only the boot path bails."
  - "Order preserved: --kernel exists/is_file check fires BEFORE config-load. The Phase-1 diagnostic shape stays untouched for operators who run `bootroom serve --kernel /does/not/exist`."
  - "Canonical-path computation lives in server::run, NOT in AppState::new. Reason: AppState::new is also called by new_for_test where canonicalization would fail (test paths often don't exist). server::run is the one production caller that has already validated kernel-exists."
metrics:
  duration_minutes: 35
  completed_at: 2026-05-19
---

# Phase 3 Plan 05: AppState Extension Summary

`AppState` now carries the four new pieces of state Plans 06, 07, and 08 will consume — canonical config + kernel paths, a `LoadedConfig` behind a tokio `RwLock`, and a `broadcast::Sender<WsMessage>` for fan-out. `server::run` performs the full startup-time config preflight (resolve → read → validate → canonicalize → construct AppState) before binding the listener, so an operator launching against a broken `bootroom.toml` learns about it immediately — same `file:line:col: message` diagnostic shape that `bootroom check` will produce in Plan 04.

## Final AppState Shape

| Field                | Type                                              | Purpose                                                                      |
| -------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------- |
| `kernel`             | `PathBuf`                                         | Phase-2: as-given `--kernel` for display/serving                              |
| `kernel_canon`       | `PathBuf` (NEW)                                   | Pitfall #1 mitigation: watcher demuxes notify events vs this absolute path    |
| `assets_dir`         | `Option<PathBuf>`                                 | Phase-1: dev override                                                         |
| `assets_dir_canon`   | `Option<PathBuf>`                                 | CR-02 traversal-check anchor                                                  |
| `digest_cache`       | `Arc<RwLock<Option<CachedDigest>>>`               | WR-03 SHA-256 cache                                                          |
| `config_path`        | `PathBuf` (NEW)                                   | As-given `--config` for display                                              |
| `config_path_canon`  | `PathBuf` (NEW)                                   | Pitfall #1: watcher compares config-edit events vs this                       |
| `loaded_config`      | `Arc<RwLock<LoadedConfig>>` (NEW)                 | One writer (watcher) + many readers (`/api/config`)                          |
| `ws_broadcast`       | `broadcast::Sender<WsMessage>` (NEW)              | Plan 06/07 send; Plan 08 per-conn subscribers forward                         |

Capacity 16 per CONTEXT `<specifics>`. Slow consumers receive `Lagged(n)` and drop the oldest frames; Plan 08 will log and continue (Pitfall #3 in 03-RESEARCH).

## Startup Ordering in `server::run`

1. `--kernel` exists check (Phase-1, unchanged — first so the operator gets the right error first)
2. `--kernel` is_file check (Phase-1, unchanged)
3. **NEW:** resolve `config_path` = `args.config.unwrap_or("bootroom.toml")`
4. **NEW:** `fs::read_to_string(config_path)` — surfaces "file not found" or permission errors with the `--config:` prefix
5. **NEW:** `LoadedConfig::load_from_str_with_overrides(content, &args.actions)` — validation; on Err emits `file:line:col: message` exactly like `bootroom check` will
6. **NEW:** `fs::canonicalize(kernel)` + `fs::canonicalize(config_path)` — both must succeed; Pitfall #1 mitigation
7. **NEW:** Construct `AppState::new(...)` with the full six-arg signature
8. Build router, parse host, bind listener, optionally auto-open browser (unchanged)
9. `axum::serve` (unchanged)

A bad config aborts before step 8 — no port collision, no half-open server, no client can connect to a misconfigured bootroom.

## Compat-Shim Approach

The Phase-2 `tests/common/mod.rs::spawn(kernel, assets_dir)` helper preserved its two-arg signature by calling `AppState::new_for_test(kernel, assets_dir)`. The shim:

- Canonicalizes `kernel` via `fs::canonicalize`, falling back to `kernel.clone()` when canonicalization fails (tests use fake paths like `/tmp/fake-kernel`).
- Uses a placeholder `config_path = bootroom.toml` (tests don't exercise the watcher).
- Builds an empty `LoadedConfig` from the trivial `"schema_version = 1\n"` source.
- Constructs a fresh `broadcast::channel(16)` sender.

This kept every Phase-2 integration test compiling unchanged. Five additional call sites in unit tests (`assets.rs`, `kernel_info.rs`, `kernel_stream.rs`, server `test_state`, spike-b) were swapped from `AppState::new` to `AppState::new_for_test`. The Phase-2 `serve_no_open.rs` subprocess test was extended with `--config <tempfile>` because `bootroom serve` now genuinely requires a readable config to boot (legitimate behavior change — Rule 1 auto-fix to keep the test in sync with the new startup contract).

## Test Coverage

5 new `state::tests`:
- `appstate_new_for_test_has_empty_config` — empty `LoadedConfig` invariant
- `appstate_broadcast_subscribe_works` — round-trip through `broadcast::channel(16)`
- `appstate_clone_shares_loaded_config` — `Arc::ptr_eq` proves Clone shares the same RwLock
- `appstate_canonical_kernel_is_absolute` — tempfile path canonicalizes to absolute
- `appstate_canonicalizes_assets_dir` — preserved Phase-1 invariant

2 new `server::tests`:
- `server_run_fails_on_invalid_config` — `schema_version = 99` → `Err` within 300ms timeout; never reaches `bind`
- `server_run_fails_on_missing_kernel_keeps_pre_existing_behavior` — `--kernel /does/not/exist` with valid config still emits the Phase-1 `--kernel: file not found` diagnostic; preserves error order

Full workspace test count: 42+34+3+6+4+3+2+1+1+2+3 = 101+ tests passing across all crates.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Phase-2 `serve_no_open.rs` subprocess test needed `--config`**
- **Found during:** Task 2 verification (`cargo test --workspace`)
- **Issue:** Pre-existing test `serve_no_open_returns_listener_without_launching_browser` ran `bootroom serve --kernel ... --no-open` without `--config`, defaulting to `./bootroom.toml`. The new startup-config-validation contract this plan introduces (and that the plan's success criteria mandate) means the server now bails before printing the canonical startup line.
- **Fix:** Added a `tempfile::NamedTempFile` writing `schema_version = 1\n` and pass `--config <path>` to the subprocess. The test now exercises only the `--no-open` codepath, as it was originally written to.
- **Files modified:** `crates/bootroom/tests/serve_no_open.rs`
- **Commit:** 2851056

**2. [Rule 2 - Critical correctness] `AppState::new_for_test` missing `# Panics` doc section**
- **Found during:** Task 2 verification (`cargo clippy --tests -- -D warnings`)
- **Issue:** The shim calls `.expect("trivial schema_version=1 config must parse")`, which clippy flags as `missing_panics_doc`. Two clippy errors blocked the verify step.
- **Fix:** Added explicit `# Panics` section documenting that the trivial config is the Plan-01 minimum-syntactic-acceptance fixture and a panic indicates a Plan-01 regression.
- **Files modified:** `crates/bootroom/src/state.rs`
- **Commit:** 2851056 (folded with Task 2 fix)

**3. [Rule 3 - Blocker] Five additional `AppState::new` test call sites outside the plan's `files_modified` list**
- **Found during:** First `cargo build` after the state.rs rewrite
- **Issue:** The plan only listed `state.rs`, `server.rs`, and `tests/common/mod.rs` under `files_modified`, but the new six-arg `AppState::new` signature breaks call sites in `crates/bootroom/src/assets.rs`, `kernel_info.rs`, `kernel_stream.rs`, and `crates/bootroom/spikes/spike-b/src/main.rs` (plus the server-test `test_state` helper).
- **Fix:** Swapped each to `AppState::new_for_test(...)`. All are inside `#[cfg(test)]` blocks or the spike-b binary; none are production code paths.
- **Files modified:** `assets.rs`, `kernel_info.rs`, `kernel_stream.rs`, `spike-b/src/main.rs`, plus the `test_state()` helper inside `server.rs::tests`
- **Commit:** 854a52a

### Other Notes

- During Task 1 commit staging, four extra docs files (`.planning/REQUIREMENTS.md`, `ROADMAP.md`, `STATE.md`, `03-03-SUMMARY.md`) ended up in commit `854a52a`. These belong to the parallel 03-03 agent and were modified between my `git add` (which staged only my source files) and `git commit`. Outcome: no harm — those files describe 03-03's completion accurately. The 03-03 agent committed its lib.rs/main.rs/cli_subcommands.rs in `ee76ad5` directly after.

## Threat Mitigations Applied

| Threat ID  | Mitigation                                                                 |
| ---------- | -------------------------------------------------------------------------- |
| T-03-05-01 | `LoadedConfig::load_from_str_with_overrides` fails loudly with file:line:col before bind. Pinned by `server_run_fails_on_invalid_config`. |
| T-03-05-02 | Both `--kernel` and `--config` canonicalized at startup. Failure to canonicalize bails (`with_context` chained). |
| T-03-05-03 | `broadcast::channel(16)` is bounded; slow consumers get `Lagged(n)`. Plan 08 handles. |
| T-03-05-04 | `Arc<tokio::sync::RwLock<LoadedConfig>>` is the textbook many-readers / one-writer primitive. |
| T-03-05-05 | Accepted — startup config errors echo operator's own file. No secret surface. |

## Commits

- `854a52a` — feat(03-05): extend AppState with config + broadcast + canonical paths (Task 1)
- `2851056` — feat(03-05): wire config-load + canonicalize in server::run (Task 2)

## Self-Check: PASSED

- `cargo test -p bootroom --lib state::` — 5 tests pass.
- `cargo test -p bootroom --lib server::` — 7 tests pass (5 pre-existing + 2 new).
- `cargo test --workspace` — all crates green (state, ws, kernel_info, kernel_stream, assets, ws_roundtrip, serve_no_open, cli_subcommands, etc.).
- `cargo clippy -p bootroom --lib --tests -- -D warnings` — exits 0.
- `AppState` carries all five new fields per the plan's `<must_haves>` truths.
- `server::run` validates config before bind; failure exits early without binding.
- `tests/common/mod.rs::spawn` continues to compile callers unchanged (Phase-2 integration tests intact).
- Both commits exist in `git log --oneline` HEAD-2.
