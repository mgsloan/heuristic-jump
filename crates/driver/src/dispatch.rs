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

use shared::proto::{PositionEncoding, WireLocation, WirePosition, WireRange};
use shared::{
    ByteLen, CommitPolicy, Deadline, DocumentSnapshot, DocumentUri, DocumentVersion, EncodingError,
    Error, FileText, HandlerError, LanguageHandler, LanguageId, Map, Offset, Outcome, ProjectError,
    ProjectPath, ProjectView, Query, RelPath, Rope, ServerProfile, SnapshotSeed, Strata, Stratum,
    Tree,
};

/// The handler set, resolved once at startup. `heuristic_jump` is the one
/// place the language list is enumerated (`core.md` §9), so this takes the
/// handlers rather than knowing any of them.
pub struct Registry {
    handlers: Vec<Arc<dyn LanguageHandler>>,
    by_language_id: Map<&'static str, usize>,
    by_extension: Map<&'static str, usize>,
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
        let mut by_language_id = Map::default();
        let mut by_extension = Map::default();

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
    ///
    /// The `Arc` is kept rather than dereferenced away because a dispatch
    /// outlives the event that started it (`shim.md` §10): a worker holding a
    /// borrow of the registry would be a worker holding a borrow of `core`.
    pub fn for_language_id(&self, language_id: &str) -> Option<&Arc<dyn LanguageHandler>> {
        let index = *self.by_language_id.get(language_id)?;
        self.handlers.get(index)
    }

    /// The same resolution, interning rather than dispatching: an incoming
    /// `languageId` is a `Box<str>` (`core.md` §8.2 leaves it that way
    /// deliberately), and a [`LanguageId`] is only obtainable from the handler
    /// that declared it. This is the lookup that turns one into the other, and
    /// it is why `Documents` cannot invent an id for a language nothing
    /// handles.
    pub fn language_id(&self, language_id: &str) -> Option<LanguageId> {
        self.for_language_id(language_id)?
            .language_ids()
            .iter()
            .copied()
            .find(|declared| declared.as_str() == language_id)
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
    DeadlineExpired(Classified),
    Failed(Error),
}

/// What had classified a query at the moment the deadline took its answer
/// away.
///
/// It exists because `core.md` §7 reports coverage on `stratum_prior` "so the
/// denominator is fixed by the reference and does not move when the
/// implementation changes", and an expiry that lost the stratum moved it: the
/// query was counted under `unimplemented` rather than under the class it was
/// really asked about. `core-017` settles why it need not be lost — *a-priori*
/// is about the rule, which "reads only the query and the reference and never
/// what the search found", so the prior is knowable before the search finishes
/// and "was never the outcome's to carry away".
#[derive(Copy, Clone, Debug)]
pub enum Classified {
    /// A handler returned and something downstream of it discarded the answer:
    /// the hard cap, or an expiry during §8.4's conversion. Either way the
    /// strata it assigned are in hand.
    By(Strata),
    /// Nothing classified anything. The parse was abandoned before any handler
    /// ran (`SnapshotSeed::realise`), or a read expired inside the handler
    /// before it assigned a stratum.
    Nothing,
}

impl Classified {
    /// What §7's two stratum columns are written from.
    pub fn strata(self) -> Strata {
        match self {
            Classified::By(strata) => strata,
            // The prior exists — `core-017` says so, and says the reference and
            // the query are all its rule needs — but the rule is
            // `resolution.md` §8's and is per-language by construction, so
            // nothing here can evaluate it without the handler that owns it.
            // Filed under the template's stratum for want of anywhere honest to
            // put it, which is the same place `Answered::of` files a handler
            // that returned `Err`.
            //
            // `core-025` is accepted and this is its site. It rules **C then
            // B**: `ProjectView`'s expiry carries out the strata the handler
            // had, as a change to `Error` — which empties the second of the two
            // routes into `Nothing` above, leaving only the abandoned parse —
            // and `stratum_prior` then becomes nullable for that residue,
            // because "nothing ever looked at this reference" is the absence of
            // a measurement rather than a kind of reference. So this arm does
            // not get a better `Stratum`; it stops returning one.
            //
            // Tagged for `core-025` and not for `core-022`, which asked the same
            // question from the driver's side and is closed as its duplicate:
            // the ruling, and the work it leaves, are only in `core-025`, and a
            // tag naming the closed record is one a search for the open work
            // does not find.
            // DECISION-core-025: provisional
            Classified::Nothing => Strata::from_reference(Stratum::Unimplemented),
        }
    }
}

impl Dispatched {
    /// The classification a handler already made, attached to an expiry that
    /// arrived without one.
    ///
    /// [`classify`] cannot know it: it is handed an `Error`, and the strata are
    /// on the `Outcome` that error is instead of. So an expiry raised
    /// *downstream* of a handler that classified — during §8.4's conversion,
    /// which reads the target file — picks it back up here, and one raised
    /// before any handler ran keeps the `Nothing` it was given.
    fn or_classified_by(self, strata: Strata) -> Self {
        match self {
            Dispatched::DeadlineExpired(Classified::Nothing) => {
                Dispatched::DeadlineExpired(Classified::By(strata))
            }
            Dispatched::DeadlineExpired(Classified::By(strata)) => {
                Dispatched::DeadlineExpired(Classified::By(strata))
            }
            Dispatched::Decided(answer) => Dispatched::Decided(answer),
            // A failure is not an abstention and carries no stratum: §7 records
            // it as `failed`, and `Answered::of` files it under the same
            // placeholder for the same reason.
            Dispatched::Failed(error) => Dispatched::Failed(error),
        }
    }
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
                strata: _,
                trace: _,
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

