# Project Research Summary

**Project:** bootroom
**Domain:** Rust CLI + embedded HTTP server + browser-served qemu-wasm test harness for RISC-V kernels (kernel-agnostic, first consumer: NORN)
**Researched:** 2026-05-17
**Confidence:** MEDIUM-HIGH

## Executive Summary

bootroom is a single static Rust binary that distributes a kernel-agnostic, web-delivered test harness for RISC-V kernels. The architecture is well-trodden in the Rust ecosystem (axum + tower-http + tokio + clap + include_dir for the binary; vanilla JS + xterm.js + xterm-pty in the browser) but the **integration target is novel**: qemu-wasm has very few prior consumers, and bootroom carves a new niche by unifying *interactive UX* and *CI scenarios* under a single TOML config. The signature value — "press one button, get the freshest kernel running with a click-to-trigger scenario library" — falls naturally out of (a) embedding qemu-wasm's emscripten output, (b) serving it with mandatory `COOP: same-origin` / `COEP: require-corp` headers so `SharedArrayBuffer` works, and (c) piping action-button bytes into the xterm-pty `slave.write()` PTY layer that qemu-wasm uses as its `ttyS0`.

The headline architectural decision is **one process, one WebSocket, two modes**: `serve` (real user tab) and `run` (headless Chromium via chromiumoxide) share the *same static assets and the same client code path*, so "works locally → works in CI" is a property of the design, not a hope. Scenarios execute client-side (low latency, no per-byte WS round-trip); the server is an exit-code translator. qemu-wasm **cannot** run in a standalone WASI runtime — TCG JIT requires `WebAssembly.Module` runtime instantiation plus emscripten's `Module.FS`/pthreads — so a real (headless) browser is non-negotiable for CI.

Top risks are concentrated in three places: (1) **COOP/COEP plumbing** must be perfect on every response or qemu-wasm silently fails to boot; (2) **the headless Chromium + SharedArrayBuffer + qemu-wasm path is the single biggest unknown** and must be de-risked by a Phase-1 spike before the roadmap commits to chromiumoxide (Playwright is the fallback); (3) **serial-output assertions** are notoriously flaky without line-buffering, ANSI stripping, per-action buffer reset, and explicit timeouts — bake the conventions in before users author scenarios. Secondary risks are partial-write kernel boots from the file watcher (debounce + ELF magic check), TOML schema drift (use `deny_unknown_fields` and `schema_version` from day one), and the `cargo install` packaging trap where embedded `web/` assets get silently excluded.

## Key Findings

### Recommended Stack

A standard tokio-async Rust web stack, with deliberate choices around embedding (no Node runtime) and CI driving (headless Chromium in-process). Full details in `STACK.md`.

**Core technologies:**
- **Rust 1.85+ / edition 2024** — project constraint; single static binary distribution.
- **axum 0.8 + tower-http 0.6 + tokio 1.x** — HTTP, SSE, WebSocket, COOP/COEP middleware. Idiomatic, small.
- **include_dir 0.7** — compile-time embed of `web/` UI and built qemu-wasm artifacts (with a `--assets-dir` dev override to avoid the "rebuild for every CSS tweak" trap).
- **clap 4.5 (derive)** — `serve` / `run` / `init` subcommands, shared `--kernel`/`--config` flags via `#[flatten]`.
- **serde + toml 0.8 + serde_json** — TOML on disk → JSON on the wire.
- **notify 8 + notify-debouncer-full 0.5** — kernel artifact watch with 300–500ms debounce.
- **chromiumoxide 0.7** — pure-Rust CDP driver for `bootroom run` headless mode. (Playwright fallback if SAB story breaks.)
- **xterm.js 5.3.0 + xterm-pty** (vendored, not CDN) — qemu-wasm's PTY surface is the load-bearing injection point. `slave.write(bytes)` is *the* action mechanism.
- **vanilla JS + HTML, ES modules, no bundler** — project constraint; covers all needs at <500 LOC.
- **cargo-dist + cargo-binstall** — prebuilt multi-platform release tarballs from CI; users get fast installs for free.

### Expected Features

PROJECT.md already commits to the major surfaces (TOML actions, freshest-build watch, headless `run` with serial assertions). The research adds *table-stakes adjacencies* that comparable tools (bootimage, OSDK, Twister, v86) all provide. Full details in `FEATURES.md`.

