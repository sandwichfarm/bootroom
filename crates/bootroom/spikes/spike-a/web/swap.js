// Spike A swap probe — paste into DevTools console of a running bootroom
// page (http://127.0.0.1:8765/) after the guest has reached RUNNING.
//
// Mirrors the production injection pattern from crates/bootroom/web/app.js:
//   Module.FS_unlink('/pack/Image')
//   Module.FS_createDataFile('/pack', 'Image', bytes, true, true, true)
// — because Module.FS is not publicly exposed on this qemu-wasm emscripten
// build. The probe tries both APIs so a future build that exposes
// Module.FS.writeFile is also covered.

(async function spikeA() {
  // CONFIGURE: where to fetch the second kernel from. Examples:
  //   '/kernel' (no-op — same kernel; just tests the writeFile path)
  //   'http://127.0.0.1:8766/kernel' (a second bootroom on a different port)
  //   'fixtures/Image-B' (if served from --assets-dir)
  const KERNEL_URL = '/kernel';

  console.group('[spike-a]');
  console.log('Pre-swap pill state:', document.getElementById('status')?.dataset?.state);
  console.log('Module exists:', typeof Module !== 'undefined');
  if (typeof Module === 'undefined') {
    console.error('Module global not present; spike cannot proceed.');
    console.groupEnd();
    return { verdict: 'red', reason: 'Module global not present' };
  }
  console.log('Module.FS exposed (public):', !!Module.FS);
  console.log('Module.FS_unlink wrapper present:', typeof Module.FS_unlink === 'function');
  console.log('Module.FS_createDataFile wrapper present:', typeof Module.FS_createDataFile === 'function');

  // Snapshot existing /pack/Image. Try the wrapper first since that's what
  // production uses.
  let preBytes;
  try {
    if (Module.FS && typeof Module.FS.readFile === 'function') {
      preBytes = Module.FS.readFile('/pack/Image');
    } else if (typeof Module.FS_readFile === 'function') {
      preBytes = Module.FS_readFile('/pack/Image');
    }
    if (preBytes) {
      console.log('Pre /pack/Image size:', preBytes.length, 'first 8 bytes:', Array.from(preBytes.slice(0, 8)));
    } else {
      console.warn('No readable FS API found; cannot snapshot pre-swap bytes.');
    }
  } catch (e) {
    console.warn('Could not read /pack/Image:', e.message);
  }

  // Fetch the candidate replacement.
  console.log('Fetching new kernel from', KERNEL_URL);
  const res = await fetch(KERNEL_URL);
  if (!res.ok) {
    console.error('Fetch failed:', res.status);
    console.groupEnd();
    return { verdict: 'red', reason: 'kernel fetch failed' };
  }
  const newBytes = new Uint8Array(await res.arrayBuffer());
  console.log('New kernel size:', newBytes.length);

  // Attempt the swap. Prefer Module.FS.writeFile if available; fall back
  // to the FS_unlink + FS_createDataFile wrapper pair (production path).
  let writeOk = false;
  try {
    if (Module.FS && typeof Module.FS.writeFile === 'function') {
      Module.FS.writeFile('/pack/Image', newBytes);
      writeOk = true;
      console.log('Wrote via Module.FS.writeFile');
    } else {
      try { Module.FS_unlink('/pack/Image'); } catch (_e) { /* may not exist */ }
      Module.FS_createDataFile('/pack', 'Image', newBytes, true, true, true);
      writeOk = true;
      console.log('Wrote via Module.FS_unlink + Module.FS_createDataFile');
    }
    // Read back to verify.
    let postBytes;
    if (Module.FS && typeof Module.FS.readFile === 'function') {
      postBytes = Module.FS.readFile('/pack/Image');
    } else if (typeof Module.FS_readFile === 'function') {
      postBytes = Module.FS_readFile('/pack/Image');
    }
    if (postBytes) {
      console.log('Post /pack/Image size:', postBytes.length,
        'matches new bytes length:', postBytes.length === newBytes.length);
    }
  } catch (e) {
    console.error('write failed:', e.message);
    console.groupEnd();
    return { verdict: 'red', reason: 'write rejected: ' + e.message };
  }

  // The write succeeded. Now the harder question: does the running QEMU
  // pick this up? QEMU doesn't re-read /pack/Image after the initial -kernel
  // load, so we need a guest reset.
  //
  // Options to trigger reset (in order of preference):
  // 1. Use QEMU's qmp/monitor — qemu-wasm may not expose this.
  // 2. Call Module._qemu_system_reset() or similar exported symbol if available.
  // 3. Full page reload — works but defeats "no page reload" goal.
  //
  // Probe option 2:
  const resetFns = Object.keys(Module).filter(k =>
    k.toLowerCase().includes('reset') || k.toLowerCase().includes('reboot')
  );
  console.log('Reset-like exports in Module:', resetFns);

  // If nothing usable, the operator falls back to location.reload().
  console.log('writeFile path: WORKS.');
  console.log('Reset path:', resetFns.length > 0
    ? 'CANDIDATES — ' + resetFns.join(', ')
    : 'NONE — page reload required for variant swap to take effect.');
  console.log('Manual verdict input:');
  console.log('  green if you call one of the reset fns and see variant B boot in place');
  console.log('  amber if writeFile works but no in-place reset path exists (page reload required)');
  console.log('  red  if writeFile itself fails or corrupts state');
  console.groupEnd();
  return { writeOk, resetFns, preLen: preBytes?.length, newLen: newBytes.length };
})();