    /// Consuming, because `core` needs both halves and neither of them twice:
    /// the wire form goes to the editor and the outcome goes into §7's record,
    /// which takes the `Trace` apart and so cannot borrow it.
    ///
    /// Taking an `Answer` apart is safe where building one from parts is not.
    /// What the private fields prevent is an `Answer` whose two halves describe
    /// different answers; after this there is no `Answer` left to disagree with
    /// itself.
    pub fn into_parts(self) -> (Outcome, Vec<WireLocation>) {
        (self.outcome, self.wire)
    }
}

/// What `core` puts on the work channel, and the reason it can do so in O(1):
/// the document arrives as a `SnapshotSeed` — three refcount bumps and a
/// struct move — rather than as a parsed `DocumentSnapshot` (`core.md` §2).
///
/// It carries a seed and not a snapshot deliberately. `Query` is what a
/// handler is given, and there is no way to reach one except through
/// `dispatch`, which parses first; so the parse happens on the worker thread
/// and inside the deadline, never in `core`, and a handler cannot be handed a
/// document that nobody parsed.
///
/// Everything but the seed is borrowed, because `core` owns it and a query
/// does not outlive its dispatch.
#[derive(Debug)]
pub struct Request<'a> {
    pub seed: SnapshotSeed,
    pub position: Offset,
    pub project: &'a ProjectView,
    pub deadline: &'a Deadline,
    pub server: &'a ServerProfile,
    pub policy: &'a CommitPolicy,
}

/// The direct call, plus the parse in front of it and the conversion onto the
/// wire behind it. No trait object registry lookup, no message, no
/// indirection beyond the one `&dyn` the handler set needs.
///
/// The parse is first and is the worker's: `realise` is where an unparseable
/// document fails, which is what keeps `DocumentSnapshot::tree` infallible and
/// keeps `core` free of parsing (`core.md` §2). A parse abandoned on the
/// deadline surfaces as `HandlerError::DeadlineExpired` and is classified
/// exactly like a handler's own expiry — the query ran out of time, and
/// nothing about the document is wrong.
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
    request: Request<'_>,
    encoding: PositionEncoding,
) -> Completed {
    let Request {
        seed,
        position,
        project,
        deadline,
        server,
        policy,
    } = request;

    let document = match realise(seed, deadline) {
        Ok(document) => document,
        // No tree, so nothing to cache: this is the one path where the parse
        // did not happen.
        Err(dispatched) => {
            return Completed {
                dispatched: hard_cap(deadline, dispatched),
                parsed: None,
            };
        }
    };
    let parsed = Parsed::of(&document);

    let query = Query {
        doc: &document,
        position,
        project,
        deadline,
        server,
        policy,
    };
    let dispatched = match call(handler, &query) {
        // The strata are taken before the conversion because the conversion can
        // end the query: §8.4's `encode` reads the target file, and
        // `ProjectView` fails a read whose deadline has already expired. That
        // expiry is downstream of a handler that *did* classify, so the
        // classification survives it.
        Ok(outcome) => {
            let strata = strata_of(&outcome);
            match encode(outcome, encoding, &query) {
                Ok(answer) => Dispatched::Decided(answer),
                Err(error) => classify(error).or_classified_by(strata),
            }
        }
        Err(dispatched) => dispatched,
    };

    Completed {
        dispatched: hard_cap(deadline, dispatched),
        // Handed back whatever the query decided, and deliberately: the parse
        // was paid for either way, and a query that abstained on its deadline
        // is the one most likely to be asked again a moment later.
        parsed: Some(parsed),
    }
}

