//! The shim binary, `heuristic-jump`. This crate is the wiring: argument
//! parsing, log setup, and the one place the language list is enumerated, so
//! that `driver` stays a library with no opinion about how it was invoked
//! (`design/core.md` §9, `design/deps.md` §11).
//!
//! There is no run loop yet — `shim.md` §13's `driver::run` arrives with the
//! transport — so what this resolves, it logs and exits. The two things it
//! resolves are already load-bearing: `core.md` §5's cap, and the mode, which
//! is what decides which of the cap's two defaults applies.

use std::ffi::OsString;

use clap::{CommandFactory, Parser, error::ErrorKind};
use driver::{Config, DeadlineMs, DeadlineOverride, Heuristics, Mode};
use tracing_subscriber::EnvFilter;

/// `deps.md` §11. The usage form is what makes a parser viable here at all:
///
/// ```text
/// heuristic-jump [OPTIONS] -- <SERVER> [SERVER ARGS...]    # proxy
/// heuristic-jump [OPTIONS]                                 # standalone
/// ```
///
/// The `--` is mandatory and there is no `--standalone` flag: the mode is
/// whether a server was given, so there is no second source of truth and no
/// conflict rule. `--` is POSIX's own "stop parsing" marker, which is what
/// gets the child's argv — `--help` and `--version` included — through
/// byte-for-byte.
#[derive(Parser, Debug)]
#[command(name = "heuristic-jump", version)]
struct Cli {
    /// Never answer heuristically; pure proxy. Meaningless without a server.
    #[arg(long, requires = "server")]
    proxy_only: bool,

    /// Overrides the hard cap. Defaults to 750 proxying, 2000 standalone.
    #[arg(long, value_name = "MS")]
    deadline_ms: Option<u64>,

    /// `tracing-subscriber` env-filter string. Defaults to `warn`, so we are
    /// quiet unless asked: our lines interleave with the proper server's in
    /// the editor's log panel (`deps.md` §9).
    #[arg(long, value_name = "FILTER")]
    log: Option<String>,

    /// The proper language server's command line, after `--`. Omitted
    /// entirely means standalone mode.
    #[arg(
        last = true,
        allow_hyphen_values = true,
        num_args = 1..,
        value_name = "SERVER"
    )]
    server: Vec<OsString>,
}

fn main() {
    let cli = Cli::parse();

    // The one check clap will not do, and the reason it matters is `core.md`
    // §5: `heuristic-jump -- $SERVER` with `$SERVER` unset parses as
    // `server = []`, which is standalone — silently swapping the oracle, and
    // the 750ms cap for 2000ms. A bare `--` is positive evidence that a server
    // was meant, where a bare `heuristic-jump` is a legitimate standalone
    // invocation and carries no such evidence (`deps.md` §11).
    if cli.server.is_empty() && std::env::args_os().any(|argument| argument == "--") {
        Cli::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "`--` given with no server command",
            )
            .exit();
    }

    let filter = match EnvFilter::try_new(cli.log.as_deref().unwrap_or("warn")) {
        Ok(filter) => filter,
        Err(error) => Cli::command()
            .error(ErrorKind::InvalidValue, format!("--log: {error}"))
            .exit(),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // The editor's log panel is not a terminal, and the child's stderr is
        // forwarded into the same stream (`shim.md` §2).
        .with_ansi(false)
        .with_writer(stderr_for_logging)
        .init();

    let config = Config::new(
        Mode::from_server_argv(
            cli.server,
            if cli.proxy_only {
                Heuristics::Disabled
            } else {
                Heuristics::Enabled
            },
        ),
        match cli.deadline_ms {
            Some(milliseconds) => DeadlineOverride::Explicit(DeadlineMs::new(milliseconds)),
            None => DeadlineOverride::ModeDefault,
        },
    );

    tracing::info!(
        mode = config.mode().name(),
        deadline_ms = config.deadline().get(),
        "resolved configuration"
    );
    tracing::warn!(
        "no run loop yet: this build resolves its configuration and exits, and proxies nothing"
    );
}

/// The single sanctioned `stderr` handle. `clippy.toml` bans the rest because
/// an ad-hoc write interleaves with the child's forwarded stderr; its
/// replacement is `tracing`, and this is where `tracing` comes out.
#[expect(
    clippy::disallowed_methods,
    reason = "the log subscriber is what clippy.toml's stderr replacement points at"
)]
fn stderr_for_logging() -> std::io::Stderr {
    std::io::stderr()
}
