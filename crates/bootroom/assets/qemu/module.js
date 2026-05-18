// crates/bootroom/assets/qemu/module.js
//
// QEMU argv for the bootroom Phase 1 walking skeleton.
//
// Derived from qemu-wasm/examples/riscv64/src/htdocs/module.js. The qemu-wasm
// data pack always extracts /pack/Image, /pack/opensbi-..., and
// /pack/rootfs.bin; the user's kernel overwrites /pack/Image at runtime
// (see app.js onRuntimeInitialized + Spike A's module-fs-write path).
//
// The `-drive` line is included so the qemu-wasm reference Linux+busybox
// image (the kernel embedded inside the data pack) can boot to a shell
// during input-path smoke tests. Bare kernels that do not have a virtio_blk
// driver and do not honor a `root=` cmdline (e.g. early NORN) ignore both
// the drive and the root= argument harmlessly.
//
// Phase 3 will turn this into a TOML-driven config; until then, edit by
// hand for kernel-specific tweaks. `make qemu-assets` does NOT overwrite
// this file.
Module['arguments'] = [
    '-nographic', '-m', '512M', '-accel', 'tcg,tb-size=500',
    '-machine', 'virt',
    '-L', '/pack/',
    '-nic', 'none',
    '-kernel', '/pack/Image',
    '-drive', 'if=virtio,format=raw,file=/pack/rootfs.bin',
    '-append', 'earlyprintk=ttyS0 console=ttyS0 root=/dev/vda ro quiet loglevel=7',
];