**Must have (table stakes):**
- xterm.js-rendered live serial console (read + write + free-form input + clear/copy)
- Single Launch button with freshest-build pickup + separate Reset
- Action buttons grouped per TOML, click → serial-write
- Live reload on `bootroom.toml` change
- Headless `bootroom run --scenario` with substring/regex assertions, per-action and per-scenario timeouts, exit 0/1
- `bootroom init` scaffold + sensible defaults (works with no config)
- Kernel info line (path, size, mtime), status pill (Idle/Loading/Running/Halted)
- `--verbose`, `--log-file` for CI debugging
- `cargo install bootroom` + prebuilt Linux/macOS binaries

**Should have (competitive differentiators — bootroom's defensible niche):**
- One TOML defines *both* interactive UI and CI scenarios (no comparable tool does this)
- Scenarios composed of named action refs (not duplicated byte strings)
- CLI `--action` flag to append/override actions without editing TOML (already in PROJECT.md)
- JUnit XML / GitHub Actions annotation output for CI integration
- `bootroom run --watch` for local TDD loops
- Per-action keyboard shortcuts
- `bootroom doctor` preflight diagnostics

**Defer (v2+):**
- Record-and-replay (button-click → TOML emit)
- Static "frozen reproducer" export
- Snapshot/save-state actions (blocked on qemu-wasm support)
- Multi-arch (x86_64, AArch64) — schema should allow per-action arch tagging
- Headless without browser via WASI — currently infeasible; major research project

**Explicitly out (per PROJECT.md, reaffirmed by research):**
- GDB / step-through debugging
- Plugin system
- Multi-user / auth
- Hot-swap kernel mid-run

### Architecture Approach

A single Rust binary with two execution modes (`serve` and `run`) backed by a shared library crate. In both modes the binary is the only process: it owns TOML config, kernel-file watcher, embedded static assets, the HTTP server with COOP/COEP, and the WebSocket session. The browser tab — real or headless Chromium — loads `qemu-system-riscv64.wasm` and a vanilla-JS shell that talks to the Rust process over one WebSocket. The **same files, same protocol, same client code path** in both modes is the load-bearing decision that eliminates "works in dev, breaks in CI." Full details in `ARCHITECTURE.md`.

**Major components:**
1. **`bootroom-core` (library)** — pure types + scenario engine: Action/Group/Scenario structs, WS message enum (serde tagged), assertion evaluator. No I/O, no tokio.
2. **`bootroom` binary** — clap dispatch (`serve`/`run`/`init`), axum app with COOP/COEP layer, embedded UI + qemu-wasm artifacts via `include_dir!`, WS session handler, notify-debouncer-full watcher with broadcast-channel fan-out, chromiumoxide-driven headless mode for `run`.
3. **Browser UI (vanilla JS, ES modules, embedded in binary)** — button panel rendered from `/api/config`, xterm.js terminal mounted on the xterm-pty `slave` (which *is* QEMU's `ttyS0`), client-side scenario engine, WebSocket to server for serial tap + scenario results.
4. **xterm-pty bridge** — the single byte boundary between action buttons / typing and the guest kernel's serial. Action injection = `slave.write(bytes)`. Serial capture = `slave.onReadable` + `slave.read()`. This is the *only* injection point qemu-wasm exposes.
5. **Kernel watcher → broadcast → SSE/WS** — `notify-debouncer-full` on the kernel artifact, debounce 300ms, ELF-magic + size-stability check, broadcast `KernelReloaded`; browser shows "fresher build available — click Launch."

### Critical Pitfalls

Top hazards from `PITFALLS.md`. The first three are show-stoppers if missed.

1. **Missing COOP/COEP headers** — qemu-wasm silently won't boot (SharedArrayBuffer is `undefined`); errors look like wasm bugs, not header bugs. *Prevention:* `tower-http::SetResponseHeaderLayer` applies both headers to every response from the start; boot-time smoke check; in-page `crossOriginIsolated` probe that renders an inline error banner with the fix.
2. **Headless Chromium + SharedArrayBuffer + qemu-wasm reliability** — the single biggest unknown. `--headless=new` is the right flag but the combination hasn't been documented end-to-end. *Prevention:* 1-day Phase-1 spike before committing to chromiumoxide; Playwright (Node subprocess) is the fallback.
3. **Serial-output regex assertions that look right but flake** — partial-line matches, CR/LF ambiguity, stale buffer matching, ANSI escape sequences. *Prevention:* line-buffer with `\r?\n` separator, strip ANSI before matching, per-action buffer reset by default, anchored regex with explicit timeouts, ~10–20 ms inter-character throttle on injection.
4. **File watcher fires mid-write → Launch boots corrupt kernel** — `make` produces 3–10 fs events; partial reads boot half-flashed images that waste 30 minutes of debugging. *Prevention:* `notify-debouncer-full` (not -mini, not raw `notify`), watch parent dir for the kernel filename to handle atomic-rename builds, size-stability check across two debounce ticks, ELF magic-byte sniff, surface as a hint not an auto-trigger.
5. **Embedded-assets workflow that's impossible to iterate on** — `include_dir!` alone means every CSS tweak requires `cargo build`. *Prevention:* `--assets-dir <path>` runtime override (or rust-embed's `debug-embed` pattern) so dev mode reads from disk; release builds embed.
6. **TOML schema drift — typos silently accepted** — `expects` vs `expect` accepted because `deny_unknown_fields = false` is the serde default. *Prevention:* `#[serde(deny_unknown_fields)]` on all config structs from day one; `schema_version` field at the top of `bootroom.toml`; validate all action/scenario references at load time, not click time; `bootroom check` subcommand.
7. **`cargo install` succeeds, then crashes at runtime — missing assets** — `web/` directory in `.gitignore` doesn't ship with `cargo publish`. *Prevention:* explicit `package.include` in `Cargo.toml`; release CI does `cargo install --path .` to a clean container and smoke-tests; build with musl targets for Linux to avoid glibc version pinning.
8. **Concurrent serial writes (UI button click during scenario)** garble guest stdin. *Prevention:* funnel all writes through a single mpsc/Mutex; disable manual input while a scenario runs; per-action atomic byte-sequence sends.

## Implications for Roadmap

Research yields a clean 6-phase build order. The order follows one principle from `ARCHITECTURE.md`: **the unblocking critical path is "can a real browser tab boot the kernel and receive a byte we sent."** Everything else is decoration on top of that path.

### Phase 1: Walking Skeleton — Serve qemu-wasm + See Boot

**Rationale:** Validates the three biggest infrastructure unknowns simultaneously — COOP/COEP plumbing, asset embedding (size budget, MIME), and reproducible qemu-wasm submodule build flow. Without this, every later step is theoretical.
**Delivers:** `bootroom serve --kernel <NORN Image>` boots NORN in Firefox/Chrome with the bundled qemu-wasm assets. Vanilla index.html + xterm.js + xterm-pty wired up; **no buttons, no WS, no config yet.**
**Addresses:** Single command to build (Cargo), single command to launch (`bootroom serve --kernel ...`), browser UI loads kernel via bundled qemu-wasm submodule.
**Uses:** axum 0.8, tower-http 0.6 (COOP/COEP layer), include_dir 0.7, clap 4.5, embedded xterm.js + xterm-pty.
**Avoids:** Pitfall 1 (COOP/COEP plumbing + in-page `crossOriginIsolated` probe), Pitfall 3 (`--assets-dir` dev override from day one), Pitfall 14 (default `-m` cap), part of Pitfall 10 (external-callable contract — test from `/tmp/empty`).
**Spike (must run before or during this phase):** confirm runtime kernel substitution via `Module.FS.writeFile` (so we don't have to re-run `file_packager.py` on every kernel change); confirm COOP/COEP propagates to *all* subresources (`.wasm`, `.data`, `.worker.js`).

### Phase 2: WebSocket + Serial Echo

**Rationale:** "The moment the architecture becomes real." Builds the substrate (PTY ↔ WS bridge) before the trigger (action buttons in Phase 3) so PTY quirks surface before the button protocol is locked in. Per architecture's deliberate ordering note.
**Delivers:** `/ws` endpoint, xterm-pty `master`/`slave` wired to ship `SerialIn`/`SerialOut` messages. Typing into a websocat session writes into guest stdin; serial output streams to the server.
**Uses:** axum WebSocket, `tokio::sync::broadcast`, serde-tagged `WsServerMsg`/`WsClientMsg` enums.
**Implements:** Architecture Pattern 3 (xterm-pty as byte boundary), Pattern 2 (TOML → broadcast → WS → JSON projection — schema can be empty here).

### Phase 3: Config + Buttons + Watcher

**Rationale:** Configures the substrate. TOML schema is the *keystone* of bootroom — it feeds both interactive UI and headless scenarios — so the schema must land before either is built on top. The kernel watcher slots in here because it shares `notify` infrastructure and the broadcast channel pattern with future config-file watching.
**Delivers:** `bootroom.toml` parsing in `bootroom-core`, `/api/config` JSON projection, button rendering, click → serial-write, CLI `--action` flag, kernel-path watcher with "fresher build" banner, `bootroom init` template scaffold.
**Addresses:** Action buttons defined in TOML, grouped, rendered. Pressing action sends serial input. CLI `--action` append/override. "Click Launch → freshest build" UX. Live reload on TOML change.
**Uses:** toml 0.8, notify 8 + notify-debouncer-full 0.5, `IndexMap` for stable action order.
**Avoids:** Pitfall 4 (debounce + ELF magic + size-stability), Pitfall 8 (`deny_unknown_fields` + `schema_version` + `bootroom check`), Pitfall 9 (single write funnel), Pitfall 12 (`IndexMap` for stable button order), Pitfall 13 (watch artifact file, not parent dir).

### Phase 4: Scenario Engine + Headless Run

**Rationale:** With substrate (Phase 2) and config (Phase 3) in place, scenarios are "ordered action refs + assertions" — natural extension. Headless `run` reuses *the exact same* served assets and WS protocol; chromiumoxide just provides a browser environment qemu-wasm requires. CI integration is the second-half of bootroom's value proposition.
**Delivers:** `bootroom-core::scenario` engine + JS twin in `app.js` (mirror logic), `?scenario=…&autoexit=1` URL handling, chromiumoxide driver in `bootroom run`, substring + regex assertions, per-action/per-scenario timeouts, exit 0/1, `--log-file` transcript, `--verbose` mode.
**Addresses:** Headless CI mode, scenario assertions, exit codes, NORN's CI integration.
**Uses:** chromiumoxide 0.7 (`tokio-runtime` feature), regex, `strip-ansi-escapes` for assertion-side normalization.
**Avoids:** Pitfall 5 (line-buffer + ANSI strip + per-action reset + explicit timeouts as defaults), Pitfall 6 (`crossOriginIsolated` startup check in headless; no `--disable-web-security`; documented minimum Chromium version), Pitfall 11 (bounded assertion buffer with eviction).
**Open risk to retire here:** the Phase-1 spike's headless Chromium SAB result determines whether chromiumoxide stays or Playwright takes over.

### Phase 5: Polish — Diagnostics, Reports, Doctor

**Rationale:** With end-to-end happy path working (Phases 1–4), polish the CI integration surfaces that make bootroom adoptable beyond NORN. These are *triggers*, not core: ship them when a real consumer requests them.
**Delivers:** JUnit XML / GitHub Actions annotation output (`--report-format`), `bootroom doctor` preflight (version, qemu-wasm rev, browser detected, header check), `bootroom run --watch` for TDD, screenshot button, per-action keyboard shortcuts, inline assertion-failure markers in xterm.
**Addresses:** Pitfalls 2 (qemu-wasm supported-flag matrix surfaced in `doctor`), 15 (browser-support matrix in `doctor`).

### Phase 6: Distribution

**Rationale:** Last because the embedded qemu-wasm artifacts and CLI surface need to be stable before release tooling locks them in. Release tooling regressions are most painful when retrofitted.
**Delivers:** `cargo install bootroom` from crates.io (with explicit `package.include`), `cargo-dist`-generated GitHub Release workflow for `x86_64-linux-musl`, `aarch64-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. `cargo-binstall` auto-discovery for free. Release-CI clean-container smoke test (`/tmp/empty` + fixture kernel).
**Addresses:** Installable from outside the repo, prebuilt binaries on GH Release, "command surface stays small."
**Avoids:** Pitfall 7 (`package.include` + musl + clean-container smoke), Pitfall 10 (external-directory CI job).

### Phase Ordering Rationale

- **Substrate before trigger** (architecture's load-bearing call): WebSocket + PTY (Phase 2) before action buttons (Phase 3). Button is a thin trigger over a serial-write; building the trigger first risks invalidation of its protocol when PTY quirks surface.
- **Schema before scenarios**: TOML schema (Phase 3) keystone for both Phase 3 (interactive buttons) and Phase 4 (scenarios). Getting it wrong forces breaking changes across both surfaces. `deny_unknown_fields` + `schema_version` from the first byte of schema.
- **Watcher with config, not standalone**: The kernel watcher (Phase 3) shares `notify` infrastructure with eventual config-file watching; landing them in the same phase avoids rework of the broadcast-channel pattern.
- **Polish after happy path**: Reports/doctor/shortcuts (Phase 5) are trigger-based — premature investment competes with core path stability.
- **Distribution last but prevention-checklist early**: Phase 6 cuts the actual release pipeline, but the prevention checklist (`package.include`, external-dir test, musl) gets added the first time release tooling is touched (likely during Phase 5).

### Research Flags

**Phases likely needing deeper research during planning (`/gsd-research-phase`):**
- **Phase 1:** qemu-wasm runtime kernel substitution mechanics (`Module.FS.writeFile` vs `file_packager.py` re-run) — load-bearing for the "freshest build" UX. *Spike, don't theorize.*
- **Phase 4:** Headless Chromium + SharedArrayBuffer + qemu-wasm end-to-end reliability — the single biggest unknown in the whole project. May require switching from chromiumoxide to Playwright. Phase-1 spike retires this risk but Phase 4 may revisit if the spike was inconclusive.

**Phases with standard patterns (skip research-phase):**
- **Phase 2:** axum WS + tokio broadcast is well-trodden ground; xterm-pty's API is documented and demonstrated in qemu-wasm's own example.
- **Phase 3:** TOML + serde + notify-debouncer-full are textbook Rust patterns.
- **Phase 5:** Report formats and CLI diagnostics are conventional.
- **Phase 6:** `cargo-dist` generates the release workflow; `cargo-binstall` works automatically.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Rust ecosystem choices are standard and verified against crates.io listings (2026-02 / 03). Cargo.toml is concrete and pin-ready. |
| Features | MEDIUM-HIGH | HIGH on table-stakes from comparable tools (bootimage, OSDK, Twister, v86); MEDIUM on UI/UX specifics for this niche (bootroom is carving new ground unifying interactive + CI). |
| Architecture | HIGH | Component split, data flow, and headless strategy verified against qemu-wasm submodule's own `examples/riscv64/src/htdocs/`. The "one process, two modes, same client code" decision is the load-bearing call and well-grounded. |
| Pitfalls | MEDIUM-HIGH | HIGH for headers / distribution / file-watching (well-documented across many sources). MEDIUM for qemu-wasm specifics (project is experimental, sparse prior art) and serial-console edge cases (drawn from embedded HIL literature, well-trodden but not bootroom-specific). |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **Headless Chromium + SAB + qemu-wasm end-to-end** is the dominant unknown. *Handle:* 1-day spike during Phase 1 — minimal axum server with COOP/COEP, navigate headless Chromium (`--headless=new`) to a static qemu-wasm boot, assert serial output appears. If green, chromiumoxide is the path; if red, Playwright subprocess is the well-trodden fallback (costs Node dep on CI runners).
- **Runtime kernel substitution into qemu-wasm `FS`** vs. re-running emscripten's `file_packager.py` per launch. *Handle:* Phase-1 spike. Plausible from emscripten docs but undemonstrated in qemu-wasm examples. If intractable, fall back to a launch-time pack rebuild (introduces a Node runtime dep we'd prefer to avoid).
- **`include_dir!` size budget** for the bundled qemu-wasm artifacts (`.wasm` + `.data` can be 10s of MB). *Handle:* if `cargo build` time or binary size becomes problematic, switch to `rust-embed` with compression, or `--qemu-wasm-dir` runtime path with embedded as default.
- **xterm-pty as a vendored ES module** (current upstream example uses a CDN URL). *Handle:* Phase-1 spike — confirm it works as a static ES module without any bundling tooling. If not, evaluate `esm.sh`-style local mirroring or a one-time pre-build.
- **Whether to compile `bootroom-core` to wasm-bindgen** so the same Rust scenario engine runs in the browser (avoids JS-side reimplementation). *Handle:* defer until the JS implementation feels painful; document as a known refactor lever in Phase 4.
- **Snapshot/save-state actions** are blocked on qemu-wasm upstream support. *Handle:* design action schema to accommodate the future kind; don't promise the feature in v1.

## Sources

Aggregated from `STACK.md`, `FEATURES.md`, `ARCHITECTURE.md`, `PITFALLS.md`. See each research file for full bibliographies.

### Primary (HIGH confidence)
- `qemu-wasm/README.md` (in-tree submodule) — build flags, output artifact set, COOP/COEP requirement, `Module.pty` injection point.
- `qemu-wasm/examples/riscv64/src/htdocs/{index.html,module.js}` (in-tree) — working browser-side pattern, xterm-pty `slave.write()` API usage.
- `qemu-wasm/examples/x86_64/src/xterm-pty.conf` (in-tree) — exact Apache header set bootroom must replicate in axum.
- `.planning/PROJECT.md` (this repo) — constraints, command surface, key decisions.
- [xterm-pty README](https://github.com/mame/xterm-pty), [xterm.js homepage](https://xtermjs.org/) — PTY API + terminal widget.
- [axum 0.8 releases](https://github.com/tokio-rs/axum/releases), [crates.io axum](https://crates.io/crates/axum), [notify-debouncer-full docs](https://docs.rs/notify-debouncer-full) — Rust crate versions and MSRV.
- [cargo-dist releases](https://github.com/axodotdev/cargo-dist/releases), [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) — release tooling.
- [web.dev COOP/COEP guide](https://web.dev/articles/coop-coep) — cross-origin isolation requirements.
- [oneuptime Rust file-watcher debouncing guide](https://oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view), [notify-rs](https://github.com/notify-rs/notify) — partial-write avoidance patterns.

### Secondary (MEDIUM confidence — multiple sources agree)
- [FOSDEM 2025 — Running QEMU Inside Browser (slides)](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-6290-running-qemu-inside-browser/slides/238760/slides_1dDtpcS.pdf) — qemu-wasm capability/limitation map.
- [chromiumoxide docs.rs](https://docs.rs/chromiumoxide), [chromiumoxide README](https://github.com/mattsse/chromiumoxide) — async tokio-native CDP client.
- [The Good Penguin — 5 Serial Automation Gotchas](https://www.thegoodpenguin.co.uk/blog/5-serial-automation-gotchas/), [Reverse to Build — Zephyr HIL CI Pipeline](https://reversetobuild.com/firmware-hil-ci-pipeline/), [Golioth automated hardware testing](https://blog.golioth.io/automated-hardware-testing-using-pytest/) — serial assertion conventions, inter-character timing.
- [rust-osdev/bootimage](https://github.com/rust-osdev/bootimage), [phil-opp — Testing OS in Rust](https://os.phil-opp.com/testing/), [Asterinas Book](https://asterinas.github.io/book/), [Zephyr Twister docs](https://docs.zephyrproject.org/latest/develop/test/twister.html) — comparable kernel test harnesses.
- [copy/v86](https://github.com/copy/v86), [Pebble qemu-wasm](https://ericmigi.github.io/pebble-qemu-wasm/), [ktock/qemu-wasm-demo](https://ktock.github.io/qemu-wasm-demo/) — browser-based emulator UI conventions.
- [QEMU RISC-V virt docs](https://www.qemu.org/docs/master/system/riscv/virt.html) — machine/CPU defaults.
- [rust-embed docs](https://docs.rs/rust-embed/latest/rust_embed/trait.RustEmbed.html), [include_dir docs](https://docs.rs/include_dir/latest/include_dir/) — asset embedding patterns.
- [Rust musl static binaries](https://doc.rust-lang.org/edition-guide/rust-2018/platform-and-target-support/musl-support-for-fully-static-binaries.html), [emk/rust-musl-builder](https://github.com/emk/rust-musl-builder) — distribution.
- [Chrome SAB origin-trial extension](https://developer.chrome.com/blog/shared-array-buffer-origin-trial-extension-124), [browser-actions/setup-chrome](https://github.com/browser-actions/setup-chrome), [Testing in headless browsers (wasm-bindgen)](https://rustwasm.github.io/docs/wasm-bindgen/wasm-bindgen-test/browsers.html) — headless CI environment.
- [serde-toml error reporting (Rust users forum)](https://users.rust-lang.org/t/serde-toml-error-reporting/127521), [clap + figment layered config](https://www.hecatron.com/posts/2025/rust-cli-cfg-opts/) — config UX.

### Tertiary (LOW confidence — Phase 1 spike must validate)
- Skipping emscripten `file_packager.py` at reload time by writing the kernel directly into `Module.FS` from JS. Plausible from emscripten docs; not demonstrated in qemu-wasm examples.
- Chromium `--headless=new` + SharedArrayBuffer reliability across CI runner images for qemu-wasm specifically. Search results note general SAB headless support; no targeted confirmation for the qemu-wasm path.
- `xterm-pty` as a vendored ES module without any bundler — upstream example uses an unpkg CDN URL.

---
*Research completed: 2026-05-17*
*Ready for roadmap: yes*
