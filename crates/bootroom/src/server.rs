//! HTTP server composition — populated by plan 01-04 Task 3.

use crate::state::AppState;
use std::sync::Arc;

/// Placeholder until Task 3 lands the real router.
pub fn build_router(_state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
}
