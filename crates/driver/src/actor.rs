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
//!
//! **A query is two events here and not one.** [`Actor::requested`] builds a
//! `SnapshotSeed` and hands it to `workers.rs`; [`Actor::finished`] is where
//! the answer comes back. That split is `core.md` §2's — "`core` builds seeds
//! and never realises one" — and everything awkward in this file follows from
//! it: [`InFlight`] exists because a dispatch now outlives the event that
//! started it, so the child's response can arrive first, a `$/cancelRequest`
//! has a running handler to signal, and [`Actor::run`] has work to drain when
//! the wire closes.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, RecvError, Sender, select};
use serde_json::value::RawValue;
use shared::proto::{
    DefinitionParams, DefinitionResult, NotifiedDocument, PositionEncoding, ShowMessageParams,
    WireLocation,
};
use shared::record::{Answered, ChildAnswer, QueryContext, QueryRecord, definition_labels, micros};
use shared::{
    Clock, CommitPolicy, Deadline, DocumentNotification, DocumentUri, EditorRequestId, Error,
    InputEdit, Map, Micros, Outcome, Trace,
};

use crate::config::{Config, DebounceMs, Heuristics};
use crate::dispatch::{Answer, Completed, Dispatched, Registry};
use crate::documents::{Documents, Queried};
use crate::files::FileListCache;
use crate::pending::{PendingQueries, PendingQuery, Resolution};
use crate::trace::Traces;
use crate::trees::{OpenDocument, TreeCache};
use crate::workers::{Asked, Dispatchable, Finished, Job, Workers};

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

/// A query the pool still holds.
#[derive(Debug)]
struct InFlight {
    /// Which document it is parsing, so that a notification about that
    /// document can find it. Nothing else `core` holds can: `PendingQueries`
    /// is keyed by request id and the pool is a channel.
    uri: DocumentUri,
    /// §5's deadline, which is only worth retaining now that a dispatch
    /// outlives the event that started it: `$/cancelRequest` signals it, and
    /// the worker learns to stop at its next poll (`shim.md` §7).
    deadline: Deadline,
    oracle: Oracle,
    tree: TreeFate,
}

/// Whether the tree a worker is producing still describes a document `core`
/// holds.
///
/// This is `core.md` §2's "text and tree can never disagree" across the window
/// the pool opens. A `didChange` or a `didOpen` forgets the cached tree for its
/// document, and while dispatch was in line that was the whole of the
/// protection — the tree came back on the event that built the seed, so
/// nothing could arrive in between. Now something can, and caching what the
/// worker returns would put a tree of the old text back under the new
/// document, where `TreeCache::seed` hands it to the next query as an
/// incremental base with an empty edit log.
///
/// A version comparison does not close it, which is why this is a flag and not
/// a number: §8.6 makes `didOpen` a *resync*, so the text behind a URI can be
/// replaced at a version we have already seen. That is what an editor does
/// after a revert.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum TreeFate {
    /// Nothing has touched the document since the seed left `core`.
    Cacheable,
    /// It has, so the tree is correct for text nobody holds any more.
    Superseded,
}

/// What the child has said about a query the pool has not answered yet.
///
/// `core` needs the distinction because dispatch now outlives the event that
/// started it, so the child's response can arrive **first** — which
/// `trace.rs`'s "the shim answers first and the child answers later" was
/// written before there was a pool to make untrue. Resolving on the child's
/// arrival then would compare its answer against one the handler has not
/// produced, record the query as one the shim declined, and leave §7's row
/// waiting for an oracle that has already been and gone.
#[derive(Debug)]
enum Oracle {
    /// Nothing yet, which is the ordinary case: the child is a process away
    /// and the worker is a thread.
    Awaited,
    /// The child beat the worker home, so its answer is held until there is
    /// something of ours to compare it with.
    Answered {
        result: DefinitionResult,
        latency: Micros,
    },
}

/// Whether [`Actor::run`]'s loop has anything left to do.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Running {
    Continue,
    Stopped,
}

