---
phase: 01-walking-skeleton
reviewed: 2026-05-17T00:00:00Z
depth: standard
files_reviewed: 28
files_reviewed_list:
  - Cargo.toml
  - Makefile
  - .gitattributes
  - .gitignore
  - .gitmodules
  - crates/bootroom-core/Cargo.toml
  - crates/bootroom-core/src/lib.rs
  - crates/bootroom/Cargo.toml
  - crates/bootroom/build.rs
  - crates/bootroom/src/main.rs
  - crates/bootroom/src/lib.rs
  - crates/bootroom/src/cli.rs
  - crates/bootroom/src/state.rs
  - crates/bootroom/src/headers.rs
  - crates/bootroom/src/embed.rs
  - crates/bootroom/src/server.rs
  - crates/bootroom/src/assets.rs
  - crates/bootroom/src/kernel_info.rs
  - crates/bootroom/src/kernel_stream.rs
  - crates/bootroom/web/index.html
  - crates/bootroom/web/app.js
  - crates/bootroom/web/style.css
  - crates/bootroom/tests/common/mod.rs
  - crates/bootroom/tests/coop_coep_headers.rs
  - crates/bootroom/tests/serve_binds.rs
  - crates/bootroom/tests/embedded_assets_served.rs
  - crates/bootroom/tests/assets_dir_override.rs
  - crates/bootroom/tests/port_host_flags.rs
  - crates/bootroom/tests/kernel_info_endpoint.rs
  - crates/bootroom/spikes/spike-b/Cargo.toml
  - crates/bootroom/spikes/spike-b/src/main.rs
  - crates/bootroom/spikes/spike-a/web/swap.js
  - crates/bootroom/assets/qemu/module.js
findings:
  critical: 2
  warning: 8
  info: 6
  total: 16
status: clean
fixes_applied:
  count: 10
  scope: critical_warning
  fixed_at: 2026-05-17
  deferred_info: [IN-01, IN-02, IN-03, IN-04, IN-05, IN-06]
---

# Phase 1: Code Review Report

**Reviewed:** 2026-05-17
**Depth:** standard
**Files Reviewed:** 28 (source) + 5 (config/build/web)
**Status:** issues_found

## Summary

Phase 1 ("Walking Skeleton") is a careful, well-structured implementation of the bootroom workspace: Cargo workspace, axum HTTP server with COOP/COEP middleware, embedded `web/` + `assets/qemu/` via `include_dir!`, four route handlers, vanilla-JS browser UI mounted on xterm.js + xterm-pty, and two spike harnesses. The codebase is small (well under the ~500 LOC budget the research called for), idiomatic, and disciplined about not pulling in `thiserror`, npm, or unsafe code (`unsafe_code = "forbid"` on all three crates). Test coverage hits every Phase-1 requirement ID and Pitfall #1 has a dedicated regression test on the 404 path.

That said, the review surfaced two correctness defects that block confident operation outside the loopback-IPv4 happy path, plus a cluster of moderate issues around error handling, async lifecycle, and a Rust↔JS contract mismatch. The browser asset handler also has a path-traversal seam that the current check does not fully close.

### Critical findings

