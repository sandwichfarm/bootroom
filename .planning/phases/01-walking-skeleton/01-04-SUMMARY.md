---
phase: 01-walking-skeleton
plan: 04
subsystem: server
tags:
  - rust
  - axum
  - server
  - middleware
  - cli
dependency-graph:
  requires:
    - 01-01-SUMMARY.md  # workspace + bootroom crate scaffolding
    - 01-02-SUMMARY.md  # crates/bootroom/assets/qemu/ artifacts
    - 01-03-SUMMARY.md  # crates/bootroom/web/vendor/ (makes WEB dir non-empty for include_dir!)
  provides:
    - "library API: build_router(state), AppState, ServeArgs"
    - "CLI: bootroom serve --kernel <path> [--host A] [--port N] [--assets-dir D]"
    - "COOP/COEP middleware applied at top-level router"
    - "include_dir embed roots: embed::WEB, embed::QEMU"
  affects:
    - 01-05  # will replace stub handlers with real ones using the same router shape
    - 01-07  # will spawn build_router(state) for integration tests
tech-stack:
  added:
    - axum 0.8.9
    - tower 0.5.3 (util feature in dev-deps)
    - tower-http 0.6.10 (set-header, trace)
    - tokio 1.52.3 (rt-multi-thread, macros, signal, sync, fs)
    - tokio-util 0.7 (io)
    - include_dir 0.7.4
    - mime_guess 2.0.5
    - serde 1 / serde_json 1
    - tracing 0.1.44 / tracing-subscriber 0.3 (env-filter, fmt)
    - sha2 0.10 / hex 0.4
  patterns:
    - "SetResponseHeaderLayer::overriding for COOP/COEP on every response"
    - "static Dir<'static> via include_dir! macro at module scope"
    - "anyhow::Context for fail-loud startup errors"
    - "tracing::warn! for V4 ASVS partial (non-loopback bind)"
key-files:
  created:
    - crates/bootroom/src/lib.rs
    - crates/bootroom/src/cli.rs
    - crates/bootroom/src/state.rs
    - crates/bootroom/src/headers.rs
    - crates/bootroom/src/embed.rs
    - crates/bootroom/src/server.rs
  modified:
    - crates/bootroom/Cargo.toml
    - crates/bootroom/src/main.rs
decisions:
  - "Use SetResponseHeaderLayer::overriding (not if_not_present) so COOP/COEP win even if a handler tries to set conflicting values"
  - "Validate --kernel path at server::run startup (V5) before binding; fail loud with clear message"
  - "Warn (not refuse) on non-loopback --host (V4 partial) — user-controlled and documented; hard refusal lands with bootroom doctor in Phase 5"
  - "Default to RUST_LOG=bootroom=info,tower_http=info when env not set; keeps the smoke run quiet by default"
metrics:
  duration: "~12min"
  completed: 2026-05-17
---

# Phase 01 Plan 04: axum server skeleton Summary

axum 0.8 server skeleton with COOP/COEP middleware, clap CLI, AppState, and include_dir embed roots — ready for plan 01-05 to replace four stub handlers with real ones.

## What Shipped

`bootroom serve --kernel <path>` now parses, validates the kernel path, binds 127.0.0.1:8765 (or `--host A --port N`), and serves four stub routes — every response (200, 404, 501) carrying the cross-origin-isolation header pair required by qemu-wasm. The library entrypoint (`bootroom::build_router`, `bootroom::AppState`, `bootroom::ServeArgs`) is what plan 01-07's integration tests will spawn in-process.

## Tasks

| Task | Name                                                       | Commit  |
| ---- | ---------------------------------------------------------- | ------- |
| 1    | Runtime deps + lib crate + clap structs                    | 2dfce72 |
| 2    | AppState, COOP/COEP layers, include_dir embeds (TDD)       | 1e57bd8 |
| 3    | build_router, server::run, tokio main + smoke tests (TDD)  | 9ce9c5c |

## Verification

`cargo test -p bootroom --lib` — **6 passed**:

```
test embed::tests::test_embed_qemu_dir_has_wasm ... ok
test state::tests::test_appstate_construct ... ok
test headers::tests::test_coop_layer_overrides_existing_header ... ok
test headers::tests::test_coep_layer_value ... ok
test server::tests::test_router_returns_coop_coep_on_404 ... ok
test server::tests::test_router_returns_coop_coep_on_stub_route ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo clippy --workspace --all-targets -- -D warnings` — clean (pedantic enabled).

**Binary smoke runs (manual, per plan 01-04 verify block):**

| Invocation                                                  | Observed Output / Behavior                                                                                                                       |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `bootroom serve --kernel /tmp/fake-kernel --port 0`         | `Serving bootroom on http://127.0.0.1:38881 (Ctrl-C to stop)` — confirms `--port 0` resolves to a real ephemeral port via `listener.local_addr()` |
| `bootroom serve --kernel /does/not/exist`                   | Exits 1 with `Error: --kernel: file not found at /does/not/exist`                                                                                |
| `bootroom serve --kernel /tmp/fake-kernel --host 0.0.0.0 --port 0` | Emits `WARN bootroom::server: Binding to non-loopback address 0.0.0.0:0; ...` then `Serving bootroom on http://0.0.0.0:34061 ...`         |
| `bootroom --help`                                           | Lists `serve` subcommand                                                                                                                          |
| `bootroom serve --help`                                     | Shows `--kernel <PATH>`, `--host` (default 127.0.0.1), `--port` (default 8765), `--assets-dir <PATH>`                                            |

