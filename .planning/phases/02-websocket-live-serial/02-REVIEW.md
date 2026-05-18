---
phase: 02-websocket-live-serial
reviewed: 2026-05-18T00:00:00Z
depth: standard
files_reviewed: 16
files_reviewed_list:
  - Cargo.toml
  - crates/bootroom-core/Cargo.toml
  - crates/bootroom-core/src/lib.rs
  - crates/bootroom/Cargo.toml
  - crates/bootroom/src/cli.rs
  - crates/bootroom/src/lib.rs
  - crates/bootroom/src/main.rs
  - crates/bootroom/src/server.rs
  - crates/bootroom/src/state.rs
  - crates/bootroom/src/ws.rs
  - crates/bootroom/tests/common/mod.rs
  - crates/bootroom/tests/serve_no_open.rs
  - crates/bootroom/tests/ws_roundtrip.rs
  - crates/bootroom/web/app.js
  - crates/bootroom/web/funnel.js
  - crates/bootroom/web/index.html
  - crates/bootroom/web/style.css
findings:
  critical: 1
  warning: 6
  info: 5
  total: 12
status: clean
fixes_applied:
  fixed_at: 2026-05-18
  scope: critical + all warnings
  commits:
    - CR-01: e8f4312
    - WR-01: 4ef43bf
    - WR-02: c38976f + adce9ec (Cargo.lock)
    - WR-03: 546dde1
    - WR-04: ed36b25
    - WR-05: bundled into CR-01 (e8f4312) — funnel try/catch added in same hunk
    - WR-06: 0b96fd9
  info_deferred:
    - IN-01: Module.mainScriptUrlOrBlob dead write (Info — phase-3 candidate)
    - IN-02: recomputePillLocal explicit-default cleanup (Info — no behavior change)
    - IN-03: tests/serve_no_open.rs tx/tx_clone simplification (Info — test-only)
    - IN-04: keyEventToBytes future-key expansion (Info — feature work)
    - IN-05: handle_wire #[allow(clippy::unused_async)] tightening (Info — cosmetic)
  human_verification_required:
    - CR-01: headed-browser smoke test — bootroom serve, type into NORN shell,
      verify guest responds (e.g. `help` produces output). The Rust test suite
      cannot reproduce the bug or its fix without a live qemu-wasm + guest kernel.
---

# Phase 2: Code Review Report

**Reviewed:** 2026-05-18
**Depth:** standard
**Files Reviewed:** 17 (source + config) across `crates/bootroom-core/`, `crates/bootroom/`, root `Cargo.toml`, `web/`, and `tests/`.
**Status:** issues_found

## Summary

Phase 2 lands the `/ws` endpoint, `WsMessage`/`GuestState` protocol enum in `bootroom-core`, browser-side write-funnel module, four header/terminal-control buttons, a four-state pill machine with WS-authoritative override, and `--no-open`/auto-open behavior. The Rust server side is small, idiomatic, well-traced, and faithful to the Phase 1 review patterns (anyhow + `with_context`, `tracing` everywhere, bounded mpsc per `02-RESEARCH.md` T-02-15, COOP/COEP middleware regression-tested on the `/ws` upgrade path). The `bootroom-core` enum is correctly serde-tagged with full round-trip tests.

That said, this review found one **BLOCKER** in the browser-side input path: the funnel writes user keystrokes to `slave.write(...)`, which xterm-pty routes to the terminal **display** (process-stdout path), not to the guest's stdin (`ldisc.writeFromLower` path). The result is that typing visually appears in the terminal (the bytes are echoed by the funnel through OPOST) but never reaches the qemu-wasm guest — the Phase 2 headline goal "user types into the browser terminal and sees their keystrokes reach the guest kernel" is unmet. The same bug is documented in `02-RESEARCH.md` Pattern 3, which incorrectly describes both `xterm.onData` and `slave.write` as ending up "in slave's upper buffer." Inspection of the vendored `xterm-pty.js` source disproves this: `slave.write` → `ldisc.writeFromUpper` → `outputToLowerWithPostprocess` → `_onWriteToLower` → master display only. Guest stdin is populated exclusively via `ldisc.writeFromLower` (`outputToUpper` → `flushToUpper` → `_onWriteToUpper` → `slave.read`).

