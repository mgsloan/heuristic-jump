//! The request path, end to end through `core`: `design/core.md` §5's
//! deadline, §6's pending-query record, and §7's trace record.
//!
//! Every one of these claims has a unit-level test already — `deadline.rs` for
//! the hard cap, `pending.rs` for the ranked list and the mismatch-only report
//! — and every one of them was true of code nothing called. What this suite
//! adds is the caller: the assertions here are made against `Actor`, driven by
//! the events `shim.md` §3's router will produce, so a claim that holds of a
//! type in isolation and not of the path a query actually takes fails here.
//!
//! **The trace is asserted as text.** `clippy.toml` denies
//! `serde_json::Value`, and a `Deserialize` derive would need `serde` as a
//! dev-dependency of `driver`, which `seam.rs` reads §9's graph out of. Both
//! are more than this needs: §7 fixes the record's field order and spelling, so
//! the record *is* its text, and a substring assertion fails on exactly the
//! changes that would matter.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builder, the handler doubles and the trace readers below are free functions and trait impls. Failing loudly is the point: a half-built fixture answers nothing, and an assertion about an absent trace row passes against a run that never wrote one."
)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::ThreadId;
use std::time::Duration;

use crossbeam_channel::Receiver;
use driver::{
    Actor, Config, DeadlineMs, DeadlineOverride, DebounceMs, Event, FileListCache, Finished,
    Heuristics, Mode, Outbound, Registry, Tracing,
};
use serde_json::value::RawValue;
use shared::proto::{DefinitionResult, PositionEncoding, WireLocation, WirePosition, WireRange};
use shared::{
    AbstainReason, Clock, Confidence, Deadline, DocumentNotification, DocumentUri, EditorRequestId,
    Error, FileExtension, LanguageHandler, LanguageId, Location, Micros, Outcome, ProjectError,
    ProjectPath, ProjectRoot, ProjectView, Query, RelPath, Strata, Stratum, TestClock, Trace,
};
use tree_sitter::Language;

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// The document the query arrives against: `fn target` on line 4 is one of the
/// two definition sites.
const DOCUMENT: &str = "fn caller() {\n    target();\n}\n\nfn target() {}\n";

/// The other, in a different file and on line 0.
const TARGET: &str = "fn target() {}\n";

const DEFINITION: &str = "fn target() {}";

/// What the same URI is reopened with: a different text, shorter than
/// `DOCUMENT`, and still with a line 1 long enough for the query's position.
const REOPENED: &str = "fn target() {}\n    // reopened\n";

/// Where in `DOCUMENT` the query is asked. Inside `target()` on line 1; no
/// handler double here reads it, but a position outside the document would be
/// refused before any of them ran.
const POSITION: u32 = 4;

/// `core.md` §5: "the deadline is absolute and starts at request arrival, not
/// at handler entry. Queueing time counts."
///
/// The contrast is the whole test. Both halves dispatch at the same instant on
/// the same clock and differ only in when the request *arrived*, so a deadline
/// measured from handler entry — the obvious implementation, and the one a
/// timeout wrapper gives you — would answer both.
#[test]
fn the_deadline_is_measured_from_arrival_and_not_from_dispatch() {
    let queued_inside = queued_by(Duration::from_millis(100));
    let queued_past_the_cap = queued_by(Duration::from_millis(800));

    assert!(
        queued_inside.answered,
        "a query queued for 100ms of a 750ms budget did not answer, so the assertion below \
         is not about the deadline"
    );
    assert!(
        !queued_past_the_cap.answered,
        "a query that arrived 800ms before it was dispatched still answered under a 750ms \
         cap, so the deadline is being measured from handler entry and the user's 800ms \
         wait is invisible to it"
    );
    assert_eq!(
        field(&queued_past_the_cap.row, "queued_us"),
        "800000",
        "the record does not report the queueing that expired the deadline, so §7's \
         \"a fast handler and an unexplained abstention\" is exactly what this row looks \
         like: {}",
        queued_past_the_cap.row
    );
    assert_eq!(
        field(&queued_past_the_cap.row, "decision"),
        "\"abstained\"",
        "a query the deadline cut off is recorded as something other than an abstention: {}",
        queued_past_the_cap.row
    );
    assert_eq!(
        field(&queued_inside.row, "queued_us"),
        "100000",
        "the queueing time is not reported as measured: {}",
        queued_inside.row
    );
}

/// §7: "each query emits one JSONL record once both answers are known".
///
/// Asserted as two facts about the same query, because either alone is
/// satisfiable by a mistake: a row written at the shim's answer would have the
/// four oracle columns null forever, and a row written twice would double-count
/// every proxied query in every metric.
#[test]
fn a_proxied_row_is_written_once_the_child_has_answered_and_not_before() {
    let fixture = Fixture::new("trace_waits_for_the_child", Proxying::Yes);
    let target = fixture.definition_in("src/target.rs");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: vec![target.clone()],
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);
    assert_eq!(
        fixture.rows(),
        Vec::<String>::new(),
        "a row was written before the child answered, so `agreement` is null for a query \
         that had an oracle and §7's precision numerator is missing it"
    );

    actor
        .handle(Event::ChildAnswered {
            editor_id: EditorRequestId::from_number(1),
            result: child_answer(&target, TARGET),
            latency: Micros(4_210_000),
        })
        .expect("the child's answer");

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one query, where §7 asks for one", rows.len());
    };
    assert_eq!(field(row, "mode"), "\"proxy\"", "{row}");
    assert_eq!(field(row, "decision"), "\"committed\"", "{row}");
    assert_eq!(field(row, "failure"), "null", "{row}");
    assert_eq!(field(row, "agreement"), "\"match_top1\"", "{row}");
    assert_eq!(field(row, "severity"), "null", "{row}");
    assert_eq!(field(row, "returned"), "1", "{row}");
    assert_eq!(
        field(row, "lsp_latency_us"),
        "4210000",
        "the oracle's latency did not reach the row, and `high-level.md`'s value weighting \
         is computed from it: {row}"
    );
}

/// §7: "`decision` has three values, not two ... `failure` names the `Error`
/// sub-enum that was converted".
///
/// The wire sees an abstention either way — a failure is not something a user
/// can act on — so the record is the only place the two are distinguishable,
/// and a driver that recorded the converted failure as an abstention would make
/// a broken handler indistinguishable from a hard stratum.
#[test]
fn a_failed_query_is_recorded_as_failed_and_names_its_class() {
    let fixture = Fixture::new("trace_failure", Proxying::No);
    let mut actor = fixture.actor(Arc::new(Failing));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one failed query", rows.len());
    };
    assert_eq!(
        field(row, "decision"),
        "\"failed\"",
        "a handler that returned `Err` was recorded as an abstention, so the per-stratum \
         table cannot tell a hard stratum from a broken handler: {row}"
    );
    assert_eq!(
        field(row, "failure"),
        "\"Project\"",
        "the failure does not name the `Error` sub-enum that was converted: {row}"
    );
    assert!(
        fixture.outbound().is_empty(),
        "a failed query answered the editor, where `shim.md` §11 serves it as an abstention"
    );
}

/// §7: in standalone "`server_health`, `lsp_latency_us`, `lsp_locations`,
/// `agreement`, and `severity` are all null, because there is no second answer
/// to compare against" — and the row is complete when the handler returns,
/// since nothing else is coming.
#[test]
fn a_standalone_row_is_complete_without_an_oracle() {
    let fixture = Fixture::new("trace_standalone", Proxying::No);
    let target = fixture.definition_in("src/target.rs");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: vec![target],
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one standalone query", rows.len());
    };
    assert_eq!(field(row, "mode"), "\"standalone\"", "{row}");
    for column in [
        "server_health",
        "lsp_latency_us",
        "lsp_locations",
        "agreement",
        "severity",
    ] {
        assert_eq!(
            field(row, column),
            "null",
            "{column} is populated in standalone, where there is no second answer for it to \
             be about: {row}"
        );
    }
}

