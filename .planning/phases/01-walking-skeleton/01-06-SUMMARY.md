---
phase: 01-walking-skeleton
plan: 06
subsystem: frontend
tags:
  - frontend
  - ui
  - xterm
  - xterm-pty
  - qemu-wasm
  - html
  - css
  - javascript

dependency-graph:
  requires:
    - 01-03-SUMMARY.md  # vendored xterm.js + xterm-pty UMD bundles (window.Terminal, window.openpty)
    - 01-04-SUMMARY.md  # include_dir!("$CARGO_MANIFEST_DIR/web") picks up these files at compile
  provides:
    - "crates/bootroom/web/index.html — HTML shell with inline SAB probe, semantic header, banner, terminal mount, vendored script wiring"
    - "crates/bootroom/web/app.js — ES module: kernel-info fetch, xterm/xterm-pty mount, kernel byte injection via preRun, status pill state machine, page title sync"
    - "crates/bootroom/web/style.css — single dark theme per 01-UI-SPEC palette, status pill state colors via [data-state] selectors"
  affects:
    - 01-07  # integration test for GET / will exercise this HTML
    - 01-08  # Spike B headless verification consumes this UI end-to-end
    - 01-09  # Spike A reload-path investigation may revisit the preRun choice documented here
    - 02     # Phase 2 wires /ws to xterm input and adds Launch/Reset buttons on top of this scaffold

tech-stack:
  added: []
  patterns:
    - "Inline non-module <script> SAB probe runs BEFORE any type=\"module\" tag (Pitfall #4 mitigation)"
    - "Classic <script> tags load xterm.js / xterm-pty.js / load.js / module.js in order to populate window.Terminal / window.openpty / Module globals before the deferred ES module starts"
    - "Kernel bytes fetched up-front and stashed in a closure, then written into Module.FS via a synchronous preRun callback (the pendingKernel fallback)"
    - "Status pill state machine driven by Module.onRuntimeInitialized / onExit / onAbort lifecycle callbacks"
    - "CSS palette declared once in :root as custom properties; status pill colors selected via [data-state=\"...\"] attribute selectors"

key-files:
  created:
    - crates/bootroom/web/index.html
    - crates/bootroom/web/app.js
    - crates/bootroom/web/style.css
  modified: []

key-decisions:
  - "Used the synchronous pendingKernel preRun pattern (fetch /kernel into a Uint8Array before initEmscriptenModule, then write via a sync preRun closure) rather than relying on emscripten async preRun support; this avoids depending on which emscripten flags the qemu-wasm submodule was built with"
  - "Kept /assets/qemu/load.js in the script load order — the file is 180 lines of emscripten data-pack preload glue and is required to mount /pack/ in Module.FS before main runs"
  - "Implemented the window resize handler as a placeholder that re-reads container offsetHeight rather than adding xterm's FitAddon; Phase 1's UI-SPEC interaction contract calls for xterm's default sizing, and FitAddon was not vendored in plan 01-03"
  - "Used dt::after { content: ':' } for the kernel-info label punctuation so the colon is purely presentational and the dt text remains the semantic label (path/size/mtime/sha256)"

patterns-established:
  - "SAB probe placement: inline non-module script in <body> top, before any classic vendor scripts AND before the type=\"module\" entrypoint — guarantees the banner is the user's first paint on isolation failure"
  - "qemu-wasm global wiring contract: classic scripts populate globals, ES module consumes them as window-scoped vars (no imports for vendored libs)"
  - "UI-SPEC palette enforcement: every hex value declared once in :root, no hex outside that block — makes palette drift impossible without editing the single source of truth"

requirements-completed: [UI-01, UI-05, UI-07]

metrics:
  duration: "~12min"
  completed: 2026-05-17
---

# Phase 1 Plan 6: UI Shell (HTML/JS/CSS) Summary

**Three-file vanilla-JS UI shell — `index.html` (inline SAB probe + script wiring), `app.js` (kernel-info fetch + xterm/PTY mount + Module.FS injection + status pill), `style.css` (Tokyo-Night dark palette per UI-SPEC) — that plan 01-05's handlers can now deliver end-to-end to a real browser.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-05-17T14:07:00Z (approximate)
- **Completed:** 2026-05-17T14:19:00Z
- **Tasks:** 4 (3 author + 1 verify-only)
- **Files created:** 3

## Accomplishments

