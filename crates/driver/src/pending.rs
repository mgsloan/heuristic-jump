//! `design/core.md#both-sides-are-sets`, from the driver's side: the record
//! `core` keeps for a query it has forwarded to the child, and what it does
//! when the child's answer eventually arrives.
//!
//! The predicate is not here. `shared::agreement` decides what "different"
//! means and `measure_core` reads the same one, which is the whole reason it
//! sits in `shared` (§6). What this module adds is the caller §6's closing
//! paragraph describes, and it makes two of that section's claims properties
//! of the types rather than rules somebody follows:
//!
//! * **The shim's answer is a ranked list and its order is load-bearing.**
//!   `answered_by_shim` is private, is set from the dispatch wrapper's
//!   [`Answer`] rather than from a caller-supplied vector, and the only way to
//!   read it back is [`PendingQuery::resolve`], which projects it into
//!   `DefinitionSite`s in stored order. There is nowhere for a caller to sort
//!   it, dedupe it, or hand it over as a set — which is what §6 rules out when
//!   it says `top1` "cannot be computed" from a collapsed list.
//!
//! * **Divergence is reported on `mismatch` only.** [`Divergence::of`] is the
//!   one constructor, it is private, and it takes its severity from
//!   `Agreement::severity()` — which is `None` on both match arms. A
//!   `match_contained` answer showed the user the correct location, so a report
//!   about it would be false; here it is not merely not-written, it is not
//!   constructible.
//!
//! `shim.md` §7's `State` is the shape of [`PendingQuery`], and its step 4 is
//! [`PendingQueries::child_answered`]. The transport that drives them does not
//! exist yet (`driver::run` has no actor), which is why nothing here reads a
//! channel: the record and the comparison are ordinary values, and testing them
//! needs no wire.

use std::time::Instant;

use shared::proto::{DefinitionResult, MessageType, ShowMessageParams};
use shared::{
    Agreement, DefinitionSite, DocumentUri, EditorRequestId, Location, Map, Offset, Outcome,
    Severity,
};

use crate::dispatch::Answer;

/// `shim.md` §7's `State`, field for field.
///
/// They exist for three things and none of them is matching one query against
/// another: cancellation, knowing which id the child's later response belongs
/// to, and carrying `arrived` so the trace record can report latency from the
/// user's point of view.
#[derive(Debug)]
pub struct PendingQuery {
    editor_id: EditorRequestId,
    uri: DocumentUri,
    position: Offset,
    arrived: Instant,
    /// Byte-space, as the handler returned it and **in the order it returned
    /// them**, kept for the divergence check — which compares `(uri, line)` and
    /// so needs nothing else. The wire form was built in the worker and sent;
    /// it is not retained (`core.md` §8.4).
    ///
    /// `None` is "the shim did not answer this query", which is not the same
    /// fact as `Some(vec![])` — a commit with no locations. §6 classifies the
    /// second (both sides empty is a match) and has nothing to say about the
    /// first, because there is no answer of ours to compare.
    answered_by_shim: Option<Vec<Location>>,
}

impl PendingQuery {
    /// Step 1 of §7's flow: the request has been forwarded to the child, and
    /// this is the record of it. `arrived` is passed rather than read, because
    /// `clippy.toml` bans `Instant::now` outside `SystemClock` so that the
    /// protocol-race tests can drive time rather than race it.
    pub fn new(
        editor_id: EditorRequestId,
        uri: DocumentUri,
        position: Offset,
        arrived: Instant,
    ) -> Self {
        Self {
            editor_id,
            uri,
            position,
            arrived,
            answered_by_shim: None,
        }
    }

    pub fn editor_id(&self) -> &EditorRequestId {
        &self.editor_id
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn position(&self) -> Offset {
        self.position
    }

    pub fn arrived(&self) -> Instant {
        self.arrived
    }

    /// Step 3: the handler returned and the dispatch wrapper's answer went to
    /// the editor.
    ///
    /// Taking an `&Answer` is the point. `Answer`'s locations are the ranked
    /// list the wrapper encoded and sent, in that order, and a caller has no
    /// opportunity to reorder them on the way in — where a `Vec<Location>`
    /// parameter would put the ordering claim in a doc comment and leave it
    /// there.
    ///
    /// An abstention is not an answer and is recorded as one having not
    /// happened (§5: "abstention is not an error", and it is not a commitment
    /// either). The match is exhaustive so that a third outcome has to be
    /// classified here.
    pub fn answered_by_shim(&mut self, answer: &Answer) {
        self.answered_by_shim = match answer.outcome() {
            Outcome::Committed {
                locations,
                confidence: _,
                strata: _,
                trace: _,
            } => Some(locations.clone()),
            Outcome::Abstain {
                reason: _,
                strata: _,
                trace: _,
            } => None,
        };
    }

    /// Step 4: the child answered, and this is §6's comparison of the two sets.
    ///
    /// `None` when the shim never answered this query. The child's answer is
    /// then the only one the user ever saw, there is nothing of ours for it to
    /// diverge from, and §7's record has no `agreement` field to fill — as
    /// distinct from a commit with an empty location list, which §6 does
    /// classify.
    pub fn resolve(&self, child: &DefinitionResult) -> Option<Resolution> {
        let ours = self.answered_by_shim.as_ref()?;
        // In stored order, and this is the only readout there is: `top1` is
        // `split_first` inside `classify`, so a projection that sorted or
        // deduped here would silently turn every `match_top1` into a coin flip.
        let sites: Vec<DefinitionSite<'_>> = ours.iter().map(DefinitionSite::of).collect();

        let agreement = Agreement::classify(&sites, child);
        Some(Resolution {
            agreement,
            divergence: Divergence::of(agreement, self, ours.first()),
        })
    }
}

/// What step 4 produces: the classification, which is always recorded, and the
/// user-facing report, which is not always sent.
///
/// The two are separate fields because `shim.md` §9 keeps them separate:
/// "every divergence is recorded for the metrics whether or not the user sees a
/// notification — display policy is a UI concern and must not reach the
/// numbers".
#[derive(Debug)]
pub struct Resolution {
    agreement: Agreement,
    divergence: Option<Divergence>,
}

impl Resolution {
    /// §7's record, on every resolved query.
    pub fn agreement(&self) -> Agreement {
        self.agreement
    }