/// §7's third decision, reached the way a handler reaches it rather than
/// through the hard cap: the query ran, found nothing it would commit to, and
/// said so.
///
/// The reason is asserted because §7 puts it in `stages` — "a second reason
/// column would be two vocabularies for one question" — so an abstention whose
/// reason went nowhere is a row that cannot be grouped by why it happened.
#[test]
fn an_abstention_is_recorded_with_its_reason_in_the_stages() {
    let fixture = Fixture::new("trace_abstention", Proxying::No);
    let mut actor = fixture.actor(Arc::new(Declining));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one abstained query", rows.len());
    };
    assert_eq!(field(row, "decision"), "\"abstained\"", "{row}");
    assert_eq!(
        field(row, "failure"),
        "null",
        "an abstention named a failure class, which is what §7 keeps `decision`'s three \
         values apart to prevent: {row}"
    );
    assert_eq!(
        field(row, "stages"),
        "[\"abstain:no_candidates\"]",
        "the abstention reason did not reach the record, so nothing downstream can group \
         the abstentions by why they happened: {row}"
    );
    assert_eq!(
        field(row, "returned"),
        "0",
        "an abstention returned locations: {row}"
    );
    assert!(
        fixture.outbound().is_empty(),
        "an abstention answered the editor, where `shim.md` §8 makes it silence"
    );
}

