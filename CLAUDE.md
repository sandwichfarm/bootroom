<!-- GSD:project-start source:PROJECT.md -->
## Project

**bootroom**

A web-based test harness for RISC-V kernels (and any qemu-wasm guest). It serves QEMU compiled to WebAssembly with a config-driven UI of action buttons that drive scenarios against the running guest — for local debug *and* CI. First consumer is the NORN kernel; the tool itself is kernel-agnostic.

**Core Value:** **Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library.** If everything else fails, that one path must stay friction-free.

### Constraints

- **Tech stack — Rust:** single static binary, embeds static assets via `include_dir!`. No Node.js or Python runtime required to run the tool. Web UI is vanilla JS + HTML (no build step).
- **Config format — TOML:** action buttons, groups, and scenarios are defined in a TOML file (default `bootroom.toml` in CWD; overridable via `--config`).
- **Distribution — cargo + binaries:** must be installable in one step from any kernel CI: `cargo install bootroom` or `curl | tar -xz` from a GitHub Release.
- **Command surface — minimal:** the user must never need >1 long-form command to do common tasks. Subcommands are short verbs (`serve`, `run`, `init`).
- **License — MIT OR Apache-2.0:** Rust-ecosystem dual license. Maximum downstream compatibility (kernel projects of either license can pull it in).
- **Repo external-callable:** the binary, once installed, must run anywhere; no assumption that `bootroom`'s repo is checked out.
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Executive Summary
## Recommended Stack
### Core Technologies
| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| **Rust** | 1.85+ (edition 2024) | Implementation language | Project constraint; single-binary distribution matches kernel-toolchain ecosystem. notify-debouncer-full requires MSRV 1.85. |
| **axum** | 0.8.x | HTTP server (routes, SSE, static, middleware) | Tokio-native, tower-based, first-class SSE, very small, the de-facto modern Rust web framework as of 2026. v0.8 has native async traits, no `#[async_trait]` boilerplate. |
| **tokio** | 1.x | Async runtime | Required by axum; broadcast channels are the right primitive for fan-out of file-watcher events to multiple SSE subscribers. |
| **tower-http** | 0.6.x | Middleware: COOP/COEP headers, tracing, compression | `SetResponseHeaderLayer` is the idiomatic way to attach the mandatory cross-origin-isolation headers to every response. |
| **include_dir** | 0.7.x | Compile-time embed of `web/` directory | One macro, zero deps, lets you walk the embedded FS at runtime. Preferred over `rust-embed` for this project (see "Alternatives Considered"). |
| **clap** (derive) | 4.5.x | CLI parsing + subcommands (`serve`, `run`, `init`) | Standard. Derive macro keeps subcommand definitions colocated with their Args structs. `flatten` lets `serve` and `run` share `--kernel`/`--config` flags cleanly. |
| **serde** | 1.0.228 | TOML/JSON serialization model | Required by both `toml` and `serde_json`. Universal. |
| **toml** | 0.8.x | Parse `bootroom.toml` config | Native Rust TOML 1.0 parser, serde-integrated. |
| **serde_json** | 1.0.x | Serialize config to the browser | One-line `Json(config)` response with axum. |
| **notify** + **notify-debouncer-full** | notify 8.x, debouncer-full 0.5.x | Watch the kernel artifact for `make`-driven rebuilds | Raw `notify` fires 3–10 events per file save on most filesystems (write, attrib, write, close…). The debouncer collapses them to a single "kernel changed" signal. `notify-debouncer-full` is preferred over `-mini` because it preserves event ordering and handles rename/move correctly. |
| **anyhow** | 1.x | Application-level error handling | Standard for CLIs where you mostly want `?`-and-bail with context. Don't use `thiserror` here — bootroom is an application, not a library; no need to expose stable error types. |
| **tracing** + **tracing-subscriber** | 0.1.x / 0.3.x | Structured logging with levels and `RUST_LOG` filtering | Tower/axum integrate natively. `--verbose`/`-v` flag in clap maps to a `Targets` filter. |
### Browser-Side Technologies
| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| **xterm.js** | 5.3.0 | Terminal widget in the browser | qemu-wasm's own demo uses it. No reason to deviate. |
| **xterm-pty** | latest (npm `xterm-pty`) | PTY layer between xterm.js and emscripten's stdin/stdout | **This is the load-bearing piece.** qemu-wasm is compiled with `--js-library=…/xterm-pty/emscripten-pty.js`, so the PTY is already wired into QEMU's chardev. The `slave.write(bytesOrString)` API is the action-button injection point. `slave.onReadable` + `slave.read()` lets bootroom mirror serial output back to the server for CI assertions. |
| **Vanilla JS + HTML** | ES2022+ | UI rendering, action button bindings, fetch/SSE clients | Project constraint: "no npm in the toolchain." All modern browsers (the only ones that can run qemu-wasm at all because of SAB + pthread requirements) support ES modules, `fetch`, `EventSource`, `WebSocket`, async/await, `<dialog>`, etc. natively. No bundler needed. Ship `xterm.js` and `xterm-pty` as pinned vendored files in `web/vendor/` so the binary works offline. |
### Development & Release Tools
| Tool | Purpose | Notes |
|------|---------|-------|
| **cargo-dist** | Build & publish multi-platform release binaries from GitHub Actions | v0.31+ supports `aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `aarch64-unknown-linux-musl` natively (uses `cargo-zigbuild` under the hood for Linux cross-compile). Generates a `release.yml` workflow you commit. |
| **cargo-binstall** | End-user fast install path | Zero work for the project — binstall auto-finds the GitHub Release artifacts cargo-dist publishes via the `Cargo.toml` `[package.repository]` link. Users who already have it skip rebuilds. |
| **`cargo install bootroom`** | Source-build install path | Default cargo behavior; works on any platform with rustc. Slower but covers any architecture cargo-dist doesn't ship. |
| **chromiumoxide** | Headless-CI driver | Async, tokio-native, pure-Rust CDP client. Lets `bootroom run` launch headless Chromium, navigate to its own `localhost`, and assert on serial output. Single dependency chain (no Node.js). |
| **rustfmt + clippy** | Lint/format | Standard. CI fails on `clippy -- -D warnings`. |
| **cargo-deny** | Supply-chain & license audit | Project constraint is "MIT OR Apache-2.0"; cargo-deny enforces it in CI. |
## qemu-wasm Integration — The Concrete Path
### What qemu-wasm ships (per `qemu-wasm/examples/riscv64/src/htdocs/`)
### Mandatory browser environment
| Requirement | Why | How bootroom provides it |
|---|---|---|
| `Cross-Origin-Opener-Policy: same-origin` | Cross-origin isolation gate for SAB | `SetResponseHeaderLayer::overriding` on every response |
| `Cross-Origin-Embedder-Policy: require-corp` | Same | Same |
| `SharedArrayBuffer` enabled | qemu-wasm uses pthreads + atomics for MTTCG | Automatic once COOP/COEP set |
| HTTPS *or* `localhost` | Browser SAB gate (localhost is exempt) | bootroom binds `127.0.0.1` by default |
| MIME `application/wasm` for `.wasm` | Required for `WebAssembly.instantiateStreaming` | `tower-http`'s `ServeDir` handles this, but with `include_dir` we set it explicitly via a small `mime_guess`-backed handler |
| Cross-origin isolation actually delivered | A single missing header breaks SAB | Health check on startup: `bootroom serve` logs the headers and exits with a hint if `--port` collides |
### The kernel-reload flow
### The action-button injection path
### Capturing serial output for assertions
### Headless CI mode
- `WebAssembly.Module` / `WebAssembly.Instance` at runtime to register dynamically-JIT'd TCG translation blocks as new Wasm modules. WASI runtimes don't expose this — they instantiate one module up front.
- Emscripten's `Module.FS`, `Module.TTY`, pthread workers — all browser/JS-host APIs.
- `SharedArrayBuffer` + atomics for MTTCG.
## Cargo.toml — Concrete Starting Point
# Async runtime + HTTP
# CLI / config / errors / logs
# Embedded assets
# File watching
# Headless-CI browser driver (only needed by the `run` subcommand)
## Project Layout
## Alternatives Considered
| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| **axum** | **actix-web** | Larger ecosystem, but actix's actor model is overkill for a single-purpose dev tool and its async traits are still less ergonomic than axum 0.8's. Stick with axum unless you have an existing actix codebase. |
| **axum** | **rocket** | Rocket's macro magic obscures behavior; axum's tower-stack composition matches the "small, transparent dev tool" goal. |
| **axum** | **poem**, **warp** | Fine but lower mindshare; harder for contributors. |
| **include_dir** | **rust-embed** | rust-embed offers per-file compression via `include-flate`. Useful if the binary balloons (qemu-wasm output is ~10–30MB). Switch if `cargo bloat` shows assets dominating. include_dir is simpler API-wise (`Dir.get_file()`, iteration) and has no proc-macro overhead beyond the embed itself. |
| **include_dir** | runtime `std::fs::read` from a sibling `share/` dir | Required for fast iteration of `web/` during dev. **Use a `#[cfg]` switch:** debug builds read from disk for hot-reload of the UI; release builds use `include_dir!`. Pattern is well-trodden (rust-embed has `debug-embed` for exactly this). |
| **chromiumoxide** | **fantoccini** (WebDriver) | Use fantoccini if you need to test Firefox/WebKit too. For Chromium-only (which is all qemu-wasm strictly needs — Chrome and Firefox both run it, but headless WebDriver setup for Firefox is fiddlier), chromiumoxide stays inside Rust. |
| **chromiumoxide** | **Playwright** (Node subprocess) | If chromiumoxide's headless SAB story turns out broken, Playwright is the fallback. It also gives you Firefox + WebKit testing if that becomes a goal. Cost: adds a Node.js dep to CI environments. |
| **chromiumoxide** | `headless_chrome` crate | Synchronous, less actively maintained than chromiumoxide as of 2026. |
| **SSE for server-push** | **WebSocket for everything** | WebSocket if you ever need true full-duplex (we don't for config/kernel-changed events). SSE is one `EventSource` in vanilla JS, no message framing, auto-reconnect built in. |
| **notify-debouncer-full** | **notify-debouncer-mini** | Mini is fine for "just tell me something changed." Full preserves event ordering and reports rename pairs as renames (mini reports them as remove+create). For kernel-artifact watching this distinction rarely matters, but full is the official recommendation. |
| **clap derive** | **clap builder API** | Derive is clearer for static CLIs. Use builder only if you need runtime-generated subcommands (you don't). |
| **clap** | **bpaf**, **argh**, **pico-args** | Niche. clap is the unambiguous default and what users expect (`--help` formatting alone is worth it). |
| **anyhow** | **eyre + color-eyre** | color-eyre gives prettier error output. Worth adding in Phase 1 polish; not required initially. |
| **cargo-dist** | hand-rolled GitHub Actions matrix | cargo-dist generates the matrix for you, handles signing, checksums, installers (shell + Powershell), and Homebrew tap formula. Hand-rolling is several days of work and ongoing maintenance. |
| **TOML config** | **YAML**, **JSON**, **KDL** | Project constraint. TOML's comment support is genuinely useful for `[[action]]` blocks the user edits by hand. |
| **vanilla JS** | **HTMX**, **Alpine.js**, **htmz** | Tempting for the "no build step" property they share, but vanilla covers the requirement set (dynamic action grid, SSE event handling, xterm widget) without adding a learning surface. Re-evaluate if the UI grows past ~500 LOC. |
## What NOT to Use
| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **npm / webpack / vite / any JS bundler** | Project constraint, and unnecessary — vanilla ES modules in `<script type="module">` cover everything. | Vendored `xterm.js` + `xterm-pty.js` as plain files in `web/vendor/`. |
| **`ratatui` / TUI mode** | Bootroom's UI lives in the browser, not the terminal. A TUI for the CLI's own output is pure scope creep. | Plain `tracing` logs. |
| **`hyper` directly** | Hand-rolling routes and middleware on raw hyper is a regression from axum. | axum 0.8. |
| **`actix-web`'s actor system** | Overkill; couples your code to an actor runtime for no domain reason. | axum + tower + tokio channels. |
| **WASI runtimes (wasmtime, wasmer, Node.js standalone)** for running qemu-wasm headlessly | qemu-wasm requires `WebAssembly.Module` runtime instantiation for TCG JIT and emscripten's `Module.FS`/`TTY`/pthreads — none available in WASI. | Headless Chromium via chromiumoxide. |
| **`cargo install` as the *only* install path** | Slow (cold compile is several minutes), requires rustc. Bad for kernel CI runners. | `cargo-dist` prebuilt binaries; `cargo install` as a secondary path. |
| **Raw `notify` without a debouncer** | One `make` produces 3–10 fs events. Without debouncing the "kernel changed" event fires repeatedly and the UI flickers. | `notify-debouncer-full`. |
| **`thiserror` everywhere** | bootroom is an application binary, not a library exposing stable error types. `thiserror` adds boilerplate for no consumer benefit. | `anyhow` with `.context(…)`. |
| **CDN-hosted `xterm.js`** (as in qemu-wasm's demo) | CI runners may be offline / firewalled; the binary should be self-contained. | Vendor `xterm.js` and `xterm-pty` into `web/vendor/`. |
| **`include_dir!` in `build.rs` driving the qemu-wasm docker build** | Builds qemu-wasm on every cargo invocation; multi-minute hit. | Separate `make qemu-assets` target that runs the docker build manually; `cargo build` only re-embeds the cached output. |
| **WebSocket for the config push / kernel-changed notifications** | Adds message-framing logic and reconnection handling for one-way data. | SSE (`EventSource`). |
| **GitHub Actions `actions/cache` for the qemu-wasm docker layer** *(maybe — flagged for spike)* | Probably fine, but verify cache hit rates before committing to the design. | Or: pre-build qemu-wasm artifacts into a separate release-only artifact bucket. |
## Stack Patterns by Variant
- Drop chromiumoxide; vendor or invoke Playwright via subprocess in `bootroom run`.
- Adds Node.js dependency on CI runners (acceptable cost — Playwright's headless SAB support is the most-tested in the industry).
- Swap `include_dir` for `rust-embed` with `compression` feature (gzips each file).
- Or: don't embed qemu-wasm assets at all; download them from a release URL on first `bootroom serve` and cache in `$XDG_CACHE_HOME/bootroom/`.
- Replace chromiumoxide with **fantoccini** (WebDriver), drives geckodriver + Firefox in addition to Chrome.
- Skip `file_packager.py` re-run. Use emscripten's `Module.FS.writeFile('/pack/Image', newBytes)` inside the browser after `fetch('/api/kernel')`, then trigger a guest reset rather than a full Worker tear-down. Requires a guest-side helper (qemu monitor command, or just `qemu_system_reset` exposed to JS) — flag as Phase 2.
- The qemu-wasm submodule already builds x86_64 and aarch64; selecting the qemu binary becomes a config field. The Rust side stays unchanged.
## Version Compatibility
| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| axum 0.8 | tokio 1.x, tower-http 0.6, tower 0.5 | The 0.7 → 0.8 jump broke handler signatures; ensure all axum examples you copy are from 0.8 docs. |
| notify-debouncer-full 0.5 | notify 8.x | Pinned together by the debouncer crate; let cargo resolve. |
| chromiumoxide 0.7 | tokio 1.x (with `tokio-runtime` feature) | Don't enable both `async-std-runtime` and `tokio-runtime`. |
| include_dir 0.7 | Rust 1.74+ | No MSRV worry — we require 1.85. |
| clap 4.5 | edition 2021 or 2024 | derive macros work cleanly in 2024. |
| xterm.js 5.3.0 | xterm-pty (latest) | The qemu-wasm demo pins exactly 5.3.0; deviating risks regressions in the PTY addon. |
| qemu-wasm (this submodule) | Emscripten (per its Dockerfile) | Submodule is self-contained; pin its commit in `.gitmodules`. |
## Sources
### Primary (HIGH confidence)
- `qemu-wasm/README.md` (in-tree) — confirms build flags, output files, COOP/COEP requirement via `xterm-pty.conf`.
- `qemu-wasm/examples/riscv64/src/htdocs/index.html` (in-tree) — confirms xterm + xterm-pty integration pattern and `slave`/`master` API usage.
- [xterm-pty README](https://github.com/mame/xterm-pty) — confirms `slave.write()` API for programmatic stdin injection and PROXY_TO_PTHREAD support.
- [axum releases](https://github.com/tokio-rs/axum/releases) and [crates.io/crates/axum](https://crates.io/crates/axum) — axum 0.8.x current.
- [crates.io/crates/notify](https://crates.io/crates/notify), [notify-debouncer-full docs](https://docs.rs/notify-debouncer-full) — MSRV 1.85, recommended debouncer.
- [cargo-dist releases](https://github.com/axodotdev/cargo-dist/releases) — v0.31 (2026-02-23), aarch64 support, simple-hosting.
- [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) — auto-discovers cargo-dist artifacts via `Cargo.toml` `[package.repository]`.
### Secondary (MEDIUM confidence — verified across multiple sources)
- [chromiumoxide docs.rs](https://docs.rs/chromiumoxide) and [chromiumoxide README](https://github.com/mattsse/chromiumoxide) — async tokio-native CDP client.
- [Playwright vs Puppeteer 2026 comparisons](https://www.firecrawl.dev/blog/playwright-vs-puppeteer) — Playwright is the broader-ecosystem fallback if chromiumoxide's SAB story has gaps.
- [container2wasm](https://github.com/container2wasm/container2wasm) and [QEMU FOSDEM 2025 slides](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6290-running-qemu-inside-browser/) — confirm qemu-wasm relies on browser `WebAssembly.Module` runtime APIs that WASI runtimes don't expose (rules out wasmtime as a headless host).
### Speculative / Spike Required (LOW confidence — Phase 1 must validate)
- Skipping `file_packager.py` at reload time by writing the kernel directly into `Module.FS` from JS. Plausible from emscripten docs but not demonstrated in qemu-wasm examples.
- Chromium `--headless=new` + SharedArrayBuffer reliability across CI runner images. Search results note general SAB support but no targeted confirmation for qemu-wasm headless execution. **This is the single biggest unknown; a 1-day spike in Phase 1 should de-risk it before the roadmap commits to chromiumoxide.**
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
