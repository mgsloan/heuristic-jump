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
use shared::proto::{PositionEncoding, WireLocation, WirePosition, WireRange};
use shared::{
    Deadline, DocumentUri, Error, FileText, HandlerError, LanguageHandler, Outcome, ProjectError,
    ProjectPath, Query, RelPath, Rope,
};

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
    Decided(Answer),
    /// The one error class mapped *back* to a decision. `ProjectView` fails a
    /// read whose deadline has expired, so a handler doing ordinary `?`
    /// propagation surfaces an expiry as `Err` — and a deadline expiry is the
    /// one latency-shaped abstention `high-level.md` allows. Recorded as an
    /// abstention, with `AbstainReason::Deadline`.
    DeadlineExpired,
    Failed(Error),
}

/// A decided query, in **both** of the two forms `core.md` §8.4 keeps apart:
/// the byte-space `Location`s the handler returned, and the `WireLocation`s
/// they encode to.
///
/// `core` sends `wire` to `writer:editor` and retains `outcome` in the pending
/// query, because §6's agreement predicate compares `(uri, line)` and the wire
/// form is never needed again (`shim.md` §7).
///
/// The fields are private and there is no constructor taking both, so the two
/// cannot be supplied separately and cannot disagree: the wire half is
/// *derived* from the byte half, the same shape that keeps `Location`'s `line`
/// from drifting away from its `range` (`conformance-004`).
#[derive(Debug)]
pub struct Answer {
    outcome: Outcome,
    wire: Vec<WireLocation>,
}

impl Answer {
    /// An outcome with nothing to encode: every abstention, and a commit whose
    /// location list is empty. The wire form of no locations is no locations,
    /// so this is the one `Answer` that can be built without a document to
    /// encode against and still cannot contradict itself.
    ///
    /// `None` for a commit that has locations, which has to go through
    /// `dispatch`.
    pub fn without_locations(outcome: Outcome) -> Option<Self> {
        match &outcome {
            Outcome::Committed {
                locations,
                confidence: _,
                stratum: _,
            } if !locations.is_empty() => None,
            Outcome::Committed { .. } | Outcome::Abstain { .. } => Some(Self {
                outcome,
                wire: Vec::new(),
            }),
        }
    }

    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    pub fn wire(&self) -> &[WireLocation] {
        &self.wire
    }
}