/// `core.md` §1: `Stratum::Unimplemented` is the unmodified template's, no real
/// handler may return it, and "its presence in a metrics table means the
/// template has not been replaced".
///
/// The hard cap is what made that false. §5 drops an answer that arrives after
/// its deadline, and the driver then had no stratum to record the row under, so
/// it synthesised the template's — which means a *real* handler that misses its
/// deadline writes an `unimplemented` **abstention**, and that is exactly the
/// counter `measure_core`'s `Table::template` reads as an unreplaced template.
/// `core-017` settles that nothing had to be synthesised: the prior's rule reads
/// only the query and the reference, so it "was never the outcome's to carry
/// away".
///
/// The two halves are asserted together because either alone passes on a
/// mistake: a row under the right stratum that was never capped is just a
/// commit, and a capped row under `unimplemented` is the bug.
#[test]
fn a_hard_capped_answer_keeps_the_stratum_the_handler_assigned() {
    let fixture = Fixture::new("actor_hard_cap", Proxying::No);
    // In the queried document, so §8.4's conversion needs no second read: a
    // read is what would expire *before* the cap, and this test is about the
    // cap.
    let target = fixture.definition_in("src/lib.rs");
    let mut actor = fixture.actor(Arc::new(Slow {
        clock: Arc::clone(&fixture.clock),
        locations: vec![target],
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one capped query", rows.len());
    };
    assert_eq!(
        field(row, "decision"),
        "\"abstained\"",
        "the late answer was not dropped, so the assertion below is not about the hard \
         cap: {row}"
    );
    assert_eq!(
        field(row, "stages"),
        "[\"abstain:deadline\"]",
        "a capped answer was recorded under some other abstention reason: {row}"
    );
    for column in ["stratum_prior", "stratum_final"] {
        assert_eq!(
            field(row, column),
            "\"explicitly_imported\"",
            "{column} is the template's stratum for a query a real handler classified, so \
             `Table::template` reads this row as an unreplaced template and §7's coverage \
             denominator lost a query from the class it was really asked about: {row}"
        );
    }
    assert!(
        fixture.outbound().is_empty(),
        "the capped answer reached the editor, which is the whole of what §5's hard cap \
         exists to prevent"
    );
}

/// The same claim on the other path that discards an answer, which is the one
/// easy to miss: §8.4's conversion reads the target file, and `ProjectView`
/// fails a read whose deadline has already expired — so a handler that answers
/// late with a definition in *another* file never reaches the hard cap at all.
/// It surfaces as `HandlerError::DeadlineExpired` from inside the conversion,
/// where the outcome has already been consumed.
///
/// The only difference from the test above is which file the definition is in.
#[test]
fn a_conversion_that_expires_keeps_the_stratum_too() {
    let fixture = Fixture::new("actor_expired_conversion", Proxying::No);
    let target = fixture.definition_in("src/target.rs");
    let mut actor = fixture.actor(Arc::new(Slow {
        clock: Arc::clone(&fixture.clock),
        locations: vec![target],
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one expired query", rows.len());
    };
    assert_eq!(field(row, "decision"), "\"abstained\"", "{row}");
    assert_eq!(
        field(row, "stratum_prior"),
        "\"explicitly_imported\"",
        "the classification a handler had already made was lost because the query ended \
         downstream of it rather than at the cap, which is the same query counted under the \
         template's stratum by a different route: {row}"
    );
}

/// `deps.md` §10: "Some `driver` code will convert an `Error` into an
/// abstention; that conversion is explicit and logged."
///
/// The deadline is the one class mapped back that way. `ProjectView` fails a
/// read whose deadline has already expired, so a handler doing the ordinary `?`
/// propagation `CLAUDE.md` asks for surfaces an expiry as `Err` — and §10 keeps
/// `Result` off the abstention path, so something has to convert it. `classify`
/// is that something, and it did it silently: a query the clock ended and a
/// handler that declined on its own left the same trace, which is the
/// distinction the whole of §7's `decision` column exists to preserve.
///
/// **`classify` is reached three ways and the claim is about all of them.** Its
/// own comment says so — "logging here covers all three callers: a parse
/// abandoned in `realise`, a handler's own `?` propagation through `call`, and
/// an expiry raised inside §8.4's conversion" — and a comment describing
/// behaviour is not a mechanism for it. The three fixtures below take one route
/// each. They are not three spellings of one test: the routes differ in what has
/// already happened when the `Error` appears, which is why `Classified` has two
/// variants, and a conversion that logged only where an outcome already existed
/// would leave the two silent ones looking exactly like a handler that declined.
///
/// The fourth fixture is what makes this a test of the *conversion* rather than
/// of expiry in general. It expires too, and differs from the third only in
/// which file the definition is in: a same-file answer takes `target_text`'s
/// free path, so no read happens, no `Error` is ever built, and nothing is
/// converted — it is `hard_cap` that drops it, under its own line. A conversion
/// line on that path would mean the message is attached to lateness rather than
/// to the conversion, and §10's claim would be unbacked with this test still
/// green.
///
/// The counts are exact in both directions for the same reason. `Actor::answer`
/// builds the `Outcome::Abstain` these all end as and deliberately logs nothing,
/// because every expiry that reaches it has been reported once already; a second
/// line there would put one query in the log twice and make the rate §7 reports
/// unreadable from the log it is supposed to explain.
#[test]
fn converting_an_expiry_into_an_abstention_is_logged() {
    let routes = [
        (
            "a parse abandoned in `realise`, before any handler ran",
            expired_before_the_parse("actor_expiry_parse"),
        ),
        (
            "a handler's own `?` propagation through `call`",
            propagated_from_a_read("actor_expiry_propagated"),
        ),
        (
            "an expiry raised inside §8.4's conversion",
            expiring_in("src/target.rs", "actor_expiry_converted"),
        ),
    ];

    for (route, lines) in &routes {
        assert_eq!(
            lines_saying(lines, CONVERSION).len(),
            1,
            "{route} converted an `Error` into an abstention without §10's one line. \
             `classify` claims to cover every caller, and a route that reaches it silently \
             is a query the clock ended wearing the trace of a handler that declined: \
             {lines:?}"
        );
        assert_eq!(
            lines_saying(lines, HARD_CAP).len(),
            0,
            "{route} reported a dropped late answer as well as a conversion, so one query \
             is in the log twice and neither line can be counted: {lines:?}"
        );
    }

    let merely_late = expiring_in("src/lib.rs", "actor_expiry_capped");
    assert_eq!(
        lines_saying(&merely_late, HARD_CAP).len(),
        1,
        "the same-file fixture was not dropped as a late answer, so the assertion below \
         is about a query that never expired: {merely_late:?}"
    );
    assert_eq!(
        lines_saying(&merely_late, CONVERSION).len(),
        0,
        "a query whose answer needed no read reported a conversion, so the line is about \
         the deadline rather than about the `Error` there was none of: {merely_late:?}"
    );
}

/// `core.md` §2: "text and tree can never disagree", which the parse cache is
/// the one thing that can break — and `core.md` §8.6 makes `didOpen` a resync
/// rather than a continuation, so the document behind a URI can be replaced by
/// a shorter one at a version we have already seen.
///
/// A cache that kept its entry across that resync would hand `TreeCache::seed`
/// a tree of the *old* text as an incremental base with no edits, and
/// tree-sitter — told nothing changed — would hand the handler back that same
/// tree for text it was never parsed from. Nothing else would notice: the
/// answer would be a location in a document that no longer says what the tree
/// says it does.
#[test]
fn a_reopened_document_is_not_parsed_from_the_tree_of_the_old_one() {
    let fixture = Fixture::new("actor_reopen", Proxying::No);
    let (parsed, seen) = crossbeam_channel::unbounded();
    let mut actor = fixture.actor(Arc::new(Checking { parsed }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);
    actor
        .handle(fixture.did_open(REOPENED))
        .expect("the document reopened with a different text");
    fixture.request(&mut actor, 2);

    let spans: Vec<(usize, usize)> = seen.try_iter().collect();
    assert_eq!(
        spans,
        vec![
            (DOCUMENT.len(), DOCUMENT.len()),
            (REOPENED.len(), REOPENED.len()),
        ],
        "a handler was given a tree that does not span its own text: the second query's \
         tree is the first document's, reused as an incremental base for a text it was \
         never parsed from"
    );
}

/// The loop rather than the state machine: the same query, delivered over the
/// channel `shim.md` §2's reader thread will send on.
///
/// Every other test here calls `handle` directly, which leaves `run`'s
/// `select!` — the part `driver::run` actually drives — asserted by nothing.
/// What this pins is that events are taken in order and that the loop ends when
/// the wire closes, which is the whole of what `driver::run` does today.
#[test]
fn the_loop_drains_its_channel_and_ends_when_the_wire_closes() {
    let fixture = Fixture::new("actor_loop", Proxying::Yes);
    let target = fixture.definition_in("src/target.rs");
    let actor = fixture.actor(Arc::new(Committing {
        locations: vec![target.clone()],
    }));

    let (events, incoming) = crossbeam_channel::unbounded();
    for event in fixture.session(1) {
        events.send(event).expect("a queued event");
    }
    events
        .send(Event::ChildAnswered {
            editor_id: EditorRequestId::from_number(1),
            result: child_answer(&target, TARGET),
            latency: Micros(1_000),
        })
        .expect("a queued event");
    // The transport going away, which is the only thing that ends the loop.
    drop(events);

    actor.run(&incoming).expect("the actor loop");

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!(
            "{} rows from a query delivered over the channel, where one arrived",
            rows.len()
        );
    };
    assert_eq!(field(row, "agreement"), "\"match_top1\"", "{row}");
}

/// `deps.md` §2: the inbox is `unbounded()` because a bounded one would
/// deadlock the transport rather than apply backpressure, so "memory is bounded
/// only by the shed-load rule in `shim.md` §10, [and] the `core` inbox length is
/// a number we should log and watch, not just assert about".
///
/// Four events are queued before the loop starts, so the depth the loop reports
/// counts *down* — which is what makes this an assertion about `Receiver::len`
/// rather than about a constant somebody wrote next to the word `depth`. The
/// fourth event leaves the inbox empty and must log nothing: an empty inbox is
/// the whole of normal operation, and a depth line per event would bury the one
/// case the number exists for.
#[test]
fn the_inbox_depth_is_logged_when_core_falls_behind() {
    let fixture = Fixture::new("actor_depth", Proxying::Yes);
    let target = fixture.definition_in("src/target.rs");
    let actor = fixture.actor(Arc::new(Committing {
        locations: vec![target.clone()],
    }));

    let (events, incoming) = crossbeam_channel::unbounded();
    for event in fixture.session(1) {
        events.send(event).expect("a queued event");
    }
    events
        .send(Event::ChildAnswered {
            editor_id: EditorRequestId::from_number(1),
            result: child_answer(&target, TARGET),
            latency: Micros(1_000),
        })
        .expect("a queued event");
    drop(events);

    let (logged, lines) = crossbeam_channel::unbounded();
    tracing::subscriber::with_default(Capturing { events: logged }, || {
        actor.run(&incoming).expect("the actor loop");
    });

    let depths: Vec<u64> = lines
        .try_iter()
        .filter_map(|line| depth_of(&line))
        .collect();
    assert_eq!(
        depths,
        vec![3, 2, 1],
        "the loop reported {depths:?} for four events queued ahead of it. §2 withdrew the \
         unbounded lint and left this as the only mechanism, so a `core` that falls behind its \
         transport is visible here or nowhere"
    );
}

/// §6: "divergence is reported to the user on `mismatch` only", through the
/// path that produces the report rather than through `Divergence` in isolation.
///
/// The two queries differ only in the shim's ranked list, and the `contained`
/// one is the interesting half: a report on `match_top1` would be absurd and
/// somebody would notice, where a report on a contained match looks like
/// diligence and trains the user to ignore the reports that matter.
#[test]
fn the_actor_reports_a_mismatch_and_stays_quiet_on_a_contained_match() {
    let fixture = Fixture::new("actor_reports", Proxying::Yes);
    let elsewhere = fixture.definition_in("src/target.rs");
    let correct = fixture.definition_in("src/lib.rs");

    let mismatched = fixture.resolve(1, vec![elsewhere.clone()], child_answer(&correct, DOCUMENT));
    let contained = fixture.resolve(
        2,
        vec![elsewhere, correct.clone()],
        child_answer(&correct, DOCUMENT),
    );

    assert_eq!(
        reports(&mismatched).len(),
        1,
        "the shim sent the user somewhere the child did not agree with and reported \
         nothing, and with no precision floor that report is the only protection they have"
    );
    assert!(
        reports(&contained).is_empty(),
        "a contained match was reported as a divergence: the user was shown the correct \
         location, so telling them they were misled would be false"
    );
}

/// §6, the same claim from the metric's side: `agreement` is `match_contained`
/// rather than `match_top1` when the child's location is the shim's *second*.
///
/// It is here and not only in `pending.rs` because the record is assembled from
/// a `Resolution` the actor takes and a list the dispatch wrapper built, and an
/// actor that handed the predicate its own copy of the locations could sort it
/// without `pending.rs` noticing.
#[test]
fn the_recorded_agreement_is_the_ranked_one() {
    let fixture = Fixture::new("actor_rank", Proxying::Yes);
    let elsewhere = fixture.definition_in("src/target.rs");
    let correct = fixture.definition_in("src/lib.rs");

    fixture.resolve(
        1,
        vec![elsewhere.clone(), correct.clone()],
        child_answer(&correct, DOCUMENT),
    );
    fixture.resolve(
        2,
        vec![correct.clone(), elsewhere],
        child_answer(&correct, DOCUMENT),
    );

    let rows = fixture.rows();
    let [contained, top1] = rows.as_slice() else {
        panic!("{} rows for two resolved queries", rows.len());
    };
    assert_eq!(
        field(contained, "agreement"),
        "\"match_contained\"",
        "the child's location was the shim's second and the row says otherwise, so the \
         number that gets optimised is measuring a set: {contained}"
    );
    assert_eq!(
        field(top1, "agreement"),
        "\"match_top1\"",
        "the child's location was the shim's first and the row says otherwise: {top1}"
    );
}

/// `shim.md` §7: `$/cancelRequest` drops the pending query, and the shim must
/// not answer a cancelled request — so there is no row either, because a row
/// with a null oracle half is what §7 reserves for a query that never had one.
#[test]
fn a_cancelled_query_leaves_no_row_and_no_report() {
    let fixture = Fixture::new("actor_cancel", Proxying::Yes);
    let elsewhere = fixture.definition_in("src/target.rs");
    let correct = fixture.definition_in("src/lib.rs");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: vec![elsewhere],
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);
    actor
        .handle(Event::Cancelled {
            editor_id: EditorRequestId::from_number(1),
        })
        .expect("a cancellation");
    // The child answers anyway, which is the case `shim.md` §7 calls harmless:
    // the request was forwarded before the cancel and the response is in
    // flight.
    actor
        .handle(Event::ChildAnswered {
            editor_id: EditorRequestId::from_number(1),
            result: child_answer(&correct, DOCUMENT),
            latency: Micros(1),
        })
        .expect("a child response for a cancelled query");

    assert!(
        reports(&fixture.outbound()).is_empty(),
        "a cancelled query produced a divergence report, which tells the user they were \
         misled by an answer they were never shown"
    );
    assert_eq!(
        fixture.rows().len(),
        0,
        "a cancelled query wrote a metric row, and nothing downstream can tell it apart \
         from a query nobody answered"
    );
}

/// `core.md` §1: "the driver resolves an incoming LSP `languageId` against the
/// registry and gets `Option<LanguageId>`. Unknown languages fail to resolve at
/// the boundary rather than travelling inward as a string that matches
/// nothing".
///
/// Both halves of the registry are asserted because they have different
/// vocabularies and only one of them is exercised by anything else here: a
/// `languageId` arrives on an open document, and a *file extension* arrives on
/// "candidate files found by search [where] closed files arrive as a bare path
/// with no languageId attached". The second lookup had no caller anywhere in the
/// workspace, which is the state in which a boundary quietly stops being one.
///
/// The negative cases are the test. A registry that resolved everything would
/// pass every positive assertion above, and the string that matches nothing is
/// exactly what §1 says must not travel inward.
#[test]
fn the_registry_resolves_only_what_a_handler_declared() {
    let registry = Registry::new(vec![Arc::new(Declining) as Arc<dyn LanguageHandler>]);

    assert!(
        registry.for_language_id("rust").is_some(),
        "the one declared languageId resolves to nothing, so every assertion below is \
         vacuous"
    );
    assert!(
        registry.for_language_id("ruby").is_none(),
        "an undeclared languageId resolved to a handler, which is a document parsed with \
         somebody else's grammar"
    );
    assert_eq!(
        registry.language_id("rust").map(LanguageId::as_str),
        Some("rust"),
        "the interning lookup does not return the id the handler declared, and it is the \
         only way to obtain a `LanguageId` — so `Documents` could not track the document \
         at all"
    );
    assert!(
        registry.language_id("ruby").is_none(),
        "an undeclared languageId was interned, which is §1's string that matches nothing \
         travelling inward with a newtype on"
    );

    assert!(
        registry.for_path(Path::new("src/lib.rs")).is_some(),
        "a bare path with a declared extension resolves to no handler, so a closed file \
         found by search can never be attributed to the language that owns it"
    );
    assert!(
        registry.for_path(Path::new("src/lib.py")).is_none(),
        "a path whose extension nobody declared resolved to a handler"
    );
    assert!(
        registry.for_path(Path::new("Makefile")).is_none(),
        "a path with no extension at all resolved to a handler: the lookup is reading \
         something other than the extension"
    );
}

/// `core.md` §2 and §8.4, from the one side a test can observe: the thread.
///
/// Three of the document's claims are the same claim — §2's "`realise` ...
/// called by the worker, never by `core`", §2's "the parse is paid inside the
/// worker and inside the deadline, never in `core`; `core` builds seeds and
/// never realises one", and §8.4's "the conversion happens in the worker, not
/// in `core`, because it **reads the target file** ... `core` may not do that".
/// All three are about which thread does the work, and all three were false
/// while `Actor::requested` called `dispatch` in line.
///
/// The definition the handler commits is in a **different file** from the one
/// queried, which is what makes this a test of §8.4 rather than only of §1:
/// `target_text` takes its reading path rather than the free same-document one,
/// so the answer on the wire is a witness that a file was read — and the
/// assertion below says the thread that read it was not this one.
///
/// What ties the parse and the conversion to the handler's thread is that
/// `dispatch` is a single function doing all three, and that `workers.rs` is
/// its only caller in `driver`. That half is `seam.rs`'s, because it is a claim
/// about the source rather than about a run.
#[test]
fn the_parse_and_the_conversion_never_run_on_the_thread_that_owns_the_state() {
    let fixture = Fixture::new("actor_worker_thread", Proxying::No);
    let target = fixture.definition_in("src/target.rs");
    let (ran_on, threads) = crossbeam_channel::unbounded();
    let mut actor = fixture.actor(Arc::new(Reporting {
        locations: vec![target],
        ran_on,
    }));

    fixture.open(&mut actor);
    fixture.request(&mut actor, 1);

    let seen: Vec<ThreadId> = threads.try_iter().collect();
    let [ran_on] = seen.as_slice() else {
        panic!("{} handler runs for one query", seen.len());
    };
    assert_ne!(
        *ran_on,
        std::thread::current().id(),
        "the handler ran on the thread that handed it the event, so the parse in front of it \
         and §8.4's target-file read behind it were both paid on `core` — which does only O(1) \
         state transitions and never touches the filesystem"
    );

    let outbound = fixture.outbound();
    let [
        Outbound::Definition {
            editor_id: _,
            locations,
        },
    ] = outbound.as_slice()
    else {
        panic!(
            "{} messages for one committed query, where §8.4's conversion produces exactly one",
            outbound.len()
        );
    };
    assert_eq!(
        locations.len(),
        1,
        "the committed location did not survive the conversion, so the assertion above is \
         about a query whose target file was never read"
    );
}

/// The race the pool introduces, which nothing could produce before it: the
/// child answers while a worker is still running.
///
/// `trace.rs` says "the shim answers first and the child answers later", and
/// that was a property of dispatch being in line rather than a fact about
/// either party — the child is a whole process away, but a handler that reads
/// candidate files is not fast. Resolving on the child's arrival would compare
/// its answer against a shim that has not spoken, record the query as one the
/// shim declined, and leave §7's row waiting for an oracle that has already
/// been and gone.
///
/// The two events are delivered in the losing order deliberately. It is the
/// same query, the same handler and the same child answer as
/// `a_proxied_row_is_written_once_the_child_has_answered_and_not_before` —
/// only the order differs, so a driver that got one right and the other wrong
/// fails exactly here.
#[test]
fn the_child_may_answer_before_the_worker_does() {
    let fixture = Fixture::new("actor_child_first", Proxying::Yes);
    let target = fixture.definition_in("src/target.rs");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: vec![target.clone()],
    }));

    fixture.open(&mut actor);
    let answers = actor.dispatches().clone();
    actor
        .handle(fixture.definition(1))
        .expect("a definition request");
    // The child wins: the request was forwarded before any heuristic work
    // started (`shim.md` §7 step 1), so this is an ordering the wire produces
    // whenever a handler is slower than the server it is standing in for.
    actor
        .handle(Event::ChildAnswered {
            editor_id: EditorRequestId::from_number(1),
            result: child_answer(&target, TARGET),
            latency: Micros(4_210_000),
        })
        .expect("the child's answer");
    assert_eq!(
        fixture.rows(),
        Vec::<String>::new(),
        "a row was written while the pool still had the query, so §7's \"once both answers are \
         known\" was decided by the child alone"
    );

    settle(&mut actor, &answers);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!(
            "{} rows for one query answered in the other order",
            rows.len()
        );
    };
    assert_eq!(
        field(row, "decision"),
        "\"committed\"",
        "the shim's own answer was lost because the child arrived first: {row}"
    );
    assert_eq!(
        field(row, "agreement"),
        "\"match_top1\"",
        "the two answers were never compared, so §6's predicate ran on nothing and the \
         precision numerator lost a query it agreed on: {row}"
    );
    assert_eq!(
        field(row, "lsp_latency_us"),
        "4210000",
        "the child's answer was dropped rather than held: {row}"
    );
}

