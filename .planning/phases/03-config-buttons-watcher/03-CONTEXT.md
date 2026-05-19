---
phase: 3
name: Config, Buttons, Watcher
gathered: 2026-05-19
status: Ready for planning
mode: discuss
---

# Phase 3: Config, Buttons, Watcher — Context

<domain>
## Phase Boundary

**Goal:** A user authors `bootroom.toml`, sees grouped action buttons in the UI, clicks them to drive the guest, and runs `make` in their kernel repo to get a non-intrusive "fresher build available" banner.

**In scope (Phase 3):**
- TOML schema parsed by `bootroom-core` (deny_unknown_fields, required `schema_version = 1`)
- `bootroom.toml` from CWD by default; `--config <path>` override
- Action button definitions (label, bytes, optional group, optional description)
- Scenario *definitions* (name, action refs, assertions, timeout_ms) — types only; engine in Phase 4
- `/api/config` endpoint returns JSON projection of parsed TOML
- Browser renders action buttons in groups, in TOML insertion order
- Click handler funnels bytes through the existing client funnel (single-writer)
- CLI `--action 'label=<bytes>'` repeatable, escape-sequence string encoding
- `bootroom check` subcommand: parse + cross-validate; exits 0/non-zero with locating errors
- `bootroom init` subcommand: writes a 25-line example with comments
- Single `notify-debouncer-full` pool watching both `bootroom.toml` and the kernel file
- TOML change → `ConfigUpdate` WS frame → browser re-renders buttons
- Kernel change → size-stability + ELF magic check → `KernelChanged` WS frame → non-intrusive banner with one-click Launch
- Banner is dismissable; reappears on next change
- "Manual serial typing is disabled while a scenario is running" — the funnel grows a `lockInput()`/`unlockInput()` API (scenarios are Phase 4 callers; the lock primitive lands here so Phase 4 can use it without re-architecting)

