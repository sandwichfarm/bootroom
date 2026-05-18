// crates/bootroom/web/app.js
//
// Phase 2 UI entrypoint. Loaded as `<script type="module">` from index.html.
//
// Depends on these globals, populated by the classic scripts that index.html
// loads BEFORE this module runs:
//   window.Terminal — xterm.js 5.3.0
//   window.openpty  — xterm-pty 0.12.0
//   window.Module   — QEMU argv (set by /assets/qemu/module.js)
//
// Browsers defer `type="module"` scripts by default, so all four classic
// <script> tags above this one have executed when this file starts.
//
// Phase 2 wires the interactive layer:
//  - funnel-mounted xterm input (single writer to slave.write per WS-02)
//  - WS /ws lifecycle (Hello / SerialIn / SerialOut / Launch / Reset / State)
//  - SerialOut mirror: guest serial -> WS server (batched per readable burst)
//  - Status pill state machine: IDLE -> LOADING -> RUNNING -> HALTED
//    (with WS State{} frames overriding local lifecycle when present)
//  - LAUNCH / RESET / CLEAR / COPY button handlers (UI-04, UI-08, UI-09).

import { Funnel, bytesToB64, b64ToBytes } from './funnel.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Format a byte count using IEC binary units per 01-UI-SPEC:
 * one decimal place when the value is >= 10, two decimals below.
 */
function humanBytes(n) {
  // WR-07: defend against null/undefined, non-numeric, negative, NaN
  // and ±Infinity inputs the kernel-info API should never produce but
  // might in error paths. Coerce through Number() FIRST so legitimate
  // numeric strings ("12345") still format, then range-check.
  if (n == null) return '—';
  const num = Number(n);
  if (!Number.isFinite(num) || num < 0) return '—';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = num;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return value.toFixed(0) + ' ' + units[unit];
  const decimals = value >= 10 ? 1 : 2;
  return value.toFixed(decimals) + ' ' + units[unit];
}

/**
 * Format an epoch-seconds timestamp as ISO-8601 in the local timezone with
 * second precision, e.g. "2026-05-17T14:32:08-07:00". UI-SPEC bans
 * "5 minutes ago" — a test harness needs exact timestamps.
 */
function isoLocal(epochSec) {
  if (epochSec == null) return '—';
  const d = new Date(Number(epochSec) * 1000);
  if (isNaN(d.getTime())) return '—';
  const pad = (n) => String(n).padStart(2, '0');
  const y = d.getFullYear();
  const mo = pad(d.getMonth() + 1);
  const da = pad(d.getDate());
  const h = pad(d.getHours());
  const mi = pad(d.getMinutes());
  const s = pad(d.getSeconds());
  // getTimezoneOffset returns minutes WEST of UTC; flip the sign for ISO.
  const tzMin = -d.getTimezoneOffset();
  const sign = tzMin >= 0 ? '+' : '-';
  const absTz = Math.abs(tzMin);
  const tzH = pad(Math.floor(absTz / 60));
  const tzM = pad(absTz % 60);
  return `${y}-${mo}-${da}T${h}:${mi}:${s}${sign}${tzH}:${tzM}`;
}

// ---------------------------------------------------------------------------
// Status pill
// ---------------------------------------------------------------------------

const pill = document.getElementById('status');
function setPill(state) {
  pill.dataset.state = state;
  pill.innerHTML = '<span aria-hidden="true">●</span> ' + state;
}

// ---------------------------------------------------------------------------
// Kernel-info fetch + header population
// ---------------------------------------------------------------------------

async function loadKernelInfo() {
  try {
    const res = await fetch('/api/kernel/info');
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const info = await res.json();
    document.getElementById('k-path').textContent  = info.path;
    document.getElementById('k-size').textContent  = humanBytes(info.size);
    document.getElementById('k-mtime').textContent = isoLocal(info.mtime);
    document.getElementById('k-sha').textContent   = info.sha256_prefix;
    const base = (info.path || '').split('/').pop() || info.path || 'kernel';
    document.title = 'bootroom — ' + base;
  } catch (e) {
    console.error('kernel-info fetch failed:', e);
    for (const id of ['k-path', 'k-size', 'k-mtime', 'k-sha']) {
      const el = document.getElementById(id);
      if (el) el.textContent = 'ERR';
    }
  }
}

