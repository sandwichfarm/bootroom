# Phase 1: Walking Skeleton — Research

**Researched:** 2026-05-17
**Domain:** Rust workspace bootstrap + axum HTTP server with cross-origin isolation + embedded qemu-wasm + vanilla-JS UI shell + two Phase-1 spikes (headless Chromium SAB; Module.FS kernel substitution)
**Confidence:** HIGH for the Rust/web-server stack; HIGH for COOP/COEP mechanics; MEDIUM-HIGH for qemu-wasm wiring (confirmed by inspecting the in-tree submodule example); MEDIUM for spike outcomes (the entire point of the spikes is to remove uncertainty here).

## Summary

Phase 1 stands up the load-bearing scaffolding for the entire project: a Cargo workspace (`bootroom-core` + `bootroom`), an axum 0.8 HTTP server that serves embedded UI + qemu-wasm artifacts with mandatory COOP/COEP headers on **every** response, a minimal vanilla-JS UI (kernel-info header, status pill, xterm.js terminal, cross-origin-isolation probe banner), and **two timed spikes** (Spike B: headless Chromium + SAB + qemu-wasm; Spike A: `Module.FS.writeFile` runtime kernel substitution). Spike outputs gate Phase-4 driver choice and Phase-2 reload mechanism respectively, but neither blocks Phase 1 itself.

The technical risk is concentrated in three places, all addressed before code: (1) COOP/COEP must apply to every subresource — one missing header silently breaks SAB. Mitigation is `tower_http::set_header::SetResponseHeaderLayer::overriding` applied once at the top of the router, plus a `crossOriginIsolated` probe banner on every page load. (2) The qemu-wasm asset pipeline must not become a per-build docker dependency — `make qemu-assets` runs the docker build once, output is committed to `crates/bootroom/assets/qemu/`, `build.rs` only validates presence. (3) `include_dir!` alone makes UI iteration painful — the `--assets-dir <path>` override is a Phase 1 hard requirement, not a nice-to-have.

**Primary recommendation:** Build the Phase 1 binary by following the in-tree reference (`qemu-wasm/examples/riscv64/src/htdocs/index.html` + `module.js` + `xterm-pty.conf`) verbatim for the qemu-wasm wiring, then wrap it with axum + tower-http for serving and clap for the CLI. Run Spike B **first** to retire the project's biggest unknown before any later phase commits to chromiumoxide; run Spike A second to lock the Phase-2 reload UX. Use the latest stable versions of every dependency — every Phase-1 dep is mature with no compatibility traps in current versions.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Serve HTML/JS/CSS/WASM assets | Frontend Server (axum) | — | The bootroom binary IS the frontend server; assets are embedded into the binary. |
| Apply COOP/COEP headers | Frontend Server (tower middleware) | — | Must be applied at the lowest middleware layer so every response inherits them — no opt-in routes. |
| Kernel-info JSON endpoint (`/api/kernel/info`) | API / Backend (axum handler) | — | Reads disk metadata + sha256 prefix; pure read-only API. |
| Raw kernel byte serve (`/kernel`) | API / Backend (axum handler) | — | Streams the file referenced by `--kernel`; browser fetches and writes into `Module.FS`. |
| qemu-wasm module instantiation + execution | Browser / Client | — | qemu-wasm requires `WebAssembly.Module`/`Instance` runtime APIs only the browser provides. |
| xterm.js mount + xterm-pty bridge | Browser / Client | — | The PTY is in-page; QEMU's chardev is wired to the slave at qemu-wasm build time. |
| `crossOriginIsolated` probe + banner render | Browser / Client | — | Must run before module scripts so the banner shows even if everything else fails. |
| Status pill state machine (Loading → Running → Halted) | Browser / Client | — | Observes emscripten Module lifecycle callbacks; no server round-trip. |
| CLI argument parsing (`serve --kernel ... --port ... --host ... --assets-dir ...`) | API / Backend (clap derive) | — | Pure Rust process boundary. |
| Asset embedding (`include_dir!`) + `--assets-dir` override | API / Backend | — | Compile-time embed for release; runtime override reads from disk for dev. |
| Spike B: headless Chromium driver | API / Backend (chromiumoxide) | Browser (controlled by CDP) | Test code; lives outside main crate in `crates/bootroom/spikes/spike-b/`. |
| Spike A: `Module.FS.writeFile` swap | Browser / Client | API / Backend (serves variant kernels) | Test code; lives outside main crate in `crates/bootroom/spikes/spike-a/`. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Rust | 1.85+ (edition 2024) `[VERIFIED: rustc 1.90 installed locally]` | Implementation language | Locked by project; MSRV set by `notify-debouncer-full`. |
| axum | 0.8.9 `[VERIFIED: crates.io 2026-05-17]` | HTTP server, routing, middleware, future SSE/WS | Tokio-native, tower-based, idiomatic. v0.8 has native async traits — copy examples from 0.8 docs only, not 0.7. |
| tower | 0.5.3 `[VERIFIED: crates.io]` | Service abstraction underpinning axum middleware | Required transitively; pin explicitly so middleware composition is stable. |
| tower-http | 0.6.10 `[VERIFIED: crates.io]` | `SetResponseHeaderLayer` for COOP/COEP, `TraceLayer` for request logging | `set-header` feature is the idiomatic COOP/COEP plumbing. |
| tokio | 1.52.3 `[VERIFIED: crates.io]` | Async runtime | Features: `rt-multi-thread`, `macros`, `signal`, `sync`, `fs`. Don't pull `process` until Phase 2. |
| clap | 4.6.1 `[VERIFIED: crates.io]` | Subcommand CLI parsing | Derive macro; `#[command(flatten)]` will be used in Phase 2+ but the `Args` structs should be designed for it now. |
| include_dir | 0.7.4 `[VERIFIED: crates.io]` | Compile-time embed of `web/` and `assets/qemu/` directories | One macro, zero deps, walks at runtime via `Dir::get_file()`. |
| mime_guess | 2.0.5 `[VERIFIED: crates.io]` | Content-Type for embedded assets | `tower-http`'s `ServeDir` is not used (we serve from `include_dir!`), so we set MIME explicitly. Critical for `.wasm` (must be `application/wasm`) and `.js` (`text/javascript`). |
| serde | 1.0.228 `[VERIFIED: crates.io]` | JSON serialization for `/api/kernel/info` | Required by `serde_json`. |
| serde_json | 1.x `[VERIFIED: crates.io]` | Serialize kernel-info struct to wire | `axum::Json` wraps it for one-line responses. |
| anyhow | 1.0.102 `[VERIFIED: crates.io]` | Error handling in the binary | Application-level; not `thiserror` (bootroom is not a library). |
| tracing | 0.1.44 `[VERIFIED: crates.io]` | Structured logs (server startup, request trace) | Pairs with `tower-http::trace::TraceLayer`. |
| tracing-subscriber | 0.3.x `[VERIFIED: crates.io]` | Subscriber + EnvFilter for `RUST_LOG` | `env-filter` feature. |
| sha2 | 0.10.x `[CITED: docs.rs/sha2]` | SHA-256 of kernel file for `/api/kernel/info` `sha256_prefix` | Standard hash crate; first 12 hex chars per UI-SPEC. |
| hex | 0.4.x `[CITED: docs.rs/hex]` | Hex-encode the SHA-256 prefix | One-liner; alternative is bit-manip on the bytes. |

