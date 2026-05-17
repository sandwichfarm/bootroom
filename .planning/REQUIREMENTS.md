# Requirements: bootroom

**Defined:** 2026-05-17
**Core Value:** Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Build & Distribution

- [ ] **DIST-01**: Single command builds the project from a clean checkout (`make` or `cargo build`).
- [ ] **DIST-02**: Single command installs the binary locally (`make install` or `cargo install --path .`).
- [ ] **DIST-03**: Published to crates.io; installable via `cargo install bootroom`.
- [ ] **DIST-04**: Prebuilt release binaries for x86_64-linux-musl, aarch64-linux-musl, x86_64-apple-darwin, aarch64-apple-darwin via cargo-dist + GitHub Releases.
- [ ] **DIST-05**: Binary runs from any working directory, no in-repo assumptions (assets embedded via `include_dir!`).
- [ ] **DIST-06**: `cargo-binstall bootroom` works automatically from release artifacts.
- [ ] **DIST-07**: License is MIT OR Apache-2.0 (dual SPDX).

### Server (`serve` mode)

- [ ] **SERV-01**: `bootroom serve --kernel <path>` starts an HTTP server bound to `127.0.0.1` on a default port.
- [ ] **SERV-02**: Server emits `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on every response.
- [ ] **SERV-03**: Server embeds and serves the qemu-wasm artifacts (`.wasm`, `.data`, `.worker.js`) and the vanilla-JS UI from `include_dir!`.
- [ ] **SERV-04**: `--assets-dir <path>` runtime flag overrides the embedded assets for development of bootroom itself.
- [ ] **SERV-05**: `--port <N>` and `--host <addr>` flags override the bind defaults.
- [ ] **SERV-06**: Server opens the user's default browser to the harness URL on start (suppressed by `--no-open`).

### Browser Harness

- [ ] **UI-01**: Page boots `qemu-system-riscv64.wasm` using qemu-wasm's xterm-pty integration, with the supplied kernel loaded into the guest.
- [ ] **UI-02**: Live serial console rendered via xterm.js, mounted on the xterm-pty `slave` (= guest `ttyS0`).
- [ ] **UI-03**: Serial console supports free-form keyboard input written through to the guest.
- [ ] **UI-04**: Console has "clear" and "copy all" controls.
- [ ] **UI-05**: Page displays a `crossOriginIsolated` probe banner with a fix hint when SAB is unavailable.
- [ ] **UI-06**: Status pill shows guest state (Idle / Loading / Running / Halted).
- [ ] **UI-07**: Header shows kernel info: path, size, mtime.
- [ ] **UI-08**: "Launch" button (re)boots the guest with the freshest kernel build.
- [ ] **UI-09**: "Reset" button restarts the guest with the currently-loaded kernel (no reload).

### WebSocket Bridge

- [ ] **WS-01**: Single `/ws` endpoint relays `SerialIn` (host→guest bytes) and `SerialOut` (guest→host bytes).
- [ ] **WS-02**: All host→guest writes are funneled through a single sender to prevent byte-interleaving between UI input and scenarios.
- [ ] **WS-03**: Inter-character pacing (default ~10–20ms, configurable) on injected byte sequences.
- [ ] **WS-04**: Message protocol is serde-tagged JSON; types live in `bootroom-core`.

### Config (TOML)

- [ ] **CFG-01**: `bootroom.toml` parsed from CWD by default; overridable via `--config <path>`.
- [ ] **CFG-02**: Config supports labeled, grouped **actions** with byte/string payloads sent to guest serial.
- [ ] **CFG-03**: Config supports **scenarios** as ordered references to actions plus optional assertions and timeouts.
- [ ] **CFG-04**: Top-level `schema_version` field; load fails on incompatible version.
- [ ] **CFG-05**: All config structs use `#[serde(deny_unknown_fields)]`; typos produce loud errors.
- [ ] **CFG-06**: Load-time validation that scenarios reference only existing actions; failure prints a clear, locating error.
- [ ] **CFG-07**: `bootroom check` subcommand validates the config without running the server.
- [ ] **CFG-08**: `bootroom init` writes a minimal example `bootroom.toml`.
- [ ] **CFG-09**: Action button order in the UI is stable (preserved from TOML insertion order).
- [ ] **CFG-10**: Live reload of `bootroom.toml`: editing the file updates the UI in place (no server restart).

### Action Buttons

- [ ] **ACT-01**: Buttons rendered from `/api/config` JSON projection, grouped per TOML.
- [ ] **ACT-02**: Pressing a button writes its byte sequence to the guest serial (via xterm-pty `slave.write`).
- [ ] **ACT-03**: CLI `--action "label=<bytes>"` (repeatable) appends/overrides ad-hoc actions without editing config.
- [ ] **ACT-04**: Manual serial typing is disabled while a scenario is running; re-enabled on completion.

### Kernel Watcher

