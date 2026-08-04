//! `shim.md` §13's `actor.rs`: the state `core` owns, and `shim.md` §7's
//! go-to-definition flow over it.
//!
//! Every module this one holds says in its own header that what it was missing
//! was an owner — `documents.rs`: "what is missing is the actor and the
//! transport"; `trees.rs` and `files.rs`: "what is missing is the channel".
//! This is that owner. Nothing here is shared and nothing is locked: the state
//! is owned by one thread and moves over channels, which is `core.md` §9's
//! whole concurrency model.
//!
//! **What is still missing is the transport**, and it is missing on purpose.
//! `shim.md` §2's codec, §3's router and the child spawn are a larger piece of
//! work in a document this phase does not audit, so [`Actor::run`] takes its
//! events from a channel rather than from a pipe: the state machine is here,
//! and what feeds it is not. `driver::run` therefore drives an actor nothing
//! sends to, which is why it returns immediately.
//!
//! Three of `core.md`'s claims become properties of this file rather than of
//! tests:
//!
//! * **§5's deadline is absolute and starts at request arrival.** It is minted
//!   in [`Actor::requested`] from the `arrived` instant the event carries, not
//!   from the clock at dispatch, and `queued_us` is the difference — so a
//!   handler given its full budget that started 200ms late is visible as such
//!   rather than as a fast handler and an unexplained abstention.
//! * **§6's pending-query record and the mismatch-only report.** Both are
//!   `pending.rs`'s, and this is where they are reached from.
//! * **§7's record.** One per query, emitted once both answers are known.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, select};
use serde_json::value::RawValue;
use shared::proto::{
    DefinitionParams, DefinitionResult, PositionEncoding, ShowMessageParams, WireLocation,
};
use shared::record::{Answered, ChildAnswer, QueryContext, QueryRecord, definition_labels, micros};
use shared::{
    Clock, CommitPolicy, Deadline, DocumentNotification, EditorRequestId, Error, InputEdit, Micros,
    Outcome, Strata, Stratum, Trace,
};

use crate::config::{Config, DebounceMs, Heuristics};
use crate::dispatch::{Answer, Completed, Dispatched, Registry, Request, dispatch};
use crate::documents::{Documents, Queried};
use crate::files::FileListCache;
use crate::pending::{PendingQueries, PendingQuery, Resolution};
use crate::trace::Traces;
use crate::trees::{OpenDocument, TreeCache};

/// What reaches `core` from the outside. `shim.md` §3's router produces these;
/// today only a test does.
///
/// Everything that carries a document's contents arrives as raw JSON, because
/// `core.md` §8.6's fail-closed rule starts at "any failure *while*
/// deserializing" — a sender that deserialized first would hold a
/// `serde_json::Error` and not the document it was about, which is exactly the
/// document that has drifted.
#[derive(Debug)]
pub enum Event {
    /// `shim.md` §4: the negotiation is over. Until this arrives the shim
    /// answers nothing itself — it has no root and no encoding, "so it cannot
    /// be correct".
    Negotiated {
        roots: Vec<PathBuf>,
        encoding: PositionEncoding,
    },
    /// One of §8.6's four state-bearing notifications.
    Notified {
        notification: DocumentNotification,
        params: Box<RawValue>,
    },
    /// `core.md` §4's second invalidation trigger, teed here by §3. It carries
    /// no payload, which is how "`core` does not read the payload" stays true
    /// by construction.
    WatchedFilesChanged,
    /// Step 1 of §7's flow. `arrived` is when the *request* arrived, which is
    /// where §5's deadline starts; it is passed rather than read, because the
    /// queueing this field exists to measure happened before `core` saw the
    /// event.
    Requested {
        editor_id: EditorRequestId,
        params: Box<RawValue>,
        arrived: Instant,
    },
    /// Step 4: the child answered. `latency` is the child's send-to-receive
    /// time, which only the transport can measure.
    ChildAnswered {
        editor_id: EditorRequestId,
        result: DefinitionResult,
        latency: Micros,
    },
    /// `$/cancelRequest`. The record is dropped, so a later child response
    /// finds nothing and reports nothing.
    Cancelled { editor_id: EditorRequestId },
}