### Supporting (Spike B only — kept out of main crate's dep tree if possible)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chromiumoxide | 0.9.1 `[VERIFIED: crates.io]` | Headless Chromium CDP driver | Phase-1 Spike B only; if Spike B is green it later lands in the main crate for Phase 4. Note: 0.9 is newer than the project STACK.md (0.7) — verify breaking changes against [chromiumoxide CHANGELOG](https://github.com/mattsse/chromiumoxide/releases). |

### Browser-Side (vendored under `crates/bootroom/web/vendor/`)
| Library | Version | Purpose | Notes |
|---------|---------|---------|-------|
| xterm.js | 5.3.0 `[VERIFIED: npm registry — `xterm@5.3.0` is the version qemu-wasm pins]` | Terminal widget | **Do not** bump to `@xterm/xterm@6.x` (scoped package, breaking changes). The xterm-pty addon targets `xterm@5.3.0` specifically. |
| xterm.css | 5.3.0 (ships with xterm.js) | Terminal stylesheet | Vendored next to xterm.js. |
| xterm-pty | 0.12.0 `[VERIFIED: npm registry 2026-05-17]` | PTY bridge addon | `openpty()` + `master`/`slave` API. Mounted via `xterm.loadAddon(master)`. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `include_dir` | `rust-embed` (with `debug-embed` feature) | rust-embed has built-in debug/release switch and optional compression — useful if qemu-wasm artifacts push binary size past ~30 MB. We don't switch in Phase 1; we handle dev iteration with explicit `--assets-dir` flag instead. |
| `include_dir` for the `assets/qemu/` directory | `include_bytes!` per file | `include_dir` walks the directory and gives us `Dir.get_file()`. With ~5 qemu files, `include_bytes!` would also work but loses the iteration story for adding files later. |
| `tower-http`'s `ServeDir` | Hand-rolled `include_dir!` handler | `ServeDir` reads from disk; we need embedded. Hand-roll a small handler that walks the `Dir` and sets `mime_guess`-derived Content-Type. |
| `clap` derive | `clap` builder | Derive is clearer for our static CLI. We pin derive for the whole project. |
| Headed `xterm@5.3.0` (the `xterm` npm package) | `@xterm/xterm@6.0.0` (the scoped successor) | The xterm-pty addon was written against `xterm@5.3.0`'s addon API. The 6.x scoped package broke this. Use 5.3.0. |
| chromiumoxide for Spike B | Playwright via Node subprocess | chromiumoxide stays in-Rust (matches our zero-Node constraint) and is the path documented in STACK.md. Playwright is the **fallback** if Spike B fails. |

**Installation:**
```bash
# Workspace root Cargo.toml lists shared deps under [workspace.dependencies].
# Per-crate Cargo.toml just references workspace = true.
# No `cargo install` step beyond the binary itself — all deps are crates.io.
```

**Version verification protocol:** Run `cargo update` once at workspace creation, then commit `Cargo.lock`. Use `^`-style ranges in `Cargo.toml` (e.g. `axum = "0.8"`) but the actual resolution is locked.

### Version delta vs project-level STACK.md

The project's `.planning/research/STACK.md` was written 2026-05-17 too; minor drift since then:

| Crate | STACK.md version | Current stable | Action |
|-------|------------------|----------------|--------|
| `clap` | 4.5.x | 4.6.1 | Use 4.6 — no breaking changes vs 4.5 derive API. |
| `notify-debouncer-full` | 0.5.x | 0.7.0 | Not needed in Phase 1; flag for Phase 3 to re-research before locking. |
| `chromiumoxide` | 0.7 | 0.9.1 | Use 0.9 for Spike B but check breaking changes; revisit if Spike B chooses chromiumoxide for Phase 4. |
| `toml` | 0.8.x | 1.1.2 | Not needed in Phase 1; flag for Phase 3. |
| `mime_guess` | 2 | 2.0.5 | Use 2.0.5 (semver-compatible with `2`). |

All other versions match.

## Architecture Patterns

### System Architecture Diagram

```
                              Phase 1 dataflow

  ┌────────────────────────────────────────────────────────────────────────────┐
  │                          bootroom (single process)                         │
  │                                                                            │
  │   ┌──────────────────┐                                                     │
  │   │ clap dispatch    │  parses: serve --kernel <path> [--port N]           │
  │   │ (main.rs)        │          [--host addr] [--assets-dir <path>]        │
  │   └────────┬─────────┘                                                     │
  │            │                                                               │
  │            ▼                                                               │
  │   ┌──────────────────┐    ┌─────────────────────────────────────────┐      │
  │   │ AppState         │───▶│ axum::Router                            │      │
  │   │ - kernel_path    │    │   .layer(COOP) .layer(COEP)             │      │
  │   │ - assets_dir Opt │    │   .layer(TraceLayer)                    │      │
  │   │ - embedded Dir   │    │                                         │      │
  │   └──────────────────┘    │   GET /                  → index.html   │      │
  │                           │   GET /api/kernel/info   → JSON         │      │
  │                           │   GET /kernel            → raw bytes    │      │
  │                           │   GET /assets/*path      → embedded /   │      │
  │                           │                            disk asset   │      │
  │                           └────────────────┬────────────────────────┘      │
  │                                            │ TCP                           │
  └────────────────────────────────────────────┼───────────────────────────────┘
                                               │ 127.0.0.1:8765
                                               ▼
                              ┌─────────────────────────────────┐
                              │  Real browser tab               │
                              │  ┌───────────────────────────┐  │
                              │  │ index.html                │  │
                              │  │   inline: SAB probe       │  │ ← runs first
                              │  │     → maybe show banner   │  │
                              │  │   <header> kernel-info    │  │ ← populated by
                              │  │   <span> status pill      │  │   fetch('/api/kernel/info')
                              │  │   <div id="terminal">     │  │
                              │  │                           │  │
                              │  │ <script type="module">    │  │
                              │  │   import xterm/xterm-pty  │  │
                              │  │   openpty() → mount xterm │  │
                              │  │   Module.pty = slave      │  │
                              │  │   fetch('/kernel')        │  │ ← initial kernel
                              │  │     → Module.FS.writeFile │  │   load
                              │  │   import('./out.js')      │  │
                              │  │     → boot QEMU           │  │
                              │  │                           │  │
                              │  │   Module.onRuntimeInit    │  │ ─┐
                              │  │     → pill = RUNNING      │  │  │ Status pill
                              │  │   Module.onExit/onAbort   │  │  │ state machine
                              │  │     → pill = HALTED       │  │ ─┘
                              │  └───────────────────────────┘  │
                              └─────────────────────────────────┘
```

