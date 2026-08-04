//! The LSP driver: everything `design/shim.md` describes, generic over the
//! handler set. Named `driver` rather than `core` because a crate named `core`
//! shadows Rust's own and the prose already uses "core" for the single-threaded
//! actor (`design/core.md` §9).
//!
//! It depends on `shared` and on no language crate, and that direction is the
//! one rule the workspace graph is built around. `tests/seam.rs` asserts it
//! rather than leaving it to be read.

use std::sync::Arc;

mod actor;
mod config;
mod dispatch;
mod documents;
mod files;
mod pending;
mod trace;
mod trees;

pub use actor::{Actor, Event, Outbound};
pub use config::{
    Config, DEFAULT_LOG_FILTER, DeadlineMs, DeadlineOverride, DebounceMs, Heuristics, LOG_PREFIX,
    Mode, PrefixedWriter, ServerCommand,
};
pub use dispatch::{Answer, Completed, Dispatched, Parsed, Registry, Request, dispatch, hard_cap};
pub use documents::{Documents, Queried, SaveCheck, Saved, Synced, Trusted};
pub use files::{DID_CHANGE_WATCHED_FILES, FileListCache, Rescan};
pub use pending::{Divergence, PendingQueries, PendingQuery, Resolution};
pub use trace::{Traces, Tracing};
pub use trees::{CacheBytes, CacheEntries, OpenDocument, TreeCache};

/// The whole of `heuristic_jump`'s `main` after argument parsing, and the
/// reason `driver` needs no opinion about how it was invoked: the binary hands
/// over the handler set and what it resolved from its argv, and everything
/// downstream of that is here (`core.md` §9, `deps.md` §11).
///
/// `shim.md` §13 puts this function in this file, beside the thread wiring and
/// the child spawn it will grow. The registry is taken by value because the
/// point of the seam is that the language list stops at the binary: once a
/// thread owns it, nothing above `driver` can reach a handler.
///
/// **What it has is the actor; what it does not have is the transport.**
/// `Actor` is `core.md` §7's request path — it mints §5's deadline at request
/// arrival, keeps §6's pending-query record, and emits §7's trace record — and
/// this function builds and drives one. What is missing is `shim.md` §2's
/// codec and §3's router: nothing reads stdin, so nothing ever sends an
/// [`Event`], so the loop below sees a channel that is already closed and
/// returns. Every line of that is deliberate rather than unfinished-and-
/// unnoticed: the state machine is testable and tested without a wire, and the
/// wire is a document this phase does not audit.
///
/// The two channels are created here rather than inside the actor because they
/// are what the reader and writer threads will be handed. Dropping `events`
/// immediately is how "there is no transport" is spelled as a value.
pub fn run(registry: Registry, config: Config) -> Result<(), shared::Error> {
    tracing::info!(
        mode = config.mode().name(),
        deadline_ms = config.deadline().get(),
        ?registry,
        "resolved configuration"
    );

    let (events, incoming) = crossbeam_channel::unbounded();
    let (outgoing, written) = crossbeam_channel::unbounded();
    let actor = Actor::new(registry, config, Arc::new(shared::SystemClock), outgoing)?;

    tracing::warn!("no transport yet: this build resolves its configuration and proxies nothing");
    drop(events);
    drop(written);

    actor.run(&incoming)
}
