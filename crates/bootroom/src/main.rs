//! bootroom binary entrypoint.
//!
//! Phase 1, Plan 01-01: stub. Plan 01-04 replaces this with the real
//! clap dispatch + axum server.

use clap::Parser;

/// Web-based test harness for RISC-V kernels via qemu-wasm.
#[derive(Parser)]
#[command(name = "bootroom", version, about, long_about = None)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    eprintln!(
        "bootroom {} - Phase 1 scaffolding; subcommands land in plan 01-04",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}