**Out of scope (later phases):**
- Scenario *engine* execution (RUN-*) — Phase 4
- `bootroom run` headless CLI — Phase 4
- Headless Chromium driver — Phase 4 (reuses Spike B's `chromiumoxide`)
- `bootroom doctor` — Phase 5
- Crates.io publish + cargo-dist — Phase 6
- Action button *keyboard shortcuts* — v2 (per FEATURES.md "Should have")
- In-place qemu reset (Spike A's deferred mechanism) — still deferred

**Phase 3 requirements (from ROADMAP.md):** CFG-01..10, ACT-01..04, WCH-01..05

</domain>

<decisions>
## Implementation Decisions

### TOML schema shape — flat `[[action]]` arrays, types in `bootroom-core`

**Decision:** Flat array-of-tables for actions and scenarios. Group is a plain optional string field on each action. Top-level `schema_version = 1` required.

Schema (illustrative):
```toml
schema_version = 1

[[action]]
label = "reboot"
bytes = "reboot\r"
group = "Boot"
description = "Send reboot command to the guest shell"

[[action]]
label = "panic_inject"
bytes = "\x03\x03\x03"   # Ctrl-C x3
group = "Diagnostics"

[[scenario]]
name = "boot_smoke"
actions = ["reboot"]
timeout_ms = 30000

  [[scenario.assert]]
  kind = "contains"     # contains | regex
  pattern = "login: "
  after = "reboot"      # serial buffer to scan (action label) or "any"
  timeout_ms = 5000
```

Rust types (in `bootroom-core::config`):
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default, rename = "action")]
    pub actions: Vec<Action>,
    #[serde(default, rename = "scenario")]
    pub scenarios: Vec<Scenario>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub label: String,
    pub bytes: String,             // escape-decoded at load time into bytes_decoded
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub actions: Vec<String>,
    #[serde(default, rename = "assert")]
    pub assertions: Vec<Assertion>,
    #[serde(default = "default_scenario_timeout")]
    pub timeout_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub kind: AssertionKind,       // Contains | Regex
    pub pattern: String,
    pub after: String,             // action label or "any"
    #[serde(default = "default_assertion_timeout")]
    pub timeout_ms: u64,
}
```

- `schema_version` MUST equal `1` in Phase 3; future bumps are loud rejects.
- `deny_unknown_fields` on every struct — typos produce loud errors with line/column from the `toml` crate.
- Insertion order preserved naturally by `Vec`s — TOML arrays-of-tables are ordered.
- A `LoadedConfig` validation layer wraps `Config`: decodes `bytes` escape sequences once, validates scenario.action refs point to known action labels, exposes `actions_by_label` HashMap. Used by `/api/config`, `bootroom check`, and Phase 4's scenario engine.

### Action invocation flow — direct client funnel; escape-sequence CLI encoding

**Decision:** Button click → `funnel.enqueue(decodedBytes, {pacingMs: 15})` directly. NO WS round-trip.

- Action button click handler reads pre-decoded bytes from `/api/config` (decoded once on the server, projected as a JS `Uint8Array` via base64 in the JSON projection).
- `funnel.enqueue` is the existing Phase-2 single-writer path; reuses the same `master.ldisc.writeFromLower` mechanism.
- `pacingMs` default is `15` per CONTEXT D-03 from Phase 2 (configurable via `?pacing=N` URL param, also configurable per-action via an optional `pacing_ms` field on `Action` in a future bump — Phase 3 ships the global default only).
- Server observes the bytes via the existing Phase-2 `SerialOut` mirror (it logs guest *output* — not action *invocation*). For full action-invocation visibility from headless (Phase 4), the headless driver will execute scenarios *client-side* (sending Action-trigger requests over WS that the client translates to funnel.enqueue). That protocol design is Phase 4's call.
- ROADMAP success criterion 2 says "writes via `slave.write`" — this is **superseded by Phase 2's CR-01 fix**. Bytes flow through `master.ldisc.writeFromLower`, not `slave.write` (which is the OUTPUT path). The ROADMAP wording is updated in this phase's SUMMARY.

**CLI `--action` encoding — escape sequences:**

Format: `--action 'label=BYTES'` where BYTES is a string with C-style escape sequences:
- `\r`, `\n`, `\t`, `\0`, `\\` — standard
- `\x41` — hex byte (two hex digits)
- everything else literal UTF-8

Examples:
- `--action 'reboot=reboot\r'`
- `--action 'ctrlc=\x03'`
- `--action 'banner=Hello, world!\r\n'`

Repeatable flag (clap's `Vec<String>` via `--action ... --action ...`). If the same label appears in both `--action` and `bootroom.toml`, the CLI wins (override semantics, matches Phase 2's `--no-open` precedence). Duplicate `--action` labels: last one wins.

A shared decoder lives in `bootroom-core::config::decode_bytes_escape(s: &str) -> Result<Vec<u8>>` — used by both `--action` parsing and the TOML `Action.bytes` field decoder. Invalid escape sequences produce a typed error with the offending character position.

### Config live-reload + kernel watcher — single notify pool, WS broadcast frames

**Decision:** One `notify-debouncer-full` instance, two watches, broadcast over `/ws` to all connected clients.

**Watcher subsystem (`crates/bootroom/src/watcher.rs`):**
- Single `Debouncer` instance, 300 ms debounce window (per research-locked pitfall #4 mitigation).
- Watches the *parent directory* for the kernel filename (atomic-rename safe; `make` typically renames a tempfile into place).
- Watches `bootroom.toml` for content changes.
- Events fan out to a tokio `broadcast::channel<WatchEvent>(16)` that the WS handler subscribes per connection.

**Kernel watcher details (per WCH-* reqs):**
- WCH-01: `notify-debouncer-full`, 300 ms.
- WCH-02: watches parent dir for the kernel basename; handles atomic-rename builds.
- WCH-03: size-stability check across two debounce ticks (read size, wait 100 ms, read again — if unchanged, accept).
- WCH-04: ELF magic byte sniff (read first 4 bytes, check `\x7f ELF`). Non-ELF → log warning, send `KernelChanged { ok: false, reason: "not ELF" }` so the UI can show a warning state.
- WCH-05: surface as `KernelChanged { ok: true, mtime, size, sha256_prefix }` WS frame. Browser shows non-intrusive banner; user clicks "Launch" (existing Phase 2 button) to reload.

**TOML watcher details (per CFG-10):**
- On change: try to re-parse via the same code path as startup (`LoadedConfig::load`).
- Success → broadcast `ConfigUpdate { config: <JSON projection> }`; client re-renders action buttons in place.
- Failure → broadcast `ConfigInvalid { error: <message>, line: <n>, col: <n> }`; client shows error state (red banner) but keeps the last-known-good config active.

**New `WsMessage` variants (in `bootroom-core`):**
```rust
KernelChanged { ok: bool, mtime: i64, size: u64, sha256_prefix: String, reason: Option<String> },
ConfigUpdate { config: serde_json::Value },     // JSON projection of LoadedConfig
ConfigInvalid { error: String, line: Option<u32>, col: Option<u32> },
```

**Banner UI:**
- Lives between `#hdr` and `#terminal` (replaces the current `#iso-banner` slot when both fire — `iso-banner` is also for one-off problems; only one banner visible at a time, priority: iso > config-invalid > kernel-fresh).
- Element: `<div id="fresh-banner" hidden>Kernel rebuilt — <button id="banner-launch">Launch</button> <button id="banner-dismiss">×</button></div>`
- Styled per UI-SPEC extension (new section in 03-UI-SPEC.md): uses `--accent` for the button text (only the dismiss "×" uses `--fg-muted`); background `--surface`.
- Dismiss hides it; next `KernelChanged` re-shows.