// ---------------------------------------------------------------------------
// xterm + xterm-pty mount
// ---------------------------------------------------------------------------

// WR-01: vendor scripts may fail to load (404, CORP/COEP violation, parse
// error). Without this guard the next line throws a synchronous ReferenceError
// at module evaluation time, neither loadKernelInfo() nor bootGuest() ever
// run, and the status pill stays "LOADING" forever with no UI feedback.
// Surface the failure in the iso-banner (the only always-visible alert
// region on the page) and HALT the pill.
if (typeof Terminal !== 'function' || typeof openpty !== 'function') {
  setPill('HALTED');
  const banner = document.getElementById('iso-banner');
  if (banner) {
    banner.removeAttribute('hidden');
    banner.innerHTML =
      '<strong>Bootroom UI failed to start.</strong>' +
      '<p>Vendor scripts (xterm.js / xterm-pty) did not load — ' +
      '<code>Terminal</code> or <code>openpty</code> is undefined. ' +
      'Open DevTools → Network and check for 4xx responses or CORP/COEP ' +
      'violations on <code>/assets/web/vendor/*</code>.</p>';
  }
  // Throwing here prevents the rest of the module from running with
  // undefined globals (which would only produce confusing downstream
  // errors in the console).
  throw new Error('vendor globals missing (Terminal or openpty)');
}

// Defensive: assert IDLE at startup. plan-05 sets data-state="IDLE" in HTML
// already; this call makes the JS state machine's initial transition
// explicit and tolerates anyone re-rendering the markup later.
setPill('IDLE');

const xterm = new Terminal();
xterm.open(document.getElementById('terminal'));

const { master, slave } = openpty();

// The Funnel is the SOLE writer to the guest-input path
// (ldisc.writeFromLower) during normal byte flow (WS-02).
const funnel = new Funnel(slave, master.ldisc);

// --- Single-writer wiring (replaces xterm.loadAddon(master)) ----------------
//
// Background: xterm-pty's master.activate() (called by xterm.loadAddon)
// auto-wires both directions, including `xterm.onData → ldisc.writeFromLower`.
// If we ALSO funnel keystrokes via attachCustomKeyEventHandler, every printable
// character lands twice (post-CR-01 smoke caught this: "ls" appeared as "llss").
// attachCustomKeyEventHandler only intercepts `keydown` and cannot cancel the
// `keypress`/`input` path that xterm derives its data emission from, so even
// returning `false` from a keydown handler does not suppress master's onData
// listener. The two write paths therefore both fire on every keystroke.
//
// Fix: skip loadAddon entirely. Wire each direction explicitly so the funnel
// is genuinely the only producer of guest-input bytes.
//
// OUTPUT path (guest → display): master.onWrite emits [Uint8Array, ackCallback]
// every time ldisc has bytes from writeFromUpper. xterm.write consumes the ack
// to drive backpressure — we plumb it straight through. A second observer
// (the SerialOut WS mirror further below) subscribes to the same event without
// touching the ack.
master.onWrite(([bytes, ack]) => xterm.write(bytes, ack));

// INPUT path (xterm → guest): the funnel is the single writer. xterm.onData
// hands us a UTF-8 string per key event (covers printable chars, paste, Enter,
// arrow keys, Ctrl-letter, etc. — xterm's own keymapping); we encode to bytes
// and enqueue. Setting `pacingMs: 0` gives native-speed typing.
xterm.onData((data) => {
  const bytes = new TextEncoder().encode(data);
  if (bytes.length > 0) funnel.enqueue(bytes, { pacingMs: 0 });
});

// Paste of binary data follows the same path.
xterm.onBinary((data) => {
  const bytes = new TextEncoder().encode(data);
  if (bytes.length > 0) funnel.enqueue(bytes, { pacingMs: 0 });
});

// Forward TIOCGWINSZ-relevant resize events so the guest's termios can react.
xterm.onResize(({ cols, rows }) => master.notifyResize(rows, cols));

// window.Module was set by /assets/qemu/module.js (the QEMU argv).
// Wire the PTY slave into qemu-wasm's chardev (xterm-pty was linked in
// at qemu-wasm build time via --js-library=…/xterm-pty/emscripten-pty.js).
Module.pty = slave;
Module.mainScriptUrlOrBlob = location.origin + '/assets/qemu/out.js';

