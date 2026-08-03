//! `design/core.md` §7's command line, one subcommand per stage of
//! `data-collection.md`.
//!
//! **The binary is per-language, so the language is never an argument.** Which
//! language a run is about comes from the handler `measure_core::run` is
//! given, and there is no flag that could disagree with it.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(version, about = "corpus scan for one language")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parse each repository, sample positions, write `positions/<repo>.jsonl`.
    Enumerate(Enumerate),
    /// Drive the real server and freeze its answers into `truth.jsonl`.
    Collect(Collect),
    /// Run the handler against the frozen truth. No server, no network.
    Replay(Replay),
}

#[derive(Debug, Args)]
pub struct Corpus {
    /// The corpus **split** — `training/` or `test/` — not the corpus root.
    ///
    /// Required and undefaulted, on purpose twice over. A defaulted corpus
    /// path is one that eventually points at the wrong one; and held-out is
    /// selected by passing a different path rather than by a `--held-out`
    /// flag, so a session that is not given the path cannot reach the data.
    /// A flag is something a loop can set; a path it was never told is not.
    #[arg(long, value_name = "DIR")]
    pub corpus: PathBuf,

    /// Restrict the run to these repositories. Empty means all of them.
    #[arg(long = "repo", value_name = "NAME")]
    pub repositories: Vec<String>,
}

#[derive(Debug, Args)]
pub struct Enumerate {
    #[command(flatten)]
    pub corpus: Corpus,

    /// Positions per repository. `data-collection.md` §3: exhaustive
    /// enumeration does not survive contact with arithmetic.
    #[arg(long, value_name = "N", default_value_t = 20_000)]
    pub limit: usize,

    /// An unseeded sample is a corpus that cannot be regenerated, which
    /// defeats freezing it.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub seed: u64,
}

#[derive(Debug, Args)]
pub struct Collect {
    #[command(flatten)]
    pub corpus: Corpus,

    /// Resolved through `servers.toml` at the root of the code repository.
    /// Naming a server rather than passing a command line is what lets the
    /// provenance header record what was actually run without trusting the
    /// invocation to be repeated correctly.
    #[arg(long, value_name = "NAME")]
    pub server: String,

    /// Discard a partial truth file and start over. Resuming is the default,
    /// because this is the destructive option and therefore the explicit one.
    #[arg(long)]
    pub restart: bool,
}

#[derive(Debug, Args)]
pub struct Replay {
    #[command(flatten)]
    pub corpus: Corpus,

    #[arg(long, value_name = "NAME")]
    pub server: String,

    #[arg(long, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Additionally dump the per-query JSONL of `core.md` §7's record,
    /// unchanged and unfiltered. With no `--records` this writes nothing, so
    /// the default stays a pure function of its inputs and `measure_core`
    /// still needs no knowledge of `state/`.
    #[arg(long, value_name = "PATH")]
    pub records: Option<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Format {
    Table,
    /// What the harness consumes.
    Json,
}
