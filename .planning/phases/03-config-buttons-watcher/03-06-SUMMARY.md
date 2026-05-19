---
phase: 03-config-buttons-watcher
plan: 06
subsystem: watcher
tags: [filesystem, debounce, broadcast, live-reload, elf-gate]
requires: [01, 02, 05]
provides: [WCH-01, WCH-02, WCH-03, WCH-04, WCH-05, CFG-10]
affects: [crates/bootroom/src/server.rs]
tech-stack-added:
  - notify 8.2.0
  - notify-debouncer-full 0.7.0
  - base64 0.22.1 (re-enabled from Phase-2 removal)
patterns:
  - "Single Debouncer + 2 watches + canonical-path demux (RESEARCH Pattern 1)"
  - "OS thread callback — sync APIs only (broadcast::Sender::send + RwLock::blocking_write); never tokio::spawn / .await / tokio::sleep (RESEARCH Pitfall #2)"
  - "Box::leak(debouncer) for process lifetime (RESEARCH Pattern 1 / Pitfall #7)"
  - "Size-stability inner check (100ms) + ELF magic gate (RESEARCH Pitfall #4)"
  - "ConfigInvalid keeps last-known-good (CFG-10 last-good-config preservation)"
key-files:
  created:
    - crates/bootroom/src/watcher.rs
    - crates/bootroom/tests/watcher_debounce.rs
    - crates/bootroom/tests/watcher_atomic_rename.rs
    - crates/bootroom/tests/watcher_size_stability.rs
    - crates/bootroom/tests/watcher_elf_magic.rs
    - crates/bootroom/tests/watcher_ws_frame.rs
    - crates/bootroom/tests/watcher_live_reload.rs
  modified:
    - crates/bootroom/Cargo.toml
    - crates/bootroom/src/server.rs
    - crates/bootroom/tests/common/mod.rs
decisions:
  - "Duplicate hash trade: watcher hashes the kernel itself (single fs::read + Sha256), DOES NOT write to digest_cache. Keeps digest_cache as single-writer (only /api/kernel/info writes). RESEARCH Pitfall accept; threat T-03-06-07 accept."
  - "ConfigUpdate uses RwLock::blocking_write (NOT tokio::sync::RwLock::write().await). Safe because the callback runs on the debouncer's owned OS thread — NOT inside any tokio runtime. Calling .await there would panic 'no reactor running' (Pitfall #2)."
  - "project_loaded_to_json lives in watcher.rs (not state.rs / not a new module). Plan 07's /api/config handler imports it directly — same JSON shape on both the WS broadcast and the HTTP GET surface."
  - "Watcher dies silently if its OS thread panics. Accepted (T-03-06-04). Phase 3 has no graceful-shutdown story; recovery = restart `bootroom serve`. Loudly logs Err(errs) from notify in the callback so OS-level errors surface at warn level."
metrics:
  duration: "~45 minutes"
  completed: "2026-05-19"
  tasks_completed: 2
  files_created: 7
  files_modified: 3
  tests_added: 9 # 3 unit + 6 integration
---

# Phase 3 Plan 06: Watcher Subsystem Summary

A single `notify-debouncer-full` instance watches both `bootroom.toml`
and the kernel's parent directory; the debounced callback demuxes events
by canonical-path equality, applies size-stability + ELF-magic gates
to kernel events, and broadcasts `WsMessage` frames to all `/ws`
subscribers. The same JSON projection helper (`project_loaded_to_json`)
is consumed by Plan 07's `/api/config` handler.

## Architecture

```text
bootroom.toml ───┐
                 ├──> Debouncer (300ms) ──> OS thread callback ──┬──> ConfigUpdate / ConfigInvalid
kernel parent ───┘                                                └──> KernelChanged (gated)
```

- **One Debouncer**: `notify-debouncer-full::new_debouncer(300ms, None, callback)`. `Box::leak`'d so its owned OS thread lives for the process lifetime.
- **Two watches**: `.watch(config_path_canon, NonRecursive)` and `.watch(kernel_canon.parent(), NonRecursive)`. Watching the kernel's *parent* dir is the atomic-rename-safety pattern (RESEARCH Pitfall #2) — `make` renames a sibling tempfile over the target, and the rename event only fires on the parent.
- **Demux**: inside the callback, each `DebouncedEvent`'s last path is compared by equality to `config_path_canon` (config branch) OR by `parent + file_name` against the kernel (kernel branch). Plan 05's startup canonicalization is what makes this a direct `==` comparison.
- **Sync APIs only**: the callback runs on the Debouncer's owned OS thread, NOT inside a tokio runtime. `broadcast::Sender::send` is sync; `RwLock::blocking_write` is correct here (calling tokio `.await` panics — RESEARCH Pitfall #2).

