//! `GET /kernel` — streams the raw kernel bytes.
//!
//! Per Pitfall 5: uses `tokio_util::io::ReaderStream` so memory usage is
//! constant regardless of kernel size. No Range support in Phase 1.

use crate::state::AppState;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;

/// `GET /kernel` handler.
///
/// # Errors
///
/// Returns `404 NOT_FOUND` if the kernel file is missing (race against
/// startup validation in `01-04`), `403 FORBIDDEN` on permission denied,
/// or `500 INTERNAL_SERVER_ERROR` on any other I/O failure. Errors are
/// also logged server-side via `tracing::warn!`. WR-02.
pub async fn kernel_stream(
    State(s): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
    let f = tokio::fs::File::open(&s.kernel).await.map_err(|e| {
        tracing::warn!(error = %e, path = %s.kernel.display(), "kernel open failed");
        match e.kind() {
            std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            std::io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    })?;
    let stream = ReaderStream::new(f);
    let body = Body::from_stream(stream);
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use std::io::Write;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_kernel_stream_round_trip() {
        let p = std::env::temp_dir()
            .join(format!("bootroom-stream-{}", std::process::id()));
        let payload = vec![0xABu8; 256 * 1024];
        std::fs::File::create(&p)
            .unwrap()
            .write_all(&payload)
            .unwrap();
        let state = Arc::new(AppState::new_for_test(p.clone(), None));
        let resp = kernel_stream(State(state)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/octet-stream"
        );
        let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body_bytes.as_ref(), payload.as_slice());
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn test_kernel_stream_missing_file() {
        let state =
            Arc::new(AppState::new_for_test(PathBuf::from("/does/not/exist"), None));
        assert_eq!(
            kernel_stream(State(state)).await.unwrap_err(),
            StatusCode::NOT_FOUND
        );
    }
}
