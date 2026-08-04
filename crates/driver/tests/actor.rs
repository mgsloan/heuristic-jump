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

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use driver::{
    Actor, Config, DeadlineMs, DeadlineOverride, DebounceMs, Event, FileListCache, Heuristics,
    Mode, Outbound, Registry, Tracing,
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
    actor
        .handle(Event::Requested {
            editor_id: EditorRequestId::from_number(1),
            params: definition_params(&fixture.uri("src/lib.rs")),
            arrived,
        })
        .expect("a definition request");

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
            Event::Notified {
                notification: DocumentNotification::DidOpen,
                params: raw(&format!(
                    r#"{{"textDocument":{{"uri":"{}","languageId":"rust","version":1,"text":{}}}}}"#,
                    self.uri("src/lib.rs"),
                    json_string(DOCUMENT),
                )),
            },
            self.definition(id),
        ]
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
        actor
            .handle(self.definition(id))
            .expect("a definition request");
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
