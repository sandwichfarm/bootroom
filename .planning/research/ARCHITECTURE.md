# Architecture Research

**Domain:** Local Rust CLI dev-tool that serves a qemu-wasm guest, bridges UI events to its serial console, and runs the same scenarios headless in CI.
**Researched:** 2026-05-17
**Confidence:** HIGH for component split, data flow, and headless strategy (verified against qemu-wasm submodule's `examples/riscv64/src/htdocs/`). MEDIUM for some library choices flagged inline.

## TL;DR — the architecture in one paragraph

`bootroom` is a **single Rust binary** with two execution modes (`serve` and `run`) backed by a shared **library crate**. In both modes, the binary is the *only* process: it owns the TOML config, the kernel-file watcher, the static asset bundle, the HTTP server with COOP/COEP, and the WebSocket session. In `serve`, a real browser tab loads `qemu-system-riscv64.wasm` and a vanilla-JS shell that talks to the Rust process over one WebSocket. In `run`, the **exact same** static assets and WebSocket are driven by a headless Chromium spawned via `chromiumoxide`, executing the **same scenarios** with **the same client code path** — the Rust process can't tell the two apart. That single-codepath property is the load-bearing architectural decision: it's what guarantees "if it works locally, it works in CI."

## Standard Architecture

### System Overview

```
                                       bootroom (single Rust process)
   ┌──────────────────────────────────────────────────────────────────────────────────┐
   │                                                                                  │
   │   ┌────────────────────┐    ┌────────────────────┐    ┌─────────────────────┐    │
   │   │  CLI (clap)        │    │  Config (TOML)     │    │  Kernel Watcher     │    │
   │   │  serve | run | init│───▶│  + CLI overrides   │    │  (notify crate)     │    │
   │   └─────────┬──────────┘    └──────────┬─────────┘    └──────────┬──────────┘    │
   │             │                          │                         │ mtime/events  │
   │             ▼                          ▼                         ▼               │
   │   ┌──────────────────────────────────────────────────────────────────────┐       │
   │   │                       bootroom-core (library)                        │       │
   │   │  - Action / Group / Scenario model                                   │       │
   │   │  - WS protocol types (serde)                                         │       │
   │   │  - Scenario engine (state machine: step → expect → timeout)          │       │
   │   │  - Assertion language (substring / regex / sequence over serial)     │       │
   │   └─────────────────┬───────────────────────────────────┬────────────────┘       │
   │                     │                                   │                        │
   │                     ▼                                   ▼                        │
   │   ┌─────────────────────────────────┐    ┌─────────────────────────────────┐     │
   │   │   bootroom-server               │    │   bootroom-headless             │     │
   │   │   - axum HTTP + WS              │    │   - chromiumoxide driver        │     │
   │   │   - COOP/COEP headers           │    │   - spawns headless Chromium    │     │
   │   │   - serves embedded static UI   │    │   - points it at 127.0.0.1:RAND │     │
   │   │   - serves qemu-wasm artifacts  │    │   - injects "scenario=foo"      │     │
   │   │   - one WS session per tab      │    │   - exits 0/1 on assertion     │     │
   │   └────────────┬────────────────────┘    └─────────────┬───────────────────┘     │
   │                │  HTTP + WS (loopback only)            │ controls real Chromium  │
   │                ▼                                       ▼                         │
   └────────────────┼───────────────────────────────────────┼─────────────────────────┘
                    │                                       │
                    │            WebSocket (text + binary)  │
                    ▼                                       ▼
        ┌─────────────────────────────────┐    ┌─────────────────────────────────┐
        │  Real browser tab               │    │  Headless Chromium (same UI)    │
        │  ┌───────────────────────────┐  │    │  ┌───────────────────────────┐  │
        │  │ index.html + app.js       │  │    │  │ index.html + app.js       │  │
        │  │ ┌──────────┐ ┌──────────┐ │  │    │  │ (auto-runs scenario from  │  │
        │  │ │ buttons  │ │ xterm.js │ │  │    │  │  ?scenario=foo URL param) │  │
        │  │ │ panel    │ │ console  │ │  │    │  └───────────────────────────┘  │
        │  │ └────┬─────┘ └────▲─────┘ │  │    │                                 │
        │  │      │            │       │  │    │  ⤷ identical asset set as the   │
        │  │      ▼            │       │  │    │     interactive serve mode      │
        │  │  ┌────────────────┴────┐  │  │    └─────────────────────────────────┘
        │  │  │ xterm-pty bridge    │  │  │
        │  │  └──────────┬──────────┘  │  │
        │  │             ▼             │  │
        │  │   ┌───────────────────┐   │  │
        │  │   │ qemu-system-      │   │  │   The PTY slave is QEMU's serial(ttyS0).
        │  │   │ riscv64.wasm      │   │  │   Anything written into the master ends
        │  │   │ (Emscripten +     │   │  │   up in the guest's stdin. That is the
        │  │   │  Web Worker)      │   │  │   universal injection point.
        │  │   └───────────────────┘   │  │
        │  └───────────────────────────┘  │
        └─────────────────────────────────┘
```

The diagram's critical point: in **both** modes, the browser-side code is the **same files** served by the **same Rust HTTP server** over the **same WebSocket protocol**. Headless mode is "I am also a browser, I just happen to be driven by chromiumoxide."

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| `bootroom-core` (lib crate) | Pure types + scenario engine. No I/O. Action/Group/Scenario structs, WS message enums, assertion evaluator, scenario state machine. | Rust library, `serde`, `regex`. Re-exported by both binaries' code paths. |
| `bootroom-cli` (binary `bootroom`) | Argument parsing, subcommand dispatch, exit codes, stderr logging. Thin shim. | `clap` derive, `anyhow`, `tracing` + `tracing-subscriber`. |
| `bootroom-server` (module in main crate) | Axum app: GET `/` (UI), GET `/assets/*` (embedded), GET `/qemu/*` (qemu-wasm), GET `/api/config` (button projection), WS `/ws`. Applies COOP/COEP headers. Holds a `tokio::sync::broadcast` for "kernel reloaded" events. | `axum`, `tower-http`, `include_dir!`, `tokio`. |
| Kernel watcher | Watches `--kernel` path (or its parent dir if file is recreated atomically) and pushes a `KernelReloaded` event onto the broadcast channel. Debounces (`make` writes can come in bursts). | `notify` + `notify-debouncer-mini`. |
| WS session handler | One actor per connected tab. Pushes `Hello`, `ConfigUpdate`, `KernelReloaded`, `SerialOut` to client; receives `SerialIn`, `RunAction`, `RunScenario`, `Assertion` from client. | `axum::extract::ws`, `tokio::select!` loop. |
| Static asset bundle | The `index.html`, `app.js`, `xterm.js` vendor copy, and qemu-wasm artifacts (`qemu-system-riscv64.{js,wasm,worker.js,data}`, `load.js`). Compiled into the binary. | `include_dir!` for the UI; `build.rs` + `include_bytes!` for qemu-wasm artifacts (large files — verify `include_dir!` handles them, or stream from a known build dir). MEDIUM confidence on `include_dir!` for >100MB; fallback is `--qemu-wasm-dir` override. |
| Browser-side UI | Renders button panel from `/api/config`, mounts xterm, owns the xterm-pty bridge to `qemu-system-riscv64.wasm`, owns the WS connection. Vanilla JS, no build. | `index.html` + `app.js` (ES modules, no bundler). |
| xterm-pty bridge | Sits between xterm and the qemu-wasm Emscripten module. The slave end becomes QEMU's `ttyS0`. **This is the byte-level injection point for actions and the byte-level capture point for assertions.** | `xterm-pty` npm package, served as a vendored ES module — no npm at runtime. |
| `bootroom-headless` (module in main crate) | Spawns headless Chromium, navigates to `http://127.0.0.1:<port>/?scenario=foo&exit=1`, watches WS or stdout for assertion result, propagates exit code. | `chromiumoxide` (CDP). |

## Recommended Project Structure

```
bootroom/
├── Cargo.toml                 # workspace
├── crates/
│   ├── bootroom-core/         # pure-Rust library: types + scenario engine
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── config.rs      # TOML schema (Action, Group, Scenario)
│   │   │   ├── protocol.rs    # WS message enum, serde
│   │   │   ├── scenario.rs    # state machine: step → expect → timeout
│   │   │   └── assertion.rs   # substring / regex / sequence matchers
│   │   └── tests/             # pure logic tests, no I/O
│   └── bootroom/              # the binary
│       ├── build.rs           # optionally regenerate vendored qemu-wasm refs
│       ├── src/
│       │   ├── main.rs        # clap dispatch
│       │   ├── cli.rs         # subcommand definitions
│       │   ├── serve.rs       # axum app, WS, watcher wiring
│       │   ├── run.rs         # headless driver
│       │   ├── watcher.rs     # debounced kernel-path watcher
│       │   └── assets.rs      # include_dir! bundle + content-type
│       └── assets/            # baked into the binary via include_dir!
│           ├── ui/
│           │   ├── index.html
│           │   ├── app.js     # entry: WS, render buttons, mount xterm
│           │   ├── style.css
│           │   └── vendor/
│           │       ├── xterm.js
│           │       ├── xterm.css
│           │       └── xterm-pty.js
│           └── qemu/          # the qemu-wasm artifacts (verify size budget)
│               ├── out.js
│               ├── qemu-system-riscv64.wasm
│               ├── qemu-system-riscv64.worker.js
│               ├── qemu-system-riscv64.data
│               └── load.js
├── examples/
│   └── bootroom.toml          # canonical config example
└── README.md
```

### Structure Rationale

- **Workspace with one library + one binary, not three crates.** The original question floats a `bootroom-server` crate split. Resist it. Server, watcher, and headless driver all share the same `tokio` runtime and the same in-process channels (`broadcast`, `mpsc`); splitting them across crates buys nothing and complicates the `include_dir!` story (asset paths are relative to a crate root). Module boundaries inside `crates/bootroom/src/` give the same isolation without the friction. `bootroom-core` is split out because it has a different shape (pure logic, no `tokio`, no `axum`) and it lets scenario logic be unit-tested without a runtime.
- **`assets/` lives next to `main.rs`, not at the workspace root.** `include_dir!` paths are evaluated relative to `CARGO_MANIFEST_DIR`. Keep the macro and its inputs in the same crate.
- **`crates/` prefix (not flat).** Future-proofs adding a `bootroom-action-recorder` or similar without renaming.
- **The qemu-wasm submodule lives at the repo root**, not inside `crates/bootroom/assets/qemu/`. `build.rs` (or `make`) copies/symlinks the built artifacts into `assets/qemu/` so the submodule stays clean and the source-of-truth is the submodule. The submodule is *built* via its own Docker flow — `bootroom`'s build does not depend on emscripten being present; release CI builds qemu-wasm artifacts and they're vendored into the release binary.

## Architectural Patterns

### Pattern 1: One process, one WebSocket, two modes

**What:** Both `serve` and `run` start the same axum server on a port. `serve` prints the URL for the user; `run` reads the URL itself, hands it to headless Chromium, and waits for a `ScenarioResult` message on the WS to decide exit code.

**When to use:** This is the spine of `bootroom`. It's not optional.

**Trade-offs:**
- ✅ Eliminates a whole class of "works in dev, breaks in CI" bugs — there is no separate CI codepath.
- ✅ One mental model. Contributors don't ask "how does the headless thing work?"; they look at the same browser code.
- ✅ Bug-reproduction is trivial: a failing CI run is "open this URL with this scenario param" — every CI failure is reproducible by hand.
- ⚠️ Headless Chromium becomes a runtime dependency for `run` mode. Mitigated: `chromiumoxide` can use any system Chrome/Chromium and the binary fails loudly with an install hint if none is found. **Do not** try to bundle Chromium into the Rust binary.
- ⚠️ Boot overhead on `run`: ~1–2s to spawn Chromium. Acceptable for a CI tool; scenario runs themselves are dominated by qemu-wasm boot anyway.

**Example:**
```rust
// crates/bootroom/src/run.rs (sketch)
pub async fn run(kernel: PathBuf, scenario: String, cfg: PathBuf) -> Result<ExitCode> {
    let app = serve::build_app(kernel, cfg).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let result_rx = app.scenario_results.subscribe();
    tokio::spawn(axum::serve(listener, app.router));

    let url = format!("http://127.0.0.1:{port}/?scenario={scenario}&autoexit=1");
    let (browser, mut handler) = Browser::launch(/* headless */).await?;
    let _hb = tokio::spawn(async move { while let Some(_) = handler.next().await {} });
    let page = browser.new_page(&url).await?;

    let result = timeout(Duration::from_secs(120), result_rx.recv()).await??;
    page.close().await.ok();
    Ok(if result.passed { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
```

### Pattern 2: TOML → broadcast → WS → JSON projection

**What:** The TOML config is parsed once into `bootroom-core` types. On WS connect, the server projects it to a JSON envelope and pushes it as the first frame. On config-file change (in a future iteration) or kernel-reload, a new envelope is pushed. The browser is a dumb renderer — it never reads the TOML.

**When to use:** Whenever you'd be tempted to put config-format knowledge in the browser. Don't. Vanilla-JS-without-a-build is fragile; keep parsing on the Rust side.

**Trade-offs:**
- ✅ Schema changes only require Rust changes; the JSON wire format is what the browser sees, and it's narrower than the TOML.
- ✅ CLI flag overrides (`--action 'name=Foo,bytes=ls\n'`) are merged in Rust before projection — the browser sees a single, flat list.
- ⚠️ Need a serde-derived `WsMessage` enum with `#[serde(tag = "type")]` discriminator. Standard.

**Example:**
```rust
// crates/bootroom-core/src/protocol.rs
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMsg {
    Hello { version: String },
    ConfigUpdate { groups: Vec<Group>, scenarios: Vec<String> },
    KernelReloaded { sha256: String, mtime: i64 },
    SerialOut { data: String }, // base64 or utf8 — see Pattern 4
    ScenarioResult { name: String, passed: bool, log: String },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMsg {
    SerialIn { data: String },
    RunAction { id: String },
    RunScenario { name: String },
    Reload,
}
```

### Pattern 3: xterm-pty is the byte boundary

**What:** All serial-direction bytes go through the xterm-pty `master`/`slave` pair the example already uses. Action bytes are written to the master; serial output is captured by attaching a listener (or transparently teeing) on the master's output stream. The `Module.pty` plumbing in `module.js` stays untouched.

**When to use:** This is the *only* injection point that works. There is no QMP, no monitor socket, no `-serial mon:stdio` — qemu-wasm runs `-nographic` and the PTY *is* `ttyS0`.

**Trade-offs:**
- ✅ Architecture-agnostic — same mechanism works for x86_64/aarch64 if someone later relaxes the RISC-V-only constraint.
- ✅ Cooperatively zero-copy: the browser tees PTY output to xterm.js *and* to the WS in a single iteration.
- ⚠️ xterm-pty's wire is character-stream, not framed. Action "bytes" that include `\r\n`/escape sequences need byte-exact authoring in TOML — surface this in the config docs.
- ⚠️ Watch for TIOCSTI-like races if a button is mashed mid-boot. Initial implementation: actions are gated until a (configurable) "ready" pattern matches in the serial stream. NORN can set its prompt as the ready marker.

**Example (browser side):**
```js
// crates/bootroom/assets/ui/app.js (sketch)
const { master, slave } = openpty();
xterm.loadAddon(master);
Module.pty = slave;

// Tee PTY → WS for transcript / assertions
master.onWriteData(chunk => ws.send(JSON.stringify({type:"serial_out", data: chunk})));

ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.type === "serial_in") master.write(m.data); // bytes from action button
  if (m.type === "kernel_reloaded") location.reload();
  // ...
};
```

### Pattern 4: Scenarios run client-side, results stream server-side

**What:** A scenario is a list of `{action, then_expect, timeout}` steps. The **engine runs in the browser** (in `app.js`, sharing the JSON projection of the TOML), iterating actions and pattern-matching against the local PTY output stream. The browser emits structured `SerialOut` events to the server *and* a final `ScenarioResult { passed, log }`. The Rust process is a pass-through and an exit-code translator.

**Alternative considered:** Run the engine in Rust, with the browser as a dumb byte-pipe. **Rejected** because:
1. Round-tripping every PTY byte through WS for assertion matching doubles latency (qemu-wasm boot is already 5–10s).
2. It splits the codepath: a future "let users author scenarios in the browser" feature would have to be reimplemented server-side. Keep one engine.
3. `bootroom-core` types are still Rust — the engine is also implemented in Rust for `bootroom-core` tests, but the browser ships a small JS reimplementation. **MEDIUM confidence** this is the right call; an alternative is to compile `bootroom-core` to `wasm-bindgen` and run the same Rust engine in the browser. Defer that until the JS implementation feels painful.

**When to use:** Initial implementation. Re-evaluate after first three real NORN scenarios are written.

**Trade-offs:**
- ✅ Low latency, simple Rust side.
- ⚠️ JS scenario engine is duplicated logic. Mitigated by keeping it intentionally tiny: substring-match + regex + ordered-sequence only.

### Pattern 5: Headless mode is a real browser, not a JS engine emulator

**What:** Do not try to run qemu-wasm in Node/Deno. It needs SharedArrayBuffer, Web Workers, `WebAssembly.instantiateStreaming`, and `xterm-pty`'s emscripten library — the contract is "real browser." `chromiumoxide` driving headless Chromium is the path.

**When to use:** Always, for v1.

**Trade-offs:**
- ✅ Identical execution environment as local serve mode.
- ⚠️ Chromium must exist on the CI runner. GitHub Actions `ubuntu-latest` has it; `macos-latest` has it via Safari Tech Preview / brew chromium. Document the install line.
- ❌ **Anti-pattern:** "let's run native QEMU instead of qemu-wasm in headless." Rejected — the entire value proposition is "test against the thing your users run in their browser." Native-QEMU CI is a different tool.

## Data Flow

### Button-press flow (the headline path)

```
User clicks "boot kernel"
    │
    ▼
[app.js button handler]
    │  ws.send({type:"run_action", id:"boot"})
    ▼
[axum WS handler]
    │  resolves action from in-memory config; serde back out
    ▼  ws.send({type:"serial_in", data:"<bytes>"})
[app.js ws.onmessage]
    │
    ▼
[xterm-pty master.write(<bytes>)]
    │
    ▼  goes through Module.pty (slave end)
[qemu-system-riscv64.wasm ttyS0 read]
    │
    ▼
[NORN kernel sees stdin]
```

The corresponding **output** path runs continuously and in parallel:

```
[NORN kernel writes to ttyS0]
    │
    ▼
[Module.pty slave write]
    │
    ▼
[xterm-pty master output stream]
    │   tees to:
    ├──▶ xterm.js terminal (visible to user / headless screenshot)
    └──▶ ws.send({type:"serial_out", data:"<bytes>"})
              │
              ▼
        [axum WS] — buffers for `bootroom run`'s scenario-result wait,
                    fan-outs to other connected tabs (multi-tab spectating),
                    and writes to a transcript file if --transcript was passed.
```

### Kernel-reload flow

```
[user runs `make` in nostros/]
    │
    ▼
[notify watcher fires on kernel artifact]
    │   debounce 300ms
    ▼
[broadcast::send(KernelReloaded { sha, mtime })]
    │
    ▼
[every WS session forwards] → [app.js shows "fresh build available — click Launch"]
    │                            │
    │                            ▼
    │                       (user click) ws.send({type:"reload"})
    │                            │
    │                            ▼
    │                       app.js calls `location.reload()` so qemu-wasm
    │                       re-fetches /qemu/out.js / load.js / .data
    │
    └─ The Rust server, when handling GET /qemu/*, serves *the current
       contents of the kernel file* in place of the bundled rootfs.bin /
       Image — so a page reload picks up the new build for free.
```

Note the key trick: the kernel artifact is **not** baked into the `.data` packfile at build time. Either (a) we override the packfile entry at HTTP-serve time, or (b) we strip the kernel out of the packfile entirely and have `module.js` use `-kernel http://.../kernel` via a small Emscripten `--preload-file` indirection. **Verify with a spike before committing**; this is a place where qemu-wasm's exact `Module.preInit` / `FS` API surface drives the design.

### Headless run flow

```
[bootroom run --kernel … --scenario boot_smoke]
    │
    ▼
[same serve setup, ephemeral port, scenario_results channel]
    │
    ▼
[chromiumoxide launches headless Chromium]
    │  navigates: http://127.0.0.1:NNNN/?scenario=boot_smoke&autoexit=1
    ▼
[app.js sees ?scenario=…, waits for ConfigUpdate, runs the scenario]
    │
    ▼
[scenario engine in app.js asserts on serial_out stream]
    │
    ▼  ws.send({type:"scenario_result", passed:true/false, log:"…"})
[scenario_results channel fires]
    │
    ▼
[bootroom run returns ExitCode::SUCCESS / FAILURE]
```

## Suggested Build Order

The right ordering follows a single principle: **the unblocking critical path is "can a real browser tab boot the kernel and receive a byte we sent."** Everything else is decoration on top of that path.

1. **M0 — Walking skeleton: serve qemu-wasm + see boot.** axum server with COOP/COEP, embed the qemu-wasm artifacts, ship the riscv64 example's `index.html`/`module.js` essentially verbatim. Success = `bootroom serve --kernel <NORN Image>` boots NORN in Firefox/Chrome. **No buttons, no WS, no config.** This validates the headers, asset packaging, and that the submodule build flow is reproducible. Without this, every later step is theoretical.

2. **M1 — WebSocket + serial echo.** Add `/ws`, wire xterm-pty's master/slave to ship `SerialIn`/`SerialOut`. Success = typing into a server-side debug REPL (or `websocat`) writes into the guest's stdin. **This is the moment the architecture becomes real** — every later feature is "make this nicer."

3. **M2 — Config + buttons.** TOML schema in `bootroom-core`, `/api/config` projection, button rendering in `app.js`, `RunAction` → `SerialIn` plumbing. Success = clicking a "ls" button in the UI runs `ls` in the guest. CLI override flag (`--action`) lands here.

4. **M3 — Kernel watcher + reload.** `notify` watcher, broadcast channel, banner in UI, `Reload` flow. Success = `touch <kernel>` causes the UI to prompt for reload; clicking Launch picks up new bytes.

5. **M4 — Scenario engine + headless run.** `bootroom-core::scenario`, JS twin in `app.js`, `?scenario=…&autoexit=1` URL handling, `chromiumoxide` driver in `bootroom run`. Success = `bootroom run --kernel … --scenario boot_smoke; echo $?` is 0 or 1 and is wired up in NORN's CI.

6. **M5 — Distribution.** `cargo install bootroom` works; GitHub Actions builds release tarballs for `x86_64-linux` and `aarch64/x86_64-macos`. This is gated behind real M0–M4 work landing, because the binary contains the qemu-wasm artifacts and changes to those artifacts have to be release-tested.

**Order-rationale call-out:** M1 (WS+serial) before M2 (buttons) inverts the obvious "buttons are the user-facing feature, do them first." That ordering is wrong because the button is a thin trigger over a serial-write — building the trigger before the thing it triggers risks discovering at M2 that PTY injection has a quirk that invalidates the M1 button protocol. Always build the substrate first.

**De-risking spike (do before or during M0):** confirm that (a) the bundled `qemu-system-riscv64.{wasm,data,worker.js}` actually load when served from axum with COOP/COEP (the example uses Apache with the headers in `xterm-pty.conf`; we need to replicate exactly), and (b) we can supply the kernel as a runtime fetch rather than a build-time `--preload-file`. If (b) is intractable, the architecture pivots: we keep the kernel out of the binary entirely and let `bootroom` rebuild a tiny `kernel.data` packfile on each launch using emscripten's `file_packager.py` (which is just JS, runnable under Node, but introduces a Node runtime dependency we'd rather avoid). **MEDIUM confidence** that runtime kernel substitution is straightforward via Emscripten's `FS.writeFile` before `Module.run`; needs a half-day spike.

