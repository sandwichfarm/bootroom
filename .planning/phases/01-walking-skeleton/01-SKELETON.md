---
phase: 01-walking-skeleton
type: skeleton
status: draft
created: 2026-05-17
---

# Phase 1 — Walking Skeleton Narrative

> The thinnest end-to-end stack that proves the architecture. Subsequent phases (`/ws`, watcher, TOML, headless `run`) build on this skeleton without renegotiating it.

## User Story

**As a** kernel developer working on NORN (or any qemu-wasm-bootable RISC-V kernel), **I want to** run a single command (`bootroom serve --kernel ./Image`) and see my kernel boot in a real browser tab with the serial output streaming live, **so that** I can validate boot behaviour without standing up a separate QEMU host environment, a custom HTTP server, or any per-project tooling.

After Phase 1 a user can do exactly this:

```
$ bootroom serve --kernel /tmp/Image
Serving bootroom on http://127.0.0.1:8765 (Ctrl-C to stop)
```

Open the URL in Chrome 144+, watch the kernel-info header populate, the status pill transition `LOADING → RUNNING`, and serial bytes stream into the embedded xterm.js terminal. Nothing else (no buttons, no input wiring, no TOML).

## Architectural Decisions That Outlive Phase 1

Phase 2+ does NOT renegotiate these. They are the load-bearing contract:

### 1. Repository + crate layout

- **Path:** repo lives at `~/Develop/bootroom` (physically renamed from `norn-web` in Plan 01-01).
- **Cargo workspace** at the repo root with two crates:
  - `crates/bootroom-core/` — pure types, no I/O, no tokio. Phase 1 ships an empty `lib.rs` to prove the workspace builds. Phase 2 will add `SerialIn` / `SerialOut`; Phase 3 will add `Action`, `Group`, `Scenario`.
  - `crates/bootroom/` — the binary crate. Holds clap dispatch, axum app, COOP/COEP middleware, `include_dir!` embedding, route handlers.
- **Edition 2024, MSRV 1.85** declared in `[workspace.package]` and inherited per crate.
- **Dual license:** `LICENSE-MIT` + `LICENSE-APACHE` at workspace root; each `Cargo.toml` declares `license = "MIT OR Apache-2.0"`. Locked from day one.

### 2. Tech stack (verified versions per 01-RESEARCH.md, 2026-05-17)

| Layer | Crate | Version |
|-------|-------|---------|
| HTTP | `axum` | 0.8.9 |
| Service trait | `tower` | 0.5.3 |
| Middleware | `tower-http` | 0.6.10 (features: `set-header`, `trace`) |
| Runtime | `tokio` | 1.52.3 (features: `rt-multi-thread`, `macros`, `signal`, `sync`, `fs`) |
| File-to-body stream | `tokio-util` | latest (`io` feature) for `ReaderStream` |
| CLI | `clap` | 4.6.1 (`derive` feature) |
| Asset embed | `include_dir` | 0.7.4 |
| MIME | `mime_guess` | 2.0.5 |
| Serde | `serde` 1.0.228 / `serde_json` 1.x | |
| Errors | `anyhow` | 1.0.102 |
| Logs | `tracing` 0.1.44 / `tracing-subscriber` 0.3 (`env-filter`) | |
| Hash | `sha2` 0.10 / `hex` 0.4 | |
| Spike B only | `chromiumoxide` | 0.9.1 |

The browser side is **vanilla ES modules** — no bundler, no transpiler. Vendored libs:

- `xterm.js` 5.3.0 (the unscoped `xterm` package; do NOT migrate to `@xterm/xterm@6.x` — breaks xterm-pty addon)
- `xterm.css` 5.3.0
- `xterm-pty` 0.12.0

All three are committed under `crates/bootroom/web/vendor/` with pin records in `crates/bootroom/web/vendor/VERSIONS.md`. No CDN.

### 3. Asset pipeline

- **qemu-wasm artifacts** (`out.js`, `qemu-system-riscv64.wasm`, `qemu-system-riscv64.worker.js`, `qemu-system-riscv64.data`, `load.js`, the xterm-pty shim bundled by the build) live in `crates/bootroom/assets/qemu/` and are **committed to git**. `make qemu-assets` is the Makefile target that runs the qemu-wasm docker build; maintainers run it when bumping the submodule. The procedure is documented in `crates/bootroom/assets/qemu/REBUILD.md`.
- `build.rs` validates that `assets/qemu/` exists and contains the expected file set; on miss it emits a clear `qemu-wasm assets missing. Run 'make qemu-assets' from the repo root.` error and fails the build. It does NOT invoke docker.
- **UI assets** (`index.html`, `app.js`, `style.css`) live in `crates/bootroom/web/`; vendored libs in `crates/bootroom/web/vendor/`. Embedded via a second `include_dir!`.