/// Everything a worker hands back: the answer, and — separately — the tree, so
/// that a parse paid for once is not paid for again.
///
/// Two fields rather than a tree inside `Dispatched`, because they are
/// independent facts. A query can fail, expire or abstain and still have
/// produced a perfectly good tree, and that tree is exactly what makes the
/// next query on the document cheap.
#[derive(Debug)]
pub struct Completed {
    pub dispatched: Dispatched,
    /// `None` only when `realise` failed, which is the one case where no tree
    /// exists.
    pub parsed: Option<Parsed>,
}

/// `core.md` §2's message back to `core`: the tree the worker parsed, and the
/// version it is a tree *of*.
///
/// The constructor is private to this module and there is no other, so the
/// only way to hold one is to have called `dispatch` — and `TreeCache::insert`
/// consumes one, so the only thing to do with it is cache it. That is what
/// makes "the dispatch wrapper, not the handler, sends it; the handler is not
/// involved and cannot forget" a property of the types rather than a rule
/// somebody follows.
#[derive(Debug)]
pub struct Parsed {
    uri: DocumentUri,
    version: DocumentVersion,
    /// The length of the text it was parsed from, which is what the parse
    /// cache's byte ceiling counts (`deps.md` §8). Taken here rather than in
    /// the cache because this is where the document still exists — `Rope::len`
    /// is a summary read and the tree carries no size of its own.
    bytes: ByteLen,
    tree: Tree,
}

impl Parsed {
    /// Three clones, and all of them refcount bumps: `Tree::clone` is
    /// `ts_subtree_retain` (`core.md` §2), and the version is a number.
    fn of(document: &DocumentSnapshot) -> Self {
        Self {
            uri: document.uri.clone(),
            version: document.version,
            bytes: document.text.len(),
            tree: document.tree().clone(),
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    /// `pub(crate)` and consuming, so `TreeCache` can take the tree out and
    /// nothing outside `driver` can put a `Parsed` back together from pieces
    /// it obtained some other way.
    pub(crate) fn into_parts(self) -> (DocumentUri, DocumentVersion, ByteLen, Tree) {
        (self.uri, self.version, self.bytes, self.tree)
    }
}

/// The hard cap, separated from `dispatch` so that it can be tested on a
/// `Dispatched` alone. That was once the only way to reach it — a `Query`
/// needs a `DocumentSnapshot`, which needs a grammar, so there was no handler
/// double until something in the workspace could supply one — and it no longer
/// is: `driver` takes `tree-sitter-rust` as a dev-dependency and
/// `tests/actor.rs` caps a real answer end to end. What the split still buys
/// is a test of the arm that discards, without a document to produce one.
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
                // The answer is dropped. The stratum it was asked under is not:
                // it is what §7's coverage denominator is grouped by, and a
                // query does not change class by being late.
                Dispatched::DeadlineExpired(Classified::By(strata_of(outcome.outcome())))
            } else {
                Dispatched::Decided(outcome)
            }
        }
        Dispatched::DeadlineExpired(classified) => Dispatched::DeadlineExpired(classified),
        Dispatched::Failed(error) => Dispatched::Failed(error),
    }
}

/// The strata an outcome carries, on either arm — `core.md` §1 reports the
/// stratum on both, "because coverage per stratum is meaningless without
/// knowing which stratum the abstentions belonged to".
///
/// The fields are spelled out rather than elided so that a new one on either
/// variant has to be looked at here, which is the rule the seam types are
/// matched under everywhere else in this file.
fn strata_of(outcome: &Outcome) -> Strata {
    match outcome {
        Outcome::Committed {
            locations: _,
            confidence: _,
            strata,
            trace: _,
        }
        | Outcome::Abstain {
            reason: _,
            strata,
            trace: _,
        } => *strata,
    }
}

