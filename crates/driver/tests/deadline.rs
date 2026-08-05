//! `design/core.md` §5's two mechanical claims about the cap: what it defaults
//! to, and that the driver drops a result that arrives after it.
//!
//! The cap is asserted against `hard_cap` rather than through `dispatch`,
//! deliberately: what §5 claims is that a late answer is dropped, and driving
//! that through `dispatch` would mean building a fixture, a document and a
//! project view to reach one `expired()` check. `tests/wire_locations.rs` is
//! the end-to-end path.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and `an_answer` below is a free function. Failing loudly is the point: it asserts the one thing that makes an `Answer` buildable without a document."
)]

use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use driver::{
    Answer, Config, DeadlineMs, DeadlineOverride, Dispatched, Heuristics, Mode, Tracing, hard_cap,
};
use shared::{
    Clock, Confidence, Deadline, Error, HandlerError, Outcome, ParseError, Strata, Stratum,
    TestClock, Trace,
};

/// An answer with no locations, which is the only `Answer` reachable without
/// a document to encode against (`core.md` §8.4): the wire form of no
/// locations is no locations, so nothing here needs a rope or an encoding.
/// What the cap does with a `Decided` does not depend on what is inside it.
fn an_answer() -> Dispatched {
    let outcome = Outcome::Committed {
        locations: Vec::new(),
        confidence: Confidence::ONE,
        strata: Strata::from_reference(Stratum::LocalBinding),
        trace: Trace::new(),
    };
    Dispatched::Decided(
        Answer::without_locations(outcome).expect("a commit with no locations has no wire form"),
    )
}

#[test]
fn an_answer_that_arrives_after_the_deadline_is_dropped() {
    let clock = Arc::new(TestClock::new());
    let arrived_at = clock.now();
    let budget = DeadlineMs::PROXYING.budget();
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, arrived_at, budget);
    // The handler returned one millisecond too late. It polled cooperatively
    // or it did not; §5 says the driver does not have to know which. Advanced
    // *after* the deadline was built, which is the order the real path has.
    clock.advance(budget + Duration::from_millis(1));

    match hard_cap(&deadline, an_answer()) {
        Dispatched::DeadlineExpired(_) => {}
        other @ (Dispatched::Decided(_) | Dispatched::Failed(_)) => {
            panic!("a late answer reached the user as {other:?}")
        }
    }
}

#[test]
fn an_answer_that_arrives_in_time_is_kept() {
    let clock = Arc::new(TestClock::new());
    let arrived_at = clock.now();
    let budget = DeadlineMs::PROXYING.budget();
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, arrived_at, budget);
    // Subtracting the millisecond in the integer rather than from the
    // `Duration`: `Duration - Duration` underflows and panics, which
    // `unchecked_time_subtraction` denies workspace-wide.
    clock.advance(Duration::from_millis(DeadlineMs::PROXYING.get() - 1));

    match hard_cap(&deadline, an_answer()) {
        Dispatched::Decided(answer) => match answer.outcome() {
            Outcome::Committed {
                locations: _,
                confidence: _,
                strata: _,
                trace: _,
            } => {}
            other @ Outcome::Abstain {
                reason: _,
                strata: _,
                trace: _,
            } => panic!("a commit came back through the cap as {other:?}"),
        },
        other @ (Dispatched::DeadlineExpired(_) | Dispatched::Failed(_)) => {
            panic!("an answer inside its deadline was dropped: {other:?}")
        }
    }
}

/// `$/cancelRequest`, and the query dying with the client that asked for it:
/// the flag expires the deadline independently of the clock, and the cap reads
/// `expired()` rather than the time.
#[test]
fn a_cancelled_query_drops_its_answer_before_the_clock_runs_out() {
    let clock = Arc::new(TestClock::new());
    let arrived_at = clock.now();
    let budget = DeadlineMs::STANDALONE.budget();
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, arrived_at, budget);
    // The clock never moves: what expires this deadline is the flag alone.
    deadline.cancel();

    match hard_cap(&deadline, an_answer()) {
        Dispatched::DeadlineExpired(_) => {}
        other @ (Dispatched::Decided(_) | Dispatched::Failed(_)) => {
            panic!("a cancelled query still answered, with {other:?}")
        }
    }
}