### 4. Server contract

- **Bind:** default `127.0.0.1:8765`. Overridable via `--host <addr>` / `--port <N>`. `--port 0` binds an ephemeral port (used by tests).
- **Headers:** `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` attached via `tower_http::set_header::SetResponseHeaderLayer::overriding` at the top-level `Router`. Applies to every response including 404s and errors. No per-route opt-in.
- **Routes (Phase 1 only):**
  - `GET /` — serves `web/index.html`.
  - `GET /api/kernel/info` — JSON `{ path, size, mtime, sha256_prefix }` where `sha256_prefix` is the first 12 hex chars of `sha256(kernel_file)`.
  - `GET /kernel` — streams raw kernel bytes (`tokio::fs::File` → `ReaderStream` → `Body::from_stream`). `application/octet-stream`. NO Range support in Phase 1.
  - `GET /assets/{*path}` — embedded asset handler with `--assets-dir` disk override. Path-traversal protection (canonicalize, descendant check) required when `--assets-dir` is set.
- **No `/ws`, no `/api/config`, no SSE, no auto-browser-open.** These land in Phases 2/3.

### 5. UI contract

Per `01-UI-SPEC.md` (which is the authoritative visual + copy contract):

- One page (`/`), four components: `<header>` strip (wordmark + kernel-info `<dl>` + status pill), `crossOriginIsolated` probe banner (hidden by default), xterm.js terminal filling the rest of the viewport. No buttons, no input wiring.
- Inline classic `<script>` at the top of `<body>` checks `crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'`. If false, un-`hidden` the banner. This runs BEFORE any `type="module"` script.
- Status pill state machine: `LOADING` → `RUNNING` (on `Module.onRuntimeInitialized` AND `crossOriginIsolated === true`) → `HALTED` (on `Module.onExit` or `Module.onAbort`).
- xterm.js is mounted on the xterm-pty `slave` per the qemu-wasm reference wiring. `attachCustomKeyEventHandler(() => false)` is the explicit Phase 1 no-op for keyboard input (Phase 2 wires `/ws`).
- Page `<title>` = `bootroom — <kernel basename>`.

### 6. Spike outputs (consumed by later phases)

Two spike result files are produced in Phase 1 and read verbatim by later phases:

- `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` — locks Phase 4 driver choice (`chromiumoxide` vs Playwright subprocess vs deferred). **Sequenced first** — biggest project risk.
- `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` — locks Phase 2 reload mechanism (`module-fs-write` vs `pack-rebuild` vs `page-reload-only`). Must record the qemu-wasm submodule SHA.

Both write the standard frontmatter + Question/Method/Observations/Decision/Follow-ups skeleton from `01-CONTEXT.md` `<spike_outputs>`.

### 7. Deployment

There is no deployment in Phase 1. `bootroom` is a CLI that the user runs locally on `127.0.0.1`. Distribution (`cargo-dist`, GitHub Releases, `cargo install bootroom`) is Phase 6.

## End-to-End Slice (the literal walk)

