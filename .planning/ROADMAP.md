# Roadmap: bootroom

**Created:** 2026-05-17
**Mode:** mvp
**Granularity:** standard
**Phases:** 6
**v1 requirements:** 59
**Coverage:** 59/59 mapped (100%)

## Core Value Recap

Press one button, get the freshest kernel running in a browser with a click-to-trigger scenario library. Same binary, same assets, same WS protocol, two modes: `serve` (real tab) and `run` (headless CI).

## Phases

- [x] **Phase 1: Walking Skeleton** — `bootroom serve --kernel <path>` boots the kernel in a real browser via embedded qemu-wasm with COOP/COEP correct on the first request. (completed 2026-05-17)
- [ ] **Phase 2: WebSocket + Live Serial** — Interactive xterm.js console wired through the xterm-pty bridge over a single `/ws` endpoint; Launch / Reset / typing work end-to-end.
- [ ] **Phase 3: Config, Buttons, Watcher** — TOML schema drives grouped action buttons that inject serial bytes; kernel-path watcher surfaces a "fresher build" banner.
- [ ] **Phase 4: Scenario Engine + Headless `run`** — `bootroom run --scenario <name>` drives the same assets via headless Chromium and exits 0/1 on serial assertions.
- [ ] **Phase 5: Diagnostics & Doctor** — `bootroom doctor` preflight (headers, browser, qemu-wasm rev, config validity) closes the CLI surface; final subcommand set lands.
- [ ] **Phase 6: Distribution** — `cargo install bootroom`, `cargo-binstall`, and prebuilt multi-platform release binaries via cargo-dist; dual-licensed; external-callable from any kernel CI.

## Phase Details

### Phase 1: Walking Skeleton
**Goal:** A user runs one command and watches their RISC-V kernel boot in a real browser tab with all the cross-origin-isolation plumbing correct.
**Mode:** mvp
**Depends on:** Nothing (first phase)
**Requirements:** DIST-01, SERV-01, SERV-02, SERV-03, SERV-04, SERV-05, UI-01, UI-05, UI-07, CLI-03
**Spikes (Phase-1 required de-risking):**
  - **Spike A — Runtime kernel substitution:** Confirm `Module.FS.writeFile('/pack/Image', bytes)` (or equivalent) lets us inject the kernel without re-running emscripten's `file_packager.py` per launch. Half-day spike; if intractable, fall back to launch-time pack rebuild (introduces a Node dep we'd prefer to avoid — document the fallback).
  - **Spike B — Headless Chromium + SharedArrayBuffer + qemu-wasm end-to-end:** Boot a fixture kernel under `--headless=new` against the bare axum + COOP/COEP server, confirm `crossOriginIsolated === true`, confirm serial bytes flow out. Retires the single biggest project risk before Phase 4 commits to chromiumoxide; Playwright subprocess is the documented fallback.