/// The worker's half of `core.md` §2's two-step split, and the first thing
/// `dispatch` does. Separated so the classification is written once: a parse
/// that ran out of time is the query's expiry and a parse that failed is the
/// document's, and `classify` already knows which is which.
fn realise(seed: SnapshotSeed, deadline: &Deadline) -> Result<DocumentSnapshot, Dispatched> {
    seed.realise(deadline).map_err(classify)
}

/// `Err` is the already-classified non-answer: the call and the conversion fail
/// in the same currency, and neither can reach `Dispatched::Decided` without an
/// `Answer`.
///
/// A handler that gets as far as `Err` has classified nothing — the stratum is
/// on the `Outcome` it did not return — which is why this one needs no
/// classification handed to it and the conversion's does.
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
        Error::Handler(HandlerError::DeadlineExpired { classified }) => {
            // `deps.md` §10: "Some `driver` code will convert an `Error` into
            // an abstention; that conversion is explicit and logged." This is
            // that code, and this is the log. It is the *only* site where an
            // `Error` stops being a failure — everything else here stays
            // `Failed` — so logging here covers all three callers: a parse
            // abandoned in `realise`, a handler's own `?` propagation through
            // `call`, and an expiry raised inside §8.4's conversion.
            //
            // Deliberately not repeated in `Actor::answer`, which is where the
            // `Outcome::Abstain` is finally built. Every `DeadlineExpired` that
            // reaches it has been logged exactly once already: this arm for the
            // ones converted from an `Error`, and `hard_cap`'s line for the one
            // that never was an `Error` at all — an answer that was merely
            // late. A second line there would report both twice and say nothing
            // the record does not.
            //
            // `debug`, the level `hard_cap` drops a late answer at: an expiry
            // is what §5's budget is *for*, so it is normal operation and not a
            // fault. The rate is §7's to report; this is for reading one query.
            tracing::debug!(%error, "converting an expiry into an abstention");
            // `core-025`, option C: the expiry carries the prior the handler
            // published before it started the I/O, so the commonest shape in the
            // field — classified from the reference, then `?` out of an expired
            // read — keeps the class §7's coverage denominator groups it by.
            // What is left arriving here with nothing is the parse abandoned
            // before any handler ran, which is the residue option B is for.
            Dispatched::DeadlineExpired(match classified {
                Some(prior) => Classified::By(Strata::from_reference(*prior)),
                None => Classified::Nothing,
            })
        }
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
        //
        // `Document` is the one that cannot arrive here at all rather than
        // merely not arriving from a handler: §8.6 converts every one of them
        // to an abstention *before* a query is built, and `Documents::query`
        // yields no `Trusted` for a document that suffered one — so there is
        // no `SnapshotSeed`, no `Request`, and nothing to dispatch. One
        // reaching this arm would mean the map handed out a document it had
        // stopped believing, which is a failure and not an abstention.
        Error::Child(_)
        | Error::Codec(_)
        | Error::Config(_)
        | Error::Document(_)
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
            strata: _,
            trace: _,
        } => locations.as_slice(),
        Outcome::Abstain {
            reason: _,
            strata: _,
            trace: _,
        } => &[],
    };

    let mut wire = Vec::with_capacity(locations.len());
    for location in locations {
        // One read per location, including several in one file.
        // `conformance-005` refused a per-query read cache for want of a
        // corpus and a benchmark, and adding one here would be that ruling
        // reversed on the same missing evidence
        // (CHANGE-conformance-014).
        let text = target_text(location.uri(), query)?;

        // The one thing that ruling makes reachable, and the reason §8.4's
        // carried row earns its place on this path rather than only in §6's
        // predicate: the handler's offsets were taken against the text it
        // read, and this is a *second* read of the same file. A file edited in
        // between gives offsets that are stale and still in range, and the
        // carried row is the only witness that they moved. Failing closed
        // rather than encoding it: an answer pointing confidently at the wrong
        // place is the failure §8.6 spends a section refusing to produce.
        let found = location.line_in(&text);
        if found != location.line() {
            return Err(EncodingError::LineDisagreesWithRange {
                carried: location.line(),
                found,
            }
            .into());
        }

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
