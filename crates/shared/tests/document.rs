//! `design/core.md` §2: the parse happens inside the worker **and inside the
//! deadline**, and the tree a handler gets was produced from the text it was
//! given.
//!
//! The deadline half is the one nothing else would notice being wrong. A
//! `realise` that ignored its deadline would pass every other test in the
//! workspace — the fixtures are small and parse in microseconds — and would
//! only show up as a proxy that stops answering on somebody's large generated
//! file, which is the failure `high-level.md` puts a latency budget in front
//! of. So the document here is deliberately large enough for tree-sitter's
//! progress callback to fire, and one test asserts what happens when it does
//! not.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the builders below are free functions. Failing loudly is the point: a seed that will not parse makes every assertion here vacuous."
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use shared::{
    Clock, Deadline, DocumentSnapshot, DocumentUri, DocumentVersion, Error, HandlerError,
    LanguageId, Rope, SnapshotSeed, SystemClock,
};
use tree_sitter::{InputEdit, Language, Point};

const RUST: LanguageId = LanguageId::new("rust");

/// Small enough that tree-sitter finishes it inside one progress-check
/// interval and never calls the callback — which is the point of
/// `a_parse_too_small_to_report_progress_is_not_abandoned`.
const SMALL: &str = "fn caller() {\n    target();\n}\n";

/// The clock the deadline tests read, since `clippy.toml` bans `Instant::now`
/// and `disallowed_methods` does not honour `allow-*-in-tests`. Every instant
/// is `SystemClock`'s one reading plus an offset, so nothing here races real
/// time.
#[derive(Debug)]
struct FrozenClock(Instant);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

/// §2's central claim, and the reason `tree()` is infallible: there is one
/// tree and it was produced from `text`.
#[test]
fn the_tree_a_handler_gets_spans_the_text_it_was_given() {
    let document = realised(SMALL, &Deadline::none());

    assert_eq!(
        document.tree().root_node().end_byte(),
        document.text.len(),
        "core.md §2: the tree and the text cannot disagree, because the tree was \
         parsed from that text"
    );
}

/// The stale tree never leaves the seed: an incremental seed carries a tree
/// parsed from an *older* version plus the edits that reconcile it, and what
/// comes out spans the *new* text.
///
/// Handing a handler the v1 tree with the v2 text is the trap §2 is written
/// against — every offset in it is wrong for that text, and nothing detects it
/// until it produces a confidently wrong jump.
#[test]
fn an_incremental_reparse_produces_a_tree_for_the_new_text() {
    let before = realised(SMALL, &Deadline::none());
    let addition = "fn target() {}\n";
    let after = Rope::from(format!("{SMALL}{addition}").as_str());

    let edit = InputEdit {
        start_byte: SMALL.len(),
        old_end_byte: SMALL.len(),
        new_end_byte: SMALL.len() + addition.len(),
        start_position: Point::new(3, 0),
        old_end_position: Point::new(3, 0),
        new_end_position: Point::new(4, 0),
    };
    let document = SnapshotSeed::incremental(
        uri(),
        after.clone(),
        DocumentVersion(2),
        RUST,
        grammar(),
        before.tree().clone(),
        Arc::new(vec![edit]),
    )
    .realise(&Deadline::none())
    .expect("reparsing incrementally");

    assert_eq!(
        document.tree().root_node().end_byte(),
        after.len(),
        "the reparse returned a tree that stops where the *old* text stopped, so \
         every offset past the edit is wrong for the text beside it (core.md §2)"
    );
    assert_eq!(
        document.version,
        DocumentVersion(2),
        "the realised snapshot is not at the version its seed was built for"
    );
}

