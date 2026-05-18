/**
 * funnel.js — single-writer funnel to xterm-pty's guest stdin (ldisc input).
 *
 * THIS MODULE IS THE WS-02 MITIGATION. Every host->guest byte MUST flow
 * through one of the helpers in this file. Do not call
 * `ldisc.writeFromLower(...)` (or any other guest-input path) from
 * anywhere else in the codebase. The xterm Terminal is wired with
 * `attachCustomKeyEventHandler(evt => { funnel.enqueue(keyEventToBytes(evt));
 * return false; })` so xterm's default `onData` path is suppressed and the
 * funnel is the only path to the guest. See 02-RESEARCH.md Pitfall #1
 * (master addon double-subscription) for the trap this avoids.
 *
 * CR-01 (Phase 2 review): the funnel previously called `slave.write([b])`,
 * which xterm-pty routes through `ldisc.writeFromUpper` -> OPOST ->
 * `_onWriteToLower` -> the terminal DISPLAY. That is the
 * process-stdout direction (guest -> terminal), not the input direction
 * (terminal -> guest). User keystrokes appeared on screen but never
 * reached qemu-wasm's PTY shim. The correct path for INPUT is
 * `ldisc.writeFromLower([b])` -> `inputFromLowerWithPreprocess` ->
 * `outputToUpper` -> `flushToUpper` -> `_onWriteToUpper` ->
 * `slave.fromLdiscToUpperBuffer` -> `slave.read()` (which qemu-wasm's
 * PTY shim drains). The funnel now plumbs `ldisc` through its
 * constructor and calls `this.ldisc.writeFromLower([b])` in the drain
 * loop. Display echo (when termios `ECHO_P` is set) is performed by
 * `inputFromLowerWithPreprocess` as a side effect, so visual feedback
 * for user typing is preserved unchanged.
 *
 * The `slave` reference is retained for any future helper that needs the
 * display path (out-of-band diagnostic writes from app.js still use
 * `slave.write` directly per <documented_exceptions> in 02-CONTEXT.md;
 * those are correctly using the display path and must not change).
 *
 * Pacing semantics (WS-03): `pacingMs` is the delay BETWEEN bytes, not
 * before the first byte. A 5-byte enqueue with `pacingMs=20` takes 80ms
 * total: 4 inter-byte gaps × 20ms. The first byte is written immediately.
 *
 * Locked decision: see 02-CONTEXT.md `<decisions>` → "Keyboard input +
 * client-side write funnel". User typing enqueues with pacingMs=0;
 * WS-arriving SerialIn frames enqueue with the configured pacing
 * (default 15ms, overridable via `?pacing=N` URL param in plan 02-06).
 *
 * Single-writer invariant (WS-02): a single `draining` flag serializes
 * the drain loop so concurrent enqueue calls cannot spawn a second pump.
 * Bytes are written in FIFO order via `this.ldisc.writeFromLower([b])`
 * one byte at a time. See 02-RESEARCH.md Pitfall #7 for the drain
 * pattern; Pitfall #8 (concurrent UI/scenario reorder) for why this
 * funnel exists at all.
 *
 * Note: `queue` and `draining` are conventionally-public fields (JS has no
 * enforceable instance privacy for non-`#` fields). Do not mutate them
 * directly; defense-in-depth only — bootroom is a loopback dev tool, no
 * adversarial caller is in scope.
 */

export class Funnel {
  /**
   * @param {object} slave xterm-pty slave (retained for future helpers and
   *   for symmetry with the documented out-of-band display path).
   * @param {object} ldisc xterm-pty line discipline (held on the master
   *   addon as `master.ldisc`). This is the INPUT path: bytes written
   *   via `ldisc.writeFromLower(bytes)` flow into the slave's
   *   upper-buffer and are drained by `slave.read()` (the qemu-wasm
   *   PTY shim's input source).
   */
  constructor(slave, ldisc) {
    this.slave = slave;
    this.ldisc = ldisc;
    /** Array of [byte:number, pacingMs:number] tuples. */
    this.queue = [];
    this.draining = false;
  }

