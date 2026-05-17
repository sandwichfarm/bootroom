// crates/bootroom/assets/qemu/module.js
//
// QEMU argv for the bootroom Phase 1 walking skeleton.
//
// Derived from qemu-wasm/examples/riscv64/src/htdocs/module.js with the
// `-drive` flag removed (Phase 1 boots a bare kernel; no rootfs).
//
// The kernel image is injected at runtime into /pack/Image by the browser
// (Spike A may later replace this with a Module.FS.writeFile-based swap).
//
// IMPORTANT: this file is bootroom-authored, NOT produced by the qemu-wasm
// docker build. `make qemu-assets` does NOT overwrite it. Edit by hand if
// you need to change the QEMU argv.
Module['arguments'] = [
    '-nographic', '-m', '512M', '-accel', 'tcg,tb-size=500',
    '-machine', 'virt',
    '-L', '/pack/',
    '-nic', 'none',
    '-kernel', '/pack/Image',
    '-append', 'earlyprintk=ttyS0 console=ttyS0 ro quiet loglevel=7',
];
