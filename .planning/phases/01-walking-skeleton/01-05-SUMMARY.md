---
phase: 01-walking-skeleton
plan: 05
subsystem: server
tags:
  - rust
  - axum
  - handlers
  - assets
  - security
dependency-graph:
  requires:
    - 01-04-SUMMARY.md  # AppState, build_router shell, embed::WEB / embed::QEMU, headers
    - 01-03-SUMMARY.md  # vendored xterm.js for test_serve_asset_embedded_vendor_xterm
    - 01-02-SUMMARY.md  # qemu-system-riscv64.wasm for test_serve_asset_embedded_wasm
  provides:
    - "GET /api/kernel/info -> {path,size,mtime,sha256_prefix} (UI-07)"
    - "GET /kernel -> streaming application/octet-stream body (SERV-03)"
    - "GET /assets/{*path} -> embedded + --assets-dir override (SERV-03, SERV-04)"
    - "GET / -> embedded web/index.html (404 until 01-06 lands the file)"
    - "V12 path-traversal protection (.. segment reject + canonicalize-and-confirm-descendant)"
  affects:
    - 01-06  # author web/index.html, app.js, style.css — GET / will start returning 200
    - 01-07  # integration tests will hit these four routes end-to-end via spawned server
tech-stack:
  added:
    - tokio "io-util" feature (for AsyncReadExt::read in kernel_info)
  patterns:
    - "tokio_util::io::ReaderStream + axum::body::Body::from_stream for constant-memory file streaming"
    - "sha2 streaming via 64KB buffer + .update(&buf[..n]); first 6 digest bytes encoded with hex (12 chars)"
    - "Disk-then-embed asset dispatch with two-pronged prefix matching (web/ vs qemu/)"
    - "Canonicalize-both-sides + starts_with for symlink-escape defense"
    - "mime_guess::from_path with first_or_octet_stream() fallback for Content-Type"
key-files:
  created:
    - crates/bootroom/src/kernel_info.rs
    - crates/bootroom/src/kernel_stream.rs
    - crates/bootroom/src/assets.rs
  modified:
    - crates/bootroom/src/lib.rs       # pub mod {assets, kernel_info, kernel_stream}
    - crates/bootroom/src/server.rs    # real handlers wired, stubs removed
    - Cargo.toml                       # tokio gains "io-util" feature
    - Cargo.lock                       # tokio feature resolution
decisions:
  - "Hash the kernel on every /api/kernel/info call (no caching) — Phase 1 simplicity; revisit if profile shows it hot"
  - "Belt-and-suspenders V12: reject `..` segments BEFORE canonicalize, and canonicalize-then-check descendant; second layer catches symlink escapes the first cannot"
  - "Replace test_router_returns_coop_coep_on_stub_route (was 501-based) with test_full_router_serves_embedded_wasm_with_coop (real 200+MIME+headers); the 404 test still proves COOP/COEP on error paths"
  - "JS MIME on this machine resolved to text/javascript (modern mime_guess 2.0.5 IANA-aligned default); both text/javascript and application/javascript pass the contains('javascript') unit test"
metrics:
  duration: "~4m25s"
  completed: 2026-05-17
---

# Phase 01 Plan 05: API + asset handlers Summary

Four real route handlers replace the 01-04 stubs: kernel metadata + SHA-256 prefix JSON, streaming kernel body, embedded-or-disk asset serving with V12 path-traversal protection. The Phase 1 server is now functionally complete on the API surface; plan 01-06 will author the UI files this plan's handlers will serve, and plan 01-07 will pin the behaviour with integration tests.

## What Shipped

- `GET /api/kernel/info` reads `state.kernel` metadata and streams the file through SHA-256 in a 64 KB loop (constant memory) and returns `{path, size, mtime, sha256_prefix}`; `sha256_prefix` is exactly 12 lowercase hex chars (6 bytes of the digest, via `hex::encode(&digest[..6])`).
- `GET /kernel` opens `state.kernel`, wraps it in `tokio_util::io::ReaderStream`, and sets `Content-Type: application/octet-stream`. No Range support in Phase 1; full bytes stream in order.
- `GET /assets/{*path}` dispatches by URL prefix: `web/...` → `embed::WEB` or `<assets-dir>/web/...`; `qemu/...` → `embed::QEMU` or `<assets-dir>/assets/qemu/...`. Disk override checked first; falls through to embedded on miss.
- `GET /` is its own route delegating to `serve_index`, which simply asks `serve_one(&state, "web/index.html")` — same disk-override + embed-fallback path. Returns 404 until plan 01-06 lands `web/index.html`.
- Path-traversal protection: any URL path containing a `..` segment is rejected with `400 BAD_REQUEST` *before* touching disk or embed. When `--assets-dir` is set, the resolved disk path is canonicalized and must remain a descendant of the canonicalized assets root — a second-line defense against symlink escapes.
- MIME types via `mime_guess::from_path` with octet-stream fallback. `.wasm` resolves to `application/wasm` (Pitfall 2 closed), `.js` to `text/javascript` on this machine.

