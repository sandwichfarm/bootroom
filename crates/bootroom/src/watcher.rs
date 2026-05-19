//! Filesystem watcher subsystem (Phase 3, plan 03-06).
//!
//! # Architecture
//!
//! A single `notify-debouncer-full` instance owns one OS thread and watches
//! two paths simultaneously (RESEARCH Pattern 1):
//!
//! 1. `AppState.config_path_canon` — the canonical absolute path to
//!    `bootroom.toml`. Edits broadcast either `ConfigUpdate { config }`
//!    (valid) or `ConfigInvalid { error, line, col }` (invalid).
//! 2. `AppState.kernel_canon.parent()` — the directory containing the
//!    kernel image (NOT the file itself). Atomic-rename safe: `make`
//!    typically writes to a sibling tempfile then renames over the
//!    target, which on Linux fires `Rename` on the *parent* — watching
//!    the file directly would miss it (Pitfall #2 / RESEARCH).
//!
//! Demux happens inside the debounced callback by canonical-path
//! equality (Pitfall #1 mitigation — Plan 05 canonicalizes both paths
//! at startup precisely so this comparison is reliable).
//!
//! # Threading
//!
//! The callback runs on the Debouncer's *owned OS thread* — NOT on any
//! tokio runtime. This means:
//!
//! - Use `broadcast::Sender::send` (sync) for fan-out.
//! - Use `RwLock::blocking_write` for state mutation.
//! - Never call `.await`; never call `tokio::spawn`.
//! - Never call `tokio::time::sleep` — use `std::thread::sleep`.
//!
//! Calling tokio APIs from this thread panics with "there is no reactor
//! running" (RESEARCH Pitfall #2).
//!
//! # Process lifetime
//!
//! The debouncer is `Box::leak`'d so its OS thread lives for the lifetime
//! of the process (RESEARCH Pattern 1 / Pitfall #7). bootroom has no
//! graceful shutdown story in Phase 3; if the OS thread panics silently
//! the recovery is "restart `bootroom serve`". The callback logs at warn
//! when `Err(errs)` arrives so OS-level errors are at least visible.
//!
//! # Kernel-event gating (RESEARCH Pitfall #4)
//!
//! `handle_kernel_change` defers any broadcast until the file passes:
//! 1. Size stability across a 100ms inner-check window (rejects
//!    partial-write races during `make` linking).
//! 2. ELF magic check on the first four bytes (`\x7f E L F`); non-ELF
//!    files broadcast `KernelChanged { ok: false, reason: "not ELF" }`.
//!
//! # Hashing
//!
//! The watcher computes its own SHA-256 of the kernel rather than going
//! through `digest_cache`. RESEARCH calls this out: keeping `digest_cache`
//! as a single-writer path (only `/api/kernel/info` writes; the watcher
//! reads its own file) avoids a two-writer race on the same `Arc<RwLock<_>>`.
//! The cost is a duplicate hash of the kernel per rebuild — kernel reads
//! are infrequent and the file is the operator's chosen artifact, so this
//! is accepted (threat T-03-06-07).

use crate::state::AppState;
use base64::{Engine, engine::general_purpose::STANDARD};
use bootroom_core::config::LoadedConfig;
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

/// Magic bytes at offset 0 of every ELF file.
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Debounce window for filesystem events. RESEARCH Pattern 1: a single
/// `make` produces 3-10 events on most filesystems; 300ms is long enough
/// to coalesce them yet short enough to feel "instant" to the operator.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(300);

/// Inner-check window for kernel size stability. RESEARCH Pitfall #4
/// (Assumption A3): a partial-write race during `make` linking can see
/// the kernel grow between the first `metadata()` and the read. We sleep
/// 100ms and re-check size; if it changed, defer to the next debounce
/// tick.
const SIZE_STABILITY_WINDOW: Duration = Duration::from_millis(100);

/// Project a `LoadedConfig` into the JSON shape consumed by `/api/config`
/// (Plan 07) and `WsMessage::ConfigUpdate` (this plan).
///
/// Shape (RESEARCH Pattern 5):
///
/// ```json
/// {
///   "schema_version": 1,
///   "actions": [
///     { "label": "...", "bytes_b64": "...", "group": null|"...", "description": null|"..." }
///   ],
///   "scenarios": [
///     { "name": "...", "actions": ["..."], "assertions": [...], "timeout_ms": 30000 }
///   ]
/// }
/// ```
///
/// `bytes_b64` is base64(`ResolvedAction.bytes_decoded`) so the browser
/// can decode and dispatch byte-perfect `SerialIn` frames without re-running
/// the escape decoder. Action order is preserved (CFG-09).
#[must_use]
pub fn project_loaded_to_json(loaded: &LoadedConfig) -> Value {
    let actions: Vec<Value> = loaded
        .actions()
        .iter()
        .map(|a| {
            json!({
                "label": a.label,
                "bytes_b64": STANDARD.encode(&a.bytes_decoded),
                "group": a.group,
                "description": a.description,
            })
        })
        .collect();

    let scenarios: Vec<Value> = loaded
        .scenarios()
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "actions": s.actions,
                "assertions": s.assertions,
                "timeout_ms": s.timeout_ms,
            })
        })
        .collect();

    json!({
        "schema_version": 1,
        "actions": actions,
        "scenarios": scenarios,
    })
}