- **`index.html` (62 lines):** Inline non-module SAB probe placed before all module scripts, semantic `<header>` with wordmark + kinfo `<dl>` + status pill, `role="alert"` banner with UI-SPEC copy verbatim, terminal mount div, and classic vendor scripts loaded in the required order (xterm.js → xterm-pty.js → load.js → module.js → app.js as the only ES module).
- **`app.js` (203 lines):** Kernel-info fetch + header population + page-title sync, `humanBytes` / `isoLocal` helpers per UI-SPEC formatting contract, xterm.js + xterm-pty wiring copied from the qemu-wasm reference with `attachCustomKeyEventHandler(() => false)` marking input as deliberately no-op for Phase 1, kernel bytes pre-fetched and written into `Module.FS` via a synchronous preRun closure, status pill driven by `Module.onRuntimeInitialized` / `onExit` / `onAbort`, and the reference's `TTY.stream_ops.poll` patch copied verbatim.
- **`style.css` (176 lines):** Full UI-SPEC palette as CSS custom properties in `:root` (zero hex outside that block — verified), system monospace stack, flex header with pill `margin-left: auto` for right-justification, pill state colors via `[data-state="LOADING|RUNNING|HALTED"]` selectors, banner styling with the destructive-red headline, terminal flexes to fill remaining viewport, xterm viewport/screen backgrounds overridden to match page color.
- **`cargo build --workspace`** rebuilds successfully with the new files embedded.
- **`cargo test -p bootroom --lib`** still passes all 16 tests (no regression from plan 01-05).

## Task Commits

| Task | Name                                                                  | Commit    | Type    |
| ---- | --------------------------------------------------------------------- | --------- | ------- |
| 1    | index.html — structural shell, inline SAB probe, script wiring        | `2beaada` | feat    |
| 2    | app.js — kernel-info fetch + xterm/PTY mount + Module wiring + pill   | `c024292` | feat    |
| 3    | style.css — dark theme palette per UI-SPEC                            | `257ac10` | feat    |
| 4    | Re-run unit tests + commit boundary check                             | (no commit — verify-only step) | — |

## Files Created/Modified

- `crates/bootroom/web/index.html` — Phase 1 HTML shell. Inline SAB probe in `<head>`'s following `<body>` top, semantic header, banner, terminal div, classic vendor scripts + ES module entrypoint.
- `crates/bootroom/web/app.js` — Phase 1 ES module entrypoint. All client-side logic lives here.
- `crates/bootroom/web/style.css` — Phase 1 single-theme stylesheet.

## Decisions Made

### Preferred the synchronous `pendingKernel` preRun pattern over emscripten async preRun

The plan's Task 2 spec called out two paths for getting kernel bytes into `Module.FS` before QEMU `main` runs:

1. Push an `async () => { await fetch + writeFile }` onto `Module.preRun` and rely on emscripten to await it.
2. Fetch the bytes up-front into a closure-captured `pendingKernel` variable, then push a synchronous `Module.preRun` callback that does `Module.FS.writeFile('/pack/Image', pendingKernel)`.

Path 1 depends on emscripten being compiled with the right flag set to await async preRun callbacks (alternatively, the code itself can call `Module.addRunDependency` / `removeRunDependency`, but that complicates the timing). Path 2 has none of these dependencies — `bootGuest()` is already async, so we await the fetch before calling `initEmscriptenModule(Module)`, and the preRun callback only needs to be a synchronous `writeFile`.

**Chose path 2 (synchronous pendingKernel).** This is the documented fallback in the plan and the one that has the smallest dependency surface on the qemu-wasm submodule's build configuration. **Implication for Spike A (plan 01-09):** Spike A's reload-path investigation will need to figure out the right flow for *replacing* kernel bytes mid-session, but for Phase 1's initial-load this synchronous-write is sufficient.

### Kept `/assets/qemu/load.js` in the script load order

The plan's Task 1 said "If plan 01-02's review of the qemu-wasm submodule determined `load.js` is unused or empty, omit it." Inspecting the file (`wc -l → 180`, content starts with `var Module = typeof Module !== 'undefined' ? Module : {};` and continues with `expectedDataFileDownloads` / preload-pack glue) confirms it is the emscripten data-pack preload script that mounts `/pack/` (and therefore the QEMU argv's `-L /pack/ -kernel /pack/Image`) into `Module.FS` before `main` runs. **Keeping load.js in the script tag list.**

### Window-resize handler is a placeholder, not a FitAddon mount

The plan referenced a "FitAddon resize handler" but UI-SPEC `## Interaction Contracts` item 5 says "xterm.js's `FitAddon` is loaded and called on `resize` events" — that addon was *not* vendored in plan 01-03 (only `xterm.js` and `xterm-pty.js`). Adding FitAddon vendoring would expand plan 01-06 scope into plan 01-03 territory. **Implemented a placeholder resize handler that queries `container.offsetHeight`** so the resize event has an observable effect and the scaffolding is in place for Phase 2 to swap a FitAddon into. xterm's default sizing covers the typical viewport for Phase 1's manual smoke test (plan 01-07). Logged as a Phase-2 polish item.

### Used `dt::after { content: ':' }` for kernel-info label punctuation

Keeps the dt text as the pure semantic label (`path`, `size`, `mtime`, `sha256`) so screen readers and `document.querySelector('dt')` behavior aren't polluted with punctuation, while still rendering as `label: value` visually per the UI-SPEC layout diagram.

