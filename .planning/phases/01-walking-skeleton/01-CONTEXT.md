---
phase: 1
name: Walking Skeleton
gathered: 2026-05-17
status: Ready for planning
mode: discuss
---

# Phase 1: Walking Skeleton — Context

<domain>
## Phase Boundary

**Goal:** A user runs one command and watches their RISC-V kernel boot in a real browser tab with all the cross-origin-isolation plumbing correct.

**In scope (Phase 1):**
- `bootroom` binary crate + `bootroom-core` library crate (Rust workspace)
- `bootroom serve --kernel <path>` HTTP server bound to `127.0.0.1:8765` by default
- COOP/COEP headers on every response (HTML, JS, WASM, worker, data)
- Embedded qemu-wasm artifacts + vanilla-JS UI via `include_dir!`
- `--assets-dir <path>` runtime override for dev iteration
- `--port <N>` and `--host <addr>` overrides
- Minimum UI: kernel-info header (path, size, mtime), status pill, xterm.js mounted on xterm-pty `slave` (visible serial output, no keyboard input), `crossOriginIsolated` probe banner with fix hint
- Spike B (headless Chromium + SAB + qemu-wasm) — go/no-go verdict deciding Phase-4 driver
- Spike A (Module.FS runtime kernel substitution) — go/no-go verdict deciding Phase-2 reload path

**Out of scope (later phases):**
- `/ws` WebSocket endpoint and serial *input* (Phase 2)
- Auto-open browser on `serve` (SERV-06 / Phase 2)
- Launch / Reset buttons (Phase 2)
- TOML config, action buttons, watcher (Phase 3)
- Headless `run` mode (Phase 4 — but its viability is de-risked by Spike B here)

**Phase 1 requirements (from ROADMAP.md):** DIST-01, SERV-01, SERV-02, SERV-03, SERV-04, SERV-05, UI-01, UI-05, UI-07, CLI-03

</domain>

<decisions>
## Implementation Decisions

### Repo + crate layout

**Decision:** Rename now + workspace from day 1.

- Rename `~/Develop/norn-web` → `~/Develop/bootroom` at the start of Phase 1 (physical directory move). Update `qemu-wasm` submodule path is unchanged since it's relative.
- Set up Cargo workspace at the repo root with two crates:
  - `crates/bootroom-core/` — pure types, no I/O, no tokio. Holds:
    - Future home of `Action`, `Group`, `Scenario`, WS message enum (added in Phases 2-4).
    - For Phase 1: just the crate skeleton with `lib.rs` and `Cargo.toml`. May be empty; importing it from `bootroom` proves the workspace works.
  - `crates/bootroom/` — binary crate. Holds clap dispatch, axum app, COOP/COEP middleware, `include_dir!` embed of `web/` and qemu-wasm artifacts.
- Root `Cargo.toml` declares the workspace, shared `[workspace.package]` (license, repo, edition 2024, rust-version 1.85), `[workspace.dependencies]` for axum, tower-http, tokio, clap, include_dir, serde, anyhow, tracing.
- License files at workspace root: `LICENSE-MIT` + `LICENSE-APACHE`; each crate's `Cargo.toml` declares `license = "MIT OR Apache-2.0"`.

### qemu-wasm artifact pipeline

**Decision:** `make qemu-assets` Makefile target + commit built artifacts to git.

- Source path: `qemu-wasm/` submodule (already present).
- `Makefile` target `qemu-assets`: runs the qemu-wasm docker build, copies output (`qemu-system-riscv64.wasm`, `.worker.js`, `.data`, the bundled `xterm-pty/emscripten-pty.js`, and any required JS shims) into `crates/bootroom/assets/qemu/`.
- `crates/bootroom/assets/qemu/` is **committed** to git. Accept the ~10–30 MB repo-size cost in exchange for:
  - `cargo build` from a clean checkout works without docker.
  - Reproducible builds — the embedded artifact is exactly what was tested.
  - CI runners don't need docker.