/// Spawn the filesystem watcher. The debouncer is `Box::leak`'d so its
/// OS thread lives for the process lifetime (no graceful shutdown in
/// Phase 3 — see module doc).
///
/// # Errors
///
/// Returns an error if:
/// - The kernel canonical path has no parent (root-of-fs edge case).
/// - The kernel canonical path has no file name.
/// - `new_debouncer` fails to start its OS thread.
/// - Either `.watch(...)` call fails (path does not exist, permissions, etc.).
// `state` is consumed: we clone it once into the leaked debouncer closure
// (which outlives this call) and once for the `info!` log. Taking `&Arc`
// would force every caller to clone explicitly at the call site.
#[allow(clippy::needless_pass_by_value)]
pub fn spawn_watcher(state: Arc<AppState>) -> anyhow::Result<()> {
    let kernel_parent: std::path::PathBuf = state
        .kernel_canon
        .parent()
        .ok_or_else(|| anyhow::anyhow!("kernel_canon has no parent dir: {}", state.kernel_canon.display()))?
        .to_path_buf();
    let kernel_basename: std::ffi::OsString = state
        .kernel_canon
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("kernel_canon has no file name: {}", state.kernel_canon.display()))?
        .to_os_string();
    let config_path: std::path::PathBuf = state.config_path_canon.clone();
    // CR-03: derive the parent dir + basename for the config too. Watching
    // the config FILE directly fails the same atomic-rename trap that
    // motivates watching the kernel's parent dir (Pitfall #2): vim with
    // `:set writebackup`, VS Code, JetBrains IDEs, `git checkout`, and
    // any `make`-driven config regenerator all save by writing a sibling
    // tempfile and renaming it over the target. inotify watches attach
    // to the inode; after one rename the watch follows the now-orphaned
    // old inode and every subsequent edit is silently missed.
    let config_parent: std::path::PathBuf = config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config_path_canon has no parent dir: {}", config_path.display()))?
        .to_path_buf();
    let config_basename: std::ffi::OsString = config_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("config_path_canon has no file name: {}", config_path.display()))?
        .to_os_string();

    let state_for_callback = state.clone();
    let kernel_parent_cb = kernel_parent.clone();
    let kernel_basename_cb = kernel_basename.clone();
    let config_parent_cb = config_parent.clone();
    let config_basename_cb = config_basename.clone();

    let mut debouncer = new_debouncer(
        DEBOUNCE_WINDOW,
        None,
        move |result: DebounceEventResult| {
            let events = match result {
                Ok(events) => events,
                Err(errs) => {
                    for e in errs {
                        tracing::warn!(error = ?e, "watcher: notify error");
                    }
                    return;
                }
            };

            let mut config_dirty = false;
            let mut kernel_dirty = false;

            for ev in events {
                // Rename pairs put dst last (notify-debouncer-full preserves
                // ordering); fall back to the first path for non-rename events.
                let target = ev
                    .event
                    .paths
                    .last()
                    .or_else(|| ev.event.paths.first());
                let Some(target) = target else {
                    continue;
                };

                // CR-03 config demux: same parent dir AND same basename.
                // Mirrors the kernel demux below. Atomic-rename saves
                // report the rename with paths in the config's parent
                // directory; basename equality catches both the in-place
                // POSIX write (target == config_path) and the rename-over
                // (a tempfile renamed to the canonical name).
                if target.parent() == Some(config_parent_cb.as_path())
                    && target.file_name() == Some(config_basename_cb.as_os_str())
                {
                    config_dirty = true;
                    continue;
                }

                // Kernel demux: same parent dir AND same filename.
                if target.parent() == Some(kernel_parent_cb.as_path())
                    && target.file_name() == Some(kernel_basename_cb.as_os_str())
                {
                    match ev.event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                            kernel_dirty = true;
                        }
                        _ => {}
                    }
                }
            }

            if config_dirty {
                handle_config_change(&state_for_callback);
            }
            if kernel_dirty {
                handle_kernel_change(&state_for_callback);
            }
        },
    )?;

    // CR-03 + Pitfall #2: watch the PARENT directory of both the config
    // and the kernel (NOT the files directly). inotify watches attach
    // to the inode; after an atomic-rename save (write tempfile then
    // rename over target) a file-level watch follows the orphaned old
    // inode and every subsequent edit is silently missed. Watching the
    // parent dir captures the rename pair regardless. The callback
    // demuxes by basename equality.
    //
    // Edge case: if `config_parent == kernel_parent` (operator placed
    // bootroom.toml next to the kernel), `notify`'s debouncer deduplicates
    // overlapping watches on the same dir — calling `watch` twice for
    // the same path is idempotent.
    debouncer.watch(&config_parent, RecursiveMode::NonRecursive)?;
    debouncer.watch(&kernel_parent, RecursiveMode::NonRecursive)?;

    // WR-03: deliberately leak the debouncer so its owned OS thread
    // lives for the process lifetime. If dropped the thread exits and
    // we lose the watcher silently — Phase 3 has no graceful shutdown
    // story (documented in the module doc + threat T-03-06-04).
    //
    // `std::mem::forget` is the simplest spelling: it consumes the
    // value without running its destructor and without the dance of
    // `Box::leak(Box::new(...))` + a no-op `let _ = &*leaked` to
    // suppress unused-binding warnings.
    std::mem::forget(debouncer);

    tracing::info!(
        config = %config_path.display(),
        kernel_parent = %kernel_parent.display(),
        kernel = %state.kernel_canon.display(),
        "watcher spawned"
    );

    Ok(())
}