// Terminal resize handler — recompute xterm's cell grid from the container's
// live offsetWidth/Height. We can't pull in xterm's FitAddon (not vendored)
// so we read the renderer's actual cell dimensions and call Terminal.resize
// directly. Falls back gracefully if the private renderer API is unavailable.
function fitTerminalToContainer() {
  const container = document.getElementById('terminal');
  if (!container) return;
  const w = container.clientWidth;
  const h = container.clientHeight;
  if (w <= 0 || h <= 0) return;
  // xterm's internal renderer exposes per-cell pixel size; path differs
  // across xterm.js minor versions, so probe both shapes.
  let cellW = 0, cellH = 0;
  try {
    const dims = xterm._core?._renderService?.dimensions;
    cellW = dims?.css?.cell?.width ?? dims?.actualCellWidth ?? 0;
    cellH = dims?.css?.cell?.height ?? dims?.actualCellHeight ?? 0;
  } catch (_e) { /* private API moved; fall back */ }
  if (cellW > 0 && cellH > 0) {
    const cols = Math.max(20, Math.floor(w / cellW));
    const rows = Math.max(5, Math.floor(h / cellH));
    try { xterm.resize(cols, rows); } catch (_e) { /* tolerate */ }
  }
}
window.addEventListener('resize', fitTerminalToContainer);
// Best-effort initial fit before the runtime is up — xterm has just
// rendered its initial 80x24 grid so cell dimensions are measurable.
// bootGuest's onRuntimeInitialized also re-fits after the runtime starts.
requestAnimationFrame(fitTerminalToContainer);

// xterm has been mounted; transition the pill IDLE -> LOADING. RUNNING
// will be set only once BOTH onRuntimeInitialized has fired AND the first
// SerialOut byte has been observed (Pattern 5 from 02-RESEARCH.md).
setPill('LOADING');

// ---------------------------------------------------------------------------
// Status pill state machine (Pattern 5 from 02-RESEARCH.md)
// ---------------------------------------------------------------------------

let runtimeInitialized = false;
let firstSerialOutSeen = false;
// When the WS server pushes a State{} frame, it wins over local lifecycle
// (per CONTEXT.md "Browser auto-open + status pill source"). Set to null
// when no server authority is in effect; uppercased string otherwise.
let serverStateAuthority = null;

function recomputePillLocal() {
  if (serverStateAuthority !== null) {
    setPill(serverStateAuthority);
    return;
  }
  if (runtimeInitialized && firstSerialOutSeen) setPill('RUNNING');
  else if (runtimeInitialized) setPill('LOADING');
  // IDLE and HALTED are set explicitly by their triggers, not derived here.
}

// ---------------------------------------------------------------------------
// WebSocket /ws lifecycle
// ---------------------------------------------------------------------------

// Pacing config for WS-arriving SerialIn (WS-03). Default 15ms per
// 02-CONTEXT.md. Override via ?pacing=N query param. Negative values
// are clamped to 0 (no pacing).
//
// WR-04: validate the query value explicitly. Math.max(0, Number('abc'))
// is NaN, and setTimeout(_, NaN) silently coerces to 0 — which is the
// opposite of fail-safe (the user asked to SLOW DOWN injection because
// the kernel was missing keystrokes, and instead got firehose pacing).
// Fall back to the documented 15ms default on any non-finite or negative
// input, with a one-line console.warn so typos are discoverable.
const urlParams = new URLSearchParams(location.search);
const PACING_DEFAULT_MS = 15;
const rawPacing = urlParams.get('pacing');
const parsedPacing = rawPacing === null ? PACING_DEFAULT_MS : Number(rawPacing);
const pacingMs = (Number.isFinite(parsedPacing) && parsedPacing >= 0)
  ? parsedPacing
  : (console.warn('[bootroom] ?pacing=' + rawPacing + ' is not a non-negative number; falling back to ' + PACING_DEFAULT_MS + 'ms'),
     PACING_DEFAULT_MS);

// Module-scope so the SerialOut mirror (subscribed on slave.onReadable
// below) can read it after this module finishes evaluating. Assigned
// inside connectWs(); may be null between disconnect and reconnect.
let ws = null;