/// What `core` sends out. `writer:editor` writes these; today only a test
/// reads them.
///
/// Two variants and not three: `shim.md` §9 asks for a `showMessageRequest`
/// followed by a `showDocument` when the client advertised one, and degrading
/// to a plain `window/showMessage` when it did not. Only the degraded form is
/// here, because the capability that chooses between them arrives in
/// `initialize` and nothing reads `initialize` yet — and `Divergence` already
/// holds a `ShowMessageParams`, so the choice is the reporter's to make when
/// there is one.
#[derive(Debug)]
pub enum Outbound {
    /// The shim's own answer to a definition request, in the negotiated
    /// encoding. Sent only when the handler *committed*: an abstention is not
    /// an answer, and in proxy mode the child's response is what reaches the
    /// editor (`shim.md` §8).
    Definition {
        editor_id: EditorRequestId,
        locations: Vec<WireLocation>,
    },
    /// §9's report, on `mismatch` only — which is `Divergence`'s property and
    /// not a rule followed here: `Divergence::of` is the one constructor and
    /// it produces nothing on either match arm.
    Report(ShowMessageParams),
}

/// What `initialize` settles, and what the shim cannot answer without
/// (`shim.md` §4).
///
/// An `Option<Negotiated>` rather than fields with defaults, because a default
/// encoding is a guess about the wire and a default root is a guess about the
/// project — and §4 is explicit that the shim answers nothing until it knows
/// both.
#[derive(Debug)]
struct Negotiated {
    files: FileListCache,
    encoding: PositionEncoding,
}

/// `shim.md` §2's single-threaded actor.
#[derive(Debug)]
pub struct Actor {
    registry: Registry,
    config: Config,
    clock: Arc<dyn Clock>,
    documents: Documents,
    trees: TreeCache,
    pending: PendingQueries,
    traces: Traces,
    policy: CommitPolicy,
    negotiated: Option<Negotiated>,
    debounce: DebounceMs,
    /// Always empty, and it is a field rather than a literal so that the day it
    /// stops being empty there is one place to fill.
    ///
    /// `OpenDocument` wants the edits applied since the *cached* tree was
    /// parsed, and `Documents::changed` applies a `didChange` without handing
    /// them back — so `core` has no edit log to offer. What it does instead is
    /// [`TreeCache::forget`] on every change, which makes the next seed a
    /// fresh parse: an empty edit log is then not a lie but a fact, since the
    /// only tree that can be cached is one at the version in hand. The cost is
    /// that incremental reparse is unreachable, which is a latency question
    /// and not a correctness one — and the alternative, an edit log built from
    /// changes `Documents` has already consumed, would hand tree-sitter edits
    /// that do not describe the text it is given.
    edits: Arc<Vec<InputEdit>>,
    outgoing: Sender<Outbound>,
}

impl Actor {
    pub fn new(
        registry: Registry,
        config: Config,
        clock: Arc<dyn Clock>,
        outgoing: Sender<Outbound>,
    ) -> Result<Self, Error> {
        let traces = Traces::resolve(config.tracing())?;
        Ok(Self {
            registry,
            config,
            clock,
            documents: Documents::new(),
            trees: TreeCache::default(),
            pending: PendingQueries::new(),
            traces,
            // `resolution.md` §7.1's permissive posture: nothing is gated on
            // confidence in v1, and the floor a `CommitPolicy` would carry is
            // what the corpus exists to derive.
            policy: CommitPolicy::permissive(),
            negotiated: None,
            debounce: DebounceMs::RESCAN,
            edits: Arc::new(Vec::new()),
            outgoing,
        })
    }

