/**
 * scenario.js — browser-side scenario engine.
 *
 * Sole consumer is `bootroom run`'s URL-query trigger (?scenario=<name>)
 * wired into `app.js` by plan 04-09. End-to-end verification path:
 *   bootroom run -> headless chromium -> ?scenario=<name> -> app.js ->
 *     runScenario(...) [this module] -> ws.send(WsMessage::ScenarioResult)
 *     -> Rust ws.rs handler (04-05) -> oneshot completion in `run_cmd`.
 *
 * Architecture
 * ============
 *   1. Subscribe to master.onWrite. Each chunk is appended to:
 *      (a) per-action buffer Map<actionLabel, Uint8Array[]> — for
 *          assertions with `after = "<label>"`;
 *      (b) flat append-only Uint8Array[] — for `after = "any"`
 *          (Pitfall #5 line-arrival ordering);
 *      (c) the transcript array as a `serial_chunk` event UNTIL the
 *          cumulative `bytes_b64` payload across `serial_chunk`
 *          events exceeds TRANSCRIPT_CAP_BYTES (5 MB). After the
 *          cap is hit, serial_chunk events are DROPPED (buffers
 *          (a) and (b) keep growing — they are needed for assertion
 *          evaluation) and a single `transcript_overflow` event is
 *          appended noting bytes_truncated_estimate.
 *   2. For each action in scenario.actions:
 *        - reset that action's per-action buffer (RUN-07 default);
 *        - currentLabel = label;
 *        - funnel.enqueue(b64ToBytes(action.bytes_b64), { pacingMs: 15 });
 *        - transcript.push({ts, type:'action_send', action, bytes_b64});
 *        - poll relevant assertions against the buffers until either all
 *          pass or the per-action timeout fires;
 *        - at timeout, do one final pass including the partial trailing
 *          line (RUN-05);
 *        - emit assertion_result events to transcript;
 *        - record per-action verdict.
 *   3. After loop: build ScenarioResult JSON; ws.send(); poll
 *      bufferedAmount === 0 (Pitfall #2).
 *   4. Always run `finally`: funnel.unlockInput(); master.onWrite
 *      disposable .dispose() (Pitfall #4).
 *
 * Pitfalls handled inline (see 04-RESEARCH.md "Common Pitfalls"):
 *   #1 — regex flavor drift (Rust ∩ JS). Construct `new RegExp(pattern)`
 *        ONCE per scenario start in a try/catch (cached on the assertion
 *        object as `_compiled`); on compile failure emit
 *        verdict='error' from the engine (defense-in-depth; Rust regex
 *        compile-check at config load makes this unreachable in
 *        practice).
 *   #2 — WS flush race. After ws.send(scenarioResultFrame), poll
 *        ws.bufferedAmount === 0 (with a 5 s deadline) before resolving.
 *   #3 — funnel.enqueue is lock-agnostic; engine calls enqueue directly
 *        and never consults funnel.locked. DO NOT add a lock guard
 *        inside funnel.enqueue — server-initiated SerialIn frames (incl.
 *        this engine's own writes) must keep flowing during a lock or
 *        the engine self-blocks. See funnel.js lines 53-67.
 *   #4 — master.onWrite Disposable is captured at subscription time and
 *        .dispose()'d in the `finally` block. Failing this leaks one
 *        listener per scenario run; engine is designed for
 *        one-scenario-per-page-load (matches `bootroom run`'s
 *        one-scenario-per-invocation contract) so the leak would still
 *        bite long-lived headed-debug sessions.
 *   #5 — `after = "any"` evaluates against the SECONDARY flat buffer
 *        (cross-action line order preserved), NOT the per-action Map.
 *        This is the only place line-order across action boundaries
 *        matters.
 *   Transcript cap — unbounded `serial_chunk` accumulation would make
 *        the final `ScenarioResult` WS frame multi-MB; the JSON.parse
 *        on the Rust side then blocks long enough for the outer Rust
 *        timeout (Pitfall #8) to fire instead of returning a useful
 *        verdict. Cap at 5 MB of cumulative serial_chunk bytes_b64
 *        payload. Drop further serial_chunk events; emit ONE
 *        `transcript_overflow` event with bytes_truncated_estimate.
 *        Per-action / flat buffers continue to receive bytes
 *        independent of the cap so assertion verdicts stay correct.
 *
 * Wire-shape contracts
 * ====================
 *   ScenarioResult WS frame — matches WsMessage::ScenarioResult from
 *     bootroom-core (04-01). Roundtrip tests in 04-01 pin the shape.
 *   TranscriptEvent shapes — match the Rust enum from 04-06. The
 *     `transcript_overflow_event_deserializes_from_browser_json` test
 *     in 04-06 round-trips the EXACT JSON this module emits.
 */

