---
phase: 04-scenario-engine-headless-run
reviewed: 2026-05-19T00:00:00Z
depth: standard
files_reviewed: 21
files_reviewed_list:
  - crates/bootroom-core/src/config.rs
  - crates/bootroom-core/src/lib.rs
  - crates/bootroom/src/cli.rs
  - crates/bootroom/src/main.rs
  - crates/bootroom/src/run_cmd.rs
  - crates/bootroom/src/server.rs
  - crates/bootroom/src/state.rs
  - crates/bootroom/src/transcript.rs
  - crates/bootroom/src/verbose.rs
  - crates/bootroom/src/ws.rs
  - crates/bootroom/src/lib.rs
  - crates/bootroom/web/scenario.js
  - crates/bootroom/web/app.js
  - crates/bootroom/tests/cli_subcommands.rs
  - crates/bootroom/tests/run_log_file_jsonl.rs
  - crates/bootroom/tests/run_subcommand_exit_codes.rs
  - crates/bootroom/tests/run_uses_same_router.rs
  - crates/bootroom/tests/run_verbose_stderr.rs
  - crates/bootroom/tests/ws_scenario_result_handoff.rs
  - crates/bootroom/tests/run_smoke_norn_kernel.rs
  - crates/bootroom/tests/fixtures/boot_smoke.toml
findings:
  critical: 0
  blocker: 2
  warning: 8
  info: 6
  total: 16
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-05-19
**Depth:** standard
**Files Reviewed:** 21
**Status:** issues_found

## Summary

Phase 4 lands a non-trivial composition: 3 additive `WsMessage` variants, an in-process axum + chromiumoxide driver (`run_cmd.rs`), a browser-side scenario engine (`scenario.js`), a JSONL transcript pipeline, and CLI surface refactoring. The code is generally well-architected — the explicit cleanup sequence in `run_cmd.rs` follows the verified Spike B pattern, the WS handoff uses correct take-once semantics, and the CLI flatten is backed by regression tests.

However, two **BLOCKER**-class bugs were found in `web/scenario.js`:

1. **Line-buffer contract violation** — when no `\r?\n` has arrived yet, `evaluate()` matches against the partial line *before* timeout, contradicting RUN-05's "partial line at timeout only" semantics.
2. **`after = "any"` flat buffer loses bytes between actions** — the `onWrite` handler short-circuits on `currentLabel === null`, dropping bytes from BOTH per-action AND the flat buffer between actions. Pitfall #5 (preserving cross-action line order for `after = "any"`) is violated; the inline comment on lines 553-557 acknowledges this *as if intentional*, but the engine doc-comment header at lines 60-63 and 04-RESEARCH Pitfall #5 both promise cross-action preservation.

Several **WARNING**-class robustness issues were also found, including misleading naming on the `transcript_overflow` event payload, URL construction without scenario-name escaping in `run_cmd.rs`, and a `Promise.race` timer leak that can mutate scenario state after the result frame has been sent.

## Blocker Issues

### BL-01: `evaluate()` matches partial line before timeout when no `\n` has arrived

**File:** `crates/bootroom/web/scenario.js:193-209`
**Issue:**

The function header doc says (line 181-183): *"Match against the substring up to the LAST `\r?\n` boundary unless `atTimeout` is true — in which case the partial trailing line is also matched."*

The implementation:

```js
const lastNl = stripped.lastIndexOf('\n');
const matchTarget = (atTimeout || lastNl === -1)
  ? stripped
  : stripped.slice(0, lastNl + 1);
```

When `atTimeout === false` AND `lastNl === -1` (no newline at all has arrived), `matchTarget` falls back to the entire `stripped` string — i.e. the partial line. This violates the RUN-05 line-buffer contract: a kernel that emits `"Booting kernel"` without a newline will get matched against `contains "Booting"` immediately, *before* the per-action timeout fires.

This silently invalidates the intent of the line-buffer / timeout interaction (which is meant to allow the kernel to finish a line before the assertion can hit on partial content). The bug only fires when (a) no `\n` has yet arrived in this match window AND (b) some bytes ARE present — which is realistic for early-boot scenarios where the very first few bytes precede any newline.

**Fix:**

