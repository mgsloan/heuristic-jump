//! `design/core.md` §4's file list: who owns it, and the two things that
//! invalidate it.
//!
//! §4 makes three claims and they are one mechanism. The list is built lazily
//! on first need and refreshed in the background; an exhaustive search that
//! found nothing schedules that refresh; and in proxy mode the editor's
//! watcher notifications, teed here by `shim.md` §3, schedule the same one.
//! "The same one" is the load-bearing word — the two triggers share a single
//! debounce rather than one each — so both are transitions on one [`Refresh`]
//! field and there is nowhere for a second timer to live.
//!
//! Everything on the query path is O(1) and infallible after the first build:
//! [`FileListCache::list`] hands back the `Arc` it holds whatever the refresh
//! state is, which is §4's "both invalidation paths are best-effort and
//! neither blocks a query. A query that arrives while a rescan is in flight
//! uses the list it has."
//!
//! What is missing is the same thing missing from [`crate::TreeCache`]: the
//! `core` loop that would own this and drive [`FileListCache::refresh_if_due`]
//! from `shim.md` §2's `select!` timer. The ownership is already right —
//! nothing here is shared and nothing is locked, the walk happens on a thread
//! that owns its own copy of the roots, and every mutator needs `&mut self`,
//! which only a single owner has.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{Builder, JoinHandle};
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use shared::{
    Clock, Deadline, Error, FileList, FileListEvidence, Language, Outcome, ProjectError,
    ProjectView,
};

use crate::config::DebounceMs;
use crate::dispatch::Dispatched;

/// The LSP notification `shim.md` §3 tees here. A constant rather than a
/// literal at the future routing site, because the string is the entire
/// coupling between that row and this module.
pub const DID_CHANGE_WATCHED_FILES: &str = "workspace/didChangeWatchedFiles";

/// Where the debounce is, and the only state either trigger touches.
///
/// Three states rather than a flag and a timestamp: "stale" and "a rescan is
/// already out" are different situations, and collapsing them is how a burst
/// of watcher frames turns into a walk per frame.
#[derive(Copy, Clone, Debug)]
enum Refresh {
    /// Nothing has invalidated the list since the last one was installed.
    Settled,
    /// A trigger fired at `since` and the debounce window is open. A second
    /// trigger inside it changes nothing, which is §4's "a burst of misses
    /// triggers at most one".
    Pending { since: Instant },
    /// A walk is out. A trigger arriving now is remembered rather than
    /// dropped: the walk started before the change, so it cannot have seen it.
    InFlight { pending: Option<Instant> },
}

/// A finished background walk, on its way back to the owner.
///
/// The field is private and only [`scan`] constructs one, so the only way to
/// hold one is to have received it from the scanner — the same shape that
/// keeps [`crate::Parsed`] from being minted by anything but a dispatch.
#[derive(Debug)]
pub struct Rescan {
    walked: Result<FileList, Error>,
}

/// The one thing a walk needs to be told, which is nothing: the scanner owns
/// its own copy of the roots, because they do not change without a
/// `workspace/didChangeWorkspaceFolders` that nothing here handles yet.
#[derive(Copy, Clone, Debug)]
struct Walk;

/// `core.md` §4's cached file list, and the owner the type has not had.
#[derive(Debug)]
pub struct FileListCache {
    roots: Vec<PathBuf>,
    /// `None` until the first query needs it — §4's "built lazily on first
    /// need". This is the one walk that happens on the calling thread; every
    /// later one is the scanner's.
    current: Option<Arc<FileList>>,
    refresh: Refresh,
    clock: Arc<dyn Clock>,
    debounce: DebounceMs,
    scanner: Scanner,
}

impl FileListCache {
    /// Spawns the background walker. Fallible only because thread creation is:
    /// a cache that could not spawn one would be a list that never refreshes,
    /// and §4's whole point is that it does.
    pub fn new(
        roots: Vec<PathBuf>,
        clock: Arc<dyn Clock>,
        debounce: DebounceMs,
    ) -> Result<Self, Error> {
        let scanner = Scanner::spawn(roots.clone())?;
        Ok(Self {
            roots,
            current: None,
            refresh: Refresh::Settled,
            clock,
            debounce,
            scanner,
        })
    }

