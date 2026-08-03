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

pub use cli::{Cli, Command, Format};
pub use record::{Decision, Mode, QueryRecord, StratumName};

/// The whole of a `measure_<lang>` binary, after `Cli::parse()`.
///
/// ```ignore
/// fn main() -> Result<(), shared::Error> {
///     measure_core::run(&lang_rust::Handler::new(), Cli::parse())
/// }
/// ```
pub fn run(handler: &dyn LanguageHandler, cli: Cli) -> Result<(), Error> {
    let clock = SystemClock;
    let language = handler
        .language_ids()
        .first()
        .copied()
        .ok_or(shared::HandlerError::DeadlineExpired)?;

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
            let server = corpus::resolve_server(language, &arguments.server)?;
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

        Command::Replay(arguments) => {
            let corpus = corpus::Corpus::open(&arguments.corpus.corpus, language)?;
            let replay = replay::Replay {
                handler,
                corpus: &corpus,
                server: &arguments.server,
                clock: &clock,
            };

            let mut table = table::Table::new();
            let mut records = Vec::new();
            for repository in &corpus.repositories(&arguments.corpus.repositories)? {
                replay.run(repository, &mut table, &mut records)?;
            }

            // With no `--records` this writes nothing, so the default stays a
            // pure function of its inputs.
            if let Some(path) = &arguments.records {
                replay::write_records(path, &records)?;
            }
            report(&table.render(arguments.format)?)
        }
    }
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