```js
const lastNl = stripped.lastIndexOf('\n');
let matchTarget;
if (atTimeout) {
  matchTarget = stripped;
} else if (lastNl === -1) {
  matchTarget = '';  // no complete line yet; nothing to match pre-timeout
} else {
  matchTarget = stripped.slice(0, lastNl + 1);
}
```

Or, equivalently:

```js
const matchTarget = atTimeout
  ? stripped
  : (lastNl === -1 ? '' : stripped.slice(0, lastNl + 1));
```

Add a unit test (DevTools manual case at file bottom is insufficient): synthesize one `serial_chunk` with no newline, evaluate a `contains` assertion, and assert it returns `false` until `atTimeout=true` is passed.

---

### BL-02: `after = "any"` flat buffer drops bytes received between actions

**File:** `crates/bootroom/web/scenario.js:384-417` (onWrite callback) + `:553-557` (comment)
**Issue:**

The `master.onWrite` handler short-circuits when `currentLabel === null`:

```js
const disposable = master.onWrite(([bytes, _ack]) => {
  if (!bytes || bytes.length === 0) return;
  if (currentLabel === null) return;     // <-- drops bytes between actions
  // ... per-action buffer append ...
  flat.push(new Uint8Array(bytes));      // <-- never runs when between actions
  // ... transcript append ...
});
```

But the doc-comment header for the module (lines 11-19) and Pitfall #5 in 04-RESEARCH both promise that the flat buffer preserves cross-action line ordering for `after = "any"`. The comment at lines 553-557 even acknowledges this:

```js
// Reset currentLabel between actions so trailing-arrival chunks
// do NOT pollute the next action's buffer (they still flow
// into `flat` for `after="any"` — but only while a current
// action label is set; between actions both are paused).
```

This comment is self-contradictory: "they still flow into flat" but "only while a current action label is set" — i.e. they do NOT flow when `currentLabel === null`. The reader is misled; the implementation silently violates Pitfall #5.

**Impact:** For a multi-action scenario with `after = "any"` assertions, kernel bytes emitted in the brief gap between actions (e.g. boot messages emerging after action 1's poll completes but before action 2's `currentLabel` assignment) are LOST from both buffers. Single-action scenarios (like the e2e fixture `boot_smoke`) do not exercise this path, so 04-11 will not catch the regression.

**Fix:**

Split per-action buffer append (gated on `currentLabel !== null`) from the flat-buffer + transcript path (always append):

```js
const disposable = master.onWrite(([bytes, _ack]) => {
  if (!bytes || bytes.length === 0) return;
  // Per-action buffer — only meaningful when an action is active.
  if (currentLabel !== null) {
    const chunks = buffers.get(currentLabel) || [];
    chunks.push(new Uint8Array(bytes));
    buffers.set(currentLabel, chunks);
  }
  // Flat buffer — ALWAYS append (Pitfall #5: preserve cross-action line order).
  flat.push(new Uint8Array(bytes));
  // Transcript — same cap logic, but always account for these bytes.
  // Tag with currentLabel || '<between>' so the JSONL is parseable.
  // ... transcript append (with appropriate "action" field) ...
});
```

Add a regression test using the DevTools-style stub from the file-bottom manual test: pump bytes BETWEEN two action invocations, then assert flat-buffer length includes those bytes and an `after = "any"` assertion can match against them.

## Warnings

### WR-01: Misleading `bytes_truncated_estimate` reports bytes ACCEPTED, not truncated

**File:** `crates/bootroom/web/scenario.js:401-408`
**Issue:**

When the transcript cap fires:

```js
if (transcriptBytes + b64.length > TRANSCRIPT_CAP_BYTES) {
  transcriptOverflowed = true;
  transcript.push({
    ts: nowIsoUtc(),
    type: 'transcript_overflow',
    bytes_truncated_estimate: transcriptBytes,  // <-- bytes ACCEPTED so far
  });
  return;
}
```

The field is named `bytes_truncated_estimate` but the value emitted is `transcriptBytes` — the cumulative bytes that have ALREADY BEEN APPENDED, i.e. bytes NOT truncated. The actual truncated count is everything dropped after this event, which is not reported.

This is confusing for downstream report-tooling: the DevTools manual test at lines 718-732 even asserts `bytes_truncated_estimate ≈ 5_000_000`, which is the cap value (=  accepted bytes), not the truncated amount.

