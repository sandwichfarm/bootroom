---
phase: 03-config-buttons-watcher
plan: 08
subsystem: ws-broadcast-forwarder
tags: [ws, broadcast, fan-out, axum, tokio]
requires: [03-02, 03-05, 03-06]
provides: [ws-broadcast-fanout, kernel-changed-push, config-update-push, config-invalid-push]
affects: [crates/bootroom/src/ws.rs]
tech_added: []
patterns: [broadcast-forwarder-per-connection, subscribe-before-hello, lagged-log-continue]
files_created:
  - crates/bootroom/tests/ws_broadcast_fanout.rs
files_modified:
  - crates/bootroom/src/ws.rs
  - crates/bootroom/tests/common/mod.rs
key_decisions:
  - "Subscribe-before-Hello ordering — frames published during connection setup are still delivered to THIS connection."
  - "Lagged → tracing::warn + continue (never break) so a momentarily-slow writer cannot permanently silence server pushes."
  - "Three tokio tasks per connection (reader / writer / bcast_forwarder); explicit abort of forwarder on disconnect for fast test teardown."
  - "spawn_with_broadcast_handle test helper exposes Arc<AppState> so tests publish broadcasts directly (no HTTP/watcher detour)."
metrics:
  duration_minutes: ~25
  tasks_completed: 2
  files_touched: 3
  commits: [6d2948e, 2088215]
completed_at: "2026-05-19"
requirements_satisfied: [CFG-10, WCH-05]
---

# Phase 3 Plan 08: WS Broadcast Forwarder Summary

**One-liner:** Per-connection `bcast_forwarder` task in `handle_socket` subscribes to `state.ws_broadcast` and forwards every server-owned `WsMessage` into the existing per-connection mpsc — making `KernelChanged`, `ConfigUpdate`, and `ConfigInvalid` reach every connected `/ws` client live.

## What Shipped

### `crates/bootroom/src/ws.rs::handle_socket`

Three tokio tasks per WebSocket connection:

1. **Reader loop** (this fn body) — Phase 2, unchanged. Drains inbound WS frames.
2. **Writer task** — Phase 2, unchanged. Drains the per-connection mpsc (`tx` / `rx`) into the WS sink.
3. **Broadcast forwarder task** (NEW) — Phase 3. Drains `state.ws_broadcast.subscribe()` and forwards each `WsMessage` into the same mpsc via a cloned `tx_for_bcast`.

```text
state.ws_broadcast (Sender, cap=16)
   └─[subscribe per connection]→ bcast_rx ── bcast_forwarder ──→ tx_for_bcast ──┐
                                                                                ├─→ per-conn mpsc (cap=32) ─→ writer ─→ WS sink
   handle_wire (reader-loop dispatch) ─────────────────────────────→ tx ───────┘
```

### Subscribe-before-Hello ordering (T-03-08-03)

`state.ws_broadcast.subscribe()` is called *before* `tx.send(Hello)`. tokio's broadcast back-buffer is per-receiver and future-only — any frame published after subscribe but before the reader-loop starts is still captured for THIS connection. This prevents the race where a watcher fires `ConfigUpdate` while a new tab is mid-handshake.

### Lagged → log + continue

`RecvError::Lagged(n)` from the broadcast receiver is logged via `tracing::warn!(skipped = n, "...")` and the loop **continues**. The cost of falling behind is dropped frames (the contract of broadcast channels), never a dead forwarder. Per T-03-08-01: `skipped` is a `u64` count, not user-controlled bytes — no log-amplification surface.

`RecvError::Closed` breaks the loop. The current `AppState` lifecycle never drops the broadcast `Sender` (it lives as long as the process), so this is defensive — but the path is handled.

### Cleanup on disconnect

After the reader loop exits:

```text
drop(tx)              → writer task sees rx return None → exits → sink.close()
writer.await          → wait for the writer to finish closing the sink
bcast_forwarder.abort() → fire-and-forget; task cancels even if parked in recv()
```

The forwarder would also exit naturally when its `tx_for_bcast.send` errors (after writer dropped its rx), but the explicit `.abort()` guarantees no straggler iteration during rapid test connect/disconnect cycles (T-03-08-05).

### `handle_wire` exhaustive match

Already restored by Plan 03-01 — `WsMessage::{KernelChanged, ConfigUpdate, ConfigInvalid}` are folded into the same `State | Hello` server-owned-message warn arm. Clippy stayed green throughout this plan; no edit needed.

### Test infrastructure

`crates/bootroom/tests/common/mod.rs` — new helper:

```rust
pub async fn spawn_with_broadcast_handle(
    kernel: PathBuf,
    assets_dir: Option<PathBuf>,
) -> (TestServer, Arc<AppState>);
```

