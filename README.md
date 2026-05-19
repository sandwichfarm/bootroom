# bootroom

A web-based test harness for RISC-V kernels (and any qemu-wasm guest).
Press one button, get the freshest kernel running in a browser with a
click-to-trigger scenario library.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![crates.io](https://img.shields.io/crates/v/bootroom.svg)](https://crates.io/crates/bootroom)
[![docs.rs](https://docs.rs/bootroom/badge.svg)](https://docs.rs/bootroom)
[![release](https://github.com/sandwich-farm/bootroom/actions/workflows/release.yml/badge.svg)](https://github.com/sandwich-farm/bootroom/actions/workflows/release.yml)

> Note: the working directory on this development host is still
> `~/Develop/norn-web` from the project's earlier name; the workspace,
> binary, and crates are all named `bootroom`. The directory may be
> renamed at the user's convenience.

First consumer: the [NORN RISC-V kernel](https://github.com/sandwich-farm/nostros).
NORN's CI and local debug flows pull `bootroom` as an external dependency.

## Install

bootroom ships three install paths. Pick the one that matches your
environment.

> **Pre-1.0 status:** the install matrix below becomes active after the
> first published release. Until bootroom is on crates.io and a tagged
> GitHub Release exists, use `cargo install --locked --git
> https://github.com/sandwich-farm/bootroom bootroom` or build from a
> clone (`make install`).

| Path | Command | When to use |
|---|---|---|
| `cargo install` | `cargo install --locked bootroom` | Primary path for Rust-equipped developers. |
| `cargo binstall` | `cargo binstall bootroom` | Secondary path when you already have `cargo-binstall`; fetches a prebuilt release binary. |
| Shell installer | `curl --proto =https --tlsv1.2 -sSf https://github.com/sandwich-farm/bootroom/releases/latest/download/bootroom-installer.sh \| sh` | Tertiary path for kernel CI runners with no Rust toolchain. |

Supported platforms: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`. Windows and Linux GNU
(non-musl) targets are not currently provided.

## Quickstart

After install, run the preflight check:

    bootroom doctor

If `doctor` reports `overall: pass`, point bootroom at a kernel ELF:

    bootroom serve --kernel /path/to/Image

bootroom opens your default browser to the harness URL. Press the
on-screen Launch button to boot the kernel; use the action-button panel
(populated from `bootroom.toml`) to drive scenarios against the running
guest. For headless CI, swap `serve` for `run --scenario <name>` once
you have a config file.

For more, see `bootroom --help` or the `.planning/` directory in this
repo.

## Source builds and contribution

If you are working on bootroom itself rather than consuming it:

    git clone https://github.com/sandwich-farm/bootroom
    cd bootroom
    make qemu-assets   # one-time; rebuilds the embedded qemu-wasm artifacts (requires docker; 10-30 min)
    make install       # runs `cargo install --locked --path crates/bootroom`

`make help` lists the full Makefile target set.

## License

Dual-licensed under MIT OR Apache-2.0. See LICENSE-MIT and LICENSE-APACHE.

## Status

Pre-1.0; under active development. See `.planning/ROADMAP.md` for phase plan
and `.planning/PROJECT.md` for design goals.
