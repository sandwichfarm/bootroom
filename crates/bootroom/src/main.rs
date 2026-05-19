//! bootroom binary entrypoint.

use bootroom::cli::{Cli, Cmd};
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // tracing init: read RUST_LOG, default to "bootroom=info,tower_http=info"
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bootroom=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve(args) => bootroom::server::run(args).await,
        Cmd::Check(args) => {
            // Plan 04 wires real handlers; placeholder until then.
            // Exit code 2 (file-not-found-class) so any accidental CI use
            // during the Plan 03 -> Plan 04 window fails loudly.
            let _ = args;
            std::process::exit(2);
        }
        Cmd::Init(args) => {
            // Plan 04 wires real handlers; placeholder until then.
            let _ = args;
            std::process::exit(1);
        }
    }
}