- `make qemu-assets` is run by maintainers when bumping the qemu-wasm submodule (pinned commit in `.gitmodules`). A `Makefile` comment + a `crates/bootroom/assets/qemu/REBUILD.md` document the rebuild procedure.
- `build.rs` does **not** invoke docker. If `assets/qemu/` is missing or empty, `build.rs` emits a clear error: `qemu-wasm assets missing. Run \`make qemu-assets\` from the repo root.`
- Vendored web deps: `xterm.js` 5.3.0 and `xterm-pty` go into `crates/bootroom/web/vendor/` (separate from qemu assets, also committed). Pinned via `crates/bootroom/web/vendor/VERSIONS.md`.

### Spike sequencing and acceptance

**Decision:** Sequence Spike B → Spike A, both inside Phase 1.

**Order rationale:** Spike B (headless Chromium + SharedArrayBuffer + qemu-wasm) is the single biggest project risk per research. If red, switch to Playwright early before sinking time into chromiumoxide elsewhere. Spike A (runtime kernel substitution) only matters once a reload UX exists (Phase 2), but its result locks the reload code path early.

**Spike B — Headless Chromium + SAB end-to-end:**
- **Question:** Can `chromiumoxide` drive `--headless=new` Chromium against a bare axum + COOP/COEP server, observe `crossOriginIsolated === true`, and successfully boot a fixture RISC-V kernel via qemu-wasm with serial bytes flowing out?
- **Fixture:** any minimal RISC-V kernel that prints to ttyS0 (a 100-line "hello world" kernel; can use a NORN early-boot artifact if one exists).
- **Pass:** `crossOriginIsolated` is true, `qemu-system-riscv64.wasm` instantiates, ≥1 byte of expected serial output captured within 10s.
- **Output:** `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` with verdict `{green|amber|red}` and chosen tool `{chromiumoxide|playwright-subprocess|deferred}`. Includes reproduction command and observed `chrome --version`.
- **Red verdict:** Adopt Playwright (Node subprocess) — documented as the fallback in research. Does not block Phase 1 itself; gates Phase 4 driver choice.
- **Time box:** 1 day.

**Spike A — Runtime kernel substitution:**
- **Question:** Can the browser swap the kernel bytes in qemu-wasm's `Module.FS` (e.g. `Module.FS.writeFile('/pack/Image', bytes)`) and trigger a guest reboot without re-running emscripten's `file_packager.py`?
- **Fixture:** the same kernel from Spike B, plus a second variant with a distinguishable serial banner.
- **Pass:** swap occurs without page reload, second variant's banner observed in serial output.
- **Output:** `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` with verdict `{green|amber|red}` and chosen reload path `{module-fs-write|pack-rebuild|page-reload-only}`.
- **Red verdict:** Document `pack-rebuild` fallback (introduces a Node dep for users wanting in-place reload; alternatively, fall back to a full page reload). Does not block Phase 1; gates Phase 2 Launch button design.
- **Time box:** half-day.

Both spikes write their result MD files in addition to whatever scratch code lands in their `spikes/spike-X/` subdir. The result MD is the authoritative artifact downstream phases read.

### Phase 1 UI scope

**Decision:** Minimum spec — status pill + kernel-info header + xterm placeholder + `crossOriginIsolated` probe.