    /// The list a query searches. Built here on first need and handed back by
    /// refcount every time after, including while a rescan is in flight —
    /// which is the one property §4 asks of this method.
    pub fn list(&mut self) -> Result<Arc<FileList>, Error> {
        match &self.current {
            Some(list) => Ok(Arc::clone(list)),
            None => {
                let list = Arc::new(FileList::enumerate(&self.roots)?);
                self.current = Some(Arc::clone(&list));
                Ok(list)
            }
        }
    }

    /// The view a query is dispatched against, and `driver`'s only route to
    /// one: a `ProjectView` built anywhere else in this crate would be a
    /// second file list, walked again and refreshed by nobody.
    ///
    /// The deadline and the grammar are per query and the list is not, which
    /// is the whole reason this takes two arguments and holds the third.
    pub fn view(&mut self, deadline: Deadline, grammar: Language) -> Result<ProjectView, Error> {
        Ok(ProjectView::new(self.list()?, deadline, grammar))
    }

    /// `shim.md` §3's tee of `workspace/didChangeWatchedFiles`.
    ///
    /// It takes no payload, which is how "`core` does not read the payload"
    /// stops being a rule somebody follows: one frame can carry thousands of
    /// events after a branch switch, and there is nothing here to read them
    /// with. Registration ids and glob patterns are not tracked either, for
    /// the same reason.
    pub fn watched_files_changed(&mut self) {
        self.mark_stale();
    }

    /// The other trigger: a query that searched exhaustively and found
    /// nothing. The triggering query still abstains — this returns nothing and
    /// changes nothing a query can observe, because a rescan cannot land
    /// inside the deadline that just expired.
    ///
    /// It takes what `dispatch` returned rather than an [`shared::AbstainReason`]
    /// picked out by the caller, so the classification cannot be done twice or
    /// done differently; and the reason-to-evidence half is
    /// `AbstainReason::file_list_evidence`, in `shared`, where a new variant
    /// fails to compile until somebody classifies it.
    pub fn observe(&mut self, dispatched: &Dispatched) {
        let reason = match dispatched {
            Dispatched::Decided(answer) => match answer.outcome() {
                Outcome::Abstain {
                    reason,
                    strata: _,
                    trace: _,
                } => reason,
                Outcome::Committed {
                    locations: _,
                    confidence: _,
                    strata: _,
                    trace: _,
                } => return,
            },
            // The search was cut off, which is evidence about nothing: it says
            // nothing about what a complete search would have found, and
            // rescanning would spend I/O in the window that just proved short
            // of it. Identical to `AbstainReason::Deadline`, which is what
            // this became on the way out.
            Dispatched::DeadlineExpired => return,
            // A failure is evidence about the handler.
            Dispatched::Failed(_) => return,
        };

        match reason.file_list_evidence() {
            FileListEvidence::Stale => self.mark_stale(),
            FileListEvidence::Inconclusive => {}
        }
    }

    /// One tick of `shim.md` §2's `select!` timer: sends the rescan if the
    /// debounce window has closed. O(1), and it never walks — the walk is the
    /// scanner's, which is what "in the background" means.
    pub fn refresh_if_due(&mut self) {
        let Refresh::Pending { since } = self.refresh else {
            return;
        };
        if self.clock.now().duration_since(since) < self.debounce.window() {
            return;
        }
        self.refresh = match self.scanner.request() {
            Requested::Sent => Refresh::InFlight { pending: None },
            // The scanner is gone or already has a walk queued. Staying
            // `Pending` means the next tick tries again, which is the
            // best-effort posture §4 asks for: a rescan that never happens
            // costs recall, and the on-demand trigger fires again anyway.
            Requested::Refused => Refresh::Pending { since },
        };
    }

    /// The receiver a `select!` waits on, beside the event channel and the
    /// timer. Exposed rather than polled internally because the owner is a
    /// single-threaded actor: it blocks in one place, and this has to be one
    /// of the arms.
    pub fn rescans(&self) -> &Receiver<Rescan> {
        &self.scanner.results
    }