// WR-03: dedupe reconnects. Both `onclose` and the synchronous
// `new WebSocket` catch fall through to scheduleReconnect; without a
// guard, an `error` event followed immediately by `close` (the common
// "server died" sequence) plus the constructor-throws edge could
// stack two pending timers and rapid-fire two connectWs calls. Also
// guards the cross-instance race where a stale prior socket's onclose
// fires after a fresh one has been opened.
let reconnectTimer = null;

function scheduleReconnect() {
  if (reconnectTimer !== null) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectWs();
  }, 1000);
}

function connectWs() {
  const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
  try {
    ws = new WebSocket(url);
  } catch (e) {
    console.warn('[bootroom] /ws constructor failed:', e);
    scheduleReconnect();
    return;
  }
  ws.onopen = () => { /* nothing — first frame is server Hello, handled below */ };
  ws.onmessage = (ev) => {
    let frame;
    try { frame = JSON.parse(ev.data); }
    catch (e) { console.warn('[bootroom] /ws: bad JSON:', e); return; }
    handleWsFrame(frame);
  };
  ws.onclose = () => {
    try { slave.write('[bootroom] /ws disconnected; reconnecting…\r\n'); } catch (_e) {}
    scheduleReconnect(); // naive retry per <deferred>; T-02-25 accept
  };
  ws.onerror = (e) => { console.warn('[bootroom] /ws error:', e); };
}

function handleWsFrame(frame) {
  if (!frame || typeof frame.type !== 'string') return;
  // T-02-24: each branch wraps its own work in try/catch so one bad frame
  // cannot break onmessage.
  switch (frame.type) {
    case 'Hello':
      // Validate version compatibility (log; do not block — UI-SPEC line 136).
      // The bootroom binary's CARGO_PKG_VERSION is not exposed to the browser
      // (no meta tag yet), so we report whatever the server advertises and
      // accept it. Phase 6 may revisit a strict-mismatch warning.
      try {
        slave.write(
          '[bootroom] /ws connected (server version ' +
          (frame.version || 'unknown') + ')\r\n'
        );
      } catch (_e) { /* slave may not be ready */ }
      break;
    case 'SerialIn':
      try {
        const bytes = b64ToBytes(frame.data || '');
        funnel.enqueue(bytes, { pacingMs });
      } catch (e) {
        console.warn('[bootroom] /ws SerialIn decode failed:', e);
      }
      break;
    case 'State':
      if (typeof frame.state === 'string') {
        // GuestState serializes as PascalCase ("Idle"/"Loading"/"Running"/
        // "Halted"); the pill uses UPPERCASE. Normalize here.
        serverStateAuthority = frame.state.toUpperCase();
        setPill(serverStateAuthority);
      }
      break;
    default:
      console.debug('[bootroom] /ws: unhandled frame type', frame.type);
  }
}

// ---------------------------------------------------------------------------
// SerialOut mirror: master.onWrite -> WS SerialOut frame
// ---------------------------------------------------------------------------
//
// CRITICAL DIRECTION NOTE (fix for the Phase 2 input-path bug):
//
// In xterm-pty the byte flow is:
//   HOST -> GUEST (input):  ldisc.writeFromLower -> slave.fromLdiscToUpperBuffer
//                           -> slave.onReadable -> slave.read (drained by qemu)
//   GUEST -> HOST (output): qemu's slave.write -> ldisc.writeFromUpper
//                           -> master.fromLdiscToLowerBuffer -> master.onWrite
//                           -> xterm.write (display)
//
// The SerialOut mirror must observe the GUEST->HOST direction, i.e. master.onWrite,
// NOT slave.onReadable. Subscribing slave.onReadable + calling slave.read() drains
// the HOST->GUEST input buffer before qemu's worker can re-read it, eating user
// keystrokes (Phase-2 post-fix smoke discovery; see funnel.js doc comment for
// the analogous CR-01 direction-swap bug).
//
// master.onWrite delivers [Uint8Array, ackCallback] to listeners. Multiple
// listeners each get their own invocation; the ackCallback is plumbing for
// xterm.write's backpressure ack chain — we MUST NOT call it (master.activate
// already wires xterm's listener which calls ack when xterm finishes writing).
// Our listener only observes.
master.onWrite(([bytes, _ack]) => {
  if (!bytes || bytes.length === 0) return;
  if (!firstSerialOutSeen) {
    firstSerialOutSeen = true;
    recomputePillLocal(); // may transition LOADING -> RUNNING
  }
  if (ws && ws.readyState === WebSocket.OPEN) {
    try {
      ws.send(JSON.stringify({ type: 'SerialOut', data: bytesToB64(bytes) }));
    } catch (e) {
      console.warn('[bootroom] SerialOut send failed:', e);
    }
  }
});