// Transcript-size cap (5 MB of cumulative serial_chunk bytes_b64).
// Bounds the final ScenarioResult WS frame; protects the Rust outer
// timeout (Pitfall #8) from waiting on a multi-MB JSON parse. See the
// DevTools-only "TRANSCRIPT CAP" manual test at the bottom of this file.
const TRANSCRIPT_CAP_BYTES = 5_000_000;

// No imports — funnel / master / ws are injected via the deps argument.
// b64ToBytes / bytesToB64 are duplicated inline (small; matches the
// "no build step" constraint). funnel.js exports equivalents; keeping
// them inline here avoids forcing app.js to also import them solely to
// hand them to this module.

/**
 * Decode base64 to a Uint8Array. Mirrors funnel.js's `b64ToBytes`.
 * @param {string} b64
 * @returns {Uint8Array}
 */
function b64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * Encode bytes to base64. Chunked to avoid V8's argument-count limit on
 * `String.fromCharCode.apply` for large buffers (some engines cap at
 * ~125k args). Mirrors funnel.js's `bytesToB64` — kept inline here to
 * avoid an extra import dependency for the engine.
 * @param {Uint8Array} bytes
 * @returns {string}
 */
function bytesToB64(bytes) {
  let s = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode.apply(null, bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

/**
 * Open Q3: ISO 8601 UTC with `Z` suffix. JS native `toISOString()`
 * emits exactly that format (e.g. `2026-05-19T14:32:01.123Z`).
 * @returns {string}
 */
function nowIsoUtc() {
  return new Date().toISOString();
}

/**
 * ANSI escape stripper — Pattern 3 / RUN-05. Removes CSI sequences of
 * the form ESC `[` <numbers/semicolons>* <letter>. Storage stays raw
 * bytes; we strip only at MATCH time so the transcript preserves the
 * exact kernel output for downstream debugging.
 *
 * Example sequences this strips (one digit, one letter — the minimal
 * single-parameter CSI form) such as `\x1b[0m`, `\x1b[1J`, `\x1b[K`.
 * The `*` quantifier in the actual regex below also covers the
 * no-digit and multi-digit parameter forms.
 * @param {string} s
 * @returns {string}
 */
function stripAnsi(s) { /* RUN-05 ANSI strip; minimal form documented: \x1b\[[0-9;][A-Za-z] — the actual regex below uses the `*` quantifier */
  return s.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
}

/**
 * Concatenate a list of Uint8Array chunks into one buffer. O(n) in total
 * bytes — fine for the scenario sizes we expect (≤ 5 MB of serial output
 * by Transcript cap).
 * @param {Uint8Array[]} chunks
 * @returns {Uint8Array}
 */
function concatChunks(chunks) {
  let total = 0;
  for (const c of chunks) total += c.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) { out.set(c, off); off += c.length; }
  return out;
}

/**
 * Decode a list of Uint8Array chunks as UTF-8 (non-fatal — invalid bytes
 * become U+FFFD). Sufficient for assertion matching.
 * @param {Uint8Array[]} chunks
 * @returns {string}
 */
function decodeChunks(chunks) {
  return new TextDecoder('utf-8', { fatal: false }).decode(concatChunks(chunks));
}

/**
 * Evaluate a single assertion against the accumulated raw chunks.
 *
 * Line buffering (RUN-05): match against the substring up to the LAST
 * `\r?\n` boundary unless `atTimeout` is true — in which case the
 * partial trailing line is also matched.
 *
 * Pitfall #1: regex assertions use the cached `_compiled` RegExp that
 * was constructed once at scenario start. The compile-check happens
 * before we ever reach this function, so failures here are
 * defense-in-depth only.
 *
 * @param {Uint8Array[]} rawChunks
 * @param {{kind:'contains'|'regex', pattern:string, _compiled?:RegExp}} assertion
 * @param {boolean} atTimeout when true, include the partial trailing line
 * @returns {boolean}
 */
function evaluate(rawChunks, assertion, atTimeout) {
  const raw = decodeChunks(rawChunks);
  const stripped = stripAnsi(raw);
  const lastNl = stripped.lastIndexOf('\n');
  // BL-01 fix: when `atTimeout === false` AND no `\n` has arrived yet,
  // the previous behaviour matched against the FULL partial buffer.
  // Per RUN-05, partial-line matching is allowed ONLY at the per-action
  // timeout. Without a complete line we therefore have nothing eligible
  // to match — return an empty target so contains/regex assertions
  // wait for either a newline or the timeout escape hatch.
  let matchTarget;
  if (atTimeout) {
    matchTarget = stripped;
  } else if (lastNl === -1) {
    matchTarget = '';
  } else {
    matchTarget = stripped.slice(0, lastNl + 1);
  }
  if (assertion.kind === 'contains') {
    return matchTarget.includes(assertion.pattern);
  }
  // 'regex': assertion._compiled was set at scenario start.
  try {
    return assertion._compiled.test(matchTarget);
  } catch (_e) {
    return false;
  }
}

/**
 * Pitfall #2 mitigation. Poll `ws.bufferedAmount` until it reaches 0
 * (i.e. the ScenarioResult frame has actually been pushed to the
 * socket) or until 5 s elapse — the escape hatch is here because
 * Chromium's bufferedAmount can lag under heavy load; the Rust outer
 * timeout in `run_cmd` is the real backstop.
 * @param {WebSocket} ws
 */
async function waitForFlush(ws) {
  const deadline = Date.now() + 5_000;
  while (ws.bufferedAmount > 0 && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 10));
  }
}

