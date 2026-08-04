//! The shim binary, `heuristic-jump`. This crate is the wiring: argument
//! parsing, log setup, and the one place the language list is enumerated, so
//! that `driver` stays a library with no opinion about how it was invoked
//! (`design/core.md` §9, `design/deps.md` §11).
//!
//! Three things resolved and handed over, and no fourth: the handler set, the
//! mode, and `core.md` §5's cap. `driver::run` has no transport behind it yet,
//! so the process still exits immediately — but the registry it is given is the
//! shipped binary's, which is what makes `lang_rust` a linked artifact rather
//! than a workspace member nothing builds.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{CommandFactory, Parser, error::ErrorKind};
use driver::{
    Config, DEFAULT_LOG_FILTER, DeadlineMs, DeadlineOverride, Heuristics, Mode, PrefixedWriter,
    Registry, Tracing,
};
use shared::LanguageHandler;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

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

    /// Where `core.md` §7's per-query JSONL records are written. Absent means
    /// none are, which is the shipped default: the records are what a corpus
    /// run and a field measurement are made of, and neither is a thing an
    /// editor session should pay for unasked.
    #[arg(long, value_name = "PATH")]
    trace: Option<PathBuf>,

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

fn main() -> Result<(), shared::Error> {
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

    let filter = match EnvFilter::try_new(cli.log.as_deref().unwrap_or(DEFAULT_LOG_FILTER)) {
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
        .with_writer(PrefixedStderr)
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
        // The `Option` clap produces becomes an enum here rather than being
        // carried inward, for the reason `DeadlineOverride` is one: absent is
        // a decision — "write no records" — and `Config::new(mode, deadline,
        // None)` would be a call site that reads as an omission.
        match cli.trace {
            Some(path) => Tracing::To(path),
            None => Tracing::Off,
        },
    );

    // `core.md` §9's `main`, and the one line adding a language costs. Every
    // other crate takes its handler as a `&dyn LanguageHandler` precisely so
    // that this vector is the only enumeration of the list in the workspace;
    // `crates/driver/tests/seam.rs` fails if a `lang_*` member is missing from
    // it, which is the half the compiler cannot catch — a language nothing
    // names simply never fails to build.
    let registry = Registry::new(vec![
        Arc::new(lang_rust::Handler::new()) as Arc<dyn LanguageHandler>
    ]);

    driver::run(registry, config)
}

/// `deps.md` §9's destination, wrapped in §9's prefix. The wrapper is
/// `driver`'s so that it can be asserted on; installing the subscriber is this
/// crate's, which is where §9 puts it and where `--log` is parsed.
struct PrefixedStderr;

impl MakeWriter<'_> for PrefixedStderr {
    type Writer = PrefixedWriter<std::io::Stderr>;

    fn make_writer(&self) -> Self::Writer {
        PrefixedWriter::new(stderr_for_logging())
    }
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
