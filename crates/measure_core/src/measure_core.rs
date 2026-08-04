//! The corpus scan (`design/core.md` §7). **Not a mode of the shim** — its own
//! crate, plus a four-line `measure_<lang>` binary per language.
//!
//! The requirements are opposed at nearly every point: the proper LSP is
//! waited for here and raced there; this optimises throughput and the shim
//! optimises latency; there are no deadlines here and hard ones there; this is
//! a plain LSP client and that is a proxy. Building one program that did both
//! would mean a transport abstraction and a policy-override switch that exist
//! for a single caller, threaded through the part of the system with the
//! strictest correctness requirements.
//!
//! The reason to hesitate is that a separate harness could drift into
//! measuring a reimplementation. That turns out to be weaker than it looks:
//! **what `measure` measures is the handler, not the driver.** The proxy and
//! the health model are not under test — resolution accuracy is. So as long as
//! the `Query` and the `DocumentSnapshot` are built the same way, the code
//! under test is genuinely identical, and both constructors live in `shared`
//! precisely so that is structural rather than a matter of discipline.
//!
//! It depends on `shared` and nothing else of ours: not on `driver`, not on
//! any language. It takes its handler as `&dyn LanguageHandler`, so it is
//! compiled once and is generic over the language without monomorphising over
//! it.

mod cli;
mod client;
mod collect;
mod corpus;
mod positions;
mod record;
mod replay;
mod table;
mod truth;

use std::io::Write;
use std::sync::Arc;

use shared::{Error, FileList, LanguageHandler, ProjectPath, SystemClock};

pub use cli::{Cli, Command, Format, Replay};
pub use client::{MAX_FRAME_BYTES, MAX_HEADER_BYTES, read_frame};
pub use corpus::{ServerEntry, grammar_pin, locked_grammar, resolve_server};
pub use record::{Decision, Mode, QueryRecord, StratumName};
pub use truth::{Provenance, check_resumable};

/// The whole of a `measure_<lang>` binary, after `Cli::parse()`.
///
/// ```ignore
/// fn main() -> Result<(), shared::Error> {
///     measure_core::run(&lang_rust::Handler::new(), Cli::parse())
/// }
/// ```
pub fn run(handler: &dyn LanguageHandler, cli: Cli) -> Result<(), Error> {
    install_logging();
    let clock = SystemClock;
    let language = first_language_id(handler)?;

    match cli.command {
        Command::Enumerate(arguments) => {
            let corpus = corpus::Corpus::open(&arguments.corpus.corpus, language)?;
            let repositories = corpus.repositories(&arguments.corpus.repositories)?;
            let quota = positions::non_identifier_quota(repositories.len());

            for repository in &repositories {
                corpus::verify_checkout(repository, None)?;
                let files = source_files(handler, repository)?;
                let found = positions::enumerate(handler, &files, quota)?;
                let sampled = positions::sample(found, arguments.limit, arguments.seed);
                positions::write(&corpus.positions(&repository.name), &sampled)?;
                tracing::info!(
                    repository = %repository.name,
                    positions = sampled.len(),
                    "enumerated"
                );
            }
            Ok(())
        }

        Command::Collect(arguments) => {
            let corpus = corpus::Corpus::open(&arguments.corpus.corpus, language)?;
            let server = resolve_server(language, &arguments.server)?;
            let collection = collect::Collection {
                corpus: &corpus,
                server: &server,
                clock: &clock,
                restart: arguments.restart,
            };
            // (repository, server) is the unit of work, which is what makes a
            // hundred machine-hours resumable at all.
            for repository in &corpus.repositories(&arguments.corpus.repositories)? {
                collection.run(repository)?;
            }
            Ok(())
        }

        Command::Replay(arguments) => report(&replay_table(handler, &clock, &arguments)?),
    }
}

