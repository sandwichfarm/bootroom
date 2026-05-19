//! UI + qemu-wasm asset handler. Dispatches to embedded `include_dir!`
//! roots or to `--assets-dir <dir>` on disk when set.
//!
//! Per `CONTEXT.md` `<specifics>` Pitfall 3: `--assets-dir` covers BOTH
//! `web/` and `assets/qemu/` subtrees so dev iteration doesn't get
//! confused by stale embeds.
//!
//! Per ASVS V12: path-traversal protection on the disk override path.

use crate::{
    embed::{QEMU, WEB},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use include_dir::Dir;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// `GET /` — serves the embedded (or overridden) `web/index.html`.
pub async fn serve_index(State(s): State<Arc<AppState>>) -> Response {
    serve_one(&s, "web/index.html").await
}

/// `GET /assets/{*path}` — serves files from `web/` or `qemu/` subtrees.
pub async fn serve_asset(
    State(s): State<Arc<AppState>>,
    AxumPath(rest): AxumPath<String>,
) -> Response {
    serve_one(&s, &rest).await
}

/// Disk-branch outcome (CR-02): distinguishes "file is genuinely absent;
/// fall through to embedded" from "file exists but resolves outside the
/// assets-dir root; hard-reject and do NOT consult the embedded fallback".
enum DiskOutcome {
    /// Serve this response from disk.
    Hit(Response),
    /// File is not on disk; caller may fall through to embedded.
    Miss,
    /// Path-traversal or unexpected I/O error; respond directly and do
    /// NOT consult the embedded fallback. This closes the race where a
    /// traversal target that exists on disk but fails canonicalization
    /// (permission denied, race-removed) would silently fall through to
    /// an embedded copy with no security check.
    Reject(Response),
}

async fn serve_one(state: &AppState, requested: &str) -> Response {
    // Reject obvious traversal and dangerous separators before touching
    // disk or embed. Defense in depth — axum decodes percent-escapes
    // before invoking the handler so URL-encoded `%2e%2e` arrives here
    // as `..` and is caught, but the explicit `\0` and `\\` rejections
    // guard against future routing changes that might bypass the URL
    // decoder.
    for seg in requested.split('/') {
        if seg == ".." || seg.contains('\\') || seg.contains('\0') {
            return (
                StatusCode::BAD_REQUEST,
                "invalid path: traversal or unsafe separator",
            )
                .into_response();
        }
    }
    // Disk override first.
    if let Some(root) = &state.assets_dir {
        match try_disk(state, root, requested).await {
            DiskOutcome::Hit(resp) | DiskOutcome::Reject(resp) => return resp,
            DiskOutcome::Miss => {} // fall through to embedded
        }
    }
    // Embedded fallback.
    if let Some((dir, sub)) = split_subtree(requested) {
        if let Some(file) = dir.get_file(sub) {
            return ok_bytes(file.contents().to_vec(), requested);
        }
    }
    (
        StatusCode::NOT_FOUND,
        format!("not found: {requested}"),
    )
        .into_response()
}

fn split_subtree(req: &str) -> Option<(&'static Dir<'static>, &str)> {
    if let Some(rest) = req.strip_prefix("web/") {
        Some((&WEB, rest))
    } else if let Some(rest) = req.strip_prefix("qemu/") {
        Some((&QEMU, rest))
    } else {
        None
    }
}

async fn try_disk(
    state: &AppState,
    root: &Path,
    requested: &str,
) -> DiskOutcome {
    // Path layout: <root>/web/... or <root>/assets/qemu/...
    let on_disk: PathBuf = if let Some(rest) = requested.strip_prefix("web/") {
        root.join("web").join(rest)
    } else if let Some(rest) = requested.strip_prefix("qemu/") {
        root.join("assets/qemu").join(rest)
    } else {
        return DiskOutcome::Miss;
    };

    // V12: canonicalize and confirm descendant. We distinguish ENOENT
    // ("genuine miss; fall through to embed") from other errors
    // (permission denied, EINVAL, race conditions; hard-reject so we
    // never silently fall through to an embedded copy).
    let canon = match tokio::fs::canonicalize(&on_disk).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return DiskOutcome::Miss;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %on_disk.display(),
                "asset canonicalize failed; hard-rejecting to avoid embedded fall-through"
            );
            return DiskOutcome::Reject(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "asset path could not be resolved",
                )
                    .into_response(),
            );
        }
    };

    // Prefer the precomputed canonical root from AppState (CR-02): avoids
    // a per-request recursive canonicalize and closes the race window
    // where the root's canonical form changes between requests.
    let root_canon = match state.assets_dir_canon.as_ref() {
        Some(rc) => rc.clone(),
        None => match tokio::fs::canonicalize(root).await {
            Ok(rc) => rc,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "assets_dir canonicalize failed; hard-rejecting"
                );
                return DiskOutcome::Reject(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "assets-dir could not be resolved",
                    )
                        .into_response(),
                );
            }
        },
    };

    if !canon.starts_with(&root_canon) {
        return DiskOutcome::Reject(
            (StatusCode::BAD_REQUEST, "path escapes --assets-dir").into_response(),
        );
    }
    match tokio::fs::read(&canon).await {
        Ok(bytes) => DiskOutcome::Hit(ok_bytes(bytes, requested)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DiskOutcome::Miss,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %canon.display(),
                "asset read failed after canonicalize"
            );
            DiskOutcome::Reject(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "asset read failed",
                )
                    .into_response(),
            )
        }
    }
}

