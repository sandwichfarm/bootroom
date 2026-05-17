//! bootroom binary entrypoint.

use bootroom::cli::{Cli, Cmd};
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve(_args) => {
            eprintln!("bootroom serve: server runtime lands in plan 01-04 Task 3");
        }
    }
}