/// Handle a debounced kernel-path event. Per RESEARCH Pitfall #4:
/// size-stability gate (100ms inner check) → ELF-magic gate → hash.
///
/// All I/O is synchronous; this runs on the debouncer's OS thread.
fn handle_kernel_change(state: &AppState) {
    let tx = &state.ws_broadcast;
    let path = &state.kernel_canon;

    // Gate 1: file currently exists?
    let s1 = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => {
            // File gone (mid-rename, mid-rm). Next debounce tick will
            // retry; do not broadcast.
            return;
        }
    };

    // Gate 2: size stability across 100ms.
    std::thread::sleep(SIZE_STABILITY_WINDOW);
    let s2 = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return,
    };
    if s1 != s2 {
        tracing::debug!(
            s1 = s1,
            s2 = s2,
            path = %path.display(),
            "kernel size unstable; deferring"
        );
        return;
    }

    // Gate 3: ELF magic. Open + read_exact 4 bytes.
    let mut magic = [0u8; 4];
    let elf_ok = match std::fs::File::open(path) {
        Ok(mut f) => f.read_exact(&mut magic).is_ok() && magic == ELF_MAGIC,
        Err(_) => false,
    };

    if !elf_ok {
        let _ = tx.send(bootroom_core::WsMessage::KernelChanged {
            ok: false,
            mtime: 0,
            size: s1,
            sha256_prefix: String::new(),
            reason: Some("not ELF".into()),
        });
        return;
    }

    // mtime (seconds since epoch). Safe to cast: u64 secs fits i64 until y2554.
    let mtime: i64 = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| {
            i64::try_from(d.as_secs()).unwrap_or(i64::MAX)
        });

    // WR-01: stream the kernel into the hasher rather than reading the
    // whole file into a Vec<u8>. RISC-V kernel images can be tens to
    // hundreds of MB and a CI runner doing back-to-back rebuilds will
    // spike RSS otherwise. The duplicate-hash trade-off vs the
    // `digest_cache` path is unchanged (kept single-writer per RESEARCH).
    let sha256_prefix = match hash_file_streaming(path) {
        Ok(prefix) => prefix,
        Err(e) => {
            let _ = tx.send(bootroom_core::WsMessage::KernelChanged {
                ok: false,
                mtime: 0,
                size: s1,
                sha256_prefix: String::new(),
                reason: Some(format!("hash error: {e}")),
            });
            return;
        }
    };

    let _ = tx.send(bootroom_core::WsMessage::KernelChanged {
        ok: true,
        mtime,
        size: s1,
        sha256_prefix,
        reason: None,
    });
}

/// WR-01: SHA-256 a file in 64 KiB chunks without buffering the whole
/// contents in memory. Returns the first 12 hex chars (= first 6 bytes
/// of the digest), matching the prior `hex::encode(&digest[..6])` shape.
///
/// On a kernel of N bytes the peak heap usage is O(64 KiB) regardless
/// of N, vs O(N) for `std::fs::read(path) -> Vec<u8>`. Sha256 itself
/// keeps a fixed-size state, so no allocations grow with input size.
fn hash_file_streaming(path: &std::path::Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(hex::encode(&digest[..6]))
}