## External Boundary — "callable from outside the repo"

Three contracts together make the binary externally callable:

1. **Install contract.** `cargo install bootroom` *or* `curl -L https://github.com/.../bootroom-<triple>.tar.gz | tar xz -C ~/.local/bin`. The release tarball is a single static binary. No data files outside the binary except the optional `bootroom.toml`.

2. **Invocation contract.** `bootroom serve [--kernel PATH] [--config bootroom.toml] [--port 0] [--no-open]` and `bootroom run [--kernel PATH] [--config bootroom.toml] --scenario NAME [--timeout 120s]`. Both subcommands resolve `--kernel` relative to CWD if relative — so NORN's CI does `bootroom run --kernel build/Image --scenario boot_smoke` from its repo root and never touches `bootroom`'s source tree. The `init` subcommand writes a starter `bootroom.toml`.

3. **Headless dependency contract.** `bootroom run` requires Chromium/Chrome at runtime. Resolution order: `$BOOTROOM_BROWSER`, then `chromium`/`google-chrome-stable` on `$PATH`, then `chromiumoxide`'s default discovery. Fails fast with `error: bootroom run requires Chromium. Install with: <distro hint>`. **No autodownload** — surprising network access from a CLI tool in CI is a smell.

There is **no** "checkout `bootroom`'s repo to use it" path. The qemu-wasm submodule exists only for *building* `bootroom`; it is not consumed by downstream users.

