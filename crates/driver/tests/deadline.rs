//! `design/core.md` §5's two mechanical claims about the cap: what it defaults
//! to, and that the driver drops a result that arrives after it.
//!
//! The cap is asserted against `hard_cap` rather than through `dispatch`,
//! because there is no handler double in phase 1a: `LanguageHandler::grammar`
//! returns a `tree_sitter::Language`, which cannot be constructed without a
//! grammar crate, and a `Query` needs a `DocumentSnapshot` that needs the same
//! grammar. The first end-to-end dispatch test arrives with `lang_rust`.

use std::ffi::OsString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use driver::{Config, DeadlineMs, DeadlineOverride, Dispatched, Heuristics, Mode, hard_cap};
use shared::{
    Clock, Confidence, Deadline, Error, HandlerError, Outcome, ParseError, Stratum, SystemClock,
};

/// The one clock a test may read, since `clippy.toml` bans `Instant::now` and
/// `disallowed_methods` does not honour `allow-*-in-tests`. Every instant
/// below is this one plus an offset, so nothing here races real time.
#[derive(Debug)]
struct FrozenClock(Instant);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

/// An answer, in the only shape phase 1a can build one: `Location::at_node`
/// needs a tree-sitter node, so the list is empty. What the cap does with a
/// `Decided` does not depend on what is inside it.
fn an_answer() -> Dispatched {
    Dispatched::Decided(Outcome::Committed {
        locations: Vec::new(),
        confidence: Confidence::ONE,
        stratum: Stratum::LocalBinding,
    })
}

#[test]
fn an_answer_that_arrives_after_the_deadline_is_dropped() {
    let arrived_at = SystemClock.now();
    let budget = DeadlineMs::PROXYING.budget();
    // The handler returned one millisecond too late. It polled cooperatively
    // or it did not; §5 says the driver does not have to know which.
    let clock = FrozenClock(arrived_at + budget + Duration::from_millis(1));
    let deadline = Deadline::new(Arc::new(clock), arrived_at, budget);

    match hard_cap(&deadline, an_answer()) {
        Dispatched::DeadlineExpired => {}
        other @ (Dispatched::Decided(_) | Dispatched::Failed(_)) => {
            panic!("a late answer reached the user as {other:?}")
        }
    }
}

#[test]
fn an_answer_that_arrives_in_time_is_kept() {
    let arrived_at = SystemClock.now();
    let budget = DeadlineMs::PROXYING.budget();
    // Subtracting the millisecond in the integer rather than from the
    // `Duration`: `Duration - Duration` underflows and panics, which
    // `unchecked_time_subtraction` denies workspace-wide.
    let just_inside = Duration::from_millis(DeadlineMs::PROXYING.get() - 1);
    let clock = FrozenClock(arrived_at + just_inside);
    let deadline = Deadline::new(Arc::new(clock), arrived_at, budget);

    match hard_cap(&deadline, an_answer()) {
        Dispatched::Decided(Outcome::Committed {
            locations: _,
            confidence: _,
            stratum: _,
        }) => {}
        other @ (Dispatched::Decided(Outcome::Abstain {
            reason: _,
            stratum: _,
        })
        | Dispatched::DeadlineExpired
        | Dispatched::Failed(_)) => panic!("an answer inside its deadline was dropped: {other:?}"),
    }
}

/// `$/cancelRequest`, and the query dying with the client that asked for it:
/// the flag expires the deadline independently of the clock, and the cap reads
/// `expired()` rather than the time.
#[test]
fn a_cancelled_query_drops_its_answer_before_the_clock_runs_out() {
    let arrived_at = SystemClock.now();
    let budget = DeadlineMs::STANDALONE.budget();
    let clock = FrozenClock(arrived_at);
    let deadline = Deadline::new(Arc::new(clock), arrived_at, budget);
    deadline.cancel();

    match hard_cap(&deadline, an_answer()) {
        Dispatched::DeadlineExpired => {}
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
    let arrived_at = SystemClock.now();
    let budget = DeadlineMs::PROXYING.budget();
    let clock = FrozenClock(arrived_at + budget);
    let deadline = Deadline::new(Arc::new(clock), arrived_at, budget);
    let broken = Dispatched::Failed(Error::Handler(HandlerError::DeadlineExpired));

    match hard_cap(&deadline, broken) {
        Dispatched::Failed(Error::Handler(HandlerError::DeadlineExpired)) => {}
        other @ (Dispatched::Failed(_) | Dispatched::Decided(_) | Dispatched::DeadlineExpired) => {
            panic!("a late failure was reclassified as {other:?}")
        }
    }

    let parse_failed = Dispatched::Failed(Error::Parse(ParseError::NoTree {
        uri: shared::DocumentUri::parse("file:///x.rs").expect("a literal file URI parses"),
    }));
    match hard_cap(&deadline, parse_failed) {
        Dispatched::Failed(Error::Parse(_)) => {}
        other @ (Dispatched::Failed(_) | Dispatched::Decided(_) | Dispatched::DeadlineExpired) => {
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