Returns the `Arc<AppState>` so the test can `state.ws_broadcast.send(...)` directly without spinning up the real watcher or going through HTTP. Distinct from the parallel Plan 03-07 `spawn_with_loaded` helper.

### Integration tests (`crates/bootroom/tests/ws_broadcast_fanout.rs`, 290 LOC)

| Test | What it pins |
|------|-------------|
| `single_client_receives_kernel_changed` | One broadcast → one connected client receives the JSON frame intact. |
| `two_clients_both_receive_one_send` | Fan-out: a single `broadcast::Sender::send` reaches BOTH connected clients. |
| `client_misses_broadcasts_before_connect` | **Pitfall #3 confirmed.** Zero-receiver broadcast returns `Err`, late-joining client only sees `Hello`. Motivates the `/api/config` HTTP fetch on connect (last-known-good fallback). |
| `config_invalid_frame_round_trips` | `ConfigInvalid { error, line, col }` survives WS framing and re-deserializes. |
| `lagged_receiver_logged_and_continues` | Burst 20 frames at 16-cap broadcast channel while client is NOT reading → `Lagged` fires inside the forwarder. Forwarder MUST survive — proven by a distinct 21st `Launch` frame eventually arriving after the client resumes reading. |

### Phase-2 regression

`tests/ws_roundtrip.rs` is **unchanged** and all three Phase-2 tests still pass:

- `ws_handshake_emits_hello`
- `ws_client_serial_in_is_logged_not_echoed`
- `ws_upgrade_response_carries_coop_coep`

This guarantees the new forwarder did not perturb the reader/writer paths.

## Deviations from Plan

**None — plan executed exactly as written.**

A couple of micro-deviations worth noting (all consistent with the plan's intent):

- The plan's Action step 6 (extend `handle_wire`'s exhaustive match to cover the 3 new variants) was already done in Plan 03-01 (see `crates/bootroom/src/ws.rs` lines 160-169). Nothing to do here. Clippy stayed green.
- Step 5's docstring on `handle_socket` was added at the top of the function instead of as a separate `///` paragraph, matching the existing file style.

## Concurrency notes (parallel agent on Plan 03-07)

The parallel 03-07 agent's untracked work (state.rs `new_for_test_with_loaded`, tests/common/mod.rs `spawn_with_loaded`, and three new test files) was visible in the working tree throughout this plan. The conflict surface was kept disjoint:

- I added `spawn_with_broadcast_handle` to `tests/common/mod.rs` as a *new* helper at a different position — no overlap with `spawn_with_loaded`.
- I did NOT touch `state.rs`. The parallel agent's `new_for_test_with_loaded` addition remained uncommitted in their working tree until their commit landed (didn't intersect with my commit window).
- The parallel agent's untracked test files (`api_config_endpoint.rs`, `config_loading.rs`, `serve_with_cli_action.rs`) reference `spawn_with_loaded` and `new_for_test_with_loaded`; they compile only after the parallel agent's own commit. My commit doesn't depend on either symbol and built cleanly in isolation.

The parallel agent re-staged `spawn_with_loaded` in the working tree after my commit landed, which is the expected steady state.

## Threat Flags

None — no new network surface, auth path, file access, or schema boundary introduced by this plan. The forwarder lives entirely behind the existing `/ws` route.

## Self-Check: PASSED

Files claimed:

- `crates/bootroom/src/ws.rs` — FOUND (modified, +62 lines for forwarder + import + doc comment)
- `crates/bootroom/tests/ws_broadcast_fanout.rs` — FOUND (290 LOC, 5 tests)
- `crates/bootroom/tests/common/mod.rs` — FOUND (modified, +25 lines for helper)

Commits claimed:

- `6d2948e` (Task 1) — FOUND in `git log --all`
- `2088215` (Task 2) — FOUND in `git log --all`

Tests:

- `cargo test --test ws_broadcast_fanout` — 5/5 pass
- `cargo test --test ws_roundtrip` — 3/3 pass (Phase-2 regression clean)
- `cargo test -p bootroom` — all green (lib + every integration test)
- `cargo clippy -p bootroom --lib --tests -- -D warnings` — clean

## Requirements satisfied

- **WCH-05** (kernel-changed push): `KernelChanged` frames published by the watcher (Plan 03-06) now reach every connected `/ws` client via the new forwarder, with the documented "browser fetches `/api/kernel/info` to refresh banner" recovery path intact.
- **CFG-10** (config live-reload push): `ConfigUpdate` / `ConfigInvalid` frames published by the watcher (Plan 03-06) now reach every connected `/ws` client. Pre-connect frames are dropped (Pitfall #3), which is correct given Plan 03-07's `/api/config` HTTP endpoint serves the last-known-good fallback.