## State Management

The server's runtime state is small and lives in one struct:

```
AppState {
  config: Arc<RwLock<ResolvedConfig>>,          // TOML + CLI overrides
  kernel: Arc<RwLock<KernelHandle>>,            // path, mtime, sha256
  serial_broadcast: tokio::sync::broadcast::Sender<SerialEvent>,
  reload_broadcast: tokio::sync::broadcast::Sender<ReloadEvent>,
  scenario_results: tokio::sync::broadcast::Sender<ScenarioResult>,
}
```

`broadcast` (not `mpsc`) so multiple WS clients can subscribe — multi-tab spectating in `serve`, and just one subscriber in `run`. The choice of `RwLock` over `Mutex` because config reads happen on every WS connect and are vastly more frequent than writes.

The browser's state is also minimal: the JSON `ConfigUpdate`, the running-scenario state (current step index, expected pattern, deadline timer), and the xterm-pty pair. No frameworks needed.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1 user, 1 tab | This is the design point. Everything works. |
| 1 user, 5 tabs (spectating) | `broadcast` handles fan-out. xterm-pty *instances* are per-tab, so each tab runs its own qemu-wasm. Acceptable; warn in UI that each tab is independent. |
| CI matrix (N parallel `bootroom run` invocations) | Each invocation is a separate process, separate port, separate Chromium. Bottleneck is wall-clock of qemu-wasm boot (5–10s) × scenario steps. Address by parallelizing scenarios across CI jobs, not by adding multi-tenancy to `bootroom`. |
| 100+ users | Not a goal. `bootroom` is a local dev/CI tool; loopback-only by default. If someone wants to host it for a team, they put it behind their own reverse proxy and accept that they're outside the support envelope. |

