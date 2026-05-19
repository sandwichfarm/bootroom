---
phase: 03-config-buttons-watcher
plan: 07
subsystem: server/api
tags: [api, config, http, cfg-01, cfg-09, act-01, act-03]
status: complete
completed_date: "2026-05-19"
duration_min: 35
tasks: 2
files_created: 4
files_modified: 3
requirements_completed: [CFG-01, CFG-09, ACT-01, ACT-03]
dependency_graph:
  requires:
    - "03-01: LoadedConfig::load_from_str / CliAction"
    - "03-05: AppState::loaded_config + ws_broadcast surface"
    - "03-06: watcher::project_loaded_to_json canonical helper"
  provides:
    - "GET /api/config — JSON projection consumed by Plan 11 (app.js render)"
    - "AppState::new_for_test_with_loaded — test infrastructure for arbitrary LoadedConfig"
    - "common::spawn_with_loaded — integration-test helper"
  affects:
    - "03-08 (parallel): coexists in common/mod.rs additively; no API surface overlap"
    - "03-11: app.js consumes /api/config on initial page load"
tech_stack:
  added: []
  patterns:
    - "thin handler over RwLock<LoadedConfig> read-lock + canonical projection helper"
    - "shared projection between /api/config (HTTP) and WsMessage::ConfigUpdate (WS) — one helper, two consumers"
key_files:
  created:
    - crates/bootroom/src/api_config.rs
    - crates/bootroom/tests/api_config_endpoint.rs
    - crates/bootroom/tests/serve_with_cli_action.rs
    - crates/bootroom/tests/config_loading.rs
  modified:
    - crates/bootroom/src/lib.rs
    - crates/bootroom/src/server.rs
    - crates/bootroom/src/state.rs
    - crates/bootroom/tests/common/mod.rs
decisions:
  - "Single canonical projection helper: /api/config and WsMessage::ConfigUpdate both go through watcher::project_loaded_to_json (T-03-07-03 mitigation by construction)"
  - "new_for_test_with_loaded added to AppState in Plan 07 (not 05) — minor scope expansion driven by test infrastructure need; no production code path uses it"
  - "Subprocess tests for --config resolution (CFG-01): the CWD-default branch can only be exercised end-to-end via the binary entrypoint"
metrics:
  duration_min: 35
  commits: 2
  tests_added: 10
  tests_passing: 90  # bootroom crate total, including library + integration
---

# Phase 3 Plan 07: `/api/config` HTTP Endpoint — Summary

**One-liner:** Browser-facing `GET /api/config` endpoint exposes the parsed `bootroom.toml` (plus CLI `--action` overrides) as JSON, projected via the same canonical helper that drives `WsMessage::ConfigUpdate`.

## What landed

### Production code

- **`crates/bootroom/src/api_config.rs`** (new, ~30 lines): the `api_config` axum handler. Thin — read-locks `AppState.loaded_config`, calls `watcher::project_loaded_to_json(&loaded)`, returns `Json(Value)`. The read lock is held only for the duration of the projection (cheap; allocates a new `serde_json::Value`).
- **`crates/bootroom/src/lib.rs`**: `pub mod api_config;` added alphabetically.
- **`crates/bootroom/src/server.rs::build_router`**: one new line, `.route("/api/config", get(crate::api_config::api_config))`, immediately after `/api/kernel/info` so the API endpoints sit together in the router definition. COOP/COEP / TraceLayer apply automatically via the existing layer stack.
- **`crates/bootroom/src/state.rs`**: added `pub fn new_for_test_with_loaded(kernel, assets_dir, loaded_config) -> Self`. Test-only constructor that accepts an externally-built `LoadedConfig`. Mirrors `new_for_test` otherwise; documented as Plan-07 scope expansion driven by test infrastructure need (no production code path uses it).

### JSON projection shape (sample)

```json
{
  "schema_version": 1,
  "actions": [
    {
      "label": "reboot",
      "bytes_b64": "cmVib290DQ==",
      "group": "system",
      "description": "Soft reboot"
    }
  ],
  "scenarios": []
}
```

`base64.decode("cmVib290DQ==")` → `[b'r', b'e', b'b', b'o', b'o', b't', 0x0d]` (i.e. `"reboot\r"`). The browser never re-runs the escape decoder — server is single source of truth.

### Test matrix