## Kernel-event gating (RESEARCH Pitfall #4)

`handle_kernel_change` runs three sequential gates before broadcasting `ok: true`:

1. **File exists**: `fs::metadata(path)` succeeds → record `s1`.
2. **Size stability**: `std::thread::sleep(100ms)` → re-read metadata → if `s1 != s2`, defer (next debounce tick retries).
3. **ELF magic**: open the file, `read_exact` 4 bytes, compare to `\x7f E L F`.

Non-ELF content broadcasts `KernelChanged { ok: false, reason: Some("not ELF") }`. Any I/O error during hashing broadcasts `KernelChanged { ok: false, reason: Some("hash error: ...") }`.

### The 100ms inner-check tuning (Assumption A3)

100ms is the documented sweet spot from `03-RESEARCH.md`: long enough that a real `make` link finishes (the kernel is fully on disk by the time the second metadata call returns), short enough that a clean rebuild still feels instant from the operator's perspective. The total worst-case latency from `make` finishing to the WS frame is `300ms (debounce) + 100ms (stability) + sha256` ≈ 400-700ms for a typical 5-40MB RISC-V kernel.

## Config-event flow (CFG-10)

`handle_config_change`:
- `fs::read_to_string` failure → `ConfigInvalid { error, line=None, col=None }`. (Mid-rename / chmod-a-r case.)
- `LoadedConfig::load_from_str` Err → `ConfigInvalid { error, line, col }`. AppState NOT mutated.
- Ok(loaded) → project to JSON via `project_loaded_to_json`, then `state.loaded_config.blocking_write()` swaps in the new config and broadcasts `ConfigUpdate { config }`.

Last-known-good is preserved on parse failure: an editor mid-save that leaves the file briefly invalid produces a transient `ConfigInvalid` frame, but the live AppState keeps the previous valid config — the next save fires another event with the fixed content.

## Hash duplication decision

The watcher computes its own SHA-256 over the kernel rather than writing through `state.digest_cache`. Rationale (RESEARCH + threat T-03-06-07):

- `digest_cache` stays single-writer (only `/api/kernel/info` updates it). Two writers on the same `Arc<tokio::sync::RwLock<_>>` would be a race source.
- The watcher's hash runs on its OS thread (sync `fs::read` + `Sha256::update`); `/api/kernel/info`'s hash runs on a tokio worker (`tokio::fs` + chunked read). Different paths, different cache lifetimes.
- The cost is one extra read per kernel rebuild — accepted since kernel rebuilds are infrequent.

## Lagged-broadcast acceptance

Per RESEARCH Pitfall #3 and threat T-03-06-01, `broadcast::Sender::send` returning `SendError` (no subscribers) is silently swallowed via `let _ = tx.send(...)`. The `Lagged(n)` semantics are intentional: the capacity-16 channel drops the oldest frames for slow subscribers (logged at warn by Plan 08's forwarder). For `ConfigUpdate` specifically, a dropped frame is recovered by the next full-replacement update on the subsequent edit (T-03-06-05 mitigate).

## Test results (6 integration + 3 unit, all green)

| Test | Requirement | Duration | Asserts |
|------|-------------|----------|---------|
| `watcher_debounce::burst_collapses_to_one_event` | WCH-01 | ~1.0s | 5 rapid writes → exactly 1 `KernelChanged` broadcast |
| `watcher_atomic_rename::tempfile_rename_fires_kernel_changed` | WCH-02 | ~0.5s | sibling write + rename → `KernelChanged { ok: true, size: 64 }` |
| `watcher_size_stability::partial_write_held_until_stable` | WCH-03 | ~2.7s | partial 32B → 64B settle; final frame shows settled (64B) size |
| `watcher_elf_magic::non_elf_yields_ok_false` | WCH-04 | ~0.5s | shell script → `KernelChanged { ok: false, reason: "not ELF" }` |
| `watcher_ws_frame::kernel_changed_payload_shape` | WCH-05 | ~0.5s | ok=true, mtime>0, size=64, sha256_prefix=12 hex chars |
| `watcher_live_reload::toml_reload` | CFG-10 | ~0.7s | valid → ConfigUpdate + state mutate; invalid → ConfigInvalid + state preserved |
| `watcher::tests::project_loaded_to_json_shape` | unit | <1ms | bytes_b64 roundtrip; null group/description |
| `watcher::tests::project_loaded_to_json_preserves_action_order` | unit / CFG-09 | <1ms | alpha/beta/gamma stay in declaration order |
| `watcher::tests::project_loaded_to_json_includes_scenarios_with_assertions` | unit | <1ms | scenarios + assertions inline via serde |