Key flows traced left-to-right:

1. **Page load** → inline `<script>` checks `crossOriginIsolated`. If false, un-`hidden` the banner; otherwise it stays hidden.
2. **Kernel info fetch** → `GET /api/kernel/info` returns `{ path, size, mtime, sha256_prefix }`; JS populates header.
3. **Kernel bytes** → `GET /kernel` streams raw kernel bytes; JS writes into `Module.FS` at the path qemu-wasm's `module.js` argv expects (`/pack/Image`).
4. **emscripten boot** → `import('./assets/qemu/out.js')` (vendored from qemu-wasm submodule) instantiates the wasm module; xterm-pty bridge serves stdin/stdout.
5. **Serial output** → flows from QEMU UART through xterm-pty `slave` → `master` addon → xterm.js renders.

### Recommended Project Structure
```
bootroom/                                      ← (post-rename of norn-web)
├── Cargo.toml                                 ← workspace root
├── Makefile                                   ← `qemu-assets` target
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── .gitmodules                                ← pins qemu-wasm commit
├── qemu-wasm/                                 ← existing submodule
├── crates/
│   ├── bootroom-core/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                         ← Phase 1: empty, just proves workspace builds
│   └── bootroom/
│       ├── Cargo.toml
│       ├── build.rs                           ← validate assets/qemu/ exists
│       ├── src/
│       │   ├── main.rs                        ← clap dispatch
│       │   ├── cli.rs                         ← CliArgs + ServeArgs
│       │   ├── server.rs                      ← build_router, run
│       │   ├── headers.rs                     ← COOP/COEP layer
│       │   ├── assets.rs                      ← embedded/disk asset handler
│       │   ├── kernel_info.rs                 ← /api/kernel/info handler
│       │   └── kernel_stream.rs               ← /kernel handler
│       ├── web/
│       │   ├── index.html
│       │   ├── app.js                         ← ES module entry
│       │   ├── style.css
│       │   └── vendor/
│       │       ├── xterm.js                   ← pinned 5.3.0
│       │       ├── xterm.css
│       │       ├── xterm-pty.js               ← pinned 0.12.0
│       │       └── VERSIONS.md                ← pin record + rebuild steps
│       ├── assets/
│       │   └── qemu/                          ← committed; rebuilt via Makefile
│       │       ├── out.js
│       │       ├── qemu-system-riscv64.wasm
│       │       ├── qemu-system-riscv64.worker.js
│       │       ├── qemu-system-riscv64.data
│       │       ├── load.js
│       │       └── REBUILD.md                 ← `make qemu-assets` walkthrough
│       └── spikes/
│           ├── spike-a/
│           │   ├── SPIKE-A-RESULT.md          ← authoritative output
│           │   ├── kernel-variants/
│           │   └── (scratch html/js or sh)
│           └── spike-b/
│               ├── SPIKE-B-RESULT.md          ← authoritative output
│               ├── Cargo.toml (optional bin)
│               └── (chromiumoxide test code)
└── .planning/                                 ← preserved
```

### Pattern 1: COOP/COEP middleware applied once at the top of the router

**What:** Attach `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` to every response via a single tower layer. No per-route opt-in.

**When to use:** Always, for the entire Phase 1 router. There is no route that should skip these headers — even error responses must carry them, because the browser caches header decisions for navigation.

**Example:**
```rust
// crates/bootroom/src/headers.rs
// Source: tower-http docs https://docs.rs/tower-http/0.6/tower_http/set_header/
use axum::http::{header::HeaderName, HeaderValue};
use tower_http::set_header::SetResponseHeaderLayer;

pub fn coop_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    )
}

pub fn coep_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-embedder-policy"),
        HeaderValue::from_static("require-corp"),
    )
}

// in server.rs
let app = Router::new()
    .route("/", get(index))
    .route("/api/kernel/info", get(kernel_info))
    .route("/kernel", get(kernel_stream))
    .route("/assets/{*path}", get(asset))
    .with_state(state)
    .layer(coop_layer())
    .layer(coep_layer())
    .layer(TraceLayer::new_for_http());
```

`overriding` is correct (not `if_not_present`) because we want our policy to win regardless of whatever a handler tries to set.

### Pattern 2: Embedded-or-disk asset handler with `--assets-dir` override

**What:** A single asset-serving function consults `state.assets_dir`. If `Some(p)`, serve from disk under `<p>/web/<requested>` (or `<p>/assets/qemu/<requested>` for qemu paths). If `None`, walk the embedded `Dir` via `include_dir!`.

**When to use:** Pitfall #3 from the project research is "embedded-assets workflow impossible to iterate on." This pattern is the documented mitigation. Ship it on day one.

**Example:**
```rust
// crates/bootroom/src/assets.rs (sketch)
use include_dir::{include_dir, Dir};
static WEB: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/web");
static QEMU: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/qemu");

pub async fn serve_web(
    State(s): State<AppState>,
    Path(path): Path<String>,
) -> Response {
    if let Some(root) = &s.assets_dir {
        let disk = root.join("web").join(&path);
        if let Ok(bytes) = tokio::fs::read(&disk).await {
            return content_typed(bytes, &path);
        }
    }
    match WEB.get_file(&path) {
        Some(f) => content_typed(f.contents().to_vec(), &path),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_typed(bytes: Vec<u8>, path: &str) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.as_ref())], bytes).into_response()
}
```

`mime_guess` correctly returns `application/wasm` for `.wasm` (verified in their source) — this is mandatory for `WebAssembly.instantiateStreaming` to work without falling back to bytes mode.

### Pattern 3: SAB probe runs BEFORE any module script

**What:** An inline `<script>` (not `type="module"`) at the very top of `<body>` checks `crossOriginIsolated && typeof SharedArrayBuffer !== 'undefined'`. If either is false, un-`hidden` the banner element. Only after that runs do the module scripts that load xterm, xterm-pty, and qemu-wasm execute.

**When to use:** Always. If the probe runs after qemu-wasm tries to instantiate, the user sees a cryptic emscripten error before our friendly banner appears.