/**
 * Compute the per-action timeout: MAX(assertion.timeout_ms across
 * this action's assertions, including assertions with `after === "any"`
 * because those evaluate against this action's wall-clock window). If
 * no assertions reference this action, fallback 5_000 ms per Open Q5.
 * @param {string} actionLabel
 * @param {Array<{after:string, timeout_ms?:number}>} assertionList
 * @returns {number}
 */
function perActionTimeoutMs(actionLabel, assertionList) {
  let max = 0;
  for (const a of assertionList) {
    if (a.after === actionLabel || a.after === 'any') {
      // WR-08 fix: use `??` so an explicit `timeout_ms: 0` is
      // preserved as `0` rather than being silently rewritten to
      // the 5000 ms default by `||`'s falsy fallback. An explicit
      // zero will still get filtered out by the `max > 0` check
      // below if every relevant assertion specifies 0 — that path
      // remains a defensive 5000 ms fallback, which is intentional.
      max = Math.max(max, a.timeout_ms ?? 5000);
    }
  }
  return max > 0 ? max : 5000;
}

/**
 * Pre-compile every regex assertion ONCE at scenario start. Mutates
 * the assertion list in place (adds `_compiled`); collects any compile
 * failures.
 *
 * Pitfall #1 (Rust ∩ JS regex flavor drift): the Rust `regex` crate
 * (used by 04-02's config validator) is the stricter engine, so a
 * config that loads on the server should always compile in the
 * browser. We still try/catch here as defense-in-depth — a future Rust
 * regex bump that adds, say, a new escape recognized by Rust but not
 * JS would otherwise crash the engine.
 *
 * @param {Array<{kind:string, pattern:string, after:string, _compiled?:RegExp}>} assertions
 * @returns {Array<{after:string, pattern:string, error:string}>}
 */
function compileRegexes(assertions) {
  const compileErrors = [];
  for (const a of assertions) {
    if (a.kind === 'regex') {
      try {
        a._compiled = new RegExp(a.pattern);
      } catch (e) {
        compileErrors.push({ after: a.after, pattern: a.pattern, error: String(e) });
      }
    }
  }
  return compileErrors;
}

/**
 * `/api/config` (Phase 3) projects either `assert` (matching the TOML
 * key) or `assertions` (the Rust struct field). Accept either; prefer
 * the explicit `assertions` form when both are present.
 * @param {{assertions?:Array, assert?:Array}} scenario
 * @returns {Array}
 */
function getAssertionList(scenario) {
  return scenario.assertions || scenario.assert || [];
}

