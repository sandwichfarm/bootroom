---
slug: console-viewer-fill-viewport
date: 2026-05-17
status: complete
---

# Summary: Console viewer fills viewport minus header (no scrollbars)

## What changed

1. **`crates/bootroom/web/style.css`**:
   - `html, body`: `height: 100vh; overflow: hidden;` (was `height: 100%`) — locks the page to the viewport so nothing can grow past it.
   - `body.dark`: `height: 100vh` (was `min-height: 100vh`) — no growth past viewport.
   - `.kinfo dd`: replaced `overflow-x: auto` with `overflow: hidden; text-overflow: ellipsis;` — long kernel paths now truncate with `…` instead of producing per-field scrollbars.
   - `#terminal`: added `overflow: hidden; position: relative; min-width: 0;` — the terminal slot itself never scrolls; xterm handles its own scrollback within the slot.
   - `.xterm, .xterm .xterm-viewport, .xterm .xterm-screen`: added `width: 100% !important; height: 100% !important;` — xterm's internal grid stretches to the container instead of clipping at its 80x24 default.

2. **`crates/bootroom/web/app.js`**:
   - Upgraded `fitTerminalToContainer` from a no-op placeholder to a real fit: reads xterm's renderer cell size and calls `xterm.resize(cols, rows)` to match the container. Falls back gracefully if the private renderer API moves between xterm.js versions.
   - Calls `requestAnimationFrame(fit)` once at module load (initial 80x24 grid is measurable) and again from `Module.onRuntimeInitialized` after the runtime has flushed its first frame.

## Verification

Headless Chromium against the live server at 1280x800 viewport:

| Metric | Value |
|--------|-------|
| Viewport | 1280 × 800 |
| Header height | 40.8 px |
| Terminal height | 759.2 px |
| Header + Terminal | 800 px (exact viewport fit) |
| `document.body.scrollHeight > clientHeight` | false |
| `document.body.scrollWidth > clientWidth` | false |
| `document.documentElement.scroll{H,W} > client{H,W}` | false |

NORN kernel boots and streams serial into a full-viewport xterm. No scrollbars at this viewport size.

## Commits

- `fix(quick/console-viewer): fill viewport, no scrollbars`
