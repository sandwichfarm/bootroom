---
phase: 02-websocket-live-serial
plan: 02
subsystem: api
tags: [axum, websocket, ws-handler, mpsc, futures-util, tokio-tungstenite, coop-coep, regression-test]

# Dependency graph
requires:
  - phase: 02-websocket-live-serial
    plan: 01
    provides: WsMessage + GuestState enums in bootroom-core
  - phase: 02-websocket-live-serial
    plan: 03
    provides: axum ws feature + futures-util + tokio-tungstenite dev-dep wired
provides:
  - "/ws axum route + ws_handler (handles upgrade, splits socket, runs writer task + reader loop)"
  - "bounded per-connection mpsc::channel::<WsMessage>(32) for back-pressure (T-02-15 mitigation)"
  - "server emits Hello { version } as first frame; passes through SerialIn / SerialOut / Launch / Reset to tracing logs"
  - "three integration tests pinning the WS contract, including COOP/COEP-on-/ws regression (Pitfall #4 / T-02-18)"
affects:
  - phase 02 plan 04 (browser WS client connects to /ws and parses Hello)
  - phase 02 plan 06 (button → Launch/Reset WS frames before reload)
  - phase 04 headless run (chromiumoxide driver speaks the same WS protocol; ws_handler stays unchanged)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pattern 1 from 02-RESEARCH.md: WebSocketUpgrade.on_upgrade(socket → split → bounded mpsc → writer task)"
    - "axum::routing::any for the WS route (forward-compat over `get`)"
    - "tokio-tungstenite::connect_async as in-process integration-test client"
    - "Pitfall #4 regression gate: middleware-headers check on a raw GET to the WS path"

key-files:
  created:
    - crates/bootroom/src/ws.rs
    - crates/bootroom/tests/ws_roundtrip.rs
  modified:
    - crates/bootroom/src/lib.rs
    - crates/bootroom/src/server.rs

key-decisions:
  - "Server is a pass-through observer in Phase 2 — Hello is the only frame the server originates"
  - "Bounded mpsc capacity 32 (02-RESEARCH.md Security Domain; T-02-15)"
  - "handle_wire kept async even though Phase 2 never awaits — Phase 4 will tx.send().await for reply frames; allow(clippy::unused_async) with a documenting comment to keep the signature stable"
  - "Route registered as axum::routing::any rather than .get() per 02-RESEARCH.md Pattern 1 (line 420) — keeps room for future negotiation"
  - "Malformed JSON / unexpected binary frames log + continue; only Close or recv error breaks the reader loop (T-02-16 mitigation)"
  - "Client-sent State/Hello frames log warn and continue — protocol error but recoverable per CONTEXT.md `<deferred>` posture (T-02-17)"
  - "COOP/COEP regression test uses a non-upgrade GET — cheapest way to inspect middleware-stack headers without doing the full handshake; status code intentionally not asserted"

patterns-established:
  - "ws.rs module shape: pub ws_handler → handle_socket → handle_wire — Phase 4 can extend handle_wire without touching the split/mpsc plumbing"
  - "tokio-tungstenite client integration tests reuse common::spawn and derive ws_url from base_url.replace(\"http://\", \"ws://\")"

requirements-completed: [WS-01, WS-04]

# Metrics
duration: ~25min
completed: 2026-05-18
---

# Phase 2 Plan 2: /ws Handler + COOP/COEP Regression Test Summary