/// Handle a debounced config-path event. Either broadcasts `ConfigUpdate`
/// (and replaces `state.loaded_config` in place) or `ConfigInvalid` (and
/// leaves the last-known-good config untouched — CFG-10).
fn handle_config_change(state: &AppState) {
    let tx = &state.ws_broadcast;
    let path = &state.config_path_canon;

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(bootroom_core::WsMessage::ConfigInvalid {
                error: format!("{e}"),
                line: None,
                col: None,
            });
            return;
        }
    };

    match LoadedConfig::load_from_str(&content) {
        Ok(loaded) => {
            // blocking_write is correct here: we're on the debouncer's
            // owned OS thread (NOT inside any tokio runtime), so calling
            // tokio::sync::RwLock::blocking_write does not panic.
            let projected = project_loaded_to_json(&loaded);
            {
                let mut guard = state.loaded_config.blocking_write();
                *guard = loaded;
            }
            let _ = tx.send(bootroom_core::WsMessage::ConfigUpdate { config: projected });
        }
        Err(e) => {
            let _ = tx.send(bootroom_core::WsMessage::ConfigInvalid {
                error: e.message,
                line: e.line,
                col: e.col,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a 2-action TOML and project to JSON. Verifies the
    /// shape contracted with Plan 07.
    #[test]
    fn project_loaded_to_json_shape() {
        let toml = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = 'reboot\r'
group = "system"
description = "Soft reboot via init"

[[action]]
label = "halt"
bytes = 'halt\r'
"#;
        let loaded = LoadedConfig::load_from_str(toml).expect("load");
        let v = project_loaded_to_json(&loaded);

        assert_eq!(v["schema_version"].as_u64(), Some(1));
        let actions = v["actions"].as_array().expect("actions array");
        assert_eq!(actions.len(), 2);

        // First action: reboot — base64 of "reboot\r" (CR is 0x0d).
        assert_eq!(actions[0]["label"].as_str(), Some("reboot"));
        let b64 = actions[0]["bytes_b64"].as_str().expect("bytes_b64 str");
        let decoded = STANDARD.decode(b64).expect("decode b64");
        assert_eq!(decoded, b"reboot\r");
        assert_eq!(actions[0]["group"].as_str(), Some("system"));
        assert_eq!(
            actions[0]["description"].as_str(),
            Some("Soft reboot via init")
        );

        // Second action: halt — group/description null.
        assert_eq!(actions[1]["label"].as_str(), Some("halt"));
        let b64 = actions[1]["bytes_b64"].as_str().expect("bytes_b64 str");
        let decoded = STANDARD.decode(b64).expect("decode b64");
        assert_eq!(decoded, b"halt\r");
        assert!(
            actions[1]["group"].is_null(),
            "group should serialize to null when None"
        );
        assert!(
            actions[1]["description"].is_null(),
            "description should serialize to null when None"
        );

        // No scenarios in this TOML.
        let scenarios = v["scenarios"].as_array().expect("scenarios array");
        assert!(scenarios.is_empty());
    }

    /// CFG-09 prerequisite: projection preserves action insertion order.
    #[test]
    fn project_loaded_to_json_preserves_action_order() {
        let toml = r#"
schema_version = 1

[[action]]
label = "alpha"
bytes = "a"

[[action]]
label = "beta"
bytes = "b"

[[action]]
label = "gamma"
bytes = "c"
"#;
        let loaded = LoadedConfig::load_from_str(toml).expect("load");
        let v = project_loaded_to_json(&loaded);
        let actions = v["actions"].as_array().expect("actions array");
        let labels: Vec<&str> = actions
            .iter()
            .map(|a| a.get("label").and_then(Value::as_str).expect("label"))
            .collect();
        assert_eq!(labels, vec!["alpha", "beta", "gamma"]);
    }

    /// Scenario projection includes assertions inlined via serde.
    #[test]
    fn project_loaded_to_json_includes_scenarios_with_assertions() {
        let toml = r#"
schema_version = 1

[[action]]
label = "reboot"
bytes = "x"

[[scenario]]
name = "smoke"
actions = ["reboot"]
timeout_ms = 10000

  [[scenario.assert]]
  kind = "contains"
  pattern = "Booting"
  after = "reboot"
  timeout_ms = 2000
"#;
        let loaded = LoadedConfig::load_from_str(toml).expect("load");
        let v = project_loaded_to_json(&loaded);
        let scenarios = v["scenarios"].as_array().expect("scenarios array");
        assert_eq!(scenarios.len(), 1);
        let s = &scenarios[0];
        assert_eq!(s["name"].as_str(), Some("smoke"));
        assert_eq!(s["timeout_ms"].as_u64(), Some(10_000));
        let asserts = s["assertions"].as_array().expect("assertions");
        assert_eq!(asserts.len(), 1);
        assert_eq!(asserts[0]["kind"].as_str(), Some("contains"));
        assert_eq!(asserts[0]["pattern"].as_str(), Some("Booting"));
    }
}