    /// `None` on `match_top1` and `match_contained`.
    pub fn divergence(&self) -> Option<&Divergence> {
        self.divergence.as_ref()
    }
}

/// A divergence the user is told about.
///
/// Holding the `ShowMessageParams` rather than a recipe for one, because §9
/// calls the report "the safety mechanism": with the precision floor deferred,
/// it is the only thing standing between a wrong jump and a false belief the
/// user acts on, so it is built where the facts are and not left to a caller.
///
/// **What it cannot name is the symbol.** §9 asks the message to name "the
/// symbol queried and the location the shim sent them to", and only the second
/// is reachable from here: the record holds a `Offset` into a document
/// version that may be several edits gone by the time the child replies, and
/// resolving one to a token needs that version's tree. The symbol would have to
/// be captured at answer time, when the worker still has the snapshot. That is
/// `shim.md` §9's to close and is not this document's claim.
#[derive(Debug)]
pub struct Divergence {
    severity: Severity,
    message: ShowMessageParams,
}

impl Divergence {
    /// The only constructor, and the whole of "divergence is reported to the
    /// user on `mismatch` only".
    ///
    /// It is not written as a `match` on `Agreement` that returns `None` twice,
    /// because that is a rule the next caller can decline to follow.
    /// `Agreement::severity` is `None` on both match arms and a `Divergence`
    /// needs a `Severity`, so on a `match_contained` there is no report to
    /// suppress — there is no report.
    fn of(agreement: Agreement, query: &PendingQuery, top: Option<&Location>) -> Option<Self> {
        let severity = agreement.severity()?;
        Some(Self {
            severity,
            message: ShowMessageParams {
                message_type: MessageType::Warning,
                // §9: the report can arrive long after the jump, so it names
                // the jump rather than only the correction. Naming where the
                // user was sent is what makes it meaningful two minutes and
                // several files later.
                message: match top {
                    Some(top) => format!(
                        "Heuristic jump was wrong: sent you to {}:{} for {}:{}.",
                        top.uri(),
                        top.line().0 + 1,
                        query.uri(),
                        query.position().0,
                    )
                    .into_boxed_str(),
                    // A commit with no locations answered "there is no
                    // definition here" and the child disagreed. There is no
                    // place we sent them, so the message says so rather than
                    // inventing one.
                    None => format!(
                        "Heuristic jump was wrong: said there was no definition at {}:{}.",
                        query.uri(),
                        query.position().0,
                    )
                    .into_boxed_str(),
                },
            },
        })
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn message(&self) -> &ShowMessageParams {
        &self.message
    }
}

/// The table `core` holds pending queries in, keyed by `EditorRequestId`
/// (`shim.md` §7).
///
/// Not a general map: every entry is put in by `record` and taken out by
/// exactly one of `child_answered` and `cancelled`, so an id cannot be resolved
/// twice and a cancelled query cannot be answered afterwards.
#[derive(Debug, Default)]
pub struct PendingQueries {
    by_editor_id: Map<EditorRequestId, PendingQuery>,
}

impl PendingQueries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.by_editor_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_editor_id.is_empty()
    }

    /// Step 1. A duplicate id would be an editor reusing one while the first
    /// was still outstanding; the newer record wins and the older is logged,
    /// since silently dropping either would leave a query nothing ever
    /// resolves.
    pub fn record(&mut self, query: PendingQuery) {
        let editor_id = query.editor_id().clone();
        if let Some(replaced) = self.by_editor_id.insert(editor_id, query) {
            tracing::warn!(
                editor_id = replaced.editor_id().as_str(),
                "a second request arrived under an id already pending"
            );
        }
    }

    /// Step 3, by id. `false` when the id is not pending — a query cancelled
    /// while the handler was running, which `shim.md` §7 says must not be
    /// answered.
    pub fn answered_by_shim(&mut self, editor_id: &EditorRequestId, answer: &Answer) -> bool {
        match self.by_editor_id.get_mut(editor_id) {
            Some(query) => {
                query.answered_by_shim(answer);
                true
            }
            None => false,
        }
    }

    /// Step 4, which also ends the query's life: the child has responded, so
    /// there is nothing further to resolve under this id.
    ///
    /// The outer `Option` is "no such pending query" and the inner one is §6's
    /// "the shim did not answer, so there is nothing to compare"; they are
    /// different facts and a caller that flattened them would report a
    /// cancelled query and an abstained one identically.
    pub fn child_answered(
        &mut self,
        editor_id: &EditorRequestId,
        child: &DefinitionResult,
    ) -> Option<Option<Resolution>> {
        let query = self.by_editor_id.remove(editor_id)?;
        Some(query.resolve(child))
    }

    /// `$/cancelRequest`. The record is dropped, so a later child response
    /// finds nothing and reports nothing.
    pub fn cancelled(&mut self, editor_id: &EditorRequestId) -> Option<PendingQuery> {
        self.by_editor_id.remove(editor_id)
    }
}
