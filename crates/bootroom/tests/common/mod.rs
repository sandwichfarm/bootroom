//! Common test helpers — spawn an ephemeral-port bootroom server.
//!
//! Files under `tests/common/` are NOT compiled as separate test binaries; each
//! integration-test file pulls them in via `mod common;` at the top.

#![allow(dead_code)] // Not every test file uses every helper.

use bootroom::{AppState, build_router};
use std::{path::PathBuf, sync::Arc};
use tokio::net::TcpListener;

pub struct TestServer {
    pub base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

/// WR-06: abort the spawned server task on drop. The previous
/// `_handle` field detached the `JoinHandle`, so the task lived until
/// the test's tokio runtime tore down — every test leaked a listener
/// and a `.expect("axum::serve")` panic inside the task became
/// invisible. Aborting on drop releases the port immediately when
/// the test scope ends and lets us drop the `expect` (a leaked task
/// can no longer drag a hidden panic into the runtime).
impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Spawn a bootroom HTTP server on an ephemeral port.
/// The server runs on the same tokio runtime as the test.
pub async fn spawn(kernel: PathBuf, assets_dir: Option<PathBuf>) -> TestServer {
    let state = Arc::new(AppState::new_for_test(kernel, assets_dir));
    let app = build_router(state);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind 0");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        // Ignore the result: when TestServer is dropped we abort the
        // task, which causes axum::serve to return an error that we
        // don't want to surface as a panic.
        let _ = axum::serve(listener, app).await;
    });
    TestServer {
        base_url: format!("http://{addr}"),
        handle,
    }
}

/// Spawn a bootroom HTTP server with an arbitrary `LoadedConfig`.
///
/// Built for the Plan 03-07 `/api/config` integration tests: the caller
/// supplies any TOML string and (optional) CLI `--action` overrides, and
/// we wire the resulting `LoadedConfig` directly into `AppState` via
/// [`bootroom::AppState::new_for_test_with_loaded`]. No real watcher runs;
/// no `bootroom.toml` is written to disk.
pub async fn spawn_with_loaded(
    kernel: PathBuf,
    loaded: bootroom_core::config::LoadedConfig,
) -> TestServer {
    let state = Arc::new(bootroom::AppState::new_for_test_with_loaded(
        kernel, None, loaded,
    ));
    let app = build_router(state);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind 0");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    TestServer {
        base_url: format!("http://{addr}"),
        handle,
    }
}

/// Spawn a bootroom HTTP server AND return the `Arc<AppState>` so the
/// test can call `state.ws_broadcast.send(...)` directly. Used by
/// `ws_broadcast_fanout.rs` to verify Plan 03-08's per-connection
/// broadcast forwarder fans frames out to every connected client.
///
/// Kept distinct from [`spawn`] (Phase 2) — this helper's sole job is
/// to expose the `AppState` handle so the test can publish broadcast
/// frames without going through HTTP / the watcher.
pub async fn spawn_with_broadcast_handle(
    kernel: PathBuf,
    assets_dir: Option<PathBuf>,
) -> (TestServer, Arc<AppState>) {
    let state = Arc::new(AppState::new_for_test(kernel, assets_dir));
    let app = build_router(state.clone());
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind 0");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let server = TestServer {
        base_url: format!("http://{addr}"),
        handle,
    };
    (server, state)
}

/// Write a tempfile with `bytes`; caller drops the returned `NamedTempFile`
/// when the test scope ends (auto-deletes from disk).
pub fn write_kernel_tempfile(bytes: &[u8]) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(bytes).expect("write");
    f
}

/// A minimal 64-byte ELF stub: magic + zero padding. Enough to pass the
/// watcher's first-4-bytes ELF magic gate. The size-stability gate also
/// passes because the file is written atomically in one `fs::write` call.
pub const ELF_STUB_64: [u8; 64] = {
    let mut out = [0u8; 64];
    out[0] = 0x7f;
    out[1] = b'E';
    out[2] = b'L';
    out[3] = b'F';
    out
};

/// Trivial valid `bootroom.toml` content with one action. The watcher
/// integration tests start from this and then rewrite to exercise
/// reload paths.
pub const INITIAL_TOML: &str = "schema_version = 1\n\n\
    [[action]]\n\
    label = \"reboot\"\n\
    bytes = 'reboot\\r'\n";

/// Set up a fresh `AppState` for watcher integration tests.
///
/// Returns `(state, _temp_guard, kernel_path, config_path)`.
///
/// - Kernel file is initialized with `ELF_STUB_64` so the watcher's
///   initial scan would (if it ran) see a valid kernel.
/// - Config file is initialized with `INITIAL_TOML` (one action: reboot).
/// - Paths are canonicalized (the live `server::run` canonicalize step is
///   what the watcher relies on for path demux; we mirror it here).
///
/// The `TempDir` guard MUST be held by the caller for the duration of
/// the test — dropping it deletes the kernel + config files on disk
/// and the watcher will fire spurious Remove events.
pub fn spawn_watcher_test_setup() -> (
    std::sync::Arc<bootroom::AppState>,
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    use bootroom_core::config::LoadedConfig;
    use std::sync::Arc;

    let tmp = tempfile::tempdir().expect("tempdir");
    let kernel_path = tmp.path().join("Image");
    let config_path = tmp.path().join("bootroom.toml");

    std::fs::write(&kernel_path, ELF_STUB_64).expect("write kernel stub");
    std::fs::write(&config_path, INITIAL_TOML).expect("write initial toml");

    let kernel_canon = std::fs::canonicalize(&kernel_path).expect("canonicalize kernel");
    let config_path_canon =
        std::fs::canonicalize(&config_path).expect("canonicalize config");

    let loaded = LoadedConfig::load_from_str(INITIAL_TOML).expect("load initial");

    let state = Arc::new(bootroom::AppState::new(
        kernel_path.clone(),
        kernel_canon,
        None, // assets_dir
        config_path.clone(),
        config_path_canon,
        loaded,
    ));

    (state, tmp, kernel_path, config_path)
}
