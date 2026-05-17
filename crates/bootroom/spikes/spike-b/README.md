# Spike B — Headless Chromium + SAB + qemu-wasm

Per CONTEXT.md decision: confirms whether `chromiumoxide` can drive
`--headless=new` Chromium against a bare bootroom server, observe
`crossOriginIsolated === true`, and successfully boot a fixture RISC-V
kernel with serial output flowing.

## Prerequisites

- Chromium 144+ installed at `/usr/bin/chromium` (Arch: `pacman -S chromium`).
- A RISC-V kernel image at `fixtures/Image`. See "Fixture options" below.

## Run

    cargo run -p spike-b -- --kernel fixtures/Image

Produces `SPIKE-B-RESULT.md` summarizing observations + verdict.

## Time box

1 day. If diagnosis exceeds half a day with no clear path to green,
record `amber` verdict documenting the failure shape.

## Fixture options

1. **Best:** A NORN early-boot Image, or any custom RISC-V kernel that
   prints to ttyS0 on boot. Easiest end-to-end signal.
2. **Fallback:** An empty zero-byte file. qemu-wasm will fail to boot;
   the spike still observes whether the headless page initializes,
   achieves `crossOriginIsolated === true`, and reaches Module.onAbort
   (proving the SAB path works even if the kernel doesn't). Verdict in
   this case is `amber` with note "boot path unverified — needs real
   kernel fixture".

## Security caveat

The spike launches Chromium with `--no-sandbox` so it can run as the
current user without setuid helpers. This is acceptable for a spike
running against a known-loopback server with no external content. Do
NOT carry this flag into any production code path.