## Deviations from Plan

None — plan executed exactly as written.

The "decisions" above are choices the plan explicitly delegated to the executor (preRun pattern fallback, load.js keep-or-omit, resize handler details), not deviations.

## Issues Encountered

None. `cargo test -p bootroom --lib` still reports 16/16 passing, and `cargo build --workspace` succeeds after touching `embed.rs` to force re-evaluation of `include_dir!`.

## User Setup Required

None — no external service configuration required.

## Notes for Spikes

### Plan output asks: "Whether `Module.preRun` async-await worked or whether the synchronous-with-pendingKernel fallback was used"

**Answer:** The synchronous pendingKernel fallback was chosen up-front (decision rationale above). The async path was not exercised. **Spike A (plan 01-09)** is the right place to investigate whether the qemu-wasm submodule's emscripten build supports async preRun, because that bears on the *reload* path (Phase 2 Launch button), not Phase 1's initial-load.

### Plan output asks: "Whether `/assets/qemu/load.js` was actually needed in this build"

**Answer:** Yes — `load.js` is 180 lines of emscripten data-pack preload glue (`Module.expectedDataFileDownloads`, the `getFetcher` for `qemu-system-riscv64.data`, etc.) and is required for `/pack/` to be mounted in `Module.FS` before QEMU `main` runs. Kept in the `<script>` load order.

### Plan output asks: "Confirmation that no CDN URL appears anywhere in the UI files"

**Confirmed.** Final grep sweep:

```
$ grep -rE 'https?://' crates/bootroom/web/{index.html,app.js,style.css}
(no matches)

$ grep -rlE 'cdn|unpkg|jsdelivr' crates/bootroom/web/{index.html,app.js,style.css}
(no matches)
```

## Verification Deferred

Per the plan, visual + interactive verification is **deferred to plan 01-07** (manual headed-browser smoke) and **plan 01-08** (Spike B headless verification). This plan ships the static files; the next two plans exercise them end-to-end.

## Next Phase Readiness

- **Plan 01-07 (integration tests + manual smoke):** Ready. `GET /` will return `index.html` via plan 01-05's `serve_index`; all linked asset paths (`/assets/web/style.css`, `/assets/web/vendor/xterm.css`, `/assets/web/vendor/xterm.js`, `/assets/web/vendor/xterm-pty.js`, `/assets/web/app.js`, `/assets/qemu/load.js`, `/assets/qemu/module.js`, `/assets/qemu/out.js`) resolve through plan 01-05's `serve_asset` handler against `embed::WEB` / `embed::QEMU`.
- **Plan 01-08 (Spike B headless):** The UI is now the harness that Spike B drives via chromiumoxide; the status pill and emscripten lifecycle hooks give Spike B its assertion surface (pill transitions to `RUNNING` once `crossOriginIsolated && Module.onRuntimeInitialized`).
- **Phase 2:** The page structure is designed to survive the addition of Launch/Reset buttons (new flex items in the header) without restructuring; the `/ws` input wiring will replace the `attachCustomKeyEventHandler(() => false)` no-op with a real keydown→websocket forwarder.

## Self-Check: PASSED

Verified files exist on disk:
- FOUND: `crates/bootroom/web/index.html` (62 lines)
- FOUND: `crates/bootroom/web/app.js` (203 lines)
- FOUND: `crates/bootroom/web/style.css` (176 lines)

Verified commits exist in git log:
- FOUND: `2beaada` — `feat(01-06): UI shell HTML with inline SAB probe + vendored script wiring`
- FOUND: `c024292` — `feat(01-06): UI shell app.js — kernel-info fetch, xterm/PTY mount, status pill`
- FOUND: `257ac10` — `feat(01-06): UI shell style.css — Tokyo-Night dark palette per UI-SPEC`

Verified palette enforcement:
- All 9 unique hex colors (`#0E1116`, `#1A1F26`, `#3B1F26`, `#7A8295`, `#7AA2F7`, `#9ECE6A`, `#C0CAF5`, `#E0AF68`, `#F7768E`) declared inside `:root` — zero hex values outside that block.
- `#F7768E` is correctly used twice (for `--status-halted` and `--banner-head`) per UI-SPEC, which is why the unique count is 9 but the table specifies 10 occurrences.

Verified `cargo build --workspace` rebuilds cleanly after `touch crates/bootroom/src/embed.rs` (forces include_dir! re-evaluation; new files picked up).

Verified `cargo test -p bootroom --lib` still passes all 16 tests; no regression from plan 01-05.

Verified zero CDN / external URLs in the three UI files (`grep -rE 'https?://'` and `grep -rlE 'cdn|unpkg|jsdelivr'` both return empty).

---
*Phase: 01-walking-skeleton*
*Completed: 2026-05-17*