/// `shim.md` §2's single-threaded actor.
#[derive(Debug)]
pub struct Actor {
    registry: Registry,
    /// Shared by refcount because every dispatched job holds one: a worker
    /// reads §1's `ServerProfile` off it, and the pool outlives the call that
    /// built the job.
    config: Arc<Config>,
    clock: Arc<dyn Clock>,
    documents: Documents,
    trees: TreeCache,
    pending: PendingQueries,
    traces: Traces,
    policy: Arc<CommitPolicy>,
    workers: Workers,
    /// Queries the pool has and `core` has not heard back about, which is what
    /// makes the drain in [`Actor::run`] terminate and what tells
    /// [`Actor::child_answered`] whether it is the second answer or the first.
    in_flight: Map<EditorRequestId, InFlight>,
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
        let workers = Workers::spawn(config.mode())?;
        Ok(Self {
            registry,
            config: Arc::new(config),
            clock,
            documents: Documents::new(),
            trees: TreeCache::default(),
            pending: PendingQueries::new(),
            traces,
            // `resolution.md` §7.1's permissive posture: nothing is gated on
            // confidence in v1, and the floor a `CommitPolicy` would carry is
            // what the corpus exists to derive.
            policy: Arc::new(CommitPolicy::permissive()),
            workers,
            in_flight: Map::default(),
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
            // Cloned for the same reason and at the same price: the arm needs
            // a borrow that ends before `answered` takes `&mut self`.
            let answers = self.workers.finished().clone();
            let arrived = match &rescans {
                Some(rescans) => select! {
                    recv(events) -> event => event.ok(),
                    recv(&answers) -> finished => match self.returned(finished) {
                        Running::Continue => continue,
                        Running::Stopped => return Ok(()),
                    },
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
                // No rescan channel because there is no root yet, and so
                // nothing for the timer arm to do either — `tick` is
                // `FileListCache`'s and there is no file list. It is still a
                // `select!` and not a `recv_timeout`, because the pool can
                // answer a query that was dispatched before a second
                // negotiation replaced the root.
                None => select! {
                    recv(events) -> event => event.ok(),
                    recv(&answers) -> finished => match self.returned(finished) {
                        Running::Continue => continue,
                        Running::Stopped => return Ok(()),
                    },
                    default(self.debounce.window()) => continue,
                },
            };
            let Some(event) = arrived else {
                tracing::debug!(
                    pending = self.pending.len(),
                    traced = self.traces.outstanding(),
                    dispatched = self.in_flight.len(),
                    "the event channel closed"
                );
                return self.drain(&answers);
            };
            // `deps.md` §2: the inbox is unbounded because a bounded one would
            // deadlock the transport rather than apply backpressure, so memory
            // here is bounded only by `shim.md` §10's shed-load rule and the
            // depth is "a number we should log and watch, not just assert
            // about". This is the watching, and it is the only caller of
            // `Receiver::len` — one of the two capabilities §1 names as its
            // reason for choosing crossbeam over the standard library's
            // channel.
            //
            // Read before the event is handled, so it is the queue as of
            // dispatch rather than as of the return; and logged only when it is
            // non-empty, because an empty inbox is the whole of normal
            // operation and a line per event would bury the one thing this
            // exists to surface — `core` falling behind what feeds it.
            let depth = events.len();
            if depth > 0 {
                tracing::debug!(depth, "core is behind its inbox");
            }
            self.handle(event)?;
        }
    }

    /// Where the pool's answers arrive, for the same reason
    /// [`FileListCache::rescans`] is reachable: `run` selects on it, and a test
    /// that drives [`Actor::handle`] without the loop has to take the other
    /// half of a query from somewhere.
    pub fn dispatches(&self) -> &Receiver<Finished> {
        self.workers.finished()
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
        // Copied out rather than held, so the borrow of `negotiated` ends here
        // and the arms below can take `&mut self`.
        let encoding = negotiated.encoding;
        let synced = match notification {
            // Three of the four invalidate the parse cache, and `didOpen` is
            // the one it would be easy to leave out. It is a *resync*
            // (`core.md` §8.6), so the text it carries need not be a
            // continuation of the one we held and its version need not be
            // larger — a document reopened at version 1 with a cached tree at
            // version 3 would otherwise leave `TreeCache::seed` handing out
            // that tree as an incremental base for text it was never parsed
            // from.
            DocumentNotification::DidOpen => {
                self.forget_trees(notification, params);
                self.documents.opened(params, &self.registry)
            }
            DocumentNotification::DidChange => {
                self.forget_trees(notification, params);
                self.documents.changed(params, encoding)
            }
            DocumentNotification::DidSave => {
                // The half that needs a read is not wired. There is a pool
                // now, but a `Job` *is* a query — a handler, a seed, a
                // position and an answer — and a checksum read is a second
                // kind of job with a second kind of reply, which nothing
                // builds. What has not changed is why it cannot happen here:
                // reading the file on this thread is the one thing `shim.md`
                // §2 forbids `core` outright. The free half — a `didSave` that
                // carried the text — is settled inside.
                match self.documents.saved(params) {
                    crate::documents::Saved::Checked(synced) => synced,
                    crate::documents::Saved::NeedsRead(check) => {
                        tracing::debug!(
                            uri = %check.uri(),
                            "a didSave checksum needs a read, and the pool takes queries only"
                        );
                        return;
                    }
                }
            }
            DocumentNotification::DidClose => {
                self.forget_trees(notification, params);
                self.documents.closed(params)
            }
        };
        tracing::trace!(%notification, ?synced, "a document notification was applied");
    }

    /// Every cached tree for the document a notification names.
    ///
    /// The document is read out on its own rather than taken from whatever the
    /// notification deserialized to, for the reason `Documents::unreadable`
    /// reads it that way: it is the one field that is still there when the
    /// modelling mistake is somewhere in `contentChanges`.
    fn forget_trees(&mut self, notification: DocumentNotification, params: &RawValue) {
        match serde_json::from_str::<NotifiedDocument>(params.get()) {
            Ok(named) => {
                self.trees.forget(&named.text_document.uri);
                // The cache is not the only place a tree of this document can
                // be: one is being parsed right now on every worker that has a
                // query against it, and those arrive *after* this.
                #[expect(
                    clippy::iter_over_hash_type,
                    reason = "every matching entry is marked and nothing is produced, so the order the map yields them in is not observable; the lint is about output that varies between runs"
                )]
                for in_flight in self.in_flight.values_mut() {
                    if in_flight.uri == named.text_document.uri {
                        in_flight.tree = TreeFate::Superseded;
                    }
                }
            }
            // Nothing said which document changed, so there is none to forget
            // — and nothing to leak either: `Documents` distrusts every open
            // document on this message (§8.6's `Unattributable`), a distrusted
            // document yields no `Trusted` and so no seed, and the `didOpen`
            // that restores trust comes back through here and forgets the
            // trees itself.
            Err(source) => tracing::warn!(
                %notification,
                %source,
                "a state-bearing message named no document, so no parse cache entry was dropped"
            ),
        }
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
        let handler = Arc::clone(handler);
        let grammar = handler.grammar();
        let asked = Asked {
            editor_id,
            uri,
            position,
            language: document.language_id(),
            arrived,
        };
        let seed = self
            .trees
            .seed(&OpenDocument::new(document, grammar, &self.edits));