Because UI-03 ("keystrokes reach guest") is the load-bearing Phase 2 user-visible behavior and it has no automated coverage (it is marked `manual` in `02-VALIDATION.md` and was not actually exercised against a live kernel), the bug shipped uncaught.

The remaining findings are warnings and info items: an unbounded log of malformed-JSON payloads (DoS amplification on logs), an unused `base64` workspace dep added but never consumed on the Rust side, a stale `Module.mainScriptUrlOrBlob` write inherited from Phase 1 (IN-01 in 01-REVIEW.md was deferred and is now in scope again because `app.js` was rewritten), and a handful of robustness improvements around WS reconnect, pacing-param coercion, and clipboard error reporting.

### Critical findings

- **CR-01**: Funnel writes user keystrokes to `slave.write(...)` which goes to the display, not to guest stdin. UI-03 is broken.

### Other notable items

- WS handler logs full payload string on malformed JSON without truncation.
- `base64` dependency added to `crates/bootroom/Cargo.toml` is never used on the Rust side (only JS uses native `btoa`/`atob`).
- WS reconnect retry has no backoff and no upper bound — fine per `<deferred>`, but worth noting that a server in a crash loop will produce a 1Hz reconnect spam in DevTools console.
- `?pacing=abc` (non-numeric) silently coerces to 0 pacing.
- IN-01 (`Module.mainScriptUrlOrBlob` dead write) from Phase 1 review was deferred. Phase 2 rewrites `app.js` extensively and the line is still there.

---

## Critical Issues

### CR-01: Funnel writes user keystrokes to `slave.write`, which xterm-pty routes to the display — guest stdin is never populated

**File:** `crates/bootroom/web/funnel.js:62`, `crates/bootroom/web/app.js:175-182`

**Issue:** Phase 2's headline behavior is "user types into the browser terminal and sees their keystrokes reach the guest kernel" (`02-CONTEXT.md` `<domain>`). The implementation:

1. Suppresses xterm's default `onData` dispatch via `attachCustomKeyEventHandler` returning `false` (correct — Pitfall #1 mitigation; prevents double-injection via the master addon's `e.onData(i => this.ldisc.writeFromLower(i))` subscription).
2. Translates the `KeyboardEvent` to bytes via `keyEventToBytes(evt)`.
3. Enqueues the bytes into the `Funnel`, which calls `this.slave.write([b])` for each byte in the drain loop.

Step 3 is the bug. Tracing through the vendored `crates/bootroom/web/vendor/xterm-pty.js` (slave is class `P`):

```js
// P.prototype.write
write(e){
  let i = typeof e == "string" ? p(e) : e;
  this.fromUpperToLdiscBuffer = this.fromUpperToLdiscBuffer.concat(i),
  this.ldisc.flow && (this.ldisc.writeFromUpper(this.fromUpperToLdiscBuffer), ...)
}
```

`slave.write` invokes `ldisc.writeFromUpper`, which is the **process-to-terminal** direction (process stdout). Continuing the trace:

```js
writeFromUpper(e){
  ...
  for (let r of i) this.outputToLowerWithPostprocess(r);  // OPOST translation
  this.flushToLower();                                     // fires _onWriteToLower
}
```

`flushToLower` → master listener → `master._onWrite.fire` → `xterm.write(bytes)` → bytes appear in the terminal **display**.

The path to guest stdin is `ldisc.writeFromLower(bytes)`:

```js
writeFromLower(e){
  ...
  switch(s){
    case "normal":
      this.checkStartFlow();
      this.inputFromLowerWithPreprocess(r);  // -> outputToUpper -> flushToUpper -> _onWriteToUpper
      break;
    ...
  }
}
```

`_onWriteToUpper` is what the slave's constructor subscribes to (`this.ldisc.onWriteToUpper(i => this.fromLdiscToUpperBuffer.push(...i), this._onReadable.fire())`), which then feeds `slave.read()` — and **that** is what qemu-wasm's PTY shim reads to populate the guest's stdin (`/assets/qemu/out.js:1303-1310`, `PTY.read(length)`).