```
                              ┌──────────────────────────────────────────────────────────┐
   User runs                  │  bootroom serve --kernel /tmp/Image                      │
   ───────────────────────▶   │    1. clap parses ServeArgs                              │
                              │    2. validates kernel file exists                       │
                              │    3. AppState { kernel, assets_dir } built              │
                              │    4. Router built; COOP+COEP layers attached            │
                              │    5. binds 127.0.0.1:8765                               │
                              │    6. prints URL                                         │
                              └──────────┬───────────────────────────────────────────────┘
                                         │
   User opens URL                        │ HTTP GET /
   ───────────────────────────▶          ▼
                              ┌──────────────────────────────────────────────────────────┐
                              │  axum handler returns web/index.html (COOP+COEP set)     │
                              └──────────┬───────────────────────────────────────────────┘
                                         │
                                         ▼
   Browser parses HTML        ┌──────────────────────────────────────────────────────────┐
                              │  inline classic <script>: SAB probe                      │
                              │    crossOriginIsolated && SharedArrayBuffer?             │
                              │      no  → un-hide #iso-banner                           │
                              │      yes → banner stays hidden                           │
                              └──────────┬───────────────────────────────────────────────┘
                                         │
                                         ▼
   Browser loads ESM         ┌──────────────────────────────────────────────────────────┐
                              │  <script type="module" src="/assets/web/app.js">         │
                              │    imports xterm.js, xterm-pty                           │
                              │    fetch /api/kernel/info → populate header              │
                              │    new Terminal(); openpty() → mount master              │
                              │    fetch /kernel → write into Module.FS('/pack/Image')   │
                              │    import /assets/qemu/out.js → initEmscriptenModule()   │
                              │    bind Module.onRuntimeInitialized → pill = RUNNING     │
                              │    bind Module.onExit / onAbort   → pill = HALTED        │
                              └──────────┬───────────────────────────────────────────────┘
                                         │
                                         ▼
   Guest boots               ┌──────────────────────────────────────────────────────────┐
                              │  qemu-system-riscv64.wasm boots; serial bytes flow       │
                              │  through xterm-pty slave → master → xterm.js renders     │
                              └──────────────────────────────────────────────────────────┘
```

That entire path is what Phase 1 must make work. Spikes B and A run alongside as separate plans and produce their own result MD files; neither blocks the Phase 1 happy path.

## What Phase 2 Inherits Without Negotiation

- The `axum::Router` instance and the COOP/COEP layer composition (Phase 2's `/ws` is just `.route("/ws", get(ws_handler))`).
- The `AppState` struct (Phase 2 extends with a `broadcast::Sender<SerialOut>`).
- The browser-side `Module.pty = slave` wiring (Phase 2 reads from `slave.onReadable` and writes via `slave.write`).
- The status pill state machine (Phase 2 adds `IDLE` once Launch/Reset exist).
- The vendored xterm.js + xterm-pty exact versions.
- The asset pipeline + `--assets-dir` flag (no rework needed for Phase 2 hot reload of the UI).
- The kernel-info contract (Phase 3 watcher reuses `/api/kernel/info`).

## Plan Index

| Plan | Wave | Objective | Files |
|------|------|-----------|-------|
| 01-01 | 1 | Workspace bootstrap: rename, Cargo workspace, license, README, .gitignore | `Cargo.toml`, `crates/*/Cargo.toml`, `LICENSE-*`, `README.md`, `.gitignore` |
| 01-02 | 1 | qemu-wasm asset pipeline: `make qemu-assets`, REBUILD.md, committed artifacts, `build.rs` validation | `Makefile`, `crates/bootroom/assets/qemu/*`, `crates/bootroom/build.rs` |
| 01-03 | 1 | Vendored web deps: xterm.js, xterm.css, xterm-pty, VERSIONS.md | `crates/bootroom/web/vendor/*` |
| 01-04 | 2 | axum server skeleton: CLI, COOP/COEP middleware, bind, embed roots, integration test harness | `crates/bootroom/src/main.rs`, `cli.rs`, `server.rs`, `headers.rs`, `state.rs` |
| 01-05 | 3 | API + asset handlers: `/api/kernel/info`, `/kernel`, `/assets/{*path}` with path-traversal protection | `crates/bootroom/src/{kernel_info,kernel_stream,assets,routes}.rs` |
| 01-06 | 3 | UI shell: `index.html`, `app.js`, `style.css` (parallel with 01-05 — different files) | `crates/bootroom/web/{index.html,app.js,style.css}` |
| 01-07 | 4 | Integration tests for SERV-01..05 and UI-07 API surface | `crates/bootroom/tests/*.rs` |
| 01-08 | 4 | Spike B: headless Chromium + SAB + qemu-wasm end-to-end (emits SPIKE-B-RESULT.md) — parallel with 01-07 | `crates/bootroom/spikes/spike-b/*` |
| 01-09 | 5 | Spike A: `Module.FS.writeFile` runtime kernel substitution (emits SPIKE-A-RESULT.md) | `crates/bootroom/spikes/spike-a/*` |

Wave 4 has 01-07 and 01-08 in parallel (no file overlap). Wave 5 has 01-09 alone (depends on 01-06's UI bits as a starting point for the in-page swap code).
