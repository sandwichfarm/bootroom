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

import { Funnel, bytesToB64, b64ToBytes, setLockObserver } from './funnel.js';

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
// Banner state machine + DOM-cached references (Phase 3)
// ---------------------------------------------------------------------------
//
// Phase 3 introduces three new banner surfaces (#actions-panel buttons,
// #fresh-banner for kernel rebuilds, #config-banner for TOML errors) plus
// a banner priority resolver enforcing the ladder iso > config-invalid >
// kernel-fresh (UI-SPEC Interaction Contract 2).
//
// Security note (T-03-11-01): banner body content originates from operator-
// controlled TOML field names (echoed by the `toml` crate's parse errors)
// and from kernel-watcher rejection reasons. ALL banner content insertion
// uses textContent (NOT the raw-HTML setter). The Phase-1+2 baseline count
// of raw-HTML setter calls is 2 (setPill + WR-01 iso-banner fallback);
// a grep gate in the plan's verify step pins it at 2 to catch regressions.
//
// Lock-aware caller note (T-03-11-02): xterm.onData, xterm.onBinary, and
// the action-button click delegate all short-circuit BEFORE funnel.enqueue
// when funnel.locked === true. The funnel itself is lock-agnostic so
// server-initiated WS SerialIn frames (scenario engine writes) keep flowing
// during the lock — see 03-CONTEXT.md "Funnel input lock primitive" and
// funnel.js doc comments. Any FUTURE caller-site that injects bytes into
// the funnel from a user-initiated event (paste handler, drag-drop, etc.)
// MUST add the same `if (funnel.locked) return;` guard.

const actionsPanel = document.getElementById('actions-panel');
const freshBanner = document.getElementById('fresh-banner');
const configBanner = document.getElementById('config-banner');
const isoBanner = document.getElementById('iso-banner');

/**
 * Banner state. `null` means absent / dismissed; otherwise the parsed WS
 * payload. resolveBanners() reads this object and applies the priority
 * ladder by toggling `hidden` on the lower-priority banners.
 */
const bannerState = { configInvalid: null, freshKernel: null };

/**
 * Apply the banner priority ladder per UI-SPEC Interaction Contract 2:
 *
 *   iso (Phase 1) > config-invalid (Phase 3) > kernel-fresh (Phase 3)
 *
 * Only one banner is visible at a time. The iso-banner is owned by the
 * inline SAB probe in index.html (and by the WR-01 vendor-load failsafe
 * at module load); this resolver only READS its hidden state and forces
 * the two Phase-3 banners hidden when iso is showing.
 */
function resolveBanners() {
  const isoActive = isoBanner && !isoBanner.hasAttribute('hidden');
  const configActive = !isoActive && bannerState.configInvalid !== null;
  const freshActive = !isoActive && !configActive && bannerState.freshKernel !== null;
  if (configBanner) configBanner.hidden = !configActive;
  if (freshBanner) freshBanner.hidden = !freshActive;
}

/**
 * Render the config-invalid banner content. textContent only — the error
 * message is operator-controlled (TOML field names echoed by the parser).
 * The actual show/hide is owned by resolveBanners().
 */
function renderConfigBanner() {
  const state = bannerState.configInvalid;
  if (!state) {
    configBanner.replaceChildren();
    return;
  }
  const head = document.createElement('strong');
  head.textContent = 'bootroom.toml is invalid';
  const body = document.createElement('p');
  body.className = 'err-body';
  const hasPos = state.line != null && state.col != null;
  body.textContent = hasPos
    ? `${state.error} (line ${state.line}, col ${state.col})`
    : `${state.error}`;
  configBanner.replaceChildren(head, body);
}

/**
 * Render the fresh-kernel banner content. Two variants:
 *  - success (ok=true): `<span>Kernel rebuilt —</span> [LAUNCH] [×]`
 *  - warning (ok=false): `<span>Kernel rebuilt but not ELF — ignored.[ (reason)]</span> [×]`
 * Inline button handlers are reattached fresh each render (UI-SPEC line 149
 * "rebuilt fresh each render"). The dismiss handler sets freshBanner.hidden
 * directly — it does NOT clear bannerState.freshKernel, so the next
 * KernelChanged WS frame re-shows the banner per UI-SPEC line 362.
 * The launch handler calls triggerLaunch() (shared with the header
 * LAUNCH button).
 */