**Fix:**

Either:
- Rename the field to `bytes_accepted_at_cap` to match what's emitted; OR
- Track and report the actual truncated count by counting dropped `b64.length` after `transcriptOverflowed = true`:

```js
let bytesTruncated = 0;
// ...
if (transcriptOverflowed) {
  bytesTruncated += b64.length;
  return;
}
// ... on cap hit, push event without estimate ...
// ... at scenario end, before sending frame, update the overflow event:
if (transcriptOverflowed) {
  const overflowEvent = transcript.find(e => e.type === 'transcript_overflow');
  if (overflowEvent) overflowEvent.bytes_truncated_estimate = bytesTruncated;
}
```

The corresponding Rust deserializer in `transcript.rs:69-75` and its test on `transcript.rs:225-253` would also need updating.

---

### WR-02: `--scenario` value injected into URL without percent-encoding

**File:** `crates/bootroom/src/run_cmd.rs:183`
**Issue:**

```rust
let url = format!("http://{bound}/?scenario={}", args.scenario);
```

`args.scenario` is operator-controlled (via `--scenario <NAME>`). If the value contains URL-special characters (`#`, `&`, `%`, ` `, `+`), the resulting URL is misparsed by Chromium:

- `--scenario "boot&kernel=evil"` → `?scenario=boot&kernel=evil` → server sees two query params, `URLSearchParams.get('scenario')` returns `"boot"`.
- `--scenario "x#y"` → `?scenario=x#y` → fragment, browser sees `?scenario=x` only.

While the value is validated against the loaded config's scenario names earlier (line 93-100), `bootroom run` *currently* requires an exact match — operator-controlled invariants. So this is not a security issue in practice. But it IS a correctness gap: when a config rejects scenario names with `&`/`#` (it does not — `Scenario.name` is a free-form `String`), the bug would surface as "scenario not found" with no diagnostic.

**Fix:**

Use `url::form_urlencoded` (already transitively pulled by axum) or hand-roll a tiny percent-encoder for the query-component subset:

```rust
fn encode_query_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
let url = format!("http://{bound}/?scenario={}", encode_query_component(&args.scenario));
```

Or reject scenario names containing URL-special chars at config load time in `bootroom-core/src/config.rs` (preferred — defends both the Rust and the browser sides).

---

### WR-03: `Promise.race` timeout has no cancellation; can mutate state after frame is sent

**File:** `crates/bootroom/web/scenario.js:215-219` + `:561-571`
**Issue:**

```js
function timeoutPromise(ms) {
  return new Promise((resolve) => setTimeout(() => resolve('timeout'), ms));
}
// ...
await Promise.race([
  inner(),
  timeoutPromise(scenarioBudgetMs).then(() => {
    scenarioVerdict = 'timeout';
    scenarioError = `scenario timeout after ${scenarioBudgetMs}ms`;
  }),
]);
```

JS has no Promise cancellation. When `inner()` resolves first, the `setTimeout(...)` is NOT cleared; the timer fires later (potentially seconds or minutes after the result frame has been sent). When it fires, the `.then()` callback mutates `scenarioVerdict` and `scenarioError`.

In the current code these mutations are harmless because the frame has already been built (lines 595-604) before the timer fires. But the dead-write surface is real: if anyone adds post-race state inspection, it would observe stale `'timeout'` after a clean pass. The timer also keeps the JS runtime alive after the scenario completes — a leak in headed-debug mode (acknowledged in the source comment "JS has no Promise cancellation").

**Fix:**

Capture the timer handle and clear it after the race resolves:

```js
let timerHandle;
const timeoutP = new Promise((resolve) => {
  timerHandle = setTimeout(() => resolve('timeout'), scenarioBudgetMs);
});
try {
  const winner = await Promise.race([
    inner().then(() => 'inner'),
    timeoutP,
  ]);
  if (winner === 'timeout') {
    scenarioVerdict = 'timeout';
    scenarioError = `scenario timeout after ${scenarioBudgetMs}ms`;
  }
} finally {
  if (timerHandle != null) clearTimeout(timerHandle);
}
```

---

### WR-04: `browser.close().await` has no timeout; can hang the run-mode shutdown

**File:** `crates/bootroom/src/run_cmd.rs:220-221`
**Issue:**

