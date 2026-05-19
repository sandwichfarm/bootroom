---
phase: "03"
status: advisory
overall_score: 21/24
pillars: { visual_hierarchy: 4, spacing: 4, color: 4, typography: 4, interaction: 3, brand: 2 }
---

# Phase 3 — UI Review

**Audited:** 2026-05-19
**Baseline:** `.planning/phases/03-config-buttons-watcher/03-UI-SPEC.md` (inherits Phase 1+2)
**Screenshots:** not captured (qemu-wasm assets not built; no live `bootroom serve` available — code-only audit)
**Stance:** advisory (non-blocking per autonomous workflow)

---

## Pillar Scores

| Pillar | Score | Key Finding |
|--------|-------|-------------|
| 1. Visual hierarchy | 4/4 | Banner priority resolver enforces ladder; one focal point per state; LAUNCH-as-accent is the loudest control when fresh-banner is up — exactly the spec intent |
| 2. Spacing / density | 4/4 | All spacing through `--xs/--sm/--md/--lg`; zero arbitrary px values introduced in Phase 3; `max-height: 25vh` is the one layout constraint and it is declared per spec |
| 3. Color / contrast | 4/4 | Zero new hex values; every Phase 3 surface paints through the 10 `:root` tokens; accent reservation amendment (banner LAUNCH only) is enforced — `#banner-launch` is the only new accent surface |
| 4. Typography | 4/4 | Exactly two weights (400/600), exactly three sizes (12/13/14px); no italic, no underline; uppercase + 0.05em letter-spacing applied consistently to Label-role surfaces |
| 5. Interaction affordance | 3/4 | Lock-aware short-circuits in `xterm.onData` / `xterm.onBinary` / action-click delegate are correct; `disabled` attribute is set (not just CSS) so AT skips them; **but** the fresh-banner dismiss is not durable across `resolveBanners()` re-runs — see BLOCKER below |
| 6. Brand consistency with Phase 1/2 | 2/4 | Phase 2 button styling reused unchanged via the shared selector list; aesthetic continuity is preserved; **but** the dismiss-persistence defect breaks the terse-no-chrome promise (a dismissed banner reappearing without a new event reads as a notification reflow, not the snap-in-snap-out aesthetic the spec calls for) |

**Overall: 21/24**

---

## Top 3 Priority Fixes

1. **BLOCKER — Fresh-banner dismiss is not durable across `resolveBanners()` re-runs.**
   **What:** `renderFreshBanner`'s dismiss handler (app.js:189) sets `freshBanner.hidden = true` directly but leaves `bannerState.freshKernel` populated. When any subsequent state mutation calls `resolveBanners()` (e.g., a `ConfigUpdate` frame arrives, or `ConfigInvalid` clears), the resolver computes `freshActive = bannerState.freshKernel !== null` → true → un-hides the dismissed banner. The spec (line 362) says dismiss is *per-event* — i.e., suppressed only until the next `KernelChanged` event — not "suppressed until any unrelated state change."
   **User impact:** Operator dismisses the stale-build banner; ten seconds later they save `bootroom.toml`; the `ConfigUpdate` handler calls `resolveBanners()`; the dismissed banner snaps back. The UI feels stuck. In Phase 4 (scenario engine) where state mutations are frequent, dismiss will feel broken.
   **Fix:** Add a `freshKernelDismissed: false` flag to `bannerState`. Dismiss handler sets it `true`. `KernelChanged` handler resets it `false`. `resolveBanners()` reads it: `const freshActive = !isoActive && !configActive && bannerState.freshKernel !== null && !bannerState.freshKernelDismissed`. Three-line change in app.js.