/// `shim.md` §7: `$/cancelRequest` drops the pending query **and** signals the
/// deadline. The second half was unreachable while dispatch was in line — a
/// cancel was only ever handled between queries, so there was never a handler
/// running to signal — and `actor.rs` said so in a comment rather than in a
/// test, because a cancellation token wired up then would have been
/// unreachable code.
///
/// The handler waits to be let go, which is the whole fixture: one that
/// returned immediately would be racing the cancellation rather than reading
/// it, and would pass or fail by scheduling. What it reports is
/// `Deadline::expired`, which is what a handler polls cooperatively — so this
/// asserts the mechanism a real handler uses and not a flag beside it.
///
/// The absent row is the other half. A query cancelled while the pool had it
/// must not be recorded: §7 reserves a null oracle half for a query that never
/// had one, and a cancelled query has no `agreement` because nobody was ever
/// going to answer it.
#[test]
fn a_cancel_reaches_the_worker_that_is_still_running_the_query() {
    let fixture = Fixture::new("actor_cancel_in_flight", Proxying::No);
    let (go, wait) = crossbeam_channel::unbounded();
    let (report, expired) = crossbeam_channel::unbounded();
    // Held rather than dropped: a handler whose start signal has nowhere to go
    // logs about it, and the log is not what this test is reading.
    let (started, _starting) = crossbeam_channel::unbounded();
    let mut actor = fixture.actor(Arc::new(Waiting {
        started,
        go: wait,
        expired: report,
    }));

    fixture.open(&mut actor);
    let answers = actor.dispatches().clone();
    actor
        .handle(fixture.definition(1))
        .expect("a definition request");
    actor
        .handle(Event::Cancelled {
            editor_id: EditorRequestId::from_number(1),
        })
        .expect("a cancellation");
    // Sent after the cancel and never before it, which is what makes the
    // observation below deterministic rather than a race the test usually wins.
    go.send(()).expect("letting the handler go");
    settle(&mut actor, &answers);

    assert_eq!(
        expired.try_iter().collect::<Vec<bool>>(),
        vec![true],
        "a handler polling its deadline was told the query is still live after the editor \
         cancelled it, so the worker spends the rest of the budget on an answer that will be \
         discarded"
    );
    assert_eq!(
        fixture.rows().len(),
        0,
        "a query cancelled while the pool had it wrote a metric row, and nothing downstream \
         can tell it apart from a query nobody answered"
    );
    assert!(
        fixture.outbound().is_empty(),
        "the shim answered a cancelled request, which `shim.md` §7 forbids outright"
    );
}