/// A whole `replay` run, returning the rendered table rather than printing it.
///
/// It is public because the table's one non-negotiable property is that it is
/// **byte-identical across two runs of the same corpus at the same commit**
/// (`core.md` §7's command line), and a property nothing can hold is a property
/// nothing can assert: `run` hands the text straight to a `stdout` handle that
/// `cargo test` does not capture. `tests/pipeline.rs` compares two returned
/// strings instead, in both formats.
///
/// Writing `--records` stays inside here rather than moving to the caller,
/// because the records are what the run produced and not what it printed —
/// with no `--records` this writes nothing, which is what keeps the default a
/// pure function of its inputs.
pub fn replay_table(
    handler: &dyn LanguageHandler,
    clock: &dyn shared::Clock,
    arguments: &Replay,
) -> Result<String, Error> {
    let language = first_language_id(handler)?;
    let corpus = corpus::Corpus::open(&arguments.corpus.corpus, language)?;
    let replay = replay::Replay {
        handler,
        corpus: &corpus,
        server: &arguments.server,
        clock,
    };

    let started = clock.now();
    let mut table = table::Table::new();
    let mut records = Vec::new();
    for repository in &corpus.repositories(&arguments.corpus.repositories)? {
        replay.run(repository, &mut table, &mut records)?;
    }

    // `loops.md` §9: "`measure replay` reports its own wall clock", recorded
    // as an ordinary metric from the very first run because no replay-time
    // target exists or should. It goes on the log stream and not into the
    // table, which is the whole of the difference between a number that is
    // read as a trend and an artifact that has to compare byte for byte.
    tracing::info!(
        wall_clock_us =
            u64::try_from(clock.now().duration_since(started).as_micros()).unwrap_or(u64::MAX),
        queries = records.len(),
        "replayed"
    );

    if let Some(path) = &arguments.records {
        replay::write_records(path, &records)?;
    }
    table.render(arguments.format)
}

/// Here rather than in each `measure_<lang>` main, for the reason `core.md` §7
/// gives for putting `clap` here: a `measure_<lang>` is four lines, and the
/// seventh copy of a log setup is the seventh chance for one binary to be quiet
/// where the others are not.
///
/// **The default is `info`, where the shim's is `warn`**, and that is not a
/// disagreement with `deps.md` §9 but the scope of its reason: the shim is
/// quiet by default because its stderr is the editor's log panel with the
/// child's own output interleaved into it (`shim.md` §2). `measure` has no
/// child forwarding anything and no editor reading it — and §7 requires it to
/// *report* something, namely the replay's own wall clock beside the per-query
/// work counters, which `loops.md` §9 records from the very first run. A `warn`
/// default would emit that into nothing.
///
/// `RUST_LOG` still overrides, and there is deliberately no flag: §7's command
/// line is a closed set (`tests/pipeline.rs`), and a flag is a thing a run can
/// be misconfigured by where an environment variable is a thing an operator
/// reaches for.
fn install_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(stderr_for_logging)
        .try_init()
    {
        // Not discarded: a subscriber is already installed, and it is the one
        // that gets told. That is the case a test drives, where the scoped
        // subscriber is the point rather than an accident.
        tracing::debug!(%error, "a log subscriber was already installed");
    }
}

/// The single sanctioned `stderr` handle, the same one `heuristic_jump` has.
/// `clippy.toml` bans the rest because an ad-hoc write interleaves with the
/// child's forwarded stderr; its replacement is `tracing`, and this is where
/// `tracing` comes out.
#[expect(
    clippy::disallowed_methods,
    reason = "the log subscriber is what clippy.toml's stderr replacement points at"
)]
fn stderr_for_logging() -> std::io::Stderr {
    std::io::stderr()
}

/// The binary is per-language, so the language is the handler's rather than an
/// argument, and there is no flag that could disagree with it (`core.md` §7).
fn first_language_id(handler: &dyn LanguageHandler) -> Result<shared::LanguageId, Error> {
    handler
        .language_ids()
        .first()
        .copied()
        .ok_or_else(|| shared::HandlerError::DeadlineExpired.into())
}

/// The repository's files, filtered to the handler's own extensions and
/// sorted, because `enumerate` must produce the same positions twice.
fn source_files(
    handler: &dyn LanguageHandler,
    repository: &corpus::Repository,
) -> Result<Vec<ProjectPath>, Error> {
    let files = Arc::new(FileList::enumerate(std::slice::from_ref(&repository.path))?);
    let mut found: Vec<ProjectPath> = files
        .paths()
        .filter(|path| {
            handler
                .file_extensions()
                .iter()
                .any(|extension| extension.matches(path.rel().as_path()))
        })
        .cloned()
        .collect();
    found.sort_by(|left, right| left.rel().as_path().cmp(right.rel().as_path()));
    Ok(found)
}

#[expect(
    clippy::disallowed_methods,
    reason = "`std::io::stdout` is banned because stdout is the JSON-RPC wire — for the shim. This is a different program: `core.md` §7's table has `measure` as a plain LSP client with no editor on stdout, and §7's command line says `replay` *prints* the table and that `--format json` is what the harness consumes. There is no frame stream here to corrupt, and the one writer is this function."
)]
fn report(text: &str) -> Result<(), Error> {
    let mut out = std::io::stdout();
    out.write_all(text.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush())
        .map_err(|source| {
            shared::ChildError::Io {
                command: "stdout".into(),
                source,
            }
            .into()
        })
}
