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

async fn serve_one(state: &AppState, requested: &str) -> Response {
    // Reject obvious traversal before touching disk or embed.
    if requested.split('/').any(|seg| seg == "..") {
        return (StatusCode::BAD_REQUEST, "invalid path: .. not allowed")
            .into_response();
    }
    // Disk override first.
    if let Some(root) = &state.assets_dir {
        if let Some(resp) = try_disk(root, requested).await {
            return resp;
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

async fn try_disk(root: &Path, requested: &str) -> Option<Response> {
    // Path layout: <root>/web/... or <root>/assets/qemu/...
    let on_disk: PathBuf = if let Some(rest) = requested.strip_prefix("web/") {
        root.join("web").join(rest)
    } else if let Some(rest) = requested.strip_prefix("qemu/") {
        root.join("assets/qemu").join(rest)
    } else {
        return None;
    };

    // V12: canonicalize and confirm descendant.
    let canon = tokio::fs::canonicalize(&on_disk).await.ok()?;
    let root_canon = tokio::fs::canonicalize(root).await.ok()?;
    if !canon.starts_with(&root_canon) {
        return Some(
            (StatusCode::BAD_REQUEST, "path escapes --assets-dir").into_response(),
        );
    }
    let bytes = tokio::fs::read(&canon).await.ok()?;
    Some(ok_bytes(bytes, requested))
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
        Arc::new(AppState::new(
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
}