/// The other end of the loop: `run` returns when the wire closes, and not
/// before the pool has handed back what `core` gave it.
///
/// The wire closing is not the query being over. A query dispatched a
/// microsecond earlier is still on a worker, and §7's row for it is written
/// when the worker comes home — so a loop that returned on the disconnect
/// would lose exactly the slow queries, which are the rows §7 is read for. In
/// a corpus run that is the tail of every repository.
///
/// The ordering is forced rather than hoped for. Every event is queued before
/// the loop starts and the sender is dropped, so the events channel is empty
/// and disconnected the instant the request has been dispatched; and the
/// handler does not return until this test lets it, so the pool cannot answer
/// before then. There is no schedule in which `run` reaches its exit with the
/// query already accounted for.
#[test]
fn the_loop_records_a_query_the_pool_still_had_when_the_wire_closed() {
    let fixture = Fixture::new("actor_drain", Proxying::No);
    // A rendezvous: the receive below returns only once the handler is inside
    // the send, which is inside the worker.
    let (started, starting) = crossbeam_channel::bounded(0);
    let (go, wait) = crossbeam_channel::unbounded();
    // Held for the reason the start channel is held in the cancellation test:
    // the handler reports its deadline whatever the test is reading, and a
    // dropped receiver would put a warning in a log nobody here is looking at.
    let (report, _expired) = crossbeam_channel::unbounded();
    let actor = fixture.actor(Arc::new(Waiting {
        started,
        go: wait,
        expired: report,
    }));

    let (events, incoming) = crossbeam_channel::unbounded();
    for event in fixture.session(1) {
        events.send(event).expect("a queued event");
    }
    // The transport goes away before the loop even starts. The queued events
    // are still delivered — that is what `the_loop_drains_its_channel_and_ends_
    // when_the_wire_closes` rests on too — so the request is handled and the
    // disconnect is what the loop sees next.
    drop(events);
    let looping = std::thread::Builder::new()
        .name("actor".to_owned())
        .spawn(move || actor.run(&incoming))
        .expect("the actor thread");

    starting
        .recv_timeout(Duration::from_secs(30))
        .expect("the handler to be reached");
    go.send(()).expect("letting the handler go");
    looping
        .join()
        .expect("the actor thread")
        .expect("the actor loop");

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!(
            "{} rows for a query the pool was still holding when the wire closed, where §7 \
             asks for one",
            rows.len()
        );
    };
    assert_eq!(
        field(row, "decision"),
        "\"abstained\"",
        "the row was written by something other than the handler's own answer: {row}"
    );
}

/// One query's worth of the deadline test, so the two halves cannot drift.
struct Queued {
    answered: bool,
    row: String,
}

fn queued_by(queued: Duration) -> Queued {
    let name = format!("deadline_queued_{}", queued.as_millis());
    let fixture = Fixture::new(&name, Proxying::No);
    let target = fixture.definition_in("src/target.rs");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: vec![target],
    }));

    fixture.open(&mut actor);
    // The request arrives, and then the clock moves before `core` gets to it —
    // which is what a queue is.
    let arrived = fixture.clock.now();
    fixture.clock.advance(queued);
    let answers = actor.dispatches().clone();
    actor
        .handle(Event::Requested {
            editor_id: EditorRequestId::from_number(1),
            params: definition_params(&fixture.uri("src/lib.rs")),
            arrived,
        })
        .expect("a definition request");
    settle(&mut actor, &answers);

    let rows = fixture.rows();
    let [row] = rows.as_slice() else {
        panic!("{} rows for one query", rows.len());
    };
    Queued {
        answered: fixture
            .outbound()
            .iter()
            .any(|outbound| matches!(outbound, Outbound::Definition { .. })),
        row: row.clone(),
    }
}