### Funnel input lock primitive (`lockInput()` / `unlockInput()`)

**Decision:** Ship the API in Phase 3; Phase 4 is the first caller.

- Add `funnel.lockInput()` / `funnel.unlockInput()` to `funnel.js`.
- When locked: `xterm.onData` callback short-circuits before enqueue; visual cue is the status pill switching to a new `BUSY` state (added alongside the existing 4 — total 5 states). Action button clicks ALSO short-circuit while locked.
- `enqueue` from WS `SerialIn` still flows (server-initiated scenarios are the reason the lock exists in the first place).
- Phase 4 will call `lockInput()` at scenario start, `unlockInput()` at scenario end. Phase 3 ships an unused API plus the BUSY pill state.

### `bootroom check` + `bootroom init` subcommands

**`bootroom check [--config PATH]`:**
- Parses the config (CWD `bootroom.toml` or `--config`).
- Cross-validates: scenario.actions all resolve to defined action labels; `schema_version == 1`; escape sequences decode; action labels are unique.
- Success → prints `bootroom.toml: ok (N actions, M scenarios)` to stdout, exits 0.
- Failure → prints structured error with file:line:col (when from `toml` crate parse error) or semantic message (e.g., `scenario "boot_smoke" references unknown action "reboot"`). Exits 1.
- Suitable for CI preflight: `bootroom check && bootroom serve ...`.

**`bootroom init [--force]`:**
- Writes `./bootroom.toml` if absent; refuses to overwrite without `--force`.
- 25-line example, well-commented, includes one action and one scenario showing the canonical shape.
- After write, prints `Wrote ./bootroom.toml` to stdout.

### Scenario WS protocol additions — deferred to Phase 4

**Decision:** Phase 3 defines scenario *schema* + validates it via `bootroom check`. The Phase-4 engine's WS protocol surface (ScenarioStart, AssertionResult, etc.) is designed under pressure of actual scenario execution, not speculatively in Phase 3.

</decisions>

