---
phase: "03"
depth: "standard"
status: "findings"
critical_count: 3
warning_count: 9
info_count: 5
---

# Phase 3: Code Review Report

**Reviewed:** 2026-05-19
**Depth:** standard
**Files Reviewed:** 17
**Status:** issues_found

## Summary

Phase 3 introduces the config + buttons + watcher surface. The Rust core (`config.rs`, `escape.rs`) is well-tested and defensive; the CLI and `check`/`init` subcommands are clean. However, three concurrency/security defects in the WebSocket and watcher paths are blocking, plus several quality issues. The TOML parser correctly enforces `deny_unknown_fields`, and the byte-escape decoder is byte-safe with no panic paths. JS surface uses `textContent` consistently in Phase-3 banner rendering — XSS posture is preserved.

## Critical Issues

### CR-01: WebSocket handler deadlocks per-connection writer/forwarder tasks

**File:** `crates/bootroom/src/ws.rs:63-183`
**Issue:** The connection cleanup sequence at lines 170-181 is structurally unable to complete in the steady state:

1. `drop(tx)` drops the local mpsc sender clone, but **`tx_for_bcast` (a clone of `tx`) is still owned by the broadcast forwarder task**.
2. Line 173: `let _ = writer.await;` blocks until the writer task exits.
3. The writer's `while let Some(msg) = rx.recv().await` returns `None` only when **all** senders are dropped. Since the forwarder holds `tx_for_bcast`, it never returns `None`.
4. The forwarder breaks out of its loop only when `tx_for_bcast.send(msg).await` errors — which requires the receiver to be dropped, but the writer is holding it.

Result: `writer.await` hangs forever, line 181 (`bcast_forwarder.abort()`) is never reached, the "ws connection closed" log line never appears, and **both tasks plus the entire `handle_socket` task leak per WebSocket disconnect**. After enough reconnects (the browser auto-reconnects every 1s on close — `app.js:528`) the process accumulates one orphan writer + one orphan forwarder per attempt.

The comment on line 174 ("would also exit naturally when its `tx_for_bcast.send()` errors") is wrong — that error path can only fire after the receiver is dropped, which is gated on the writer exiting, which is gated on `tx_for_bcast` being dropped. Circular.

**Fix:** Abort the forwarder **before** awaiting the writer so its `tx_for_bcast` clone is dropped, freeing the writer to observe channel close:

```rust
drop(tx);
bcast_forwarder.abort();
let _ = bcast_forwarder.await;   // optional — swallow JoinError(Cancelled)
let _ = writer.await;
```

Alternative: extract `tx_for_bcast` ownership into the outer scope and `drop` it explicitly alongside `tx`.

---

### CR-02: WebSocket handler accepts cross-origin upgrades — CSRF-over-WS surface