  /**
   * Enqueue bytes for delivery to the guest.
   * @param {Uint8Array|number[]} bytes
   * @param {{pacingMs?: number}} opts pacingMs delays BETWEEN bytes (0 = no pacing)
   */
  enqueue(bytes, opts = {}) {
    const pacingMs = opts.pacingMs ?? 0;
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    for (let i = 0; i < u8.length; i++) this.queue.push([u8[i], pacingMs]);
    if (!this.draining) {
      this.draining = true;
      this.#drain().finally(() => { this.draining = false; });
    }
  }

  async #drain() {
    while (this.queue.length > 0) {
      const [b, ms] = this.queue.shift();
      // INPUT path: bytes from the user (keystrokes) or WS-arriving
      // SerialIn frames head to the guest's stdin via
      // ldisc.writeFromLower -> _onWriteToUpper ->
      // slave.fromLdiscToUpperBuffer -> slave.read (which qemu-wasm's
      // PTY shim drains). Verified against vendored xterm-pty.js
      // (writeFromLower in the d/ldisc class).
      //
      // WR-05: writeFromLower does not have the same flowActivated
      // guard as writeFromUpper, so it cannot throw on XOFF. We still
      // wrap defensively in case a future xterm-pty bump changes that;
      // dropping a single byte is preferable to crashing the drain
      // loop and silently breaking all subsequent input.
      try {
        this.ldisc.writeFromLower([b]);
      } catch (e) {
        console.warn('[funnel] writeFromLower threw; dropping byte', b, e);
      }
      if (ms > 0) await new Promise(r => setTimeout(r, ms));
    }
  }
}

/**
 * Encode bytes to base64. Handles bytes >= 0x80 correctly (avoids the
 * Latin-1 `btoa` trap — see 02-RESEARCH.md Pitfall #3). Chunked to avoid
 * V8's argument-count limit on `String.fromCharCode.apply` for large
 * buffers (some engines cap at ~125k args).
 * @param {Uint8Array|number[]} bytes
 * @returns {string} base64-encoded payload
 */
