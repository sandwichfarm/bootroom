---
phase: 01-walking-skeleton
plan: 03
subsystem: frontend
tags: [xterm, vendor, javascript, dependencies, npm, license, offline]

requires:
  - phase: 01-walking-skeleton
    provides: vendor directory convention established by CONTEXT.md no-CDN constraint
provides:
  - "Pinned xterm.js 5.3.0 UMD bundle at crates/bootroom/web/vendor/xterm.js"
  - "xterm.css 5.3.0 stylesheet at crates/bootroom/web/vendor/xterm.css"
  - "Pinned xterm-pty 0.12.0 UMD bundle at crates/bootroom/web/vendor/xterm-pty.js"
  - "VERSIONS.md with SHA-256 digests, source URLs, re-vendor procedure, and globals contract"
  - "LICENSES.md with verbatim MIT license texts for both packages"
affects:
  - 01-04 (include_dir embed of web/ directory needs these files in place)
  - 01-06 (UI shell loads xterm.js + xterm-pty.js as classic <script> tags)

tech-stack:
  added: [xterm.js@5.3.0, xterm-pty@0.12.0]
  patterns:
    - "Vendored web deps live under crates/bootroom/web/vendor/ with VERSIONS.md + LICENSES.md alongside"
    - "Pinned via SHA-256 in VERSIONS.md so re-vendor diffs are auditable"
    - "Loaded as classic UMD scripts (not ES modules) so globals attach via the UMD wrapper"

key-files:
  created:
    - crates/bootroom/web/vendor/xterm.js
    - crates/bootroom/web/vendor/xterm.css
    - crates/bootroom/web/vendor/xterm-pty.js
    - crates/bootroom/web/vendor/VERSIONS.md
    - crates/bootroom/web/vendor/LICENSES.md
  modified: []

key-decisions:
  - "Vendored xterm via the unscoped `xterm@5.3.0` npm package (NOT `@xterm/xterm@6.x`) because xterm-pty 0.12.0 targets the pre-6.x addon contract"
  - "Copied only the runtime UMD files (lib/xterm.js, css/xterm.css, index.js) — skipped sourcemaps, TypeScript sources, and the ESM variant to keep embed size minimal"
  - "Pinned via full SHA-256 in VERSIONS.md and recorded re-vendor procedure so future bumps stay reproducible"

patterns-established:
  - "VERSIONS.md as the single source of truth for vendored web dependency pins (file, package, version, source URL, tarball path, SHA-256)"
  - "LICENSES.md sits alongside VERSIONS.md and reproduces upstream LICENSE bytes verbatim — required because include_dir! will ship them inside the release binary"

requirements-completed: [SERV-03, UI-01]

duration: 3min
completed: 2026-05-17
---

# Phase 1 Plan 3: Vendor xterm.js and xterm-pty Summary

**Pinned xterm.js 5.3.0 (UMD) and xterm-pty 0.12.0 (UMD) into `crates/bootroom/web/vendor/` with SHA-256 hashes, source URLs, re-vendor procedure, and verbatim MIT license texts so the bootroom binary can ship offline-only with full attribution.**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-17T13:55:56Z
- **Completed:** 2026-05-17T13:58:19Z
- **Tasks:** 2
- **Files created:** 5

## Accomplishments

- Three vendored library files in place totalling 301,550 bytes (xterm.js 283,404 B, xterm.css 5,383 B, xterm-pty.js 12,763 B).
- `VERSIONS.md` (113 lines) documents version pins, full SHA-256 digests, source npm tarball URLs, the rationale for not bumping to `@xterm/xterm@6.x`, a reproducible re-vendor procedure parameterised by `VERSION_XTERM`/`VERSION_PTY`, and the globals contract (`window.Terminal`, `window.openpty`).
- `LICENSES.md` (58 lines) reproduces the upstream MIT license bytes for both packages verbatim from their respective tarballs.
- `cargo build --workspace` still passes after the additions.

## Task Commits

1. **Task 1: Download and vendor xterm.js 5.3.0 + xterm.css + xterm-pty 0.12.0** — `273fa89` (chore)
2. **Task 2: VERSIONS.md and LICENSES.md pin records** — `69a2ae9` (chore)

## Files Created/Modified

- `crates/bootroom/web/vendor/xterm.js` — UMD bundle from `xterm@5.3.0` tarball path `package/lib/xterm.js`; attaches `window.Terminal` when loaded as a classic script.
- `crates/bootroom/web/vendor/xterm.css` — stylesheet from `xterm@5.3.0` tarball path `package/css/xterm.css`.
- `crates/bootroom/web/vendor/xterm-pty.js` — UMD bundle from `xterm-pty@0.12.0` tarball path `package/index.js`; attaches `window.openpty` (and other named exports) onto the host global object via the UMD wrapper.
- `crates/bootroom/web/vendor/VERSIONS.md` — pin record, source URLs, SHA-256 digests, re-vendor procedure, globals contract.
- `crates/bootroom/web/vendor/LICENSES.md` — upstream MIT license texts verbatim.

## SHA-256 Digests (full)

