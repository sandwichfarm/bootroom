//! bootroom binary entrypoint.

use bootroom::cli::{Cli, Cmd};
use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    // tracing init: read RUST_LOG, default to "bootroom=info,tower_http=info"
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bootroom=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve(args) => {
            bootroom::server::run(args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Check(args) => Ok(bootroom::check_cmd::run(args)),
        Cmd::Init(args) => {
            // Plan 04 Task 2 wires the real init handler; placeholder
            // until then so the bin still builds during Task 1 work.
            let _ = args;
            Ok(ExitCode::from(1))
        }
    }
}
