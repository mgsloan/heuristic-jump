//! `shim.md` §10's dispatch pool: the threads a query is answered on, and the
//! two channels `core` reaches them through.
//!
//! It exists because three of `core.md`'s claims are about *which thread* does
//! the work, and none of them was true while `Actor::requested` called
//! [`dispatch`] in line:
//!
//! * §2: "`realise` ... called by the worker, never by `core`", and "the split
//!   is what keeps `core` doing O(1) work while the parse still happens inside
//!   the worker".
//! * §2 again: "the parse is paid inside the worker and inside the deadline,
//!   never in `core`; `core` builds seeds and never realises one".
//! * §8.4: "the conversion happens in the worker, not in `core`, because it
//!   **reads the target file** ... `core` may not do that — it does only O(1)
//!   state transitions and never touches the filesystem".
//!
//! The third is the one that had teeth. `Actor::notified` already refuses the
//! `didSave` checksum read for exactly that reason — "reading the file on this
//! thread is the one thing `shim.md` §2 forbids `core` outright" — while the
//! query path did a read per returned location on the same thread.
//!
//! **What is here is the pool and its sizing. §10's two additional limits are
//! applied before a job ever reaches this module** — the in-flight cap and the
//! shed-load rule are both refusals to *start* a query, and `core` is what
//! knows how many are in flight and how far behind its inbox it is, so both
//! live in [`crate::Actor::requested`].
//!
//! They were unbuildable until `core-026` was answered, and the obstacle was
//! vocabulary rather than effort: a refusal had nothing to say. There is no
//! [`shared::AbstainReason`] for a query nobody attempted, and adding one would
//! have put a variant no handler can ever return on the frozen seam. Option D
//! gives the refusal a disposition of its own instead — `core.md` §7's fourth
//! `decision` — so a shed query is recorded at the level where "it says
//! nothing" is true.

use std::sync::Arc;
use std::thread::{Builder, JoinHandle, available_parallelism};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use shared::proto::PositionEncoding;
use shared::{
    Clock, CommitPolicy, ConfigError, Deadline, DocumentUri, EditorRequestId, Error,
    LanguageHandler, LanguageId, Offset, ProjectView, SnapshotSeed,
};

use crate::config::{Config, Mode};
use crate::dispatch::{Completed, Request, dispatch};

/// One query's worth of work, owned rather than borrowed.
///
/// Everything `Request` takes by reference is held here by value or by `Arc`,
/// which is the whole difference between a dispatch that borrows `core`'s state
/// and one that outlives the event that started it. The `ProjectView` is
/// already owned — `FileListCache::view` returns one per query — and the
/// handler set, the configuration and the policy are shared by refcount, so a
/// job is a struct move and a handful of increments rather than a copy of
/// anything.
pub struct Job {
    handler: Arc<dyn LanguageHandler>,
    seed: SnapshotSeed,
    project: ProjectView,
    deadline: Deadline,
    config: Arc<Config>,
    policy: Arc<CommitPolicy>,
    clock: Arc<dyn Clock>,
    encoding: PositionEncoding,
    asked: Asked,
    /// The subscriber in force where the query was dispatched, carried so the
    /// worker's lines land in the same place `core`'s do.
    ///
    /// `deps.md` §9 installs one subscriber globally in the crate that owns the
    /// command line, and against a global default this is the identity: the
    /// worker would have found the same one. It is carried anyway because
    /// `tracing`'s default is *thread-local* first, so without it a line that
    /// moved from `core`'s thread to a worker's would quietly stop being
    /// collected — which is what happens to `classify`'s conversion line, the
    /// one `deps.md` §10 requires, the moment the parse it reports on moves to
    /// the pool.
    subscriber: tracing::Dispatch,
}

/// By hand, and only the query: a `Job` holds an `Arc<dyn LanguageHandler>`
/// and the seam does not require a handler to be `Debug` — the same reason
/// [`crate::Registry`] writes its own. What identifies a job is the query it
/// is, which is `Asked`.
impl std::fmt::Debug for Job {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Job")
            .field("asked", &self.asked)
            .finish_non_exhaustive()
    }
}

/// What `core` needs back to finish a query, carried out to the pool and
/// returned untouched.
///
/// A worker cannot reconstruct any of it — the document map, the mode and the
/// arrival instant are all `core`'s — and holding it in `core` instead would
/// mean a second table keyed by request id that says nothing the pool does not
/// already know.
#[derive(Clone, Debug)]
pub struct Asked {
    pub editor_id: EditorRequestId,
    pub uri: DocumentUri,
    pub position: Offset,
    pub language: LanguageId,
    /// When the *request* arrived, which is where §5's deadline starts.
    pub arrived: Instant,
}

/// A query the pool has finished with, whatever it decided.
#[derive(Debug)]
pub struct Finished {
    pub asked: Asked,
    pub completed: Completed,
    /// When the worker took the job off the channel. §5's `queued_us` is the
    /// distance from `asked.arrived` to here, and taking it on this side is
    /// what makes it the queueing the user actually waited through rather than
    /// the queueing `core` happened to notice.
    pub started: Instant,
    /// Dispatch to outcome: the parse, the handler and §8.4's conversion.
    pub elapsed: Duration,
}