### Scaling priorities

1. **First bottleneck: qemu-wasm boot time.** Not solvable in `bootroom`'s architecture — it's qemu-wasm's problem. Mitigation: snapshot-based "skip to userspace" if/when qemu-wasm gains snapshotting (it has `examples/migration/`, worth a follow-up dig).
2. **Second bottleneck: assertion latency in long scenarios.** If client-side scenario engine becomes a hotspot, compile `bootroom-core` to wasm-bindgen and run the same Rust engine in the browser. (See Pattern 4.)

## Anti-Patterns

### Anti-Pattern 1: "Let's run native QEMU in CI, qemu-wasm only in browser"

**What people do:** Treat the browser as the dev experience and CI as a separate `qemu-system-riscv64 -nographic -kernel …` invocation glued to `expect(1)`.
**Why it's wrong:** Two execution environments, two sets of timing quirks, two failure modes. The whole reason this tool exists is "test exactly what the browser does." A green CI on native QEMU plus a broken browser session is the worst possible outcome.
**Do this instead:** Headless Chromium in CI. See Pattern 5.

### Anti-Pattern 2: Putting the scenario engine on the server with the browser as a dumb pipe

**What people do:** Server reads serial bytes over WS, runs regex assertions, decides pass/fail.
**Why it's wrong:** Doubles latency (every byte round-trips), couples the engine to the WS framing format, and forces a server reimplementation when "let users author scenarios in the browser" comes up later.
**Do this instead:** Engine in the browser, server is exit-code translator. See Pattern 4.

