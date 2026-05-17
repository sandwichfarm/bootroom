//! Command-line argument parsing.
//!
//! Phase 1 ships a single `serve` subcommand. Phase 2 will add `run`;
//! Phase 3 adds `init`/`check`; Phase 5 adds `doctor`.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Web-based test harness for RISC-V kernels via qemu-wasm.
#[derive(Debug, Parser)]
#[command(name = "bootroom", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Start the local HTTP server and serve the qemu-wasm UI.
    Serve(ServeArgs),
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    /// Path to the kernel image to load into the guest.
    #[arg(long, value_name = "PATH")]
    pub kernel: PathBuf,

    /// Address to bind the HTTP listener to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port for the HTTP listener (0 = OS-assigned ephemeral, useful for tests).
    #[arg(long, default_value_t = 8765)]
    pub port: u16,

    /// Serve UI and qemu-wasm assets from this directory instead of the
    /// compiled-in copy. Layout: `<dir>/web/` and `<dir>/assets/qemu/`.
    ///
    /// Intended for bootroom development — end users should leave this unset.
    #[arg(long, value_name = "PATH")]
    pub assets_dir: Option<PathBuf>,
}