**Wires the `/ws` axum endpoint with the split-socket + bounded-mpsc pattern, sends `Hello { version }` on connect, treats Phase 2 as a pass-through observer for client frames, and pins COOP/COEP-on-upgrade with an integration test (Pitfall #4 regression).**

## Performance

- **Duration:** ~25 min wall-clock
- **Started:** 2026-05-18T10:25:35Z
- **Completed:** 2026-05-18T10:26:40Z (commit timing window for the two task commits)
- **Tasks:** 2
- **Files modified:** 4 (2 created, 2 edited)

## Accomplishments

- `crates/bootroom/src/ws.rs` (~134 LOC) — Pattern 1 implementation: `ws_handler` (the axum extractor entrypoint), `handle_socket` (split sink/stream + spawn writer task draining a `mpsc::channel::<WsMessage>(32)`), and `handle_wire` (dispatches each variant to tracing).
- `Hello { version: env!("CARGO_PKG_VERSION").to_string() }` queued onto `tx` immediately after channel creation — clients see it as the first WS frame on every connection.
- Reader loop: `Text` → `serde_json::from_str::<WsMessage>` → dispatch; `Binary` → warn; `Ping`/`Pong` → no-op (axum auto-pongs); `Close` → break; recv error → debug + break.
- `handle_wire`: `SerialIn`/`SerialOut` → `trace!`; `Launch`/`Reset` → `info!`; `State`/`Hello` (client-sent) → `warn!` + continue (server-owned message kinds; protocol error but recoverable per CONTEXT.md `<deferred>`).
- Route registered as `.route("/ws", axum::routing::any(crate::ws::ws_handler))` in `build_router`, before `.with_state(state)` so the State extractor resolves; the COOP/COEP layers stay at the router root and apply automatically to the 101 upgrade response (and to any non-upgrade GET — that's the Pitfall #4 regression gate).
- `crates/bootroom/tests/ws_roundtrip.rs` (~121 LOC, 3 tests):
  - `ws_handshake_emits_hello` — `tokio_tungstenite::connect_async`, reads the first frame, asserts `Hello { version == env!("CARGO_PKG_VERSION") }`.
  - `ws_client_serial_in_is_logged_not_echoed` — discards Hello, sends `SerialIn { data: "aGVsbG8=" }`, then `Close`; passes if both sends succeed (no server panic, no premature close).
  - `ws_upgrade_response_carries_coop_coep` — Pitfall #4 / T-02-18 regression: a `reqwest::get("/ws")` returns the missing-Upgrade error response, and both `cross-origin-opener-policy: same-origin` and `cross-origin-embedder-policy: require-corp` are present.

## Task Commits

1. **Task 1: /ws axum handler + route + mod wiring** — `206844e` (feat)
2. **Task 2: ws_roundtrip integration tests (Hello, SerialIn, COOP/COEP)** — `8537e3b` (test)

(Plan metadata commit will follow this SUMMARY.)

## Files Created/Modified

- `crates/bootroom/src/ws.rs` (created, 134 lines) — handler module
- `crates/bootroom/tests/ws_roundtrip.rs` (created, 121 lines) — 3 integration tests
- `crates/bootroom/src/lib.rs` (modified) — added `pub mod ws;`
- `crates/bootroom/src/server.rs` (modified) — added `.route("/ws", axum::routing::any(crate::ws::ws_handler))`

## Decisions Made

- **`handle_wire` is `async` even though Phase 2 never awaits.** Clippy's `unused_async` pedantic lint fires. Rather than churn the signature when Phase 4 starts awaiting `tx.send(...)`, allowed the lint inline with an explanatory comment. Signature stays stable; one annotation in `ws.rs` vs. a Phase 4 refactor.
- **`axum::routing::any` instead of `.get(ws_handler)`** per 02-RESEARCH.md Pattern 1 line 420 — WS upgrades arrive as HTTP GET today but `any` keeps the door open for future protocol negotiation without breaking the route.
- **Pass-through observer.** Server logs Phase-2 SerialIn/SerialOut but does NOT decode the base64 (T-02-19: input-cap responsibility moves to the scenario engine layer in Phase 4). `data: _` in the match arm makes the no-decode policy explicit.
- **COOP/COEP regression test asserts on a non-upgrade GET, not the actual handshake response.** Both styles catch the same middleware-stripped failure mode; the GET form is cheaper to run and clearer to read. The plan's Open Question 4 noted a fourth test on `connect_async`'s second tuple element as optional polish — left out (default per the plan).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Lint] `clippy::unused_async` on `handle_wire`**

- **Found during:** Task 1 verification (`cargo clippy -p bootroom --all-targets -- -D warnings`)
- **Issue:** Pedantic clippy (inherited via `[lints.clippy] pedantic = warn`) rejected `handle_wire` because its body never awaits — even though the plan's action #3 explicitly specifies it `async` for Phase-4 forward compatibility.
- **Fix:** Added `#[allow(clippy::unused_async)]` with an inline 3-line comment documenting the forward-looking design intent (Phase 4 will `tx.send(...).await` from this function).
- **Files modified:** `crates/bootroom/src/ws.rs`
- **Verification:** `cargo clippy -p bootroom --all-targets -- -D warnings` exits 0.
- **Committed in:** `206844e` (folded into the Task 1 commit — same logical change set).

---

**Total deviations:** 1 auto-fixed (1 lint)
**Impact on plan:** Zero behavioural change. The signature called for by the plan is preserved.

## Issues Encountered

None. Both task verification gates passed on first run:

- Task 1: `cargo build -p bootroom` exited 0; `cargo clippy --all-targets -- -D warnings` exited 0 after the one inline `allow` for `unused_async`.
- Task 2: `cargo test -p bootroom --test ws_roundtrip` showed 3 passed / 0 failed on first invocation.

The tests passed first try because Task 1 implemented the contract exactly per the plan's `<behavior>` block.

## Authentication Gates

None — `/ws` is unauthenticated by Phase 1's loopback-only design (PROJECT.md non-goal).

## Verification

- `cargo build -p bootroom` — exit 0.
- `cargo clippy -p bootroom --all-targets -- -D warnings` — exit 0.
- `cargo test -p bootroom --test ws_roundtrip` — `3 passed; 0 failed`.
- `cargo test --workspace` — full suite green; no regressions in Phase 1 tests (24 lib tests + the rest of the integration suite). The Phase 1 COOP/COEP-on-404 test still passes alongside the new Phase 2 COOP/COEP-on-/ws test.
- `grep -q "pub mod ws" crates/bootroom/src/lib.rs` — present.
- `grep -q 'route("/ws"' crates/bootroom/src/server.rs` — present.

## Known Stubs

None. `handle_wire` arms that log instead of acting (`SerialIn`, `SerialOut`) are the documented Phase 2 design — the plan's `<behavior>` block explicitly says "logs them at trace level (no echo in Phase 2)". Phase 4 (headless run) is where these arms gain logic, and that's noted in the inline comments. Not a stub; an explicit phase boundary.

## Threat Flags

None. No new surface introduced beyond what the threat register already covers:
- T-02-14 through T-02-21 from the plan's `<threat_model>` are all accounted for in the implementation:
  - T-02-15 (frame flooding) → mpsc capacity 32
  - T-02-16 (malformed JSON panic) → `serde_json::from_str` Err arm logs + continues
  - T-02-17 (client-spoofed server frames) → `State { .. } | Hello { .. }` arm logs warn + continues
  - T-02-18 (middleware stripping COOP/COEP) → `ws_upgrade_response_carries_coop_coep` regression test
  - T-02-19 (base64 decode allocation) → server does not decode in Phase 2 (`data: _`)

## Next Plan Readiness

- Plan 02-04 (browser-side WS client) can connect to `ws://<host>/ws`, expect a first `Hello` frame, and send `SerialIn` / `SerialOut` / `Launch` / `Reset` — server is ready.
- Plan 02-06 (button wiring) can send `WsMessage::Launch` / `Reset` before issuing `window.location.reload()`; the server will log them at `info!` level.
- Phase 4 (`bootroom run` headless driver) can reuse `ws_handler` unchanged; only `handle_wire` will grow logic (e.g., persist `SerialOut` for assertions, drive `Launch` from a scenario step).
- No blockers.

## Self-Check: PASSED

- FOUND: `crates/bootroom/src/ws.rs` (134 lines, contains `pub async fn ws_handler`, `handle_socket`, `handle_wire`, `mpsc::channel::<WsMessage>(32)`, `env!("CARGO_PKG_VERSION")`)
- FOUND: `crates/bootroom/tests/ws_roundtrip.rs` (121 lines, contains `ws_handshake_emits_hello`, `ws_client_serial_in_is_logged_not_echoed`, `ws_upgrade_response_carries_coop_coep`)
- FOUND: `pub mod ws;` in `crates/bootroom/src/lib.rs`
- FOUND: `.route("/ws", axum::routing::any(crate::ws::ws_handler))` in `crates/bootroom/src/server.rs`
- FOUND commit `206844e` (Task 1, feat: handler + route + mod)
- FOUND commit `8537e3b` (Task 2, test: ws_roundtrip integration tests)

---
*Phase: 02-websocket-live-serial*
*Completed: 2026-05-18*
