//! Command-line argument parsing.
//!
//! Phase 1 shipped a single `serve` subcommand. Phase 3 (this revision) adds
//! `check` and `init` so callers can match on `Cmd::{Serve, Check, Init}`
//! exhaustively. The `check`/`init` handlers themselves are stubs in
//! `main.rs` until Plan 04 wires the real bodies.
//!
//! Phase 3 also extends `ServeArgs` with `--config <PATH>` (TOML config
//! location override) and `--action <LABEL=BYTES>` (repeatable ad-hoc
//! action definitions). `--action` decodes via the shared
//! `bootroom_core::decode_bytes_escape` helper so the CLI grammar and the
//! TOML grammar can never drift.

use bootroom_core::config::CliAction;
use bootroom_core::decode_bytes_escape;
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
    ///
    /// MUST be the first variant — preserves help-text ordering and the
    /// Phase-2 subprocess test invocation shape (Pitfall #9 mitigation).
    Serve(ServeArgs),

    /// Parse and validate bootroom.toml without starting the server.
    Check(CheckArgs),

    /// Write a starter bootroom.toml to the current directory.
    Init(InitArgs),
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

    /// Do not auto-open the default browser on start.
    ///
    /// By default `bootroom serve` opens the harness URL in the user's
    /// default browser via `open::that_detached` once the listener is bound.
    /// Pass `--no-open` for headless / CI usage or when running under a
    /// supervisor that opens the browser itself.
    #[arg(long)]
    pub no_open: bool,

    /// Path to bootroom.toml; default = ./bootroom.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Define an ad-hoc action without editing config. Format: 'label=BYTES'
    /// with C-style escapes (\r \n \t \0 \\ \xNN). Repeatable. Overrides
    /// config-file actions on label collision; last --action wins among
    /// repeated CLI values.
    #[arg(
        long = "action",
        value_name = "LABEL=BYTES",
        action = clap::ArgAction::Append,
        value_parser = parse_cli_action,
    )]
    pub actions: Vec<CliAction>,
}