### Anti-Pattern 3: A second "build the JS" step

**What people do:** Reach for esbuild/vite/rollup the moment the JS gets non-trivial.
**Why it's wrong:** Project constraint is "no Node in the toolchain." Adding a build step regresses "can I install from a fresh CI runner with just `cargo install`?"
**Do this instead:** ES modules, vendor copies of `xterm.js` and `xterm-pty`, keep `app.js` under ~500 lines. If it grows past that, the right move is to split into multiple ES modules — still no bundler.

### Anti-Pattern 4: Watching the kernel file with a `tokio::time::interval` polling loop

**What people do:** Poll `fs::metadata(kernel).mtime` every 500ms.
**Why it's wrong:** Misses fast successive builds, wastes wakeups, doesn't handle atomic rename (the kernel file may be `unlinked` then created — inode changes; the inode-based poll sees nothing). 
**Do this instead:** `notify` crate with the debouncer, watching the **parent directory** with a filter for the kernel filename. Handles atomic-rename builds (`make` often writes to a temp file then renames).

### Anti-Pattern 5: Bundling Chromium

**What people do:** Try to ship a headless-Chromium binary inside `bootroom`'s release tarball.
**Why it's wrong:** 300MB+ binary, license complexity, OS-specific dynamic-loader pain.
**Do this instead:** Document Chromium as a runtime dependency for `run` mode only. CI runners already have it; local devs already have it (or trivially install it). See External Boundary.

