---
phase: 02-websocket-live-serial
plan: 05
subsystem: web-ui
tags: [html, css, ui-spec, static-affordances]
requires:
  - 01-08-SUMMARY.md  # Phase 1 web shell (index.html, style.css, palette)
provides:
  - dom-contract: btn-launch, btn-reset, btn-clear, btn-copy, status (pill)
  - css-hooks: .hdr-btns, .term-ctrls, .pill[data-state="IDLE"]
affects:
  - crates/bootroom/web/index.html
  - crates/bootroom/web/style.css
tech-stack:
  added: []           # static HTML/CSS only; no new deps
  patterns:
    - "Absolute-positioned overlay inside flex-grow terminal container"
    - "Palette-tokens-only styling (zero new hex values)"
    - "Real <button> elements with visible-text accessible names"
key-files:
  created: []
  modified:
    - crates/bootroom/web/index.html
    - crates/bootroom/web/style.css
decisions:
  - "Header button group placed AFTER kinfo and BEFORE pill so pill keeps margin-left:auto"
  - "Terminal controls overlay is a sibling of (not parent of) the xterm render layers; z-index 10 floats over without disturbing xterm DOM"
  - "Initial pill state moved from LOADING to IDLE (UI-SPEC); plan 06 wires the LOADING/RUNNING transitions"
  - "Hover state uses inset box-shadow (var(--fg-muted)) instead of a border so layout does not shift; accent reserved for focus ring only"
metrics:
  duration: ~10 minutes
  completed: 2026-05-18
---

# Phase 2 Plan 5: Static UI Affordances (LAUNCH/RESET/CLEAR/COPY + IDLE pill) Summary

LAUNCH, RESET, CLEAR, COPY buttons and the IDLE pill state are now in the DOM and styled per 02-UI-SPEC.md — pure visual surface, no behavior. Plan 06 wires the click handlers and pill state machine.

## What Changed

**index.html**
- Header strip now contains, in order: wordmark, kinfo dl, `.hdr-btns` div (LAUNCH + RESET), status pill. The pill keeps `margin-left: auto` so the button group sits flush against the right-anchored pill block.
- `#terminal` is now an opening/closing pair containing `.term-ctrls` as its first child (CLEAR + COPY). The controls are siblings of (not parents of) xterm's render layers; xterm appends its viewport/screen later in DOM order, and `z-index: 10` on the overlay keeps it visible above.
- Initial pill markup changed from `data-state="LOADING"` / inner text `LOADING` to `data-state="IDLE"` / inner text `IDLE`. The `<span aria-hidden="true">●</span>` glyph prefix is unchanged.

**style.css** (appended at end, after the `.xterm*` block)
- `.hdr-btns` flex container with `gap: var(--sm)`.
- Shared mono-button styling on `.hdr-btns button, .term-ctrls button`: `var(--surface)` background, `var(--fg)` text, no border, `var(--sm) var(--md)` padding, 12px / 600 / `text-transform: uppercase` / `letter-spacing: 0.05em`, `cursor: pointer`.
- `:hover` -> `box-shadow: inset 0 1px 0 0 var(--fg-muted)` (no border, no color change, no transition — terminal aesthetic stays flat and immediate per UI-SPEC).
- `:focus-visible` -> `outline: 1px solid var(--accent); outline-offset: 1px` (the only new use of `--accent` in this plan, and the keyboard-only accessible focus ring).
- `:disabled` -> `color: var(--fg-muted); cursor: not-allowed` (hook for plan 06's WS-reconnect-in-progress disabled state).
- `.term-ctrls` -> `position: absolute; top: var(--sm); right: var(--sm); z-index: 10` flex container with `gap: var(--sm)`. Anchors to the pre-existing `position: relative` on `#terminal`.
- `.pill[data-state="IDLE"]` -> `background: var(--fg-muted); color: var(--fg)`.

## Verification

- `grep` gates from both task verify blocks: PASS (all four button IDs present, both class containers present, initial pill state is IDLE, no `data-state="LOADING"` literal remains, focus-visible + inset-fg-muted hover hook present).
- `cargo build --workspace` -> green (asset embed compiles cleanly with `include_dir!`).
- Hex count in `style.css`: 10 -> 10 (zero new hex values introduced; palette purity preserved).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan's CSS verify gate asserted hex count `-eq 11`, but Phase 1 baseline is 10**
- **Found during:** Task 2 baseline check before edits.
- **Issue:** Plan 02-05-PLAN.md task 2 verify block contained `test "$HEX_COUNT" -eq 11`. The Phase 1 palette in `:root` actually contains 10 six-digit hex literals (verified via `grep -o "#[0-9a-fA-F]\{6\}" crates/bootroom/web/style.css | wc -l` against the pre-edit file), not 11. The 11-value count appears to have been computed including a non-existent palette token.
- **Substantive rule honored:** The plan's actual constraint (and 02-UI-SPEC.md `## Diff Summary`) is "ZERO new hex values introduced." That constraint is satisfied — the post-edit count is still 10, equal to baseline.
- **Fix:** Applied the correct semantic check (`-eq 10` post-edit) instead of failing on the stale gate.
- **Files modified:** none (gate is plan-document text; no code change needed).
- **Commit:** N/A (this is a verification-rule reconciliation, not a code fix).

No other deviations. No authentication gates.

## Known Stubs

The four new buttons are intentional stubs in this plan — they have no JS click handlers yet. This is the deliberate plan boundary documented in 02-05-PLAN.md `<objective>`:

> The buttons are dead until plan 06 wires their click handlers. This separation keeps the visual-only change reviewable in isolation.

Plan 02-06 will:
- Add `addEventListener('click', ...)` to all four buttons.
- Drive the pill state machine `IDLE -> LOADING -> RUNNING / HALTED`.
- Implement the actual LAUNCH (WS connect + xterm mount), RESET (reload), CLEAR (xterm `clear()`), and COPY (selection or full-buffer copy to clipboard) behaviors.

Not a true stub in the SUMMARY sense — by design, plan-boundary-deferred.

## DOM / CSS Contract Exposed to Plan 06

| DOM ID / selector | Element | Purpose for plan 06 |
|---|---|---|
| `btn-launch` | `<button>` | Bind click -> WS connect + xterm mount + state transition |
| `btn-reset` | `<button>` | Bind click -> page reload |
| `btn-clear` | `<button>` | Bind click -> xterm `clear()` |
| `btn-copy` | `<button>` | Bind click -> clipboard write of selection-or-full-buffer |
| `status` (existing) | `<span class="pill">` | Mutate `data-state` to drive pill color |
| `.hdr-btns button:disabled` | CSS hook | Apply via `.disabled = true` during WS reconnect |
| `.pill[data-state="IDLE"]` | CSS hook | Initial state before LAUNCH is clicked |

## Self-Check: PASSED

Files exist:
- `crates/bootroom/web/index.html` — FOUND (modified)
- `crates/bootroom/web/style.css` — FOUND (modified)
- `.planning/phases/02-websocket-live-serial/02-05-SUMMARY.md` — FOUND (this file)

Commits exist:
- `da2767b` feat(02-05): add LAUNCH/RESET/CLEAR/COPY markup + IDLE pill state — FOUND
- `2f29501` feat(02-05): add CSS hooks for header/terminal buttons and IDLE pill — FOUND