impl Job {
    #[expect(
        clippy::too_many_arguments,
        reason = "every one of these is a distinct per-query fact `core` holds and a worker cannot reconstruct; a builder or a bag struct would move the same list somewhere the compiler checks less"
    )]
    pub fn new(
        handler: Arc<dyn LanguageHandler>,
        seed: SnapshotSeed,
        project: ProjectView,
        deadline: Deadline,
        config: Arc<Config>,
        policy: Arc<CommitPolicy>,
        clock: Arc<dyn Clock>,
        encoding: PositionEncoding,
        asked: Asked,
    ) -> Self {
        Self {
            handler,
            seed,
            project,
            deadline,
            config,
            policy,
            clock,
            encoding,
            asked,
            subscriber: tracing::dispatcher::get_default(Clone::clone),
        }
    }

    /// The whole of a worker thread's body, and the only place a `Request` is
    /// built: `dispatch` parses the seed, calls the handler and converts the
    /// answer onto the wire, all three on this thread.
    fn run(self) -> Finished {
        let Self {
            handler,
            seed,
            project,
            deadline,
            config,
            policy,
            clock,
            encoding,
            asked,
            subscriber,
        } = self;

        let started = clock.now();
        let _collecting = tracing::dispatcher::set_default(&subscriber);
        let completed = dispatch(
            handler.as_ref(),
            Request {
                seed,
                position: asked.position,
                project: &project,
                deadline: &deadline,
                server: config.server(),
                policy: &policy,
            },
            encoding,
        );
        Finished {
            asked,
            completed,
            started,
            elapsed: clock.now().saturating_duration_since(started),
        }
    }
}

/// The pool, from `core`'s side: somewhere to put a job and somewhere to read
/// the answers back.
///
/// `Debug` by hand because a `Job` holds an `Arc<dyn LanguageHandler>` and the
/// seam does not require a handler to be `Debug` — the same reason
/// [`crate::Registry`] writes its own.
pub struct Workers {
    work: Sender<Job>,
    finished: Receiver<Finished>,
    /// Held rather than detached so that `let_underscore_drop` has nothing to
    /// catch and so a thread's name outlives its spawn. Nothing joins them: the
    /// orderly shutdown is `Actor::run` draining what is still in flight before
    /// it returns, and a join after that would wait on threads that are already
    /// blocked on a channel `Workers`'s own drop closes.
    threads: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for Workers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Workers")
            .field("threads", &self.threads.len())
            .finish()
    }
}

impl Workers {
    /// §10's sizing, which is the reason the pool is bounded at all: "the
    /// entire justification for having no index is not competing with the
    /// proper LSP for CPU during its startup — and startup is exactly when the
    /// shim is busiest".
    pub fn spawn(mode: &Mode) -> Result<Self, Error> {
        let (work, jobs) = unbounded::<Job>();
        let (done, finished) = unbounded::<Finished>();
        let mut threads = Vec::new();
        let wanted = size(mode);
        for index in 0..wanted {
            let jobs = jobs.clone();
            let done = done.clone();
            let thread = Builder::new()
                .name(format!("dispatch-{index}"))
                .spawn(move || work_until_closed(&jobs, &done))
                .map_err(|source| ConfigError::PoolUnavailable {
                    threads: wanted,
                    source,
                })?;
            threads.push(thread);
        }
        Ok(Self {
            work,
            finished,
            threads,
        })
    }

    /// Hands a query to the pool. Failure is not propagated because there is
    /// nothing above this that could act on it: the receivers live as long as
    /// the threads, the threads live as long as `Workers`, and a send that
    /// fails means every worker died — at which point the query is lost and so
    /// is the next one.
    pub fn dispatch(&self, job: Job) -> Dispatchable {
        match self.work.send(job) {
            Ok(()) => Dispatchable::Accepted,
            Err(_) => {
                tracing::error!("the dispatch pool is gone; this query will not be answered");
                Dispatchable::Refused
            }
        }
    }

    /// The answers, for `Actor::run`'s `select!` and for the drain that
    /// follows it.
    pub fn finished(&self) -> &Receiver<Finished> {
        &self.finished
    }
}

/// Whether the pool took the job. `core` records a query as in flight only when
/// something is going to answer it, or the drain in `Actor::run` would wait for
/// an answer nobody is producing.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dispatchable {
    Accepted,
    Refused,
}

/// `max(1, available_parallelism() - 2)` when proxying, and
/// `available_parallelism()` standalone: "the `- 2` exists for a reason that
/// does not apply there, and keeping it would be cargo-culting".
///
/// A machine that will not report its parallelism gets one thread. That is the
/// same floor the proxying formula has, and a pool of one still keeps the parse
/// and the conversion off `core`'s thread, which is what every claim above is
/// about.
fn size(mode: &Mode) -> usize {
    let available = available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    match mode {
        Mode::Proxy {
            server: _,
            heuristics: _,
        } => available.saturating_sub(2).max(1),
        Mode::Standalone => available,
    }
}

/// One worker, until `core` goes away.
///
/// The blocking iteration is deliberate and is not what `clippy.toml`'s ban on
/// `Receiver::recv` is about: that ban protects a thread "that owes an answer",
/// and a worker waiting for work owes nobody one — it holds no query, no
/// deadline and no editor request. The bound the ban exists to preserve is on
/// the query, and the query's bound is `Deadline`, which is polled inside
/// `dispatch`. A `recv_timeout` loop here would wake every thread on a timer
/// forever to discover that there is still nothing to do.
fn work_until_closed(jobs: &Receiver<Job>, done: &Sender<Finished>) {
    for job in jobs {
        let finished = job.run();
        if done.send(finished).is_err() {
            // `core` is gone, so there is nobody to give this to and no reason
            // to take the next job either.
            tracing::debug!("nothing is reading what a worker answers");
            return;
        }
    }
}