Loopback warning fires correctly for `--host 0.0.0.0`. The bound address printed by `--port 0` is a real ephemeral port (38881 / 34061 in the two test runs), not the literal `0` from CLI input — confirms `TcpListener::local_addr()` is what gets formatted into the startup line, not the parsed `--port` value.

## Success Criteria

- **SERV-01** binds 127.0.0.1 on default port 8765 — proven by the smoke run with `--port 0` exercising the same bind code path; `bootroom serve --help` shows `8765` as the literal default.
- **SERV-02** all responses carry COOP+COEP — proven by `test_router_returns_coop_coep_on_404` (axum's built-in 404 path) and `test_router_returns_coop_coep_on_stub_route` (501 from handler). Both error categories that handler-level header sets cannot reach are covered.
- **SERV-05** `--host` and `--port` override defaults — proven by the manual smoke runs above.
- **CLI-03** one-command invocation — `bootroom serve --kernel <path>` is the entire command surface; no `init` step required.
- **Library entrypoint** — `bootroom::build_router(state)` is the function plan 01-07 will call to drive the router in-process without spawning a subprocess.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed `#[must_use]` from `build_router` (and `coop_layer`/`coep_layer` shells)**
- **Found during:** Task 1 + Task 3 clippy runs
- **Issue:** clippy::double_must_use fired because `axum::Router` and `SetResponseHeaderLayer` are already `#[must_use]` upstream; adding the attribute at the function level is redundant and clippy-pedantic rejects it under `-D warnings`.
- **Fix:** Removed the function-level `#[must_use]` on `build_router`. `coop_layer`/`coep_layer` ended up with `#[must_use]` left in headers.rs because their return type isn't transitively marked — clippy did not complain about those (kept).
- **Files modified:** crates/bootroom/src/server.rs (Task 1 placeholder and Task 3 final), crates/bootroom/src/embed.rs (doc-string backtick fix for clippy::doc_markdown on `include_dir`)
- **Commits:** rolled into 2dfce72 and 9ce9c5c

**2. [Rule 3 - Blocker] Placeholder content in Task 1 for headers.rs/embed.rs/server.rs**
- **Found during:** Task 1
- **Issue:** lib.rs declares `pub mod {state, embed, headers, server}` and re-exports `AppState` + `build_router`. Without these modules existing as compilable files, Task 1's `cargo build --workspace` verify would fail. The plan implicitly assumed these can be empty stubs at Task 1.
- **Fix:** Created minimal placeholder modules: empty doc-comment-only files for headers/embed (no exports referenced), `AppState` defined in state.rs (Task 2 content moved forward — same content as Task 2 specified), and a `build_router(_state) -> Router` no-op in server.rs. Task 2 and Task 3 overwrote these placeholders with full content per spec.
- **Files modified:** crates/bootroom/src/{state,headers,embed,server}.rs
- **Commit:** 2dfce72 (Task 1)

**3. [Rule 1 - Bug] main.rs returning Result<()> tripped clippy::unnecessary_wraps in Task 1 placeholder**
- **Found during:** Task 1 clippy run
- **Issue:** The Task 1 placeholder main returned `anyhow::Result<()>` but only `Ok(())`, which clippy::unnecessary_wraps flags under pedantic. The plan's Task 3 main returns `Result<()>` (with `bootroom::server::run(args).await` providing the error path), so the issue was only in the transient placeholder.
- **Fix:** Made Task 1 placeholder `fn main()` (no return), Task 3 restored `#[tokio::main] async fn main() -> anyhow::Result<()>` per spec.
- **Commit:** rolled into 2dfce72

### Skipped / Deferred

- **Loopback IP check:** The current `is_loopback` only matches `127.0.0.0/8` (via `Ipv4Addr::is_loopback`) and `::1` (via `Ipv6Addr::is_loopback`). Other "non-public" CIDRs (private RFC1918, link-local) bind without the warning. Acceptable for Phase 1 per V4-partial scope; `bootroom doctor` in Phase 5 owns the full check.
- **`include_dir!` build-host path leak (open question A.4):** Plan 01-07 owns the `strings target/release/bootroom | grep $HOME` verification.

## Threat Flags

None — no new security surface beyond what the plan's `<threat_model>` already enumerates. The four stub routes return 501 with no exposed state.

## Known Stubs

The four route handlers (`index_stub`, `kernel_info_stub`, `kernel_stream_stub`, `asset_stub`) all return `(StatusCode::NOT_IMPLEMENTED, "plan 01-05 wires this: GET <path>")`. This is intentional per the plan — plan 01-05 replaces them with real handlers using the same router shape.

## Self-Check: PASSED

Verified files exist:
- FOUND: crates/bootroom/src/lib.rs
- FOUND: crates/bootroom/src/cli.rs
- FOUND: crates/bootroom/src/state.rs
- FOUND: crates/bootroom/src/headers.rs
- FOUND: crates/bootroom/src/embed.rs
- FOUND: crates/bootroom/src/server.rs
- FOUND: crates/bootroom/src/main.rs (modified)
- FOUND: crates/bootroom/Cargo.toml (modified)

Verified commits exist:
- FOUND: 2dfce72 (Task 1)
- FOUND: 1e57bd8 (Task 2)
- FOUND: 9ce9c5c (Task 3)