```rust
let _ = browser.close().await;
let _ = browser.wait().await;
```

If the chromiumoxide handler task has already crashed or the Chromium subprocess has hung (e.g., a zombie process, a wedged event loop), `browser.close()` and `browser.wait()` can block indefinitely. The 04-07 RUN-10 contract requires explicit teardown; the 04-11 e2e test even pgrep-checks for orphan `chromium.*--headless` processes after run.

If `browser.close()` hangs, `bootroom run` never exits even though the verdict has been computed. The outer `run` doesn't wrap this with a timeout, so a single bad shutdown wedges the whole CI job.

**Fix:**

Wrap both calls with a `tokio::time::timeout`:

```rust
use tokio::time::{timeout, Duration};

// 5s is a generous shutdown budget; Chromium normally closes in < 1s.
if let Err(_) = timeout(Duration::from_secs(5), browser.close()).await {
    tracing::warn!("browser.close() exceeded 5s shutdown budget; forcing abort");
}
if let Err(_) = timeout(Duration::from_secs(5), browser.wait()).await {
    tracing::warn!("browser.wait() exceeded 5s shutdown budget");
}
handler_task.abort();
server_task.abort();
```

This also helps the `run_smoke_norn_kernel.rs` 90-second outer test budget hold up under unhealthy shutdown paths.

---

### WR-05: `coi_self_check` swallows non-bool eval results as `false`

**File:** `crates/bootroom/src/run_cmd.rs:356-369`
**Issue:**

```rust
let coi: bool = page
    .evaluate(...)
    .await
    .map_err(|e| format!("COI self-check eval failed: {e}"))?
    .into_value::<bool>()
    .unwrap_or(false);
if !coi {
    return Err(coi_self_check_diagnostic().into());
}
```

`into_value::<bool>().unwrap_or(false)` swallows any deserialization error (the JS eval returned `null`, `undefined`, or a non-bool type) as `false`, then the diagnostic claims COOP/COEP are missing. This is misleading when the actual cause is a different runtime failure (e.g., page error before `self` is ready, JS parsing error in the eval string). Operators chasing the diagnostic will look at headers when they should be looking at the JS console.

**Fix:**

Distinguish "eval succeeded but didn't return bool" from "COI=false":

```rust
let raw = page
    .evaluate(...)
    .await
    .map_err(|e| format!("COI self-check eval failed: {e}"))?;
let coi = raw.into_value::<bool>().map_err(|e| {
    format!("COI self-check returned non-bool value: {e}; \
             this usually means the page failed to load before the eval ran")
})?;
if !coi {
    return Err(coi_self_check_diagnostic().into());
}
```

---

### WR-06: `BOOTROOM_CHROMIUM_ARGS` parsing breaks on quoted args with spaces

**File:** `crates/bootroom/src/run_cmd.rs:159-166`
**Issue:**

```rust
let extra_args = std::env::var("BOOTROOM_CHROMIUM_ARGS")
    .ok()
    .map(|s| {
        s.split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
```

`split_whitespace()` does not respect quoting. An operator setting `BOOTROOM_CHROMIUM_ARGS='--proxy-server="http://host:port" --user-agent="Mozilla"'` ends up with the chromium argv containing `--proxy-server="http://host:port"` as one literal token (which Chromium probably rejects) plus stray `"Mozilla"` fragments.

The threat model (T-04-03) accepts env-var injection because the operator controls it, but quoted-args handling is still a usability papercut.

**Fix:**

Use `shell-words` or `shlex` (small crates, popular in CLI ecosystem). Or document the limitation prominently in `--help` and bail loudly if the env var contains quote chars:

```rust
let raw = std::env::var("BOOTROOM_CHROMIUM_ARGS").ok();
if let Some(s) = raw.as_deref() {
    if s.contains('"') || s.contains('\'') {
        tracing::warn!(
            "BOOTROOM_CHROMIUM_ARGS contains quote chars; \
             whitespace-split does NOT respect quoting. \
             Pass args without spaces or rebuild bootroom with shlex support."
        );
    }
}
let extra_args = raw.map(|s| s.split_whitespace().map(String::from).collect()).unwrap_or_default();
```

---

### WR-07: `discover_chromium` uses external `which` (not portable; not present on minimal images)