**Example:**
```html
<!-- index.html (head) -->
<script>
  // Inline (not module) so it runs synchronously, before deferred module scripts.
  (function() {
    var ok = (typeof SharedArrayBuffer !== 'undefined') && self.crossOriginIsolated === true;
    if (!ok) {
      // Defer DOM access slightly so the body is parsed.
      document.addEventListener('DOMContentLoaded', function() {
        var b = document.getElementById('iso-banner');
        if (b) b.removeAttribute('hidden');
      });
    }
  })();
</script>
```

### Pattern 4: xterm + xterm-pty wiring via openpty() (verbatim from reference)

**What:** Follow the qemu-wasm reference `index.html` exactly for the wiring. Don't reinvent.

**Reference:** `qemu-wasm/examples/riscv64/src/htdocs/index.html` (lines 9–35).

```javascript
// app.js (Phase 1, output-only PTY)
import './vendor/xterm.js';        // defines window.Terminal
import './vendor/xterm-pty.js';    // defines window.openpty
import './assets/qemu/module.js';  // sets Module.arguments
import initEmscriptenModule from './assets/qemu/out.js';

const xterm = new Terminal();
xterm.open(document.getElementById('terminal'));

// Phase 1: input deliberately no-op; Phase 2 wires through /ws
xterm.attachCustomKeyEventHandler(() => false);

const { master, slave } = openpty();
xterm.loadAddon(master);
Module.pty = slave;
Module['mainScriptUrlOrBlob'] = location.origin + '/assets/qemu/out.js';

(async () => {
  // Fetch kernel and inject into Module.FS BEFORE instantiation
  // (Spike A will validate the timing of this — Phase 1 uses the same
  //  pack-rebuild flow as the reference if Spike A is amber/red.)
  const instance = await initEmscriptenModule(Module);

  // Reference uses this poll patch — copy it verbatim to avoid PTY hangs.
  var oldPoll = Module['TTY'].stream_ops.poll;
  var pty = Module['pty'];
  Module['TTY'].stream_ops.poll = function(stream, timeout) {
    if (!pty.readable) {
      return (pty.readable ? 1 : 0) | (pty.writable ? 4 : 0);
    }
    return oldPoll.call(stream, timeout);
  };
})();
```

### Pattern 5: Status pill via emscripten lifecycle callbacks

**What:** Bind to `Module.onRuntimeInitialized`, `Module.onExit`, `Module.onAbort` (all emscripten-standard callbacks) and update a small state machine.

```javascript
// app.js (Phase 1 status logic)
const pill = document.getElementById('status');
function setPill(state) { pill.textContent = '● ' + state; pill.dataset.state = state; }
setPill('LOADING');

Module.onRuntimeInitialized = () => {
  if (self.crossOriginIsolated) setPill('RUNNING');
};
Module.onExit = (code) => setPill('HALTED');
Module.onAbort = (what) => setPill('HALTED');
```

CSS targets `[data-state="LOADING"]` etc. for the color treatment.

### Anti-Patterns to Avoid