/// `classify`'s line, and `hard_cap`'s. Substrings of the message rather than
/// the whole of it, for the reason [`field`] is a scan: what is being asserted
/// is that the conversion is legible in the log, not its exact wording.
const CONVERSION: &str = "converting an expiry into an abstention";
const HARD_CAP: &str = "dropping an answer that arrived after its deadline";

/// One query answered too late, with every line it logged.
///
/// `relative` is the whole of the difference between the two paths that discard
/// a late answer, so it is the only parameter: the handler, the clock and the
/// budget are the same on both.
fn expiring_in(relative: &str, name: &str) -> Vec<String> {
    let fixture = Fixture::new(name, Proxying::No);
    let target = fixture.definition_in(relative);
    let mut actor = fixture.actor(Arc::new(Slow {
        clock: Arc::clone(&fixture.clock),
        locations: vec![target],
    }));

    let (logged, lines) = crossbeam_channel::unbounded();
    // The `didOpen` is inside the capture too: a line it emitted that happened
    // to match would be a false positive, and excluding it would hide one.
    tracing::subscriber::with_default(Capturing { events: logged }, || {
        fixture.open(&mut actor);
        fixture.request(&mut actor, 1);
    });
    lines.try_iter().collect()
}

fn lines_saying<'a>(lines: &'a [String], message: &str) -> Vec<&'a String> {
    lines.iter().filter(|line| line.contains(message)).collect()
}

/// `classify`'s first caller: the deadline is already gone when the request is
/// taken off the queue, so the parse in front of the handler is abandoned and no
/// handler ever runs.
///
/// It needs a document of its own, and that is the one non-obvious thing here.
/// `SnapshotSeed::realise` polls the deadline from tree-sitter's progress
/// callback, which fires "once per 100 parser operations" — so the five-line
/// `DOCUMENT` every other test uses finishes inside a single interval and
/// observes no deadline at all. The handler commits nothing, so if the parse
/// were *not* abandoned the query would reach the hard cap instead and this
/// fixture would fail on the `HARD_CAP` count rather than pass quietly on the
/// wrong route.
fn expired_before_the_parse(name: &str) -> Vec<String> {
    let fixture = Fixture::new(name, Proxying::No);
    let text = large_document();
    fs::write(fixture.root.join("src").join("lib.rs"), &text).expect("the large fixture document");
    let mut actor = fixture.actor(Arc::new(Committing {
        locations: Vec::new(),
    }));

    let (logged, lines) = crossbeam_channel::unbounded();
    tracing::subscriber::with_default(Capturing { events: logged }, || {
        actor
            .handle(Event::Negotiated {
                roots: vec![fixture.root.clone()],
                encoding: PositionEncoding::Utf16,
            })
            .expect("the negotiation");
        actor
            .handle(fixture.did_open(&text))
            .expect("the large document opening");
        // The request arrives, and the clock passes the 750ms budget before
        // `core` reaches it — which is a queue, and is what leaves the parse
        // with no time to start.
        let arrived = fixture.clock.now();
        fixture.clock.advance(Duration::from_millis(1_000));
        let answers = actor.dispatches().clone();
        actor
            .handle(Event::Requested {
                editor_id: EditorRequestId::from_number(1),
                params: definition_params(&fixture.uri("src/lib.rs")),
                arrived,
            })
            .expect("a definition request");
        settle(&mut actor, &answers);
    });
    lines.try_iter().collect()
}

/// A document long enough for the parser to reach its progress callback. The
/// count is far past the 100-operation interval rather than tuned to it: a
/// fixture sitting on the boundary would decide this test by which tree-sitter
/// revision is vendored.
fn large_document() -> String {
    let mut text = String::from("fn caller() {\n    target();\n}\n");
    for index in 0..2_000 {
        let _ = writeln!(
            text,
            "fn filler_{index}() {{ let held_{index} = {index}; }}"
        );
    }
    text
}

/// `classify`'s second caller: the handler itself runs out of time in a read and
/// propagates it with `?`, which is the one route that arrives having classified
/// nothing *and* having had a handler run.
fn propagated_from_a_read(name: &str) -> Vec<String> {
    let fixture = Fixture::new(name, Proxying::No);
    let mut actor = fixture.actor(Arc::new(Propagating {
        clock: Arc::clone(&fixture.clock),
    }));

    let (logged, lines) = crossbeam_channel::unbounded();
    tracing::subscriber::with_default(Capturing { events: logged }, || {
        fixture.open(&mut actor);
        fixture.request(&mut actor, 1);
    });
    lines.try_iter().collect()
}

/// Whether there is a child to be an oracle. `Mode` carries the argv and the
/// heuristics flag with it, so a test that wants only "is there a second
/// answer" says it here rather than assembling one.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Proxying {
    Yes,
    No,
}

struct Fixture {
    root: PathBuf,
    trace: PathBuf,
    clock: Arc<TestClock>,
    proxying: Proxying,
}