function renderFreshBanner() {
  const state = bannerState.freshKernel;
  if (!state) {
    freshBanner.replaceChildren();
    return;
  }
  const dismiss = document.createElement('button');
  dismiss.id = 'banner-dismiss';
  dismiss.type = 'button';
  dismiss.setAttribute('aria-label', 'Dismiss');
  dismiss.textContent = '×'; // U+00D7 MULTIPLICATION SIGN (literal glyph per UI-SPEC)
  dismiss.addEventListener('click', () => { freshBanner.hidden = true; });

  if (state.ok === true) {
    const text = document.createElement('span');
    text.className = 'banner-text';
    text.textContent = 'Kernel rebuilt —'; // em-dash per UI-SPEC line 147
    const launch = document.createElement('button');
    launch.id = 'banner-launch';
    launch.type = 'button';
    launch.textContent = 'LAUNCH';
    launch.addEventListener('click', triggerLaunch);
    freshBanner.replaceChildren(text, launch, dismiss);
  } else {
    const text = document.createElement('span');
    text.className = 'banner-text';
    const suffix = state.reason ? ` (${state.reason})` : '';
    text.textContent = `Kernel rebuilt but not ELF — ignored.${suffix}`;
    freshBanner.replaceChildren(text, dismiss);
  }
}

/**
 * Rebuild #actions-panel from a /api/config JSON payload (or a
 * WsMessage::ConfigUpdate `config` field — same shape per Plan 06's
 * project_loaded_to_json helper).
 *
 * Grouping: actions with `group === <label>` are collected under a single
 * `.action-group` container; first-seen group label wins for ordering.
 * Actions with `group === null` accumulate into a final unlabeled group
 * appended at the END (UI-SPEC line 144). Inside each group, action order
 * matches TOML insertion order (the server already preserves this).
 *
 * Each `.action-btn` carries `data-bytes-b64` (the pre-decoded byte payload
 * the server projected from TOML escape sequences) and `data-action-label`
 * (for debugging). Button text is the TOML `label` uppercased at render
 * time per UI-SPEC line 145.
 *
 * If `funnel.locked === true` at render time, every fresh button is created
 * with `disabled` set so the visual lock state survives re-renders.
 */
function renderActionButtons(config) {
  const actions = (config && Array.isArray(config.actions)) ? config.actions : [];
  const groups = new Map(); // label -> HTMLDivElement
  const ungrouped = [];

  for (const action of actions) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'action-btn';
    btn.dataset.actionLabel = action.label || '';
    btn.dataset.bytesB64 = action.bytes_b64 || '';
    btn.textContent = (action.label || '').toUpperCase();
    if (funnel.locked === true) btn.disabled = true;

    if (action.group == null) {
      ungrouped.push(btn);
      continue;
    }

    let group = groups.get(action.group);
    if (!group) {
      group = document.createElement('div');
      group.className = 'action-group';
      const label = document.createElement('span');
      label.className = 'action-group-label';
      label.textContent = String(action.group).toUpperCase();
      group.appendChild(label);
      groups.set(action.group, group);
    }
    group.appendChild(btn);
  }

  const children = [];
  for (const groupEl of groups.values()) children.push(groupEl);
  if (ungrouped.length > 0) {
    // Ungrouped final group: no heading element per UI-SPEC line 144.
    const tail = document.createElement('div');
    tail.className = 'action-group';
    for (const btn of ungrouped) tail.appendChild(btn);
    children.push(tail);
  }
  actionsPanel.replaceChildren(...children);
  actionsPanel.hidden = actions.length === 0;
}

