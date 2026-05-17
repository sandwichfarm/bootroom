// crates/bootroom/web/app.js
//
// Phase 1 UI entrypoint. Loaded as `<script type="module">` from index.html.
//
// Depends on these globals, populated by the classic scripts that index.html
// loads BEFORE this module runs:
//   window.Terminal — xterm.js 5.3.0
//   window.openpty  — xterm-pty 0.12.0
//   window.Module   — QEMU argv (set by /assets/qemu/module.js)
//
// Browsers defer `type="module"` scripts by default, so all four classic
// <script> tags above this one have executed when this file starts.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Format a byte count using IEC binary units per 01-UI-SPEC:
 * one decimal place when the value is >= 10, two decimals below.
 */
function humanBytes(n) {
  if (n == null || isNaN(n)) return '—';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = Number(n);
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

const xterm = new Terminal();
xterm.open(document.getElementById('terminal'));

// Phase 1: input deliberately no-op; Phase 2 wires through /ws
xterm.attachCustomKeyEventHandler(() => false);

const { master, slave } = openpty();
xterm.loadAddon(master);

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
    try { Module.FS_unlink('/pack/Image'); } catch (_e) { /* not present yet */ }
    try {
      Module.FS_createDataFile('/pack', 'Image', pendingKernel, true, true, true);
    } catch (e) {
      console.error('kernel inject failed:', e);
      try { slave.write('[bootroom] Failed to inject kernel: ' + e.message + '\r\n'); } catch (_e) {}
      setPill('HALTED');
      return;
    }
    if (self.crossOriginIsolated) setPill('RUNNING');
    // Re-fit once xterm has begun streaming serial — cell dimensions are
    // most accurate now that the renderer has flushed at least one frame.
    requestAnimationFrame(fitTerminalToContainer);
  };
  Module.onExit = (_code) => setPill('HALTED');
  Module.onAbort = (_what) => setPill('HALTED');

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
// Kick off both flows
// ---------------------------------------------------------------------------

loadKernelInfo();
bootGuest().catch((e) => {
  console.error('boot failed:', e);
  setPill('HALTED');
  try {
    slave.write('[bootroom] Boot failed: ' + e.message + '\r\n');
  } catch (_e) { /* slave may not be ready */ }
});