impl Fixture {
    fn new(name: &str, proxying: Proxying) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("clearing a previous run");
        }
        fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
        fs::create_dir_all(root.join("src")).expect("the fixture source directory");
        fs::write(root.join("src").join("lib.rs"), DOCUMENT).expect("the fixture document");
        fs::write(root.join("src").join("target.rs"), TARGET).expect("the fixture target file");

        // Outside the root, so the file list does not enumerate the trace of
        // the run that is writing it.
        let trace = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.jsonl"));
        if trace.exists() {
            fs::remove_file(&trace).expect("clearing a previous trace");
        }

        Self {
            root,
            trace,
            clock: Arc::new(TestClock::new()),
            proxying,
        }
    }

    fn actor(&self, handler: Arc<dyn LanguageHandler>) -> Actor {
        let server = match self.proxying {
            Proxying::Yes => vec![std::ffi::OsString::from("rust-analyzer")],
            Proxying::No => Vec::new(),
        };
        let config = Config::new(
            Mode::from_server_argv(server, Heuristics::Enabled),
            // Named rather than defaulted, because the deadline test's two
            // halves straddle it and a default that moved would move them.
            DeadlineOverride::Explicit(DeadlineMs::new(750)),
            Tracing::To(self.trace.clone()),
        );
        let (outgoing, written) = crossbeam_channel::unbounded();
        OUTBOUND.with(|slot| slot.replace(Some(written)));
        Actor::new(
            Registry::new(vec![handler]),
            config,
            Arc::clone(&self.clock) as Arc<dyn Clock>,
            outgoing,
        )
        .expect("an actor")
    }

    /// Everything that has to have happened before a query can be answered,
    /// in order: `shim.md` §4 answers nothing before the negotiation, and §8.6
    /// gives no query a document before the `didOpen`.
    ///
    /// A list rather than three calls, so that the same session can be handed
    /// to `handle` one at a time or sent down the channel `run` reads.
    fn session(&self, id: i64) -> Vec<Event> {
        vec![
            Event::Negotiated {
                roots: vec![self.root.clone()],
                encoding: PositionEncoding::Utf16,
            },
            self.did_open(DOCUMENT),
            self.definition(id),
        ]
    }

    /// `version` is 1 every time, deliberately: `core.md` §8.6 makes `didOpen`
    /// a *resync*, so the text it carries need not continue the one we held and
    /// its version need not be larger than the last one we saw.
    fn did_open(&self, text: &str) -> Event {
        Event::Notified {
            notification: DocumentNotification::DidOpen,
            params: raw(&format!(
                r#"{{"textDocument":{{"uri":"{}","languageId":"rust","version":1,"text":{}}}}}"#,
                self.uri("src/lib.rs"),
                json_string(text),
            )),
        }
    }

    fn definition(&self, id: i64) -> Event {
        Event::Requested {
            editor_id: EditorRequestId::from_number(id),
            params: definition_params(&self.uri("src/lib.rs")),
            arrived: self.clock.now(),
        }
    }

    fn open(&self, actor: &mut Actor) {
        for event in self.session(0).into_iter().take(2) {
            actor.handle(event).expect("the session's opening events");
        }
    }

    fn request(&self, actor: &mut Actor, id: i64) {
        let answers = actor.dispatches().clone();
        actor
            .handle(self.definition(id))
            .expect("a definition request");
        settle(actor, &answers);
    }

    /// One whole query: opened, asked, answered by the handler, and resolved
    /// against the child. Returns what `core` sent out for it.
    fn resolve(&self, id: i64, ranked: Vec<Location>, child: DefinitionResult) -> Vec<Outbound> {
        let mut actor = self.actor(Arc::new(Committing { locations: ranked }));
        self.open(&mut actor);
        self.request(&mut actor, id);
        actor
            .handle(Event::ChildAnswered {
                editor_id: EditorRequestId::from_number(id),
                result: child,
                latency: Micros(1_000),
            })
            .expect("the child's answer");
        self.outbound()
    }

    fn outbound(&self) -> Vec<Outbound> {
        OUTBOUND.with(|slot| {
            let written = slot.borrow();
            let written = written.as_ref().expect("an actor was built");
            written.try_iter().collect()
        })
    }

    /// The trace, one line per record. Absent is no rows rather than a panic:
    /// "nothing has been written yet" is a state two of these tests assert.
    fn rows(&self) -> Vec<String> {
        match fs::read_to_string(&self.trace) {
            Ok(text) => text.lines().map(str::to_owned).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn uri(&self, relative: &str) -> DocumentUri {
        DocumentUri::from_file_path(&self.root.join(relative)).expect("a file URI")
    }

    /// A `Location` for the `fn target` definition, through `ProjectView`'s own
    /// `read` and `parse` — because `Location::at_node` is the only
    /// constructor, and one built any other way is not one a handler could have
    /// returned.
    fn definition_in(&self, relative: &str) -> Location {
        let view = self.view();
        let rel = RelPath::new(Path::new(relative)).expect("a relative path");
        let path: ProjectPath = view
            .lookup(&ProjectRoot::new(&self.root), &rel)
            .unwrap_or_else(|| panic!("{relative} is not in the fixture file list"));
        let text = view.read(&path).expect("reading a fixture file");
        let tree = view.parse(&path, &text).expect("parsing a fixture file");
        let flat: String = text.chunks().collect();
        let start = flat
            .find(DEFINITION)
            .unwrap_or_else(|| panic!("{relative} does not contain the definition"));
        let node = tree
            .root_node()
            .descendant_for_byte_range(start, start + DEFINITION.len())
            .unwrap_or_else(|| panic!("no node spans the definition in {relative}"));

        Location::at_node(self.uri(relative), &node)
    }

    fn view(&self) -> ProjectView {
        FileListCache::new(
            vec![self.root.clone()],
            Arc::clone(&self.clock) as Arc<dyn Clock>,
            DebounceMs::RESCAN,
        )
        .expect("the scanner thread")
        .view(Deadline::none(), grammar())
        .expect("enumerating the fixture")
    }
}

thread_local! {
    /// Where the actor's outbound channel is parked, so that `Fixture` can hand
    /// back what was sent without every test threading a receiver through.
    /// Thread-local because cargo's test runner is multi-threaded and each test
    /// is one thread with one actor.
    static OUTBOUND: std::cell::RefCell<Option<Receiver<Outbound>>> =
        const { std::cell::RefCell::new(None) };
}

/// The pool's half of a query, taken the way `Actor::run`'s `select!` takes it.
///
/// A query is two steps now rather than one, which is the whole of `core.md`
/// §2's split: `core` accepts the request and hands over a `SnapshotSeed`, and
/// a worker does the parse, the handler and §8.4's conversion. Every test here
/// drives the state machine directly rather than through the loop, so what
/// `run` would have selected has to be handed over by hand.
///
/// The timeout is not a latency assertion and is far past anything these
/// fixtures do. What it protects is the suite: a pool that stops answering
/// should fail a test rather than hang one.
fn settle(actor: &mut Actor, answers: &Receiver<Finished>) {
    match answers.recv_timeout(Duration::from_secs(30)) {
        Ok(finished) => actor.finished(finished),
        Err(error) => panic!("the dispatch pool did not answer the query: {error}"),
    }
}

fn reports(outbound: &[Outbound]) -> Vec<&Outbound> {
    outbound
        .iter()
        .filter(|sent| matches!(sent, Outbound::Report(_)))
        .collect()
}

/// The value of one of §7's columns, as JSON text.
///
/// A scan rather than a parse: `clippy.toml` denies `serde_json::Value`, and
/// what is being asserted is the record's text, which §7 fixes.
fn field(row: &str, name: &str) -> String {
    let key = format!("\"{name}\":");
    let start = row
        .find(&key)
        .unwrap_or_else(|| panic!("no {name} column in {row}"))
        + key.len();
    let rest = &row[start..];
    let mut depth = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in rest.char_indices() {
        match byte {
            '\\' if quoted => escaped = !escaped,
            '"' if !escaped => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted && depth > 0 => depth -= 1,
            ',' | '}' if !quoted && depth == 0 => return rest[..index].to_owned(),
            _ => escaped = false,
        }
        if byte != '\\' {
            escaped = false;
        }
    }
    rest.to_owned()
}

/// A subscriber that keeps every event's fields as one line of text.
///
/// Hand-written rather than `tracing_subscriber::fmt`, and not for want of the
/// crate: `driver` may not *name* `tracing_subscriber` anywhere, because
/// `deps.md` §9 installs the subscriber in the crate that owns a program's
/// command line and `seam.rs` scans every source file in the crate — this one
/// included — to hold it. `tracing`'s own `Subscriber` trait needs no such
/// permission.
///
/// A channel rather than a `Mutex<Vec<_>>` because `Subscriber` is `Send + Sync`
/// and records through `&self`, which is the one shape in a test that tempts a
/// lock. It is the same reason the `Checking` handler double below has a sender.
struct Capturing {
    events: crossbeam_channel::Sender<String>,
}

impl tracing::Subscriber for Capturing {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    /// Nothing under test opens a span, and a subscriber must still mint ids.
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.events
            .send(fields.0)
            .expect("the test is still listening");
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

/// One event's fields, spelled `name=value` the way a `fmt` subscriber spells
/// them, so an assertion reads like the log a human would be looking at.
struct Fields(String);

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}

/// The `depth` field of a captured line, if it has one. A scan rather than a
/// parse, for the reason [`field`] is one: what is being asserted is the text.
fn depth_of(line: &str) -> Option<u64> {
    let start = line.find("depth=")? + "depth=".len();
    line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

fn definition_params(uri: &DocumentUri) -> Box<RawValue> {
    raw(&format!(
        r#"{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":1,"character":{POSITION}}}}}"#
    ))
}

fn raw(json: &str) -> Box<RawValue> {
    RawValue::from_string(json.to_owned()).expect("valid JSON in a fixture")
}

/// A JSON string literal, escaped. Only newlines and quotes occur in these
/// fixtures, and a fixture that grew a backslash would fail loudly here rather
/// than deserialize into something else.
fn json_string(text: &str) -> String {
    assert!(
        !text.contains('\\'),
        "this escaper does not handle backslashes, and the fixture now has one"
    );
    format!("\"{}\"", text.replace('"', "\\\"").replace('\n', "\\n"))
}

/// The child's `textDocument/definition` response, in the one-location shape.
fn child_answer(location: &Location, text: &str) -> DefinitionResult {
    let rope = shared::Rope::from(text);
    let encode = |offset| {
        WirePosition::encode(offset, PositionEncoding::Utf16, &rope)
            .expect("a byte offset the fixture took from a node in this text")
    };
    DefinitionResult::One(WireLocation::new(
        location.uri().clone(),
        WireRange {
            start: encode(location.range().start),
            end: encode(location.range().end),
        },
    ))
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

/// A handler that answers, and says which thread it answered on. The location
/// it commits is supplied so the test can put the definition in a file the
/// query did not come from, which is what makes §8.4's conversion do a read.
struct Reporting {
    locations: Vec<Location>,
    ran_on: crossbeam_channel::Sender<ThreadId>,
}

impl LanguageHandler for Reporting {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        self.ran_on
            .send(std::thread::current().id())
            .expect("the test is still listening");
        Ok(query.policy.decide(
            Strata::from_reference(Stratum::LocalBinding),
            Confidence::ONE,
            self.locations.clone(),
            Trace::new(),
        ))
    }
}

/// A handler that does not return until the test lets it, and then reports what
/// its deadline says. It is the only way to observe a `$/cancelRequest` from
/// inside a query rather than beside one.
struct Waiting {
    /// Sent before the wait, so a test can know the query is *inside* a worker
    /// rather than still on the channel. A rendezvous rather than a buffered
    /// send: what it establishes is an ordering, and a buffered one would
    /// establish only that the handler had been reached eventually.
    started: crossbeam_channel::Sender<()>,
    go: Receiver<()>,
    expired: crossbeam_channel::Sender<bool>,
}

impl LanguageHandler for Waiting {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        if self.started.send(()).is_err() {
            tracing::warn!("the test stopped waiting for the handler to start");
        }
        // Logged rather than expected: `panic_in_result_fn` is denied, and a
        // handler that panicked here would take the pool thread with it and
        // leave the test blocked on an answer instead of failing.
        match self.go.recv_timeout(Duration::from_secs(30)) {
            Ok(()) => {}
            Err(error) => tracing::warn!(%error, "the test never let the handler go"),
        }
        if self.expired.send(query.deadline.expired()).is_err() {
            tracing::warn!("the test stopped listening for the deadline");
        }
        Ok(Outcome::Abstain {
            reason: AbstainReason::NoCandidates,
            strata: Strata::from_reference(Stratum::LocalBinding),
            trace: Trace::new(),
        })
    }
}

struct Committing {
    locations: Vec<Location>,
}

impl LanguageHandler for Committing {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        Ok(query.policy.decide(
            Strata::from_reference(Stratum::LocalBinding),
            Confidence::ONE,
            self.locations.clone(),
            Trace::new(),
        ))
    }
}