**File:** `crates/bootroom/src/run_cmd.rs:267-284`
**Issue:**

```rust
let which_out = Command::new("which")
    .arg("chromium")
    .output()
    .ok()
    ...
```

Three issues:
1. **Windows** has no `which` binary (it has `where`); `cargo install bootroom` is supposed to work on any platform per the project README/CLAUDE.md.
2. **Minimal CI images** (alpine, busybox-based, distroless) may lack `which` entirely; the discovery silently moves on, but the operator gets no signal that `which` itself was missing.
3. `which` resolves against the inherited `$PATH`, which the test harness deliberately empties — this is the *only* test path that empties `$PATH`. The `chromium_works = /usr/bin/chromium --version` self-skip in `run_subcommand_exit_codes.rs:128` is the proper escape hatch.

**Fix:**

Use the `which` crate (pure Rust, ~50 LoC, no system dep) — already common in the Rust ecosystem. Or hand-roll a PATH-walking loop:

```rust
fn which_chromium_via_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(if cfg!(windows) { "chromium.exe" } else { "chromium" });
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
```

Replace the `Command::new("which")` call with `which_chromium_via_path().map(|p| p.display().to_string()).unwrap_or_default()`.

---

### WR-08: `perActionTimeoutMs` uses `||` falsy-default — explicit `timeout_ms=0` silently becomes 5000ms

**File:** `crates/bootroom/web/scenario.js:248-249`
**Issue:**

```js
if (a.after === actionLabel || a.after === 'any') {
  max = Math.max(max, a.timeout_ms || 5000);
}
```

If an operator deliberately sets `timeout_ms = 0` in `bootroom.toml` (e.g., for a synchronous-must-already-be-true assertion), the `||` falsy fallback silently substitutes `5000`. There's no warning. The Rust load-time validator (config.rs:80-88) defaults `timeout_ms` to 5_000 on `#[serde(default)]` but does NOT reject `0`.

Same pattern at line 425: `const scenarioBudgetMs = scenario.timeout_ms || 30_000;` — explicit `0` becomes 30000.

**Fix:**

Use the nullish coalescing operator (`??`) to only fall back on `null`/`undefined`:

```js
max = Math.max(max, a.timeout_ms ?? 5000);
// ...
const scenarioBudgetMs = scenario.timeout_ms ?? 30_000;
```

Or, if `0` is genuinely invalid, reject it at Rust config load:

```rust
// In Assertion::validate (config.rs):
if a.timeout_ms == 0 {
    return Err(LoadError { /* timeout_ms must be > 0 */ });
}
```

## Info

### IN-01: `chromiumoxide` workspace dep uses `default-features = false` without justification comment

**File:** `Cargo.toml:16`
**Issue:**

```toml
chromiumoxide = { version = "0.9.1", default-features = false }
```

The default features almost certainly include `tokio-runtime` (which run_cmd.rs requires). Disabling defaults likely breaks the build elsewhere, or relies on transitive feature unification with the `spike-b` crate. There is no comment explaining the choice.

**Fix:**

Add a comment explaining what features are deliberately excluded and what's relied on:

```toml
# default-features = false: turns off `async-std-runtime` (we use tokio).
# Required features (`tokio-runtime`) are enabled implicitly via spike-b's
# dependency on chromiumoxide with default features — this works because
# cargo unifies features across workspace deps. If spike-b is ever removed,
# bootroom's chromiumoxide dep must add `features = ["tokio-runtime"]`.
chromiumoxide = { version = "0.9.1", default-features = false }
```

Or explicitly enable the features bootroom needs, so feature unification isn't load-bearing.

---

### IN-02: `Action.bytes` in `boot_smoke.toml` fixture uses empty string with leading space

**File:** `crates/bootroom/tests/fixtures/boot_smoke.toml:24`
**Issue:**

```toml
[[action]]
label = "boot"
bytes = ''
```

The `bytes = ''` action does nothing (no enqueue happens because `scenario.js:462` short-circuits on empty bytes). This is functionally a NOP for the scenario engine but still generates an `action_send` transcript event with `bytes_b64 = ""`. The intent is documented in the comment but the empty-string action label "boot" feels wrong — a clearer name would be `idle` or `observe`.

**Fix:** Rename to a more accurate label:

