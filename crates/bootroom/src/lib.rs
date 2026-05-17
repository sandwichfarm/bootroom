//! bootroom library API — exposed primarily for integration tests in `tests/`.
//!
//! The binary entrypoint lives in `main.rs`; everything reusable lives here.

pub mod cli;
pub mod embed;
pub mod headers;
pub mod server;
pub mod state;

// Re-export the surface tests will use.
pub use cli::ServeArgs;
pub use server::build_router;
pub use state::AppState;