struct Failing;

impl LanguageHandler for Failing {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        // Not `HandlerError::DeadlineExpired`, which `dispatch` maps back to
        // an abstention: this is the class §7 wants to see named, and the arm
        // that would swallow it is the one being checked.
        Err(ProjectError::Unresolvable {
            uri: query.doc.uri.clone(),
        }
        .into())
    }
}

/// A handler that ran and would not commit. Distinct from `Failing` in exactly
/// the way §7 keeps `abstained` and `failed` distinct: this one is a hard
/// query, not a broken handler.
struct Declining;

impl LanguageHandler for Declining {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, _query: &Query<'_>) -> Result<Outcome, Error> {
        Ok(Outcome::Abstain {
            reason: AbstainReason::NoCandidates,
            strata: Strata::from_reference(Stratum::LocalBinding),
            trace: Trace::new(),
        })
    }
}

/// A handler that answers, and takes longer than its deadline doing it — which
/// is the one thing `Committing` cannot be made to do, since a handler that
/// respects its budget is never hard-capped.
///
/// It moves the clock rather than sleeping: `core.md` §10's whole reason for an
/// injected clock is that a test which waits for a real 750ms is a test that
/// fails on a loaded machine and is deleted.
struct Slow {
    clock: Arc<TestClock>,
    locations: Vec<Location>,
}

impl LanguageHandler for Slow {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        // Past the 750ms the fixture names, so what this returns is already
        // late by the time it returns it.
        self.clock.advance(Duration::from_millis(1_000));
        // Not `LocalBinding`: every other double here reports that one, and a
        // stratum that survives the cap has to be distinguishable from the one
        // a mistake would most likely leave behind.
        Ok(query.policy.decide(
            Strata::from_reference(Stratum::ExplicitImport),
            Confidence::ONE,
            self.locations.clone(),
            Trace::new(),
        ))
    }
}

/// A handler that spends its budget and then reads, propagating the refusal
/// with the `?` `core.md` §1 says a handler should write rather than checking
/// the clock itself.
///
/// Distinct from `Slow` in the way `Classified`'s two variants are distinct:
/// `Slow` returns an `Outcome`, so the strata it assigned survive whatever
/// discards the answer, where this one never gets as far as assigning any. That
/// is the case `dispatch.rs` calls "a read expired inside the handler before it
/// assigned a stratum", and it is the route with no fixture until now.
struct Propagating {
    clock: Arc<TestClock>,
}

impl LanguageHandler for Propagating {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        self.clock.advance(Duration::from_millis(1_000));
        let root = query
            .project
            .root_of(&query.doc.uri)
            .expect("the queried document is under a root");
        let rel = RelPath::new(Path::new("src/target.rs")).expect("a relative path");
        let path = query
            .project
            .lookup(root, &rel)
            .expect("src/target.rs is in the fixture file list");
        // `ProjectView` checks the deadline before starting the I/O, so this is
        // the refusal and not a short read. Propagated rather than matched on:
        // a handler that inspected the class here would be doing the driver's
        // job, which is the whole of what §10 keeps on one side of the seam.
        query.project.read(&path)?;
        // Only reachable if the read did *not* refuse, which means the clock
        // never expired and there was no `Error` for anything to convert. A
        // failure rather than a panic — `panic_in_result_fn` is denied, and the
        // driver logs a failure under its own line, so the fixture that collects
        // the log says which of the two happened.
        Err(ProjectError::Unresolvable {
            uri: query.doc.uri.clone(),
        }
        .into())
    }
}

/// A handler that answers nothing and reports one thing: how far its tree
/// reaches, beside how far its text does. `core.md` §2 says those cannot
/// disagree, and a stale incremental base is the one way to make them.
struct Checking {
    parsed: crossbeam_channel::Sender<(usize, usize)>,
}

impl LanguageHandler for Checking {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        grammar()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        self.parsed
            .send((
                query.doc.tree().root_node().end_byte(),
                query.doc.text.len().0,
            ))
            .expect("the test is still listening");
        Ok(Outcome::Abstain {
            reason: AbstainReason::NoCandidates,
            strata: Strata::from_reference(Stratum::LocalBinding),
            trace: Trace::new(),
        })
    }
}