/**
 * Build an early-exit ScenarioResult frame for malformed scenarios or
 * unrecoverable engine errors. Verdict is `error` and `actions` /
 * `transcript` are empty arrays; the `error` field carries the human
 * description.
 * @param {{name?:string}} scenario
 * @param {string} error
 * @returns {object}
 */
function buildErrorResult(scenario, error) {
  return {
    type: 'ScenarioResult',
    verdict: 'error',
    scenario: scenario && scenario.name ? scenario.name : '',
    started_at: nowIsoUtc(),
    ended_at: nowIsoUtc(),
    actions: [],
    transcript: [],
    error,
  };
}

/**
 * Run a scenario to completion. Returns when the ScenarioResult WS
 * frame has been sent and (best-effort) flushed.
 *
 * @param {{
 *   name: string,
 *   actions: string[],
 *   assertions?: Array<{kind:'contains'|'regex', pattern:string, after:string, timeout_ms?:number}>,
 *   assert?:      Array<{kind:'contains'|'regex', pattern:string, after:string, timeout_ms?:number}>,
 *   timeout_ms?: number,
 * }} scenario
 * @param {Array<{label:string, bytes_b64:string, group?:string, description?:string}>} actions
 * @param {{ws: WebSocket, funnel: object, master: object}} deps
 * @returns {Promise<void>}
 */
