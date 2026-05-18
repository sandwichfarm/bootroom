---
phase: 2
slug: websocket-live-serial
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-18
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test`. Phase 1 established `TestServer` harness at `crates/bootroom/tests/common/mod.rs`. |
| **Config file** | `Cargo.toml` `[dev-dependencies]` |
| **Quick run command** | `cargo test -p bootroom --lib && cargo test -p bootroom-core --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5s warm-cache; ~30s full suite |

Headless browser smoke: Spike B's chromiumoxide harness (already green per Phase 1).

---

## Sampling Rate

- **Per task commit:** `cargo test -p bootroom-core --lib && cargo test -p bootroom --lib`
- **Per wave merge:** `cargo test --workspace` + manual headed-browser smoke (all four UI flows)
- **Phase gate:** Full suite green + manual smoke vs NORN kernel + Spike B chromiumoxide harness re-run

---

## Per-Task Verification Map

| Req ID | Behavior | Test Type | Automated Command | File Exists | Status |
|--------|----------|-----------|-------------------|-------------|--------|
| WS-04 | `WsMessage` round-trips through serde-tagged JSON | unit | `cargo test -p bootroom-core --lib serial_in_roundtrip` (+ siblings) | ❌ Wave 0 | ⬜ pending |
| WS-01 | `/ws` accepts upgrade, emits Hello, accepts SerialIn | integration | `cargo test -p bootroom --test ws_roundtrip ws_handshake_emits_hello` | ❌ Wave 0 | ⬜ pending |
| WS-01 (regression) | WS upgrade carries COOP+COEP | integration | `cargo test -p bootroom --test ws_roundtrip ws_upgrade_response_carries_coop_coep` | ❌ Wave 0 | ⬜ pending |
| WS-02 | Single-writer funnel (no byte interleaving) | unit JS + smoke | Documented manual test for funnel.js + headed-browser smoke | ❌ Wave 0 | ⬜ pending |
| WS-03 | Inter-character pacing on SerialIn | manual + log inspect | DevTools trace of 10-byte SerialIn with `?pacing=50` | manual-only | ⬜ pending |
| SERV-06 | `--no-open` suppresses browser; default opens | integration (subprocess) | `cargo test -p bootroom --test serve_no_open` | ❌ Wave 0 | ⬜ pending |
| UI-02 | xterm renders live serial via xterm-pty slave | manual + headless smoke | Visual; Spike B harness asserts on serial bytes | manual-only | ⬜ pending |
| UI-03 | Keystrokes reach guest | manual | Log into NORN, type, observe responses | manual-only | ⬜ pending |
| UI-04 | Clear empties terminal; Copy populates clipboard | manual | Click Clear; click Copy + paste | manual-only | ⬜ pending |
| UI-06 | Status pill cycles Idle→Loading→Running→Halted | manual + console | Observe transitions on page load + kernel exit | manual-only | ⬜ pending |
| UI-08 | Launch reloads + boots fresh kernel | manual | Modify kernel, click Launch, observe new boot | manual-only | ⬜ pending |
| UI-09 | Reset reloads + boots same kernel | manual | Click Reset, observe reload+boot | manual-only | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/bootroom-core/src/lib.rs` — replace skeleton with `WsMessage` / `GuestState` + `#[cfg(test)]` round-trip tests
- [ ] `crates/bootroom/src/ws.rs` — new WS handler module
- [ ] `crates/bootroom/tests/ws_roundtrip.rs` — three tests (hello, serial_in, COOP/COEP upgrade)
- [ ] `crates/bootroom/tests/serve_no_open.rs` — SERV-06 subprocess test
- [ ] `crates/bootroom/web/funnel.js` — Funnel + bytesToB64 / b64ToBytes / keyEventToBytes helpers
- [ ] Inline manual test plan in `funnel.js` doc comment (no JS runner in Phase 2)

No external runner install required.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live keystroke flow | UI-03 | Requires real guest interaction | Open page, log into NORN shell, type commands, verify responses |
| Status pill transitions | UI-06 | Visual state machine | Page load → IDLE → LOADING → RUNNING (first byte) → HALTED (on exit) |
| Launch / Reset buttons | UI-08, UI-09 | Browser primitive (location.reload) | Click each; observe page reload + fresh boot |
| Clear / Copy controls | UI-04 | Clipboard interaction | Click Clear → terminal empty; click Copy → paste verifies content |
| Inter-character pacing | WS-03 | Timing observable only at runtime | Inject 10-byte SerialIn with `?pacing=50`; DevTools console.trace shows delta |
| Browser auto-open default | SERV-06 | Spawns external process | Run `bootroom serve --kernel /tmp/k` without `--no-open`; verify browser opens |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