<canonical_refs>
## Canonical References

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md` (Phase 3 reqs: CFG-01..10, ACT-01..04, WCH-01..05)
- `.planning/ROADMAP.md` (Phase 3 section + success criteria — note: criterion 2 wording "via slave.write" is superseded by Phase 2's CR-01 fix)
- `.planning/phases/02-websocket-live-serial/02-CONTEXT.md` (WsMessage protocol; funnel architecture)
- `.planning/phases/02-websocket-live-serial/02-RESEARCH.md` (xterm-pty direction pitfalls — INPUT vs OUTPUT)
- `.planning/phases/02-websocket-live-serial/02-REVIEW.md` (Phase 2 patterns to preserve)
- `.planning/phases/02-websocket-live-serial/02-UI-SPEC.md` (palette, spacing, button styles — Phase 3 extends)
- `.planning/phases/01-walking-skeleton/01-RESEARCH.md` (notify-debouncer-full 0.5/0.7 versions; ELF magic sniff)
- `.planning/research/PITFALLS.md` (#4 watcher debounce + ELF + size-stability; #5 TOML schema drift)
- `crates/bootroom-core/src/lib.rs` (current WsMessage + GuestState; Phase 3 adds Config types + ConfigUpdate/ConfigInvalid/KernelChanged variants)
- `crates/bootroom/src/server.rs` (router; add /api/config + /api/config/raw endpoints)
- `crates/bootroom/src/cli.rs` (add `--config`, `--action`, `check`, `init` subcommands)
- `crates/bootroom/src/ws.rs` (subscribe each connection to the broadcast channel from watcher.rs)
- `crates/bootroom/web/funnel.js` (add lockInput/unlockInput)
- `crates/bootroom/web/app.js` (render buttons from /api/config; subscribe to ConfigUpdate/KernelChanged WS frames)
- `crates/bootroom/web/index.html` (add #fresh-banner placeholder + action-buttons container)
- `crates/bootroom/web/style.css` (style new banner + button-panel; palette-pure)

External:
- notify-debouncer-full: https://docs.rs/notify-debouncer-full
- toml crate: https://docs.rs/toml (use 1.x for accurate error spans)
- clap subcommand patterns: https://docs.rs/clap/4/clap/_derive/#subcommands

</canonical_refs>

<code_context>
## Existing Code Insights

After Phase 2 the repo has:
- `bootroom-core` exports `WsMessage` + `GuestState` — Phase 3 adds `Config`/`Action`/`Scenario`/`Assertion` + 3 new WsMessage variants.
- `crates/bootroom/src/server.rs` has /api/kernel/info, /kernel, /assets/*, /ws — Phase 3 adds /api/config (and updates /ws to broadcast watcher events).
- `crates/bootroom/src/cli.rs` has `ServeArgs` (clap derive) — Phase 3 refactors to subcommands enum: `Serve(ServeArgs)`, `Check(CheckArgs)`, `Init(InitArgs)`.
- `crates/bootroom/web/funnel.js` Funnel class — Phase 3 adds `locked: bool` + lock/unlock methods.
- `crates/bootroom/web/app.js` already has `connectWs`, `handleWsFrame` — Phase 3 extends the switch with KernelChanged/ConfigUpdate/ConfigInvalid; adds renderActionButtons(config) called on Hello (initial) and ConfigUpdate.
- `crates/bootroom/web/index.html` already has `#iso-banner` slot — Phase 3 adds `#fresh-banner` + `#actions-panel` containers.
- Phase 2's TestServer harness reuses unchanged.

</code_context>

<specifics>
## Specific Ideas

- The action buttons render below the existing terminal-controls overlay but above the terminal scroll area? Or in a separate side panel? UI-SPEC will resolve. Initial proposal: a thin strip above the terminal, grouped horizontally, each group label small + buttons next to it. Hidden if config has 0 actions.
- `/api/config` JSON projection includes decoded bytes (base64) and groups (derived) so the browser doesn't re-implement escape decoding.
- `bootroom check` exit codes: 0 ok, 1 parse/validation error, 2 file not found, 3 schema_version mismatch. Documented in `--help`.
- The watcher broadcast channel uses `tokio::sync::broadcast::channel(16)` — bounded, drops oldest on lag (acceptable for dev tool; a slow client missing a config update just gets the next one).
- File lock on `bootroom.toml` during write+parse: skip. notify's debouncer already handles partial-write events. If a parse fails because the user wrote a partial file, we just send ConfigInvalid; user fixes it; next save broadcasts a new ConfigUpdate.

</specifics>

<deferred>
## Deferred Ideas

- **Scenario engine** (RUN-*) — Phase 4
- **Scenario WS protocol additions** — Phase 4 designs under pressure of real use
- **Per-action `pacing_ms` override field** — schema can carry it later in a `schema_version = 2` bump
- **Action keyboard shortcuts** — v2 (FEATURES.md)
- **Per-button confirmation dialogs** for destructive actions — out of scope; the harness is a dev tool, not a production console
- **TOML hot-reload of `serve` flags** (e.g. changing `--port` at runtime) — out of scope; only action/scenario definitions live-reload
- **Config diff in `ConfigUpdate` frame** — currently sends full config; if size becomes an issue, switch to JSON Patch. Defer until profiling shows it matters.

</deferred>