    /// Installs what the scanner walked, superseding the list in hand.
    ///
    /// A failed walk is dropped rather than retried on the spot: the two
    /// triggers fire again on their own, and an immediate retry would turn an
    /// unreadable root into a spin.
    pub fn install(&mut self, rescan: Rescan) {
        let pending = match self.refresh {
            Refresh::InFlight { pending } => pending,
            // Nothing outstanding, so this arrived after the owner already
            // gave up on it. Installing it is still right — it is a newer walk
            // than the list in hand — but the debounce state is not this
            // method's to invent.
            Refresh::Settled | Refresh::Pending { since: _ } => None,
        };
        self.refresh = match pending {
            Some(since) => Refresh::Pending { since },
            None => Refresh::Settled,
        };

        match rescan.walked {
            Ok(walked) => {
                let generation = match &self.current {
                    Some(previous) => walked.superseding(previous),
                    // Nothing to supersede: the first need was a rescan rather
                    // than a query, so this walk starts the sequence.
                    None => walked,
                };
                self.current = Some(Arc::new(generation));
            }
            Err(error) => {
                tracing::warn!(%error, "a background rescan failed; keeping the list in hand");
            }
        }
    }

    /// O(1), and the only mutator both triggers share. Being one method is
    /// what makes "the two triggers share one debounce rather than one each"
    /// true by construction.
    fn mark_stale(&mut self) {
        let now = self.clock.now();
        self.refresh = match self.refresh {
            Refresh::Settled => Refresh::Pending { since: now },
            // Already inside a window. Not restarted: a burst would otherwise
            // push the rescan out indefinitely, which is the failure mode of
            // debouncing a signal that repeats.
            Refresh::Pending { since } => Refresh::Pending { since },
            Refresh::InFlight { pending: None } => Refresh::InFlight { pending: Some(now) },
            Refresh::InFlight {
                pending: Some(since),
            } => Refresh::InFlight {
                pending: Some(since),
            },
        };
    }
}

/// Whether a request reached the scanner. An enum rather than a `Result` with
/// nothing in it: the caller does not handle an error here, it chooses a state.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Requested {
    Sent,
    Refused,
}

/// The background half: one named thread, and the two channels it sits
/// between. Both are `bounded(1)` because at most one walk is ever
/// outstanding — `Refresh::InFlight` is what guarantees it — and an unbounded
/// channel here would hide the day that stops being true.
#[derive(Debug)]
struct Scanner {
    /// `Option` so [`Drop`] can close it, which is how the thread is asked to
    /// stop: the walk loop ends when its receiver disconnects.
    requests: Option<Sender<Walk>>,
    results: Receiver<Rescan>,
    thread: Option<JoinHandle<()>>,
}

impl Scanner {
    fn spawn(roots: Vec<PathBuf>) -> Result<Self, Error> {
        let (requests, walks) = bounded(1);
        let (finished, results) = bounded(1);
        let thread = Builder::new()
            .name("file-list-scanner".to_owned())
            .spawn(move || scan(&roots, &walks, &finished))
            .map_err(|source| Error::Project(ProjectError::Scanner { source }))?;
        Ok(Self {
            requests: Some(requests),
            results,
            thread: Some(thread),
        })
    }

    fn request(&self) -> Requested {
        let Some(requests) = &self.requests else {
            return Requested::Refused;
        };
        match requests.try_send(Walk) {
            Ok(()) => Requested::Sent,
            Err(TrySendError::Full(_)) => {
                tracing::debug!("a rescan is already queued");
                Requested::Refused
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!("the file-list scanner is gone; the list will not refresh again");
                Requested::Refused
            }
        }
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        // Closing the request channel first, so the join below is bounded by
        // one walk rather than by nothing.
        self.requests = None;
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the file-list scanner panicked");
        }
    }
}

/// The walk loop. It owns the roots and sends back whole lists, so no state is
/// shared with the owner and there is nothing to lock.
#[expect(
    clippy::disallowed_methods,
    reason = "`clippy.toml` bans a blocking `recv` on a thread that owes an answer inside the deadline. This thread owes none: nothing waits on it, a query that arrives mid-walk uses the list it has (`core.md` §4), and blocking is also how it stops — the request channel closing is the shutdown signal."
)]
fn scan(roots: &[PathBuf], walks: &Receiver<Walk>, finished: &Sender<Rescan>) {
    while let Ok(Walk) = walks.recv() {
        let walked = FileList::enumerate(roots);
        // Blocking, and bounded by the owner: `Refresh::InFlight` keeps at
        // most one walk outstanding, so the slot is free. If the owner has
        // gone away this returns an error rather than blocking, and there is
        // nothing left to do either way.
        if finished.send(Rescan { walked }).is_err() {
            return;
        }
    }
}