        let encoding = negotiated.encoding;
        // The one walk that happens on this thread, and §4's rather than §2's:
        // the list is built lazily on first need and handed back by refcount
        // every time after (`files.rs`). What §2 keeps off this thread is the
        // per-query work below, which is unbounded where this is once.
        let project = match negotiated.files.view(deadline.clone(), handler.grammar()) {
            Ok(project) => project,
            // The file list could not be walked. It is the driver's failure
            // rather than the handler's, and §7 has one column for both: the
            // record says `failed` and names the class, because a stratum with
            // no coverage because the walk failed and one with no coverage
            // because resolution is hard must not be the same row.
            //
            // Nothing is dispatched, so this is the one answer `core` produces
            // itself — and it can, because a failure carries no locations and
            // so needs neither a parse nor §8.4's read.
            Err(error) => {
                let started = self.clock.now();
                self.settle(
                    &asked,
                    Dispatched::Failed(error),
                    micros(started.saturating_duration_since(arrived)),
                    Micros(0),
                );
                return Ok(());
            }
        };

        // Step 2's end and the whole of §2's split: what leaves this thread is
        // a `SnapshotSeed` — three refcount bumps and a struct move — and what
        // happens on the other side is the parse, the handler and §8.4's
        // conversion, none of which `core` is allowed to do.
        self.in_flight.insert(
            asked.editor_id.clone(),
            InFlight {
                uri: asked.uri.clone(),
                deadline: deadline.clone(),
                oracle: Oracle::Awaited,
                tree: TreeFate::Cacheable,
            },
        );
        let editor_id = asked.editor_id.clone();
        let dispatchable = self.workers.dispatch(Job::new(
            handler,
            seed,
            project,
            deadline,
            Arc::clone(&self.config),
            Arc::clone(&self.policy),
            Arc::clone(&self.clock),
            encoding,
            asked,
        ));
        if dispatchable == Dispatchable::Refused {
            // Logged inside `dispatch`. The records die with the query rather
            // than outliving it: nothing is going to answer, so a pending entry
            // would wait for a child response that resolves against nothing and
            // a trace row would wait forever.
            self.in_flight.remove(&editor_id);
            self.pending.cancelled(&editor_id);
            self.traces.dropped(&editor_id);
        }
        Ok(())
    }

    /// A query the pool has finished with: §2's `Parsed` goes into the cache,
    /// §4's evidence goes to the file list, and the answer goes where
    /// [`Actor::settle`] takes it.
    ///
    /// It is `pub` for the reason [`Actor::handle`] is: a test drives the state
    /// machine without the loop, and this is the half of a query that the loop
    /// — rather than the transport — delivers.
    pub fn finished(&mut self, finished: Finished) {
        let Finished {
            asked,
            completed,
            started,
            elapsed,
        } = finished;
        let Completed { dispatched, parsed } = completed;
        let Some(held) = self.in_flight.remove(&asked.editor_id) else {
            // Cancelled while the pool had it. `shim.md` §7 says the shim must
            // not answer a cancelled request, and §7's row is not written
            // either: a cancelled query has no `agreement` because nobody was
            // ever going to answer it.
            //
            // The tree goes with it rather than being kept for the next query.
            // The entry that was just not found is the only thing that knew
            // whether the document had moved since the seed was built, so
            // there is no longer any way to say this tree describes text
            // `core` holds — and a cold miss is correct, just slower
            // (`shim.md` §5).
            tracing::debug!("dropping an answer to a query that was cancelled");
            return;
        };
        match (parsed, held.tree) {
            // The parse was paid for whatever the query decided, and a query
            // that abstained on its deadline is the one most likely to be
            // asked again a moment later.
            (Some(parsed), TreeFate::Cacheable) => self.trees.insert(parsed),
            (Some(_), TreeFate::Superseded) => tracing::debug!(
                uri = %asked.uri,
                "dropping a tree of text the editor has replaced"
            ),
            (None, TreeFate::Cacheable | TreeFate::Superseded) => {}
        }
        if let Some(negotiated) = &mut self.negotiated {
            negotiated.files.observe(&dispatched);
        }
        self.settle(
            &asked,
            dispatched,
            micros(started.saturating_duration_since(asked.arrived)),
            micros(elapsed),
        );
        match held.oracle {
            // The child answered while the worker was still running, so this is
            // the moment both answers are known — which is when §7 says the row
            // is written and §6 says the comparison happens.
            Oracle::Answered { result, latency } => {
                self.child_answered(&asked.editor_id, &result, latency);
            }
            Oracle::Awaited => {}
        }
    }

    /// Steps 3 and 4's bookkeeping, from whichever side produced the answer:
    /// the editor hears about a commit, and §7's record is assembled and handed
    /// to `Traces` — completed in standalone, and waiting for the oracle when
    /// proxying.
    fn settle(&mut self, asked: &Asked, dispatched: Dispatched, queued: Micros, elapsed: Micros) {
        let answered = self.answer(&asked.editor_id, dispatched);
        let record = QueryRecord::new(
            &QueryContext {
                uri: &asked.uri,
                position: asked.position,
                language: asked.language,
                mode: self.config.mode().recorded(),
                // `shim.md` §6's health model is not built, so there is no
                // value to report. `null` is what §7 gives standalone for the
                // same reason: there is no server whose health this is.
                server_health: None,
                queued,
                elapsed,
            },
            answered,
        );
        match self.config.mode().recorded() {
            // No oracle is coming, so the row is complete (§7: in standalone
            // the four oracle columns are all null).
            shared::record::Mode::Standalone => self.traces.finished(record),
            shared::record::Mode::Proxy => {
                self.traces.awaiting_child(asked.editor_id.clone(), record)
            }
        }
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
            // The outcome was dropped, by the hard cap or by an expiry during
            // the conversion. The stratum it was asked under was not dropped
            // with it: `core-017` settles that the prior's rule "reads only the
            // query and the reference and never what the search found", so it
            // "was never the outcome's to carry away" — which is what keeps
            // §7's coverage denominator from moving by one query every time a
            // deadline expires.
            Dispatched::DeadlineExpired(classified) => Answered::of(Ok(Outcome::Abstain {
                reason: shared::AbstainReason::Deadline,
                strata: classified.strata(),
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
        if let Some(in_flight) = self.in_flight.get_mut(editor_id) {
            // The pool still has this query, so there is no answer of ours to
            // compare and no §7 row to complete. Held rather than resolved:
            // resolving here would classify the child's answer against a shim
            // that has not spoken, which §6 says nothing about and which the
            // record would spell as a query the shim declined.
            //
            // The newer answer wins if the child somehow answers twice under
            // one id, for the same reason `PendingQueries::record` keeps the
            // newer request: the older one describes a state nothing else
            // still holds.
            tracing::trace!("the child answered while the pool still had the query");
            in_flight.oracle = Oracle::Answered {
                result: result.clone(),
                latency,
            };
            return;
        }
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
    /// **It also signals `Deadline::cancel`**, which `shim.md` §7 asks for
    /// beside the drop and which was unreachable while dispatch was in line on
    /// this thread: a cancel was then only ever handled *between* queries, so
    /// there was never a handler running to signal. Now that a query outlives
    /// the event that started it there is, and the flag is what stops a worker
    /// spending the rest of a budget on an answer that will be discarded —
    /// `Deadline::expired` reads it beside the clock, so a handler polling
    /// cooperatively needs to know nothing about cancellation.
    ///
    /// The entry is removed rather than left for the worker to find, which is
    /// what makes `finished` able to tell a cancelled query from an answered
    /// one: it writes no row, because a cancelled query has no `agreement` and
    /// nobody was ever going to answer it.
    fn cancelled(&mut self, editor_id: &EditorRequestId) {
        if let Some(in_flight) = self.in_flight.remove(editor_id) {
            in_flight.deadline.cancel();
        }
        match self.pending.cancelled(editor_id) {
            Some(_) => tracing::debug!("a pending query was cancelled"),
            // Harmless and explicitly not an error: `shim.md` §7 says the shim
            // can receive a stale cancel for a request it already answered.
            None => tracing::trace!("a cancel for a query nothing is pending on"),
        }
        self.traces.dropped(editor_id);
    }

    /// One answer from the pool, and what the loop does next.
    ///
    /// `Err` is every worker gone, which needs all of them to have panicked —
    /// they hold their channels for as long as this actor does. The loop ends
    /// rather than continuing, because a `select!` arm on a disconnected
    /// channel is ready forever and there is nothing left that can answer a
    /// query. Degrading to `--proxy-only` instead (§11's permanent degraded
    /// mode) means a loop with the arm removed, which `select!` cannot express
    /// without the `Select` builder; it belongs with §10's other limits.
    fn returned(&mut self, finished: Result<Finished, RecvError>) -> Running {
        match finished {
            Ok(finished) => {
                self.finished(finished);
                Running::Continue
            }
            Err(RecvError) => {
                tracing::error!(
                    dispatched = self.in_flight.len(),
                    "every dispatch worker is gone; no query can be answered"
                );
                Running::Stopped
            }
        }
    }

    /// What the loop does after the wire closes: the pool may still hold
    /// queries, and §7's record for one of them is written when the worker
    /// comes back rather than when the request arrived.
    ///
    /// Nothing here can reach the editor — it is the editor that went away —
    /// so what the drain is for is the trace. A corpus run's rows are the whole
    /// output of the run, and the queries still outstanding when the wire
    /// closes are the slow ones, which are exactly the rows §7 is read for.
    ///
    /// The timeout is per answer rather than for the drain as a whole, so it
    /// fires only when the pool has gone silent for a whole budget with work
    /// outstanding. Every query is bounded by that budget, so silence for
    /// longer means a handler that is not polling its deadline — and waiting
    /// on it indefinitely would hang the process on exit.
    fn drain(&mut self, answers: &Receiver<Finished>) -> Result<(), Error> {
        while !self.in_flight.is_empty() {
            match answers.recv_timeout(self.config.deadline().budget()) {
                Ok(finished) => self.finished(finished),
                Err(_) => {
                    tracing::warn!(
                        dispatched = self.in_flight.len(),
                        "the pool did not answer every query before the shim exited"
                    );
                    return Ok(());
                }
            }
        }
        Ok(())
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