| File | Test | Requirement(s) | Mechanism |
|------|------|----------------|-----------|
| `api_config_endpoint.rs` | `shape_includes_base64_bytes` | ACT-01 | in-process axum, reqwest, base64 round-trip |
| `api_config_endpoint.rs` | `order_preserved` | CFG-09 | 4-action TOML in deliberate non-alphabetic order; wire labels match TOML order |
| `api_config_endpoint.rs` | `coop_coep_present` | regression (T-03-07-05) | header assertion against `/api/config` response |
| `api_config_endpoint.rs` | `empty_config_returns_empty_arrays` | shape | empty `actions: []` / `scenarios: []` not absent fields |
| `serve_with_cli_action.rs` | `cli_action_overrides_config_in_api_config` | ACT-03 (integration half) | TOML `reboot` + `--action reboot=<Ctrl-C>` → CLI bytes win, group/description cleared per CONTEXT D-02 |
| `serve_with_cli_action.rs` | `cli_action_appends_new_action` | ACT-03 (integration half) | new CLI label appended after existing TOML actions |
| `config_loading.rs` | `default_path_is_cwd_bootroom_toml` | CFG-01 | subprocess: `current_dir(tempdir)` with valid `bootroom.toml`, server must still be alive after 500ms |
| `config_loading.rs` | `config_flag_overrides_cwd_default` | CFG-01 | subprocess: empty tempdir + `--config <external>`, server must still be alive after 500ms |
| `config_loading.rs` | `missing_config_file_fails_startup` | CFG-01 (negative) | subprocess: `--config /nonexistent` exits non-zero, stderr references the bad path |
| `common/mod.rs` | `spawn_with_loaded` helper | — | reusable for any future test that needs an arbitrary `LoadedConfig` |

### Verification (post-Task-2)

- `cargo test -p bootroom`: **90 tests, 0 failures** across all integration test binaries.
- `cargo clippy -p bootroom --tests -- -D warnings`: clean for code under this plan's scope. (The lib has a pre-existing `unused_imports` warning in `ws.rs` introduced by parallel Plan 03-08 — out of scope per the SCOPE BOUNDARY rule; logged and left alone.)
- All four requirements satisfied:
  - **ACT-01:** the action list is fetched via HTTP at page load and rendered from JSON with `bytes_b64` pre-decoded server-side.
  - **CFG-09:** TOML insertion order survives `Vec<ResolvedAction>` → `serde_json::Value` → HTTP body.
  - **CFG-01:** `--config <path>` honored end-to-end; CWD default works in a clean tempdir; missing config is fatal at startup.
  - **ACT-03 (integration half):** the CLI override merge (Plan 03-01) + the AppState load (Plan 03-05) + the projection (Plan 03-06) + the handler (Plan 03-07) compose correctly.

## Architecture decisions

### Single canonical projection helper (T-03-07-03 mitigation)

`/api/config` and `WsMessage::ConfigUpdate` (Plan 03-08) both go through `watcher::project_loaded_to_json`. Two consumers, one source — structural drift between the initial-load HTTP response and the live-reload WS frame is impossible by construction. The browser-side renderer (Plan 03-11) can treat both paths identically.

### `new_for_test_with_loaded` scope expansion

Plan 03-05 owned the AppState surface but didn't anticipate the need for test infrastructure that drives an arbitrary `LoadedConfig` through the projection. Two options were considered:

1. Backfill 03-05 to add the test constructor.
2. Add it in 03-07 alongside the consumers.

Chose option 2 — the constructor is unmistakably test infrastructure (the module doc and the method doc both flag it), and the only callers live in 03-07's test files. Treat this as a 1-method addition justified by the test infrastructure need surfacing during 03-07 execution.

## Cross-agent coordination

Plan 03-08 ran in parallel and touched `crates/bootroom/src/ws.rs` + `crates/bootroom/tests/common/mod.rs` + a new `tests/ws_broadcast_fanout.rs`. Their commits (`6d2948e feat(03-08)` and `2088215 test(03-08)`) landed between this plan's Task-1 and Task-2 commits.

Coordination outcome:
- **`ws.rs`:** untouched by this plan per the parent-agent directive.
- **`common/mod.rs`:** parallel agent added `spawn_with_broadcast_handle`; this plan added `spawn_with_loaded`. Both helpers coexist additively, distinct names, no overlap.
- **`state.rs`:** only this plan modified (added `new_for_test_with_loaded`).
- **No commit collisions:** the plan-03-08 commits landed cleanly between this plan's commits without rebase.

## Deviations from Plan

None — the plan was executed exactly as specified. The `new_for_test_with_loaded` AppState method was already flagged in the plan body as an approved minor scope expansion.

## Deferred Items

One pre-existing clippy `unused_imports` warning in `crates/bootroom/src/ws.rs` line 33 (`broadcast::error::RecvError`). Introduced by Plan 03-08; out of scope for this plan per the SCOPE BOUNDARY rule. Either Plan 03-08's verifier or a follow-up cleanup will resolve.

## Commits

| Hash | Type | Description |
|------|------|-------------|
| `d4d0614` | `feat(03-07)` | `/api/config` handler + route wiring + signature unit test |
| `92baef7` | `test(03-07)` | 9 integration tests across 3 new files + `new_for_test_with_loaded` + `spawn_with_loaded` helper |

## Self-Check: PASSED

- `crates/bootroom/src/api_config.rs` — FOUND
- `crates/bootroom/tests/api_config_endpoint.rs` — FOUND
- `crates/bootroom/tests/serve_with_cli_action.rs` — FOUND
- `crates/bootroom/tests/config_loading.rs` — FOUND
- Commit `d4d0614` — FOUND
- Commit `92baef7` — FOUND
- `cargo test -p bootroom` — 90 passed, 0 failed