```
f0aea0f75f48559013ae6643c2479dd737d26da42d5524e6d2b70915ae6523c7  xterm.js
832f3f2c603b43ad4351ff04970150cc7a873014276db126a6065c6dd81e4872  xterm.css
2e7cbffea02dad1f72637c564534d104a13f9eec306deb9cc34fffe1faa58947  xterm-pty.js
```

First 16 hex (also recorded in VERSIONS.md):
- xterm.js — `f0aea0f75f485590`
- xterm.css — `832f3f2c603b43ad`
- xterm-pty.js — `2e7cbffea02dad1f`

## Tarball Layouts (vs. plan assumptions)

The plan assumed:
- `xterm@5.3.0` tarball has `package/lib/xterm.js` and `package/css/xterm.css` — **confirmed**.
- `xterm-pty@0.12.0` tarball has `package/index.js` — **confirmed** (also ships `index.mjs`, `index.d.ts`, `emscripten-pty.js`; only `index.js` was copied because that is the classic-script UMD build).

No path remapping was needed.

## Globals Exposed (confirmed empirically)

- `xterm.js` opens with `!function(e,t){...}(self, (()=>(()=>{...})()))` — canonical UMD wrapper that attaches the inner module's exports to `self` (i.e. `window`) when neither AMD nor CommonJS is present. `Terminal` becomes `window.Terminal`.
- `xterm-pty.js` opens with `(function(g,f){if(typeof define=="function"&&define.amd){...}else if(typeof exports=="object"...){...}else{var m=f();for(var i in m) g[i]=m[i]}}(globalThis,...))` — UMD wrapper that iterates named exports and copies each one onto `globalThis`. The token `openpty` appears in the bundle source, confirming the named export survives. Loading the bundle as a classic `<script>` therefore produces `window.openpty`.

Plan 01-06 can rely on the documented order: load `xterm.js` first (defines `Terminal`), then `xterm-pty.js` (uses `Terminal` at module-init time).

## Vendoring Size Impact

Total bytes added to the repo: **301,550 bytes (~295 KiB)** across three files. Well below any embed-binary-bloat threshold; no compression needed.

## Decisions Made

- **Use the unscoped `xterm@5.3.0` npm package, not `@xterm/xterm@6.x`.** Plan 01-RESEARCH.md and CONTEXT.md `<specifics>` both locked this in. The scoped 6.x package has a different addon contract and would silently break `xterm-pty 0.12.0`'s `loadAddon` wiring.
- **Copy only the UMD build (`index.js`), not the ESM build (`index.mjs`).** The plan calls for classic `<script>` loading; the ESM bundle would not attach a global and would force an ES-module-import dance in the UI shell. Skipping `index.mjs` also halved the vendored payload.
- **Skip sourcemaps and TypeScript sources.** `*.js.map` and `*.d.ts` files in the tarballs are useful for upstream development but add bytes to every embedded release artifact without runtime benefit.

## Deviations from Plan

None - plan executed exactly as written.

The plan's stated size estimates for `xterm.js` ("700-900 KB") and `xterm-pty.js` ("50-200 KB") were over-estimates — actual sizes are 283 KB and 13 KB respectively. The plan's automated `<verify>` only checks `xterm.js > 100KB`, which passes. The `grep` smoke checks for `Terminal` and `openpty` both pass. The smaller-than-expected sizes are correct: the lib/xterm.js UMD is the minified production build, and xterm-pty's UMD is genuinely a small library. No action required.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- **Plan 01-04 (include_dir embed):** All three vendor files plus `VERSIONS.md` and `LICENSES.md` are in place under `crates/bootroom/web/vendor/`. The embed macro will pick them up automatically.
- **Plan 01-06 (UI shell):** Ready to author HTML with `<link rel="stylesheet" href="/vendor/xterm.css">`, `<script src="/vendor/xterm.js"></script>`, `<script src="/vendor/xterm-pty.js"></script>` in that order. After load, `window.Terminal` and `window.openpty` are guaranteed by the UMD wrappers documented in VERSIONS.md.

## Self-Check: PASSED

Verified all created files exist on disk:

- `crates/bootroom/web/vendor/xterm.js` — FOUND (283,404 B, SHA-256 starts `f0aea0f7…`)
- `crates/bootroom/web/vendor/xterm.css` — FOUND (5,383 B)
- `crates/bootroom/web/vendor/xterm-pty.js` — FOUND (12,763 B)
- `crates/bootroom/web/vendor/VERSIONS.md` — FOUND (113 lines)
- `crates/bootroom/web/vendor/LICENSES.md` — FOUND (58 lines)

Verified both task commits in git log:

- `273fa89` — FOUND (`chore(01-03): vendor xterm.js 5.3.0 and xterm-pty 0.12.0`)
- `69a2ae9` — FOUND (`chore(01-03): record vendor pins and licenses for xterm.js + xterm-pty`)

Verified `cargo build --workspace` passes (no compile regression from the additions; build is a no-op for an embed-only change since plan 01-04 hasn't yet wired `include_dir!` to read this directory).

---
*Phase: 01-walking-skeleton*
*Completed: 2026-05-17*