// ---------------------------------------------------------------------------
// Button handlers (UI-04, UI-08, UI-09)
// ---------------------------------------------------------------------------

function disableHeaderButtons() {
  const launch = document.getElementById('btn-launch');
  const reset = document.getElementById('btn-reset');
  if (launch) launch.disabled = true;
  if (reset) reset.disabled = true;
}

// LAUNCH (UI-08): best-effort notify the server, then reload the page.
// Reload is the canonical "re-instantiate qemu-wasm with fresh kernel"
// gesture (Spike A verdict; 02-CONTEXT.md "Launch / Reset" decision).
document.getElementById('btn-launch').addEventListener('click', () => {
  disableHeaderButtons();
  try {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'Launch' }));
    }
  } catch (e) { console.warn('[bootroom] Launch send failed:', e); }
  // rAF gives the browser one frame to flush the WS send before the
  // navigation tears down the document.
  requestAnimationFrame(() => window.location.reload());
});

// RESET (UI-09): per 02-CONTEXT.md "Launch / Reset" decision, Phase 2
// makes the two visually distinct but behaviorally identical.
document.getElementById('btn-reset').addEventListener('click', () => {
  disableHeaderButtons();
  try {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'Reset' }));
    }
  } catch (e) { console.warn('[bootroom] Reset send failed:', e); }
  requestAnimationFrame(() => window.location.reload());
});

// CLEAR (UI-04): wipe scrollback + viewport. No WS traffic.
document.getElementById('btn-clear').addEventListener('click', () => {
  xterm.clear();
});

// COPY (UI-04): copy current selection, or full active buffer if no
// selection. Flash COPIED / COPY FAILED for 1500ms, then revert. On
// failure also write a diagnostic line to the terminal so the failure
// leaves an audit trail (T-02-29 mitigation).
const copyBtn = document.getElementById('btn-copy');
// WR-06: cache the canonical button label OUTSIDE the click handler so
// a rapid second click (or a click that catches the button mid-flash)
// doesn't capture 'COPIED' or 'COPY FAILED' as its "original" label
// and freeze the button on that transient text forever. Track the
// pending revert timer so a new click cancels the old revert before
// scheduling its own.
const COPY_LABEL = copyBtn.textContent || 'COPY';
let copyRevertTimer = null;
copyBtn.addEventListener('click', async () => {
  // Per 02-RESEARCH.md Pitfall #5: endRow = xterm.buffer.active.length is
  // correct for xterm.js 5.3.0 (length is one-past-the-last index).
  // trimRight: true trims trailing whitespace per row (UX choice).
  const selection = xterm.getSelection();
  const text = selection ||
    xterm.buffer.active.translateToString(true, 0, xterm.buffer.active.length);
  try {
    await navigator.clipboard.writeText(text);
    copyBtn.textContent = 'COPIED';
  } catch (e) {
    copyBtn.textContent = 'COPY FAILED';
    try {
      slave.write('[bootroom] Copy failed: ' + (e?.message || e) + '\r\n');
    } catch (_e) { /* slave may not be ready */ }
  }
  if (copyRevertTimer !== null) clearTimeout(copyRevertTimer);
  copyRevertTimer = setTimeout(() => {
    copyBtn.textContent = COPY_LABEL;
    copyRevertTimer = null;
  }, 1500);
});

// ---------------------------------------------------------------------------
// Boot the guest
// ---------------------------------------------------------------------------