export function bytesToB64(bytes) {
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let s = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < u8.length; i += CHUNK) {
    s += String.fromCharCode.apply(null, u8.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

/**
 * Decode base64 to bytes. Inverse of `bytesToB64`. Round-trips bytes
 * 0x00-0xff losslessly.
 * @param {string} b64
 * @returns {Uint8Array}
 */
export function b64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * Minimal KeyboardEvent -> bytes translator covering the keys a kernel
 * REPL typically needs. Returns Uint8Array, or null if the event has no
 * byte representation (modifier-only keys, unknown keys). Callers may
 * log nulls at console.debug; this module is intentionally silent.
 * @param {KeyboardEvent} evt
 * @returns {Uint8Array|null}
 */
export function keyEventToBytes(evt) {
  // Ignore pure-modifier keypresses.
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(evt.key)) return null;

  // Ctrl-letter: emit C-x control byte.
  if (evt.ctrlKey && evt.key.length === 1) {
    const c = evt.key.toUpperCase().charCodeAt(0);
    if (c >= 0x40 && c <= 0x5f) return new Uint8Array([c - 0x40]);
  }

  // Special keys.
  switch (evt.key) {
    case 'Enter':      return new Uint8Array([0x0d]); // CR; ldisc handles ICRNL
    case 'Backspace':  return new Uint8Array([0x7f]); // DEL; termios VERASE default
    case 'Tab':        return new Uint8Array([0x09]);
    case 'Escape':     return new Uint8Array([0x1b]);
    case 'ArrowUp':    return enc('\x1b[A');
    case 'ArrowDown':  return enc('\x1b[B');
    case 'ArrowRight': return enc('\x1b[C');
    case 'ArrowLeft':  return enc('\x1b[D');
    case 'Home':       return enc('\x1b[H');
    case 'End':        return enc('\x1b[F');
    case 'PageUp':     return enc('\x1b[5~');
    case 'PageDown':   return enc('\x1b[6~');
    case 'Delete':     return enc('\x1b[3~');
  }

  // Printable single character (covers most ASCII + UTF-8 BMP).
  if (evt.key.length === 1) return new TextEncoder().encode(evt.key);
  return null;
}

function enc(s) { return new TextEncoder().encode(s); }

/*
 * MANUAL TEST PLAN (no JS runner in Phase 2 — covered by headed-browser
 * smoke during Phase 2 wave-merge per 02-VALIDATION.md. Phase 3 may
 * revisit if a JS test harness is justified.)
 *
 * Open DevTools console on a page that imports this module and exposes
 * `funnel` globally (plan 02-06 will wire it). Run each block; observe
 * the expected outcome.
 *
 * 1. Single-writer (WS-02) + guest-input path (CR-01): subscribe to
 *    slave.onReadable, push known bytes through the funnel, drain
 *    slave.read() and assert byte equality. This proves bytes reach
 *    the guest-stdin upper buffer (qemu-wasm's PTY shim source),
 *    not just the display:
 *      const seen = [];
 *      const disp = funnel.slave.onReadable(() => {
 *        seen.push(...funnel.slave.read());
 *      });
 *      funnel.enqueue([0x68, 0x69], {pacingMs: 0});
 *      funnel.enqueue([0x21], {pacingMs: 0});
 *      await new Promise(r => setTimeout(r, 10));
 *      disp.dispose();
 *      console.assert(JSON.stringify(seen) === '[104,105,33]');
 *    Expected: bytes arrive in enqueue order; no interleaving; all
 *    three bytes are read out of slave.read() (guest stdin).
 *
 * 2. Pacing (WS-03): with `?pacing=50` in URL (or override locally):
 *      const t0 = performance.now();
 *      funnel.enqueue([0x61, 0x62, 0x63], {pacingMs: 50});
 *      while (funnel.queue.length > 0) await new Promise(r => setTimeout(r, 5));
 *      console.log('elapsed', performance.now() - t0);
 *    Expected: ~100ms (2 inter-byte gaps × 50ms; first byte is immediate).
 *
 * 3. Concurrency: rapid-fire 100 enqueues in a tight loop.
 *      let entered = 0, maxConcurrent = 0;
 *      // Temporarily instrument #drain by wrapping enqueue (see plan 06).
 *      for (let i = 0; i < 100; i++) funnel.enqueue([i & 0xff], {pacingMs: 0});
 *    Expected: only one drain loop active at any time (draining flag
 *    pattern from Pitfall #7); 100 bytes delivered exactly once each.
 *
 * 4. bytesToB64 round-trip (Pitfall #3 — Latin-1 btoa trap):
 *      const x = new Uint8Array([0xff, 0x80, 0x00, 0x7f]);
 *      const r = b64ToBytes(bytesToB64(x));
 *      console.assert(r.length === 4 && r[0]===0xff && r[1]===0x80
 *                     && r[2]===0x00 && r[3]===0x7f);
 *    Expected: byte-for-byte identical; no throw on high bytes.
 *
 * 5. keyEventToBytes coverage:
 *      const ke = (init) => new KeyboardEvent('keydown', init);
 *      console.assert(keyEventToBytes(ke({key:'Enter'}))[0] === 0x0d);
 *      console.assert(keyEventToBytes(ke({key:'Backspace'}))[0] === 0x7f);
 *      console.assert(keyEventToBytes(ke({key:'a'}))[0] === 0x61);
 *      console.assert(keyEventToBytes(ke({key:'c', ctrlKey:true}))[0] === 0x03);
 *      const up = keyEventToBytes(ke({key:'ArrowUp'}));
 *      console.assert(up.length === 3 && up[0]===0x1b && up[1]===0x5b && up[2]===0x41);
 *      console.assert(keyEventToBytes(ke({key:'Shift'})) === null);
 *    Expected: each assertion passes.
 */