**Page renders (Phase 1):**
- **Kernel info header:** kernel path (the value of `--kernel`), file size, mtime, sha256 prefix (first 12 chars). Server exposes via `GET /api/kernel/info` returning JSON; page fetches and renders on load.
- **Status pill:** `Loading` → `Running` → `Halted`, driven by JS observing qemu-wasm Worker lifecycle events (no `/ws` yet — purely client-side, observable from emscripten's Module callbacks).
- **xterm.js terminal:** mounted on the xterm-pty `slave` (qemu-wasm's `ttyS0`). Serial *output* streams in and renders. Keyboard *input* is **not wired** in Phase 1 (xterm input handler is a no-op; Phase 2 wires through `/ws`). This is the "visible terminal, even if not yet interactive" success criterion 4 from ROADMAP.
- **`crossOriginIsolated` probe banner:** inline JS on page load checks `crossOriginIsolated` and `typeof SharedArrayBuffer`. If either is false, render a red banner with the fix hint: `Cross-origin isolation is not active. Bootroom requires COOP: same-origin + COEP: require-corp on every response. Hit Ctrl-Shift-I → Network to inspect headers.` Banner hidden otherwise.

**Page does NOT render (deferred to Phase 2+):**
- Launch / Reset buttons (Phase 2)
- Clear / Copy controls on the terminal (Phase 2)
- Any action buttons (Phase 3)
- "Fresher build available" banner (Phase 3)

**Server endpoints (Phase 1):**
- `GET /` — the HTML shell
- `GET /api/kernel/info` — JSON `{ path, size, mtime, sha256_prefix }`
- `GET /kernel` — raw kernel bytes (used by browser to fetch and load into `Module.FS` initially)
- `GET /assets/*` — embedded UI + qemu-wasm artifacts via `include_dir!` (or read from `--assets-dir` if set)

No `/ws`, no `/api/config`, no SSE in Phase 1.

### Default port + --no-open behavior

**Decision:** Default port `8765`. `--no-open` is the default (i.e., Phase 1 does NOT auto-open the browser).

- **Port 8765:** Uncommon — avoids collisions with `3000`/`8080`/`8000`. Mnemonic for `bootroom`. Bind defaults to `127.0.0.1:8765`. `--port 0` (ephemeral) is supported for tests.
- **Browser auto-open:** Implemented in Phase 2 (SERV-06 belongs to Phase 2 in ROADMAP). Phase 1 prints the URL to stdout: `Serving bootroom on http://127.0.0.1:8765 (Ctrl-C to stop)`. No CLI `--no-open` flag yet — it lands when `--open` becomes the default in Phase 2.

</decisions>

<canonical_refs>
## Canonical References

Every downstream agent (researcher, planner, executor) MUST read these before acting on Phase 1 work.

- `.planning/PROJECT.md` — project goals, constraints, key decisions
- `.planning/REQUIREMENTS.md` — full 59-requirement traceability; Phase 1 requirements = DIST-01, SERV-01..05, UI-01, UI-05, UI-07, CLI-03
- `.planning/ROADMAP.md` — phase boundaries and success criteria
- `.planning/STATE.md` — current decisions, open spikes, pitfalls
- `.planning/research/SUMMARY.md` — recommended stack, architecture decisions, top risks
- `.planning/research/STACK.md` — full stack rationale and alternatives
- `.planning/research/ARCHITECTURE.md` — one-process / one-WS / two-modes architecture
- `.planning/research/PITFALLS.md` — top hazards, especially #1 COOP/COEP, #2 headless SAB, #5 embedded-assets workflow
- `CLAUDE.md` (project root) — tech-stack table (axum 0.8, include_dir 0.7, clap 4.5, etc.), MSRV 1.85, things to avoid
- `qemu-wasm/README.md` — qemu-wasm build flags, output files, COOP/COEP requirement
- `qemu-wasm/examples/riscv64/src/htdocs/index.html` — reference xterm + xterm-pty wiring for `slave.write` / `slave.onReadable`

External (linked from research, fetch on demand):
- xterm-pty README: https://github.com/mame/xterm-pty
- axum 0.8 docs: https://docs.rs/axum/0.8
- tower-http SetResponseHeaderLayer: https://docs.rs/tower-http/0.6/tower_http/set_header/
- notify-debouncer-full: https://docs.rs/notify-debouncer-full (Phase 3, listed early so researchers can preview)

</canonical_refs>

<code_context>
## Existing Code Insights

Current repo state (pre-Phase-1):
- `~/Develop/norn-web/` (to be renamed `bootroom`) contains:
  - `qemu-wasm/` — git submodule, the QEMU-to-WASM build source. Pin commit in `.gitmodules`.
  - `.planning/` — GSD planning artifacts (preserved).
  - `CLAUDE.md` — project instructions and stack table.
  - No Cargo workspace yet. No `crates/`, no `src/`, no `web/`, no built qemu-wasm output. Phase 1 creates all of these.
- `qemu-wasm/examples/riscv64/src/htdocs/` shows the reference HTML + JS wiring for xterm-pty. **Copy this pattern**, don't reinvent the integration. Specifically: `index.html` shows how the `slave` (PTY) is created, how qemu-wasm's Module is configured (`--js-library=…/xterm-pty/emscripten-pty.js` was passed at build time), and how the xterm `Terminal` is mounted.
- qemu-wasm's CDN-loaded xterm.js in that example MUST be vendored — bootroom must work offline.

</code_context>

<specifics>
## Specific Ideas (from this discussion)

- **Workspace name:** root `Cargo.toml` workspace; binary crate is `bootroom` (publishable to crates.io), library is `bootroom-core`.
- **Asset layout in the binary:**
  - `crates/bootroom/web/` — HTML, JS, CSS for the UI shell (committed; small).
  - `crates/bootroom/web/vendor/` — xterm.js 5.3.0 + xterm-pty (committed).
  - `crates/bootroom/assets/qemu/` — built qemu-wasm artifacts (committed; large).
  - `include_dir!` in `main.rs` embeds both `web/` and `assets/qemu/` into the binary.
- **`--assets-dir <path>`:** when set, the server serves files from disk instead of the embedded copy. Affects only `web/` lookups; `assets/qemu/` stays embedded (the dev-iteration use case is UI, not qemu binaries). Actually — flag also supports `assets/qemu/` for completeness; the path is structured as `<assets-dir>/web/` and `<assets-dir>/assets/qemu/`, mirroring the embedded layout.
- **COOP/COEP middleware:** single `tower::Layer` via `SetResponseHeaderLayer::overriding`, attached to the top-level axum router. Headers attached to EVERY response — no per-route opt-in.
- **Spike code lives outside the main crate:** `crates/bootroom/spikes/spike-a/` and `…/spike-b/` are sibling directories (could be their own internal Cargo bin or just shell scripts that invoke the main `bootroom` binary). Keep them isolated so spike scaffolding doesn't pollute the main crate.
- **README at repo root** introduces `bootroom`, the one-line install (`cargo install bootroom` once Phase 6 publishes), the quickstart `bootroom serve --kernel ./Image`, and links to PROJECT.md / ROADMAP.md.

</specifics>

<deferred>
## Deferred Ideas (out of Phase 1 scope)

- **Auto-open browser:** SERV-06 / Phase 2.
- **Launch / Reset buttons:** UI-08, UI-09 / Phase 2.
- **xterm keyboard input → guest:** UI-03 / Phase 2 (needs `/ws`).
- **xterm Clear / Copy controls:** UI-04 / Phase 2.
- **Kernel-path watcher:** WCH-* / Phase 3.
- **TOML config + action buttons:** CFG-* / ACT-* / Phase 3.
- **Headless `run` driver implementation (not the Spike B verdict):** RUN-* / Phase 4.
- **`bootroom doctor`:** DOC-01 / Phase 5.
- **Crates.io publish + cargo-dist release:** DIST-02..07 / Phase 6.
- **Multi-arch (x86_64, AArch64) support:** v2.
- **Snapshot/save-state actions:** v2 (blocked on qemu-wasm support).

</deferred>

<spike_outputs>
## Spike Outputs (consumed by later phases)

Spike A and B each emit a result MD file in their respective directories. These files are the authoritative downstream input:

- `crates/bootroom/spikes/spike-a/SPIKE-A-RESULT.md` — locks Phase 2 reload path (Launch button reload mechanism).
- `crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md` — locks Phase 4 headless driver choice (`chromiumoxide` vs Playwright subprocess).

Result MD format (both spikes):
```markdown
---
spike: A|B
verdict: green|amber|red
chosen_path: <one of the documented options>
date: YYYY-MM-DD
---

## Question

## Method

## Observations

## Decision

## Follow-ups
```

</spike_outputs>
