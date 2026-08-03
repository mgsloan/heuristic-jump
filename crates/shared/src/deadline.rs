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
    at: Instant,
    cancelled: Arc<AtomicBool>,
    clock: Arc<dyn Clock>,
}

impl Deadline {
    /// `core.md` §5 prints this type with two fields; the clock is the third
    /// because `expired()` has to read one and `Instant::now` is banned. The
    /// signature it prints — `expired(&self)` — is what carrying it preserves.
    pub fn new(clock: Arc<dyn Clock>, arrived_at: Instant, budget: Duration) -> Self {
        Self {
            // `Instant + Duration` panics on overflow. A budget too large to
            // represent becomes one that has already expired, which costs
            // coverage rather than correctness — the direction `CLAUDE.md`'s
            // performance posture asks for.
            at: arrived_at.checked_add(budget).unwrap_or(arrived_at),
            cancelled: Arc::new(AtomicBool::new(false)),
            clock,
        }
    }

    pub fn expired(&self) -> bool {
        // Relaxed: this flag carries no data, only the fact that somebody set
        // it, and a poll that misses it observes it at the next loop boundary.
        self.cancelled.load(Ordering::Relaxed) || self.clock.now() >= self.at
    }

    /// `$/cancelRequest`, and the query dying with the client that asked for
    /// it. Independent of the clock, which is why `expired` checks both.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn at(&self) -> Instant {
        self.at
    }
}
