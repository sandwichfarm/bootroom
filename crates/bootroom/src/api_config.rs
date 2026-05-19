//! `/api/config` — JSON projection of `LoadedConfig` for the browser.
//!
//! The same shape is carried in `WsMessage::ConfigUpdate` (Plan 03-08).
//! Both go through [`crate::watcher::project_loaded_to_json`] to guarantee
//! structural identity between the initial-load HTTP response and the
//! live-reload WS frame. Two consumers, one source — drift impossible by
//! construction (threat T-03-07-03 mitigation).
//!
//! Handler responsibilities:
//! - Acquire a read-lock on `AppState.loaded_config`.
//! - Call the canonical projection helper.
//! - Drop the read-lock and return `Json(Value)`.
//!
//! The read-lock is held only for the duration of the projection (which
//! is cheap — it allocates a new `serde_json::Value`).

use crate::state::AppState;
use crate::watcher::project_loaded_to_json;
use axum::{Json, extract::State};
use serde_json::Value;
use std::sync::Arc;

/// `GET /api/config` handler. Returns HTTP 200 with the JSON projection
/// of the currently-loaded config. COOP/COEP headers are applied by the
/// router-level middleware stack.
pub async fn api_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    let loaded = state.loaded_config.read().await;
    Json(project_loaded_to_json(&loaded))
}

#[cfg(test)]
mod tests {
    use super::api_config;

    /// Trivial signature test: the handler must satisfy axum's
    /// `Handler<State<Arc<AppState>>>` trait. Taking a function pointer
    /// to it is enough to prove the signature compiles. The real shape
    /// verification lives in `tests/api_config_endpoint.rs`.
    #[test]
    fn api_config_handler_signature_compiles() {
        let _ = api_config;
    }
}