export async function runScenario(scenario, actions, deps) {
  const { ws, funnel, master } = deps;

  // Open Q2 defense-in-depth: malformed scenario object. Phase 3 config
  // load + the 04-09 wire-up reject this before we get here; this is
  // belt-and-braces for hand-driven DevTools experiments.
  if (!scenario || typeof scenario.name !== 'string' || !Array.isArray(scenario.actions)) {
    const frame = buildErrorResult(scenario, 'scenario object malformed');
    try { ws.send(JSON.stringify(frame)); await waitForFlush(ws); } catch (_e) {}
    return;
  }

  const assertions = getAssertionList(scenario);
  const compileErrors = compileRegexes(assertions);
  if (compileErrors.length > 0) {
    // Pitfall #1 fallback — unreachable in practice, but emit a useful
    // verdict instead of crashing the page.
    const frame = buildErrorResult(
      scenario,
      'regex compile failed: ' + JSON.stringify(compileErrors)
    );
    try { ws.send(JSON.stringify(frame)); await waitForFlush(ws); } catch (_e) {}
    return;
  }

  const actionsByLabel = new Map(actions.map(a => [a.label, a]));
  const startedAt = nowIsoUtc();
  /** @type {Array<object>} TranscriptEvent[] — wire shape pinned by 04-06 */
  const transcript = [];
  /** @type {Array<{label:string, verdict:string, assertions:Array<{kind:string,pattern:string,verdict:string}>, error?:string}>} */
  const actionResults = [];

  // Transcript byte budget — counts ONLY serial_chunk event payload
  // bytes (the only event class that grows linearly with kernel
  // output). action_send / assertion_result / scenario_result events
  // are constant-size per action; they don't need to be metered.
  let transcriptBytes = 0;
  let transcriptOverflowed = false;
  // WR-01 fix: track the cumulative count of serial_chunk bytes_b64
  // bytes that were DROPPED after the cap fired (i.e. actually
  // truncated). The transcript_overflow event's
  // `bytes_truncated_estimate` is updated at scenario end so the
  // emitted value matches the field name.
  let transcriptBytesTruncated = 0;
  /** @type {object|null} reference to the single transcript_overflow event */
  let overflowEvent = null;

  // Per-action buffer (Pattern 2 + RUN-07). RESET on action start; one
  // entry per action in scenario.actions.
  /** @type {Map<string, Uint8Array[]>} */
  const buffers = new Map();
  // Secondary flat append-only buffer for `after = "any"` (Pitfall #5).
  /** @type {Uint8Array[]} */
  const flat = [];
  /** Label of the currently-executing action; null between actions so
   *  trailing-arrival chunks do not pollute the next action's buffer. */
  let currentLabel = null;

  // Subscribe to master.onWrite. Pitfall #4: capture the Disposable so
  // we can `.dispose()` in `finally`.
  //
  // BL-02 fix: do NOT short-circuit on `currentLabel === null`. Pitfall #5
  // promises that the flat append-only buffer preserves cross-action
  // line ordering for `after = "any"` — bytes arriving between actions
  // (e.g. trailing-arrival kernel output after action N's poll resolves
  // but before action N+1's `currentLabel` assignment) MUST land in
  // `flat` and in the transcript. Only the per-action `buffers` mutation
  // is gated on `currentLabel`, because that map's keys are action
  // labels and "between actions" has no meaningful label.
  const BETWEEN_LABEL = '<between>';
  const disposable = master.onWrite(([bytes, _ack]) => {
    if (!bytes || bytes.length === 0) return;
    // Per-action buffer — only meaningful while an action is active.
    if (currentLabel !== null) {
      const chunks = buffers.get(currentLabel) || [];
      chunks.push(new Uint8Array(bytes));
      buffers.set(currentLabel, chunks);
    }
    // Secondary flat buffer — ALWAYS append (Pitfall #5: preserves
    // cross-action line ordering for `after = "any"`).
    flat.push(new Uint8Array(bytes));
    // Transcript serial_chunk — append until the cumulative cap.
    const b64 = bytesToB64(bytes);
    if (transcriptOverflowed) {
      // Cap already reached on a prior chunk; drop further serial_chunk
      // events but keep running. Buffers above continue to grow so
      // assertion verdicts remain correct. Account for the dropped
      // bytes so the overflow event can report an honest truncated
      // count at scenario end (WR-01).
      transcriptBytesTruncated += b64.length;
      return;
    }
    if (transcriptBytes + b64.length > TRANSCRIPT_CAP_BYTES) {
      transcriptOverflowed = true;
      // The bytes_truncated_estimate is patched at scenario end with
      // the actual dropped-byte total (the entire current chunk plus
      // every subsequent chunk). Start the running count by counting
      // THIS chunk — it is the first dropped chunk.
      transcriptBytesTruncated += b64.length;
      overflowEvent = {
        ts: nowIsoUtc(),
        type: 'transcript_overflow',
        bytes_truncated_estimate: 0, // patched at scenario end
      };
      transcript.push(overflowEvent);
      return;
    }
    transcriptBytes += b64.length;
    transcript.push({
      ts: nowIsoUtc(),
      type: 'serial_chunk',
      // Tag inter-action bytes with `<between>` so the JSONL transcript
      // stays parseable; the Rust deserializer treats this as an
      // opaque string.
      action: currentLabel !== null ? currentLabel : BETWEEN_LABEL,
      bytes_b64: b64,
    });
  });

  /** demoted to 'fail' / 'timeout' / 'error' as failures accumulate */
  let scenarioVerdict = 'pass';
  /** human-readable error description (null on success) */
  let scenarioError = null;

  // Outer per-scenario timeout (RUN-06). Fallback 30 s.
  // WR-08 fix: use `??` so an explicit `timeout_ms: 0` in the config
  // is preserved (`||` would silently substitute 30_000 for any
  // falsy value including 0). A zero budget will resolve the race
  // arm immediately at scenario start, which is the documented
  // semantic for "scenario must already have passed when triggered".
  const scenarioBudgetMs = scenario.timeout_ms ?? 30_000;

  try {
    funnel.lockInput();

    const inner = async () => {
      for (const label of scenario.actions) {
        const action = actionsByLabel.get(label);
        if (!action) {
          actionResults.push({
            label,
            verdict: 'error',
            assertions: [],
            error: 'unknown action label',
          });
          scenarioVerdict = 'error';
          scenarioError = `unknown action '${label}'`;
          continue;
        }

        // Reset per-action buffer (RUN-07 default). currentLabel must
        // be assigned AFTER the reset so onWrite chunks go into the
        // fresh array.
        buffers.set(label, []);
        currentLabel = label;

        // Enqueue bytes (Pitfall #3: funnel.enqueue is lock-agnostic
        // by design — this is the SOLE byte-injection path the engine
        // uses, and it never consults funnel.locked).
        const b64Payload = action.bytes_b64 || '';
        const bytes = b64ToBytes(b64Payload);
        transcript.push({
          ts: nowIsoUtc(),
          type: 'action_send',
          action: label,
          bytes_b64: b64Payload,
        });
        if (bytes.length > 0) {
          funnel.enqueue(bytes, { pacingMs: 15 });
        }

        // Compute per-action timeout (Open Q5): MAX of relevant
        // assertion.timeout_ms; fallback 5_000 ms when there are no
        // relevant assertions.
        const actionTimeoutMs = perActionTimeoutMs(label, assertions);

        // Collect assertions whose `after` matches this label or "any".
        // `any` assertions evaluate against the FLAT buffer (Pitfall #5).
        const relevant = assertions.filter(a => a.after === label || a.after === 'any');

        // Poll until either all relevant assertions pass OR the
        // per-action timeout fires. Each assertion is "sticky" — once
        // it passes for this action's window, it stays passed.
        const passed = new Map(relevant.map(a => [a, false]));
        const pollDeadline = Date.now() + actionTimeoutMs;
        let pollResult = null;

        while (Date.now() < pollDeadline) {
          for (const a of relevant) {
            if (passed.get(a)) continue;
            // Pitfall #5: `any` reads from the flat append-only buffer
            // (preserves cross-action line order); per-action labels
            // read from this action's isolated buffer.
            const target = a.after === 'any' ? flat : (buffers.get(label) || []);
            if (evaluate(target, a, false)) passed.set(a, true);
          }
          if ([...passed.values()].every(Boolean)) {
            pollResult = 'all-passed';
            break;
          }
          await new Promise(r => setTimeout(r, 25));
        }
        if (pollResult === null) pollResult = 'timeout';

        // Final pass at timeout — include the partial trailing line
        // (RUN-05 line-buffer + timeout escape). Without this an
        // assertion targeting a prompt that lacks a trailing newline
        // (e.g. `login: `) would always fail.
        if (pollResult === 'timeout') {
          for (const a of relevant) {
            if (passed.get(a)) continue;
            const target = a.after === 'any' ? flat : (buffers.get(label) || []);
            if (evaluate(target, a, true)) passed.set(a, true);
          }
        }

        // Build per-assertion verdict list for this action and emit
        // assertion_result transcript events.
        const perAssert = relevant.map(a => {
          const v = passed.get(a) ? 'pass' : 'fail';
          transcript.push({
            ts: nowIsoUtc(),
            type: 'assertion_result',
            action: label,
            kind: a.kind,
            pattern: a.pattern,
            verdict: v,
          });
          return { kind: a.kind, pattern: a.pattern, verdict: v };
        });

        // Action verdict:
        //   - all assertions passed -> 'pass';
        //   - some failed AND we hit the per-action timeout -> 'timeout';
        //   - some failed without timing out (only possible if relevant
        //     is empty, which already maps to 'pass' via every) -> 'fail'.
        const allActionAssertionsPassed = perAssert.every(p => p.verdict === 'pass');
        let actionVerdict;
        if (allActionAssertionsPassed) {
          actionVerdict = 'pass';
        } else if (pollResult === 'timeout') {
          actionVerdict = 'timeout';
        } else {
          actionVerdict = 'fail';
        }

        actionResults.push({
          label,
          verdict: actionVerdict,
          assertions: perAssert,
        });
        if (actionVerdict !== 'pass') {
          // Demote scenario verdict on first non-pass; subsequent
          // failures don't further demote (e.g. a 'fail' after a
          // 'timeout' keeps the scenario at 'timeout').
          if (scenarioVerdict === 'pass') scenarioVerdict = actionVerdict;
        }

        // Reset currentLabel between actions so trailing-arrival chunks
        // do NOT pollute the next action's PER-ACTION buffer. They DO
        // still flow into `flat` (Pitfall #5: cross-action line order
        // preserved for `after = "any"`) and into the transcript with
        // an `<between>` action tag — see BL-02 fix in onWrite above.
        currentLabel = null;
      }
    };

    // WR-03 fix: capture the setTimeout handle so it can be cleared
    // when inner() wins the race. Without this, the timer keeps
    // running long after the scenario completes; when it fires it
    // mutates `scenarioVerdict` / `scenarioError` — harmless today
    // because the frame has already been built, but a real dead-write
    // surface for future readers AND a runtime-keepalive leak in
    // headed-debug sessions. We additionally gate the mutation on a
    // `completed` flag so the timeout handler is a no-op if inner()
    // has already resolved between the race winner being decided and
    // setTimeout's callback running.
    let completed = false;
    let timerHandle = null;
    const timeoutP = new Promise((resolve) => {
      timerHandle = setTimeout(() => resolve('timeout'), scenarioBudgetMs);
    });
    try {
      const winner = await Promise.race([
        inner().then(() => 'inner'),
        timeoutP,
      ]);
      if (winner === 'timeout' && !completed) {
        // The scenario budget fired before inner() resolved. We do
        // not forcibly cancel inner() — JS has no Promise
        // cancellation — but we record the verdict; the finally
        // block still cleans up the subscription and the lock.
        scenarioVerdict = 'timeout';
        scenarioError = `scenario timeout after ${scenarioBudgetMs}ms`;
      }
    } finally {
      completed = true;
      if (timerHandle !== null) clearTimeout(timerHandle);
    }
  } catch (e) {
    scenarioVerdict = 'error';
    scenarioError = String(e);
  } finally {
    // Pitfall #4: dispose the onWrite subscription. Wrapped in
    // try/catch because xterm-pty's Disposable contract does not
    // forbid throwing — defense-in-depth.
    try { disposable && disposable.dispose && disposable.dispose(); } catch (_e) {}
    // Always release the input lock so the page is usable again on
    // pass, fail, timeout, OR exception.
    try { funnel.unlockInput(); } catch (_e) {}
  }

  const endedAt = nowIsoUtc();
  // WR-01: patch the single transcript_overflow event (if any) with the
  // actual count of dropped serial_chunk bytes_b64 bytes, so the
  // emitted `bytes_truncated_estimate` matches the field name. The
  // previous implementation pinned this to `transcriptBytes` — the
  // bytes ACCEPTED before the cap, which is the opposite quantity.
  if (overflowEvent) {
    overflowEvent.bytes_truncated_estimate = transcriptBytesTruncated;
  }
  // Append a terminal scenario_result event so the JSONL transcript
  // is self-describing without needing the WS frame's outer fields.
  transcript.push({
    ts: endedAt,
    type: 'scenario_result',
    verdict: scenarioVerdict,
    actions: actionResults,
  });

  const frame = {
    type: 'ScenarioResult',
    verdict: scenarioVerdict,
    scenario: scenario.name,
    started_at: startedAt,
    ended_at: endedAt,
    actions: actionResults,
    transcript,
    error: scenarioError,
  };

  try {
    ws.send(JSON.stringify(frame));
    await waitForFlush(ws);   // Pitfall #2
  } catch (e) {
    // Best-effort. The Rust outer timeout in run_cmd is the backstop.
    // Log to console for headed-debug runs so the cause is at least
    // visible in DevTools.
    console.warn('[scenario] ws.send(ScenarioResult) failed:', e);
  }
}

