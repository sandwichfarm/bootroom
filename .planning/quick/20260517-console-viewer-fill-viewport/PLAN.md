---
slug: console-viewer-fill-viewport
date: 2026-05-17
type: quick
---

# Quick: Console viewer fills viewport minus header (no scrollbars)

## Problem

The xterm.js terminal in `crates/bootroom/web/index.html` does not reliably fill the viewport. Body uses `min-height: 100vh` (so the page can grow taller than the viewport when the kernel-info header wraps onto multiple lines, producing a vertical scrollbar). Long kernel paths in `.kinfo dd` use `overflow-x: auto`, producing tiny per-cell horizontal scrollbars. xterm.js's default rendering can also overflow its container in either axis depending on cell-size vs container-size.

## Goal

After this change:
- Header (with kernel-info + status pill) sits at the top, naturally sized.
- xterm terminal fills 100% of the remaining viewport height.
- Neither horizontal nor vertical scrollbar appears on `<body>` or on `#terminal`.
- Long kernel paths truncate with ellipsis instead of producing per-`dd` scrollbars.
- xterm's internal viewport scrollbar (for scrollback buffer) remains hidden when content fits, visible only on intentional overflow (xterm-managed, not the page).

## Tasks

1. `crates/bootroom/web/style.css`:
   - `html, body`: change `height: 100%` → `height: 100vh; overflow: hidden;` (lock the page to viewport)
   - `body.dark`: replace `min-height: 100vh` with `height: 100vh` (no growth past viewport)
   - `.kinfo dd`: replace `overflow-x: auto; white-space: nowrap;` with `overflow: hidden; text-overflow: ellipsis; white-space: nowrap;` — long paths truncate, no scrollbar
   - `#terminal`: add `overflow: hidden; position: relative;` — the slot itself never scrolls; xterm manages its own scrollback
   - `.xterm, .xterm .xterm-viewport, .xterm .xterm-screen`: add `width: 100% !important; height: 100% !important;` so xterm's internal grid stretches to the container

2. `crates/bootroom/web/app.js`: hook the existing `fitTerminalToContainer` resize handler to actually re-cell xterm to the container size on `resize` and once on `onRuntimeInitialized` (compute rows/cols from `offsetHeight` / `offsetWidth` against `Terminal._core._renderService.dimensions.css.cell.{height,width}` if available; fallback to xterm's default if not). Best effort — if the API isn't there, falls back gracefully and the CSS still keeps scrollbars off.

3. Verify in the live Chromium harness: refresh the page, confirm no scrollbar on `<body>` (check `document.body.scrollHeight === document.body.clientHeight`) and no scrollbar on `#terminal`.

## Out of Scope

- Vendoring FitAddon (UI-checker noted this is Phase-2 work; we approximate via direct `Terminal.resize(cols, rows)`).
- Layout changes to the header (still wraps on narrow viewports, which is correct for narrow widths).