## Integration Points

### External Services

None at runtime. (`bootroom` is offline-capable by design.)

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| CLI ↔ server module | Direct function calls | Single process; no IPC layer needed. |
| Server ↔ watcher | `tokio::sync::broadcast` | Watcher emits, every WS session subscribes. |
| Server ↔ WS clients | JSON over WS (text frames) | Single `WsMessage` enum, serde, `#[serde(tag = "type")]`. Binary frames reserved for future bulk transfers. |
| Browser UI ↔ qemu-wasm | `Module.pty` (xterm-pty slave) + `Module.arguments` (kernel cmdline) | This is the **only** runtime touchpoint with the qemu-wasm submodule. Keep it small. |
| Server ↔ kernel file | `notify` watcher (events) + `fs::File` (HTTP serve) | Lazy-load on each GET; do not hold the file open. |
| Server ↔ Chromium (run mode) | CDP via `chromiumoxide` over a local websocket the browser opens | Only used to navigate, surface page errors, and (optionally) capture a screenshot on assertion failure. |

## Open Questions to Resolve in M0 Spike

1. **Runtime kernel substitution.** Can we override a specific path in qemu-wasm's `FS` after the Emscripten module loads but before `Module.run`? If yes, no rebuild of `.data` is needed. If no, we need a strategy for streaming the kernel separately.
2. **COOP/COEP gotchas with axum.** The example uses Apache headers. Need to verify `tower-http::set_header` applies COOP/COEP to *all* responses including the `.data` packfile (it must — otherwise SharedArrayBuffer cross-origin isolation fails silently). 
3. **`include_dir!` size budget.** `qemu-system-riscv64.wasm` is in the 10s of MB; `.data` can be larger depending on the rootfs. Confirm the macro handles it; if compile times explode, fall back to `include_bytes!` per-file or an `--qemu-wasm-dir` runtime path with the bundled assets as a default.
4. **`xterm-pty` ES-module shape.** The example uses a CDN `unpkg.com` URL. Vendor the file locally; confirm it works as a static ES module without any module bundling tooling.

## Sources

- `qemu-wasm/README.md` (this repo's submodule), HIGH confidence — defines the artifact set, COOP/COEP requirement, and `Module.pty` injection point.
- `qemu-wasm/examples/riscv64/src/htdocs/{index.html,module.js}`, HIGH confidence — the working browser-side pattern we're extending.
- `qemu-wasm/examples/x86_64/src/xterm-pty.conf`, HIGH confidence — names the exact headers (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`).
- `.planning/PROJECT.md` (this repo), HIGH confidence — the constraints and command surface.
- Rust ecosystem standards for `axum` + `tower-http` COOP/COEP, MEDIUM confidence (training data + well-known patterns; verify in M0 spike).
- `chromiumoxide` crate as the headless driver, MEDIUM confidence — alternative is `fantoccini`; choice rests on whether we need CDP screenshot capture (chromiumoxide better) or WebDriver portability (fantoccini better). Recommendation stands at chromiumoxide.

---
*Architecture research for: local dev-tool + CI harness wrapping qemu-wasm*
*Researched: 2026-05-17*
