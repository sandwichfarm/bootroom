# Phase 3: Config, Buttons, Watcher — Research

**Researched:** 2026-05-19
**Domain:** TOML schema parsing with span-aware errors + axum `/api/config` projection + `notify-debouncer-full` single-pool watcher for two paths + `tokio::sync::broadcast` fan-out over `/ws` + clap subcommand refactor + browser-side action-button rendering and banner state machine
**Confidence:** HIGH for crate APIs (Context7-equivalent verified against `cargo info`, docs.rs, and CONTEXT.md decisions); HIGH for axum/tokio patterns (Phase 2 review pinned them); HIGH for the planner-facing shapes (CONTEXT.md is exceptionally specific); MEDIUM for the size-stability tick algorithm (the exact wait window is a tuning question that surfaces only against a live `make`).

## Summary

Phase 3 is an additive layer on top of the Phase 2 substrate. The Rust side gains four pieces — TOML config types in `bootroom-core::config`, a `/api/config` endpoint, a single `notify-debouncer-full` instance that watches both `bootroom.toml` and the kernel-parent directory, and a `tokio::sync::broadcast` fan-out that pushes three new `WsMessage` variants (`ConfigUpdate`, `ConfigInvalid`, `KernelChanged`) to every connected `/ws` socket. The CLI is refactored from a single `Serve(ServeArgs)` arm into a three-arm `Cmd` enum (`Serve` / `Check` / `Init`) with a small new `--config` flag shared via clap's `#[arg(global = true)]` or per-args; `--action 'label=BYTES'` is a repeatable `Vec<String>` on `ServeArgs` parsed via a shared `bootroom_core::config::decode_bytes_escape` helper.

The browser side adds three components (`#actions-panel`, `#fresh-banner`, `#config-banner`), a `BUSY` pill state, a synchronous banner priority resolver, and a `funnel.lockInput()` / `unlockInput()` pair (shipped unused — Phase 4 is the first caller). The funnel does **not** change its byte-path mechanics; it only grows the lock primitive and a `disabled` toggle on action buttons. Action clicks go straight through `funnel.enqueue(decodedBytes, { pacingMs: 15 })`, bypassing the WS server entirely — this is the explicit CONTEXT.md decision and it keeps Phase 4's headless action-invocation protocol design unconstrained.

The single biggest risk-of-the-phase is the **two-path debounced watcher**. `notify-debouncer-full` 0.7 wants one `Debouncer` instance with multiple `.watch()` calls; the events come out of a single callback with `Vec<DebouncedEvent>` and the consumer must demux by inspecting `event.paths[0]`. The kernel path is filename-matched against the parent directory's events (atomic-rename-safe per Pitfall #4); `bootroom.toml` is path-matched against its canonical absolute form (atomic-save aware: editors often rename `bootroom.toml.swp` → `bootroom.toml`, producing `Remove` then `Create` events the debouncer collapses).

**Primary recommendation:** Lock the dependency triple to `notify = "8"` + `notify-debouncer-full = "0.7"` + `toml = "1.1"` (verified via `cargo info`, all on MSRV 1.85 which matches the workspace floor). Build the watcher as a separate `crates/bootroom/src/watcher.rs` module that owns the `Debouncer` and the `broadcast::Sender<WsMessage>`; pass a `broadcast::Sender` clone into `AppState` so `/ws` handlers subscribe per connection. Land the TOML types in `bootroom-core::config` so the future Phase-4 scenario engine reuses them unchanged.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| TOML parsing + schema validation | API/Backend (`bootroom-core::config`) | — | Pure types library; reused by `serve`, `check`, future `run`. |
| Escape-sequence byte decoding (`\r`, `\n`, `\x41`) | API/Backend (`bootroom-core::config::decode_bytes_escape`) | — | Shared by `--action` parser and the `Action.bytes` field decoder; one canonical implementation. |
| `/api/config` JSON projection | API/Backend (axum handler) | — | Server pre-decodes bytes to base64; browser does `atob` once, never re-implements escape rules. |
| `notify-debouncer-full` instance | API/Backend (`crates/bootroom/src/watcher.rs`) | — | Single pool, two paths, one `broadcast::Sender<WsMessage>` outbound. |
| Kernel ELF magic + size-stability gate | API/Backend (watcher) | — | Pitfall #4 mitigation; sniffing happens in the watcher task, not in `/kernel` (that's still byte-streaming). |
| `WsMessage` variant definitions (`ConfigUpdate`/`ConfigInvalid`/`KernelChanged`) | API/Backend (`bootroom-core`) | — | Wire protocol home; Phase 4 reuses unchanged. |
| `tokio::sync::broadcast` fan-out | API/Backend (axum `/ws` handler) | — | Each WS connection calls `tx.subscribe()` to get a per-conn `Receiver`; bounded capacity 16 per CONTEXT. |
| Action button rendering | Browser/Client (`app.js`) | — | DOM-only; reads pre-decoded base64 bytes from `/api/config`. |
| Action click → `funnel.enqueue` | Browser/Client | — | Direct funnel write; no WS round-trip per D-02 in CONTEXT.md. |
| Banner priority resolver | Browser/Client (`app.js`) | — | Synchronous `resolveBanners()` runs on every relevant state mutation; enforces iso > config-invalid > kernel-fresh ladder. |
| `funnel.lockInput()` / `unlockInput()` | Browser/Client (`funnel.js`) | — | API ships unused in Phase 3; Phase 4 scenario engine is the first caller. |
| `bootroom check` CLI subcommand | API/Backend | — | Parses + cross-validates via `LoadedConfig::load`; structured errors to stderr; exit 0/1/2/3. |
| `bootroom init` CLI subcommand | API/Backend | — | Writes a 25-line example TOML to CWD; refuses overwrite without `--force`. |

## Standard Stack

### Core (new deps for Phase 3)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `toml` | `1.1.2+spec-1.1.0` `[VERIFIED: cargo info 2026-05-19]` | Parse `bootroom.toml`; provides `toml::de::Error::span()` for line/col location | The successor to `toml = "0.8"`; serde-integrated, MSRV 1.85. CONTEXT.md `<canonical_refs>` already points at "use 1.x for accurate error spans". |
| `notify` | `8.2` (cargo will resolve from `^8`; latest stable is `8.2.x`) `[VERIFIED via cargo info: 9.0.0-rc.4 exists but is a release candidate — pin to 8.x for stability]` | Cross-platform FS watcher | Required transitively by `notify-debouncer-full = "0.7"`. Pin 8.x — 9.0 is RC and `notify-debouncer-full = "0.8.0-rc.2"` is also RC; both move together. |
| `notify-debouncer-full` | `0.7.0` `[VERIFIED: cargo info 2026-05-19, MSRV 1.85]` | 300ms debounce window over raw `notify` events; preserves event ordering and reports rename pairs | Pitfall #4 mitigation. Phase 1 RESEARCH flagged this for re-verification at Phase 3; the verification is complete. The `0.8.0-rc.2` exists but is RC — stay on 0.7.0 (the production line). |

