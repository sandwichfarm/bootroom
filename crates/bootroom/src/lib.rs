//! bootroom library API — exposed primarily for integration tests in `tests/`.
//!
//! The binary entrypoint lives in `main.rs`; everything reusable lives here.

pub mod assets;
pub mod cli;
pub mod embed;
pub mod headers;
pub mod kernel_info;
pub mod kernel_stream;
pub mod server;
pub mod state;

// Re-export the surface tests will use.
pub use cli::ServeArgs;
pub use server::build_router;
pub use state::AppState;
