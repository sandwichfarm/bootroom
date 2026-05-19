---
phase: 04-scenario-engine-headless-run
type: overview
plans: 11
waves: 5
requirements: [RUN-01, RUN-02, RUN-03, RUN-04, RUN-05, RUN-06, RUN-07, RUN-08, RUN-09, RUN-10, CLI-02]
---

# Phase 4: Scenario Engine + Headless `run` — Plan Set Overview

**Goal:** A kernel CI job runs `bootroom run --kernel build/Image --scenario boot_smoke`, gets a 0/1 exit code from serial-output assertions and a full JSONL transcript on failure — driving the **same** embedded assets and **same** `/ws` protocol as `serve` mode via a headless Chromium under chromiumoxide.

This phase is a **composition phase** (per 04-RESEARCH.md "synthesis"). Every load-bearing primitive (`Funnel` lock, COOP/COEP middleware, broadcast forwarder, `LoadedConfig` validation, chromiumoxide launch incantation) already ships. The new surface is: 3 additive `WsMessage` variants, a `web/scenario.js` engine (~300 LoC), a `run_cmd.rs` driver (~250 LoC), a CLI refactor extracting `SharedArgs` via `#[command(flatten)]`, and a small JSONL transcript writer.

## Multi-Source Coverage Audit

Every item below maps to at least one plan. Auditing ALL FOUR source types per the planner contract.

### GOAL (ROADMAP Phase 4 success criteria)
| Success Criterion | Covered By |
|-------------------|-----------|
| `bootroom run --kernel <path> --scenario <name>` drives headless Chromium against same assets/`/ws`, no separate CI codepath | 04-07 (run_cmd driver lifts spike-b incantation; `build_router(state)` reuse pinned by 04-10 router-equality test) |
| Substring + regex assertions over per-action serial buffers; ANSI stripped; line-buffered (`\r?\n`) | 04-02 (regex compile-check), 04-08 (scenario.js evaluate(), ANSI strip, line buffer, per-action `Map`), 04-11 (NORN fixture e2e) |
| Per-action + per-scenario timeouts with structured failures; per-action buffer reset by default | 04-08 (`Promise.race` per-action timeout, buffer reset on action start), 04-10 (timeout shape pin) |
| `crossOriginIsolated` startup self-check; exits 3 if SAB unavailable; pass = 0, fail = non-zero | 04-07 (COI probe via CDP `Runtime.evaluate` BEFORE scenario kickoff; exit-code translation table) |
| `--log-file` JSONL transcript with timestamps, sends, serial chunks, assertion results | 04-06 (JSONL writer + event types), 04-10 (subprocess test asserts JSONL line shape) |
| `--verbose` stderr progress; `--kernel`/`--config`/`--verbose` shared via clap `#[flatten]` | 04-03 (CommonArgs + flatten), 04-06 (stderr formatter), 04-10 (parse/stderr tests) |

### REQ (REQUIREMENTS.md Phase-4 requirement IDs)
| Req | Plan(s) |
|-----|---------|
| RUN-01 (`bootroom run` exits 0/non-zero) | 04-07, 04-10 |
| RUN-02 (chromiumoxide drives Chromium) | 04-07 |
| RUN-03 (same assets + same `/ws` as `serve`) | 04-07, 04-10 |
| RUN-04 (substring + regex assertions per-action) | 04-02, 04-08 |
| RUN-05 (ANSI strip + line-buffered `\r?\n`) | 04-08 |
| RUN-06 (per-action + per-scenario timeouts, structured failures) | 04-07, 04-08 |
| RUN-07 (per-action serial buffer reset by default) | 04-08 |
| RUN-08 (`--log-file` JSONL transcript) | 04-06, 04-07 |
| RUN-09 (`--verbose` stderr progress) | 04-06, 04-07 |
| RUN-10 (COI self-check; abort early if SAB missing) | 04-07 |
| CLI-02 (shared `--kernel`/`--config`/`--verbose` via `#[flatten]`) | 04-03 |