/* eslint-disable max-len */
/**
 * MANUAL TEST CASE — HEADED-BROWSER SMOKE (DEFERRED)
 *
 * Status: deferred per Phase 3 plan 03-11 precedent — qemu-wasm assets
 * are blocked by Phase-1 plan 01-02 (run the docker build). Once
 * assets are available, run this on the next interactive session.
 *
 * 1. Build:
 *      cargo build
 * 2. Write `/tmp/bootroom.toml` declaring one scenario:
 *      [[action]]
 *      label       = "reboot"
 *      group       = "default"
 *      bytes_b64   = "cmVib290DQ=="   # "reboot\r"
 *
 *      [[scenario]]
 *      name        = "boot_smoke"
 *      actions     = ["reboot"]
 *      timeout_ms  = 30000
 *      [[scenario.assert]]
 *      kind        = "contains"
 *      pattern     = "login: "
 *      after       = "reboot"
 *      timeout_ms  = 10000
 * 3. Place a kernel image at `/tmp/Image` (or use the spike-b fixture).
 * 4. Run:
 *      ./target/debug/bootroom serve \
 *        --kernel /tmp/Image \
 *        --config /tmp/bootroom.toml \
 *        --no-open
 * 5. Open `http://127.0.0.1:8765/?scenario=boot_smoke` in Chrome /
 *    Chromium / Firefox.
 * 6. Confirm:
 *      a. The terminal locks immediately (status pill flips to `BUSY`;
 *         action buttons become disabled; manual keystrokes are not
 *         delivered to the guest).
 *      b. The `reboot` action plays with visible 15 ms inter-byte
 *         pacing.
 *      c. Serial output for that action accumulates in xterm.
 *      d. The funnel unlocks on completion (status pill returns to
 *         normal; action buttons re-enable).
 *      e. A `ScenarioResult` frame is visible in DevTools Network →
 *         WS → /ws → messages, with `verdict` ∈ {`pass`, `fail`,
 *         `timeout`, `error`} and a populated `transcript` array.
 *      f. On verdict `pass` the page stays interactive; on verdict
 *         `fail` it stays interactive AND a `console.warn` is logged
 *         from this module for each failing assertion (added via the
 *         `assertion_result` transcript events — the actual warn lives
 *         in 04-09's app.js wire-up, not here).
 * 7. Type "approved" if the flow behaves as documented; else describe
 *    what diverged.
 */
