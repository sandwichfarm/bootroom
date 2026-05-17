//! HTTP server composition: routes + state + middleware + bind.

use crate::{
    cli::ServeArgs,
    headers::{coep_layer, coop_layer},
    state::AppState,
};
use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

async fn index_stub(State(_s): State<Arc<AppState>>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "plan 01-05 wires this: GET /")
}
async fn kernel_info_stub(State(_s): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        "plan 01-05 wires this: GET /api/kernel/info",
    )
}
async fn kernel_stream_stub(State(_s): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        "plan 01-05 wires this: GET /kernel",
    )
}
async fn asset_stub(
    State(_s): State<Arc<AppState>>,
    Path(_p): Path<String>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        "plan 01-05 wires this: GET /assets/{*path}",
    )
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_stub))
        .route("/api/kernel/info", get(kernel_info_stub))
        .route("/kernel", get(kernel_stream_stub))
        .route("/assets/{*path}", get(asset_stub))
        .with_state(state)
        .layer(coop_layer())
        .layer(coep_layer())
        .layer(TraceLayer::new_for_http())
}

/// Run the bootroom HTTP server.
///
/// # Errors
///
/// Returns an error if `--kernel` does not exist, the host/port fail to
/// parse, the listener cannot bind, or the axum runtime exits abnormally.
pub async fn run(args: ServeArgs) -> Result<()> {
    // V5: validate --kernel at startup (per 01-RESEARCH.md open question 3).
    if !args.kernel.exists() {
        anyhow::bail!("--kernel: file not found at {}", args.kernel.display());
    }
    if !args.kernel.is_file() {
        anyhow::bail!(
            "--kernel: path is not a regular file: {}",
            args.kernel.display()
        );
    }

    let state = Arc::new(AppState::new(args.kernel.clone(), args.assets_dir.clone()));
    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid --host/--port: {}:{}", args.host, args.port))?;

    // V4: warn if binding non-loopback.
    if !is_loopback(&addr.ip()) {
        tracing::warn!(
            "Binding to non-loopback address {}; bootroom exposes kernel-control surface to any \
             reachable client. Use --host 127.0.0.1 unless you know what you're doing.",
            addr
        );
    }

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    let bound = listener.local_addr()?;

    // CONTEXT.md D-04: exact startup line.
    println!("Serving bootroom on http://{bound} (Ctrl-C to stop)");

    axum::serve(listener, app).await.context("axum::serve exited")?;
    Ok(())
}

fn is_loopback(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::path::PathBuf;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(PathBuf::from("/tmp/fake-kernel"), None))
    }

    #[tokio::test]
    async fn test_router_returns_coop_coep_on_404() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
        assert_eq!(
            resp.headers().get("cross-origin-embedder-policy").unwrap(),
            "require-corp"
        );
    }

    #[tokio::test]
    async fn test_router_returns_coop_coep_on_stub_route() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 501 from stub, but the layer must still attach
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            resp.headers().get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
        assert_eq!(
            resp.headers().get("cross-origin-embedder-policy").unwrap(),
            "require-corp"
        );
    }
}
