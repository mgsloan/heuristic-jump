//! Direct dispatch. `design/core.md` §1: no framework, no config format that
//! languages have to be expressed in — the registry resolves a `languageId` or
//! a file extension to a handler, and the call is
//! `handler.goto_definition(&query)`.
//!
//! The registry is also where `driver` stays language-free: a handler hands
//! over its grammar as a runtime `tree_sitter::Language`
//! (`LanguageHandler::grammar`), so the parse cache can parse any registered
//! language without a build dependency on a single grammar crate.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use shared::{Deadline, Error, HandlerError, LanguageHandler, Outcome, Query};

/// The handler set, resolved once at startup. `heuristic_jump` is the one
/// place the language list is enumerated (`core.md` §9), so this takes the
/// handlers rather than knowing any of them.
pub struct Registry {
    handlers: Vec<Arc<dyn LanguageHandler>>,
    by_language_id: FxHashMap<&'static str, usize>,
    by_extension: FxHashMap<&'static str, usize>,
}

// By hand, and from `handlers` rather than from either map: a `LanguageHandler`
// is not required to be `Debug` — adding that to the seam would put a
// requirement on every `lang_*` crate for the sake of a derive — and iterating
// the maps would print in hash order, which `iter_over_hash_type` denies for
// exactly the reason that would bite here.
impl fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Registry")
            .field(
                "language_ids",
                &self
                    .handlers
                    .iter()
                    .flat_map(|handler| handler.language_ids())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Registry {
    /// First registration wins for a contested id, and the loser is logged
    /// rather than dropped silently: two handlers claiming `rust` is a wiring
    /// mistake in the binary, not a runtime condition to recover from.
    pub fn new(handlers: Vec<Arc<dyn LanguageHandler>>) -> Self {
        let mut by_language_id = FxHashMap::default();
        let mut by_extension = FxHashMap::default();

        for (index, handler) in handlers.iter().enumerate() {
            for language_id in handler.language_ids() {
                if let Some(existing) = by_language_id.insert(language_id.as_str(), index) {
                    tracing::warn!(
                        language_id = language_id.as_str(),
                        existing,
                        ignored = index,
                        "two handlers claim the same languageId"
                    );
                    by_language_id.insert(language_id.as_str(), existing);
                }
            }
            for extension in handler.file_extensions() {
                if let Some(existing) = by_extension.insert(extension.as_str(), index) {
                    tracing::warn!(
                        extension = extension.as_str(),
                        existing,
                        ignored = index,
                        "two handlers claim the same file extension"
                    );
                    by_extension.insert(extension.as_str(), existing);
                }
            }
        }

        Self {
            handlers,
            by_language_id,
            by_extension,
        }
    }

    /// An incoming LSP `languageId` is a string until this returns. A language
    /// nothing handles fails to resolve at the boundary rather than travelling
    /// inward as a string that matches nothing.
    pub fn for_language_id(&self, language_id: &str) -> Option<&dyn LanguageHandler> {
        let index = *self.by_language_id.get(language_id)?;
        self.handlers.get(index).map(Arc::as_ref)
    }

    /// For closed files found by search, which arrive as a bare path.
    pub fn for_path(&self, path: &Path) -> Option<&dyn LanguageHandler> {
        let extension = path.extension()?.to_str()?;
        let index = *self.by_extension.get(extension)?;
        self.handlers.get(index).map(Arc::as_ref)
    }
}

/// What the dispatch wrapper hands back.
///
/// The wire sees an abstention either way — a failure is not something a user
/// can act on, and the shim's job is to get out of the way (`shim.md` §11).
/// What differs is the *record*: a converted failure is written as
/// `decision: "failed"` with the error's class, never as an abstention, or
/// else a stratum with no coverage because resolution is hard and a stratum
/// with no coverage because the handler is broken become the same row
/// (`core.md` §1, §7).
#[derive(Debug)]
pub enum Dispatched {
    Decided(Outcome),
    /// The one error class mapped *back* to a decision. `ProjectView` fails a
    /// read whose deadline has expired, so a handler doing ordinary `?`
    /// propagation surfaces an expiry as `Err` — and a deadline expiry is the
    /// one latency-shaped abstention `high-level.md` allows. Recorded as an
    /// abstention, with `AbstainReason::Deadline`.
    DeadlineExpired,
    Failed(Error),
}

/// The direct call. No trait object registry lookup, no message, no
/// indirection beyond the one `&dyn` the handler set needs.
///
/// `core.md` §5's hard cap is applied here rather than left to the caller.
/// The deadline is not a fact the caller holds and this function does not —
/// it is `query.deadline`, which the handler was already required to poll, so
/// enforcing it here makes "the driver drops a late answer" a property of the
/// only path a handler is reached by.
pub fn dispatch(handler: &dyn LanguageHandler, query: &Query<'_>) -> Dispatched {
    hard_cap(query.deadline, call(handler, query))
}

/// The hard cap, separated from `dispatch` because it is the half of it that
/// can be tested: a `Query` cannot be built without a `DocumentSnapshot`, and
/// that cannot be built without a grammar, so there is no handler double until
/// a `lang_*` crate exists.
///
/// A late *failure* stays a failure. The cap exists to stop a late answer
/// reaching the user, and a failure is not an answer; recording it as an
/// expiry would merge a broken handler with a slow one, which is the
/// distinction `Dispatched` exists to keep (`core.md` §7).
pub fn hard_cap(deadline: &Deadline, dispatched: Dispatched) -> Dispatched {
    match dispatched {
        Dispatched::Decided(outcome) => {
            if deadline.expired() {
                // Not a handler bug on its own: a deadline can expire between
                // the last poll and the return. Visible at `debug` because the
                // record says `abstain`/`deadline` either way, so nothing
                // downstream can tell the two apart.
                tracing::debug!(
                    ?outcome,
                    "dropping an answer that arrived after its deadline"
                );
                Dispatched::DeadlineExpired
            } else {
                Dispatched::Decided(outcome)
            }
        }
        Dispatched::DeadlineExpired => Dispatched::DeadlineExpired,
        Dispatched::Failed(error) => Dispatched::Failed(error),
    }
}

fn call(handler: &dyn LanguageHandler, query: &Query<'_>) -> Dispatched {
    match handler.goto_definition(query) {
        Ok(outcome) => Dispatched::Decided(outcome),
        // Written as an exhaustive match on `Error` rather than a catch-all,
        // so that a new sub-enum has to be classified here instead of falling
        // into `Failed` by default.
        Err(error) => match &error {
            Error::Handler(HandlerError::DeadlineExpired) => Dispatched::DeadlineExpired,
            // `Encoding` is here for completeness rather than because a
            // handler can raise one: encoding stops at the dispatch wrapper
            // and never crosses the seam (`core.md` §3, §8.4), so a handler
            // has nothing to convert. If one ever appears here it is the
            // wrapper's own failure surfacing through the same `Result`, which
            // is a failure and not an abstention.
            Error::Encoding(_)
            | Error::Handler(_)
            | Error::Parse(_)
            | Error::Project(_)
            | Error::Protocol(_) => Dispatched::Failed(error),
        },
    }
}