/// A parse that runs past its deadline is abandoned, and reported as the
/// **expiry** rather than as the document failing to parse.
///
/// The distinction is what §7's record is built to keep: a query that ran out
/// of time is an abstention and costs coverage, where a document that will not
/// parse is a failure and means something is broken. `driver::dispatch` maps
/// only the first back to `AbstainReason::Deadline`, so putting this in
/// `ParseError` would log every slow parse as a handler bug.
#[test]
fn a_parse_that_runs_past_its_deadline_is_abandoned_rather_than_failed() {
    let started = SystemClock.now();
    let budget = Duration::from_millis(20);
    // Already over, before the parse begins. §5's deadline is absolute and
    // starts at request arrival, so a query that queued for longer than its
    // budget arrives in exactly this state.
    let clock = FrozenClock(started + budget + Duration::from_millis(1));
    let deadline = Deadline::new(Arc::new(clock), started, budget);

    match seed(&large()).realise(&deadline) {
        Err(Error::Handler(HandlerError::DeadlineExpired)) => {}
        Err(other) => panic!(
            "an abandoned parse was reported as {other:?}, where core.md §1 has exactly \
             one error class mapped back to an abstention"
        ),
        Ok(document) => panic!(
            "a {} byte parse ran to completion under an expired deadline, so nothing \
             bounds it: core.md §2 pays the parse inside the deadline",
            document.text.len()
        ),
    }
}

/// `$/cancelRequest` and the client going away are not latency, and
/// `Deadline::expired` reports both — so the parse has to observe the flag as
/// well as the clock, or a cancelled query keeps a worker busy for as long as
/// the parse takes.
#[test]
fn a_cancelled_query_abandons_its_parse_too() {
    let deadline = Deadline::none();
    deadline.cancel();

    match seed(&large()).realise(&deadline) {
        Err(Error::Handler(HandlerError::DeadlineExpired)) => {}
        Err(other) => panic!("a cancelled parse was reported as {other:?}"),
        Ok(_) => panic!(
            "Deadline::none() is unbounded in *time* only: cancellation still has to \
             stop the parse (shared::Deadline)"
        ),
    }
}

/// The honest limit on the claim above, asserted rather than left as prose.
///
/// tree-sitter calls the progress callback once every 100 parser operations
/// (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `src/parser.c:81`), so a parse that
/// finishes inside one interval observes no deadline at all and returns a tree
/// however expired the query was. That is why `driver::hard_cap` still
/// exists behind this: the cap is what makes a late answer harmless, and the
/// callback is only what stops the *work*.
///
/// If this test starts failing, tree-sitter has become more eager and the
/// abstention is tighter than §2 promises — which is a better world, and a
/// deliberate decision to record rather than a regression to fix.
#[test]
fn a_parse_too_small_to_report_progress_is_not_abandoned() {
    let deadline = Deadline::none();
    deadline.cancel();

    let document = seed(SMALL)
        .realise(&deadline)
        .expect("a parse below the progress callback's granularity still finishes");

    assert_eq!(
        document.tree().root_node().end_byte(),
        document.text.len(),
        "the tree returned by an unbounded-in-practice parse is still a tree for its text"
    );
    assert!(
        SMALL.len() < 1024,
        "this test only says anything while the document is small enough that \
         tree-sitter never reports progress on it"
    );
}

/// Big enough that tree-sitter reports progress while parsing it, which is
/// what makes the deadline observable at all. Around 46 KB.
///
/// The granularity is 100 *parser operations* rather than a byte count
/// (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `tree-sitter/src/parser.c:81`), so
/// there is no size at which a document is guaranteed to be interruptible —
/// only sizes at which it reliably is, and this is one.
fn large() -> String {
    (0..800)
        .map(|index| {
            format!("fn generated_{index}(argument: u32) -> u32 {{ argument + {index} }}\n")
        })
        .collect()
}

fn realised(text: &str, deadline: &Deadline) -> DocumentSnapshot {
    seed(text)
        .realise(deadline)
        .expect("parsing a fixture document")
}

fn seed(text: &str) -> SnapshotSeed {
    SnapshotSeed::fresh(uri(), Rope::from(text), DocumentVersion(1), RUST, grammar())
}

fn uri() -> DocumentUri {
    DocumentUri::from_file_path(std::path::Path::new("/fixture/src/lib.rs"))
        .expect("a file URI for a fixture path")
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}
