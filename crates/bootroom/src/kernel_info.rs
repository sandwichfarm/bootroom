//! `GET /api/kernel/info` — returns kernel metadata + sha256 prefix.
//!
//! Covers UI-07's API surface. The DOM rendering is plan 01-06's job.

use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{io::ErrorKind, sync::Arc, time::UNIX_EPOCH};
use tokio::io::AsyncReadExt;

/// WR-02: map an `io::Error` to a meaningful HTTP status instead of
/// coercing everything to 404. Permission denied → 403; any other I/O
/// failure → 500. The error is logged via `tracing::warn!` server-side
/// so the operator can diagnose even though the browser only sees a
/// status code.
fn io_to_status(e: &std::io::Error) -> StatusCode {
    match e.kind() {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct KernelInfo {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub sha256_prefix: String,
}

/// `GET /api/kernel/info` handler.
///
/// Reads metadata + streams the kernel file through SHA-256 (constant memory)
/// and returns the four-field JSON per `01-UI-SPEC.md`.
///
/// # Errors
///
/// Returns `404 NOT_FOUND` if the kernel file is missing (validated at
/// startup, but may have been deleted since), `403 FORBIDDEN` if the
/// process lost read permission, or `500 INTERNAL_SERVER_ERROR` for
/// any other I/O failure (mid-stream or otherwise). All errors are
/// also logged server-side via `tracing::warn!`.
#[allow(clippy::cast_possible_wrap)] // mtime well within i64 range until year 292277
pub async fn kernel_info(
    State(s): State<Arc<AppState>>,
) -> Result<Json<KernelInfo>, StatusCode> {
    let meta = tokio::fs::metadata(&s.kernel).await.map_err(|e| {
        tracing::warn!(error = %e, path = %s.kernel.display(), "kernel metadata failed");
        io_to_status(&e)
    })?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);

    let mut f = tokio::fs::File::open(&s.kernel).await.map_err(|e| {
        tracing::warn!(error = %e, path = %s.kernel.display(), "kernel open failed");
        io_to_status(&e)
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await.map_err(|e| {
            tracing::warn!(error = %e, "kernel read failed mid-hash");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let sha256_prefix = hex::encode(&digest[..6]); // 12 hex chars

    Ok(Json(KernelInfo {
        path: s.kernel.display().to_string(),
        size,
        mtime,
        sha256_prefix,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_tmp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "bootroom-test-{}-{}",
            std::process::id(),
            name
        ));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[tokio::test]
    async fn test_kernel_info_known_bytes() {
        let p = write_tmp("abc", b"abc");
        let state = Arc::new(AppState::new(p.clone(), None));
        let Json(info) = kernel_info(State(state)).await.unwrap();
        assert_eq!(info.size, 3);
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(info.sha256_prefix, "ba7816bf8f01");
        assert_eq!(info.path, p.display().to_string());
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn test_kernel_info_missing_file() {
        let state = Arc::new(AppState::new(
            PathBuf::from("/does/not/exist/at/all"),
            None,
        ));
        let err = kernel_info(State(state)).await.unwrap_err();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }
}