/// The direct call, plus the conversion onto the wire. No trait object
/// registry lookup, no message, no indirection beyond the one `&dyn` the
/// handler set needs.
///
/// `core.md` §5's hard cap is applied here rather than left to the caller.
/// The deadline is not a fact the caller holds and this function does not —
/// it is `query.deadline`, which the handler was already required to poll, so
/// enforcing it here makes "the driver drops a late answer" a property of the
/// only path a handler is reached by.
///
/// `encoding` is §8.4's third argument and stops here: it is `Copy`, settled
/// once from `InitializeResult`, and never reaches the handler — which is what
/// leaves §3's rule that no encoding crosses the seam intact while the answer
/// still gets onto the wire in the negotiated units. `tests/seam.rs` asserts
/// that no `lang_*` crate can name it.
///
/// The conversion is *here*, on the worker thread, rather than in `core` or in
/// `writer:editor`, because it reads the target file: `core` does only O(1)
/// state transitions and never touches the filesystem (`shim.md` §2), and the
/// target is frequently a file the editor never opened.
pub fn dispatch(
    handler: &dyn LanguageHandler,
    query: &Query<'_>,
    encoding: PositionEncoding,
) -> Dispatched {
    let dispatched = match call(handler, query)
        .and_then(|outcome| encode(outcome, encoding, query).map_err(classify))
    {
        Ok(answer) => Dispatched::Decided(answer),
        Err(dispatched) => dispatched,
    };
    hard_cap(query.deadline, dispatched)
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

/// `Err` is the already-classified non-answer, which is what lets `dispatch`
/// chain the call and the conversion: both fail in exactly the same currency,
/// and neither can reach `Dispatched::Decided` without an `Answer`.
fn call(handler: &dyn LanguageHandler, query: &Query<'_>) -> Result<Outcome, Dispatched> {
    handler.goto_definition(query).map_err(classify)
}

/// Never returns `Decided`: an outcome is what makes an answer, and an error
/// is not one.
fn classify(error: Error) -> Dispatched {
    // Written as an exhaustive match on `Error` rather than a catch-all, so
    // that a new sub-enum has to be classified here instead of falling into
    // `Failed` by default.
    match &error {
        Error::Handler(HandlerError::DeadlineExpired) => Dispatched::DeadlineExpired,
        // `Encoding` is a *failure*, and it is the wrapper's own rather than a
        // handler's: encoding stops at the dispatch wrapper and never crosses
        // the seam (`core.md` §3, §8.4), so the only way one arrives is
        // `encode` below refusing an offset that is not a character boundary —
        // a `Location` that names a place its own file does not have.
        // `Config`, `Codec` and `Child` are `measure_core`'s classes — the
        // corpus root, the JSON-RPC framing, the language server as a process.
        // A handler cannot reach any of them, and they are listed rather than
        // wildcarded because this match is the mechanism `deps.md` §10 relies
        // on: a new sub-enum must fail to compile until somebody says which
        // side of the decision it falls on.
        Error::Child(_)
        | Error::Codec(_)
        | Error::Config(_)
        | Error::Encoding(_)
        | Error::Handler(_)
        | Error::Parse(_)
        | Error::Project(_)
        | Error::Protocol(_) => Dispatched::Failed(error),
    }
}

/// §8.4's conversion: the byte-space answer, plus the same answer in the
/// negotiated encoding.
fn encode(
    outcome: Outcome,
    encoding: PositionEncoding,
    query: &Query<'_>,
) -> Result<Answer, Error> {
    let locations = match &outcome {
        Outcome::Committed {
            locations,
            confidence: _,
            stratum: _,
        } => locations.as_slice(),
        Outcome::Abstain {
            reason: _,
            stratum: _,
        } => &[],
    };

    let mut wire = Vec::with_capacity(locations.len());
    for location in locations {
        // One read per location, including several in one file. §8.4 prices
        // this at nearly free because "every target file's text is already in
        // the view's cache" — and there is no cache: `conformance-005` refused
        // one for want of a corpus and a benchmark, and adding it here would
        // be that ruling reversed on the same missing evidence.
        let text = target_text(location.uri(), query)?;
        let range = location.range();
        wire.push(WireLocation::new(
            location.uri().clone(),
            WireRange {
                start: WirePosition::encode(range.start, encoding, &text)?,
                end: WirePosition::encode(range.end, encoding, &text)?,
            },
        ));
    }

    Ok(Answer { outcome, wire })
}

/// The text a `Location`'s offsets are offsets into.
///
/// `Location` carries a `LineIndex` and this does not use it, which looks like
/// the redundancy §8.4 defends and is not: `WirePosition::encode` is
/// deliberately the only constructor and takes a whole `Rope` (§8.3), so the
/// read-free conversion the section describes needs a second constructor that
/// does not exist. The line still earns its place — §6's predicate is the
/// consumer that reads nothing (`shared::agreement`).
fn target_text(uri: &DocumentUri, query: &Query<'_>) -> Result<Rope, Error> {
    // The common case, and the free one: the definition is in the document the
    // query came from, whose rope the snapshot already holds. Cloning it is
    // three refcount bumps regardless of file size (`core.md` §2).
    if uri == &query.doc.uri {
        return Ok(query.doc.text.clone());
    }

    let path =
        project_path(uri, query).ok_or_else(|| ProjectError::Unresolvable { uri: uri.clone() })?;
    match query.project.read(&path)? {
        FileText::Disk(text) => Ok(Rope::from(&*text)),
        FileText::Open(text) => Ok(text),
    }
}

/// Back through `lookup`, which is one of the two ways a `ProjectPath` is
/// minted: a URI outside the file list resolves to nothing here rather than to
/// a path, so the scope rule survives the round trip through a `Location`.
fn project_path(uri: &DocumentUri, query: &Query<'_>) -> Option<ProjectPath> {
    let absolute = uri.to_file_path()?;
    let root = query.project.root_of(uri)?;
    let rel = RelPath::new(absolute.strip_prefix(root.path()).ok()?)?;
    query.project.lookup(root, &rel)
}