## Tasks

| Task | Name                                                            | Commit  |
| ---- | --------------------------------------------------------------- | ------- |
| 1    | Kernel info + streaming handlers (TDD)                          | 8fe742e |
| 2    | Asset handler with --assets-dir + V12 path traversal (TDD)      | 54f19b5 |
| 3    | Wire real handlers into server.rs                               | 2093927 |

## Verification

`cargo test -p bootroom --lib` — **16 passed**:

```
test embed::tests::test_embed_qemu_dir_has_wasm ... ok
test assets::tests::test_serve_asset_unknown_404 ... ok
test assets::tests::test_serve_asset_path_traversal_rejected ... ok
test headers::tests::test_coop_layer_overrides_existing_header ... ok
test headers::tests::test_coep_layer_value ... ok
test assets::tests::test_serve_asset_embedded_vendor_xterm ... ok
test state::tests::test_appstate_construct ... ok
test assets::tests::test_serve_asset_disk_override ... ok
test kernel_stream::tests::test_kernel_stream_missing_file ... ok
test kernel_info::tests::test_kernel_info_missing_file ... ok
test assets::tests::test_serve_asset_disk_override_fallthrough ... ok
test kernel_info::tests::test_kernel_info_known_bytes ... ok
test server::tests::test_router_returns_coop_coep_on_404 ... ok
test kernel_stream::tests::test_kernel_stream_round_trip ... ok
test assets::tests::test_serve_asset_embedded_wasm ... ok
test server::tests::test_full_router_serves_embedded_wasm_with_coop ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo clippy --workspace --all-targets -- -D warnings` — clean (pedantic enabled).

### Smoke run (manual, against the live binary)

Started `bootroom serve --kernel /tmp/fake-kernel --port 18765` and exercised each endpoint:

```
$ curl -s http://127.0.0.1:18765/api/kernel/info
{"path":"/tmp/fake-kernel","size":0,"mtime":1779027100,"sha256_prefix":"e3b0c44298fc"}
```

`e3b0c44298fc` is the canonical SHA-256 prefix of zero bytes — confirms the hash is computed over file contents, not, e.g., the path. 12 hex chars exactly.

```
$ curl -sI http://127.0.0.1:18765/assets/qemu/qemu-system-riscv64.wasm
HTTP/1.1 200 OK
content-type: application/wasm
cross-origin-opener-policy: same-origin
cross-origin-embedder-policy: require-corp
content-length: 41735175

$ curl -sI http://127.0.0.1:18765/assets/web/vendor/xterm.js
HTTP/1.1 200 OK
content-type: text/javascript
cross-origin-opener-policy: same-origin
cross-origin-embedder-policy: require-corp
content-length: 283404
```

COOP + COEP + correct Content-Type on both an embedded wasm artifact and a vendored JS asset. Pitfall 1 (COOP/COEP everywhere) and Pitfall 2 (wasm MIME) both verified end-to-end.