// Delegated click handler — installed ONCE so ConfigUpdate re-renders don't
// need to re-attach listeners. Lock-aware short-circuit BEFORE enqueue
// (T-03-11-02; mirrors the xterm.onData / xterm.onBinary guards).
actionsPanel.addEventListener('click', (e) => {
  const btn = e.target.closest('.action-btn');
  if (!btn) return;
  // funnel is declared later in this module but is in scope by the time
  // any .action-btn exists (renderActionButtons only runs after the WS
  // Hello handler triggers initialConfigLoad, by which point the rest of
  // module evaluation has completed).
  if (funnel.locked === true) return;
  const b64 = btn.dataset.bytesB64 || '';
  if (b64.length === 0) return;
  try {
    funnel.enqueue(b64ToBytes(b64), { pacingMs: 15 });
  } catch (err) {
    console.warn('[bootroom] action click failed:', err);
  }
});

/**
 * Initial `/api/config` fetch. Called from the WS Hello handler (so the
 * terminal log order is Hello-then-config-load). On 4xx/5xx, the panel
 * stays hidden and a single diagnostic line is written to the terminal —
 * the page remains usable for manual Launch/Reset/typing per UI-SPEC
 * Interaction Contract 3.
 */
async function initialConfigLoad() {
  try {
    const res = await fetch('/api/config');
    if (!res.ok) {
      actionsPanel.hidden = true;
      try { slave.write('[bootroom] config unavailable: ' + res.status + '\r\n'); } catch (_e) {}
      return;
    }
    const config = await res.json();
    renderActionButtons(config);
  } catch (e) {
    actionsPanel.hidden = true;
    try { slave.write('[bootroom] config unavailable: ' + (e?.message || e) + '\r\n'); } catch (_e) {}
  }
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
  // Phase 3 / ACT-04: silently drop user keystrokes while the funnel is
  // input-locked. The BUSY pill is the only visible signal — no terminal
  // feedback per UI-SPEC Interaction Contract 2 (amended).
  if (funnel.locked === true) return;
  const bytes = new TextEncoder().encode(data);
  if (bytes.length > 0) funnel.enqueue(bytes, { pacingMs: 0 });
});

