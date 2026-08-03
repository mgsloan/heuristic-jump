//! `design/core.md` §5: the hard cap is enforced by the driver, not trusted to
//! the handler, and cancellation is cooperative — wrapping CPU-bound work in a
//! timeout does not stop the work, it only stops waiting for it, leaving a
//! thread burning CPU the proper LSP needs.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The one clock. `clippy.toml` bans `Instant::now` everywhere else, so that
/// `shim.md` §12's protocol-race tests can drive time rather than race it
/// (`deps.md` §12).
pub trait Clock: fmt::Debug + Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[expect(
        clippy::disallowed_methods,
        reason = "the single sanctioned Instant::now: this is what clippy.toml's replacement points at"
    )]
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Absolute, and started at request arrival rather than at handler entry —
/// queueing time counts, because the metric in `high-level.md` measures the
/// user's point of view.
///
/// Handlers must check `expired()` at every loop boundary: per candidate file,
/// per search result batch. The driver additionally hard-caps by dropping the
/// result of any handler that returns after the deadline, so a
/// non-cooperative handler produces a correctness-neutral waste of CPU rather
/// than a late answer.
#[derive(Clone, Debug)]
pub struct Deadline {
    budget: Budget,
    cancelled: Arc<AtomicBool>,
}

/// An explicit unbounded form, because `core.md` §7 makes "replay enforces no
/// deadline at all" a *requirement* rather than a default, and the obvious
/// implementation gets it wrong: a wall-clock deadline makes abstention depend
/// on machine load, so coverage — not just latency — becomes a property of
/// what else was running, and metrics that move with background load cannot be
/// compared across runs.
///
/// A variant rather than a far-future `Instant`, so a replay can assert on the
/// value rather than on a convention, and so nothing can arrive at "no
/// deadline" by arithmetic.
#[derive(Clone, Debug)]
enum Budget {
    Until {
        at: Instant,
        clock: Arc<dyn Clock>,
    },
    /// Sound because a search is exhaustive: it reads every candidate file and
    /// stops when it runs out of them (`resolution.md` §1.3), so with the
    /// clock removed there is nothing left that could vary.
    Unbounded,
}

impl Deadline {
    /// `core.md` §5 prints this type with two fields; the clock is the third
    /// because `expired()` has to read one and `Instant::now` is banned. The
    /// signature it prints — `expired(&self)` — is what carrying it preserves.
    pub fn new(clock: Arc<dyn Clock>, arrived_at: Instant, budget: Duration) -> Self {
        Self {
            budget: Budget::Until {
                // `Instant + Duration` panics on overflow. A budget too large
                // to represent becomes one that has already expired, which
                // costs coverage rather than correctness — the direction
                // `CLAUDE.md`'s performance posture asks for.
                at: arrived_at.checked_add(budget).unwrap_or(arrived_at),
                clock,
            },
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// No clock at all — `measure replay`'s, and nothing on the shim path may
    /// build one. Still cancellable, since `$/cancelRequest` and the client
    /// going away are not latency.
    pub fn none() -> Self {
        Self {
            budget: Budget::Unbounded,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn expired(&self) -> bool {
        // Relaxed: this flag carries no data, only the fact that somebody set
        // it, and a poll that misses it observes it at the next loop boundary.
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        match &self.budget {
            Budget::Until { at, clock } => clock.now() >= *at,
            Budget::Unbounded => false,
        }
    }

    /// `$/cancelRequest`, and the query dying with the client that asked for
    /// it. Independent of the clock, which is why `expired` checks both.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// `None` for [`Deadline::none`]. An `Option` rather than a sentinel so a
    /// caller has to say what it does when there is no instant, instead of
    /// comparing against one that is merely very far away.
    pub fn at(&self) -> Option<Instant> {
        match &self.budget {
            Budget::Until { at, .. } => Some(*at),
            Budget::Unbounded => None,
        }
    }
}