    /// The event loop. It returns when the event channel closes, which is what
    /// the transport going away looks like from in here.
    ///
    /// `recv` is banned by `clippy.toml` on a thread that owes an answer, and
    /// this one does: the timer arm is `core.md` §4's debounce tick, which is
    /// the reason the loop cannot simply block until something arrives.
    pub fn run(mut self, events: &Receiver<Event>) -> Result<(), Error> {
        loop {
            // Cloned per iteration rather than held, because the receiver only
            // exists once a root does — `select!` needs an arm that is there,
            // and a `Receiver` clone is a refcount bump.
            let rescans = self
                .negotiated
                .as_ref()
                .map(|negotiated| negotiated.files.rescans().clone());
            let arrived = match &rescans {
                Some(rescans) => select! {
                    recv(events) -> event => event.ok(),
                    recv(rescans) -> rescan => {
                        match (rescan, &mut self.negotiated) {
                            (Ok(rescan), Some(negotiated)) => negotiated.files.install(rescan),
                            (Ok(_), None) => {}
                            (Err(_), _) => tracing::warn!("the file-list scanner is gone"),
                        }
                        continue;
                    }
                    default(self.debounce.window()) => {
                        self.tick();
                        continue;
                    }
                },
                None => match events.recv_timeout(self.debounce.window()) {
                    Ok(event) => Some(event),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => None,
                },
            };
            let Some(event) = arrived else {
                tracing::debug!(
                    pending = self.pending.len(),
                    traced = self.traces.outstanding(),
                    "the event channel closed"
                );
                return Ok(());
            };
            self.handle(event)?;
        }
    }