// Paste of binary data follows the same path.
xterm.onBinary((data) => {
  if (funnel.locked === true) return;
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
      // Phase 3: kick off the initial /api/config fetch AFTER Hello so the
      // terminal log order is Hello-then-config-load. WS is established at
      // this point, so any ConfigUpdate that races our fetch will arrive
      // after this initial render (frame ordering preserved by axum's
      // broadcast forwarder per Plan 08).
      initialConfigLoad().catch((e) => console.warn('[bootroom] initial config load threw:', e));
      break;
    case 'SerialIn':
      try {
        const bytes = b64ToBytes(frame.data || '');
        // Lock-agnostic on purpose: server-initiated scenario writes MUST
        // flow during the funnel input lock (03-CONTEXT.md "Funnel input
        // lock primitive"). The lock guards the user-input side only.
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
    case 'ConfigUpdate':
      // Watcher detected a valid TOML; replace the buttons in place and
      // clear any prior config-invalid state. The 300ms debounce in the
      // watcher (Plan 06) caps re-render rate to ~3/sec — replaceChildren
      // is O(n) synchronous; xterm + DOM both handle this trivially
      // (T-03-11-03).
      try {
        renderActionButtons(frame.config || { actions: [], scenarios: [] });
        bannerState.configInvalid = null;
        renderConfigBanner();
        resolveBanners();
        const n = (frame.config && frame.config.actions) ? frame.config.actions.length : 0;
        const m = (frame.config && frame.config.scenarios) ? frame.config.scenarios.length : 0;
        try { slave.write('[bootroom] config reloaded (' + n + ' actions, ' + m + ' scenarios)\r\n'); } catch (_e) {}
      } catch (e) {
        console.warn('[bootroom] ConfigUpdate handler failed:', e);
      }
      break;
    case 'ConfigInvalid':
      // Last-known-good actions remain clickable per UI-SPEC Interaction
      // Contract 5; we touch only the banner state and the terminal log.
      try {
        bannerState.configInvalid = {
          error: typeof frame.error === 'string' ? frame.error : 'unknown',
          line: (typeof frame.line === 'number') ? frame.line : null,
          col: (typeof frame.col === 'number') ? frame.col : null,
        };
        renderConfigBanner();
        resolveBanners();
        const pos = (bannerState.configInvalid.line !== null && bannerState.configInvalid.col !== null)
          ? ' (line ' + bannerState.configInvalid.line + ', col ' + bannerState.configInvalid.col + ')'
          : '';
        try { slave.write('[bootroom] config invalid: ' + bannerState.configInvalid.error + pos + '\r\n'); } catch (_e) {}
      } catch (e) {
        console.warn('[bootroom] ConfigInvalid handler failed:', e);
      }
      break;
    case 'KernelChanged':
      // Phase 3 / WCH-05: do NOT auto-reload. The user MUST click LAUNCH
      // (header button or the inline LAUNCH inside the fresh-banner)
      // before the new kernel boots. UI-SPEC Interaction Contract 6.
      try {
        bannerState.freshKernel = {
          ok: frame.ok === true,
          reason: (typeof frame.reason === 'string') ? frame.reason : null,
        };
        renderFreshBanner();
        resolveBanners();
        if (frame.ok !== true) {
          try { slave.write('[bootroom] kernel rebuild rejected: ' + (bannerState.freshKernel.reason || 'unknown') + '\r\n'); } catch (_e) {}
        }
      } catch (e) {
        console.warn('[bootroom] KernelChanged handler failed:', e);
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
//
// Phase 3: extracted to a module-level function so the fresh-banner's
// inline LAUNCH button (rendered by renderFreshBanner above) can reuse
// the same WS-send + rAF + reload sequence. UI-SPEC Interaction Contract 7.
function triggerLaunch() {
  disableHeaderButtons();
  try {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'Launch' }));
    }
  } catch (e) { console.warn('[bootroom] Launch send failed:', e); }
  // rAF gives the browser one frame to flush the WS send before the
  // navigation tears down the document.
  requestAnimationFrame(() => window.location.reload());
}
document.getElementById('btn-launch').addEventListener('click', triggerLaunch);

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
// Funnel lock observer registration (Phase 3 / ACT-04)
// ---------------------------------------------------------------------------
//
// Wired ONCE here so all dependencies (funnel, setPill, recomputePillLocal,
// actionsPanel) are already defined. The funnel itself fires the observer
// from lockInput()/unlockInput() (Plan 10); app.js's job is to translate
// the boolean into the visible UI surface per UI-SPEC Interaction Contract 9.
//
// On lock=true:  pill flips to BUSY + every existing .action-btn disabled.
// On lock=false: every .action-btn re-enabled + recomputePillLocal restores
//                the prior local-derived state (LOADING/RUNNING/IDLE/HALTED
//                per Pattern 5) OR re-applies any active serverStateAuthority.
// Phase 3 ships the observer; Phase 4's scenario engine is the first caller.
setLockObserver((locked) => {
  if (locked) {
    setPill('BUSY');
    document.querySelectorAll('#actions-panel .action-btn').forEach((b) => { b.disabled = true; });
  } else {
    document.querySelectorAll('#actions-panel .action-btn').forEach((b) => { b.disabled = false; });
    recomputePillLocal();
  }
});

// ---------------------------------------------------------------------------
// Kick off all flows
// ---------------------------------------------------------------------------

// Wire up WS first (cheap, non-blocking); then fetch info + boot guest.
// initialConfigLoad() is NOT called here — it runs from the Hello WS
// handler so timing is "after WS is established", which prevents the race
// where the config fetch returns before the WS is ready to deliver
// ConfigUpdate frames (would result in a missed live-reload edit).
connectWs();
loadKernelInfo();
bootGuest().catch((e) => {
  console.error('boot failed:', e);
  setPill('HALTED');
  try {
    slave.write('[bootroom] Boot failed: ' + e.message + '\r\n');
  } catch (_e) { /* slave may not be ready */ }
});
