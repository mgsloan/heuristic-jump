//! `design/core.md` §5: the hard cap is enforced by the driver, not trusted to
//! the handler, and cancellation is cooperative — wrapping CPU-bound work in a
//! timeout does not stop the work, it only stops waiting for it, leaving a
//! thread burning CPU the proper LSP needs.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// The clock a test drives rather than reads. `deps.md` §12: "The injected
/// clock for `shim.md` §12's protocol race tests is a `trait Clock` with a
/// `TestClock` impl in `shared`, not a dependency."
///
/// It lives here rather than in each suite because five of them had written it
/// already — four copies of a frozen clock and one drivable one — and a test
/// double copied five times is one whose semantics differ in five places
/// without anyone comparing them.
///
/// **Not behind a feature**, which is the obvious alternative and costs more
/// than it saves. `#[cfg(test)]` is invisible to an integration test in another
/// crate, which is every caller here; a `test-support` feature would mean two
/// build configurations of `shared` and a self-referential dev-dependency, and
/// `CLAUDE.md` asks for the build matrix not to grow. What guards it instead is
/// `driver/tests/seam.rs`, which asserts no `src/` file outside this one names
/// it — a production clock that can be driven is the actual risk, and that is
/// the thing being checked.
///
/// The offset is an atomic rather than a cell because `Clock` is `Sync` and the
/// scanner thread that reads it holds nothing. There is no lock here and none
/// is wanted (`deps.md` §13 on `parking_lot`).
#[derive(Debug)]
pub struct TestClock {
    base: Instant,
    elapsed_nanos: AtomicU64,
}

impl TestClock {
    /// One reading of the real clock, through the one type sanctioned to take
    /// it, and every instant after this is that reading plus an offset a test
    /// chose — so a suite built on this races nothing.
    pub fn new() -> Self {
        Self {
            base: SystemClock.now(),
            elapsed_nanos: AtomicU64::new(0),
        }
    }

    /// Nanoseconds rather than the milliseconds the driven copy in
    /// `driver/tests/file_list.rs` used: `as_millis` truncates, so a test
    /// advancing by 500µs advanced by nothing and asserted on a clock that had
    /// not moved.
    pub fn advance(&self, by: Duration) {
        let nanoseconds = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
        self.elapsed_nanos.fetch_add(nanoseconds, Ordering::Relaxed);
    }
}

impl Default for TestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for TestClock {
    fn now(&self) -> Instant {
        let elapsed = Duration::from_nanos(self.elapsed_nanos.load(Ordering::Relaxed));
        // Saturating for the reason `Deadline::new` gives: `Instant + Duration`
        // panics on overflow, and a test that advanced past the representable
        // range wants a clock that stopped rather than a process that died.
        self.base.checked_add(elapsed).unwrap_or(self.base)
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
