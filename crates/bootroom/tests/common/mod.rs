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
    let state = Arc::new(AppState::new(kernel, assets_dir));
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

/// Write a tempfile with `bytes`; caller drops the returned `NamedTempFile`
/// when the test scope ends (auto-deletes from disk).
pub fn write_kernel_tempfile(bytes: &[u8]) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(bytes).expect("write");
    f
}