```toml
[[action]]
label = "observe"
bytes = ''
description = "No bytes sent; the scenario observes whatever the kernel emits on its own."
```

And update the scenario reference at line 29.

---

### IN-03: `persist_transcript` clones every transcript event JSON for `from_value` — O(n*size) work

**File:** `crates/bootroom/src/run_cmd.rs:410`
**Issue:**

```rust
for ev_json in events {
    match serde_json::from_value::<TranscriptEvent>(ev_json.clone()) {
```

`ev_json.clone()` deep-copies each `serde_json::Value`. With a transcript at 5 MB cap, this doubles memory briefly. Not a correctness issue but a peak-memory smell.

**Fix:** Use `from_value::<TranscriptEvent>(ev_json.take())` if you can mutate the array, or roundtrip via string `serde_json::from_str(&serde_json::to_string(ev_json)?)?` (single allocation). Cleanest is to deserialize the whole array as `Vec<TranscriptEvent>` once at the call site instead of as `serde_json::Value`. Out-of-scope per the no-performance-issues note, but worth flagging.

---

### IN-04: WS handler arm for `ScenarioResult` could log scenario name for traceability

**File:** `crates/bootroom/src/ws.rs:287-303`
**Issue:**

The success arm logs `tracing::info!("ScenarioResult delivered to run_cmd driver");` — no scenario name. The frame *has* a `scenario` field but it's consumed before logging.

**Fix:**

```rust
ws @ WsMessage::ScenarioResult { .. } => {
    let scenario_name = if let WsMessage::ScenarioResult { ref scenario, .. } = ws {
        scenario.clone()
    } else { String::new() };
    match state.take_scenario_result_tx().await {
        Some(tx) => {
            if tx.send(ws).is_err() {
                tracing::warn!(%scenario_name, "ScenarioResult oneshot send failed");
            } else {
                tracing::info!(%scenario_name, "ScenarioResult delivered to run_cmd");
            }
        }
        // ...
    }
}
```

Or destructure the verdict + scenario fields up front using `if let` bindings.

---

### IN-05: `run_log_file_jsonl.rs` test does not gate on exit code — could miss a regression

**File:** `crates/bootroom/tests/run_log_file_jsonl.rs:73-83`
**Issue:**

The test explicitly says "We don't gate on exit code" because it expects exit 3 (chromium missing) but tolerates exit 2 for misconfigured hosts. This is a reasonable test posture but means a regression that makes `bootroom run` exit 0 silently (e.g., by accidentally NOT awaiting the result) would NOT fail this test as long as a `scenario_start` event was written.

**Fix:** At minimum, assert the exit code is NOT 0:

```rust
assert_ne!(
    out.status.code(),
    Some(0),
    "expected non-zero exit (chromium missing); stderr:\n{}",
    String::from_utf8_lossy(&out.stderr)
);
```

The "either 2 or 3 is acceptable" tolerance can be expressed as `assert!(matches!(out.status.code(), Some(2) | Some(3)))`.

---

### IN-06: Defense-in-depth `maybeRunScenarioFromUrlQuery` could emit `error` for an unknown scenario over a not-yet-open WS

**File:** `crates/bootroom/web/app.js:354-368`
**Issue:**

```js
if (!scenario) {
  // ... build frame ...
  if (ws && ws.readyState === WebSocket.OPEN) {
    try { ws.send(JSON.stringify(frame)); } catch (_e) {}
  }
  console.warn('[scenario] unknown scenario:', scenarioName);
  return;
}
```

The `maybeRunScenarioFromUrlQuery` runs from the `Hello` handler, so `ws.readyState === OPEN` should hold. But if for any reason the socket has already closed (network blip), the frame is silently dropped and `run_cmd` waits for its outer timeout (scenario.timeout_ms + 30s). The Rust side at line 92-100 validates the scenario name before Chromium launches, so the manual-URL hand-driven case is the only path here — but a flaky WS at that exact moment would degrade exit time from ~instant to ~30s+.

**Fix:** If `ws.readyState !== OPEN`, log a `console.error` (not just `warn`) and proactively close the page so `run_cmd`'s outer timeout is the only failure mode — there is no useful retry path for "unknown scenario". A future plan could send the frame from `connectWs`'s onopen if it was pending.

---

_Reviewed: 2026-05-19_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