2. **WARNING — `resolveBanners()` does not re-render banner content before un-hiding.**
   **What:** Each banner's content is rendered by its handler (`renderConfigBanner`, `renderFreshBanner`) — but `resolveBanners()` only toggles `hidden`. If state mutates between renders (e.g., `KernelChanged{ok:true}` arrives, then `KernelChanged{ok:false}` arrives while iso-banner is force-hiding fresh), the banner could end up showing stale success content when un-hidden. Currently the handlers always call `renderXxxBanner()` *before* `resolveBanners()`, so this hazard is latent — but it is one careless edit away from a real bug.
   **User impact:** Subtle stale-content risk; not currently observable but fragile.
   **Fix:** Make `resolveBanners()` call `renderFreshBanner()` and `renderConfigBanner()` itself before toggling `hidden`. Centralizes the contract.

3. **WARNING — `#actions-panel` empty-but-visible flash on initial load.**
   **What:** `index.html` line 49 declares `<div id="actions-panel" hidden>` but the `[hidden]` attribute is removed only after `initialConfigLoad()` resolves — which itself only runs after the WS `Hello` frame arrives (app.js:578). If the WS handshake stalls (slow network, throttled CDN), the panel stays hidden — that part is fine. But once Hello arrives and `/api/config` returns a config with `actions.length === 0`, `renderActionButtons` correctly sets `actionsPanel.hidden = actions.length === 0` (line 271). The hazard: if a config arrives with actions, then a `ConfigUpdate` drops them all to zero, the panel is un-hidden by the first render and re-hidden by the second; mid-frame the user may see a 25vh-tall empty surface flash on slow paint. No animation/transition policy in the spec means this should snap, but `replaceChildren([])` + `hidden=true` is two paint cycles in some browsers.
   **User impact:** Brief visual flicker on TOML edits that remove all actions. Aesthetic friction in a tool whose value prop is terseness.
   **Fix:** Set `actionsPanel.hidden = actions.length === 0` *before* `replaceChildren(...)`. Single-line reorder in `renderActionButtons` (app.js:270-271 swap order).

---

## Detailed Findings

### Pillar 1: Visual hierarchy (4/4)

- **Banner priority ladder implemented as specified.** `resolveBanners()` (app.js:137) correctly derives `isoActive` / `configActive` / `freshActive` in priority order and toggles `hidden` on the two Phase-3 banners. The iso check reads `isoBanner.hasAttribute('hidden')` which matches the spec contract that the iso banner is owned by the inline SAB probe and the WR-01 vendor-load failsafe.
- **One focal point per state.** In State B (fresh-banner active), the inline `LAUNCH` button is the only accent-colored interactive control on the page — matching the spec's intent (UI-SPEC line 110): "the single visually-loudest control on the page when the banner is shown."
- **Action group label is correctly secondary.** `--fg-muted` on `--surface` for group labels keeps them subordinate to action buttons (which use `--fg`). The inline-flex group layout (`label [BTN] [BTN]`) reads as a heading-with-children, not a competing column.
- **No competing accents.** Verified `--accent` usage: wordmark, `dd.accent` (sha256 prefix), focus ring outline, `#banner-launch` — exactly 4 surfaces, matching the spec's reservation policy.

### Pillar 2: Spacing / density (4/4)

- **Zero arbitrary px values introduced in Phase 3.** Every spacing declaration uses `var(--xs)` / `var(--sm)` / `var(--md)` / `var(--lg)`. Audited the diff with `grep -nE "padding|gap|margin" crates/bootroom/web/style.css`: 18 spacing declarations, all token-driven (except `margin: 0` resets and `margin-left: auto` for pill alignment, both intentional).
- **`max-height: 25vh` is the one layout constraint, declared exactly as the spec calls for** (style.css:255). Not a token; not abused.
- **`#banner-dismiss padding: var(--xs) var(--sm)`** — tighter than the standard `var(--sm) var(--md)` button padding, justified by the 1-char `×` glyph. Spec line 289 explicitly approves.
- **`.action-group gap: var(--sm)`** between label and buttons; `#actions-panel gap: var(--md)` between groups. Two-level rhythm reads as terse-but-grouped, matching the aesthetic anchor.

### Pillar 3: Color / contrast (4/4)