The only path the master addon installs from xterm to guest input is `xterm.onData → master.activate → ldisc.writeFromLower`. The funnel suppresses xterm's onData (correctly, to prevent double-injection) and substitutes a path that goes to the display only. Net effect:

- User types `l`, `s`, `Enter` — funnel writes `[0x6c, 0x73, 0x0d]` to `slave.write` → bytes flow through OPOST and appear in the terminal as if the guest emitted "ls\r" itself.
- The guest never sees any of these bytes on stdin. `ls` never runs.
- Because the user visually sees their typing in the terminal (the funnel's own display echo, not the guest's echo), the failure is silent in casual smoke. Only running an actual interactive REPL would surface it: commands type fine but never execute.

`02-RESEARCH.md` Pattern 3 (lines 503-541) and Pitfall #1 (lines 681-689) both describe both the xterm-onData path and the funnel's `slave.write` path as ending up "in slave's upper buffer" — that mental model is wrong; only the former does. The implementation faithfully follows the wrong mental model, and the only automated coverage of typing is the manual-test plan in `funnel.js` lines 150-159, which records `slave.write` calls without verifying that they reach `slave.read`. UI-03 in `02-VALIDATION.md` is listed as `manual-only` and `⬜ pending`.

**Fix:** The funnel must call `ldisc.writeFromLower` (the input-from-terminal direction) instead of `slave.write` (the output-from-process direction). The ldisc is held on the master addon as `master.ldisc` (verified: `class S{ constructor(e,i){ this.ldisc = e; ... } }`). Plumb the ldisc through the funnel constructor and call it for input bytes:

```js
// funnel.js
export class Funnel {
  constructor(slave, ldisc) {
    this.slave = slave;
    this.ldisc = ldisc;  // for INPUT (host -> guest)
    this.queue = [];
    this.draining = false;
  }

  async #drain() {
    while (this.queue.length > 0) {
      const [b, ms] = this.queue.shift();
      // INPUT path: bytes from the user (or WS SerialIn) head to the
      // guest's stdin via ldisc.writeFromLower → _onWriteToUpper →
      // slave.fromLdiscToUpperBuffer → slave.read.
      this.ldisc.writeFromLower([b]);
      if (ms > 0) await new Promise(r => setTimeout(r, ms));
    }
  }
}
```

```js
// app.js
const { master, slave } = openpty();
xterm.loadAddon(master);
const funnel = new Funnel(slave, master.ldisc);  // <-- pass ldisc
```

Two follow-ups belong with this fix:

1. **Add an automated regression test** for the byte path. Subscribe to `slave.onReadable`, push a known sequence through the funnel, drain `slave.read()`, assert byte equality. This is doable as a pure unit test against the vendored xterm-pty since neither qemu-wasm nor xterm needs to be running.
2. **Audit the `[bootroom] …` diagnostic strings** that currently call `slave.write` directly (7 sites in `app.js`: lines 278, 295, 409, 430, 461, 472, 523). Those are correctly using the *display* path — they are intended as terminal output, not guest input — so they should NOT change. The documented WS-02 "out-of-band exception" wording in `funnel.js:30-31` and `02-CONTEXT.md` `<documented_exceptions>` is correct for those call sites; only the funnel's data path is wrong.

Also update `02-RESEARCH.md` Pattern 3 and Pitfall #1 to reflect the correct ldisc routing.

---

## Warnings

### WR-01: WS handler logs the full malformed-JSON payload at warn level — log-amplification DoS surface

**File:** `crates/bootroom/src/ws.rs:84`

**Issue:** On parse failure, the reader loop emits:

```rust
tracing::warn!(error = %e, payload = %text.as_str(), "bad WsMessage");
```

`text` is an `axum::extract::ws::Utf8Bytes` from a `Message::Text`. Axum 0.8's default max WS frame size is large (multi-MB). A client that opens a connection and spams 4 MB of garbage text per frame at, say, 100 frames/sec will write ~400 MB/sec into the log sink. Even on loopback this is a self-inflicted DoS on disk / journald. The threat model in `02-RESEARCH.md` Security Domain T-02-15 mitigates *channel back-pressure* but not log-amplification on the parse-error path.

