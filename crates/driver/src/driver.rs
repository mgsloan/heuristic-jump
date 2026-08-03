//! The LSP driver: everything `design/shim.md` describes, generic over the
//! handler set. Named `driver` rather than `core` because a crate named `core`
//! shadows Rust's own and the prose already uses "core" for the single-threaded
//! actor (`design/core.md` §9).
//!
//! It depends on `shared` and on no language crate, and that direction is the
//! one rule the workspace graph is built around. `tests/seam.rs` asserts it
//! rather than leaving it to be read.

mod config;
mod dispatch;
mod files;
mod trees;

pub use config::{
    Config, DeadlineMs, DeadlineOverride, DebounceMs, Heuristics, Mode, ServerCommand,
};
pub use dispatch::{Answer, Completed, Dispatched, Parsed, Registry, Request, dispatch, hard_cap};
pub use files::{DID_CHANGE_WATCHED_FILES, FileListCache, Rescan};
pub use trees::{OpenDocument, TreeCache};

/// The whole of `heuristic_jump`'s `main` after argument parsing, and the
/// reason `driver` needs no opinion about how it was invoked: the binary hands
/// over the handler set and what it resolved from its argv, and everything
/// downstream of that is here (`core.md` §9, `deps.md` §11).
///
/// `shim.md` §13 puts this function in this file, beside the thread wiring and
/// the child spawn it will grow. It does not have them yet — there is no
/// transport, no codec and no actor — so today it reports what was resolved
/// and returns. The registry is taken by value regardless, because the point of
/// the seam is that the language list stops at the binary: once a thread owns
/// it, nothing above `driver` can reach a handler.
pub fn run(registry: Registry, config: Config) -> Result<(), shared::Error> {
    tracing::info!(
        mode = config.mode().name(),
        deadline_ms = config.deadline().get(),
        ?registry,
        "resolved configuration"
    );
    tracing::warn!("no transport yet: this build resolves its configuration and proxies nothing");
    Ok(())
}