/* eslint-enable max-len */

/* eslint-disable max-len */
/**
 * MANUAL TEST CASE — TRANSCRIPT CAP
 *
 * Verifies the 5 MB transcript cap fires and emits exactly one
 * `transcript_overflow` event. Does NOT require qemu-wasm — uses a
 * synthetic master + funnel + ws stub from the DevTools console.
 * Status: deferred to next interactive session (does NOT depend on
 * Phase-1 plan 01-02 — runs on any page `bootroom serve` is serving).
 *
 * 1. Open any page served by `bootroom serve` (so COOP/COEP are set
 *    and the module path resolves). E.g.
 *    `http://127.0.0.1:8765/?_=transcript-cap-test`.
 * 2. In DevTools Console, paste:
 *
 *      const { runScenario } = await import('./scenario.js');
 *      const sent = [];
 *      const ws = { bufferedAmount: 0, send: f => sent.push(JSON.parse(f)) };
 *      const funnel = { lockInput(){}, unlockInput(){}, enqueue(){} };
 *      let writeCb;
 *      const master = { onWrite: cb => { writeCb = cb; return { dispose(){} }; } };
 *      const promise = runScenario(
 *        { name: 'cap', actions: ['a'], assert: [], timeout_ms: 60000 },
 *        [{ label: 'a', bytes_b64: '' }],
 *        { ws, funnel, master }
 *      );
 *      // Pump 6 MB of synthetic bytes — 6_000 chunks of 1 KB each.
 *      // Each 1 KB chunk yields a ~1366-char base64 string, so the
 *      // cumulative base64 byte count is ~6_000 * 1366 ≈ 8.2 MB —
 *      // the 5 MB cap fires well before chunk 4_000.
 *      await new Promise(r => setTimeout(r, 50)); // let onWrite subscribe
 *      const oneKb = new Uint8Array(1024);
 *      for (let i = 0; i < 6_000; i++) writeCb([oneKb, () => {}]);
 *      // Wait out the per-action timeout (5 s by default since no
 *      // assertions reference action `a`). The scenario then resolves
 *      // naturally because the action loop finishes.
 *      await promise;
 *
 * 3. Inspect `sent[0].transcript`:
 *      - Count `serial_chunk` events: MUST be < 6_000 (capped before
 *        the full 6 MB has been logged).
 *      - Count `transcript_overflow` events: MUST be exactly 1.
 *      - The `transcript_overflow.bytes_truncated_estimate` is the
 *        COUNT OF DROPPED serial_chunk bytes_b64 bytes after the cap
 *        fired (WR-01). With 6_000 * 1 KB raw chunks → ~8.2 MB total
 *        base64 → ~3.2 MB truncated (8.2 MB total minus ~5 MB
 *        accepted, with a chunk-sized tolerance).
 *
 *      Snippet to evaluate in DevTools:
 *        const t = sent[0].transcript;
 *        const chunks = t.filter(e => e.type === 'serial_chunk').length;
 *        const overflows = t.filter(e => e.type === 'transcript_overflow');
 *        console.log({ chunks, overflowCount: overflows.length,
 *                      estimate: overflows[0]?.bytes_truncated_estimate });
 *        console.assert(chunks < 6_000, 'serial_chunk count not capped');
 *        console.assert(overflows.length === 1, 'transcript_overflow not unique');
 *        // After WR-01: bytes_truncated_estimate ≈ total_b64 - accepted ≈ 3.2 MB.
 *        console.assert(overflows[0].bytes_truncated_estimate > 2_000_000 &&
 *                       overflows[0].bytes_truncated_estimate < 4_000_000,
 *                       'bytes_truncated_estimate outside expected dropped-byte range');
 *
 * 4. Type "approved" if the cap fires as documented; else describe
 *    what diverged.
 */
/* eslint-enable max-len */
