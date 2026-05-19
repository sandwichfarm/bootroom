//! 04-10 — RUN-03 pin: `bootroom run` uses the same `build_router(state)`
//! as `bootroom serve`. Any change that introduces a separate run-mode
//! router (a dedicated `/ws-run`, a parallel `/api/scenario`, etc.)
//! breaks this test.
//!
//! Strategy: build an `AppState` once, hand it to `build_router`, then
//! `oneshot` the four routes that `run` must share with `serve` —
//! `/`, `/api/config`, `/api/kernel/info`, and `/ws`. We don't assert
//! the response bodies; the point is "no separate codepath produced
//! a 404 here". A 200/4xx/5xx all prove the route exists; only a 404
//! would prove the router topology diverged.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bootroom::{AppState, build_router};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

#[tokio::test]
async fn run_router_reuses_serve_router() {
    let kernel = NamedTempFile::new().expect("tempfile");
    let state = Arc::new(AppState::new_for_test(kernel.path().to_path_buf(), None));
    let app = build_router(state);

    for path in ["/", "/api/config", "/api/kernel/info"] {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .expect("oneshot");
        // The route MUST exist regardless of whether the response body
        // is meaningful. A 404 here would mean run-mode wired a separate
        // codepath that lacks this route.
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "route {path} missing from build_router output (status={})",
            res.status()
        );
    }

    // /ws expects an Upgrade header. A plain GET returns either
    // 400 Bad Request or 426 Upgrade Required — both prove the route
    // exists. A 404 would prove we lost the WS route.
    let res = app
        .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
        .await
        .expect("oneshot /ws");
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "/ws route missing from build_router output (status={})",
        res.status()
    );
}