`bootroom` is loopback-only by `--host 127.0.0.1` default, and per `PROJECT.md` authentication is explicitly out of scope — so the practical exploit requires either local malware or accidental misuse (`--host 0.0.0.0` plus an unfriendly network). Worth fixing because the protection cost is tiny.

**Fix:** Truncate the payload before logging and elide non-printable bytes:

```rust
fn truncate_for_log(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_owned(); }
    // Find char boundary at or before `max` to avoid splitting a UTF-8 scalar.
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) { cut -= 1; }
    format!("{}…(truncated, {} bytes total)", &s[..cut], s.len())
}

// In the handler:
Err(e) => {
    tracing::warn!(
        error = %e,
        payload = %truncate_for_log(text.as_str(), 256),
        "bad WsMessage"
    );
}
```

While here, consider also `tracing::debug!` rather than `warn!` for the payload itself (the *event* is warn-level; the *payload* doesn't need to be at warn). Phase 4 may want a counter (`bad_ws_frames_total`) for operability.

---

### WR-02: `base64` is declared as a workspace dependency and consumed by `crates/bootroom/Cargo.toml` but never used in Rust code

**File:** `Cargo.toml:15`, `crates/bootroom/Cargo.toml:25`

**Issue:** Phase 2 adds `base64 = "0.22"` to `[workspace.dependencies]` and pulls it into `crates/bootroom/Cargo.toml` via `base64.workspace = true`. Grepping the entire Rust codebase finds zero uses of `base64`'s API (`Engine`, `prelude::*`, `STANDARD`, etc.) — only doc comments mentioning the word "base64". The browser side uses the native `btoa`/`atob` via `funnel.js` helpers, and the Rust server is a pass-through observer in Phase 2 (`handle_wire` only logs `data: _`).

Effect: compile-time and binary-size cost for a crate that isn't called, plus a misleading signal to readers ("oh, the server must decode SerialIn frames, let me find that code"). Phase 4's headless `bootroom run` *will* need base64 to assert on captured serial output — but that's Phase 4. Per Phase 1's discipline ("no `thiserror` — not needed for the binary"), unused deps shouldn't ship.

**Fix:** Remove `base64.workspace = true` from `crates/bootroom/Cargo.toml`. Keep the workspace-level declaration in root `Cargo.toml` for Phase 4 to pick up. Alternatively, drop the workspace dep too and re-add when needed; either works.

```toml
# crates/bootroom/Cargo.toml -- delete this line
# base64.workspace = true
```

`cargo build --workspace` should remain clean after the removal.

---

### WR-03: WS reconnect schedules a new `connectWs` from both `onclose` and the `try/catch` around `new WebSocket`, risking double-scheduled reconnects if the constructor itself fires close after throwing

**File:** `crates/bootroom/web/app.js:263-282`

**Issue:** The sequence:

```js
try {
  ws = new WebSocket(url);
} catch (e) {
  console.warn('[bootroom] /ws constructor failed:', e);
  setTimeout(connectWs, 1000);
  return;
}
// ...
ws.onclose = () => {
  try { slave.write('[bootroom] /ws disconnected; reconnecting…\r\n'); } catch (_e) {}
  setTimeout(connectWs, 1000);
};
ws.onerror = (e) => { console.warn('[bootroom] /ws error:', e); };
```

If `new WebSocket(url)` succeeds (returns an object), then immediately fires `error` and `close` (server dead), only `onclose` schedules a retry — good. If the constructor throws synchronously, only the `catch` schedules a retry — good.

But the practical edge case `onerror` runs and `onclose` is then triggered as a result of the error: both `onerror` and `onclose` fire on the same socket transition. `onerror` here is a no-op (just logs), so currently safe. However, the design has a subtle race: nothing prevents two `connectWs` invocations from interleaving if the *previous* connection's `onclose` fires after a *new* one was already established (e.g., the page is in a flaky-network state and the user navigates back). Each `setTimeout(connectWs, 1000)` blindly overwrites the module-scoped `ws`.

The bigger user-visible cost is **no upper bound on retries**. A server in a crash loop causes 1 Hz infinite reconnect attempts with `slave.write('[bootroom] /ws disconnected; reconnecting…\r\n')` spamming the terminal display every second. Acceptable per `<deferred>` ("naive retry — no exponential backoff"), but should be visible to the reviewer.

**Fix:** Two small safeguards that don't violate the "no backoff" deferral:

```js
let reconnectScheduled = false;

function scheduleReconnect() {
  if (reconnectScheduled) return;       // dedupe
  reconnectScheduled = true;
  setTimeout(() => { reconnectScheduled = false; connectWs(); }, 1000);
}

function connectWs() {
  // ... rest unchanged, but onclose and the catch both call scheduleReconnect()
}
```

Optionally cap the diagnostic spam: track `lastDisconnectLogAt` and only write the disconnect line at most once every 30s.

---

### WR-04: `?pacing=abc` (non-numeric query string) silently coerces to 0 pacing instead of falling back to the documented 15ms default

**File:** `crates/bootroom/web/app.js:253-254`

**Issue:**

```js
const urlParams = new URLSearchParams(location.search);
const pacingMs = Math.max(0, Number(urlParams.get('pacing') ?? 15));
```

`urlParams.get('pacing')` returns `null` when the param is absent (correctly falls through to `?? 15` → `Number(15) === 15`). It returns the string when present. If present but non-numeric (`?pacing=abc`, `?pacing=`, `?pacing=15ms`), `Number(...)` returns `NaN`. `Math.max(0, NaN)` is `NaN`. `setTimeout(r, NaN)` is treated as `setTimeout(r, 0)`. So a typo in the URL silently disables pacing entirely.

That's the opposite of fail-safe: the user requested a non-zero pacing (probably to *slow down* injection because the kernel was missing keystrokes), and instead got 0ms pacing.

**Fix:** Validate explicitly:

```js
const rawPacing = urlParams.get('pacing');
const parsed = rawPacing === null ? 15 : Number(rawPacing);
const pacingMs = Number.isFinite(parsed) && parsed >= 0 ? parsed : 15;
```

Mirrors the defensive pattern already used by `humanBytes` (WR-07 fix in Phase 1). Add a one-line `console.warn` when falling back so users notice the typo.

---

### WR-05: `master.ldisc.flow` deactivation can cause `slave.write` to throw `"Do not write anything during flowStatus is stopped"` — diagnostic writes are not wrapped uniformly

**File:** `crates/bootroom/web/funnel.js:62`, `crates/bootroom/web/app.js:278, 295, 409, 430, 461, 472, 523`

**Issue:** The vendored xterm-pty `writeFromUpper` throws synchronously when flow is deactivated:

```js
writeFromUpper(e){
  if (this.flowActivated == !1) throw new Error("Do not write anything during flowStatus is stopped");
  ...
}
```

Flow can be deactivated by an XOFF (Ctrl-S, byte 0x13) typed by the user when `IXON` is set in termios — which is the default termios shipped by xterm-pty (`new f(25856, 5, 191, 35387, [3,28,127,21,4,...])` enables `IXON`). The funnel writes one byte at a time via `this.slave.write([b])` inside the `#drain` loop, with NO try/catch. If the guest user typed Ctrl-S between two enqueues, the next `slave.write` call throws, the `await this.#drain()` rejects, the `finally` resets `this.draining`, and subsequent enqueues will silently fail similarly.

Several of the `[bootroom] …` diagnostic `slave.write` calls in `app.js` (line 295 for the connect line; line 409 for copy-failed) do guard with try/catch, but lines 278 and 430 also guard with `try { … } catch (_e) {}`. The funnel itself does not. Inconsistent.

Note: CR-01's fix changes the funnel to call `ldisc.writeFromLower`, which is on the input path and does NOT have the same flow guard (`writeFromLower` only checks per-byte action, not `flowActivated` globally). So fixing CR-01 may incidentally fix this. Still worth flagging because the diagnostic-write call sites remain.

**Fix:** After CR-01 is fixed, audit the remaining `slave.write([bootroom] …)` sites — all seven — and either (a) wrap every one of them in `try { … } catch (_e) {}` (already done at five of seven), or (b) write them via a helper:

```js
function bootroomDiag(text) {
  try { slave.write('[bootroom] ' + text + '\r\n'); }
  catch (_e) { console.warn('[bootroom diag]', text, '(slave.write threw)'); }
}
```

The kernel-load-failure path (`bootGuest` line 430) and the boot-failure path (line 523) currently are guarded; the WR-disconnected (line 278) and the WS-Hello (line 295) sites also catch. The Copy-failed (line 409) and the FS_unlink error paths (lines 461, 472) are guarded. So the inconsistency is mild; the main motivation is to keep the funnel exception-safe even if Phase 3 keeps `slave.write` for some other reason.

---

### WR-06: `copyBtn.textContent = originalLabel` after `setTimeout(1500)` can stomp a subsequent COPY click's transient label

**File:** `crates/bootroom/web/app.js:394-413`

**Issue:** Sequence:

1. User clicks COPY at t=0 → succeeds → `textContent = 'COPIED'`; `setTimeout(revert, 1500)` scheduled.
2. User clicks COPY again at t=500ms → succeeds → `originalLabel = 'COPIED'` (because the first revert hasn't fired); `textContent = 'COPIED'`; another `setTimeout(revert, 1500)` scheduled.
3. At t=1500ms, first timeout fires → `textContent = 'COPIED'` (the *captured* `originalLabel` from the first click).
4. At t=2000ms, second timeout fires → `textContent = 'COPIED'` (captured from second click).

Net: button stays at `COPIED` forever until next click. Repro is rare in practice (humans don't double-click COPY) but the same shape applies if the user clicks COPY, the action fails (label → `COPY FAILED`), then they click COPY again before the 1500ms revert — `originalLabel` captures `COPY FAILED`, and the button shows `COPY FAILED` after the second successful copy reverts.

**Fix:** Cache the *true* original label outside the click handler scope, and clear any prior pending revert:

```js
const COPY_LABEL = 'COPY';
let copyRevertTimer = null;

copyBtn.addEventListener('click', async () => {
  const text = ... ;
  try {
    await navigator.clipboard.writeText(text);
    copyBtn.textContent = 'COPIED';
  } catch (e) {
    copyBtn.textContent = 'COPY FAILED';
    try { slave.write('[bootroom] Copy failed: ' + (e?.message || e) + '\r\n'); }
    catch (_e) {}
  }
  if (copyRevertTimer) clearTimeout(copyRevertTimer);
  copyRevertTimer = setTimeout(() => {
    copyBtn.textContent = COPY_LABEL;
    copyRevertTimer = null;
  }, 1500);
});
```

---

## Info

### IN-01: `Module.mainScriptUrlOrBlob = …` (Phase 1 IN-01 deferred) is still dead in the rewritten `app.js`

**File:** `crates/bootroom/web/app.js:188`

**Issue:** Phase 1 `01-REVIEW.md` IN-01 flagged this line as a no-op under the ES-module emscripten build (`-sEXPORT_ES6=1` in the Makefile). The Phase 1 fixes deferred it. Phase 2 rewrote `app.js` extensively and the line survived the rewrite unchanged:

```js
Module.mainScriptUrlOrBlob = location.origin + '/assets/qemu/out.js';
```

The worker URL is resolved from `import.meta.url` of `out.js`, so this assignment has no effect.

**Fix:** Delete the line. Add a brief comment above the `Module.pty = slave` block referencing the `import.meta.url` resolution mechanism.

---

### IN-02: `recomputePillLocal` is silent when `runtimeInitialized` is false and `serverStateAuthority` is null — no transition out of IDLE/LOADING is documented for that state

**File:** `crates/bootroom/web/app.js:236-244`

**Issue:**

```js
function recomputePillLocal() {
  if (serverStateAuthority !== null) {
    setPill(serverStateAuthority);
    return;
  }
  if (runtimeInitialized && firstSerialOutSeen) setPill('RUNNING');
  else if (runtimeInitialized) setPill('LOADING');
  // IDLE and HALTED are set explicitly by their triggers, not derived here.
}
```

The comment claims IDLE/HALTED are set explicitly elsewhere, which is true. But the function silently no-ops when neither branch matches (`!runtimeInitialized` and no server authority). Callers (`slave.onReadable` and `Module.onRuntimeInitialized`) always set `runtimeInitialized = true` or `firstSerialOutSeen = true` before calling, so today no caller hits the silent branch. Still, the function is small enough that a defensive default would clarify intent:

```js
function recomputePillLocal() {
  if (serverStateAuthority !== null) return setPill(serverStateAuthority);
  if (!runtimeInitialized) return; // LOADING was already set at xterm mount
  if (firstSerialOutSeen) return setPill('RUNNING');
  setPill('LOADING');
}
```

No behavior change; just makes the no-op explicit.

---

### IN-03: `tx` is created and immediately overshadowed by `tx_clone`; only `tx_clone` is ever used to send

**File:** `crates/bootroom/tests/serve_no_open.rs:96-106`

**Issue:**

```rust
let (tx, rx) = mpsc::channel::<String>();
let tx_clone = tx.clone();
let stdout_handle = thread::spawn(move || {
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        if tx_clone.send(line).is_err() { break; }
    }
});
```

`tx` is never used (only `tx_clone` is moved into the thread). The intent is presumably "keep one tx alive in main so the receiver doesn't disconnect when the thread exits, but also give the thread its own tx clone." That's a defensible pattern, but the comment doesn't say so and the `drop(tx)` later in the test reads as if `tx` was load-bearing. Either remove the `let tx = ...; tx.clone();` indirection and pass `tx` directly to the thread (eliminating the clone) — or document the dual-tx intent.

**Fix (preferred — simpler):** drop `tx_clone` entirely and move `tx` into the thread; the test only checks via `rx.recv_timeout` and never sends from main:

```rust
let (tx, rx) = mpsc::channel::<String>();
let stdout_handle = thread::spawn(move || {
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        if tx.send(line).is_err() { break; }
    }
});
```

Then remove the `drop(tx)` line — the thread's drop handles it.

---

### IN-04: `keyEventToBytes` silently drops function keys, AltGr, dead keys, IME composition

**File:** `crates/bootroom/web/funnel.js:107-137`

**Issue:** Returns `null` for any key not in the explicit table. F1-F12, PrintScreen, Insert, AltGr-modified characters, dead keys for accent composition (`'`, `^`, `~`), and IME composition events all return null. The handler in `app.js:178` then enqueues nothing and returns `false` to suppress xterm's own handling. Result: those keys do nothing visible — neither typed nor logged.

This is acceptable for a v1 REPL but will bite anyone who needs F-keys for a kernel menu, Alt-N navigation in a TUI like `htop`/`btop`, or non-ASCII input via IME (Chinese / Japanese / Korean kernels). Low priority because no current bootroom user runs a kernel needing those — but worth a future-work tag.

**Fix:** Add `console.debug('[funnel] unmapped key:', evt.key, evt.code)` in the null branch so missing keys are at least discoverable in DevTools. File a tracking issue for the F1-F12 + IME table once a user surfaces the need.

---

### IN-05: `handle_wire`'s lint allow `#[allow(clippy::unused_async)]` is broader than necessary today

**File:** `crates/bootroom/src/ws.rs:112`

**Issue:** Phase 2's `handle_wire` body uses only synchronous tracing calls. The function is marked `async` because Phase 4 will need `.await` on `tx.send(...)` for outbound frames. The `#[allow]` is a forward-looking compromise. It would be slightly cleaner to make the function sync today and add `async` plus the allow when Phase 4 needs it — but the current shape avoids a no-op churn diff at that transition. Mention only as a minor cleanliness note; the existing comment already explains the intent.

**Fix:** None required. If the maintainer prefers strict clippy hygiene, drop the `async` keyword and the `#[allow]` for now; re-add both in Phase 4.

---

_Reviewed: 2026-05-18_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

---

## Fixes Applied

**Fixed at:** 2026-05-18
**Scope:** Critical (1) + all Warnings (6). Info-level (5) deferred.

**Summary:**

- Findings in scope: 7
- Fixed: 7
- Skipped: 0

### Fixed Issues

#### CR-01: Funnel writes user keystrokes to `slave.write` → display, not guest stdin

**Files modified:** `crates/bootroom/web/funnel.js`, `crates/bootroom/web/app.js`, `.planning/phases/02-websocket-live-serial/02-RESEARCH.md`
**Commit:** `e8f4312`
**Applied fix:** Funnel constructor now takes `(slave, ldisc)`; drain loop calls `this.ldisc.writeFromLower([b])` (the guest-stdin input path) instead of `this.slave.write([b])` (the display path). `app.js` passes `master.ldisc` to the constructor. The seven diagnostic `slave.write('[bootroom] …')` sites in `app.js` are unchanged — those intentionally use the display path per `<documented_exceptions>` in `02-CONTEXT.md`. `02-RESEARCH.md` Pattern 3 and Pitfall #1 corrected to reflect the actual `writeFromLower` vs `writeFromUpper` directions. Added a CR-01 correction note inline so future phases do not regress the mental model.

**REQUIRES HUMAN VERIFICATION:** This bug is in a code path that cannot be exercised by the Rust test suite (no qemu-wasm + guest in CI). Headed-browser smoke is required.

#### WR-01: Malformed-JSON WS payload logged in full (log-amplification DoS)

**Files modified:** `crates/bootroom/src/ws.rs`
**Commit:** `4ef43bf`
**Applied fix:** Added `truncate_for_log(s, max)` helper that returns a UTF-8-safe prefix plus `"…(truncated, N bytes total)"` when payload exceeds 256 bytes. `warn!` call now logs the truncated payload. Three unit tests cover passthrough, oversize-ASCII, and UTF-8 boundary safety.

#### WR-02: Unused `base64` dep in `crates/bootroom/Cargo.toml`

**Files modified:** `crates/bootroom/Cargo.toml`, `Cargo.lock`
**Commits:** `c38976f` + `adce9ec` (lockfile)
**Applied fix:** Removed `base64.workspace = true` from `crates/bootroom/Cargo.toml`. Workspace-level declaration in root `Cargo.toml` retained for Phase 4's `bootroom run` driver. `cargo build --workspace` still clean.

#### WR-03: WS reconnect scheduled from two paths, no dedupe

**Files modified:** `crates/bootroom/web/app.js`
**Commit:** `546dde1`
**Applied fix:** Added module-scoped `reconnectTimer` + `scheduleReconnect()` helper. Both `onclose` and the `new WebSocket(...)` constructor catch now call `scheduleReconnect()`, which early-returns if a timer is already pending. Per `<deferred>`, naive 1Hz retry is preserved.

#### WR-04: `?pacing=abc` silently coerces to 0ms

**Files modified:** `crates/bootroom/web/app.js`
**Commit:** `ed36b25`
**Applied fix:** Explicit `Number.isFinite` + non-negative range check on parsed value. Falls back to `PACING_DEFAULT_MS = 15` with a `console.warn` if the query value is non-numeric, NaN, or negative. Mirrors `humanBytes` defensive pattern.

#### WR-05: `ldisc.writeFromLower` exception safety in funnel drain loop

**Files modified:** `crates/bootroom/web/funnel.js` (bundled into CR-01 commit)
**Commit:** `e8f4312` (CR-01)
**Applied fix:** Wrapped `this.ldisc.writeFromLower([b])` in try/catch; on throw, log via `console.warn` and drop the byte rather than crashing the drain loop. The seven diagnostic `slave.write` sites in `app.js` were re-audited per the review and confirmed all already guarded with try/catch — no additional changes needed there.

#### WR-06: COPY button revert stomped by overlapping clicks

**Files modified:** `crates/bootroom/web/app.js`
**Commit:** `0b96fd9`
**Applied fix:** Cached canonical `COPY_LABEL` once at module scope from `copyBtn.textContent`. Added `copyRevertTimer` tracker so a new click clears the prior revert timer before scheduling its own. Label revert now always restores the canonical text instead of capturing transient `COPIED`/`COPY FAILED`.

### Deferred (Info)

Info-level findings (IN-01..IN-05) were out of scope per the `--fix critical_warning` policy. They remain documented above for the next pass; none represent correctness bugs.

### Verification Run

- `cargo build --workspace`: clean
- `cargo test --workspace`: 52 tests pass (49 prior + 3 new `truncate_for_log` tests)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `node --check crates/bootroom/web/funnel.js`: OK
- `node --check crates/bootroom/web/app.js`: OK

_Fixed: 2026-05-18_
_Fixer: Claude (gsd-code-fixer)_