    /// One event. Separate from the loop so that a test can drive the state
    /// machine without a channel, and so the loop is only the `select!`.
    pub fn handle(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Negotiated { roots, encoding } => self.negotiated(roots, encoding),
            Event::Notified {
                notification,
                params,
            } => {
                self.notified(notification, &params);
                Ok(())
            }
            Event::WatchedFilesChanged => {
                match &mut self.negotiated {
                    Some(negotiated) => negotiated.files.watched_files_changed(),
                    None => tracing::debug!("a watcher notification arrived before the root did"),
                }
                Ok(())
            }
            Event::Requested {
                editor_id,
                params,
                arrived,
            } => self.requested(editor_id, &params, arrived),
            Event::ChildAnswered {
                editor_id,
                result,
                latency,
            } => {
                self.child_answered(&editor_id, &result, latency);
                Ok(())
            }
            Event::Cancelled { editor_id } => {
                self.cancelled(&editor_id);
                Ok(())
            }
        }
    }

    /// `shim.md` §4. The root is what the file list is walked from and the
    /// encoding is what every wire position is in, so this is the event that
    /// makes the shim able to answer at all.
    fn negotiated(&mut self, roots: Vec<PathBuf>, encoding: PositionEncoding) -> Result<(), Error> {
        let files = FileListCache::new(roots, Arc::clone(&self.clock), self.debounce)?;
        if self
            .negotiated
            .replace(Negotiated { files, encoding })
            .is_some()
        {
            // A second `initialize` is a protocol error the editor made, not a
            // state to reconcile: the old file list is dropped with the old
            // root, which is the only reading that cannot leave a query
            // searching a project nobody is in.
            tracing::warn!("a second negotiation replaced the first");
        }
        Ok(())
    }

    /// `core.md` §8.6's four state-bearing notifications, projected inside
    /// `Documents` rather than here.
    fn notified(&mut self, notification: DocumentNotification, params: &RawValue) {
        let Some(negotiated) = &self.negotiated else {
            // §4: no sync kind and no encoding, so an incremental change has
            // no ranges we can resolve. Dropping it rather than guessing keeps
            // the map from holding a text nobody can vouch for.
            tracing::warn!(%notification, "a document notification arrived before the negotiation");
            return;
        };
        let synced = match notification {
            DocumentNotification::DidOpen => self.documents.opened(params, &self.registry),
            DocumentNotification::DidChange => {
                let synced = self.documents.changed(params, negotiated.encoding);
                // Every cached tree for this document, dropped: see `edits`.
                if let Ok(changed) =
                    serde_json::from_str::<shared::proto::NotifiedDocument>(params.get())
                {
                    self.trees.forget(&changed.text_document.uri);
                }
                synced
            }
            DocumentNotification::DidSave => {
                // The half that needs a worker's read is not wired: there is no
                // worker pool, and reading the file on this thread is the one
                // thing `shim.md` §2 forbids `core` outright. The free half —
                // a `didSave` that carried the text — is settled inside.
                match self.documents.saved(params) {
                    crate::documents::Saved::Checked(synced) => synced,
                    crate::documents::Saved::NeedsRead(check) => {
                        tracing::debug!(
                            uri = %check.uri(),
                            "a didSave checksum needs a read, and there is no worker to do it"
                        );
                        return;
                    }
                }
            }
            DocumentNotification::DidClose => {
                if let Ok(closed) =
                    serde_json::from_str::<shared::proto::NotifiedDocument>(params.get())
                {
                    self.trees.forget(&closed.text_document.uri);
                }
                self.documents.closed(params)
            }
        };
        tracing::trace!(%notification, ?synced, "a document notification was applied");
    }

    /// Steps 1 to 3 of `shim.md` §7's flow, minus the forwarding: the request
    /// has already gone to the child by the time it reaches `core`, because
    /// §1 forbids gating the forward on shim work.
    ///
    /// It is one function rather than several because the borrows are
    /// disjoint-by-field — the document map, the parse cache and the file list
    /// are all touched between building the seed and caching the tree — and
    /// splitting it into methods on `&mut self` would need clones of the very
    /// things `core.md` §2 keeps by reference.
    fn requested(
        &mut self,
        editor_id: EditorRequestId,
        params: &RawValue,
        arrived: Instant,
    ) -> Result<(), Error> {
        let asked = match serde_json::from_str::<DefinitionParams>(params.get()) {
            Ok(asked) => asked,
            // Not §8.6's rule: a definition request carries no document state,
            // so nothing we hold has drifted. The child answers it.
            Err(source) => {
                tracing::warn!(%source, "a definition request would not deserialize");
                return Ok(());
            }
        };
        let uri = asked.text_document.uri;

        let Some(negotiated) = &mut self.negotiated else {
            tracing::debug!(%uri, "a definition arrived before the negotiation; the child answers");
            return Ok(());
        };
        let document = match self.documents.query(&uri) {
            Queried::Trusted(document) => document,
            // Two different facts, and the log lines differ: a document nobody
            // opened is an editor and a shim that disagree about what exists,
            // where an untrusted one is §8.6 in force.
            Queried::NotOpen => {
                tracing::debug!(%uri, "a definition against a document we do not have");
                return Ok(());
            }
            Queried::Untrusted(error) => {
                tracing::debug!(%uri, %error, "a definition against an untrusted document");
                return Ok(());
            }
        };
        // Refusing rather than clipping, exactly as `Documents::changed` does:
        // a position that does not survive the round trip is the divergence,
        // and a clipped one answers a question the user did not ask.
        let position = match asked.position.resolve(negotiated.encoding, document.text()) {
            Ok(position) => position,
            Err(error) => {
                tracing::debug!(%uri, %error, "a definition at a position outside the document");
                return Ok(());
            }
        };

        // §5, and the whole of it: absolute, and started at the instant the
        // request arrived rather than at handler entry. Queueing time counts,
        // because the metric measures the user's point of view.
        let deadline = Deadline::new(
            Arc::clone(&self.clock),
            arrived,
            self.config.deadline().budget(),
        );

        // Step 1's record, before any decision about whether to answer: the
        // child was sent this query, and matching its later response to this
        // id is what the record is for even when the shim stays quiet.
        self.pending.record(PendingQuery::new(
            editor_id.clone(),
            uri.clone(),
            position,
            arrived,
        ));

        // Step 2, "check the policy". `shim.md` §6's health model does not
        // exist, so the table it would be a lookup in has exactly one row that
        // is not the default: `--proxy-only`, which is §11's permanent
        // degraded mode.
        if self.config.mode().heuristics() == Heuristics::Disabled {
            tracing::trace!(%uri, "heuristics are disabled; the child answers");
            return Ok(());
        }

        let Some(handler) = self
            .registry
            .for_language_id(document.language_id().as_str())
        else {
            // Unreachable through `Documents`, which interns a `LanguageId`
            // through this same registry — but the two lookups are separate
            // calls and this one is the honest way to say so.
            tracing::debug!(%uri, "no handler for a document the map is tracking");
            return Ok(());
        };
        let language = document.language_id();
        let grammar = handler.grammar();
        let seed = self
            .trees
            .seed(&OpenDocument::new(document, grammar, &self.edits));

        let started = self.clock.now();
        let completed = negotiated
            .files
            .view(deadline.clone(), handler.grammar())
            .map(|project| {
                dispatch(
                    handler,
                    Request {
                        seed,
                        position,
                        project: &project,
                        deadline: &deadline,
                        server: self.config.server(),
                        policy: &self.policy,
                    },
                    negotiated.encoding,
                )
            });
        let elapsed = self.clock.now().saturating_duration_since(started);

        let dispatched = match completed {
            Ok(Completed { dispatched, parsed }) => {
                // The parse is cached whatever the query decided: it was paid
                // for either way, and a query that abstained on its deadline is
                // the one most likely to be asked again a moment later.
                if let Some(parsed) = parsed {
                    self.trees.insert(parsed);
                }
                negotiated.files.observe(&dispatched);
                dispatched
            }
            // The file list could not be walked. It is the driver's failure
            // rather than the handler's, and §7 has one column for both: the
            // record says `failed` and names the class, because a stratum with
            // no coverage because the walk failed and one with no coverage
            // because resolution is hard must not be the same row.
            Err(error) => Dispatched::Failed(error),
        };

        let answered = self.answer(&editor_id, dispatched);
        let record = QueryRecord::new(
            &QueryContext {
                uri: &uri,
                position,
                language,
                mode: self.config.mode().recorded(),
                // `shim.md` §6's health model is not built, so there is no
                // value to report. `null` is what §7 gives standalone for the
                // same reason: there is no server whose health this is.
                server_health: None,
                queued: micros(started.saturating_duration_since(arrived)),
                elapsed: micros(elapsed),
            },
            answered,
        );
        match self.config.mode().recorded() {
            // No oracle is coming, so the row is complete (§7: in standalone
            // the four oracle columns are all null).
            shared::record::Mode::Standalone => self.traces.finished(record),
            shared::record::Mode::Proxy => self.traces.awaiting_child(editor_id, record),
        }
        Ok(())
    }

    /// Step 3: the answer reaches the editor, and the pending record learns
    /// what we said.
    ///
    /// The order is load-bearing. `answered_by_shim` is `false` when the id is
    /// no longer pending — a query cancelled while the handler was running,
    /// which `shim.md` §7 says must not be answered — so the send happens only
    /// if the record was still there.
    fn answer(&mut self, editor_id: &EditorRequestId, dispatched: Dispatched) -> Answered {
        match dispatched {
            Dispatched::Decided(answer) => {
                let live = self.pending.answered_by_shim(editor_id, &answer);
                let (outcome, wire) = Answer::into_parts(answer);
                let committed = matches!(outcome, Outcome::Committed { .. });
                if !live {
                    tracing::debug!("dropping an answer to a query that was cancelled");
                } else if committed {
                    self.send(Outbound::Definition {
                        editor_id: editor_id.clone(),
                        locations: wire,
                    });
                } else {
                    // Proxying, an abstention is silence and the child answers
                    // (`shim.md` §8). Standalone has nobody else to answer, and
                    // §14.5 — "abstention must say something" — is what fills
                    // this in; it is not built, and inventing a reply here
                    // would be inventing the thing that section decides.
                    tracing::debug!("abstained");
                }
                Answered::of(Ok(outcome))
            }
            // The outcome was dropped by the hard cap, and with it the stratum
            // the handler had assigned — so this row lands in `unimplemented`
            // rather than in the stratum it was really asked about, and §7's
            // coverage denominator moves by one query. The alternatives both
            // change something else's shape.
            // DECISION-core-017: provisional
            Dispatched::DeadlineExpired => Answered::of(Ok(Outcome::Abstain {
                reason: shared::AbstainReason::Deadline,
                strata: Strata::from_reference(Stratum::Unimplemented),
                trace: Trace::new(),
            })),
            // Served as an abstention on the wire — which here means silence —
            // and recorded as a failure, never as an abstention.
            Dispatched::Failed(error) => {
                tracing::warn!(%error, "a query failed; the shim stays quiet");
                Answered::of(Err(error))
            }
        }
    }

    /// Step 4: the child answered, so this query is over.
    ///
    /// The two `None`s are different facts and neither is an error. The outer
    /// one is "no such pending query" — cancelled, or never recorded — and the
    /// inner one is §6's "the shim did not answer, so there is nothing to
    /// compare".
    fn child_answered(
        &mut self,
        editor_id: &EditorRequestId,
        result: &DefinitionResult,
        latency: Micros,
    ) {
        let Some(resolved) = self.pending.child_answered(editor_id, result) else {
            tracing::trace!("a child response for a query nothing is pending on");
            self.traces.dropped(editor_id);
            return;
        };
        if let Some(divergence) = resolved.as_ref().and_then(Resolution::divergence) {
            tracing::info!(
                severity = %divergence.severity(),
                "the shim and the child disagreed",
            );
            self.send(Outbound::Report(divergence.message().clone()));
        }
        self.traces.child_answered(
            editor_id,
            ChildAnswer {
                latency,
                locations: definition_labels(result),
                agreement: resolved.as_ref().map(Resolution::agreement),
            },
        );
    }

    /// `$/cancelRequest`, and the two records that die with the query. The
    /// shim must not answer a cancelled request, which is what dropping the
    /// pending record buys: a handler that returns afterwards finds no id.
    ///
    /// **What it does not do is signal `Deadline::cancel`**, which `shim.md`
    /// §7 asks for beside the drop. It cannot, and the reason is worth writing
    /// down rather than rediscovering: dispatch is in-line on this thread, so
    /// a cancel is only ever handled *between* queries and there is no handler
    /// running to signal. `Deadline` is cancellable already and `core` would
    /// have to hold one per pending query to use it — state with no reader
    /// until `shim.md` §10's worker pool makes a dispatch outlive the event
    /// that started it. That is the campaign that closes it; a cancellation
    /// token wired up now would be unreachable code with a test that cannot
    /// fail.
    fn cancelled(&mut self, editor_id: &EditorRequestId) {
        match self.pending.cancelled(editor_id) {
            Some(_) => tracing::debug!("a pending query was cancelled"),
            // Harmless and explicitly not an error: `shim.md` §7 says the shim
            // can receive a stale cancel for a request it already answered.
            None => tracing::trace!("a cancel for a query nothing is pending on"),
        }
        self.traces.dropped(editor_id);
    }

    /// `core.md` §4's debounce, one tick of it. O(1), and it never walks.
    fn tick(&mut self) {
        if let Some(negotiated) = &mut self.negotiated {
            negotiated.files.refresh_if_due();
        }
    }

    fn send(&self, outbound: Outbound) {
        if self.outgoing.send(outbound).is_err() {
            // The writer is gone, which means the editor is. Logged rather
            // than propagated: there is nothing to recover, and the loop ends
            // when the event channel closes for the same reason.
            tracing::debug!("nothing is reading what `core` writes");
        }
    }
}