- **`Cache-Control: no-store` on the wasm file.** Forces re-download on every reload; qemu-wasm is 10+ MB. Let the browser cache them.
- **Setting COOP/COEP on `/` only.** Subresources need them too — the layer must cover the whole router.
- **CDN-loaded xterm.js (as in the reference HTML).** Bootroom must work offline; vendor everything under `web/vendor/`.
- **`include_dir!` inside `build.rs`.** Macro must be at module scope; `build.rs` is for cargo to read printlns from.
- **Building qemu-wasm in `build.rs`.** Multi-minute docker step on every `cargo build`. Use `make qemu-assets` once, commit the output.
- **Reading kernel file synchronously in the request handler.** Use `tokio::fs::read` (already available with `tokio` feature `fs`).
- **Binding to `0.0.0.0` by default.** Security pitfall — anyone on the LAN can drive your kernel. Default to `127.0.0.1`; require explicit `--host 0.0.0.0` for non-local exposure (Phase 1 surfaces a `--host` flag per SERV-05; default value is `127.0.0.1`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Cross-origin isolation header policy | Custom middleware | `tower_http::set_header::SetResponseHeaderLayer` | Idiomatic; handles all response codes including errors. |
| MIME-type for embedded assets | Hardcoded match arm per extension | `mime_guess::from_path` | Covers obscure extensions (`.wasm`, `.map`, `.data`) and won't drift. |
| Static asset embedding | Manually `include_bytes!` each file | `include_dir!` | Walks directory, supports adding files without code changes. |
| CLI parsing for `--kernel`, `--port`, `--host`, `--assets-dir` | Hand-roll `std::env::args` | `clap` derive with `#[command(...)]` and `#[arg(...)]` | Generates `--help`, validates types, supports `BOOTROOM_*` env via `#[arg(env)]`. |
| SHA-256 for kernel info | DIY hash | `sha2::Sha256` + `hex::encode` | Constant-time-not-required here, but rolling your own is silly. |
| Terminal widget | Canvas DOM | xterm.js 5.3.0 | qemu-wasm's chardev is wired to it. Period. |
| PTY bridge between Worker and DOM | Worker postMessage protocol | xterm-pty 0.12.0 | qemu-wasm is compiled with `--js-library=…/xterm-pty/emscripten-pty.js`; the bridge is already linked in. |
| Headless browser driving for Spike B | Manual CDP frames | `chromiumoxide` 0.9 | Tokio-native, typed protocol. |
| Status text/badge | CSS art | Plain `<span>` with `data-state` attribute styled via CSS | A11y free; screen readers read the text directly. |

**Key insight:** Phase 1 is overwhelmingly a wiring exercise — every "what to build" question has an obvious crate answer. Resist the urge to hand-roll any of these; the only Phase-1-specific code should be: clap struct definitions, `AppState`, four route handlers, the COOP/COEP layer composition, a ~150-line `style.css`, a ~80-line `index.html`, and a ~120-line `app.js`. Anything beyond that is overscope.

## Common Pitfalls

### Pitfall 1: One missing COOP/COEP header silently breaks SAB
**What goes wrong:** A subresource (xterm.js, the wasm file, the .data file, the worker .js) is served without the headers; the browser sets `crossOriginIsolated = false` despite the main page being fine; `SharedArrayBuffer` is `undefined`; qemu-wasm aborts with a cryptic `pthread_create` error.
**Why it happens:** Per-route header attachment is the default mental model; one route gets forgotten.
**How to avoid:** Attach via top-level `Router::layer(coop_layer()).layer(coep_layer())` so it applies to **every** handler including `NOT_FOUND`. Confirm with a startup integration test that `curl -I http://localhost:8765/assets/qemu/qemu-system-riscv64.wasm` returns both headers.
**Warning signs:** Page loads but probe banner appears; or page loads and qemu-wasm hangs at boot with `pthread_create: function not implemented` in console.

### Pitfall 2: Wrong MIME for `.wasm` falls back to interpret mode
**What goes wrong:** Server returns `application/octet-stream` for the wasm file; `WebAssembly.instantiateStreaming` rejects with a MIME-type error; emscripten falls back to fetching as ArrayBuffer + non-streaming instantiate (works but is slower and noisy in console).
**Why it happens:** Hand-rolled MIME table missing `.wasm`, or `octet-stream` default.
**How to avoid:** Use `mime_guess::from_path` — it returns `application/wasm` for `.wasm`. Verify with `curl -I .../qemu-system-riscv64.wasm | grep Content-Type`.
**Warning signs:** Console warning `wasm streaming compile failed: MIME type 'application/octet-stream' is not 'application/wasm'`.

### Pitfall 3: `--assets-dir` only covers UI, leaving qemu artifacts stale
**What goes wrong:** Dev edits `web/index.html` on disk via `--assets-dir`, reloads, sees changes — but a stale qemu-wasm build in `assets/qemu/` continues to be embedded because that path isn't checked from disk. Confusing because "some" things hot-reload and "others" don't.
**Why it happens:** CONTEXT.md specifies `--assets-dir` mirrors the embedded layout (`<dir>/web/`, `<dir>/assets/qemu/`), but it's easy to implement only the `web/` half.
**How to avoid:** Implement disk-fallback for both subtrees. The reasonable default is `--assets-dir` overrides EVERYTHING; document this in `--help`.
**Warning signs:** "I rebuilt qemu-wasm and `make qemu-assets` and ran but the page is still booting the old guest" — they ran with `--assets-dir` pointing at a stale tree.

### Pitfall 4: SAB probe runs after emscripten init
**What goes wrong:** Probe is a `type="module"` script, deferred, so it runs after the qemu-wasm module instantiation begins; user sees emscripten throwing before the banner appears.
**Why it happens:** "All my scripts are modules" pattern.
**How to avoid:** The probe MUST be an inline non-module `<script>` block, placed before the module script imports.
**Warning signs:** Banner appears for a millisecond then is overwritten by an error overlay; or banner never appears because the main module errored first and tore down the page.

### Pitfall 5: Kernel served without `Accept-Ranges`/proper streaming
**What goes wrong:** The kernel file can be 10+ MB; serving it via `tokio::fs::read` then returning a `Vec<u8>` is fine for small kernels but loads the whole thing into RAM and won't support Range requests. Phase 1 doesn't need ranges, but if we hand-roll badly, we hit a wall later.
**Why it happens:** Quick `fs::read` then `Response::new(body)`.
**How to avoid:** Use `axum::body::Body::from_stream(ReaderStream::new(file))` from the start. Sets up Phase-2 incremental loading correctly. (tokio-util's `ReaderStream` is the standard adapter.)
**Warning signs:** Memory spike per request equal to kernel size.

### Pitfall 6: Embedding the kernel file itself
**What goes wrong:** Someone tries to `include_bytes!` the user's kernel into the binary because "we embed everything else." Defeats the entire purpose of `--kernel <path>`.
**Why it happens:** Pattern overreach.
**How to avoid:** The only embedded things are `web/` (UI assets — small) and `assets/qemu/` (the qemu-wasm artifacts — committed). The kernel is **always** read from `--kernel` at request time. Document explicitly in `assets.rs`.

### Pitfall 7: Spike B chooses chromiumoxide based on success but never tests the *exact* qemu-wasm boot flow
**What goes wrong:** Spike B confirms `crossOriginIsolated === true` and `typeof SharedArrayBuffer !== 'undefined'` but uses a trivial wasm fixture that doesn't exercise the pthread MTTCG path or the `.data` preload pack. Phase 4 later fails at the actual qemu-wasm boot.
**Why it happens:** "Get the spike done fast" trap.
**How to avoid:** Spike B's fixture MUST be a minimal RISC-V kernel boot via the actual `qemu-system-riscv64.wasm` artifact. Pass criterion is "≥1 byte of expected serial output" — not "module loaded."
**Warning signs:** Spike B is green in <1 hour. (Suspicious — re-run with the real qemu-wasm.)

### Pitfall 8: Spike A's "module-fs-write" verdict is fragile to qemu-wasm submodule bumps
**What goes wrong:** Spike A confirms `Module.FS.writeFile('/pack/Image', bytes)` works at a specific submodule revision; a later submodule bump changes the preload path or strips the FS export, and the Phase-2 reload silently breaks.
**Why it happens:** No version pin in the spike result.
**How to avoid:** SPIKE-A-RESULT.md MUST record the qemu-wasm submodule SHA it was validated against. Phase 2 plan must re-validate if the submodule SHA differs.

### Pitfall 9: Building qemu-wasm inside `build.rs`
**What goes wrong:** Every `cargo build` triggers a docker invocation; cold builds take ~10 minutes; CI runners may not have docker.
**Why it happens:** "Make it reproducible" instinct.
**How to avoid:** `make qemu-assets` is a separate target; output is committed to `crates/bootroom/assets/qemu/`. `build.rs` ONLY checks that the directory exists and is non-empty, failing with a clear "Run `make qemu-assets`" message if not.

### Pitfall 10: `127.0.0.1` binding fails on systems with strict loopback IPv6
**What goes wrong:** Some hosts resolve `localhost` to `::1` (IPv6); a server binding only `127.0.0.1` is unreachable; user sees "connection refused" with no obvious cause.
**Why it happens:** Default `localhost` resolution varies by system.
**How to avoid:** Print the URL with the literal IP (`http://127.0.0.1:8765/`), not `http://localhost:8765/`. The user can always navigate manually if their system resolves `localhost` differently. Document `--host ::1` for explicit IPv6 if requested.
**Warning signs:** "Page didn't open" — but only on systems that prefer IPv6.

## Code Examples

### Minimal axum app with COOP/COEP and embedded asset serving
```rust
// crates/bootroom/src/main.rs
// Source: axum 0.8 docs https://docs.rs/axum/0.8/axum/
// Source: tower-http 0.6 docs https://docs.rs/tower-http/0.6/tower_http/set_header/
use anyhow::Result;
use axum::{Router, routing::get};
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};
use axum::http::{HeaderName, HeaderValue};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand)]
enum Cmd {
    Serve(ServeArgs),
}

#[derive(clap::Args, Clone)]
struct ServeArgs {
    #[arg(long, value_name = "PATH")]
    kernel: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8765)]
    port: u16,
    #[arg(long, value_name = "PATH")]
    assets_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    kernel: PathBuf,
    assets_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "bootroom=info,tower_http=info".into())
    ).init();

    let Cli { cmd: Cmd::Serve(args) } = Cli::parse();
    let state = AppState { kernel: args.kernel.clone(), assets_dir: args.assets_dir };

    let coop = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    let coep = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-embedder-policy"),
        HeaderValue::from_static("require-corp"),
    );

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/api/kernel/info", get(routes::kernel_info))
        .route("/kernel", get(routes::kernel_stream))
        .route("/assets/{*path}", get(routes::asset))
        .with_state(Arc::new(state))
        .layer(coop)
        .layer(coep)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("Serving bootroom on http://{} (Ctrl-C to stop)", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

### Kernel-info handler (sha256 prefix + size + mtime)
```rust
// Source: sha2 docs https://docs.rs/sha2
use axum::{extract::State, Json};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::UNIX_EPOCH};
use tokio::io::AsyncReadExt;

#[derive(Serialize)]
pub struct KernelInfo {
    path: String,
    size: u64,
    mtime: i64,
    sha256_prefix: String,
}

pub async fn kernel_info(State(s): State<Arc<AppState>>) -> Result<Json<KernelInfo>, StatusCode> {
    let meta = tokio::fs::metadata(&s.kernel).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let size = meta.len();
    let mtime = meta.modified().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64).unwrap_or(0);

    let mut f = tokio::fs::File::open(&s.kernel).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let sha256_prefix = hex::encode(&digest[..6]); // 12 hex chars

    Ok(Json(KernelInfo {
        path: s.kernel.display().to_string(),
        size, mtime, sha256_prefix,
    }))
}
```

### Streaming kernel handler
```rust
// Source: tokio-util ReaderStream https://docs.rs/tokio-util/latest/tokio_util/io/struct.ReaderStream.html
use axum::body::Body;
use tokio_util::io::ReaderStream;

