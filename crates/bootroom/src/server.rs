//! HTTP server composition: routes + state + middleware + bind.

use crate::{
    cli::ServeArgs,
    headers::{coep_layer, coop_layer},
    state::AppState,
};
use anyhow::{Context, Result};
use axum::{Router, routing::get};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(crate::assets::serve_index))
        .route("/api/kernel/info", get(crate::kernel_info::kernel_info))
        .route("/kernel", get(crate::kernel_stream::kernel_stream))
        .route("/assets/{*path}", get(crate::assets::serve_asset))
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

    // Parse host as IpAddr first so IPv6 literals like `::1` work — naive
    // `"{host}:{port}"` concatenation produces ambiguous strings such as
    // `::1:8765` which SocketAddr cannot reliably round-trip (CR-01).
    let ip: IpAddr = args
        .host
        .parse()
        .with_context(|| format!("invalid --host: {}", args.host))?;
    let addr = SocketAddr::new(ip, args.port);

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
    use axum::{body::Body, http::Request, http::StatusCode};
    use std::path::PathBuf;
    use tower::ServiceExt;

    /// CR-01 regression: `--host ::1` must parse to a valid IPv6 loopback
    /// SocketAddr rather than mis-parsing the legacy `"{host}:{port}"`
    /// concatenation. We construct the SocketAddr the same way `run` does
    /// without actually binding (no port collisions on CI).
    #[test]
    fn test_ipv6_loopback_host_parses() {
        let host = "::1";
        let port: u16 = 8765;
        let ip: IpAddr = host.parse().expect("::1 should parse as IpAddr");
        let addr = SocketAddr::new(ip, port);
        assert!(addr.is_ipv6());
        assert!(is_loopback(&addr.ip()));
        assert_eq!(addr.port(), port);
        // The bracketed Display form is the canonical IPv6 socket-address
        // rendering and is what the startup banner emits.
        assert_eq!(addr.to_string(), "[::1]:8765");
    }

    #[test]
    fn test_ipv4_host_parses() {
        let ip: IpAddr = "127.0.0.1".parse().expect("v4 loopback parses");
        let addr = SocketAddr::new(ip, 0);
        assert!(addr.is_ipv4());
        assert!(is_loopback(&addr.ip()));
    }

    #[test]
    fn test_invalid_host_rejected() {
        let res: std::result::Result<IpAddr, _> = "not-an-ip".parse();
        assert!(res.is_err(), "non-IP host strings must fail to parse");
    }

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
    async fn test_full_router_serves_embedded_wasm_with_coop() {
        let app = build_router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/assets/qemu/qemu-system-riscv64.wasm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/wasm"
        );
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