**Success Criteria** (what must be TRUE):
  1. `cargo build` from a clean checkout produces a single static `bootroom` binary with embedded qemu-wasm artifacts and the vanilla-JS UI.
  2. `bootroom serve --kernel <path>` binds `127.0.0.1`, opens an HTTP server on a default port, and emits `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on **every** response (HTML, JS, WASM, worker, data).
  3. Opening the served URL in Chrome/Firefox boots `qemu-system-riscv64.wasm` with the supplied kernel; a `crossOriginIsolated` probe banner appears (with a fix hint) if SAB is unavailable.
  4. The page header displays the kernel's path, file size, and mtime; the bundled qemu-wasm submodule's xterm-pty integration is wired up (terminal visible, even if not yet interactive).
  5. `--assets-dir <path>`, `--port <N>`, and `--host <addr>` flags work; `bootroom serve` is the only command needed for the common case (no >1-line invocation).
**Plans:** 9/9 plans complete
Plans:
- [ ] 01-SKELETON.md — Walking Skeleton architectural narrative
- [x] 01-01-PLAN.md — Workspace bootstrap (Cargo workspace, license, README, .gitignore)
- [x] 01-02-PLAN.md — qemu-wasm asset pipeline (Makefile, build.rs validation, committed artifacts)
- [x] 01-03-PLAN.md — Vendored web deps (xterm.js 5.3.0, xterm-pty 0.12.0, VERSIONS.md)
- [x] 01-04-PLAN.md — axum server skeleton (CLI, COOP/COEP middleware, bind, embed roots)
- [x] 01-05-PLAN.md — API + asset handlers (/api/kernel/info, /kernel, /assets/{*path})
- [x] 01-06-PLAN.md — UI shell (index.html, app.js, style.css per UI-SPEC)
- [x] 01-07-PLAN.md — Integration tests (SERV-01..05, UI-07 API) + headed-browser smoke checkpoint
- [x] 01-08-PLAN.md — Spike B: headless Chromium + SAB + qemu-wasm (emits SPIKE-B-RESULT.md)
- [x] 01-09-PLAN.md — Spike A: Module.FS runtime kernel substitution (emits SPIKE-A-RESULT.md)

### Phase 2: WebSocket + Live Serial
**Goal:** A user types into the browser terminal and sees their keystrokes reach the guest kernel; serial output streams back in real time through the architecture's load-bearing PTY-over-WS substrate.
**Mode:** mvp
**Depends on:** Phase 1
**Requirements:** SERV-06, UI-02, UI-03, UI-04, UI-06, UI-08, UI-09, WS-01, WS-02, WS-03, WS-04
**Success Criteria** (what must be TRUE):
  1. xterm.js renders the live serial console mounted on the xterm-pty `slave` (= guest `ttyS0`); free-form keyboard input written to the terminal reaches guest stdin.
  2. A single `/ws` endpoint relays `SerialIn` (host→guest) and `SerialOut` (guest→host) messages; the protocol is serde-tagged JSON defined in `bootroom-core`.
  3. The "Launch" button (re)boots the guest with the freshest kernel build; the "Reset" button restarts the guest with the currently-loaded kernel without reloading the page.
  4. A status pill reflects guest state (Idle / Loading / Running / Halted); console "clear" and "copy all" controls work.
  5. All host→guest writes funnel through a single sender (no byte-interleaving possible) with configurable ~10–20ms inter-character pacing on injected sequences; running `bootroom serve` opens the user's default browser unless `--no-open`.
**Plans:** TBD
**UI hint:** yes

### Phase 3: Config, Buttons, Watcher
**Goal:** A user authors a `bootroom.toml`, sees grouped action buttons appear in the UI, clicks them to drive the guest, and runs `make` in their kernel repo to get a non-intrusive "fresher build available" banner.
**Mode:** mvp
**Depends on:** Phase 2
**Requirements:** CFG-01, CFG-02, CFG-03, CFG-04, CFG-05, CFG-06, CFG-07, CFG-08, CFG-09, CFG-10, ACT-01, ACT-02, ACT-03, ACT-04, WCH-01, WCH-02, WCH-03, WCH-04, WCH-05
**Success Criteria** (what must be TRUE):
  1. `bootroom.toml` is parsed from CWD by default (overridable via `--config`); all config structs use `#[serde(deny_unknown_fields)]` and a top-level `schema_version` field is required — typos and incompatible versions produce loud, locating errors.
  2. Action buttons (labeled, grouped, byte/string payloads) render from `/api/config` in stable TOML insertion order; clicking a button writes its byte sequence to the guest serial via `slave.write`. CLI `--action "label=<bytes>"` (repeatable) appends/overrides ad-hoc actions.
  3. Scenarios are declared as ordered references to actions plus optional assertions and timeouts; load-time validation rejects references to missing actions with a clear, locating error. `bootroom check` validates the config without running the server; `bootroom init` writes a minimal example.
  4. Editing `bootroom.toml` while `serve` is running updates the UI in place (no server restart).
  5. The kernel watcher uses `notify-debouncer-full` (~300ms debounce), watches the parent dir for the kernel filename (atomic-rename safe), requires size-stability across debounce ticks, sniffs ELF magic bytes (rejecting non-ELF with a UI warning), and surfaces a non-intrusive "fresher build available" banner — never auto-reloads. Manual serial typing is disabled while a scenario is running and re-enabled on completion.
**Plans:** TBD
**UI hint:** yes

### Phase 4: Scenario Engine + Headless `run`
**Goal:** A kernel CI job runs `bootroom run --kernel build/Image --scenario boot_smoke`, gets a 0/1 exit code from serial-output assertions, and a full transcript on failure — using the exact same embedded assets and WS protocol as `serve` mode.
**Mode:** mvp
**Depends on:** Phase 3
**Requirements:** RUN-01, RUN-02, RUN-03, RUN-04, RUN-05, RUN-06, RUN-07, RUN-08, RUN-09, RUN-10, CLI-02
**Success Criteria** (what must be TRUE):
  1. `bootroom run --kernel <path> --scenario <name>` drives a headless Chromium browser (via `chromiumoxide`; Playwright subprocess as documented fallback if Phase-1 Spike B was inconclusive) against the same embedded assets and same `/ws` protocol — no separate CI code path.
  2. Scenario assertions support substring and anchored regex match against per-action serial buffers; ANSI escape sequences are stripped before matching; matches operate on line-buffered (`\r?\n`) output.
  3. Per-action and per-scenario timeouts have explicit defaults and produce structured failures; per-action serial buffers reset by default (configurable carry-over).
  4. Headless mode performs a `crossOriginIsolated` startup self-check and aborts early with a clear message if SAB is unavailable; the process exits 0 on pass / non-zero on fail.
  5. `--log-file <path>` writes a full transcript (timestamps, action sends, serial output, assertion results); `--verbose` streams scenario progress to stderr for CI logs. Common flags (`--kernel`, `--config`, `--verbose`) are shared across `serve` and `run` via clap `#[flatten]`.