**File:** `crates/bootroom/src/ws.rs:38-43`, `crates/bootroom/src/server.rs:26`
**Issue:** `ws_handler` upgrades any HTTP request without inspecting the `Origin` header. Same-origin policy does **not** apply to WebSocket handshakes (browsers attach `Origin` but enforcement is the server's responsibility). With the default `--host 127.0.0.1:8765`, any web page the operator visits in the same browser can open `ws://127.0.0.1:8765/ws` and:

- Subscribe to every server-pushed frame, including `ConfigUpdate { config }` (leaks the operator's `bootroom.toml` action labels + `bytes_b64` payloads — which may encode kernel-control byte sequences the operator considers sensitive).
- Inject `Launch` / `Reset` frames (Phase 2 server-side does no-op logging today, but Phase 4 wires real behavior; the WS protocol is already wired).

The `--host 127.0.0.1` warning at `server.rs:115-121` does NOT mitigate this — loopback binding stops the *network* but not in-browser JavaScript on a malicious page the user has open. The COOP/COEP headers on `/` protect the harness page from being iframed; they don't gate WebSocket upgrades from other origins.

**Fix:** Check `Origin` against `http(s)://<bound host>:<bound port>` (or against the request's `Host` header) before calling `ws.on_upgrade`. Reject with HTTP 403. Make the allowed origin set part of `AppState` (`Vec<HeaderValue>`). Example:

```rust
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let origin = headers.get(ORIGIN).and_then(|v| v.to_str().ok());
    if !state.allowed_origins.iter().any(|o| Some(o.as_str()) == origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(ws.on_upgrade(move |s| handle_socket(s, state)))
}
```

---

### CR-03: Config-file watcher misses edits after atomic-rename saves

**File:** `crates/bootroom/src/watcher.rs:233-241`
**Issue:** Line 237 watches the config **file** non-recursively:

```rust
debouncer.watch(&config_path, RecursiveMode::NonRecursive)?;
```

But the same module deliberately watches the **parent directory** for the kernel (lines 238-241, Pitfall #2) precisely because `make` and most editors save via atomic-rename (write tempfile → rename over target). The inotify watch is attached to the **inode**, not the path; after one atomic-rename save the watch follows the now-orphaned old inode, and every subsequent edit to `bootroom.toml` is silently missed.

Affected editors / tools include vim (default `:set writebackup nowritebackup`), VS Code, JetBrains IDEs, `git checkout`, and `make`-driven config regeneration.

The kernel-watch comment at line 238 calls this out explicitly: "atomic-rename safety. `make` writes a sibling tempfile and renames over the target; the rename only fires events on the parent dir." The same trap applies to the config — but the config is watched as a file, not a directory.

**Fix:** Watch the config parent directory (non-recursive) and demux by filename equality, same pattern as the kernel block:

```rust
let config_parent = config_path.parent()
    .ok_or_else(|| anyhow::anyhow!("config_path_canon has no parent"))?
    .to_path_buf();
let config_basename = config_path.file_name()
    .ok_or_else(|| anyhow::anyhow!("config_path_canon has no file name"))?
    .to_os_string();
// ... in the callback:
if target.parent() == Some(config_parent.as_path())
    && target.file_name() == Some(config_basename.as_os_str()) {
    config_dirty = true;
}
debouncer.watch(&config_parent, RecursiveMode::NonRecursive)?;
```

Be aware: this widens the event surface to siblings in the config's parent dir; existing logic already demuxes by exact path, so the comparison is straightforward. The `paths_equal` helper at line 264 will then never match for the config (since notify reports the absolute path; OK for direct-write editors that don't rename).

## Warnings

### WR-01: Kernel hash loads entire file into memory

**File:** `crates/bootroom/src/watcher.rs:331-348`
**Issue:** `std::fs::read(path)` reads the entire kernel into a `Vec<u8>` before hashing. RISC-V kernel images can be tens to hundreds of MB; on a CI runner with constrained memory, repeated rebuilds will spike RSS. The hash is also recomputed on every kernel rebuild without leveraging `state.digest_cache`. Acknowledged as a trade-off in the module doc, but the in-memory load is a separate concern from the cache decision.
**Fix:** Stream the file into the hasher:

```rust
use std::io::Read;
let mut file = std::fs::File::open(path)?;
let mut hasher = Sha256::new();
let mut buf = [0u8; 64 * 1024];
loop {
    let n = file.read(&mut buf)?;
    if n == 0 { break; }
    hasher.update(&buf[..n]);
}
let digest = hasher.finalize();
```

---

### WR-02: Watcher OS thread blocks 100ms on every kernel event

**File:** `crates/bootroom/src/watcher.rs:287`
**Issue:** `std::thread::sleep(SIZE_STABILITY_WINDOW)` blocks the single debouncer dispatch thread for 100 ms per kernel-dirty cycle. If a config event arrives in the same debounce window, it queues behind the kernel processing. With back-to-back saves (a `make` rebuild while editing TOML), config edits visibly stutter by 100 ms each. The pattern is acknowledged in the module header but should be documented to operators or moved off the dispatch thread.
**Fix:** Either spawn a one-shot `std::thread` per kernel-dirty event to run gates 2-3 + hash, or move the size-stability + hash into a `tokio::task::spawn_blocking` driven from a hand-off channel. Out of v1 perf scope; downgrade if intentional.

---

### WR-03: `Box::leak` + unused binding instead of `std::mem::forget`

**File:** `crates/bootroom/src/watcher.rs:246-247`
**Issue:**

```rust
let leaked: &'static mut _ = Box::leak(Box::new(debouncer));
let _ = &*leaked; // suppress "unused leaked" pedantic warnings.
```

The `&*leaked` line is a no-op. Clippy's `let_underscore_must_use` and `unused_must_use` don't apply to a leaked `&'static mut` — there's nothing to "use." The clearer pattern is `std::mem::forget(debouncer)` (which doesn't even need `Box`), or `let _: &'static mut _ = Box::leak(...);` with the binding name elided.
**Fix:**

```rust
std::mem::forget(debouncer);
```

or

```rust
Box::leak(Box::new(debouncer));
```

(let-binding to `_` is unnecessary; the expression's value can just be dropped.)

---

### WR-04: `bootroom init` has TOCTOU race on file existence check

**File:** `crates/bootroom/src/init_cmd.rs:75-80`
**Issue:** `path.exists()` followed by `std::fs::write` is a TOCTOU window. Two concurrent `bootroom init` processes can both pass the `!exists()` check; one will clobber the other's write. Low-impact (init is an interactive onboarding command), but the fix is one line.
**Fix:** Use `OpenOptions::new().write(true).create_new(true).open(&path)` when `!args.force`. `create_new` is atomic and returns `AlreadyExists` if the file appeared between check and create.

---

### WR-05: Banner `state.ok` boolean narrowing loses the server's response

**File:** `crates/bootroom/web/app.js:640`
**Issue:** `ok: frame.ok === true` collapses `false`, `null`, `undefined`, and missing into `false`. The watcher only sends `ok: bool` (Rust serde), so in practice the narrowing is safe — but the same client also handles `ConfigInvalid` where `frame.error` is similarly typed-narrowed (`typeof frame.error === 'string' ? frame.error : 'unknown'`). For `KernelChanged.reason`, line 640 already does `(typeof frame.reason === 'string') ? frame.reason : null`. Consistent but worth a single helper to centralize the wire-validation pattern so future frame types don't drift.
**Fix:** Introduce a tiny `coerceString`/`coerceBool` helper in `app.js` and route all frame-field reads through it. Defensive; low priority.

---

### WR-06: `cli.rs::parse_cli_action` lossy display of label in error path

**File:** `crates/bootroom/src/cli.rs:121`
**Issue:** `format!("--action {s:?}: empty label")` uses `{s:?}` which Debug-prints the entire input. For long inputs this is verbose. Minor UX; the error is correct.
**Fix:** Truncate `s` to ~60 chars before printing, or skip displaying it entirely once `=` is missing.

---

### WR-07: `humanBytes` overflows past TiB without rolling to PiB

**File:** `crates/bootroom/web/app.js:40-49`
**Issue:** `units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']` — a 5 PiB kernel image displays as "5120.0 TiB". Not realistic for a RISC-V kernel; flagged for completeness only.
**Fix:** Add `'PiB', 'EiB'`. Optional.

---

### WR-08: `assets_dir_canon` falls back silently to `None` on canonicalize failure

**File:** `crates/bootroom/src/state.rs:79-81`
**Issue:** `assets_dir.as_ref().and_then(|d| std::fs::canonicalize(d).ok())` swallows the error. If `--assets-dir /nonexistent` is passed, `assets_dir` is `Some` but `assets_dir_canon` is `None`. Downstream consumers that compare against `assets_dir_canon` for path-traversal checks (per the doc on line 39-42) will see "no canonical form available" and may fail-open or fail-closed depending on the consumer's logic.
**Fix:** Surface canonicalize failure as a startup error in `server::run` instead of silently `None`-ing the field. `server::run` already canonicalizes `--kernel` and `--config` strictly; treat `--assets-dir` the same.

---

### WR-09: `handle_kernel_change` retries forever on persistent size instability

**File:** `crates/bootroom/src/watcher.rs:287-300`
**Issue:** If the kernel is being written by a producer that consistently grows the file faster than the 100 ms window (or never reaches stability), the watcher returns early forever and never broadcasts. No backoff, no maximum-attempts counter, and the next debounce tick is gated on the next FS event — if the writer happens to stop emitting events during a partial write, the kernel stays stale until the next save. Edge case; mitigated in practice because `make` finishes link in well under a second and emits a final `Close` event.
**Fix:** Optional — accept after N consecutive deferrals, or sample over a longer window.

## Info

### IN-01: `escape.rs` non-ASCII follow byte after `\` maps to literal `?`

**File:** `crates/bootroom-core/src/escape.rs:122-126`
**Issue:** When `\` is followed by a UTF-8 lead byte ≥ 0x80, the error renders as `unknown escape '\?' at byte N`. The `?` substitution discards information; operator sees a misleading message ("there's no `\?` in my config"). Documented at line 123 and covered by a test.
**Fix:** Render the actual byte as hex: `"unknown escape '\\x{next:02x}' at byte {pos}"`. Optional polish.

---

### IN-02: `loaded_config` write lock held during JSON projection in watcher reload

**File:** `crates/bootroom/src/watcher.rs:379-389`
**Issue:** `project_loaded_to_json(&loaded)` runs *before* acquiring the write lock — correct (the projection is over the freshly-loaded view). The subsequent block scope drops the lock before `tx.send(...)`. Order is right; flag for future readers to preserve.
**Fix:** None; informational.

---

### IN-03: `watcher.rs::handle_kernel_change` recomputes mtime after the size-stability check

**File:** `crates/bootroom/src/watcher.rs:321-327`
**Issue:** `std::fs::metadata(path)` is called three times in this function (lines 277, 288, 321). The third call (for mtime) races against a fresh write that may have happened during the ELF-magic + size-stability gates. Result: the broadcast `mtime` may not match `s1` (the size we stabilized on). Cosmetic in practice — the browser uses mtime as a label, not for correctness.
**Fix:** Optional — capture `m.modified()` from the second `metadata` call.

---

### IN-04: `style.css` uses `!important` on `.xterm` selectors

**File:** `crates/bootroom/web/style.css:177-183`
**Issue:** `!important` is documented but worth pinning — if xterm.js is bumped past 5.3.0 and changes its theme-resolution path, this override may stop working invisibly. Track xterm version alongside the override.
**Fix:** None; document in vendor pinning.

---

### IN-05: WS Hello version compatibility check is a stub

**File:** `crates/bootroom/web/app.js:560-569`
**Issue:** The Hello handler logs the server version but doesn't compare to anything. Documented as Phase 6 work. No action required for Phase 3.
**Fix:** None; informational.

---

_Reviewed: 2026-05-19_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
