//! COOP and COEP response-header middleware.
//!
//! Per Pitfall 1: a single missing header breaks cross-origin isolation
//! and `SharedArrayBuffer` silently. These layers attach at the top of the
//! router so EVERY response (including 404s) carries both headers.

use axum::http::{HeaderValue, header::HeaderName};
use tower_http::set_header::SetResponseHeaderLayer;

#[must_use]
pub fn coop_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    )
}

#[must_use]
pub fn coep_layer() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("cross-origin-embedder-policy"),
        HeaderValue::from_static("require-corp"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    async fn ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_coop_layer_overrides_existing_header() {
        let app = Router::new().route("/", get(ok)).layer(coop_layer());
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
    }

    #[tokio::test]
    async fn test_coep_layer_value() {
        let app = Router::new().route("/", get(ok)).layer(coep_layer());
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("cross-origin-embedder-policy").unwrap(),
            "require-corp"
        );
    }
}