- [ ] **WCH-01**: Watches the kernel path with `notify-debouncer-full` (~300ms debounce).
- [ ] **WCH-02**: Detects atomic-rename builds (watches the parent dir for the kernel filename).
- [ ] **WCH-03**: Requires size-stability across debounce ticks before considering a new build ready.
- [ ] **WCH-04**: Sniffs ELF magic bytes; rejects non-ELF files with a UI warning.
- [ ] **WCH-05**: A new build surfaces a non-intrusive "fresher build available" banner; user (or CI) triggers Launch — never auto-reload by default.

### Headless / CI (`run` mode)

- [ ] **RUN-01**: `bootroom run --kernel <path> --scenario <name>` executes a scenario headlessly and exits 0 on pass / non-zero on fail.
- [ ] **RUN-02**: Headless mode drives Chromium via `chromiumoxide` (Playwright subprocess as documented fallback).
- [ ] **RUN-03**: Same embedded assets and same WS protocol as `serve` mode — no separate code path for CI.
- [ ] **RUN-04**: Assertions support substring and anchored regex match against per-action serial buffers.
- [ ] **RUN-05**: ANSI escape sequences are stripped before matching; matches operate on line-buffered (`\r?\n`) serial output.
- [ ] **RUN-06**: Per-action and per-scenario timeouts with explicit defaults; timeouts produce structured failures.
- [ ] **RUN-07**: Per-action serial buffer reset by default (configurable to "carry-over").
- [ ] **RUN-08**: `--log-file <path>` writes a full transcript (timestamps, action sends, serial output, assertion results).
- [ ] **RUN-09**: `--verbose` mode prints scenario progress to stderr for CI logs.
- [ ] **RUN-10**: Headless mode performs a `crossOriginIsolated` startup self-check; aborts early with a clear message if SAB is unavailable.

### CLI Surface

- [ ] **CLI-01**: Top-level subcommands are short verbs: `serve`, `run`, `init`, `check`, `doctor`, `--version`, `--help`.
- [ ] **CLI-02**: Common flags (`--kernel`, `--config`, `--verbose`) are shared across subcommands via clap `#[flatten]`.
- [ ] **CLI-03**: Common task = one command; no >1-line invocations required for routine work.

### Diagnostics

- [ ] **DOC-01**: `bootroom doctor` reports bootroom version, embedded qemu-wasm rev, detected browser, COOP/COEP self-check on `/`, and current config validity.

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Reporting

- **REP-01**: `--report-format=junit` emits JUnit XML for CI test reporters.
- **REP-02**: `--report-format=github` emits GitHub Actions `::error::` annotations on assertion failures.
- **REP-03**: Inline assertion failure markers rendered in the xterm scrollback.

### Authoring UX

- **AUTH-01**: `bootroom run --watch` — re-run scenario on TOML or kernel change (local TDD loop).
- **AUTH-02**: Per-action keyboard shortcuts driven from TOML.
- **AUTH-03**: Screenshot button capturing the xterm output to PNG.
- **AUTH-04**: Record-and-replay: click buttons interactively, emit a TOML scenario from the session.

### Extended Targets

- **TGT-01**: Multi-arch action schema (per-action arch tag) supporting x86_64 / AArch64 guest images.
- **TGT-02**: `--frozen-export <dir>` writes a static, self-contained reproducer bundle (HTML + assets + kernel) reviewers can open with no server.

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| GDB / step-through debugging | Defer to v2+; serial-based assertions cover initial test needs. |
| Multi-kernel side-by-side comparison | Niche; resurface only on real demand. |
| Persistent test history / dashboards | Out of scope for a local dev tool; CI artifacts handle longitudinal data. |
| Non-RISC-V architectures in v1 | qemu-wasm supports more, but v1 is RISC-V-only to keep surface small. |
| Hot-swap kernel mid-run | Force user to click Launch; live-replacing the running guest is complexity we don't need. |
| Authentication / multi-user | Local dev tool; never exposed to the public internet. |
| Plugin system | Anti-pattern at this stage; bake batteries-included defaults instead. |
| Real-time collaboration | Out of scope for a dev tool. |
| Web IDE / source editor | Outside the test-harness charter. |
| Headless without a browser (WASI standalone) | qemu-wasm's TCG JIT requires real `WebAssembly.Module` runtime instantiation + emscripten pthreads; no current WASI runtime supports the combination. Major research project, not in scope. |
| npm-based frontend toolchain | Project constraint: zero npm dep. Vanilla ES modules + vendored xterm.js are the contract. |
| Snapshot / save-state actions | Blocked on qemu-wasm upstream support; schema may accommodate the future kind but feature is not promised. |
| QMP / monitor command actions in v1 | Serial-injection covers the v1 need; QMP surface in qemu-wasm is undertested. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (populated by gsd-roadmapper) | — | Pending |

**Coverage:**
- v1 requirements: 53 total
- Mapped to phases: 0 (pending roadmap)
- Unmapped: 53 ⚠️

---
*Requirements defined: 2026-05-17*
*Last updated: 2026-05-17 after initial definition*