Path-traversal smoke (separate run with `--assets-dir /tmp/bootroom-smoke-assets`, using `curl --path-as-is` so curl doesn't normalize `..` client-side):

```
$ curl -s --path-as-is -o /dev/null -w "status=%{http_code}\n" \
    "http://127.0.0.1:18766/assets/web/../../../etc/passwd"
status=400
```

400 BAD_REQUEST, as planned. Without `--path-as-is` curl collapses the path and the server sees `/etc/passwd` directly (no `..`), which routes to nothing and returns 404 — also acceptable.

### MIME for .js on this machine

`mime_guess 2.0.5` returns `text/javascript` for `.js` files on this machine (IANA's current recommendation; the older `application/javascript` is now legacy). The `test_serve_asset_embedded_vendor_xterm` unit test asserts `contains("javascript")` so it accepts either spelling and would not need to change if a future `mime_guess` release flips the default.

### Clippy notes

One `clippy::map_unwrap_or` warning in `kernel_info.rs` was raised by the `pedantic` group on the initial draft (`.map(|d| ...).unwrap_or(0)` → `.map_or(0, |d| ...)`); fixed before committing Task 1. Final code has zero pedantic warnings.

`#[allow(clippy::cast_possible_wrap)]` is intentional on `kernel_info` for the `u64 -> i64` mtime cast — `as_secs()` returns `u64` and clippy::cast_possible_wrap complains, but the value won't approach `i64::MAX` until year 292277, so a documented allow is correct.

## Success Criteria

- **SERV-03** (embedded asset serving): verified by `test_serve_asset_embedded_wasm`, `test_serve_asset_embedded_vendor_xterm`, and live `curl -I` against both `/assets/qemu/qemu-system-riscv64.wasm` and `/assets/web/vendor/xterm.js`.
- **SERV-04** (`--assets-dir` override): verified by `test_serve_asset_disk_override` (disk wins over embed when file present) and `test_serve_asset_disk_override_fallthrough` (falls through to embed when file missing on disk).
- **UI-07** (kernel info API surface): verified by `test_kernel_info_known_bytes` (sha256(b"abc") = ba7816bf8f01...) and live `curl /api/kernel/info` returning the documented four-key JSON.
- **Pitfall 1** (COOP+COEP everywhere): `test_router_returns_coop_coep_on_404` covers the error path; `test_full_router_serves_embedded_wasm_with_coop` covers the success path; live `curl -I` confirms.
- **Pitfall 2** (.wasm MIME): `test_serve_asset_embedded_wasm` asserts exactly `application/wasm`; live `curl -I` confirms.
- **Pitfall 5** (no full-file buffer): `kernel_stream` uses `ReaderStream` + `Body::from_stream`; verified for a 256 KB payload by `test_kernel_stream_round_trip`. Large kernels (10–30 MB qemu-wasm-sized) will stream identically since memory cost is independent of file size.
- **ASVS V12** (path traversal): `test_serve_asset_path_traversal_rejected` (400) plus live `curl --path-as-is` reproduction.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocker] tokio missing `io-util` feature**
- **Found during:** Task 1 (`cargo test -p bootroom --lib kernel` failed to compile)
- **Issue:** Workspace `tokio` feature list from 01-04 was `["rt-multi-thread", "macros", "signal", "sync", "fs"]`. `AsyncReadExt::read` lives behind the `io-util` feature gate, so the streaming-hash loop in `kernel_info` failed with `no method named 'read' found for struct 'tokio::fs::File'`.
- **Fix:** Added `"io-util"` to `workspace.dependencies.tokio.features` in the root `Cargo.toml`. Plan 01-04's feature set didn't anticipate this — every prior consumer used `tokio::fs::read` (which is wholesale and doesn't need `io-util`) or didn't read at all.
- **Files modified:** `Cargo.toml`, `Cargo.lock` (rebuilt)
- **Commit:** `8fe742e` (rolled into Task 1)

**2. [Rule 1 - Bug] `clippy::map_unwrap_or` on the mtime extraction**
- **Found during:** Task 1 clippy run
- **Issue:** `.map(|d| d.as_secs() as i64).unwrap_or(0)` is the exact shape `clippy::map_unwrap_or` flags under `pedantic`.
- **Fix:** Rewrote as `.map_or(0, |d| d.as_secs() as i64)`. Behaviour identical.
- **Files modified:** `crates/bootroom/src/kernel_info.rs`
- **Commit:** `8fe742e` (rolled into Task 1)

### Skipped / Deferred

- **Test `web/index.html` via tempdir + --assets-dir override:** plan 01-05 mentions "use a separate test that creates a temp `web/index.html` via --assets-dir override" for the `serve_index` path. The disk-override path is already covered by `test_serve_asset_disk_override` (same `serve_one` codepath, just with `web/x.txt` instead of `web/index.html`); adding a near-duplicate test for the index alias would be redundant. The smoke run verified the absence of `web/index.html` returns 404 (correct behaviour until plan 01-06).
- **`mtime` as ISO 8601 string:** plan's `<phase_constraints>` mentions "mtime: as ISO 8601 UTC string in JSON" but the `<interfaces>` block and UI-SPEC reference explicitly specify "Unix epoch seconds (i64)" and show `"mtime": 1715961128`. The two contradict each other; followed the more-detailed `<interfaces>` spec since it cites UI-SPEC as the source of truth. Live `/api/kernel/info` returns `"mtime":1779027100` (epoch seconds). If plan 01-06's UI rendering or plan 01-07's tests insist on ISO 8601, switch via `chrono` or a `time` crate — recorded here as a known shape decision.

## Threat Flags

None — no new security surface beyond what the plan's `<threat_model>` already enumerates. V12 mitigation landed exactly as planned (T-01-05-01, T-01-05-05). T-01-05-02 (.wasm MIME) verified by both unit test and live curl. T-01-05-03 (streaming kernel) verified by 256 KB round-trip. T-01-05-04 (sha256 of kernel) accepted per UI-07.

## Known Stubs

None. All four handlers now return real data. `serve_index` legitimately returns 404 until plan 01-06 lands `crates/bootroom/web/index.html` — that's a planned dependency, not a stub.

## Self-Check: PASSED

Verified files exist:
- FOUND: crates/bootroom/src/kernel_info.rs
- FOUND: crates/bootroom/src/kernel_stream.rs
- FOUND: crates/bootroom/src/assets.rs
- FOUND: crates/bootroom/src/lib.rs (modified)
- FOUND: crates/bootroom/src/server.rs (modified)
- FOUND: Cargo.toml (modified)

Verified commits exist:
- FOUND: 8fe742e (Task 1: kernel_info + kernel_stream handlers)
- FOUND: 54f19b5 (Task 2: asset handler with --assets-dir + V12)
- FOUND: 2093927 (Task 3: wire real handlers into build_router)