- **Zero new hex values.** `grep -nE "#[0-9a-fA-F]" crates/bootroom/web/style.css | grep -v :root` returns only the `:root` declarations. The Phase 3 amendment "exactly one new accent use" is enforced: `#banner-launch { color: var(--accent); }` (style.css:289-291) is the only new accent surface.
- **`BUSY` pill = IDLE pill visually** — `.pill[data-state="BUSY"]` (style.css:320-323) uses `background: var(--fg-muted); color: var(--fg);` — byte-identical to `.pill[data-state="IDLE"]` (style.css:239-242). Spec line 99 calls for this; the text discriminates the state, not the color.
- **Config-invalid banner reuses iso-banner palette** — `--banner-bg` + `--banner-head` (style.css:298-310) mirror the iso-banner box exactly. Mutual exclusion via the resolver ensures the two destructive banners never stack.
- **Fresh-banner uses `--surface` (neutral)** — informational, non-intrusive, matches spec line 119.
- **Contrast cannot be measured from code alone** (no live render). The CSS pair `--accent` (#7AA2F7) on `--surface` (#1A1F26) — expected ≈7:1, should pass AA easily. The `--fg-muted` (#7A8295) on `--surface` (#1A1F26) for the dismiss `×` is the borderline case the spec already flagged; without a live ren render, I cannot confirm AA pass. Recommend executor runs WCAG checker against rendered output before sign-off.

### Pillar 4: Typography (4/4)

- **Three sizes only: 12px, 13px, 14px.** Verified with `grep -nE "font-size" crates/bootroom/web/style.css | sort -u`.
- **Two weights only: 400, 600.** Verified.
- **Uppercase + 0.05em letter-spacing applied consistently** to pill (style.css:125-126), shared button rule (style.css:205-206), and action-group label (style.css:270-271). Three uppercase-Label surfaces, one rule each — no drift.
- **No italic, no underline, no font-variant tricks.** Strict weight policy holds.
- **`.action-group-label` correctly inherits Label-role typography** without introducing a new role — matches spec line 64.

### Pillar 5: Interaction affordance (3/4)

- **Lock-aware short-circuits implemented in all three caller sites:**
  - `xterm.onData` app.js:412 (`if (funnel.locked === true) return;`)
  - `xterm.onBinary` app.js:419 (same)
  - Action-click delegate app.js:284 (same)
  Funnel itself is lock-agnostic so server-initiated `SerialIn` flows during the lock — matching spec line 339.
- **`disabled` attribute (not just CSS) is set on action buttons during lock** — `renderActionButtons` line 241 + `setLockObserver` callback lines 893-901. AT skips disabled buttons during Tab traversal; matches spec accessibility floor line 397.
- **Idempotent lock API** — `funnel.lockInput()` and `unlockInput()` (funnel.js:123-138) early-return if already in the requested state; observer is not re-fired. Matches spec line 364.
- **Focus ring on action buttons** — `.action-btn:focus-visible` inherits the shared rule (style.css:216-221) with 1px `--accent` outline and 1px offset.
- **BLOCKER — Fresh-banner dismiss is not durable.** See Top Priority Fix #1. The current implementation hides the banner on dismiss but leaves the state populated; the next `resolveBanners()` re-shows it. This is the one real interaction defect found.
- **WARNING — `resolveBanners()` does not re-render content.** See Top Priority Fix #2. Latent staleness hazard.

### Pillar 6: Brand consistency with Phase 1/2 baseline (2/4)

- **Phase 2 button styling reused unchanged.** The shared selector `.hdr-btns button, .term-ctrls button, .action-btn` (style.css:194-228) is the single button rule; resting, hover, focus-visible, and disabled states all inherit. No drift, no duplication. Spec line 320 explicitly endorses this consolidation choice; the executor did NOT introduce a new `.btn-mono` class but achieved the same effect by appending `.action-btn` to the existing list.
- **Banner aesthetic continuity** — `#config-banner` uses the same box model as `#iso-banner` (Phase 1): `padding: var(--md); background: var(--banner-bg);` with `<strong>` headline + body paragraph. The two destructive banners are visually parallel as the spec requires.
- **`#fresh-banner` reads as page chrome, not a notification.** No box-shadow, no border, no rounded corners, no icon — just `--surface` background with `padding: var(--sm) var(--md)`, identical inline padding to the header. Aesthetic anchor preserved.
- **Status pill state machine extended without breaking Phase 2 transitions.** Five states (IDLE/LOADING/RUNNING/HALTED/BUSY); `recomputePillLocal()` is unchanged; BUSY is overlaid by the lock observer, not woven into the local lifecycle. Phase 4 callers can opt-in without re-architecting.
- **Score drop: -2 for the dismiss-persistence defect (Top Fix #1) and -0 for the empty-render flicker (Top Fix #3).** Brand consistency with the "terse-no-chrome / snap-in-snap-out" promise (spec line 20, spec line 371) is broken by a dismissed banner reappearing without a new event. This is the dominant brand finding. The flicker is a paint-order quirk that could erode the terseness on slow renders; not a defect today but is the kind of small thing that compounds.

---

## Conformance to UI-SPEC.md

| Section | Conforms? | Notes |
|---------|-----------|-------|
| Design System (line 24-37) | YES | No new tooling; glyph-only dismiss `×` implemented as real `<button>` with `aria-label="Dismiss"` (app.js:184-188). |
| Spacing Scale (line 40-53) | YES | Zero new tokens; `max-height: 25vh` constraint matches spec. |
| Typography (line 57-71) | YES | Zero new roles; uppercase Label applied to 3 surfaces. |
| Color (line 74-132) | YES | Zero new hex; accent reservation amended by exactly one use (`#banner-launch`); BUSY pill = IDLE pill visually. |
| Copywriting Contract (line 136-165) | YES | All literal strings present and exact: `BUSY`, `Kernel rebuilt —` (with em-dash), `LAUNCH`, `×`, `Kernel rebuilt but not ELF — ignored.`, `bootroom.toml is invalid`, `[bootroom] config reloaded (N actions, M scenarios)`, `[bootroom] kernel rebuild rejected: <reason>`, `[bootroom] config unavailable: <status>`. Verified against app.js lines 157, 194, 198, 188, 205, 612, 631, 648, 306, 313. |
| Component Inventory (line 169-230) | YES | Three new components present in DOM (index.html:49, :52, :55); structure matches the spec's diagrams. |
| Interaction Contracts (line 324-368) | MOSTLY | Contract 1 (action click) ✓; Contract 2 (banner resolver) ✗ — dismiss persistence bug; Contract 3 (initial load) ✓; Contracts 4-9 ✓; Contract 10 (CLI surfaces) is out of browser scope. |
| Accessibility Floor (line 384-402) | YES | `role="alert"` on config-banner; deliberately omitted on fresh-banner; `aria-label="Dismiss"` on `×`; `disabled` attribute used (not CSS-only); contrast check deferred to live-render verification. |
| Registry Safety (line 406-415) | YES | No new vendored assets. |

---

## Files Audited

- `crates/bootroom/web/index.html` (95 lines) — DOM containers, vendor script order, COOP/COEP probe
- `crates/bootroom/web/style.css` (324 lines) — palette, spacing, typography, all three Phase 3 components + BUSY pill
- `crates/bootroom/web/app.js` (921 lines) — banner state machine, action button rendering, lock observer wiring, WS frame handlers, initial config fetch
- `crates/bootroom/web/funnel.js` (353 lines) — lock primitive (lockInput/unlockInput, setLockObserver, idempotent state, observer try/catch)

---

## Recommendation Count

- **Priority fixes:** 3 (1 BLOCKER, 2 WARNING)
- **Minor recommendations:** 0 (no nits surfaced; the code is dense and the spec was followed closely)

The dismiss-persistence finding (Top Fix #1) is the only true blocker found; the other two are latent quality hazards that compound poorly in Phase 4. Recommend addressing all three before Phase 4 kicks off, since Phase 4's scenario engine is the first caller of `funnel.lockInput()` and the first source of frequent `resolveBanners()` invocations.