**Existing deps that Phase 3 reuses unchanged:** axum 0.8, tower-http 0.6, tokio 1.x (we'll add the `tokio::sync::broadcast` API — already enabled by the `sync` feature already in `Cargo.toml`), clap 4.6 derive, serde 1.0, serde_json 1.x, anyhow 1.x, tracing 0.1, mime_guess 2.0.5, sha2 0.10, hex 0.4.

**Installation:**
```bash
# Workspace Cargo.toml [workspace.dependencies] additions:
toml = "1.1"
notify = "8"
notify-debouncer-full = "0.7"

# crates/bootroom-core/Cargo.toml: pulls toml + serde
# crates/bootroom/Cargo.toml: pulls notify + notify-debouncer-full + toml
```

**Version verification protocol:** Run `cargo update` once after the Cargo.toml edit; commit `Cargo.lock`. The three new crates are all MSRV ≤ 1.85, matching the workspace floor. No feature-flag traps observed.

### Supporting (no new deps required)

- `tokio::sync::broadcast::channel(16)` — bounded broadcast; bounded means slow consumers see `RecvError::Lagged(skipped: u64)`. The watcher subsystem and `/ws` handler both already pull `tokio` via workspace.

### Browser-Side (no new vendored deps)

- Vanilla JS + DOM. No npm. The base64 helpers in `funnel.js` (`bytesToB64` / `b64ToBytes`) already exist from Phase 2 and Phase 3 reuses them to decode `/api/config`'s `bytes_b64` field on action-button mount.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `toml = "1.1"` | `toml = "0.8"` | 1.1 has stable `span()` on `de::Error`. 0.8 has it via `Error::line_col()` but the migration story to 1.x is "now and cheap" or "later and forced." Pick 1.1. |
| `notify-debouncer-full` | `notify-debouncer-mini` | Mini reports rename pairs as Remove+Create, which would fire the kernel watcher twice on a single `make` atomic-rename. Full preserves the rename relationship; preferred per CONTEXT.md `<decisions>` "Kernel watcher details (per WCH-* reqs)". |
| `notify` 8.x | `notify` 9.0-rc | 9.0 is release candidate; CC0-1.0 license unchanged; no breaking-change urgency. Stick with 8.x and `notify-debouncer-full = "0.7"` (which pins to notify 8). |
| Single `Debouncer` + two `.watch()` calls | Two separate `Debouncer` instances | One pool is cheaper (one OS-level inotify/FSEvents handle pair) and the demux-by-path cost is trivial. The two-debouncer alternative would also double the tokio task count and complicate the `broadcast::Sender` ownership story. |
| `serde(deny_unknown_fields)` per struct | A custom `Deserialize` impl that reports field-name suggestions | `deny_unknown_fields` is the Phase 3 CONTEXT decision and aligns with Pitfall #8 mitigation. "Did you mean?" Levenshtein suggestions are a polish item (UX backlog, not Phase 3). |
| `Vec<Action>` (TOML insertion-order naturally preserved) | `IndexMap<String, Action>` | Pitfall #12 prevention is satisfied by `Vec` alone — TOML array-of-tables `[[action]]` is positionally ordered and `serde::Deserialize` for `Vec<T>` honors it. The `actions_by_label: HashMap<&str, usize>` index lives only on `LoadedConfig` (the validated wrapper). |
| Custom WS protocol negotiation on schema bump | `schema_version = 1` hard reject of other values | Spec says "future bumps are loud rejects" — Phase 3 ships `1` only; no negotiation. |
| Action invocation via WS round-trip | Direct browser `funnel.enqueue` | CONTEXT D-02: "NO WS round-trip." Server observes via existing SerialOut mirror. Headless action injection is Phase 4's design call. |

## Architecture Patterns

### System Architecture Diagram

```
                            Phase 3 dataflow (delta over Phase 2)

  ┌────────────────────────────────────────────────────────────────────────────────┐
  │                     bootroom (single process)                                  │
  │                                                                                │
  │   ┌───────────────┐                                                            │
  │   │ clap Cmd enum │ ──── Serve(ServeArgs{kernel, config, action[], …})        │
  │   │ (Phase 3 add) │ ──── Check(CheckArgs{config})                              │
  │   └───────┬───────┘ ──── Init(InitArgs{force})                                 │
  │           │                                                                    │
  │           ▼                                                                    │
  │  ┌─────────────────────────────────────────────────────────────────────┐       │
  │  │ AppState (extended)                                                 │       │
  │  │   kernel: PathBuf            config_path: PathBuf                   │       │
  │  │   assets_dir: Option<…>      loaded_config: RwLock<LoadedConfig>    │       │
  │  │   digest_cache: …            ws_broadcast: broadcast::Sender<WsMsg> │       │
  │  └────────┬──────────────────────────────────────────────┬─────────────┘       │
  │           │                                              │                     │
  │           ▼                                              ▼                     │
  │  ┌────────────────────────┐                ┌─────────────────────────────┐     │
  │  │ axum::Router (extend)  │                │ watcher.rs (new module)     │     │
  │  │   /api/config  → JSON  │                │  Debouncer (notify-debounce │     │
  │  │   /ws (extend: each    │                │  -full, 300ms)              │     │
  │  │     conn subscribes    │                │   ├─ watch(bootroom.toml)   │     │
  │  │     broadcast)         │                │   └─ watch(kernel.parent)   │     │
  │  └───────────┬────────────┘                │  callback demux:            │     │
  │              │                             │   - path == config_path →   │     │
  │              │                             │       LoadedConfig::reload  │     │
  │              │                             │       → broadcast Config*   │     │
  │              │ ws.send broadcast frames    │   - path.parent == kernel.  │     │
  │              │                             │     parent AND basename     │     │
  │              ▼                             │     == kernel basename →    │     │
  │       ┌──────────────┐                     │       size-stability +      │     │
  │       │ broadcast    │  ◄──────────────────│       ELF magic →           │     │
  │       │ Sender<WsMsg>│      tx.send        │       broadcast KernelChng  │     │
  │       │ cap 16       │                     └─────────────────────────────┘     │
  │       └──────┬───────┘                                                         │
  │              │                                                                 │
  └──────────────┼─────────────────────────────────────────────────────────────────┘
                 │ per-conn tx.subscribe() → Receiver
                 ▼
        ┌──────────────────┐
        │ Browser tab(s)   │
        │  app.js:         │
        │  - fetch /api/   │
        │    config →      │
        │    renderButtons │
        │  - ws.onmessage  │
        │      switch:     │
        │       Config*    │
        │         re-      │
        │         render   │
        │       Kernel*    │
        │         show     │
        │         banner   │
        │  - resolveBanners│
        │  - funnel.       │
        │    lockInput()   │
        │    (Phase 4 user)│
        └──────────────────┘
```

Key flows traced:

1. **Startup** → `Serve` loads `bootroom.toml`, applies `--action` overrides, validates, stores `LoadedConfig` in `AppState`. Spawns the watcher task. Starts axum.
2. **Client connect** → `/ws` handler subscribes to `state.ws_broadcast`, forwards each `WsMessage` from the receiver to the per-connection sink via the Phase 2 mpsc writer task.
3. **Initial config render** → Browser `GET /api/config` (after WS `Hello`), renders `#actions-panel`, runs `resolveBanners()`.
4. **`bootroom.toml` edit** → debouncer fires after 300ms idle → watcher demuxes by path → `LoadedConfig::reload` → on success broadcasts `ConfigUpdate { config: <JSON projection> }`; on failure broadcasts `ConfigInvalid { error, line, col }`.
5. **`make` rebuild of kernel** → debouncer fires → watcher demuxes by basename → opens file → reads `(size_t1, sleep 100ms, size_t2)`; if stable AND first 4 bytes = `\x7f ELF` → broadcasts `KernelChanged { ok: true, mtime, size, sha256_prefix }`. Otherwise `KernelChanged { ok: false, reason }`.
6. **Action button click** → DOM data-attribute `data-bytes-b64` → `b64ToBytes` → `funnel.enqueue(bytes, { pacingMs: 15 })`. No WS traffic for the action itself; SerialOut mirror (Phase 2) still observes guest behavior.

### Recommended Project Structure

```
crates/bootroom-core/src/
├── lib.rs                ← re-exports + WsMessage (Phase 2) + 3 new variants
└── config.rs             ← NEW: Config / Action / Scenario / Assertion / AssertionKind
                              + LoadedConfig + decode_bytes_escape + load_from_str /
                              load_from_path

crates/bootroom/src/
├── main.rs               ← Cmd dispatch (Serve | Check | Init)
├── cli.rs                ← REFACTOR: Cmd enum + ServeArgs/CheckArgs/InitArgs
├── lib.rs                ← re-export AppState, build_router (unchanged signature)
├── server.rs             ← extend build_router with /api/config route
├── state.rs              ← extend AppState: + loaded_config, + ws_broadcast
├── watcher.rs            ← NEW: spawn_watcher(state) -> Result<()>
├── api_config.rs         ← NEW: GET /api/config handler (JSON projection)
├── check_cmd.rs          ← NEW: bootroom check entry
├── init_cmd.rs           ← NEW: bootroom init entry (25-line example bytes)
├── ws.rs                 ← extend: subscribe broadcast Receiver in handle_socket
├── assets.rs             ← unchanged
├── headers.rs            ← unchanged
├── kernel_info.rs        ← unchanged
├── kernel_stream.rs      ← unchanged
└── embed.rs              ← unchanged

crates/bootroom/web/
├── index.html            ← + #actions-panel + #fresh-banner + #config-banner
├── app.js                ← + renderActionButtons + resolveBanners + new
                              WS branches (ConfigUpdate / ConfigInvalid /
                              KernelChanged) + initial /api/config fetch
├── funnel.js             ← + lockInput() / unlockInput() + locked flag
└── style.css             ← + #actions-panel / .action-group / .action-btn /
                              #fresh-banner / #config-banner / .pill[data-state="BUSY"]
```

### Pattern 1: Single `Debouncer`, two `.watch()` calls, demux in callback

**What:** One `notify-debouncer-full::Debouncer` instance owns watches for both `bootroom.toml` (file watch) and the kernel's parent directory (directory watch for atomic-rename safety). The callback receives `Vec<DebouncedEvent>`; the demux compares each event's `paths[0]` to the two known paths.

**When to use:** Always in Phase 3. Two separate debouncers would double the OS-handle cost and complicate the `broadcast::Sender` ownership; one pool is cheaper and the demux is trivial.

**Example:**

```rust
// crates/bootroom/src/watcher.rs
// Source: docs.rs/notify-debouncer-full/0.7.0
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use std::{path::{Path, PathBuf}, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use bootroom_core::WsMessage;

pub fn spawn_watcher(
    config_path: PathBuf,         // canonicalized
    kernel_path: PathBuf,          // canonicalized
    tx: broadcast::Sender<WsMessage>,
    state: Arc<crate::state::AppState>,
) -> anyhow::Result<()> {
    let kernel_parent = kernel_path.parent()
        .ok_or_else(|| anyhow::anyhow!("--kernel has no parent dir"))?
        .to_path_buf();
    let kernel_basename = kernel_path.file_name()
        .ok_or_else(|| anyhow::anyhow!("--kernel has no filename"))?
        .to_os_string();

    // Spawn the blocking debouncer thread; it owns the inotify handle.
    // The callback marshals events back into tokio via tx.send().
    let tx_for_cb = tx.clone();
    let cfg_for_cb = config_path.clone();
    let kparent_for_cb = kernel_parent.clone();
    let kbase_for_cb = kernel_basename.clone();
    let state_for_cb = state.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(300),
        None,
        move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    let mut config_dirty = false;
                    let mut kernel_dirty = false;
                    for ev in events {
                        let Some(p) = ev.event.paths.first() else { continue };
                        // Compare canonical paths; on rename events `paths`
                        // is [src, dst] — dst is what we care about for
                        // atomic-save flows.
                        let target = ev.event.paths.last().unwrap_or(p);
                        if target == &cfg_for_cb {
                            config_dirty = true;
                        } else if target.parent() == Some(&kparent_for_cb)
                            && target.file_name() == Some(&kbase_for_cb)
                        {
                            // Filter event kinds we care about. Modify
                            // and Create are relevant; Access::Open is
                            // noise (some FS emit it on every read).
                            if matches!(
                                ev.event.kind,
                                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                            ) {
                                kernel_dirty = true;
                            }
                        }
                    }
                    // Hand off to tokio. spawn_blocking is NOT required —
                    // tx.send is non-blocking on a broadcast channel.
                    if config_dirty {
                        handle_config_change(&cfg_for_cb, &state_for_cb, &tx_for_cb);
                    }
                    if kernel_dirty {
                        handle_kernel_change(&kernel_path, &state_for_cb, &tx_for_cb);
                    }
                }
                Err(errs) => {
                    for e in errs { tracing::warn!(?e, "watcher error"); }
                }
            }
        },
    )?;

    debouncer.watch(&config_path, RecursiveMode::NonRecursive)?;
    debouncer.watch(&kernel_parent, RecursiveMode::NonRecursive)?;

    // Keep the debouncer alive for the lifetime of the process.
    // Leak the Box (intentional) — the watcher MUST outlive any future
    // graceful-shutdown path; bootroom serve only exits on Ctrl-C.
    Box::leak(Box::new(debouncer));
    Ok(())
}
```

**Notes:**
- `Debouncer` is `Send + 'static` and owns its own OS thread; the callback runs on that thread (NOT a tokio task). Keep the callback short — do parsing + I/O inside `handle_config_change` / `handle_kernel_change` which are sync (or use `tokio::runtime::Handle::current().spawn(...)` for async work).
- `Box::leak` is the documented "process-lifetime" pattern. If we ever add graceful shutdown, store the `Debouncer` in `AppState` instead.
- Atomic-save editors (vim default, etc.) emit `Create` events when the renamed temp file lands; the dst path of a rename is what `paths.last()` exposes per `notify::Event` docs.

### Pattern 2: `broadcast::channel` fan-out for per-WS subscription

**What:** The `AppState` holds a `broadcast::Sender<WsMessage>` (cloned, bounded capacity 16). Each `/ws` connection's `handle_socket` calls `tx.subscribe()` on connect to get a fresh `Receiver<WsMessage>`, then forwards every received `WsMessage` to the per-connection mpsc (Phase 2 writer task pattern).

**When to use:** Every Phase 3 server-pushed event (`ConfigUpdate` / `ConfigInvalid` / `KernelChanged`) fans out to every connected client. broadcast is the correct primitive: bounded; slow consumers drop oldest with a `Lagged(n)` error the receiver MUST handle.

**Example:**

```rust
// crates/bootroom/src/ws.rs (extend existing handle_socket)
// Source: docs.rs/tokio/latest/tokio/sync/broadcast/
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    let (tx_local, mut rx_local) = mpsc::channel::<WsMessage>(32);
    let mut bcast_rx = state.ws_broadcast.subscribe();  // <-- NEW

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx_local.recv().await {
            // existing JSON serialize + sink.send loop, unchanged
        }
    });

    // NEW: forward broadcast events to the per-conn mpsc.
    let tx_for_bcast = tx_local.clone();
    let bcast_forwarder = tokio::spawn(async move {
        loop {
            match bcast_rx.recv().await {
                Ok(msg) => {
                    if tx_for_bcast.send(msg).await.is_err() { break; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "ws broadcast receiver lagged");
                    // Continue — a missed ConfigUpdate is acceptable:
                    // the next one will carry the full state. Per CONTEXT.md
                    // <specifics>: "a slow client missing a config update
                    // just gets the next one".
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // ... existing Hello send + reader loop ...

    drop(tx_local);
    let _ = writer.await;
    bcast_forwarder.abort();
}
```

**Notes:**
- Capacity 16 per CONTEXT.md `<specifics>`: bounded, drops oldest on lag.
- On `Lagged` we log + continue (don't break). Each `Config*` and `KernelChanged` frame is self-contained and an old one being dropped is fine — the next one re-establishes truth.
- The Phase 2 reader loop is UNCHANGED. The broadcast forwarder is a separate tokio task that lives alongside the writer.

### Pattern 3: TOML schema with span-aware error formatting

**What:** Parse via `toml::from_str::<Config>(s)`. On error, extract `e.span()` (`Option<Range<usize>>`) and convert byte offset to `(line, col)` by counting newlines in the input. Surface as `ConfigInvalid { error, line, col }` or as the `bootroom check` stderr line.

**When to use:** Every config-load site: startup, watcher reload, `bootroom check`. One canonical helper.

**Example:**

```rust
// crates/bootroom-core/src/config.rs
// Source: docs.rs/toml/1.1.2/toml/de/struct.Error.html
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default, rename = "action")]
    pub actions: Vec<Action>,
    #[serde(default, rename = "scenario")]
    pub scenarios: Vec<Scenario>,
}

/// Span-aware load error. `line` and `col` are 1-based when set.
#[derive(Debug, Clone, Serialize)]
pub struct LoadError {
    pub message: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

pub fn parse_str(input: &str) -> Result<Config, LoadError> {
    toml::from_str::<Config>(input).map_err(|e| {
        let (line, col) = e.span()
            .and_then(|range| offset_to_line_col(input, range.start))
            .map(|(l, c)| (Some(l), Some(c)))
            .unwrap_or((None, None));
        LoadError {
            message: e.message().to_string(),
            line, col,
        }
    })
}

fn offset_to_line_col(input: &str, byte_off: usize) -> Option<(u32, u32)> {
    if byte_off > input.len() { return None; }
    let prefix = &input[..byte_off];
    let line = (prefix.matches('\n').count() as u32) + 1;
    let col = match prefix.rfind('\n') {
        Some(nl) => (input[nl + 1 .. byte_off].chars().count() as u32) + 1,
        None     => (prefix.chars().count() as u32) + 1,
    };
    Some((line, col))
}
```

**Notes:**
- `toml = "1.1"` exposes `Error::span()` returning `Option<Range<usize>>` (byte offsets). There is **no** built-in `line_col()` helper in 1.x — manual offset→line/col conversion is required.
- Use `prefix.chars().count()` (not `prefix.len()`) so columns count Unicode scalar values, not bytes. Matches `vim`/`code` 1-based columns for non-ASCII configs.
- `e.message()` returns `&str` — clone into the `LoadError`.

### Pattern 4: Escape-sequence byte decoder (shared by `--action` and TOML)

**What:** A `decode_bytes_escape(s: &str) -> Result<Vec<u8>, EscapeError>` that handles `\r \n \t \0 \\ \xNN` and passes other bytes through as UTF-8.

**When to use:** TOML `Action.bytes` field; CLI `--action 'label=BYTES'` value parsing.

**Example:**

```rust
// crates/bootroom-core/src/config.rs
pub fn decode_bytes_escape(s: &str) -> Result<Vec<u8>, EscapeError> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            return Err(EscapeError::TrailingBackslash { pos: i });
        }
        match bytes[i + 1] {
            b'r'  => { out.push(0x0d); i += 2; }
            b'n'  => { out.push(0x0a); i += 2; }
            b't'  => { out.push(0x09); i += 2; }
            b'0'  => { out.push(0x00); i += 2; }
            b'\\' => { out.push(b'\\'); i += 2; }
            b'x' => {
                if i + 3 >= bytes.len() {
                    return Err(EscapeError::ShortHex { pos: i });
                }
                let h1 = hex_digit(bytes[i + 2]).ok_or(EscapeError::BadHex { pos: i + 2 })?;
                let h2 = hex_digit(bytes[i + 3]).ok_or(EscapeError::BadHex { pos: i + 3 })?;
                out.push((h1 << 4) | h2);
                i += 4;
            }
            other => return Err(EscapeError::UnknownEscape { pos: i + 1, char: other as char }),
        }
    }
    Ok(out)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
```

**CLI integration via clap custom value parser:**

```rust
// crates/bootroom/src/cli.rs
#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    // ... existing fields ...

    /// Define an ad-hoc action without editing config.
    /// Format: `--action 'label=BYTES'` where BYTES accepts \r \n \t \0
    /// \\ \xNN escapes. Repeatable. Overrides config-file actions on
    /// label collision (last --action wins per CONTEXT.md D-02).
    #[arg(long = "action", value_name = "LABEL=BYTES",
          action = clap::ArgAction::Append,
          value_parser = parse_cli_action)]
    pub actions: Vec<CliAction>,

    /// Path to bootroom.toml. Default: ./bootroom.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CliAction { pub label: String, pub bytes: Vec<u8> }

fn parse_cli_action(s: &str) -> Result<CliAction, String> {
    let (label, rest) = s.split_once('=')
        .ok_or_else(|| format!("--action: expected 'label=BYTES', got '{s}'"))?;
    if label.is_empty() {
        return Err("--action: empty label".to_string());
    }
    let bytes = bootroom_core::config::decode_bytes_escape(rest)
        .map_err(|e| format!("--action {label}: {e}"))?;
    Ok(CliAction { label: label.to_string(), bytes })
}
```

### Pattern 5: `/api/config` JSON projection — pre-decoded bytes via base64

**What:** Server projects `LoadedConfig` to a JSON shape that includes decoded bytes as base64 strings (NOT escape sequences) so the browser never re-implements the decoder. Groups are derived (collected in TOML insertion order).

**When to use:** Both the initial `GET /api/config` and every `WsMessage::ConfigUpdate { config: serde_json::Value }` payload.

**Shape:**

```json
{
  "schema_version": 1,
  "actions": [
    {
      "label": "reboot",
      "bytes_b64": "cmVib290DQ==",
      "group": "Boot",
      "description": "Send reboot command to the guest shell"
    },
    {
      "label": "panic_inject",
      "bytes_b64": "AwMD",
      "group": "Diagnostics",
      "description": null
    }
  ],
  "scenarios": [
    {
      "name": "boot_smoke",
      "actions": ["reboot"],
      "assertions": [],
      "timeout_ms": 30000
    }
  ]
}
```

**Example handler:**

```rust
// crates/bootroom/src/api_config.rs
use axum::{extract::State, Json};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{json, Value};

pub async fn api_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    let loaded = state.loaded_config.read().await;
    Json(project_loaded(&loaded))
}

fn project_loaded(loaded: &LoadedConfig) -> Value {
    json!({
        "schema_version": 1,
        "actions": loaded.actions().iter().map(|a| json!({
            "label": a.label,
            "bytes_b64": STANDARD.encode(&a.bytes_decoded),
            "group": a.group,
            "description": a.description,
        })).collect::<Vec<_>>(),
        "scenarios": loaded.scenarios().iter().map(|s| json!({
            "name": s.name,
            "actions": s.actions,
            "assertions": s.assertions,
            "timeout_ms": s.timeout_ms,
        })).collect::<Vec<_>>(),
    })
}
```

**Note:** Re-enable `base64.workspace = true` in `crates/bootroom/Cargo.toml` (Phase 2 removed it per WR-02; Phase 3's `/api/config` projection needs it). The workspace declaration is still in root `Cargo.toml` so the change is one line.

### Pattern 6: Browser banner priority resolver

**What:** A single synchronous `resolveBanners()` JS function runs after every state mutation that could affect banner visibility. Enforces iso > config-invalid > kernel-fresh ladder. Sets `hidden` attribute on the lower-priority banners when a higher one is shown.

**When to use:** Initial `/api/config` fetch completion; every `KernelChanged` / `ConfigUpdate` / `ConfigInvalid` WS frame; dismiss-click; inline-LAUNCH-click.

**Example:**

```javascript
// crates/bootroom/web/app.js (new section)
const banners = {
  iso:    document.getElementById('iso-banner'),
  config: document.getElementById('config-banner'),
  fresh:  document.getElementById('fresh-banner'),
};
const bannerState = {
  configInvalid: null,   // { error, line, col } | null
  freshKernel:   null,   // { ok, reason } | null  (null = dismissed/none)
};

function resolveBanners() {
  // iso-banner is owned by the inline SAB probe; we only enforce
  // mutual exclusion. If it's not [hidden], force-hide the others.
  const isoActive = !banners.iso.hasAttribute('hidden');
  const configActive = bannerState.configInvalid !== null && !isoActive;
  const freshActive  = bannerState.freshKernel !== null && !isoActive && !configActive;

  banners.config.hidden = !configActive;
  banners.fresh.hidden  = !freshActive;
}
```

### Pattern 7: Funnel `lockInput()` / `unlockInput()` — defined but not consumed in Phase 3

**What:** Add `locked: boolean` to `Funnel`; `lockInput()` sets true and sets pill to `BUSY` + adds `disabled` to every `.action-btn`. `enqueue()` short-circuits when locked. Server-initiated `SerialIn` frames bypass the lock (they're already in `funnel.enqueue` via the WS branch; CONTEXT decision is "server-initiated scenarios are the reason the lock exists").

**Wait — important nuance:** CONTEXT says "WS `SerialIn` STILL flows through." So `enqueue()` itself must NOT short-circuit. The lock must be enforced by **the caller**:

- `xterm.onData` callback checks `funnel.locked` before calling `funnel.enqueue` for user keystrokes.
- Action button click handlers check `funnel.locked` before calling `funnel.enqueue`.
- The WS `SerialIn` branch calls `funnel.enqueue` unconditionally — bypassing the lock by design.

**Example:**

```javascript
// crates/bootroom/web/funnel.js — additions to existing Funnel class
export class Funnel {
  constructor(slave, ldisc) {
    // ... existing ...
    this.locked = false;
  }
  lockInput()   { this.locked = true;  onLockChange(true);  }
  unlockInput() { this.locked = false; onLockChange(false); }
}

// onLockChange is supplied by app.js (which owns the pill + button DOM).
let _onLockChange = () => {};
export function setLockObserver(cb) { _onLockChange = cb; }
function onLockChange(v) { _onLockChange(v); }
```

```javascript
// crates/bootroom/web/app.js
import { Funnel, setLockObserver, ... } from './funnel.js';
const funnel = new Funnel(slave, master.ldisc);
setLockObserver((locked) => {
  if (locked) {
    setPill('BUSY');
    document.querySelectorAll('#actions-panel .action-btn')
      .forEach(b => b.disabled = true);
  } else {
    document.querySelectorAll('#actions-panel .action-btn')
      .forEach(b => b.disabled = false);
    recomputePillLocal();
  }
});

// xterm input is now lock-aware:
xterm.onData((data) => {
  if (funnel.locked) return;            // <-- new
  const bytes = new TextEncoder().encode(data);
  if (bytes.length > 0) funnel.enqueue(bytes, { pacingMs: 0 });
});

// Action button click — lock-aware:
function onActionBtnClick(btn) {
  if (funnel.locked) return;             // <-- new
  const bytes = b64ToBytes(btn.dataset.bytesB64);
  funnel.enqueue(bytes, { pacingMs: 15 });
}
```

**Phase 3 manual exercise (per UI-SPEC Interaction Contract 9):** paste `funnel.lockInput()` into DevTools console with a TOML that defines at least one action; confirm pill flips to BUSY and `.action-btn[disabled]` renders correctly; confirm `funnel.unlockInput()` reverses both.

### Pattern 8: `bootroom check` and `bootroom init` exit codes + stdout/stderr split

**`check`:**

```rust
// crates/bootroom/src/check_cmd.rs
pub fn run(args: CheckArgs) -> std::process::ExitCode {
    let path = args.config.unwrap_or_else(|| PathBuf::from("bootroom.toml"));
    let bytes = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("{}: file not found", path.display());
            return std::process::ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("{}: {}", path.display(), e);
            return std::process::ExitCode::from(2);
        }
    };
    match LoadedConfig::load_from_str(&bytes) {
        Ok(loaded) => {
            println!("{}: ok ({} actions, {} scenarios)",
                path.display(), loaded.actions().len(), loaded.scenarios().len());
            std::process::ExitCode::SUCCESS
        }
        Err(e) if e.is_schema_version_mismatch() => {
            eprintln!("{}: schema_version mismatch (expected 1, got {})",
                path.display(), e.actual_version());
            std::process::ExitCode::from(3)
        }
        Err(e) => {
            // Span-aware:
            match (e.line, e.col) {
                (Some(l), Some(c)) => eprintln!("{}:{}:{}: {}", path.display(), l, c, e.message),
                _ => eprintln!("{}: {}", path.display(), e.message),
            }
            std::process::ExitCode::from(1)
        }
    }
}
```

**`init`:**

```rust
// crates/bootroom/src/init_cmd.rs
const EXAMPLE: &str = include_str!("../assets/bootroom-example.toml");

pub fn run(args: InitArgs) -> std::process::ExitCode {
    let path = PathBuf::from("bootroom.toml");
    if path.exists() && !args.force {
        eprintln!("bootroom.toml already exists; pass --force to overwrite.");
        return std::process::ExitCode::from(1);
    }
    if let Err(e) = std::fs::write(&path, EXAMPLE) {
        eprintln!("failed to write {}: {}", path.display(), e);
        return std::process::ExitCode::from(1);
    }
    println!("Wrote ./bootroom.toml");
    std::process::ExitCode::SUCCESS
}
```

### Anti-Patterns to Avoid

- **Two separate `Debouncer` instances** — Doubles inotify handles, complicates ownership of `broadcast::Sender`. Single pool + `.watch()` twice is correct.
- **Auto-reloading the page on `KernelChanged`** — Pitfall #4 explicit: watcher is a *hint*, not a trigger. Only the user (or a future scenario) initiates Launch.
- **Sending action bytes over WS to the server, server proxies to guest** — CONTEXT D-02 explicitly rejects this. Direct browser funnel write. Server is observer-only via existing SerialOut mirror.
- **Treating `KernelChanged { ok: false }` as an error banner (red)** — UI-SPEC explicit: this is a *warning*; background stays `--surface` (informational). Red is reserved for `#config-banner` and `#iso-banner`.
- **`#[serde(default)]` without `deny_unknown_fields`** — Pitfall #8: typos silently swallowed. The CONTEXT struct definitions already get this right; verify the planner doesn't loosen them.
- **Hashing-based config "diff" in `ConfigUpdate`** — CONTEXT `<deferred>`: full replacement now; diff is a profiling-driven optimization.
- **Per-action `pacing_ms` field in Phase 3** — CONTEXT `<deferred>`: schema_version=2 bump.
- **`include_str!("./bootroom-example.toml")` in `init_cmd.rs` directory** — Path is relative to the .rs file's directory by Cargo convention; storing under `crates/bootroom/assets/bootroom-example.toml` keeps it discoverable but the macro path is `../assets/...`. Alternative: just inline a 25-line `const EXAMPLE: &str = "..."` literal.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| FS event debouncing | A custom 300ms timer + event coalescer | `notify-debouncer-full = "0.7"` | Pitfall #4. Handles rename pairs (`paths[0]`=src, `paths[1]`=dst), preserves event ordering, MSRV-matched. |
| Atomic-rename detection | `inotify_add_watch` with IN_MOVED_TO masking | `notify-debouncer-full` parent-dir watch | Cross-platform (notify abstracts inotify/FSEvents/ReadDirectoryChangesW); CONTEXT WCH-02 calls for atomic-rename safe. |
| TOML parsing | `nom`/`pest`/regex over the file | `toml = "1.1"` | Spec-conformant; `Error::span()` gives byte offsets; serde-integrated. |
| TOML error span → line/col | (your own newline-counting if avoidable) | The 7-line helper in Pattern 3 above. There is genuinely no built-in `line_col()` in `toml` 1.x. | Verified via docs.rs — `span()` returns `Option<Range<usize>>` only. Manual conversion is the canonical approach; keep it tiny and tested. |
| Base64 encode/decode | `String::from_utf8(buf)`-style ad-hoc | `base64::engine::general_purpose::STANDARD` (already in workspace deps; re-enable in `crates/bootroom/Cargo.toml`) | Round-trips bytes 0x00-0xFF; what the browser's native `atob` consumes. |
| Per-WS-conn fan-out | `Arc<Mutex<Vec<Sender>>>` + manual broadcast | `tokio::sync::broadcast::channel(16)` | Bounded by design; per-receiver `Lagged(n)` signal lets each connection make its own catchup decision (we choose `continue`). |
| TOML insertion-order preservation | `BTreeMap<String, Action>` (sorted, NOT insertion) | `Vec<Action>` with TOML `[[action]]` arrays | TOML arrays-of-tables are positionally ordered by spec; serde `Vec<T>` honors it. |
| CLI subcommand parsing | Hand-roll a `match args[0]` | `clap` derive `#[command(subcommand)]` | We already use clap derive; the migration from one-arm `Cmd::Serve` to three-arm is mechanical. |
| Escape-sequence byte decode | Embed the Rust `\xNN` parser | The 30-line helper in Pattern 4 above | The Rust string-literal parser is private to `rustc`; rolling our own is the standard approach. Keep it tight and unit-tested. |
| File "is ELF?" sniff | Custom magic table | `if first4 == [0x7f, b'E', b'L', b'F']` | Four bytes. No crate needed. |

**Key insight:** Phase 3 has zero greenfield "hard problems" — every primitive is a mature crate or a 10-line idiomatic helper. The risk is in the **wiring** (single Debouncer, broadcast fan-out, banner priority resolver) — not in any individual mechanism.

## Runtime State Inventory

> Phase 3 is greenfield additive work, NOT a rename/refactor. Section is included for completeness; nothing requires migration.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — no datastore in Phase 3 | None |
| Live service config | None — bootroom is a single-process dev tool | None |
| OS-registered state | None | None |
| Secrets/env vars | None | None |
| Build artifacts | None — Phase 3 adds new modules, doesn't rename anything. `bootroom-core`'s `WsMessage` enum **gains** 3 variants; the new variants are tagged so existing `WsMessage` consumers continue to compile (the absence of `#[serde(deny_unknown_fields)]` on `WsMessage` per Phase 2 02-01 plan was explicitly to allow this). | Document the additive-variant policy in the `WsMessage` doc comment when new variants land; no migration. |

## Common Pitfalls

### Pitfall 1: Single missing path-canonicalization makes the demux miss its own config edits

**What goes wrong:** The watcher canonicalizes `bootroom.toml` to `/home/user/proj/bootroom.toml` at startup. The user runs `bootroom serve --config ./bootroom.toml` from a different CWD; the comparison path uses the user-supplied relative path; `notify` reports events with absolute paths; the comparison fails; edits silently don't reload.

**Why it happens:** `notify` always reports canonical absolute paths in `event.paths`. The comparison path must match.

**How to avoid:** Canonicalize both `config_path` and `kernel_path` once at startup via `std::fs::canonicalize`. Store the canonical form in `AppState` and pass it to the watcher. Reject startup if canonicalize fails (file doesn't exist yet → friendlier error than "watcher silently does nothing").

**Warning signs:** `bootroom check && bootroom serve` works; live-edit of TOML produces no UI update; no error in logs.

### Pitfall 2: `notify-debouncer-full` callback runs on a non-tokio thread; `tokio::spawn` inside it panics

**What goes wrong:** The callback closure passed to `new_debouncer` runs on the debouncer's own OS thread (not a tokio runtime). Calling `tokio::spawn(async { ... })` inside it panics: "there is no reactor running."

**Why it happens:** `Debouncer` owns its own thread + `Watcher` (the underlying inotify/FSEvents handle); the callback is invoked from that thread.

**How to avoid:** Capture a `tokio::runtime::Handle` (via `Handle::current()` at watcher-spawn time, while still in tokio context) and pass it into the closure: `handle.spawn(async { ... })`. OR keep the callback body fully synchronous and use `broadcast::Sender::send` (which is sync; succeeds when at least one receiver exists, returns count of receivers; no `.await` needed). The patterns above use the latter.

**Warning signs:** Test mode (`#[tokio::test]`) works because there's an ambient runtime; release `bootroom serve` panics with "there is no reactor running" the first time the watcher fires.

### Pitfall 3: `broadcast::Sender::send` succeeds with zero receivers — silent dropped frames before first WS connect

**What goes wrong:** Watcher fires before any WS connection has been opened. `tx.send(msg)` returns `Err(SendError(msg))` — there are no receivers, the message is dropped. Watcher logs nothing because the convention is to ignore that error.

**Why it happens:** broadcast channels with no receivers can't buffer (the buffer is per-receiver, ring-buffered backwards). New subscribers via `tx.subscribe()` get only **future** messages, not the back-buffer.

**How to avoid:** This is actually acceptable for Phase 3 — the **browser fetches `/api/config` on connect**, so it gets the current truth via HTTP, not via a missed broadcast. `KernelChanged` similarly: if the user has no browser open, no banner can be shown. The pattern is "broadcast is fire-and-forget; first source of truth is HTTP for both config and `/api/kernel/info`."

**Warning signs:** "Why didn't my fresh-kernel banner show?" answered by "you weren't connected yet, but reload now and you'll see the current kernel info."

### Pitfall 4: `make` produces partial writes; the watcher fires while the file is still being written

**What goes wrong:** Build pipeline writes the kernel in chunks; the debouncer fires after 300ms idle, but `make` is still flushing. ELF magic check passes (header was written first), `bootroom` broadcasts `KernelChanged`, user clicks Launch, partial kernel boots and hangs.

**Why it happens:** ELF magic check alone is necessary but not sufficient. CONTEXT WCH-03 calls for size-stability across debounce ticks.

**How to avoid:** In `handle_kernel_change`:
1. Read file size as `s1`.
2. Sleep 100ms (sync sleep in the callback thread — keeps the watcher off the tokio runtime).
3. Read file size as `s2`.
4. Only broadcast if `s1 == s2 && s1 >= 4 && first_4_bytes == [0x7f, b'E', b'L', b'F']`.
5. If unstable, log + skip; the next debounce tick will retry.

**Code:**
```rust
fn handle_kernel_change(path: &Path, state: &AppState, tx: &broadcast::Sender<WsMessage>) {
    let s1 = match std::fs::metadata(path).map(|m| m.len()) {
        Ok(s) => s,
        Err(_) => return, // file gone — wait for the next event
    };
    std::thread::sleep(Duration::from_millis(100));
    let s2 = match std::fs::metadata(path).map(|m| m.len()) {
        Ok(s) => s,
        Err(_) => return,
    };
    if s1 != s2 {
        tracing::debug!("kernel size unstable ({s1} -> {s2}); deferring");
        return;
    }
    // ELF magic
    let mut magic = [0u8; 4];
    use std::io::Read;
    let ok = std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .map(|_| magic == [0x7f, b'E', b'L', b'F'])
        .unwrap_or(false);
    if !ok {
        let _ = tx.send(WsMessage::KernelChanged {
            ok: false, mtime: 0, size: s1, sha256_prefix: String::new(),
            reason: Some("not ELF".into()),
        });
        return;
    }
    // mtime + sha256 prefix (can reuse the digest_cache from AppState).
    // Broadcast KernelChanged { ok: true, ... }.
}
```

**Warning signs:** Intermittent "qemu-wasm aborts very early at boot" after `make` completes; works when `sleep 1` is inserted before clicking Launch.

### Pitfall 5: TOML with `[[action]]` and `[[scenario]]` only — empty arrays must be tolerated

**What goes wrong:** `serde(deny_unknown_fields)` on `Config` PLUS the absence of `actions` / `scenarios` in a fresh `bootroom init` template makes the parser reject it.

**Why it happens:** With `#[serde(default, rename = "action")]` on the `Config.actions` field, missing entries serialize to `Vec::new()`. Without `default`, deserializing a `Config { schema_version = 1 }` alone fails with "missing field `action`".

**How to avoid:** Keep `#[serde(default, rename = "action")]` on both `actions` and `scenarios`. The CONTEXT struct sample already has this — verify the planner preserves it.

**Warning signs:** `bootroom init && bootroom check` fails with "missing field action".

### Pitfall 6: Action button DOM rebuild on `ConfigUpdate` loses focus / scrollbar position

**What goes wrong:** `actionsPanel.replaceChildren(...buildFromConfig(config))` blows away the live DOM; if the user had focused an action button when the TOML changed, focus moves to `<body>`; if they had scrolled the panel partway down, scroll resets to 0.

**Why it happens:** Synchronous wholesale replace, no preservation of UI state.

**How to avoid:** Accept this in Phase 3 — UI-SPEC explicitly says "no animation / transition on action panel re-render — `replaceChildren` is synchronous + flat." If it becomes a real complaint, a focused-button-label key-based preservation is a one-helper polish item (Phase 4+ if needed).

**Warning signs:** User feedback "the panel jumps when I save the TOML."

### Pitfall 7: `Box::leak(debouncer)` masks the watcher silently dying if the OS thread panics

**What goes wrong:** If the debouncer's internal thread panics (e.g., the watched parent dir gets `unlink`'d), `Box::leak` keeps the heap allocation alive but the inotify handle is gone. No further events fire; logs say nothing.

**Why it happens:** `Debouncer` doesn't expose a "is the underlying watcher alive?" probe.

**How to avoid:** Treat this as acceptable for the dev tool — Phase 3 only. The recovery path is "restart `bootroom serve`." Log loudly in the callback's `Err(errs)` branch so OS-level errors at least surface. If watcher reliability becomes a real problem, the post-MVP refactor stores the `Debouncer` in `AppState` and re-creates it on a watchdog timer.

**Warning signs:** UI stops getting `ConfigUpdate` after the user `rm -rf`s and re-creates the kernel directory; silent until `bootroom serve` is restarted.

### Pitfall 8: `WsMessage` variant ordering / serde discriminant changes break Phase 2's existing tests

**What goes wrong:** Adding `KernelChanged { ok: bool, ... }` to the enum changes nothing about Phase 2's `SerialIn`/`SerialOut`/`State`/`Launch`/`Reset`/`Hello` round-trip tests — those test by tag string, not by variant ordinal. BUT the wire shape **is** sensitive to field types: `mtime: i64` serializes to a JSON number; if anyone (Phase 4 future) deserializes into a stricter type (e.g., `u32`), values larger than 2^31 silently fail.

**Why it happens:** `#[serde(tag = "type")]` is robust to additions but each variant's payload is its own subschema.

**How to avoid:** New variants use `i64` for timestamps (Unix epoch seconds; safe through year 292,277,026,596), `u64` for sizes (file size on any FS), `String` for sha256 prefix (12 hex chars = 12 ASCII bytes), `bool` for the ELF acceptance flag. Phase 4 reuses unchanged.

### Pitfall 9: clap subcommand refactor breaks `serve_no_open.rs` test (subprocess call shape changes)

**What goes wrong:** `tests/serve_no_open.rs` spawns `CARGO_BIN_EXE_bootroom serve --kernel ... --no-open ...`. The refactor preserves `serve` as the subcommand verb so the existing test continues to pass. BUT if someone reorders or renames the subcommand, the test silently times out.

**Why it happens:** clap's `--help` test catches the rename; the subprocess test doesn't.

**How to avoid:** Keep `Cmd::Serve(ServeArgs)` as the first variant; verify the existing Phase 2 subprocess test passes UNCHANGED after the refactor. Add a `bootroom --help` snapshot test that asserts the three subcommands are present.

## Code Examples

### Example A: `LoadedConfig` — wrapper with cross-validation + decoded bytes

```rust
// crates/bootroom-core/src/config.rs
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    actions: Vec<ResolvedAction>,
    scenarios: Vec<Scenario>,
    actions_by_label: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAction {
    pub label: String,
    pub bytes_decoded: Vec<u8>,
    pub group: Option<String>,
    pub description: Option<String>,
}

impl LoadedConfig {
    pub fn load_from_str(s: &str) -> Result<Self, LoadError> {
        let cfg: Config = parse_str(s)?;
        Self::from_config(cfg, /* cli_actions = */ &[])
    }

    pub fn load_from_str_with_overrides(s: &str, cli: &[CliAction]) -> Result<Self, LoadError> {
        let cfg: Config = parse_str(s)?;
        Self::from_config(cfg, cli)
    }

    fn from_config(cfg: Config, cli_overrides: &[CliAction]) -> Result<Self, LoadError> {
        if cfg.schema_version != 1 {
            return Err(LoadError::schema_mismatch(cfg.schema_version));
        }
        let mut actions: Vec<ResolvedAction> = Vec::new();
        for a in &cfg.actions {
            let bytes = decode_bytes_escape(&a.bytes).map_err(|e| LoadError {
                message: format!("action {}: {}", a.label, e),
                line: None, col: None,
            })?;
            actions.push(ResolvedAction {
                label: a.label.clone(),
                bytes_decoded: bytes,
                group: a.group.clone(),
                description: a.description.clone(),
            });
        }
        // Apply CLI overrides (last --action wins; --action overrides config).
        for cli in cli_overrides {
            if let Some(existing) = actions.iter_mut().find(|x| x.label == cli.label) {
                existing.bytes_decoded = cli.bytes.clone();
            } else {
                actions.push(ResolvedAction {
                    label: cli.label.clone(),
                    bytes_decoded: cli.bytes.clone(),
                    group: None,
                    description: None,
                });
            }
        }
        // Uniqueness check (after override merge).
        let mut by_label = std::collections::HashMap::new();
        for (i, a) in actions.iter().enumerate() {
            if by_label.insert(a.label.clone(), i).is_some() {
                return Err(LoadError::duplicate_action(a.label.clone()));
            }
        }
        // Cross-validate scenarios.
        for s in &cfg.scenarios {
            for refed in &s.actions {
                if !by_label.contains_key(refed) {
                    return Err(LoadError::unknown_action_ref(s.name.clone(), refed.clone()));
                }
            }
        }
        Ok(Self {
            actions,
            scenarios: cfg.scenarios,
            actions_by_label: by_label,
        })
    }

    pub fn actions(&self) -> &[ResolvedAction] { &self.actions }
    pub fn scenarios(&self) -> &[Scenario] { &self.scenarios }
}
```

### Example B: `init`-generated `bootroom-example.toml`

```toml
# bootroom.toml — bootroom test harness configuration.
# https://github.com/sandwich-farm/bootroom

schema_version = 1

# Action buttons appear in the UI in the order declared below.
# `bytes` accepts C-style escapes: \r \n \t \0 \\ \xNN.
[[action]]
label = "reboot"
bytes = "reboot\r"
group = "Boot"
description = "Send reboot command to the guest shell"

[[action]]
label = "ctrlc"
bytes = "\x03"
group = "Diagnostics"
description = "Send Ctrl-C to the foreground process"

# Scenarios are scripted action sequences with assertions.
# Phase 3 ships scenario *definitions*; the engine that runs them
# lands in Phase 4.
[[scenario]]
name = "boot_smoke"
actions = ["reboot"]
timeout_ms = 30000

  [[scenario.assert]]
  kind = "contains"
  pattern = "login: "
  after = "reboot"
  timeout_ms = 5000
```

(~25 lines; meets CONTEXT.md spec.)

### Example C: WS broadcast forwarder integrated into existing `handle_socket`

(See Pattern 2 above — the forwarder is `tokio::spawn`'d alongside the existing writer task; the existing reader loop is untouched.)

### Example D: TOML span → line/col conversion

```rust
fn offset_to_line_col(input: &str, byte_off: usize) -> Option<(u32, u32)> {
    if byte_off > input.len() { return None; }
    let prefix = &input[..byte_off];
    let line = (prefix.matches('\n').count() as u32) + 1;
    let col = match prefix.rfind('\n') {
        Some(nl) => (input[nl + 1 .. byte_off].chars().count() as u32) + 1,
        None     => (prefix.chars().count() as u32) + 1,
    };
    Some((line, col))
}

#[test]
fn span_to_line_col() {
    let s = "schema_version = 1\n[[action]]\nlable = \"reboot\"\n";
    //       0123456789012345678 901234567890123 4
    //       0         1                  2          3
    // byte offset 31 is the start of "lable" — line 3 col 1.
    assert_eq!(offset_to_line_col(s, 31), Some((3, 1)));
}
```

### Example E: clap derive Cmd enum refactor

```rust
// crates/bootroom/src/cli.rs
#[derive(Debug, Parser)]
#[command(name = "bootroom", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Start the local HTTP server and serve the qemu-wasm UI.
    Serve(ServeArgs),
    /// Parse and validate bootroom.toml without starting the server.
    Check(CheckArgs),
    /// Write a starter bootroom.toml to the current directory.
    Init(InitArgs),
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    #[arg(long, value_name = "PATH")]
    pub kernel: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 8765)]
    pub port: u16,
    #[arg(long, value_name = "PATH")]
    pub assets_dir: Option<PathBuf>,
    #[arg(long)]
    pub no_open: bool,
    /// Path to bootroom.toml; default = ./bootroom.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Ad-hoc action override: 'label=BYTES'. Repeatable. Escapes: \r \n \t \0 \\ \xNN.
    #[arg(long = "action", value_name = "LABEL=BYTES",
          action = clap::ArgAction::Append,
          value_parser = parse_cli_action)]
    pub actions: Vec<CliAction>,
}

#[derive(Debug, Args, Clone)]
pub struct CheckArgs {
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
pub struct InitArgs {
    /// Overwrite an existing bootroom.toml.
    #[arg(long)]
    pub force: bool,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `toml = "0.8"` (Phase 1 STACK.md) | `toml = "1.1"` | toml 1.0 released early 2025 | `Error::span()` returns byte range; the API is now stable. Migration is a Cargo.toml bump. |
| `notify-debouncer-full = "0.5"` (Phase 1 STACK.md) | `notify-debouncer-full = "0.7"` | 0.7 released 2025 with MSRV 1.85 (matches our floor) | Cleaner API, `DebounceEventResult` ergonomic Vec, single-pool multi-watch confirmed. |
| Raw `notify` events with hand-rolled timer | `notify-debouncer-full` | Always — Pitfall #4 mitigation | Phase 1 RESEARCH already locked this; Phase 3 is the first real consumer. |
| Per-WS state via `Arc<Mutex<Vec<Sender>>>` | `tokio::sync::broadcast::channel` | Tokio 1.0 era; broadcast has been stable for years | Bounded by design; per-receiver Lagged signal handled per-conn. |
| WS message variants forbidden to extend (`deny_unknown_fields` on enum) | Variants extend additively (Phase 2 explicitly omitted the deny attribute on `WsMessage`) | Phase 2 02-01 plan | Phase 3 lands 3 new variants with no Phase 2 churn. |

**Deprecated/outdated for Phase 3:**
- The CONTEXT canonical_refs line "notify-debouncer-full: https://docs.rs/notify-debouncer-full" — verified at 0.7.0 (latest stable; 0.8.0-rc.2 is RC).
- xterm-pty `slave.write` as the action injection path (in ROADMAP success criterion 2) — already superseded by Phase 2 CR-01; CONTEXT calls this out and 03-SUMMARY will update the ROADMAP wording per CONTEXT instruction.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `notify-debouncer-full = "0.7"` is the right pin (vs. `0.8.0-rc.2`). | Standard Stack | `[ASSUMED]` — based on "prefer non-RC for production." If the 0.8 RC stabilizes during planning, the planner can opt-in; the API surface used in Pattern 1 is similar enough that the migration is a Cargo.toml bump. |
| A2 | `notify = "8"` is correct (vs. `9.0-rc.4`). | Standard Stack | `[ASSUMED]` — `notify-debouncer-full = "0.7"` transitively pins notify 8. Verify with `cargo tree` after the dep add. |
| A3 | The size-stability window is 100ms (between two reads inside the watcher callback). | Pitfall #4 | `[ASSUMED]` — CONTEXT.md says "two debounce ticks"; the planner may choose a different sleep value. 100ms is a reasonable default; if `make` writes are slow (large kernel + slow FS), the next debounce tick (300ms later) gets the second chance. |
| A4 | `broadcast::Sender::send` returning `Err(SendError)` (no receivers) is safely ignorable. | Pitfall #3 | `[VERIFIED via tokio docs]` — the doc says receivers see only future messages; zero-receiver send is expected when no clients connected. |
| A5 | `Box::leak(debouncer)` is acceptable for Phase 3 lifetime. | Pattern 1 | `[ASSUMED]` — bootroom is a single-process dev tool that exits on Ctrl-C; no graceful-shutdown contract exists. The planner may prefer storing the Debouncer in `AppState`. |
| A6 | The browser's banner-priority resolver runs synchronously without races. | Pattern 6 | `[VERIFIED via JS semantics]` — JS event loop is single-threaded; WS `onmessage` handlers run to completion before the next event. |
| A7 | `master.ldisc.flow` is `false` initially and only flips via XOFF — relevant only to the **OUTPUT** path (`writeFromUpper`), not the funnel's input path. | Pattern 7 / funnel.lockInput | `[VERIFIED via Phase 2 review WR-05]` — CR-01 fix moved the funnel to `writeFromLower` which has no `flowActivated` guard. The `lockInput` API doesn't change byte semantics; it short-circuits at the caller. |
| A8 | The 25-line `bootroom-example.toml` for `init` can be `include_str!`'d from `crates/bootroom/assets/bootroom-example.toml`. | Pattern 8 / Example B | `[ASSUMED]` — alternative is to inline as a `const &str` literal. Either is fine. The planner picks. |

**Discuss-phase / planner action items:** None — the watcher size-stability window (A3) and the Box::leak vs. AppState-stored Debouncer (A5) are both Claude's-discretion per CONTEXT (those choices are not locked in the `<decisions>` block). The other assumptions are either verified or safe defaults.

## Open Questions

1. **Does the watcher receive events for the kernel parent directory if the kernel file doesn't exist at startup?**
   - What we know: `notify` requires the watched path to exist when `.watch()` is called.
   - What's unclear: If `--kernel /tmp/Image` doesn't exist yet, `--kernel` validation in `server::run` already rejects startup (`anyhow::bail!("--kernel: file not found ...")`). So the watcher is only spawned when the file (and parent) exists. ✓ Closed by existing Phase 1 behavior.

2. **Should `bootroom check` exit code 3 (schema version mismatch) be distinct, or fold into exit 1?**
   - What we know: CONTEXT.md `<specifics>` explicitly lists 0/1/2/3 with 3 for "schema_version mismatch."
   - What's unclear: Nothing — implement as specified.

3. **What happens if the user `chmod a-r bootroom.toml` while `serve` is running?**
   - What we know: The watcher will still fire `Modify(Metadata(Permissions))` events; the subsequent `LoadedConfig::reload` will fail with `std::io::Error::PermissionDenied`.
   - What's unclear: Should this surface as `ConfigInvalid { error: "permission denied", ... }`? Recommendation: yes — same broadcast variant, semantic-error message (no line/col).

4. **CFG-09 says "preserved from TOML insertion order" — does the `Vec<Action>` survive `serde_json::to_value(loaded.actions())` for `/api/config`?**
   - What we know: `Vec<T>` serializes to a JSON array preserving Rust-side ordering; the projection helper iterates in order.
   - What's unclear: Nothing — Vec preserves; verified.

5. **The `bootroom.toml` MAY live OUTSIDE the kernel parent directory. Does the single Debouncer handle disjoint watch trees?**
   - What we know: Yes — `notify` supports independent watches; `Debouncer::watch` is called separately per path.
   - What's unclear: Whether watching `/tmp/kernel_dir` and `~/proj/bootroom.toml` produces interleaved events from two distinct inotify watches. Per `notify` docs, this is supported. The demux by path handles it correctly.

6. **Should `--action` overrides be in scope of `bootroom check`?**
   - What we know: `check` is config-validation; CLI overrides are runtime concerns.
   - Recommendation: `bootroom check` validates the **file only** (no CLI overrides). Document in `--help`. Reduces test surface and matches Pitfall #8 prevention (the file is the source of truth that's version-controlled).

## Environment Availability

> All Phase 3 dependencies are crates.io Rust packages; nothing external is required at runtime beyond what Phase 1+2 already need (Chrome/Chromium for headed dev; nothing for `serve`).

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` ≥ 1.85 | All Phase 3 work | ✓ | 1.90 (per Phase 1 RESEARCH) | — |
| `notify-debouncer-full 0.7` | watcher.rs | ✓ (via crates.io) | 0.7.0 | — |
| `notify 8.x` | transitive | ✓ | 8.x | — |
| `toml 1.1` | config parsing | ✓ | 1.1.2 | — |
| `base64 0.22` | /api/config projection | ✓ (already in workspace deps; re-enable in `crates/bootroom/Cargo.toml`) | 0.22 | — |
| Chromium (for headed dev test of #fresh-banner) | UI verification | ✓ (per Phase 1) | — | Manual screenshot review in any modern browser |
| `qemu-wasm` assets (existing) | runtime kernel boot | ✓ (committed) | committed SHA `0ef7b4e` | — |

**Missing dependencies with no fallback:** None.

**Missing dependencies with fallback:** None — Phase 3 is pure additive Rust + JS.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust unit + integration); `node --check` for JS syntax |
| Config file | `Cargo.toml` workspace + per-crate `[dev-dependencies]` |
| Quick run command | `cargo test --workspace --lib` (unit-only, fast) |
| Full suite command | `cargo test --workspace && node --check crates/bootroom/web/{app,funnel}.js && cargo clippy --workspace --all-targets -- -D warnings` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CFG-01 | `--config` path override; CWD `bootroom.toml` default | integration | `cargo test --test config_loading` | ❌ Wave 0 |
| CFG-02 | TOML actions with label, bytes, group | unit | `cargo test -p bootroom-core config::tests::actions_roundtrip` | ❌ Wave 0 |
| CFG-03 | Scenarios reference actions + assertions + timeout | unit | `cargo test -p bootroom-core config::tests::scenarios_parse` | ❌ Wave 0 |
| CFG-04 | `schema_version = 1` required, others rejected | unit | `cargo test -p bootroom-core config::tests::schema_version_rejected` | ❌ Wave 0 |
| CFG-05 | `deny_unknown_fields` rejects typos with line/col | unit | `cargo test -p bootroom-core config::tests::deny_unknown_fields_with_span` | ❌ Wave 0 |
| CFG-06 | Scenario→action ref validation | unit | `cargo test -p bootroom-core config::tests::scenario_unknown_action_ref` | ❌ Wave 0 |
| CFG-07 | `bootroom check` exit codes 0/1/2/3 | integration | `cargo test --test check_subcommand` (subprocess via CARGO_BIN_EXE_bootroom) | ❌ Wave 0 |
| CFG-08 | `bootroom init` writes + refuses overwrite | integration | `cargo test --test init_subcommand` | ❌ Wave 0 |
| CFG-09 | Action button TOML insertion order preserved through `/api/config` | integration | `cargo test --test api_config_endpoint::order_preserved` | ❌ Wave 0 |
| CFG-10 | Live TOML edit → WS ConfigUpdate frame | integration | `cargo test --test watcher_live_reload::toml_reload` (tempfile + write + assert WS frame within 1s) | ❌ Wave 0 |
| ACT-01 | `/api/config` JSON projection shape | integration | `cargo test --test api_config_endpoint::shape_includes_base64_bytes` | ❌ Wave 0 |
| ACT-02 | Button click writes bytes to guest serial | manual (headed) | smoke checklist — bootroom serve + NORN kernel, click a TOML-defined action, observe guest response | n/a |
| ACT-03 | `--action 'label=BYTES'` parsing, repeatable, override semantics | unit + integration | `cargo test -p bootroom -- cli::tests::parse_cli_action` (unit) + `cargo test --test serve_with_cli_action` (integration via /api/config check) | ❌ Wave 0 |
| ACT-04 | `funnel.lockInput()` API present + disables buttons (Phase 3 manual; Phase 4 first real caller) | manual (DevTools) | per UI-SPEC Interaction Contract 9 — paste `funnel.lockInput()` into console, verify pill BUSY + buttons disabled | n/a |
| WCH-01 | `notify-debouncer-full` 300ms debouncing | integration | `cargo test --test watcher_debounce::burst_collapses_to_one_event` (write 5 chunks, assert 1 KernelChanged broadcast) | ❌ Wave 0 |
| WCH-02 | Atomic-rename detection via parent-dir watch | integration | `cargo test --test watcher_atomic_rename::tempfile_rename_fires_kernel_changed` | ❌ Wave 0 |
| WCH-03 | Size-stability gate | integration | `cargo test --test watcher_size_stability::partial_write_held_until_stable` | ❌ Wave 0 |
| WCH-04 | ELF magic byte sniff; non-ELF → warning frame | integration | `cargo test --test watcher_elf_magic::non_elf_yields_ok_false` | ❌ Wave 0 |
| WCH-05 | `KernelChanged` WS frame includes mtime/size/sha256_prefix | integration | `cargo test --test watcher_ws_frame::kernel_changed_payload_shape` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test --workspace --lib` (unit-only — typically completes in < 30s).
- **Per wave merge:** `cargo test --workspace && node --check crates/bootroom/web/{app,funnel}.js && cargo clippy --workspace --all-targets -- -D warnings`.
- **Phase gate:** Full suite green + headed-browser smoke (per ACT-02 / ACT-04) before `/gsd-verify-work`.

### Wave 0 Gaps

- [ ] `crates/bootroom-core/src/config.rs` — module + unit tests (CFG-02..06)
- [ ] `crates/bootroom/tests/check_subcommand.rs` — subprocess test for `bootroom check` (CFG-07)
- [ ] `crates/bootroom/tests/init_subcommand.rs` — subprocess test for `bootroom init` (CFG-08)
- [ ] `crates/bootroom/tests/api_config_endpoint.rs` — `/api/config` integration (CFG-09, ACT-01)
- [ ] `crates/bootroom/tests/serve_with_cli_action.rs` — `--action` integration (ACT-03)
- [ ] `crates/bootroom/tests/watcher_debounce.rs` — debounce timing (WCH-01)
- [ ] `crates/bootroom/tests/watcher_atomic_rename.rs` — atomic-rename detection (WCH-02)
- [ ] `crates/bootroom/tests/watcher_size_stability.rs` — size-stability (WCH-03)
- [ ] `crates/bootroom/tests/watcher_elf_magic.rs` — ELF sniff (WCH-04)
- [ ] `crates/bootroom/tests/watcher_ws_frame.rs` — KernelChanged payload shape (WCH-05)
- [ ] `crates/bootroom/tests/watcher_live_reload.rs` — TOML edit → ConfigUpdate (CFG-10)
- [ ] `crates/bootroom/assets/bootroom-example.toml` — 25-line example file for `bootroom init` (or inline `const`)

**Test infrastructure that exists and Phase 3 reuses unchanged:**
- `crates/bootroom/tests/common/mod.rs` — `TestServer::spawn` (ephemeral-port axum + auto-abort on drop)
- `write_kernel_tempfile` helper (used by watcher tests for atomic-rename setup)
- `tokio-tungstenite` dev-dep for WS-roundtrip integration tests (already wired for Phase 2)

## Security Domain

> Phase 3 inherits the loopback-only security posture from Phase 1+2. `--host 0.0.0.0` exposes the broader surface; the additions below catalog Phase 3's incremental risk.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | bootroom is loopback-only per PROJECT.md; auth explicitly out of scope |
| V3 Session Management | no | stateless per-connection WS; no session model |
| V4 Access Control | no | single user, single machine |
| V5 Input Validation | yes | `toml::from_str` with `deny_unknown_fields` (struct level) + `decode_bytes_escape` typed errors (CLI level) + `bootroom check` preflight |
| V6 Cryptography | no | no secrets, no signed payloads, no auth tokens — Phase 3 surface adds none |
| V8 Data Protection | yes (minor) | `/api/config` exposes the contents of `bootroom.toml` — any operator who reads-only it has full action coverage; document that the file should not embed secrets |

### Known Threat Patterns for Phase 3 stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious `bootroom.toml` from a checked-out PR (sec hygiene for downstream kernel CI) | Tampering | Document: `bootroom.toml` is treated as trusted input; CI jobs that pull from untrusted PRs should `bootroom check` from a known-good config and not the PR's. |
| Action `bytes` containing arbitrary bytes (`\x00`, `\xff`, etc.) | Tampering | Pass-through by design — the harness IS the byte source. User intent expressed in TOML. |
| TOML parse error consuming unbounded memory | DoS | `toml = "1.1"` is `winnow`-backed; finite memory per input. Document a soft max config size if user reports get suspicious. |
| Watcher fan-out amplifying a single rapid edit into 100 broadcasts | DoS (self) | `notify-debouncer-full` 300ms window collapses bursts. `broadcast::channel(16)` bounds per-receiver backlog. |
| Log-amplification on the `Lagged` warn log in the broadcast forwarder | DoS (self) | Phase 2 review WR-01 already established truncate-for-log + warn-level. Apply the same pattern to `tracing::warn!(skipped, "ws broadcast receiver lagged")` — the `skipped` is a `u64` count, not user-controlled bytes; no amplification surface. ✓ |
| Path traversal via `--config ../../etc/shadow` | Tampering | `--config` is operator-supplied, not user-input; treated as trusted. `std::fs::read_to_string` errors propagate as a `bootroom check` exit-2 / `serve` startup error. |
| ELF magic sniff bypass (header valid, body garbage) | Tampering | Size-stability + ELF magic catches half-flashed; bytes-after-magic are user concern. The fresh-banner is a *hint*; Launch is user-initiated. |

## Sources

### Primary (HIGH confidence)

- `cargo info notify-debouncer-full` — confirms version 0.7.0 stable, 0.8.0-rc.2 RC, MSRV 1.85
- `cargo info notify` — confirms version 8.x stable (notify-debouncer-full 0.7 pins to it transitively); 9.0-rc.4 is RC
- `cargo info toml` — confirms version 1.1.2+spec-1.1.0, MSRV 1.85
- [docs.rs/notify-debouncer-full/0.7.0](https://docs.rs/notify-debouncer-full/0.7.0/notify_debouncer_full/) — `new_debouncer` signature, `DebounceEventResult`, multi-`watch` pattern, callback-on-OS-thread semantics
- [docs.rs/notify-debouncer-full/0.7.0 DebouncedEvent](https://docs.rs/notify-debouncer-full/0.7.0/notify_debouncer_full/struct.DebouncedEvent.html) — Deref to Event, time field
- [docs.rs/notify/8.2/notify/event/struct.Event.html](https://docs.rs/notify/8.2/notify/event/struct.Event.html) — paths field ordering (src first, dst last on rename); kind hierarchy; attrs
- [docs.rs/toml/1.1.2/toml/de/struct.Error.html](https://docs.rs/toml/1.1.2/toml/de/struct.Error.html) — `span() → Option<Range<usize>>`, `message() → &str`, no built-in line/col
- [docs.rs/tokio/latest/tokio/sync/broadcast/](https://docs.rs/tokio/latest/tokio/sync/broadcast/) — bounded channel, Lagged error, subscribe pattern, oldest-dropped semantics
- `.planning/phases/02-websocket-live-serial/02-CONTEXT.md` — WsMessage protocol baseline (Phase 3 extends, doesn't break)
- `.planning/phases/02-websocket-live-serial/02-REVIEW.md` — Phase 2 patterns (truncate-for-log, exception safety, copy-button revert pattern, ENOENT-narrow catches) — Phase 3 follows the same disciplines
- `.planning/phases/03-config-buttons-watcher/03-CONTEXT.md` — locked decisions (struct shapes, watcher subsystem, WS variants, banner ladder)
- `.planning/phases/03-config-buttons-watcher/03-UI-SPEC.md` — visual + interaction contracts (banner priority resolver, BUSY pill, no-empty-state policy)
- `.planning/research/PITFALLS.md` Pitfall #4 (watcher debounce + ELF + size-stability) and Pitfall #8 (TOML schema drift, span errors, deny_unknown_fields)
- `crates/bootroom/web/funnel.js` (Phase 2 source-of-truth for the lock primitive's host module)
- `crates/bootroom/src/cli.rs` (current ServeArgs — refactor target)
- `crates/bootroom/src/ws.rs` (current handle_socket — extension point for broadcast forwarder)

### Secondary (MEDIUM confidence)

- `cargo search` for current crate versions — verified at 2026-05-19
- Phase 1 RESEARCH (`.planning/phases/01-walking-skeleton/01-RESEARCH.md`) — established the COOP/COEP, embedded-assets, and dev-vs-release `--assets-dir` patterns Phase 3 reuses
- Phase 2 RESEARCH — established the tokio mpsc + split-socket pattern that the broadcast forwarder extends

### Tertiary (LOW confidence — flagged for validation during implementation)

- Exact size-stability sleep window (100ms vs 200ms) — Pitfall #4 Assumption A3
- Whether `Box::leak(debouncer)` produces audit-noise complaints in Phase 6 release tooling (cargo-deny) — Pattern 1 Assumption A5

## Metadata

**Confidence breakdown:**
- Standard stack (toml 1.1 / notify-debouncer-full 0.7 / notify 8): HIGH — `cargo info`-verified
- Architecture (watcher.rs ownership, broadcast fan-out, AppState extensions): HIGH — Phase 2 patterns + CONTEXT decisions are extremely specific
- Pitfalls (size-stability, atomic-rename, callback-thread): HIGH — corroborated by Pitfall #4 in `.planning/research/PITFALLS.md`
- TOML span → line/col conversion: HIGH — `toml` 1.x docs explicit on what is and isn't provided
- Browser banner resolver synchronicity: HIGH — JS semantics
- The exact size-stability window value (100ms): MEDIUM — tunable; correct in spirit, exact number is empirical
- `WsMessage` additive variants don't break Phase 2 round-trip tests: HIGH — Phase 2 explicitly omitted `deny_unknown_fields` on the enum per 02-01

**Research date:** 2026-05-19
**Valid until:** 2026-06-18 (30 days — Phase 3 deps are stable; notify-debouncer-full 0.8 may stabilize within this window, in which case the planner should re-verify the migration cost before locking).