fn ok_bytes(bytes: Vec<u8>, hint_path: &str) -> Response {
    let mime = mime_guess::from_path(hint_path).first_or_octet_stream();
    let ct = HeaderValue::from_str(mime.as_ref())
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    ([(header::CONTENT_TYPE, ct)], Body::from(bytes)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn state(assets_dir: Option<PathBuf>) -> Arc<AppState> {
        Arc::new(AppState::new_for_test(
            PathBuf::from("/tmp/fake-kernel"),
            assets_dir,
        ))
    }

    fn tempdir_for_test() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "bootroom-assets-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn rand_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string()
    }

    #[tokio::test]
    async fn test_serve_asset_embedded_wasm() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("qemu/qemu-system-riscv64.wasm".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/wasm"
        );
    }

    #[tokio::test]
    async fn test_serve_asset_embedded_vendor_xterm() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("web/vendor/xterm.js".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        // text/javascript or application/javascript both acceptable
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("javascript"), "got: {ct}");
    }

    #[tokio::test]
    async fn test_serve_asset_unknown_404() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("web/does-not-exist.js".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_asset_path_traversal_rejected() {
        let dir = tempdir_for_test();
        let resp = serve_asset(
            State(state(Some(dir.clone()))),
            AxumPath("web/../../../../etc/passwd".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_serve_asset_disk_override() {
        let dir = tempdir_for_test();
        std::fs::create_dir_all(dir.join("web")).unwrap();
        std::fs::File::create(dir.join("web/x.txt"))
            .unwrap()
            .write_all(b"disk content")
            .unwrap();
        let resp = serve_asset(
            State(state(Some(dir.clone()))),
            AxumPath("web/x.txt".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body =
            axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), b"disk content");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_serve_asset_disk_override_fallthrough() {
        let dir = tempdir_for_test();
        std::fs::create_dir_all(&dir).unwrap();
        // No /web/vendor/xterm.js on disk, but it IS embedded.
        let resp = serve_asset(
            State(state(Some(dir.clone()))),
            AxumPath("web/vendor/xterm.js".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// CR-02 regression: when there is NO disk override (embedded-only
    /// branch), a literal `..` segment must be rejected at the early
    /// guard rather than reaching `Dir::get_file`. Previously the
    /// security control was only documented as the disk-branch
    /// canonicalize check; this test asserts the unified guard.
    #[tokio::test]
    async fn test_serve_asset_embedded_only_rejects_traversal() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("web/../qemu/qemu-system-riscv64.wasm".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// CR-02 regression: NUL byte in a path segment is rejected even
    /// without a disk override.
    #[tokio::test]
    async fn test_serve_asset_embedded_only_rejects_nul() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("web/\0evil".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// CR-02 regression: backslash in a path segment is rejected.
    #[tokio::test]
    async fn test_serve_asset_embedded_only_rejects_backslash() {
        let resp = serve_asset(
            State(state(None)),
            AxumPath("web\\vendor\\xterm.js".into()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