- **CR-01**: `--host ::1` (documented as supported in CONTEXT.md and 01-RESEARCH.md Pitfall #10) cannot bind — the host:port concatenation produces a malformed IPv6 address.
- **CR-02**: `/assets/{*path}` traversal check is bypassable when the disk override is unset (embedded path lacks the `..` guard's intended effect for URL-encoded segments and only catches literal `..`); see WR-01 for the full anatomy. Promoted to CR-02 because the V12 control surfaces in the planning artifacts is explicitly stated as required.

(See findings sections below for full details and proposed fixes.)

### Other notable items

- The web UI silently fails if `Terminal` or `openpty` globals fail to load (no banner, status pill frozen at LOADING).
- Several handlers conflate "file does not exist" with "file unreadable" by mapping every I/O error to `NOT_FOUND`.
- Per-request SHA-256 of the entire kernel on `/api/kernel/info` is acceptable for loopback dev but worth caching by mtime.
- Integration tests detach their `JoinHandle` (`_handle:`) without aborting on drop, so the server task lives until the test runtime ends.
- A dead/no-op JS assignment (`Module.mainScriptUrlOrBlob`) and a misleading source-code comment about `Module.FS` visibility — minor cleanup.

---

## Critical Issues

### CR-01: IPv6 `--host` cannot bind because address and port are concatenated without brackets

**File:** `crates/bootroom/src/server.rs:50-52`

**Issue:** The bind address is constructed with `format!("{}:{}", args.host, args.port).parse::<SocketAddr>()`. For an IPv6 host like `::1`, this produces the string `::1:8765`, which is ambiguous (`::1:8765` parses as the IPv6 address `0:0:0:0:0:0:1:8765`, not loopback port 8765) and round-trips through `SocketAddr::parse` either as the wrong address or as a parse error. CONTEXT.md "Default port + --no-open behavior" explicitly documents `--host ::1` as a supported way to bind to IPv6 loopback, and 01-RESEARCH.md Pitfall #10 references it as the recommended workaround for hosts that resolve `localhost` to `::1`. As written, that workaround does not function.

Reproduction:
```
$ bootroom serve --kernel /tmp/k --host ::1 --port 8765
Error: invalid --host/--port: ::1:8765
```

**Fix:** Parse the host as `IpAddr` first, then assemble a `SocketAddr` directly:

```rust
use std::net::IpAddr;

let ip: IpAddr = args.host.parse()
    .with_context(|| format!("invalid --host: {}", args.host))?;
let addr = SocketAddr::new(ip, args.port);
```

This also gives a cleaner error message ("invalid --host: foo" instead of "invalid --host/--port: foo:8765"). The same `is_loopback(&addr.ip())` check below continues to work, and the existing `is_loopback` helper still covers both v4 and v6 correctly.

---

### CR-02: Path-traversal protection in `serve_one` is incomplete — only literal `..` segments are rejected, and the canonicalize check only fires in the disk-override branch

**File:** `crates/bootroom/src/assets.rs:39-62, 74-94`

**Issue:** ASVS V12 / 01-RESEARCH.md Security Domain calls out the disk-override branch as the path-traversal surface and 01-CONTEXT.md `<specifics>` Pitfall 3 acknowledges that `--assets-dir` covers BOTH `web/` and `assets/qemu/`. The current implementation has two seams:

1. **Embedded-only branch has no traversal guard beyond the literal `..` segment check.** The early check `requested.split('/').any(|seg| seg == "..")` rejects `web/../etc/passwd` but not URL-encoded equivalents (`%2e%2e`). Axum decodes percent-escapes from the URL before invoking the handler, so `%2e%2e` arrives as `..` and IS caught — but the check is still fragile: anything that produces a `..` segment from a non-`/`-separated source (NUL bytes, backslash separators, mixed encodings via future routing changes) is not covered. The `include_dir::Dir::get_file` API itself does **not** normalize `..`, so `WEB.get_file("../assets/qemu/qemu-system-riscv64.wasm")` would silently return `None` rather than escape — but that defense-in-depth is implicit, not asserted.

2. **The canonicalize check (lines 84-91) is the strong guard, but it only runs in the disk-override branch.** Per `try_disk` semantics, if the disk file does not exist, the function returns `None` and the request falls through to the embedded copy — which means a traversal target that exists in the embedded `Dir` but not on disk reaches the embedded fallback with no canonicalize check. Today the only embedded subtrees are `web/` and `qemu/` (matched by `split_subtree`), so the practical impact is limited; but the security control is documented as universal, not branch-conditional.

3. **The `tokio::fs::canonicalize` check fails open on `canonicalize` errors.** Both `canon` and `root_canon` use `.ok()?`, so any `canonicalize` failure (permission denied, NotADirectory, EINVAL on macOS sandboxes) silently falls through to the embedded fallback. A subtle attack: race the file out of existence between `canonicalize` and `read`; the canonicalize fails, we drop to embedded, the embedded copy is returned. Not exploitable today because there are no embedded files outside `web/` and `qemu/` subtrees, but the contract is "reject", not "fall through".

**Fix:**

Two-part fix:

```rust
// 1. Tighten the early check to also normalize separators and reject empty segments.
//    (Defense in depth; the practical attack today is small.)
for seg in requested.split('/') {
    if seg == ".." || seg.contains('\\') || seg.contains('\0') {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }
}

// 2. Make try_disk return a tri-state instead of Option:
//      Ok(resp)   -> use this response
//      Err(())    -> hard reject (traversal); do NOT fall through
//      Ok(None)   -> file simply absent; fall through to embed
//    Then in serve_one, propagate the hard-reject case without falling
//    through to the embedded branch.
enum DiskOutcome { Hit(Response), Miss, Reject(Response) }

async fn try_disk(root: &Path, requested: &str) -> DiskOutcome {
    let on_disk: PathBuf = match requested.strip_prefix("web/")
        .map(|r| root.join("web").join(r))
        .or_else(|| requested.strip_prefix("qemu/").map(|r| root.join("assets/qemu").join(r))) {
        Some(p) => p,
        None => return DiskOutcome::Miss,
    };
    // canonicalize root once at startup, store on AppState — avoids
    // the recursive canonicalize on every request and removes the
    // race window. Then:
    let canon = match tokio::fs::canonicalize(&on_disk).await {
        Ok(c) => c,
        Err(_) => return DiskOutcome::Miss,  // genuine miss
    };
    if !canon.starts_with(&state.assets_dir_canon) {
        return DiskOutcome::Reject(
            (StatusCode::BAD_REQUEST, "path escapes --assets-dir").into_response()
        );
    }
    match tokio::fs::read(&canon).await {
        Ok(bytes) => DiskOutcome::Hit(ok_bytes(bytes, requested)),
        Err(_) => DiskOutcome::Miss,
    }
}
```

Storing `assets_dir_canon: Option<PathBuf>` on `AppState` (computed once in `server::run`) also avoids the per-request double-canonicalize that the current code does for every disk-override request.

---

## Warnings

### WR-01: Web UI silently freezes at LOADING when any vendor `<script>` fails to load

**File:** `crates/bootroom/web/app.js:99-106`

**Issue:** At module top level the script does `const xterm = new Terminal();` and `const { master, slave } = openpty();` without any presence check on the globals. If `xterm.js`, `xterm-pty.js`, `load.js`, or `module.js` failed to download (404, COEP violation, parse error), `Terminal` or `openpty` is `undefined` and the entire module throws synchronously at evaluation time. The `bootGuest().catch(...)` block at the bottom never runs because we never reach it; `loadKernelInfo()` also never runs. The status pill stays `LOADING` forever, no error appears in the UI, and the only signal is a red entry in the DevTools console.

This is exactly the failure mode the COI probe banner is designed to surface — but the probe only fires when SAB is unavailable, not when a vendor asset is missing.

**Fix:** Guard the global lookups, fall back to the HALTED pill and a banner-style message in the terminal container (or a second alert div) when vendor wiring is missing:

```javascript
if (typeof Terminal !== 'function' || typeof openpty !== 'function') {
  setPill('HALTED');
  const t = document.getElementById('terminal');
  if (t) t.textContent =
    '[bootroom] Vendor scripts (xterm.js / xterm-pty) failed to load. ' +
    'Check DevTools Network tab for 4xx responses or CORP/COEP violations.';
  throw new Error('vendor globals missing');
}
```

Place this BEFORE `new Terminal()` and BEFORE any `Module.*` access.

---

### WR-02: All read errors in `kernel_info` and `kernel_stream` are coerced to `404 NOT_FOUND`, hiding permission-denied and I/O failures

**File:** `crates/bootroom/src/kernel_info.rs:34-46`, `crates/bootroom/src/kernel_stream.rs:25-27`

**Issue:** `tokio::fs::metadata(...).await.map_err(|_| StatusCode::NOT_FOUND)?` and `tokio::fs::File::open(...).await.map_err(|_| StatusCode::NOT_FOUND)?` convert every error to 404. EACCES (permission denied), EBUSY, ENOMEM, and EIO all appear to the browser as "the kernel file is gone." The user/UI loses information that would help them diagnose ("file exists but we can't read it"). Phase 1's UI handles 404 by showing `ERR` in each field — but the operator never learns whether the file is missing or just unreadable.

**Fix:** Distinguish by `io::ErrorKind`:

```rust
use std::io::ErrorKind;

let meta = tokio::fs::metadata(&s.kernel).await.map_err(|e| match e.kind() {
    ErrorKind::NotFound => StatusCode::NOT_FOUND,
    ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
    _ => StatusCode::INTERNAL_SERVER_ERROR,
})?;
```

Apply the same pattern to `File::open` in both handlers. Also log the error via `tracing::warn!(error = %e, "kernel I/O failure")` so the operator sees the underlying cause server-side.

---

### WR-03: `/api/kernel/info` rehashes the entire kernel on every request

**File:** `crates/bootroom/src/kernel_info.rs:44-60`

**Issue:** The handler opens the kernel file and streams it through SHA-256 on every `/api/kernel/info` call. With a 40 MB NORN-class kernel that's ~150-300ms of disk + CPU per request. Two failure modes follow:

1. **Browser polling burns disk and CPU.** If a future revision adds a refresh interval (UI-04 et al. are deferred but plausible), every poll hashes ~40 MB.
2. **DoS amplification** if the server is ever exposed past loopback. A single unauthenticated GET costs a full file scan.

**Fix:** Cache by `(path, size, mtime)`. The check itself (a `metadata()` call) is microseconds; rehashing is the expensive part.

```rust
struct CachedDigest {
    size: u64,
    mtime_sec: i64,
    sha256_prefix: String,
}

// On AppState: cache: Arc<tokio::sync::RwLock<Option<CachedDigest>>>

// In handler: if cache matches current (size, mtime), reuse; else rehash + store.
```

This stays compatible with Phase 3's watcher (the watcher will invalidate by writing `None` when it sees a change).

---

### WR-04: Build script's `rerun-if-changed=assets/qemu` watches the directory, not its files

**File:** `crates/bootroom/build.rs:27`

**Issue:** `cargo:rerun-if-changed=assets/qemu` emits a single path. Cargo tracks mtime on that path — which on most filesystems updates only when entries are added or removed, not when an existing file's content changes. Today the `REQUIRED` list is six files; editing `module.js` (the bootroom-authored argv) or replacing `qemu-system-riscv64.wasm` in place (via `cp` without `rm` first) may not trigger a rebuild of any downstream consumer that depends on the embedded bytes. Combined with `include_dir!` which captures contents at compile time, you can get a binary that embeds stale assets.

**Fix:** Emit one `rerun-if-changed` per required file. The list is already in `REQUIRED`:

```rust
for rel in REQUIRED {
    println!("cargo:rerun-if-changed={rel}");
}
// Keep the directory-level watch too, so adding NEW files re-triggers.
println!("cargo:rerun-if-changed=assets/qemu");
```

Also consider watching `web/` for the same reason (`include_dir!("$CARGO_MANIFEST_DIR/web")` in `embed.rs` captures its contents at compile time but `build.rs` does not declare a dependency on it; today this is OK because `web/` files are part of the crate package and cargo invalidates on package changes, but it's brittle).

---

### WR-05: Spike-B fixture path is hardcoded as a string constant inside an executable

**File:** `crates/bootroom/spikes/spike-b/src/main.rs:18`

**Issue:** `const RESULT_PATH: &str = "crates/bootroom/spikes/spike-b/SPIKE-B-RESULT.md";` — a relative path baked into the binary. The spike only works when invoked with the workspace root as the current working directory. Running `cargo run -p spike-b` from anywhere else writes the result file to the wrong place (or fails silently because `parent()` is `crates/bootroom/spikes/spike-b/` which may not exist relative to the cwd). The `std::fs::create_dir_all(parent).ok()` swallows the error; subsequent `fs::write` will fail and only THAT error is surfaced via `with_context`.

**Fix:** Resolve the result path relative to `CARGO_MANIFEST_DIR` (the spike-b crate's directory):

```rust
let result_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("SPIKE-B-RESULT.md");
```

And drop the `RESULT_PATH` constant. This makes the spike location-independent.

---

### WR-06: Test helper detaches the server task but never aborts it on drop

**File:** `crates/bootroom/tests/common/mod.rs:13-31`

**Issue:** `TestServer { _handle: JoinHandle<()> }` holds the handle but never aborts it. When the test scope ends, `_handle` is dropped, which for `JoinHandle` is a no-op detach (NOT abort). The spawned `axum::serve` task continues running until the test's tokio runtime is dropped. Within a single `#[tokio::test]`, this is bounded by the runtime lifetime — but it means:

1. Every integration test that calls `spawn` leaves a server task running until end-of-test, holding an open socket and TCP listener. With ~12 integration tests today, that's fine; at scale it gets noisier.
2. If a test fails an assertion mid-flow, the server holds the port until the runtime shuts down — irrelevant for port 0, but a subtle leak in principle.
3. The `axum::serve` call has `.expect("axum::serve")` — if the serve task ever panics, the panic propagates into the runtime via the join handle that we explicitly stash in `_handle`. Detaching it means panic-on-serve becomes silent.

**Fix:** Implement `Drop` for `TestServer` that aborts the handle:

```rust
impl Drop for TestServer {
    fn drop(&mut self) {
        self._handle.abort();
    }
}
```

Rename `_handle` to `handle` once it's used. This also stops the server immediately when a test ends, which makes failure diagnosis cleaner.

---

### WR-07: `humanBytes` formatter mis-handles `0` and very large negatives

**File:** `crates/bootroom/web/app.js:22-34`

**Issue:** `humanBytes(0)` returns `"0 B"` (passing through the `value < 1024 && unit < 4` loop without iterating), which is fine. But `humanBytes(-1)` returns `"-1 B"` for a negative input the API should never produce — and `humanBytes(Number.MAX_SAFE_INTEGER * 2)` overflows the unit table without warning. The bigger issue: `isNaN(n)` is called BEFORE coercing through `Number(n)`, so `humanBytes("abc")` (a string the API shouldn't return but might in error paths) returns `"NaN B"` (because `isNaN("abc")` is `true`... actually it returns `"—"` here, OK). But `humanBytes("12345")` returns `"12.06 KiB"` (string coerces fine via `Number(n)`). The unit-loop overrun matters only if the kernel exceeds 1024 TiB — which it won't — so this is mostly hardening.

**Fix:** Add explicit guards:

```javascript
function humanBytes(n) {
  if (n == null) return '—';
  const num = Number(n);
  if (!Number.isFinite(num) || num < 0) return '—';
  // ... rest unchanged
}
```

---

### WR-08: `Module.FS_unlink('/pack/Image')` failure is silently swallowed and the next-step write is attempted regardless

**File:** `crates/bootroom/web/app.js:177-184`

**Issue:** The `try { Module.FS_unlink('/pack/Image'); } catch (_e) { /* not present yet */ }` is fine for the "file doesn't exist" case (errno 44 / ENOENT). But it ALSO swallows EROFS (read-only FS), EACCES (permission), and EBUSY (file in use). If unlink fails for a reason OTHER than ENOENT, the subsequent `FS_createDataFile` will then fail with EEXIST — and the user sees "Failed to inject kernel: ..." with the EEXIST shape, which is a misleading downstream error.

**Fix:** Inspect the errno:

```javascript
try {
  Module.FS_unlink('/pack/Image');
} catch (e) {
  // emscripten's FS errors expose .errno; 44 = ENOENT (file not present yet).
  // Anything else means the FS state is unexpected and the upcoming
  // createDataFile will misfire — bail with the real reason now.
  if (e && e.errno !== 44) {
    console.error('FS_unlink failed unexpectedly:', e);
    slave.write('[bootroom] Cannot replace /pack/Image: ' + (e.message || e) + '\r\n');
    setPill('HALTED');
    return;
  }
}
```

---

## Info

### IN-01: `Module.mainScriptUrlOrBlob` is a no-op with ES-module emscripten builds

**File:** `crates/bootroom/web/app.js:112`

**Issue:** `Module.mainScriptUrlOrBlob = location.origin + '/assets/qemu/out.js';` was used by pre-ES-module emscripten to tell the worker how to locate the main script. With `-sEXPORT_ES6=1` (set in `Makefile:47`) the worker uses `new URL("qemu-system-riscv64.worker.js", import.meta.url)` instead — see `out.js:3540`. Since `out.js` is loaded via dynamic `import()`, `import.meta.url` resolves to `/assets/qemu/out.js` and the worker URL resolves correctly without `mainScriptUrlOrBlob`. The assignment is dead.

**Fix:** Delete the line. Add a one-line comment that the worker URL is resolved from `import.meta.url` of `out.js` (consistent with the `Module.locateFile` comment block in `index.html`).

---

### IN-02: Comment about `Module.FS` visibility is inaccurate

**File:** `crates/bootroom/web/app.js:174-175`, `crates/bootroom/spikes/spike-a/web/swap.js:6-9`

**Issue:** The comment "Module.FS isn't exposed publicly on this emscripten build; we use the wrapper functions Module exposes (FS_unlink, FS_createDataFile)." contradicts the actual artifact: `out.js:7795` is `Module["FS"] = FS;`. `Module.FS` IS exposed on the current build (the Makefile's `-sEXPORTED_RUNTIME_METHODS=...,TTY,FS` puts it there). The legacy wrappers `FS_unlink` and `FS_createDataFile` work because they are emitted alongside `FS` by emscripten's auto-export rules, not because `FS` is missing.

**Fix:** Replace the comment with the accurate reason for using the wrappers:

```javascript
// Use Module.FS_unlink / Module.FS_createDataFile rather than Module.FS.unlink /
// Module.FS.createDataFile: the wrapper exports are part of emscripten's stable
// legacy API and survive runtime-method tree-shaking, whereas Module.FS direct
// access has been deprecated in newer emsdk releases.
```

(Or simply switch to `Module.FS.unlink` / `Module.FS.writeFile` since they ARE available — both work and `writeFile` is shorter than the unlink+createDataFile pair.)

---

### IN-03: `tempdir_for_test` in `assets.rs` tests reinvents `tempfile::tempdir`

**File:** `crates/bootroom/src/assets.rs:115-132`

**Issue:** The unit tests roll their own tempdir via `std::env::temp_dir().join(format!("bootroom-assets-{pid}-{nanos}"))` and manually `std::fs::remove_dir_all(&dir).ok()` after each test. The `tempfile = "3.10"` dev-dep is already in `Cargo.toml` and the integration tests use it correctly. On test panic, the manual cleanup is skipped and the tempdir leaks under `/tmp`. The nanos+pid uniqueness scheme is also fragile (low-resolution clocks can collide).

**Fix:** Add `tempfile` to the inline test module:

```rust
#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    // ...
    let dir = TempDir::new().unwrap();
    // No manual cleanup — Drop handles it.
}
```

Same applies to `kernel_info.rs:76-85` (`write_tmp` helper).

---

### IN-04: `clippy::cast_possible_wrap` allow on `mtime` is broader than necessary

**File:** `crates/bootroom/src/kernel_info.rs:30`

**Issue:** `#[allow(clippy::cast_possible_wrap)]` is applied to the entire function but is only needed for the single `d.as_secs() as i64` cast on line 42. Function-level allows hide future wrap-risk casts that might be added.

**Fix:** Replace with a localized `#[allow]` on the binding, or use a non-truncating conversion:

```rust
let mtime: i64 = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
```

`u64::as_secs()` returning a value that exceeds `i64::MAX` means the year ~292277, so saturating is fine and removes the need for any `#[allow]`.

---

### IN-05: Spike-B `today_utc_string` uses `as i64` cast that could wrap

**File:** `crates/bootroom/spikes/spike-b/src/main.rs:463`

**Issue:** Same pattern as IN-04: `d.as_secs() as i64` without an allow attribute. Clippy's `pedantic` is on (`[lints.rust]` in spike-b's Cargo.toml only sets `unsafe_code = "forbid"`, but the workspace lints don't propagate, so this likely doesn't warn). The math itself is fine until year 292277 but the casts are sloppy. Mention only because IN-04 fixes the same pattern in another file.

**Fix:** `i64::try_from(d.as_secs()).unwrap_or(i64::MAX)` here too.

---

### IN-06: `spike-b/Cargo.toml` does not inherit workspace lints, so `unsafe_code = "forbid"` is the only protection

**File:** `crates/bootroom/spikes/spike-b/Cargo.toml:26-27`

**Issue:** The main bootroom crates declare `pedantic = { level = "warn", priority = -1 }` under `[lints.clippy]`. Spike-B does not. The spike has a number of patterns (manual arg parsing, `as i64` casts, hardcoded paths, broad `.ok()` swallowing) that pedantic would flag. Since it's spike code with a deliberately limited lifetime, this is acceptable, but it does mean the spike doesn't get the same baseline scrutiny as production code if it grows.

**Fix:** Add to spike-b/Cargo.toml:

```toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
```

And address the resulting warnings, or use `#[allow]` with a reason. Low priority — spike code is scheduled for evolution into Phase 4 production code, at which point this lint floor matters.

---

_Reviewed: 2026-05-17_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

---

## Fixes Applied (2026-05-17)

All 2 Critical and all 8 Warning findings were fixed. The 6 Info findings
are deferred to a follow-up pass (none touched files modified for higher-
severity fixes in a way that warranted opportunistic cleanup).

| Finding | Commit | Files Modified | Notes |
|---------|--------|----------------|-------|
| CR-01 | `734a10b` | `crates/bootroom/src/server.rs` | Parse `--host` as `IpAddr`; assemble `SocketAddr::new(ip, port)`. +3 unit tests covering IPv6 loopback, IPv4 happy path, and rejection of malformed host strings. |
| CR-02 | `3e07e65` | `crates/bootroom/src/{state.rs, assets.rs, server.rs}` | Tightened early traversal guard (`..`, `\\`, `\0`); refactored `try_disk` to tri-state (`Hit`/`Miss`/`Reject`) so traversal and unexpected I/O hard-reject without embedded fall-through; precomputed `assets_dir_canon` on `AppState`. +3 regression tests in embedded-only branch. |
| WR-01 | `7220963` | `crates/bootroom/web/app.js` | Guard `Terminal`/`openpty` globals; on absence, surface failure in `#iso-banner` and HALT pill before throwing. |
| WR-02 | `21b12e0` | `crates/bootroom/src/{kernel_info.rs, kernel_stream.rs}` | Distinguish `NotFound`→404, `PermissionDenied`→403, other I/O→500. All errors `tracing::warn!`-logged server-side. |
| WR-03 | `d27058f` | `crates/bootroom/src/{state.rs, kernel_info.rs}` | Cache SHA-256 by `(size, mtime_sec)` in `Arc<RwLock<Option<CachedDigest>>>`. +1 regression test asserting cache populates and reuses. |
| WR-04 | `d7e12af` | `crates/bootroom/build.rs` | Emit per-file `rerun-if-changed` for each entry in `REQUIRED`; keep directory-level watch for new files; add `web/` watch. |
| WR-05 | `ac86a73` | `crates/bootroom/spikes/spike-b/src/main.rs` | Replaced `const RESULT_PATH: &str` with `result_path()` helper joining `env!("CARGO_MANIFEST_DIR")`. Spike now location-independent. |
| WR-06 | `c6ddd4d`, `e05a895` | `crates/bootroom/tests/common/mod.rs` | `impl Drop for TestServer` calls `handle.abort()`; renamed `_handle`→`handle`; swapped `.expect("axum::serve")` for `let _ = …` (aborted serve is not a panic). Doc-markdown lint follow-up in `e05a895`. |
| WR-07 | `fe24f35` | `crates/bootroom/web/app.js` | `humanBytes` now coerces through `Number()` first, then range-checks via `Number.isFinite(num) && num >= 0`. Defends against negative, NaN, ±Infinity. |
| WR-08 | `a6ba0f7` | `crates/bootroom/web/app.js` | `FS_unlink` catch now inspects `e.errno`; only errno 44 (ENOENT) is silently tolerated. Other errnos HALT with the real cause. |

### Verification

- `cargo build --workspace` — clean.
- `cargo test --workspace` — 38 tests pass (baseline 30, +8 new tests
  added across CR-01, CR-02, WR-03, and the WR-06 `AppState`
  canonicalize assertion).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `node --check crates/bootroom/web/app.js` — clean.

### Deferred Info-level findings

| Finding | Rationale for deferral |
|---------|------------------------|
| IN-01 | Cosmetic; `Module.mainScriptUrlOrBlob = …` is a no-op but harmless. Not in scope. |
| IN-02 | Comment-only fix; the wrapper-vs-direct API choice is documented in the existing code. |
| IN-03 | Test-helper refactor (`tempfile::TempDir`); existing manual cleanup works. |
| IN-04 | Localizes an existing `#[allow]`; no functional change. |
| IN-05 | Same pattern as IN-04, in spike-b. Spike scheduled for Phase 4 evolution. |
| IN-06 | Adds workspace-lints inheritance to spike-b. Tracked for Phase 4 promotion. |