#[derive(Debug, Args, Clone)]
pub struct CheckArgs {
    /// Path to bootroom.toml; default = ./bootroom.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
pub struct InitArgs {
    /// Overwrite an existing bootroom.toml.
    #[arg(long)]
    pub force: bool,
}

/// Clap value-parser for `--action LABEL=BYTES`.
///
/// Splits on the FIRST `=` so operators may embed `=` in the byte payload
/// (e.g. `--action 'env=KEY=VALUE\r'`). Rejects an empty label. Decodes the
/// rhs via the shared `decode_bytes_escape` helper so the CLI escape
/// grammar is byte-for-byte identical to the TOML one — no second decoder
/// to drift.
///
/// Returns `Result<_, String>` (never panics) so clap renders the error as
/// the usage error and exits 2 (clap's standard exit code for argv errors).
fn parse_cli_action(s: &str) -> Result<CliAction, String> {
    let eq_idx = s.find('=').ok_or_else(|| {
        // WR-06: truncate the input before Debug-printing so a 10 KiB
        // `--action junk` does not dump the whole payload into the error.
        format!(
            "--action {:?}: expected 'label=BYTES'",
            truncate_for_error(s, 60)
        )
    })?;
    let (label, rest) = s.split_at(eq_idx);
    // `rest` starts with the `=` — strip it.
    let raw_bytes = &rest[1..];
    if label.is_empty() {
        // WR-06: when `s` starts with `=`, displaying `s` adds no
        // information beyond what the operator typed; emit only the
        // actionable diagnostic.
        return Err("--action: empty label (expected 'label=BYTES')".to_string());
    }
    let bytes = decode_bytes_escape(raw_bytes)
        .map_err(|e| format!("--action {label}: {e}"))?;
    Ok(CliAction {
        label: label.to_owned(),
        bytes,
    })
}

/// WR-06: truncate an arbitrary user input to at most `max` UTF-8 bytes
/// for safe inclusion in an error message. Snaps backward to a char
/// boundary so the returned prefix is always valid UTF-8.
fn truncate_for_error(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_action_simple() {
        let got = parse_cli_action("reboot=reboot\\r").expect("parses");
        assert_eq!(got.label, "reboot");
        assert_eq!(got.bytes, vec![b'r', b'e', b'b', b'o', b'o', b't', 0x0d]);
    }

    #[test]
    fn parse_cli_action_hex() {
        let got = parse_cli_action("ctrlc=\\x03").expect("parses");
        assert_eq!(got.label, "ctrlc");
        assert_eq!(got.bytes, vec![0x03]);
    }

    #[test]
    fn parse_cli_action_empty_label_rejected() {
        let err = parse_cli_action("=foo").expect_err("empty label rejected");
        assert!(
            err.contains("empty label"),
            "expected 'empty label' in error, got: {err}"
        );
    }

    #[test]
    fn parse_cli_action_no_equals_rejected() {
        let err = parse_cli_action("reboot").expect_err("no '=' rejected");
        assert!(
            err.contains("expected 'label=BYTES'"),
            "expected helpful format hint, got: {err}"
        );
    }

    #[test]
    fn parse_cli_action_invalid_escape_propagates() {
        let err = parse_cli_action("x=\\q").expect_err("invalid escape rejected");
        // EscapeError::Display says "unknown escape"; we also prefix with
        // the label so the operator knows which --action failed.
        assert!(
            err.contains("unknown escape") || err.contains('x'),
            "expected unknown-escape error or label 'x' in error, got: {err}"
        );
    }

    #[test]
    fn cli_parses_serve_with_repeated_actions() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "serve",
            "--kernel",
            "/tmp/x",
            "--no-open",
            "--action",
            "reboot=reboot\\r",
            "--action",
            "ctrlc=\\x03",
        ])
        .expect("parses");
        assert!(matches!(cli.cmd, Cmd::Serve(_)));
        let Cmd::Serve(args) = cli.cmd else {
            unreachable!("matched above")
        };
        assert_eq!(args.actions.len(), 2);
        assert_eq!(args.actions[0].label, "reboot");
        assert_eq!(
            args.actions[0].bytes,
            vec![b'r', b'e', b'b', b'o', b'o', b't', 0x0d]
        );
        assert_eq!(args.actions[1].label, "ctrlc");
        assert_eq!(args.actions[1].bytes, vec![0x03]);
    }

    #[test]
    fn cli_parses_check_with_config() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "check",
            "--config",
            "/tmp/bootroom.toml",
        ])
        .expect("parses");
        let Cmd::Check(args) = cli.cmd else {
            panic!("expected Cmd::Check, got {:?}", cli.cmd);
        };
        assert_eq!(args.config.as_deref(), Some(std::path::Path::new("/tmp/bootroom.toml")));
    }

    #[test]
    fn cli_parses_init_force() {
        let cli = Cli::try_parse_from(["bootroom", "init", "--force"]).expect("parses");
        let Cmd::Init(args) = cli.cmd else {
            panic!("expected Cmd::Init, got {:?}", cli.cmd);
        };
        assert!(args.force);
    }

    #[test]
    fn cli_parses_init_default() {
        let cli = Cli::try_parse_from(["bootroom", "init"]).expect("parses");
        let Cmd::Init(args) = cli.cmd else {
            panic!("expected Cmd::Init, got {:?}", cli.cmd);
        };
        assert!(!args.force);
    }

    #[test]
    fn cli_serve_args_phase2_compat() {
        // Pitfall #9 regression pin: the five Phase-2 ServeArgs fields keep
        // their flag names, types, and defaults; the new --config /
        // --action flags default to None / empty so existing Phase-2
        // invocations parse unchanged.
        //
        // Plan 04-03: `--kernel` / `--config` migrated into the flattened
        // `CommonArgs`. The PARSING INPUTS are intentionally unchanged
        // (Pitfall #9) — only field-access paths shift to `args.common.*`.
        let cli = Cli::try_parse_from([
            "bootroom",
            "serve",
            "--kernel",
            "/tmp/x",
            "--host",
            "::1",
            "--port",
            "9999",
            "--no-open",
        ])
        .expect("parses");
        let Cmd::Serve(args) = cli.cmd else {
            panic!("expected Cmd::Serve, got {:?}", cli.cmd);
        };
        assert_eq!(args.common.kernel, std::path::PathBuf::from("/tmp/x"));
        assert_eq!(args.host, "::1");
        assert_eq!(args.port, 9999);
        assert!(args.no_open);
        assert!(args.assets_dir.is_none());
        assert!(args.common.config.is_none());
        assert!(args.actions.is_empty());
    }

    // ----- Plan 04-03 tests: CommonArgs flatten + Cmd::Run(RunArgs) -----

    #[test]
    fn cli_parses_run_minimal() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "run",
            "--kernel",
            "/tmp/Image",
            "--scenario",
            "boot_smoke",
        ])
        .expect("parses");
        let Cmd::Run(args) = cli.cmd else {
            panic!("expected Cmd::Run, got {:?}", cli.cmd);
        };
        assert_eq!(args.common.kernel, std::path::PathBuf::from("/tmp/Image"));
        assert_eq!(args.scenario, "boot_smoke");
        assert!(args.common.config.is_none());
        assert!(!args.common.verbose);
        assert!(args.log_file.is_none());
    }

    #[test]
    fn cli_parses_run_all_flags() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "run",
            "--kernel",
            "/tmp/Image",
            "--scenario",
            "boot_smoke",
            "--config",
            "/tmp/b.toml",
            "--verbose",
            "--log-file",
            "/tmp/log.jsonl",
        ])
        .expect("parses");
        let Cmd::Run(args) = cli.cmd else {
            panic!("expected Cmd::Run, got {:?}", cli.cmd);
        };
        assert_eq!(args.common.kernel, std::path::PathBuf::from("/tmp/Image"));
        assert_eq!(args.scenario, "boot_smoke");
        assert_eq!(
            args.common.config.as_deref(),
            Some(std::path::Path::new("/tmp/b.toml"))
        );
        assert!(args.common.verbose);
        assert_eq!(
            args.log_file.as_deref(),
            Some(std::path::Path::new("/tmp/log.jsonl"))
        );
    }

    #[test]
    fn cli_parses_run_short_v() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "run",
            "--kernel",
            "/tmp/x",
            "--scenario",
            "boot_smoke",
            "-v",
        ])
        .expect("parses");
        let Cmd::Run(args) = cli.cmd else {
            panic!("expected Cmd::Run, got {:?}", cli.cmd);
        };
        assert!(args.common.verbose);
    }

    #[test]
    fn cli_run_requires_scenario() {
        let err = Cli::try_parse_from(["bootroom", "run", "--kernel", "/tmp/x"])
            .expect_err("missing --scenario must fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("scenario"),
            "error must mention `scenario`; got: {msg}"
        );
    }

    #[test]
    fn cli_run_requires_kernel() {
        let err = Cli::try_parse_from([
            "bootroom",
            "run",
            "--scenario",
            "boot_smoke",
        ])
        .expect_err("missing --kernel must fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("kernel"),
            "error must mention `kernel`; got: {msg}"
        );
    }

    #[test]
    fn cli_serve_args_phase3_compat_via_flatten() {
        // Phase 3 compat pin — exercises the flatten on `ServeArgs`. Field
        // renames inside `CommonArgs` are caught here.
        let cli = Cli::try_parse_from([
            "bootroom",
            "serve",
            "--kernel",
            "/tmp/x",
            "--host",
            "::1",
            "--port",
            "9999",
            "--no-open",
            "--config",
            "/tmp/b.toml",
        ])
        .expect("parses");
        let Cmd::Serve(args) = cli.cmd else {
            panic!("expected Cmd::Serve, got {:?}", cli.cmd);
        };
        assert_eq!(args.common.kernel, std::path::PathBuf::from("/tmp/x"));
        assert_eq!(args.host, "::1");
        assert_eq!(args.port, 9999);
        assert!(args.no_open);
        assert_eq!(
            args.common.config.as_deref(),
            Some(std::path::Path::new("/tmp/b.toml"))
        );
        assert!(!args.common.verbose);
        assert!(args.assets_dir.is_none());
        assert!(args.actions.is_empty());
    }

    #[test]
    fn cli_serve_short_v_sets_verbose() {
        let cli = Cli::try_parse_from([
            "bootroom",
            "serve",
            "--kernel",
            "/tmp/x",
            "--no-open",
            "-v",
        ])
        .expect("parses");
        let Cmd::Serve(args) = cli.cmd else {
            panic!("expected Cmd::Serve, got {:?}", cli.cmd);
        };
        assert!(args.common.verbose);
    }

    #[test]
    fn cli_help_lists_shared_flags_on_run() {
        use clap::CommandFactory;
        let help = Cli::command().render_long_help().to_string();
        for needle in ["--kernel", "--config", "--verbose", "--scenario", "--log-file"] {
            assert!(
                help.contains(needle),
                "Cli long help must mention `{needle}`; got:\n{help}"
            );
        }
    }

    #[test]
    fn cli_parses_run_with_repeated_actions_unsupported() {
        // Sanity pin: `--action` lives on ServeArgs, NOT CommonArgs. A
        // `--action` passed to `run` must error (unknown argument).
        let err = Cli::try_parse_from([
            "bootroom",
            "run",
            "--kernel",
            "/tmp/x",
            "--scenario",
            "boot_smoke",
            "--action",
            "reboot=reboot\\r",
        ])
        .expect_err("`--action` is serve-only");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("action") || msg.contains("unexpected") || msg.contains("unrecognized"),
            "error must surface the unknown --action arg; got: {msg}"
        );
    }
}
