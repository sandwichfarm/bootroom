# Spike A — Runtime kernel substitution via Module.FS

Per CONTEXT.md decision: confirm whether the browser can swap the kernel
bytes in qemu-wasm's `Module.FS` (e.g. `Module.FS.writeFile('/pack/Image', bytes)`)
at runtime and trigger a guest reboot WITHOUT re-running emscripten's
`file_packager.py`.

Spike A is run AFTER Spike B (plan 01-08) per CONTEXT.md sequencing.

## Status: substitution mechanism PROVEN in production code

The underlying substitution mechanism is **already proven by production**.
See `crates/bootroom/web/app.js` (`Module.onRuntimeInitialized`): on every
page load we call

    Module.FS_unlink('/pack/Image');
    Module.FS_createDataFile('/pack', 'Image', pendingKernel, true, true, true);

after the emscripten data pack has extracted and before `callMain` runs.
This was the fix from plan 01-07 (commit `04a31fa`) when the original
`Module.preRun` write collided on `EEXIST`. The user's actual NORN kernel
boots reliably through this path — confirmed by the headed-browser smoke
test (01-07) and the chromiumoxide headless run (01-08).

**Translation:** for Phase 2's Launch button, the "fetch new kernel bytes,
write into `/pack/Image`, reload the page" path is a known-working
sequence. The verdict file records this with `chosen_path: page-reload-only`.

The Phase 2 *open question* — and a candidate Phase 2 spike — is whether
the swap can be done WITHOUT a full page reload (reuse the existing
qemu-wasm Worker, swap kernel bytes, trigger an in-place CPU reset). That
question is recorded as a follow-up in SPIKE-A-RESULT.md and does NOT
gate Phase 2's Launch button (a full reload is the conservative default).

## Prerequisites (only relevant if re-running interactive investigation)

- bootroom built (`cargo build --release`).
- Two distinguishable RISC-V kernel images under `fixtures/`:
  - `fixtures/Image-A` — first variant (any kernel that boots and prints
    recognizable serial output, e.g., a Linux init banner).
  - `fixtures/Image-B` — second variant. Easiest options:
    - A different kernel build with a recognizably different banner.
    - The same kernel but with a deliberately corrupted first byte; this
      will produce a different early error rather than a successful boot,
      which is still an observable signal that the swap took effect.
    - A trivial bare-metal "hello world" kernel that prints a unique string.

If only one variant is available, the spike degrades to "in-place reload" —
we verify the writeFile path doesn't error and that the page reload picks
up the new bytes; the cross-variant boot test is skipped. Verdict downgraded
to `amber`.

## Procedure (for re-running)

1. Start bootroom with the FIRST kernel:

       ./target/release/bootroom serve --kernel crates/bootroom/spikes/spike-a/fixtures/Image-A --port 8765

2. Open Chromium at `http://127.0.0.1:8765/` (the normal Phase 1 UI).
   Wait for the status pill to reach RUNNING and observe the unique serial
   output of variant A.

3. Open DevTools console. Paste the contents of `swap.js` and replace
   `KERNEL_URL` with `'http://127.0.0.1:8766/kernel'` (see "Two-kernel
   serving" below). Run it.

4. The script writes the new bytes into `Module.FS.writeFile('/pack/Image', ...)`
   (or `Module.FS_createDataFile` if `Module.FS` is not publicly exposed)
   and enumerates reset-like Module exports.

5. Observe the terminal — if a reset export was found and invoked, does
   variant B's serial output appear? Otherwise, `location.reload()` and
   confirm variant B boots after reload.

6. Record observations in SPIKE-A-RESULT.md per the format below.

## Two-kernel serving

The simplest approach for the spike: start TWO bootroom instances on
different ports.

    ./target/release/bootroom serve --kernel fixtures/Image-A --port 8765 &
    ./target/release/bootroom serve --kernel fixtures/Image-B --port 8766 &

Variant A: open `http://127.0.0.1:8765/`.
Variant B kernel bytes available via `http://127.0.0.1:8766/kernel` —
swap.js fetches from that URL, writes into the page-8765 Module.FS, and
triggers reset (or reload).

Alternative: drop `fixtures/Image-B` next to `fixtures/Image-A` and serve
it via a dirt-simple Python HTTP server on a third port. swap.js then
fetches from there.

## Time box

Half day. If no clear path to in-place swap emerges within 3 hours of
investigation, record `amber` with chosen_path `page-reload-only`
(Phase 2 Launch button = full page reload with new --kernel; user
experience is slightly worse but architecturally simpler).