/// The cap drops answers, not failures. A stratum with no coverage because the
/// handler is broken and one with no coverage because it was slow are
/// different rows in `core.md` §7's table.
#[test]
fn a_late_failure_is_still_recorded_as_a_failure() {
    let clock = Arc::new(TestClock::new());
    let arrived_at = clock.now();
    let budget = DeadlineMs::PROXYING.budget();
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, arrived_at, budget);
    clock.advance(budget);
    let broken = Dispatched::Failed(Error::Handler(HandlerError::expired_unclassified()));

    match hard_cap(&deadline, broken) {
        Dispatched::Failed(Error::Handler(HandlerError::DeadlineExpired { classified: _ })) => {}
        other @ (Dispatched::Failed(_)
        | Dispatched::Decided(_)
        | Dispatched::DeadlineExpired(_)) => {
            panic!("a late failure was reclassified as {other:?}")
        }
    }

    let parse_failed = Dispatched::Failed(Error::Parse(ParseError::NoTree {
        uri: shared::DocumentUri::parse("file:///x.rs").expect("a literal file URI parses"),
    }));
    match hard_cap(&deadline, parse_failed) {
        Dispatched::Failed(Error::Parse(_)) => {}
        other @ (Dispatched::Failed(_)
        | Dispatched::Decided(_)
        | Dispatched::DeadlineExpired(_)) => {
            panic!("a late failure was reclassified as {other:?}")
        }
    }
}

/// `core.md` §5's two numbers, and `deps.md` §11's rule that the presence of a
/// server command *is* the mode — so the default follows from the argv and
/// nothing else.
#[test]
fn the_default_cap_follows_the_mode() {
    let proxying = Config::new(
        Mode::from_server_argv(vec![OsString::from("rust-analyzer")], Heuristics::Enabled),
        DeadlineOverride::ModeDefault,
        Tracing::Off,
    );
    assert_eq!(proxying.mode().name(), "proxy");
    assert_eq!(
        proxying.deadline().get(),
        750,
        "core.md §5: 750ms proxying, high-level.md's number"
    );

    let standalone = Config::new(
        Mode::from_server_argv(Vec::new(), Heuristics::Enabled),
        DeadlineOverride::ModeDefault,
        Tracing::Off,
    );
    assert_eq!(standalone.mode(), &Mode::Standalone);
    assert_eq!(
        standalone.deadline().get(),
        2000,
        "core.md §5: 2000ms standalone, where an abstention costs the answer entirely"
    );
}

#[test]
fn deadline_ms_overrides_either_default() {
    for argv in [vec![OsString::from("pyright-langserver")], Vec::new()] {
        let config = Config::new(
            Mode::from_server_argv(argv, Heuristics::Enabled),
            DeadlineOverride::Explicit(DeadlineMs::new(37)),
            Tracing::Off,
        );
        assert_eq!(config.deadline().get(), 37, "in {}", config.mode().name());
        assert_eq!(config.deadline().budget(), Duration::from_millis(37));
    }
}

/// The child's argv reaches the child whole: `deps.md` §11's table has
/// `-- rust-analyzer --version -Ctarget-cpu=native` passing through verbatim,
/// and the split into program and arguments is what stops an empty command
/// from being representable.
#[test]
fn the_server_argv_survives_the_split() {
    let argv: Vec<OsString> = ["rust-analyzer", "--version", "-Ctarget-cpu=native"]
        .iter()
        .map(OsString::from)
        .collect();

    match Mode::from_server_argv(argv, Heuristics::Disabled) {
        Mode::Proxy { server, heuristics } => {
            assert_eq!(server.program(), "rust-analyzer");
            assert_eq!(server.arguments(), ["--version", "-Ctarget-cpu=native"]);
            assert_eq!(heuristics, Heuristics::Disabled);
        }
        Mode::Standalone => panic!("a server command was given, so this is not standalone"),
    }
}

/// `core.md` §7: **replay enforces no deadline at all**, and the section calls
/// this the constraint that makes replay worth having and easy to get wrong by
/// doing the obvious thing.
///
/// The obvious thing is a wall-clock deadline set very far out, which makes
/// abstention depend on machine load: the same handler on the same snapshot
/// gives up on a busy machine and finishes on an idle one, so *coverage* — not
/// just latency — becomes a property of what else was running, and a tuning
/// session cannot tell an improvement from a quiet minute.
///
/// `Deadline::none` is what makes that a value rather than a convention. The
/// test is in `driver` rather than `shared` for the same reason the seam tests
/// are: what is being asserted is that something is *not* reachable, and a
/// crate that holds no clock is where that shows.
#[test]
fn an_unbounded_deadline_never_expires_and_names_no_instant() {
    let deadline = Deadline::none();

    assert!(
        !deadline.expired(),
        "Deadline::none() expired, so a replay would abstain for a reason that \
         is not a fact about the code (core.md §7)"
    );
    assert_eq!(
        deadline.at(),
        None,
        "Deadline::none() named an instant, which is the far-future sentinel \
         core.md §7 rules out: coverage must not be a function of machine load"
    );

    // Still cancellable, because `$/cancelRequest` and the client going away
    // are not latency.
    deadline.cancel();
    assert!(
        deadline.expired(),
        "an unbounded deadline ignored a cancellation, which is not a clock \
         question (core.md §5)"
    );
}