**Plans:** TBD

### Phase 5: Diagnostics & Doctor
**Goal:** A user (or a confused CI job) runs `bootroom doctor` and gets a single-screen preflight report — version, embedded qemu-wasm rev, detected browser, COOP/COEP self-check, config validity — closing the documented CLI surface.
**Mode:** mvp
**Depends on:** Phase 4
**Requirements:** CLI-01, DOC-01
**Success Criteria** (what must be TRUE):
  1. The full top-level subcommand set is finalized: `serve`, `run`, `init`, `check`, `doctor`, plus `--version` and `--help` — all short verbs, all discoverable via `bootroom --help`.
  2. `bootroom doctor` reports bootroom version, embedded qemu-wasm submodule rev, detected browser (Chrome/Chromium path + version), the COOP/COEP self-check against a live `/` request, and current `bootroom.toml` validity.
  3. `doctor` exits 0 when all checks pass and non-zero with a structured summary when any check fails — usable directly in CI preflight steps.
**Plans:** TBD

### Phase 6: Distribution
**Goal:** A kernel project on any supported platform (Linux x86_64/aarch64-musl, macOS x86_64/aarch64) installs `bootroom` in one step — `cargo install bootroom`, `cargo binstall bootroom`, or a `curl | tar -xz` from a GitHub Release — and runs it from any working directory with no in-repo assumptions.
**Mode:** mvp
**Depends on:** Phase 5
**Requirements:** DIST-02, DIST-03, DIST-04, DIST-05, DIST-06, DIST-07
**Success Criteria** (what must be TRUE):
  1. `make install` / `cargo install --path .` installs the binary locally from a clean checkout; the binary then runs from any working directory because all assets are embedded via `include_dir!` (no in-repo path assumptions). License is MIT OR Apache-2.0 (dual SPDX) — `cargo deny` enforces it in CI.
  2. `bootroom` is published to crates.io; `cargo install bootroom` from a clean container produces a working binary that boots a fixture kernel from `/tmp/empty` (release-CI smoke test gates publication).
  3. Prebuilt release binaries via `cargo-dist` cover `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`; `Cargo.toml` `package.include` explicitly lists `web/` and the bundled qemu-wasm artifacts so they cannot be silently dropped by `cargo publish`.
  4. `cargo binstall bootroom` discovers and installs the prebuilt release artifacts automatically (no extra metadata work — drops out of `cargo-dist` + `[package.repository]`).
**Plans:** TBD

## Phase Ordering Rationale

- **Substrate before trigger:** WebSocket + PTY bridge (Phase 2) lands before action buttons (Phase 3). A button is a thin trigger over a serial-write; building the trigger before the substrate risks invalidation of its protocol when PTY quirks surface.
- **Schema before scenarios:** The TOML schema (Phase 3) is the keystone — it feeds both interactive UI (Phase 3) and headless scenarios (Phase 4). `deny_unknown_fields` + `schema_version` ship with the very first byte of schema.
- **Watcher with config, not standalone:** Kernel watcher (Phase 3) shares `notify` infrastructure with eventual config-file watching; landing them together avoids rework of the broadcast pattern.
- **Polish after happy path:** Doctor / final subcommand set (Phase 5) is consumer-triggered; investing earlier competes with core-path stability.
- **Distribution last but prevention-checklist early:** Phase 6 cuts the actual release pipeline. The prevention checklist (`package.include`, musl targets, external-dir smoke test) is added the first time release tooling is touched — likely incrementally through Phases 5 and 6.

## Coverage

All 59 v1 requirements mapped to exactly one phase. See REQUIREMENTS.md Traceability section for the full mapping table.

| Phase | Count | Category Coverage |
|-------|-------|--------------------|
| 1 — Walking Skeleton | 10 | DIST (1), SERV (5), UI (3), CLI (1) |
| 2 — WebSocket + Live Serial | 11 | SERV (1), UI (6), WS (4) |
| 3 — Config, Buttons, Watcher | 19 | CFG (10), ACT (4), WCH (5) |
| 4 — Scenario Engine + Headless | 11 | RUN (10), CLI (1) |
| 5 — Diagnostics & Doctor | 2 | CLI (1), DOC (1) |
| 6 — Distribution | 6 | DIST (6) |
| **Total** | **59** | **DIST (7), SERV (6), UI (9), WS (4), CFG (10), ACT (4), WCH (5), RUN (10), CLI (3), DOC (1)** |

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Walking Skeleton | 4/9 | In progress | — |
| 2. WebSocket + Live Serial | 0/? | Not started | — |
| 3. Config, Buttons, Watcher | 0/? | Not started | — |
| 4. Scenario Engine + Headless | 0/? | Not started | — |
| 5. Diagnostics & Doctor | 0/? | Not started | — |
| 6. Distribution | 0/? | Not started | — |

---
*Roadmap created: 2026-05-17 via gsd-roadmapper*