`cargo clippy -p bootroom --lib --tests -- -D warnings` clean. `cargo test --workspace` green.

## Watcher silently dies if OS thread panics (operator note)

Phase 3 has no graceful-shutdown or watchdog story (T-03-06-04 explicit accept). If the `notify` OS thread panics — e.g. inotify watch descriptor limit exceeded mid-run, FUSE volume disconnected — the watcher stops emitting events and bootroom continues serving stale config and a frozen kernel-changed signal. Recovery is "restart `bootroom serve`". The callback logs `Err(errs)` from notify at `warn` level so OS-level errors are at least visible in the log.

A post-MVP refactor (out of scope for Phase 3) would store the Debouncer in `AppState` and add a periodic heartbeat from a tokio task that asserts the watcher thread is alive.

## Deviations from Plan

### Cross-agent commit collision (informational; no functional impact)

The plan's Task 1 work (Cargo.toml deps + watcher.rs scaffold + Cargo.lock) was committed transparently as part of commit `669f1e4` by the concurrent plan 03-04 executor — they ran `git add` against modified files that included my staged-but-uncommitted edits to those shared paths. This was a near-miss of the scenario the orchestrator prompt warned about ("CRITICAL: Another agent is concurrently running plan 03-04 ... If a commit collision happens, `git pull --rebase` and retry").

In this case the collision was benign — the agent's commit included the correct file contents — but the per-task commit message ("feat(03-06): add notify deps + watcher.rs scaffold ...") was lost. Task 2 was committed cleanly as `33333db` ("feat(03-06): wire spawn_watcher into server::run + 6 integration tests"). All Task 1 behaviors (3 unit tests, deps wired, watcher.rs created) are present in the working tree and pass verification.

### Auto-fixed during Task 2 (Rule 3 — blocking)

**1. [Rule 3 - Blocker] Stub init_cmd.rs created temporarily**
- **Found during:** Task 1 verify
- **Issue:** Plan 03-04 had declared `pub mod init_cmd;` in lib.rs but not yet created `init_cmd.rs`, breaking the build for any concurrent agent.
- **Fix:** Wrote a placeholder `init_cmd.rs` with a doc comment so the lib could compile; later removed once 03-04 committed their real implementation.
- **Files modified:** crates/bootroom/src/init_cmd.rs (stub; removed before final commit)
- **Outcome:** Workaround for cross-agent in-flight state; net change in this plan = none.

### Auto-fixed during Task 2 (clippy pedantic)

**2. [Rule 1 - Bug] backtick-missing doc comments**
- **Issue:** `cargo clippy --tests -- -D warnings` rejected `Arc<RwLock<_>>` / `metadata()` / `sha256_prefix` etc. in doc comments under `pedantic::doc_markdown`.
- **Fix:** Backticked the identifiers in `watcher.rs`, `watcher_ws_frame.rs`, `watcher_size_stability.rs`, `common/mod.rs`.
- **Commit:** `33333db`.

**3. [Rule 1 - Bug] u64-as-usize cast in `watcher_elf_magic.rs`**
- **Fix:** Replaced `size as usize` with `usize::try_from(size).unwrap()` to silence `clippy::cast_possible_truncation`.
- **Commit:** `33333db`.

**4. [Rule 1 - Bug] redundant `continue` + identical-arm match in `watcher_live_reload.rs`**
- **Fix:** Merged the two `ConfigUpdate | ConfigInvalid` arms via or-pattern; replaced `continue` with empty arm bodies (already at end of match).
- **Commit:** `33333db`.

**5. [Rule 2 - Critical functionality] `#[allow(needless_pass_by_value)]` on `spawn_watcher`**
- **Rationale:** `state` is consumed twice (once into the leaked debouncer closure with `'static` lifetime, once for the `info!` log). Forcing every caller to `state.clone()` explicitly would obscure the invariant that the watcher takes ownership of an `Arc` clone. Documented inline.

## Threat Flags

None — no new security surface beyond the threat model already documented in PLAN frontmatter.

## Self-Check: PASSED

- `crates/bootroom/src/watcher.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_debounce.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_atomic_rename.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_size_stability.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_elf_magic.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_ws_frame.rs` exists (FOUND)
- `crates/bootroom/tests/watcher_live_reload.rs` exists (FOUND)
- `crates/bootroom/Cargo.toml` lists notify + notify-debouncer-full + base64 (FOUND)
- commit `669f1e4` contains watcher.rs + Cargo.toml deps (FOUND via parallel-agent sweep)
- commit `33333db` contains server::run wire + 6 integration tests (FOUND)
- `cargo test -p bootroom` green (VERIFIED — 0 failures)
- `cargo clippy -p bootroom --lib --tests -- -D warnings` clean (VERIFIED)
