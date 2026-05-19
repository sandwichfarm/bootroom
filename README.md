# bootroom

A web-based test harness for RISC-V kernels (and any qemu-wasm guest).
Press one button, get the freshest kernel running in a browser with a
click-to-trigger scenario library.

> Note: the working directory on this development host is still
> `~/Develop/norn-web` from the project's earlier name; the workspace,
> binary, and crates are all named `bootroom`. The directory may be
> renamed at the user's convenience.

## Quickstart

    cargo build --release
    ./target/release/bootroom serve --kernel /path/to/Image
    # Open http://127.0.0.1:8765

## License

Dual-licensed under MIT OR Apache-2.0. See LICENSE-MIT and LICENSE-APACHE.

## Status

Pre-1.0; under active development. See `.planning/ROADMAP.md` for phase plan
and `.planning/PROJECT.md` for design goals.