### RESEARCH (04-RESEARCH.md features/constraints)
| Item | Plan(s) |
|------|---------|
| 3 additive `WsMessage` variants — `ScenarioStart` (reserved, unused), `ScenarioResult` (browser→server), `ScenarioAbort` (server→browser) | 04-01 |
| `oneshot::Sender<ScenarioResult>` parked on `AppState` (`Mutex<Option<_>>`, take-once) | 04-04 |
| URL-query run-mode detection (`?scenario=<name>`, precedent: `?pacing=N`) | 04-09 |
| Chromium discovery: `$CHROMIUM` → `/usr/bin/chromium` → `which chromium`; each candidate verified via `--version` exit 0 (Pitfall #6) | 04-07 |
| Chromium launch flags — `--headless=new --no-sandbox --disable-dev-shm-usage --disable-gpu` + `$BOOTROOM_CHROMIUM_ARGS`; lift spike-b verbatim (Pitfall #7) | 04-07 |
| COI self-check via CDP `Runtime.evaluate("self.crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'")` | 04-07 |
| Assertion engine: ANSI strip `/\x1b\[[0-9;]*[A-Za-z]/g`; line-buffered match up to last `\r?\n`; partial line at timeout only | 04-08 |
| Regex flavor pinned to Rust-`regex` ∩ JS `RegExp` (no backrefs, no lookaround) — document in 04-02 | 04-02, 04-08 |
| `after = "<label>"` → per-action `Map.get(label)`; `after = "any"` → secondary flat append buffer (Pitfall #5) | 04-08 |
| WS flush race: `bufferedAmount === 0` poll after `ws.send(ScenarioResult)` (Pitfall #2) | 04-08 |
| `master.onWrite` disposable cleanup in `finally` (Pitfall #4) | 04-08 |
| `funnel.enqueue` is lock-agnostic (Pitfall #3) — don't add lock guard inside funnel | 04-08 (asserted in pitfall section) |
| JSONL events: `scenario_start`, `action_send`, `serial_chunk`, `assertion_result`, `scenario_result`; UTC ISO 8601 with `Z` suffix (Open Q3) | 04-06 |
| Exit codes 0/1/2/3 + outer timeout = `scenario.timeout_ms + 30_000` (Pitfall #8) | 04-07 |
| Per-action timeout default = MAX(assertion.timeout_ms); fallback 5_000 ms when zero assertions (Open Q5) | 04-08 |
| `--verbose` glyphs ASCII-only (`>`, `+`, `-`) for cross-platform CI (Open Q4) | 04-06 |
| Browser-side defensive scenario-name check (Open Q2: defense-in-depth) | 04-08 |
| chromiumoxide 0.9.1 (Apache-2.0/MIT) + futures 0.3 + regex 1 promoted into `bootroom`/`bootroom-core` `[dependencies]` | 04-02, 04-07 |
| Wave-0 test files listed in 04-RESEARCH "Wave 0 Gaps" | 04-10, 04-11 |

### CONTEXT (04-CONTEXT.md locked decisions)
| Decision | Plan(s) |
|----------|---------|
| Browser-side engine in `web/scenario.js` (sibling of `app.js`) | 04-08 |
| Per-action `Map<label, Uint8Array[]>`; reset on action start | 04-08 |
| `ScenarioResult` carries verdict + per-action verdicts + per-assertion verdicts + transcript | 04-01, 04-08 |
| In-process axum server bound `127.0.0.1:0`; chromiumoxide-driven Chromium; oneshot await; shutdown | 04-04, 04-05, 04-07 |
| Chromium discovery + launch-flag set | 04-07 |
| COI self-check via `Runtime.evaluate` BEFORE scenario kickoff; same hint as UI banner | 04-07 |
| ANSI strip + line buffer; Rust `regex` for compile, JS `RegExp` for runtime | 04-02, 04-08 |
| `after = "<label>"` vs `after = "any"` (union, line-ordered) | 04-08 |
| Flags `--kernel`, `--config`, `--scenario`, `--verbose`, `--log-file`; `--kernel`/`--config`/`--verbose` shared via `#[flatten]` | 04-03 |
| Exit codes 0/1/2/3 (pass/scenario-fail/config-error/startup-error) | 04-07 |
| JSONL `--log-file` event types | 04-06 |
| `--verbose` per-action progress + verdicts + final summary; non-verbose silent on success, one-line on fail | 04-06 |
| Funnel lock primitive used: `funnel.lockInput()` at scenario start, `funnel.unlockInput()` on completion | 04-08 |
| Same `index.html`/`app.js`/`funnel.js`; URL query `?scenario=<name>` is run-mode entry point | 04-09 |

**Verdict:** Zero unplanned items. No phase split required. No deferred items snuck into plans.

## Dependency Graph

```
              ┌─────────────────────────────────────┐
              │ 04-01  bootroom-core: WsMessage     │
              │   ScenarioStart/Result/Abort + tests│
              └────────┬────────────────────────────┘
                       │
        ┌──────────────┼──────────────────────────────────────┐
        ▼              ▼                                      ▼
  ┌──────────┐  ┌─────────────────────┐         ┌──────────────────────┐
  │ 04-02    │  │ 04-03               │         │ 04-04                │
  │ regex    │  │ CLI: SharedArgs +   │         │ AppState:            │
  │ compile- │  │ Cmd::Run(RunArgs) + │         │ scenario_result_tx   │
  │ check    │  │ refactor ServeArgs  │         │ Mutex<Option<_>>     │
  └─────┬────┘  └──────────┬──────────┘         └──────────┬───────────┘
        │                  │                               │
        │                  │                               ▼
        │                  │                    ┌──────────────────────┐
        │                  │                    │ 04-05                │
        │                  │                    │ ws.rs: ScenarioResult│
        │                  │                    │ handler + Abort log  │
        │                  │                    └──────────┬───────────┘
        │                  │                               │
        │                  ▼                               │
        │       ┌────────────────────────┐                 │
        │       │ 04-06                  │                 │
        │       │ JSONL writer + verbose │                 │
        │       │ stderr formatter       │                 │
        │       └─────────────┬──────────┘                 │
        │                     │                            │
        └──────────┬──────────┴──────────┬─────────────────┘
                   │                     │
                   ▼                     ▼
        ┌──────────────────────────────────────────────────┐
        │ 04-07  run_cmd.rs                                │
        │  - chromium discovery (Pitfall #6)               │
        │  - in-process axum boot on 127.0.0.1:0           │
        │  - chromiumoxide launch (Pitfall #7)             │
        │  - COI self-check via Runtime.evaluate (RUN-10)  │
        │  - oneshot await + outer timeout (Pitfall #8)    │
        │  - exit-code translation 0/1/2/3                 │
        │  + chromiumoxide/futures dep wiring              │
        └────────────────────────┬─────────────────────────┘
                                 │
                                 ▼
        ┌──────────────────────────────────────────────────┐
        │ 04-08  web/scenario.js                           │
        │  - sequencer over actions[]                      │
        │  - per-action Map<label, Uint8Array[]>           │
        │  - secondary flat buffer for `after="any"`       │
        │  - ANSI strip + line-buffered evaluate           │
        │  - per-action timeout via Promise.race           │
        │  - funnel.lockInput/unlockInput                  │
        │  - ws.send(ScenarioResult) + bufferedAmount poll │
        │  - master.onWrite disposable cleanup (Pitfall #4)│
        └────────────────────────┬─────────────────────────┘
                                 │
                                 ▼
        ┌──────────────────────────────────────────────────┐
        │ 04-09  app.js wire-up                            │
        │  - detect URLSearchParams.get('scenario')        │
        │  - dynamic-import scenario.js after WS Hello +   │
        │    initial /api/config load                      │
        └────────────────────────┬─────────────────────────┘
                                 │
                  ┌──────────────┴──────────────┐
                  ▼                             ▼
        ┌─────────────────────┐      ┌────────────────────────────┐
        │ 04-10               │      │ 04-11                      │
        │ Integration tests:  │      │ E2E NORN fixture           │
        │ - exit codes        │      │ (#[ignore]):               │
        │ - same router       │      │ - chromium + real kernel   │
        │ - --log-file shape  │      │ - boot_smoke scenario      │
        │ - --verbose stderr  │      │ - banner assertion         │
        └─────────────────────┘      └────────────────────────────┘
```

## Wave Structure

| Wave | Plans | Rationale |
|------|-------|-----------|
| 1 | 04-01 | Pure types in `bootroom-core`. No dependencies. Unblocks every consumer. |
| 2 | 04-02, 04-03, 04-04 | Independent of each other: 04-02 extends `bootroom-core::config` (regex compile-check), 04-03 refactors `bootroom::cli`, 04-04 extends `bootroom::state`. All depend on 04-01. |
| 3 | 04-05, 04-06 | 04-05 needs 04-01 (`ScenarioResult` variant) + 04-04 (the oneshot field). 04-06 needs 04-01 only but is grouped here because run_cmd (04-07) needs both as immediate priors. |
| 4 | 04-07, 04-08 | 04-07 (Rust driver) needs 04-02/03/04/05/06. 04-08 (JS engine) needs 04-01 (wire shape) + 04-09 spec lock-in. They touch disjoint files (`crates/bootroom/src/run_cmd.rs` vs `crates/bootroom/web/scenario.js`) so they run in parallel. |
| 5 | 04-09, 04-10, 04-11 | 04-09 wires 04-08 into `app.js`. 04-10 is subprocess-style tests over the assembled Rust surface. 04-11 is the e2e NORN gate (`#[ignore]`). All three touch disjoint files. |

## Plan Index

- **04-01** — Add `ScenarioStart` (reserved) / `ScenarioResult` (browser→server) / `ScenarioAbort` (server→browser) variants to `WsMessage`; add `Verdict` + `ActionResult` + `AssertionResult` companion structs; roundtrip tests. [`RUN-01..RUN-09`]
- **04-02** — Extend `LoadedConfig` (or new `Assertion::validate_regex()`) to compile-check `kind = "regex"` patterns at load time via the `regex` crate; promote `regex` to a direct `bootroom-core` dep; document the Rust∩JS regex feature intersection. [`RUN-04, RUN-05`]
- **04-03** — Extract a `SharedArgs` struct (`--kernel`, `--config`, `--verbose`) with `#[command(flatten)]`; add `Cmd::Run(RunArgs)` variant carrying `--scenario`, `--log-file`; refactor `ServeArgs` to use the shared flatten; backward-compat tests pin existing Phase-2/3 invocations. [`CLI-02`]
- **04-04** — Extend `AppState` with `scenario_result_tx: Mutex<Option<oneshot::Sender<ScenarioResult>>>`; preserve `new_for_test` / `new_for_test_with_loaded` shape; unit-test take-once semantics. [`RUN-01`]
- **04-05** — Extend `ws.rs::handle_wire` to take the oneshot and send on `ScenarioResult`; log+continue on `ScenarioAbort` (server-owned variant arriving from client); integration test over real WS roundtrip. [`RUN-01, RUN-03`]
- **04-06** — Define JSONL transcript event shapes + writer (`bootroom::transcript`); define a verbose stderr formatter (ASCII glyphs); unit tests for serialization stability + verbose line shape. [`RUN-08, RUN-09`]
- **04-07** — `crates/bootroom/src/run_cmd.rs`: chromium discovery with `--version` probe (Pitfall #6); in-process axum boot on `127.0.0.1:0`; chromiumoxide launch (lift spike-b verbatim, Pitfall #7); COI self-check via `Runtime.evaluate` (RUN-10); oneshot await with outer timeout = `scenario.timeout_ms + 30_000` (Pitfall #8); exit-code translation 0/1/2/3; transcript persistence; verbose summary. Promote `chromiumoxide 0.9.1` + `futures 0.3` to `crates/bootroom` `[dependencies]`. [`RUN-01..03, RUN-06, RUN-10`]
- **04-08** — `crates/bootroom/web/scenario.js`: action sequencer using `funnel.enqueue`; per-action `Map<label, Uint8Array[]>`; secondary flat buffer for `after = "any"` (Pitfall #5); ANSI strip + line-buffered evaluate; per-action timeout via `Promise.race`; `funnel.lockInput()` at start / `unlockInput()` in `finally`; `ws.send(ScenarioResult)` + `bufferedAmount === 0` poll (Pitfall #2); `master.onWrite` `Disposable` cleanup (Pitfall #4); defensive scenario-name check. [`RUN-04..07`]
- **04-09** — Extend `app.js` `handleWsFrame` Hello branch: detect `URLSearchParams.get('scenario')`; dynamic-import `./scenario.js` after `initialConfigLoad()` resolves; hand off `(scenario, actions, { ws, funnel, master })`. [`RUN-01, RUN-03`]
- **04-10** — Subprocess integration tests (no real chromium): `cli_subcommands.rs` extension for parse-shape; `run_log_file_jsonl.rs` for `--log-file` event shape pin via stub scenario; `run_verbose_stderr.rs` for stderr line glyphs; `run_uses_same_router.rs` for `build_router(state)` reuse pin. [`CLI-02, RUN-03, RUN-08, RUN-09`]
- **04-11** — `#[ignore]`-tagged `run_smoke_norn_kernel.rs`: real chromium + Spike-B `fixtures/Image`; runs `bootroom run --kernel spike-b/fixtures/Image --scenario boot_smoke` against a generated `bootroom.toml` declaring `boot_smoke` with a known-NORN-banner `contains` assertion; asserts exit code 0 + transcript event sequence. Phase-gate test (mirrors spike-b harness; runs under `cargo test --workspace -- --ignored`). [`RUN-01, RUN-02, RUN-04, RUN-05`]

## Threat Model (Phase-Scoped)

| Boundary | Description |
|----------|-------------|
| CLI argv → `bootroom run` | Operator-controlled; values flow into URL query, file paths, JSON events. |
| Browser → `/ws` | New `ScenarioResult` frame surface; existing CR-02 Origin allow-list applies unchanged. |
| `bootroom run` → Chromium subprocess | New surface: argv constructed from `$BOOTROOM_CHROMIUM_ARGS` + discovery results. |
| `bootroom run` → `--log-file` path | New write surface; operator-controlled path. |

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-04-01 | Tampering | `--scenario` arg as URL query | accept | clap parses as String; value flows into `URLSearchParams` (browser percent-decodes); no shell concatenation; covered by 04-09 query-detection test. |
| T-04-02 | DoS | Pathological regex in `bootroom.toml` (browser-side, since JS `RegExp` is backtracking) | mitigate | Per-assertion `timeout_ms` is a natural circuit-breaker (default 5000 ms); 04-08 per-action timeout via `Promise.race` enforces it. Rust-side regex compile-check (04-02) rejects malformed patterns before they reach the browser. |
| T-04-03 | Tampering | `$BOOTROOM_CHROMIUM_ARGS` env-var injection into Chromium argv | accept | Env var is operator-controlled — same trust model as `--kernel`. Document in 04-07 task action as "trusts the same operator as `--kernel`." |
| T-04-04 | Resource exhaustion (self) | Chromium process leak on `bootroom run` panic / error path | mitigate | 04-07 wraps `browser` in a Drop-style RAII guard that calls `browser.close().await + browser.wait().await + handler.abort()` (lift spike-b cleanup; spike-b/src/main.rs:240-243). Required `<verify>` step: kill `bootroom run` with SIGTERM mid-scenario, verify no orphan `chromium` process via `pgrep -f bootroom-test`. |
| T-04-05 | Info disclosure | `--log-file` JSONL contains base64-encoded serial bytes (kernel output) | accept | Operator-controlled path; this is the operator's own kernel output. No PII surface; matches CR-02 trust model (loopback dev tool). |
| T-04-06 | DoS | Stale `/usr/bin/chromium` symlink to missing target (Pitfall #6) | mitigate | 04-07: each discovery candidate verified via `Command::new(cand).arg("--version").output()`; failing exit fall-through to next candidate. Exit 3 lists every candidate tried + per-candidate error. |
| T-04-07 | Spoofing | `ScenarioResult` frame arriving in `serve` mode (no `bootroom run` driver awaiting) | mitigate | 04-05 match arm: `state.scenario_result_tx.lock().await.take()` is `None` in serve mode → warn-and-continue (matches existing `<deferred>` recovery posture in `ws.rs`). |
| T-04-08 | Tampering | WS flush race: `ws.send(ScenarioResult)` returns before TCP send, browser navigates → server times out (Pitfall #2) | mitigate | 04-08 `await` a `Promise` that polls `ws.bufferedAmount === 0` (then a final microtask) before resolving. Outer Rust timeout (Pitfall #8) is the safety net; diagnostic distinguishes "no serial output seen" vs "result frame missing." |

## Risk Concentration

The phase is mostly mechanical; the two real risks are isolated to specific tasks:

1. **Pitfall #2 — WS flush race** (T-04-08): localized to 04-08. The `bufferedAmount === 0` poll is the recommended mitigation, and the outer Rust timeout in 04-07 is the backstop. If 04-11 turns out flaky on the green-path NORN fixture, this is the first place to look.
2. **Pitfall #6 — chromium discovery on stale symlinks** (T-04-06): localized to 04-07. Verified-via-`--version` candidate probe is the mitigation. CI runners on minimal images may hit this; the diagnostic must list every candidate + per-candidate error.

Both pitfalls are covered by explicit task actions in 04-07 / 04-08. Neither blocks dependency order.

## Notes for Executor

- **Phase 1 Spike B is the reference implementation.** When in doubt about chromiumoxide 0.9.1 API surface, read `crates/bootroom/spikes/spike-b/src/main.rs` rather than chromiumoxide docs (the latter may reflect a different minor; Spike B is pinned).
- **Phase 3 `funnel.lockInput()` is the load-bearing primitive.** Plan 04-08 explicitly calls it on scenario start and `unlockInput()` in a `finally` block. Do NOT modify `funnel.js` itself; the lock-agnostic `enqueue` contract is documented inline (`crates/bootroom/web/funnel.js:53-67`) and Pitfall #3 in 04-RESEARCH explains why.
- **Additive variants only.** No existing `WsMessage` variant changes; no `deny_unknown_fields` on the enum (the Phase-2 deliberate omission survives). All new wire shapes are append-only.
- **`build_router(state)` reuse is the architectural pin for RUN-03.** Plan 04-10's `run_uses_same_router.rs` is the regression test that locks "no separate CI code path." Do not introduce an alternative router constructor.
- **`bootroom.toml` for 04-11 must declare `boot_smoke` with a known-NORN banner assertion.** Read the Spike B SUMMARY (`crates/bootroom/spikes/SPIKE-B-RESULT.md`) for the actual NORN serial output observed; the assertion pattern is derived from that observation, not invented.

---
*Phase plan set created: 2026-05-19 via gsd-planner under `/gsd-plan-phase`.*