async function bootGuest() {
  // Fetch the kernel bytes BEFORE calling initEmscriptenModule and stash
  // them on Module so the onRuntimeInitialized callback can write them
  // into the emscripten FS after the data pack has finished extracting.
  let pendingKernel = null;
  try {
    const res = await fetch('/kernel');
    if (!res.ok) throw new Error('kernel fetch HTTP ' + res.status);
    pendingKernel = new Uint8Array(await res.arrayBuffer());
  } catch (e) {
    console.error('kernel fetch failed:', e);
    slave.write('[bootroom] Failed to load kernel from /kernel: ' + e.message + '\r\n');
    setPill('HALTED');
    return;
  }

  // Hook lifecycle callbacks. onRuntimeInitialized fires AFTER preRun
  // completes (data pack extracted, /pack/ populated) and BEFORE callMain
  // — the only safe window to swap /pack/Image with the user's kernel.
  //
  // We can't write in preRun: emscripten's addOnPreRun uses unshift, which
  // reverses the FIFO queue order. Our callback would land FIRST in
  // __ATPRERUN__, creating /pack/Image before the data pack extraction
  // runs. The data pack then collides on FS.mayCreate → throws errno 20
  // (EEXIST in musl). onRuntimeInitialized is the natural overwrite point.
  //
  // Module.FS isn't exposed publicly on this emscripten build; we use the
  // wrapper functions Module exposes (FS_unlink, FS_createDataFile).
  Module.onRuntimeInitialized = () => {
    runtimeInitialized = true;
    // WR-08: emscripten's FS errors expose .errno; 44 = ENOENT in the
    // musl errno table emscripten uses ("file not yet present", the
    // common case on first boot). Anything else (EROFS, EACCES, EBUSY,
    // unexpected internal-FS state) means the subsequent
    // FS_createDataFile is going to misfire with EEXIST or worse; bail
    // now with the real cause rather than letting the user chase a
    // misleading downstream error.
    try {
      Module.FS_unlink('/pack/Image');
    } catch (e) {
      if (e && e.errno !== undefined && e.errno !== 44) {
        console.error('FS_unlink failed unexpectedly:', e);
        try { slave.write('[bootroom] Cannot replace /pack/Image: ' + (e.message || e) + '\r\n'); } catch (_e2) {}
        setPill('HALTED');
        return;
      }
      // errno === 44 (ENOENT) or no errno field at all — treat as
      // "file not present yet"; this is the expected first-boot case.
    }
    try {
      Module.FS_createDataFile('/pack', 'Image', pendingKernel, true, true, true);
    } catch (e) {
      console.error('kernel inject failed:', e);
      try { slave.write('[bootroom] Failed to inject kernel: ' + e.message + '\r\n'); } catch (_e) {}
      setPill('HALTED');
      return;
    }
    // Pattern 5: runtime ready, but pill stays LOADING until the first
    // SerialOut byte arrives. recomputePillLocal handles both cases.
    recomputePillLocal();
    // Re-fit once xterm has begun streaming serial — cell dimensions are
    // most accurate now that the renderer has flushed at least one frame.
    requestAnimationFrame(fitTerminalToContainer);
  };
  Module.onExit = (_code) => {
    // Local lifecycle wins on terminal exit; clear any server authority
    // so subsequent recomputes use local truth (Pattern 5 line 617).
    serverStateAuthority = null;
    setPill('HALTED');
  };
  Module.onAbort = (_what) => {
    serverStateAuthority = null;
    setPill('HALTED');
  };

  // Dynamic import of the emscripten glue. Vendored at /assets/qemu/out.js.
  const mod = await import('/assets/qemu/out.js');
  const initEmscriptenModule = mod.default;
  await initEmscriptenModule(Module);

  // Reference's PTY poll patch — copy verbatim from
  // qemu-wasm/examples/riscv64/src/htdocs/index.html to avoid hangs when
  // the PTY's readable buffer is empty.
  const oldPoll = Module.TTY.stream_ops.poll;
  const pty = Module.pty;
  Module.TTY.stream_ops.poll = function (stream, timeout) {
    if (!pty.readable) {
      return (pty.readable ? 1 : 0) | (pty.writable ? 4 : 0);
    }
    return oldPoll.call(stream, timeout);
  };
}

// ---------------------------------------------------------------------------
// Kick off all flows
// ---------------------------------------------------------------------------

// Wire up WS first (cheap, non-blocking); then fetch info + boot guest.
connectWs();
loadKernelInfo();
bootGuest().catch((e) => {
  console.error('boot failed:', e);
  setPill('HALTED');
  try {
    slave.write('[bootroom] Boot failed: ' + e.message + '\r\n');
  } catch (_e) { /* slave may not be ready */ }
});