pub async fn kernel_stream(State(s): State<Arc<AppState>>) -> Result<Response, StatusCode> {
    let f = tokio::fs::File::open(&s.kernel).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let stream = ReaderStream::new(f);
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Body::from_stream(stream),
    ).into_response())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `xterm@5.x` (unscoped npm package) | `@xterm/xterm@6.x` (scoped) | xterm.js 6.0 released 2024 | DO NOT migrate in Phase 1 — xterm-pty 0.12.0 targets `xterm@5.3.0` API. Revisit only if xterm-pty itself migrates. |
| `--headless=old` (Chromium) | `--headless=new` | Chrome 112 (default), 2023 | Use `--headless=new` exclusively for Spike B; old headless lacks proper SAB support. |
| `notify-debouncer-mini` | `notify-debouncer-full` | 2024 | Not Phase 1 concern but locked for Phase 3. |
| Hand-rolled COOP/COEP middleware | `tower_http::set_header::SetResponseHeaderLayer` | tower-http 0.4+ | Use it; less code, can't forget edge cases. |
| `cargo build` → emit binary | `cargo-dist` → multi-platform release | Phase 6 only | Out of Phase 1 scope; mentioned for completeness. |

**Deprecated/outdated:**
- `clap` v3 builder-only API: replaced by v4 derive. We use 4.6.
- `axum` 0.7 handler signatures: replaced by 0.8's native async traits. Only copy from 0.8 docs.
- `wasm-bindgen-test` in Firefox headless: known-flaky for SAB; we use Chromium headless exclusively.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `mime_guess::from_path("foo.wasm")` returns `application/wasm` | Code Examples / Pitfall 2 | Have to add an explicit override per-extension. Low risk — verified in mime_guess's source historically. |
| A2 | `xterm-pty@0.12.0` is API-compatible with the qemu-wasm reference (which doesn't pin a version, just imports from unpkg) | Standard Stack | Vendored version may behave differently from CDN-resolved "latest." Validate by serving the reference's HTML verbatim before any wrapping. |
| A3 | qemu-wasm's `module.js` argv (`-kernel /pack/Image`) works unchanged with a kernel injected via `Module.FS.writeFile('/pack/Image', bytes)` before instantiate | Pattern 4 / Spike A | If wrong, Phase 1's initial-kernel-load path must use the file_packager.py rebuild flow (slower; requires node/emscripten in the build pipeline). Spike A resolves this. |
| A4 | `chromiumoxide` 0.9 is the right version (project STACK.md says 0.7) | Spike B | 0.9 may have breaking API changes; Spike B may want to pin 0.7 to match STACK.md. Verify by reading [chromiumoxide release notes](https://github.com/mattsse/chromiumoxide/releases). |
| A5 | Inline non-module `<script>` runs before `<script type="module">` blocks regardless of placement | Pattern 3 | If wrong, the SAB probe banner appears late. Per HTML spec, module scripts are deferred by default; inline classic scripts run immediately. Safe assumption. |
| A6 | The committed qemu-wasm artifacts (~10–30 MB) won't make `cargo build` painfully slow | Project Structure | If `cargo build` exceeds ~30s due to embed step, swap `include_dir` for `rust-embed` with compression. |
| A7 | `127.0.0.1:8765` is unlikely to collide with other dev tools | Decisions (CONTEXT.md) | Already user-locked; if collision occurs, `--port` flag handles it. |
| A8 | Local Chromium 148 will satisfy Spike B's headless SAB requirement | Environment Availability | Very low risk; Chrome 92+ supports headless SAB with proper headers. |

## Open Questions

1. **Should `build.rs` validate qemu-wasm asset SHA against `.gitmodules` submodule commit?**
   - What we know: CONTEXT.md says `build.rs` only emits a "missing assets" error.
   - What's unclear: Whether to also catch the "submodule was bumped but `make qemu-assets` not re-run" case.
   - Recommendation: Phase 1 ships the simpler check; add a SHA sanity check in Phase 2 once we have a documented submodule bump procedure.

2. **Does Spike B need a network NIC for the fixture kernel?**
   - What we know: The reference module.js uses `-nic none`.
   - What's unclear: Whether NORN's early-boot artifact (if used as the fixture) requires anything beyond serial.
   - Recommendation: Spike B starts with a hello-world fixture (no NIC); if NORN booting is also tested, document it separately.

3. **Should `bootroom serve` exit non-zero if the kernel file doesn't exist at startup?**
   - What we know: clap with `value_parser = clap::value_parser!(PathBuf)` accepts any path string; existence isn't checked.
   - What's unclear: UX preference — fail at startup (cleaner) vs fail at first `/kernel` request (more permissive for race conditions like an in-progress build).
   - Recommendation: Fail at startup with a clear `--kernel: file not found at <path>` message. If user wants to launch before build completes, they can `touch` the path. Listed as a planner decision.

4. **Does the `static Dir<'static>` from `include_dir!` capture absolute build-host paths in the binary?**
   - What we know: `include_dir!("$CARGO_MANIFEST_DIR/web")` resolves at compile time.
   - What's unclear: Whether the file paths inside the embedded `Dir` are relative (good) or absolute (bad — leaks build host paths).
   - Recommendation: Verify with `strings target/release/bootroom | grep -F '$HOME'` during Phase 1 testing. If absolute, switch to `Dir::new(path, files)` with relative paths or set `--remap-path-prefix`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `rustc` | Build the binary | ✓ | 1.90.0 (≥ 1.85 MSRV) | — |
| `cargo` | Build the binary | ✓ | 1.90.0 | — |
| `docker` | `make qemu-assets` (one-shot) | ✓ | 29.4.3 | — (only maintainers building qemu-wasm need this; users don't) |
| `make` | Run `make qemu-assets` | ✓ | GNU Make 4.4.1 | Shell script fallback if make is unavailable on a maintainer machine. |
| `node` | NOT REQUIRED at build/run | ✓ (22.22.2 if needed) | — | Only emscripten's `file_packager.py` (run inside docker) uses Python; bootroom itself has zero Node deps. |
| `chromium` | Spike B headless test only | ✓ | 148.0.7778.167 (≥ 112 needed for `--headless=new`) | If absent, install via pacman: `sudo pacman -S chromium`. |
| `git` (submodule) | Initial checkout of qemu-wasm | (assumed ✓) | — | — |
| `python3` | qemu-wasm's docker `file_packager.py` step | (in docker image) | — | Already inside docker container; host doesn't need Python. |

**Missing dependencies with no fallback:** None for Phase 1.

**Missing dependencies with fallback:** None — environment is fully provisioned.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `cargo test` (no external runner needed for Phase 1) |
| Config file | None — `Cargo.toml`'s `[dev-dependencies]` is the only config |
| Quick run command | `cargo test -p bootroom --lib` |
| Full suite command | `cargo test --workspace` |
| Headless browser smoke (Spike B + Phase-1 acceptance) | Manual `chromium --headless=new --disable-gpu http://127.0.0.1:8765` + DevTools protocol check, **or** the Spike B chromiumoxide bin if green |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DIST-01 | `cargo build` from clean checkout produces single binary | smoke | `cargo build --workspace --release` | ❌ Wave 0 |
| SERV-01 | `bootroom serve --kernel <path>` binds 127.0.0.1:<port> | integration | `cargo test -p bootroom --test serve_binds` | ❌ Wave 0 |
| SERV-02 | COOP and COEP headers on every response | integration | `cargo test -p bootroom --test coop_coep_headers` | ❌ Wave 0 |
| SERV-03 | Embedded qemu-wasm artifacts + UI served via include_dir | integration | `cargo test -p bootroom --test embedded_assets_served` | ❌ Wave 0 |
| SERV-04 | `--assets-dir <path>` overrides embedded assets | integration | `cargo test -p bootroom --test assets_dir_override` | ❌ Wave 0 |
| SERV-05 | `--port <N>` and `--host <addr>` flags work | integration | `cargo test -p bootroom --test port_host_flags` | ❌ Wave 0 |
| UI-01 | Page boots qemu-system-riscv64.wasm with supplied kernel | manual + headless smoke | `chromium --headless=new ... && grep "serial output" stdout` | ❌ Wave 0 (manual until Spike B green) |
| UI-05 | crossOriginIsolated probe banner appears when SAB unavailable | manual | Open page over file:// or a non-COOP server; observe banner | manual-only |
| UI-07 | Header shows kernel path, size, mtime | integration (API) + manual (DOM) | `cargo test -p bootroom --test kernel_info_endpoint` + visual | ❌ Wave 0 |
| CLI-03 | Common task = one command (no >1-line invocations) | meta-test (run-and-observe) | `bootroom serve --kernel /tmp/fixture` succeeds in one command | manual |

### Sampling Rate
- **Per task commit:** `cargo build --workspace && cargo test -p bootroom --lib` (~5s after warm cache)
- **Per wave merge:** `cargo test --workspace` + manual headed-browser smoke against a fixture kernel
- **Phase gate:** Full suite green + Spike A and Spike B verdicts recorded + manual browser smoke against the NORN kernel (or its fixture stand-in) before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/bootroom/tests/coop_coep_headers.rs` — covers SERV-02. Use `axum_test::TestServer` or `reqwest` against a spawned listener.
- [ ] `crates/bootroom/tests/serve_binds.rs` — covers SERV-01.
- [ ] `crates/bootroom/tests/embedded_assets_served.rs` — covers SERV-03; asserts `GET /assets/qemu/qemu-system-riscv64.wasm` returns 200 + correct MIME.
- [ ] `crates/bootroom/tests/assets_dir_override.rs` — covers SERV-04; writes a tempdir, asserts override served.
- [ ] `crates/bootroom/tests/port_host_flags.rs` — covers SERV-05; exercises `--port 0` ephemeral binding.
- [ ] `crates/bootroom/tests/kernel_info_endpoint.rs` — covers UI-07's API surface.
- [ ] `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` (the test code itself is scaffolded by Spike B)
- [ ] `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` (the test code itself is scaffolded by Spike A)
- [ ] No external test framework install required (`#[cfg(test)]` only).

## Project Constraints (from CLAUDE.md)

These directives are inherited from the workspace CLAUDE.md and the Phase 1 CLAUDE.md; the planner MUST honor them and the executor MUST NOT contradict them.

- **Tech stack: Rust only.** Single static binary, embeds static assets via `include_dir!`. No Node.js or Python required to *run* bootroom (Node is only used inside the docker container for the qemu-wasm one-shot build).
- **Config format: TOML.** Out of scope for Phase 1 but the schema lands in Phase 3.
- **Distribution: `cargo install` + binaries.** Phase 6 concern, but `Cargo.toml`'s `[package]` metadata must be MIT OR Apache-2.0 from day one.
- **Minimal command surface.** `bootroom serve --kernel <path>` is the only command Phase 1 ships.
- **MIT OR Apache-2.0 dual license.** Workspace root has both `LICENSE-MIT` and `LICENSE-APACHE`; each crate's `Cargo.toml` declares `license = "MIT OR Apache-2.0"`.
- **External-callable binary.** Once installed, bootroom must run anywhere — no assumption that the repo is checked out. All assets must be embedded (the `--assets-dir` flag is a dev affordance, not the runtime requirement).
- **GSD workflow enforcement.** All work goes through GSD commands; direct edits bypassing the workflow are forbidden unless explicitly authorized.
- **Wayland environment.** No X11-only tools. For headless browser testing, Chromium runs in `--headless=new` mode (no display server needed).
- **No npm-based frontend toolchain.** Vanilla JS + ES modules + vendored libs only. xterm.js and xterm-pty are pinned files in `web/vendor/`.
- **No emojis in code or written documentation** (global instruction).
- **No --break-system-packages**; use environments.
- **Don't pollute home directory with temp files**; use `/tmp/` (relevant for Spike scratch work).
- **Playwright is available system-wide** if Spike B falls back to it (~/.local/lib/node_modules/@playwright/mcp); but the goal is to NOT need it.

## Security Domain

> `security_enforcement` is unset in `.planning/config.json`, so default = enabled. Phase 1 has limited security surface (no auth, no untrusted input) but the floor still applies.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | bootroom is local-dev-only; no auth surface in v1. |
| V3 Session Management | no | No sessions in Phase 1. |
| V4 Access Control | partial | Default-bind to `127.0.0.1` prevents LAN exposure; `--host 0.0.0.0` requires explicit opt-in. |
| V5 Input Validation | partial | `--kernel <path>` should be validated (file exists, readable) at startup, not blindly trusted. clap derive handles type validation. |
| V6 Cryptography | partial | sha2 used for kernel-info digest (not security-critical — informational identity hash like git's short SHA). |
| V12 File and Resources | yes | `/kernel` and `/assets/*` serve files; path-traversal protection required for `--assets-dir` mode. |
| V14 Configuration | yes | COOP/COEP are configuration of cross-origin isolation; misconfiguration breaks the application. |

### Known Threat Patterns for {stack}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path traversal via `/assets/<../etc/passwd>` when `--assets-dir` is set | Information Disclosure | Canonicalize the requested path against the assets root; reject if not a descendant. The `axum::extract::Path<String>` does not auto-protect — manual check required. |
| Binding to `0.0.0.0` exposes kernel control to LAN | Elevation of Privilege | Default `127.0.0.1`; print warning if `--host` resolves to non-loopback. |
| Reading arbitrary files via `--kernel <path>` containing `~/.ssh/id_rsa` | Information Disclosure (low severity — user-controlled) | The kernel file IS readable bytes served over `/kernel`. If user passes a sensitive file, that's user error; document that `--kernel` is fully trusted user input. Don't try to sandbox. |
| Embedded CDN script in `index.html` | Tampering (supply chain) | Vendor xterm.js and xterm-pty; no `<script src="https://...">` allowed. CONTEXT.md already locks this. |
| Cross-origin isolation downgrade | Spoofing | COOP `same-origin` + COEP `require-corp` on every response; in-page probe surfaces failures. |

## Sources

### Primary (HIGH confidence)
- `qemu-wasm/README.md` (in-tree) — build flags, output files (`qemu-system-riscv64.{wasm,worker.js,data}`, `out.js`, `load.js`), COOP/COEP requirement.
- `qemu-wasm/examples/riscv64/src/htdocs/index.html` (in-tree) — verbatim xterm + xterm-pty wiring; `openpty()` / `slave.write` API; the TTY poll patch we must copy.
- `qemu-wasm/examples/riscv64/src/htdocs/module.js` (in-tree) — exact QEMU argv (`-machine virt`, `-kernel /pack/Image`, `-nic none`, `-m 512M`).
- `qemu-wasm/examples/riscv64/src/xterm-pty.conf` (in-tree) — the exact two-header set we replicate in axum.
- crates.io (queried 2026-05-17) — verified current versions of axum, tower-http, tower, include_dir, clap, notify, notify-debouncer-full, chromiumoxide, tokio, toml, serde, anyhow, tracing, mime_guess.
- npm registry (queried 2026-05-17) — verified xterm-pty 0.12.0, xterm 5.3.0 (latest under unscoped name).
- `.planning/research/STACK.md` (in-tree) — locked stack choices.
- `.planning/research/PITFALLS.md` (in-tree) — top hazards, especially #1 COOP/COEP, #2 headless SAB, #3 embedded-assets workflow.
- `.planning/research/ARCHITECTURE.md` (in-tree) — component split, browser-vs-server boundary.
- `.planning/phases/01-walking-skeleton/01-CONTEXT.md` — locked decisions (workspace layout, qemu-asset pipeline, spike sequencing, UI scope, port 8765).
- `.planning/phases/01-walking-skeleton/01-UI-SPEC.md` — component inventory, color palette, copywriting contract.

### Secondary (MEDIUM confidence)
- [axum 0.8 docs](https://docs.rs/axum/0.8/axum/) — handler signatures, router composition, layer ordering.
- [tower-http 0.6 set_header docs](https://docs.rs/tower-http/0.6/tower_http/set_header/) — `SetResponseHeaderLayer::overriding` semantics.
- [include_dir 0.7 docs](https://docs.rs/include_dir/0.7/include_dir/) — `Dir.get_file()` API, MSRV.
- [mime_guess 2 docs](https://docs.rs/mime_guess/2/mime_guess/) — `.wasm` returns `application/wasm`.
- [tokio-util ReaderStream](https://docs.rs/tokio-util/latest/tokio_util/io/struct.ReaderStream.html) — file-to-body streaming.
- [web.dev — COOP/COEP guide](https://web.dev/articles/coop-coep) — cross-origin isolation gating of SharedArrayBuffer.
- [xterm-pty README](https://github.com/mame/xterm-pty) — `openpty()` API; `slave.write(bytesOrString)`; `slave.onReadable` callback.
- [chromiumoxide README](https://github.com/mattsse/chromiumoxide) — tokio-runtime feature, headless launch pattern.

### Tertiary (LOW confidence — Spikes resolve)
- The exact API surface of `Module.FS.writeFile` for qemu-wasm's preload pack (Spike A).
- chromiumoxide 0.9 vs 0.7 behavioral differences (Spike B documents the version used).
- Whether `Module.onAbort` fires reliably on a fresh `--headless=new` Chromium when qemu-wasm hits a fatal error (Spike B observation).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every dep verified against crates.io 2026-05-17; all are stable, mature, and well-documented.
- Architecture: HIGH — the system architecture and component split derive directly from `.planning/research/ARCHITECTURE.md` and are validated by the in-tree qemu-wasm reference.
- COOP/COEP plumbing: HIGH — well-documented; the reference Apache config tells us exactly what headers and `SetResponseHeaderLayer` is the idiomatic axum mechanism.
- qemu-wasm wiring: HIGH for serving + initial boot (reference covers it verbatim); MEDIUM for runtime kernel substitution (Spike A's job).
- Headless CI viability: MEDIUM — chromiumoxide 0.9 is current, Chromium 148 is installed, headless SAB has worked since Chrome 92, but the actual qemu-wasm-end-to-end has not been documented in print (Spike B's job).
- Spike-result-derived decisions: LOW until spikes run — but that's the *purpose* of the spikes.

**Research date:** 2026-05-17
**Valid until:** 2026-06-17 (30 days — fast-moving area is chromiumoxide; verify versions before Phase 4).
