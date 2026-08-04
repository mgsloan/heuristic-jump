//! `design/core.md` §7's corpus scan, end to end over a fixture repository.
//!
//! Three claims are asserted here that nothing else in the build would notice
//! being dropped:
//!
//! * **A replay row has §7's field set, in §7's order.** The section's whole
//!   argument for one record type is that "a completed replay row is byte
//!   comparable with a row the shim emitted in the field"; a field silently
//!   renamed or dropped breaks that and nothing fails.
//! * **Replay is deterministic — same corpus, same commit, same output, byte
//!   for byte.** That is what makes it usable as a gate rather than a report,
//!   and it is the property the whole replay design rests on. It is also the
//!   one a hash-set iteration order or a wall-clock deadline would quietly
//!   destroy.
//! * **A run given one split's path cannot name the other.** Held-out
//!   isolation is a filesystem boundary rather than a rule somebody remembers,
//!   and `test/` is a *sibling* of `training/` for exactly that reason.
//!
//! The handler is written out here rather than taken from `lang_rust`, and
//! that is the point of the file as much as a convenience: `measure_core`
//! takes its handler as `&dyn LanguageHandler` and depends on no language, so
//! a test that could only run against `lang_rust` would not be testing the
//! claim.

#![expect(
    clippy::disallowed_methods,
    reason = "Two bans, neither of which reaches here. `Command::output` is banned so the shim polls cooperatively against its deadline; this is a test building a fixture git checkout, where there is no deadline and the child is `git` on a three-file repository. `read_dir` is banned because it bypasses gitignore semantics on the *search* path, where a gitignored file is out of scope; `files_under` asks the opposite question — whether a file the run was not asked for appeared at all — and a walk that honoured .gitignore would answer no whatever the run wrote."
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builders below are free functions in a file that is nothing but tests. Failing loudly is the point: a fixture that half-built would leave an empty corpus, and every assertion here passes against an empty corpus."
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use shared::{
    AbstainReason, ByteLen, CandidateCount, Confidence, Error, FileCount, FileExtension,
    LanguageHandler, LanguageId, Location, Margin, Micros, Outcome, Query, Refinement, StageLabel,
    StageName, Strata, Stratum, Trace,
};
use tree_sitter::Language;

/// §7's record, transcribed. The order is the order `serde_json` writes a
/// struct in, which is the declaration order, which is the section's.
const RECORD_FIELDS: &[&str] = &[
    "uri",
    "position",
    "language",
    "mode",
    "server_health",
    "decision",
    "failure",
    "stratum_prior",
    "stratum_final",
    "confidence",
    "margin",
    "considered",
    "stages",
    "bytes_scanned",
    "files_parsed",
    "queued_us",
    "stage_us",
    "heuristic_latency_us",
    "heuristic_locations",
    "returned",
    "truncated_list",
    "lsp_latency_us",
    "lsp_locations",
    "agreement",
    "severity",
];

const SOURCE: &str =
    "pub fn alpha() -> u32 {\n    7\n}\n\npub fn beta() -> u32 {\n    alpha()\n}\n";

/// A second file whose identifiers sit at the *same* byte offsets as
/// [`SOURCE`]'s and spell something else — `alpha`/`gamma` both begin at byte
/// 7. That is what makes a join on `(file, offset)` distinguishable from one on
/// `offset` alone, which a single-file repository cannot show.
const OTHER_SOURCE: &str =
    "pub fn gamma() -> u64 {\n    3\n}\n\npub fn delta() -> u64 {\n    gamma()\n}\n";

/// The nine strata as the record and the report spell them, which is
/// `StratumName`'s spelling and not `Stratum`'s. Transcribed rather than
/// derived, so a renamed variant has to be renamed here too.
const STRATUM_NAMES: [&str; 9] = [
    "local_binding",
    "same_file_module",
    "explicitly_imported",
    "wildcard_imported",
    "ambiguous_name",
    "external_dependency",
    "macro_generated",
    "type_inference_required",
    "unimplemented",
];

/// The grammar the workspace pins, read from the manifest rather than from the
/// lockfile the implementation embeds: asserting a value against the file it
/// was computed from asserts nothing.
const WORKSPACE_MANIFEST: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"));

/// `replay`'s own source, for the half of "replay enforces no deadline at all"
/// that no record can observe: the `Deadline` the *snapshot parse* runs under.
/// A bounded one there drops rows on a busy machine and nothing else, which is
/// invisible to a fast fixture and is exactly the failure the requirement is
/// about.
const REPLAY_SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/replay.rs"));

/// The document, as the fixture for the claims that are a *list*: §7's command
/// line prints one usage line per stage, and the set of flags is the claim.
const CORE_MD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../design/core.md"));

/// Two lockfiles differing only in which revision of the grammar they pin.
/// Written out rather than generated, so what the pin has to distinguish is
/// visible: the same crate at two versions, with the checksums cargo records
/// for them.
const LOCKED_ONE: &str = "\
version = 4

[[package]]
name = \"tree-sitter\"
version = \"0.26.11\"
source = \"registry+https://github.com/rust-lang/crates.io-index\"
checksum = \"af1c71c1c4cc0920b20d6b0f6572e7682cd07a6a2faec71067a31fa394c586df\"

[[package]]
name = \"tree-sitter-rust\"
version = \"0.24.2\"
source = \"registry+https://github.com/rust-lang/crates.io-index\"
checksum = \"439e577dbe07423ec2582ac62c7531120dbfccfa6e5f92406f93dd271a120e45\"

[metadata]
\"checksum unrelated\" = \"0\"
";

const LOCKED_TWO: &str = "\
version = 4

[[package]]
name = \"tree-sitter-rust\"
version = \"0.25.0\"
source = \"registry+https://github.com/rust-lang/crates.io-index\"
checksum = \"0000000000000000000000000000000000000000000000000000000000000000\"
";

struct TestHandler;

impl LanguageHandler for TestHandler {
    fn language_ids(&self) -> &'static [LanguageId] {
        const IDS: &[LanguageId] = &[LanguageId::new("rust")];
        IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        const EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];
        EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        // Deliberately the template's behaviour: everything abstains, which is
        // what makes the fixture's expected table computable by hand.
        let reason = match shared::identifier_at(query.doc, query.position) {
            None => AbstainReason::NotAnIdentifier,
            Some(_) => AbstainReason::UnsupportedRole,
        };
        Ok(Outcome::Abstain {
            reason,
            strata: Strata::from_reference(Stratum::Unimplemented),
            trace: Trace::new(),
        })
    }
}

/// A handler that *reports*, which the template deliberately does not.
///
/// It refines its stratum and fills a `Trace` with one of everything §7 calls
/// handler-reported. It commits with an empty location list, which is legal
/// and is what keeps the fixture's expected table computable by hand: the
/// oracle answered `null` everywhere, so an empty commit is the mutual "no
/// definition here" §6 calls a match.
struct ReportingHandler;

impl LanguageHandler for ReportingHandler {
    fn language_ids(&self) -> &'static [LanguageId] {
        const IDS: &[LanguageId] = &[LanguageId::new("rust")];
        IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        const EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];
        EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        let mut trace = Trace::new();
        trace.stage(StageLabel::new("ref:Type"));
        trace.stage(StageLabel::new("scope:miss"));
        trace.timed(StageName::new("search"), Micros(900));
        trace.scanned(ByteLen(1234));
        trace.parsed(FileCount(3));
        trace.ranked(
            Margin::new(0.5).expect("a finite, non-negative margin"),
            CandidateCount(7),
        );

        Ok(query.policy.decide(
            // The one refinement §8 permits, and the reason §7 makes the
            // stratum two fields: the reference said `explicitly_imported` and
            // the search found the name to be ambiguous.
            Strata::from_reference(Stratum::ExplicitImport).refine(Refinement::AmbiguousName),
            Confidence::ONE,
            Vec::new(),
            trace,
        ))
    }
}

/// A handler that commits a ranked list, so that a replay reaches
/// `Agreement::classify` with something on both sides.
///
/// The list is the same two locations for every query — the first identifier in
/// the document and the first one on row 4 — because what the fixture varies is
/// the *oracle's* answer, and a handler whose answer moved with the position
/// would make the expected classification a function of the corpus rather than
/// of the predicate.
struct MismatchingHandler;

impl LanguageHandler for MismatchingHandler {
    fn language_ids(&self) -> &'static [LanguageId] {
        const IDS: &[LanguageId] = &[LanguageId::new("rust")];
        IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        const EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];
        EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        let mut trace = Trace::new();
        trace.stage(StageLabel::new("ref:Value"));

        let mut locations = Vec::new();
        for node in shared::identifiers(query.doc) {
            let row = node.start_position().row;
            let wanted = match locations.len() {
                0 => row == 0,
                1 => row >= 4,
                _ => break,
            };
            if wanted {
                locations.push(Location::at_node(query.doc.uri.clone(), &node));
            }
        }

        Ok(query.policy.decide(
            Strata::from_reference(Stratum::ExplicitImport),
            Confidence::ONE,
            locations,
            trace,
        ))
    }
}

/// A handler that reports the deadline it was handed, through the only channel
/// a `&self` handler has: its own `Trace`. The labels are what
/// [`a_replay_enforces_no_deadline_at_all`] reads back out of the record.
struct DeadlineReportingHandler;

impl LanguageHandler for DeadlineReportingHandler {
    fn language_ids(&self) -> &'static [LanguageId] {
        const IDS: &[LanguageId] = &[LanguageId::new("rust")];
        IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        const EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];
        EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        let mut trace = Trace::new();
        // `at()` is `None` only for `Deadline::none`, which is why it is an
        // `Option` and not an instant very far away: a caller has to say what
        // it does when there is no instant rather than compare against one.
        trace.stage(StageLabel::new(if query.deadline.at().is_some() {
            "deadline:bounded"
        } else {
            "deadline:unbounded"
        }));
        trace.stage(StageLabel::new(if query.deadline.expired() {
            "deadline:expired"
        } else {
            "deadline:live"
        }));
        Ok(Outcome::Abstain {
            reason: AbstainReason::NoCandidates,
            strata: Strata::from_reference(Stratum::ExplicitImport),
            trace,
        })
    }
}

/// A handler that fails rather than answering. `replay` files it under
/// `Unimplemented` for want of anywhere honest to put a query whose handler
/// reported no stratum at all, which is why the template check cannot be the
/// presence of that row.
struct FailingHandler;

impl LanguageHandler for FailingHandler {
    fn language_ids(&self) -> &'static [LanguageId] {
        const IDS: &[LanguageId] = &[LanguageId::new("rust")];
        IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        const EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];
        EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, _query: &Query<'_>) -> Result<Outcome, Error> {
        Err(shared::ProjectError::Read {
            path: PathBuf::from("/nonexistent"),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        }
        .into())
    }
}

/// `core.md`: the placeholder "reports `Stratum::Unimplemented`, which no real
/// handler may return, and its presence in a metrics table means the template
/// has not been replaced — **a gate check** rather than something anybody has
/// to notice".
///
/// A gate check needs something to read, and the two obvious readings are both
/// wrong: the row is printed whatever the corpus held, and its `queries` count
/// includes handlers that returned `Err` and reported no stratum at all. The
/// third handler here is the one that matters — it is as far from a template as
/// a handler gets, and every one of its queries lands in that row.
#[test]
fn the_unimplemented_stratum_identifies_the_template_and_not_a_broken_handler() {
    let corpus = fixture("template_state");
    enumerate(&corpus);
    write_truth(&corpus);

    for (expected, handler) in [
        ("unreplaced", &TestHandler as &dyn LanguageHandler),
        ("replaced", &ReportingHandler),
        ("replaced", &FailingHandler),
    ] {
        let report = measure_core::replay_table(
            handler,
            &shared::SystemClock,
            &replay_arguments(&corpus, measure_core::Format::Json),
        )
        .expect("replay");

        assert!(
            report.contains(&format!("\"template\": \"{expected}\"")),
            "the table should read {expected} for this handler, and a gate that \
             cannot read it off the report is a gate somebody has to remember \
             to run. core.md makes Stratum::Unimplemented self-identifying, \
             which a handler that merely failed must not be able to \
             counterfeit.\n{report}"
        );
    }
}

#[test]
fn a_replay_row_carries_section_7s_field_set_in_section_7s_order() {
    let corpus = fixture("field_set");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let first = text
        .lines()
        .next()
        .expect("at least one query was replayed");
    let found = field_order(first);

    assert_eq!(
        found, RECORD_FIELDS,
        "a replay row and core.md §7's record disagree. The section's argument \
         for one record type is that a replay row is byte-comparable with a \
         field row, and a renamed or dropped field breaks that silently"
    );
}

/// §7: "everything from `stratum_prior` through `files_parsed` is reported
/// *by the handler*, since only it knows which resolution path produced the
/// answer and what it cost". Until `conformance-013` widened the seam, every
/// one of those columns was written at its empty value and no test could tell
/// the difference between a handler that reported nothing and a seam that
/// could not carry it.
///
/// Half of this is a compile-time claim already — `ReportingHandler` does not
/// build without a `Strata` and a `Trace` — and the assertions are the other
/// half: that what it reported arrives in the record rather than being
/// dropped between the seam and the JSON.
#[test]
fn the_handler_reported_half_of_the_record_crosses_the_seam() {
    let corpus = fixture("handler_report");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay_with(&ReportingHandler, &corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let first = text
        .lines()
        .next()
        .expect("at least one query was replayed");

    for expected in [
        // Two fields, not one, and they differ — which is the property a
        // single `Stratum` could not express at all.
        "\"stratum_prior\":\"explicitly_imported\"",
        "\"stratum_final\":\"ambiguous_name\"",
        "\"margin\":0.5",
        "\"considered\":7",
        "\"stages\":[\"ref:Type\",\"scope:miss\"]",
        "\"bytes_scanned\":1234",
        "\"files_parsed\":3",
        "\"stage_us\":{\"search\":900}",
    ] {
        assert!(
            first.contains(expected),
            "the handler reported {expected} and the record does not carry it. \
             core.md §7 makes these the handler's own account of which \
             resolution path produced the answer and what it cost, and a \
             column written at its empty value is indistinguishable in the \
             metrics from a handler that did no work.\nrecord: {first}"
        );
    }
}

#[test]
fn replay_is_deterministic_byte_for_byte() {
    let corpus = fixture("deterministic");
    enumerate(&corpus);
    write_truth(&corpus);

    let once = corpus.scratch.join("once.jsonl");
    let twice = corpus.scratch.join("twice.jsonl");
    replay(&corpus, Some(&once));
    replay(&corpus, Some(&twice));

    assert_eq!(
        without_the_clock(&fs::read_to_string(&once).expect("first run")),
        without_the_clock(&fs::read_to_string(&twice).expect("second run")),
        "two replays of the same corpus at the same commit disagree. \
         core.md §7 makes this the property that lets replay be a gate rather \
         than a report — a hash-set iteration order or a wall-clock deadline \
         is what usually takes it away"
    );
}

/// The sibling of the test above, and the one §7's command line actually
/// states: "**`replay` is deterministic.** Same corpus, same commit, same
/// table, byte for byte — which is what makes it usable as a gate rather than
/// a report."
///
/// The records file is not the table, and the assertion above has to mask a
/// field to hold at all. This one masks nothing, in both formats, because
/// `--format json` is the one the harness consumes and a wall clock in the
/// table would take the property away there first.
#[test]
fn the_printed_table_is_byte_identical_across_runs() {
    let corpus = fixture("table_determinism");
    enumerate(&corpus);
    write_truth(&corpus);

    for format in [measure_core::Format::Table, measure_core::Format::Json] {
        let once = table_of(&corpus, format);
        let twice = table_of(&corpus, format);

        // Two empty strings are equal, and the assertion below would pass on a
        // corpus that produced no rows at all.
        assert!(
            once.contains("unimplemented"),
            "the {format:?} table named no stratum, so the comparison below \
             would hold whatever the replay did.\n{once}"
        );
        assert_eq!(
            once, twice,
            "two replays of the same corpus at the same commit printed \
             different {format:?} tables. core.md §7's command line makes this \
             the property that lets replay be a gate rather than a report; a \
             clock reading in the rendered artifact is what usually takes it \
             away, which is why `Table` holds counters and no `Duration`"
        );
    }
}

/// §7 on how fast a replay is: "**How fast a replay actually is, is a
/// measurement rather than a target.** No number is set here, and none should
/// be inferred… So `measure replay` reports its own wall clock alongside the
/// per-query work counters, `loops.md` §9 records both from the first run, and
/// what to do about the number is decided when there is one."
///
/// Two halves, and the second was not merely untested — it did not happen. The
/// event was emitted into a facade with no subscriber behind it: `heuristic_jump`
/// installs one in its `main` and a `measure_<lang>` main is four lines that do
/// not, so every `tracing::info!` and every `tracing::warn!` in this crate —
/// the wall clock, the collection checkpoints, the unreadable-file warnings —
/// went nowhere at all. It is deliberately not in the table: §7's command line
/// makes that byte-identical across runs, which is the whole difference between
/// a number read as a trend and an artifact that has to compare exactly.
///
/// The subscriber the run installs writes to stderr, which a test cannot read
/// back; a scoped one takes precedence for this thread and can. That the run
/// installs *a* subscriber is the other assertion, and it needs no writer at
/// all — nothing else in the workspace sets a global dispatcher, so if this
/// crate stops doing it there is none.
#[test]
fn a_replay_reports_its_own_wall_clock() {
    let corpus = fixture("wall_clock");
    enumerate(&corpus);
    write_truth(&corpus);

    assert!(
        tracing::dispatcher::has_been_set(),
        "a measure run installed no log subscriber, so §7's wall clock — and \
         every warning this crate emits about a server that errored or a file \
         it could not read — is written into a facade with nothing behind it"
    );

    let truth = fs::read_to_string(truth_path(&corpus, "oracle")).expect("the truth file");
    let queries = truth.lines().count() - 1;

    let log = corpus.scratch.join("replay.log");
    // Created empty rather than left to the first event, so a replay that
    // reports nothing fails on what is missing from the log instead of on the
    // log not existing.
    fs::write(&log, "").expect("an empty log to append to");
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_ansi(false)
        .with_writer(LogFile(log.clone()))
        .finish();
    tracing::subscriber::with_default(subscriber, || replay(&corpus, None));

    let text = fs::read_to_string(&log).expect("the scoped subscriber wrote the log");
    for expected in ["replayed", "wall_clock_us=", &format!("queries={queries}")] {
        assert!(
            text.contains(expected),
            "the replay did not report {expected}. loops.md §9 records the wall \
             clock and the per-query counters from the first run, and a number \
             nobody set a target for is one that has to be observed before it \
             can be argued about\n{text}"
        );
    }
}

/// A `MakeWriter` over a file, because the alternative is a shared in-memory
/// buffer and the design has no locks in it.
struct LogFile(PathBuf);

impl tracing_subscriber::fmt::MakeWriter<'_> for LogFile {
    type Writer = fs::File;

    fn make_writer(&self) -> fs::File {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .expect("the log file")
    }
}

/// §7: "**Only the heuristic side is re-measured, and its timing is an
/// observation, not a control input.** … `lsp_latency_us` comes from `collect`
/// and is a property of the frozen truth — which is exactly what
/// `high-level.md`'s value weighting wants, since it is a fact about how slow
/// the real server was, not about this run."
///
/// The two latencies sit side by side in the record and one of them is a
/// measurement of the machine the replay ran on. A replay that filled the
/// oracle's column from its own clock — or from nothing — would weight every
/// query by a number that says how fast this laptop is, and the weighting is
/// the whole reason the field is carried.
#[test]
fn the_oracles_latency_is_the_frozen_truths_and_never_this_runs() {
    let corpus = fixture("frozen_latency");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    assert!(text.lines().count() > 1, "the fixture replayed nothing");
    for line in text.lines() {
        assert!(
            line.contains("\"lsp_latency_us\":1234"),
            "the oracle's latency is not the one the truth file recorded. It is \
             a fact about how slow the real server was on this repository at \
             this commit, and a replay that re-measured it would weight every \
             query by how fast the machine that replayed it happened to \
             be.\nrecord: {line}"
        );
    }
}

/// §7: "**Replay enforces no deadline at all.** This is the constraint that
/// makes replay worth having, and it is easy to get wrong by doing the obvious
/// thing. A wall-clock deadline makes abstention depend on machine load: the
/// same handler on the same snapshot gives up on a busy machine and finishes on
/// an idle one, so *coverage* — not just latency — becomes a property of what
/// else was running."
///
/// `crates/driver/tests/deadline.rs` holds the type's half — that
/// `Deadline::none` never expires and names no instant. This is the other half,
/// and it is the one the requirement is actually about: that a **replay** hands
/// the handler one of those. Nothing observed it, and the mistake it guards
/// against is a one-line edit that no other test would notice, because a
/// deadline generous enough to pass on this machine is what a wall-clock
/// deadline looks like right up until the machine is busy.
///
/// Two assertions because the query is only half the surface. The parse runs
/// under a deadline of its own that no handler can see, and an abandoned parse
/// drops the row entirely — so that half is held by scanning `replay.rs` for
/// the constructor instead. Planting `Deadline::new` in either place fails
/// exactly one of these.
#[test]
fn a_replay_enforces_no_deadline_at_all() {
    let corpus = fixture("no_deadline");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay_with(&DeadlineReportingHandler, &corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    assert!(
        text.lines().count() > 1,
        "the fixture replayed {} queries, and a loop over none of them holds \
         nothing",
        text.lines().count()
    );
    for line in text.lines() {
        assert!(
            line.contains(
                "\"stages\":[\"deadline:unbounded\",\"deadline:live\",\
                 \"abstain:no_candidates\"]"
            ),
            "a replayed query ran under a deadline the handler could see. \
             core.md §7 makes an unbounded deadline the constraint that makes \
             replay worth having: with a clock, the same handler on the same \
             snapshot abstains on a busy machine and commits on an idle one, \
             so two runs cannot be compared at all.\nrecord: {line}"
        );
    }

    assert!(
        !REPLAY_SOURCE.contains("Deadline::new") && REPLAY_SOURCE.contains("Deadline::none()"),
        "replay builds a bounded Deadline somewhere. The snapshot parse takes \
         one and no record can report it, so a wall clock there costs coverage \
         silently — the row is dropped rather than abstained, and the metric \
         moves with whatever else the machine was doing"
    );
}

/// §7's command line: "`--limit` defaults to 20 000 and `--seed` makes the
/// sample reproducible — an unseeded sample is a corpus that cannot be
/// regenerated, which defeats freezing it."
///
/// Every other test here passes a limit above what the fixture holds, so the
/// sampler is never reached and the seed is decoration. Both halves matter and
/// they fail in opposite directions: a seed nothing reads makes the corpus
/// unregenerable, and a seed that changes nothing makes `--seed` a flag that
/// lies. The second is the one a hash of the position, or a sort before the
/// sample rather than after it, would quietly produce.
#[test]
fn the_sample_is_the_seeds_and_a_second_seed_is_a_second_sample() {
    let corpus = fixture("seeded_sample");

    enumerate_with(&corpus, "5", "7");
    let once = positions_of(&corpus);
    enumerate_with(&corpus, "5", "7");
    let twice = positions_of(&corpus);
    enumerate_with(&corpus, "5", "8");
    let other = positions_of(&corpus);

    assert_eq!(
        once.lines().count(),
        5,
        "the limit sampled {} of the fixture's positions, so nothing below \
         exercises the sampler at all",
        once.lines().count()
    );
    assert_eq!(
        once, twice,
        "two enumerations at the same seed wrote different position files. \
         core.md §7 makes the corpus a frozen artifact, and one that cannot be \
         regenerated byte for byte is one no truth file can be re-collected \
         against"
    );
    assert_ne!(
        once, other,
        "two enumerations at different seeds wrote the same position file, so \
         --seed decides nothing and the sample is whatever the walk order was"
    );
}

/// §7's command line, parsed out of the document and compared against the one
/// `clap` builds.
///
/// The section prints three usage lines and then says of them: "**There is no
/// `--held-out` flag**, and there must not be. Held-out is selected by passing
/// a different `--corpus` path, so a session that is not given the path cannot
/// reach the data. A flag is something a loop can set; a path it was never told
/// is not." That is a claim about a flag's *absence*, and the only honest way
/// to hold one is to pin the whole set: a test naming `--held-out` would pass
/// while any other reachable flag was added.
///
/// The document is the fixture rather than a transcription, because this is one
/// of the two shapes where editing the *document* has to fail — the way of
/// faking progress the audit cannot catch. A flag added to `cli.rs` and not to
/// §7 fails here, and so does the reverse.
#[test]
fn the_command_line_is_section_7s_and_admits_no_flag_it_does_not_name() {
    let section = &CORE_MD[CORE_MD
        .find("### The command line")
        .expect("core.md §7 prints the command line")..];
    let usage = between(section, "```\n", "```");

    let mut documented: Vec<(String, Vec<String>)> = Vec::new();
    for line in usage.lines() {
        if let Some(rest) = line.split_once("measure-<lang> ") {
            let name = rest.1.split_whitespace().next().unwrap_or_default();
            documented.push((name.to_owned(), Vec::new()));
        }
        let Some((_, flags)) = documented.last_mut() else {
            continue;
        };
        flags.extend(
            line.split_whitespace()
                // `[--repo <name>]...` and `[--restart]`: the brackets are the
                // usage line saying optional, not part of the flag.
                .filter_map(|word| word.trim_start_matches('[').strip_prefix("--"))
                .map(|flag| {
                    flag.trim_end_matches(|scalar: char| {
                        !scalar.is_ascii_lowercase() && scalar != '-'
                    })
                    .to_owned()
                }),
        );
    }
    for (_, flags) in &mut documented {
        flags.sort();
        flags.dedup();
    }
    assert_eq!(
        documented.len(),
        3,
        "§7 prints one usage line per stage of data-collection.md and this \
         found {}: {documented:?}",
        documented.len()
    );

    let command = <measure_core::Cli as clap::CommandFactory>::command();
    let mut built: Vec<(String, Vec<String>)> = command
        .get_subcommands()
        .map(|sub| {
            let mut flags: Vec<String> = sub
                .get_arguments()
                .filter_map(clap::Arg::get_long)
                // `clap` adds this one; §7 is about the flags that decide what
                // a run reads and writes.
                .filter(|long| *long != "help")
                .map(str::to_owned)
                .collect();
            flags.sort();
            (sub.get_name().to_owned(), flags)
        })
        .collect();
    built.sort();
    documented.sort();

    assert_eq!(
        built, documented,
        "the binary's flags and §7's usage lines disagree. The section's rule \
         that held-out is a path and never a flag holds only if the flag set is \
         exactly the printed one — a loop can set any flag it can see, and it \
         cannot set a corpus path it was never told"
    );
}

/// The rest of what §7's flags are chosen to give, which is about their
/// defaults rather than their names: "**`--corpus <dir>` is required and has no
/// default.** A defaulted corpus path is one that eventually points at the
/// wrong one"; `--limit` defaults to 20 000; "Resuming is the default;
/// `--restart` discards a partial truth file, which is the destructive option
/// and therefore the explicit one"; and with no `--records` a replay "**writes
/// nothing**".
#[test]
fn the_flags_defaults_are_the_ones_section_7_argues_for() {
    for stage in ["enumerate", "collect", "replay"] {
        let missing = measure_core::Cli::try_parse_from(["measure-test", stage])
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| format!("`{stage}` parsed with no --corpus at all"));
        assert!(
            missing.contains("--corpus"),
            "`{stage}` did not require --corpus: {missing}. A defaulted corpus \
             path is one that eventually points at the wrong one, and held-out \
             isolation is that path being one a session was never given"
        );
    }

    let enumerate = measure_core::Cli::parse_from(["measure-test", "enumerate", "--corpus", "x"]);
    match enumerate.command {
        measure_core::Command::Enumerate(arguments) => {
            assert_eq!(
                arguments.limit, 20_000,
                "data-collection.md §3's default sample size"
            );
        }
        measure_core::Command::Collect(_) | measure_core::Command::Replay(_) => {
            panic!("`enumerate` parsed as another subcommand")
        }
    }

    let collect = measure_core::Cli::parse_from([
        "measure-test",
        "collect",
        "--corpus",
        "x",
        "--server",
        "oracle",
    ]);
    match collect.command {
        measure_core::Command::Collect(arguments) => {
            assert!(
                !arguments.restart,
                "resuming is the default, because discarding a partial truth \
                 file is the destructive option and therefore the explicit one"
            );
        }
        measure_core::Command::Enumerate(_) | measure_core::Command::Replay(_) => {
            panic!("`collect` parsed as another subcommand")
        }
    }

    let replay = measure_core::Cli::parse_from([
        "measure-test",
        "replay",
        "--corpus",
        "x",
        "--server",
        "oracle",
    ]);
    match replay.command {
        measure_core::Command::Replay(arguments) => {
            assert_eq!(arguments.format, measure_core::Format::Table);
            assert!(
                arguments.records.is_none(),
                "with no --records a replay writes nothing, which is what keeps \
                 the default a pure function of its inputs and measure_core \
                 ignorant of where a harness would put a digest"
            );
        }
        measure_core::Command::Enumerate(_) | measure_core::Command::Collect(_) => {
            panic!("`replay` parsed as another subcommand")
        }
    }
}

/// §7's command line: "`collect` drives the server named in `servers.toml`,
/// which carries its command and pinned version… Naming a server rather than
/// passing a command line is what lets the provenance header record what was
/// actually run without trusting the invocation to be repeated correctly."
///
/// Against the manifest **in the repository**, which is the whole assertion.
/// `conformance-010`'s hand-reader accepted `[[server]]` with one `key = value`
/// per line; `servers.toml` is `[server.<name>]` tables with multi-line arrays,
/// so `collect` could not resolve a single server — and nothing failed, because
/// the reader had no test and a test with a fixture of its own would have gone
/// on passing. The ruling took `toml` for exactly this.
#[test]
fn a_server_resolves_against_the_manifest_in_the_repository() {
    let rust = LanguageId::new("rust");

    let server =
        measure_core::resolve_server(rust, "rust-analyzer").expect("servers.toml names it");
    assert_eq!(&*server.name, "rust-analyzer");
    assert!(
        server.version.starts_with("rust-analyzer "),
        "the version compared on a resume is what the installed server reports, \
         and this is {}",
        server.version
    );
    let command = server.command.join(" ");
    assert!(
        command.ends_with("rust-analyzer") && !command.contains("${servers}"),
        "the command is a command line and not a template: ${{servers}} expands \
         to servers_root, resolved relative to the manifest, so the install \
         tree relocates without editing an entry. Got {command}"
    );

    // Named, but for another language. A rust binary given the go server would
    // collect a truth file no handler can be scored against, and the provenance
    // header would record it as though it could.
    let wrong_language = measure_core::resolve_server(rust, "gopls")
        .expect_err("gopls answers for go, and this asked as rust");
    let Error::Config(shared::ConfigError::UnknownServer { name, .. }) = &wrong_language else {
        panic!("another language's server was refused as {wrong_language}");
    };
    assert_eq!(&**name, "gopls");

    let unknown = measure_core::resolve_server(rust, "no-such-server")
        .expect_err("servers.toml names no such server");
    assert!(
        matches!(
            unknown,
            Error::Config(shared::ConfigError::UnknownServer { .. })
        ),
        "an unnamed server was refused as {unknown}"
    );
}

/// §7: "`truth.jsonl` carries its provenance in a header record: repository
/// path and commit, server name and version, **grammar revision**, and the
/// `measure` version that wrote it."
///
/// A literal in that field is the failure mode the whole header exists to
/// prevent: two collections under different grammar pins produce identical
/// provenance, so nothing downstream can tell that the positions were
/// enumerated by a different parser. The pin is therefore read out of the
/// lockfile, and this is the assertion that two lockfiles cannot produce one
/// pin — which no amount of reading a constant would give.
#[test]
fn two_grammar_pins_produce_two_headers() {
    let rust = LanguageId::new("rust");

    let one = measure_core::locked_grammar(LOCKED_ONE, rust).expect("a locked grammar");
    let two = measure_core::locked_grammar(LOCKED_TWO, rust).expect("a locked grammar");

    assert!(
        one.contains("0.24.2") && one.contains("439e577dbe07423e"),
        "a pin that names neither the locked version nor its revision is not a \
         grammar revision: {one}"
    );
    assert_ne!(
        one, two,
        "two lockfiles pinning different revisions of the same grammar produced \
         the same provenance header. core.md §7 puts the grammar revision in \
         that header so a truth file says which parser enumerated it, and a \
         value that does not move when the pin moves says nothing"
    );
}

/// The other half: the pin this build would actually write names the grammar
/// the workspace declares. Asserted against `Cargo.toml` rather than against
/// the lockfile the crate embeds, so the two would have to drift together —
/// reading the same file the implementation reads would assert nothing.
#[test]
fn the_shipped_grammar_pin_names_the_declared_grammar() {
    let declared = between(WORKSPACE_MANIFEST, "tree-sitter-rust = \"", "\"");
    assert!(
        !declared.is_empty(),
        "the workspace declares no tree-sitter-rust"
    );

    let pin = measure_core::grammar_pin(LanguageId::new("rust")).expect("the locked grammar");
    assert!(
        pin.contains("tree-sitter-rust") && pin.contains(declared),
        "the provenance header would name {pin}, and the workspace pins \
         tree-sitter-rust {declared}. deps.md §6 makes the grammar the one \
         dependency that is not ours to pick, so a header that names a \
         different one is a truth file attributed to the wrong parser"
    );

    let missing = measure_core::grammar_pin(LanguageId::new("fortran"))
        .expect_err("no tree-sitter-fortran is locked");
    let Error::Config(shared::ConfigError::GrammarNotLocked { package }) = &missing else {
        panic!("an unlocked grammar reported {missing} rather than naming itself");
    };
    assert_eq!(&**package, "tree-sitter-fortran");
}

/// §7: "replay refuses to run against a truth file whose repository commit
/// does not match the checkout, rather than silently reporting metrics for
/// positions that have since moved" — which a resume could defeat from the
/// other side, by writing the current `HEAD` into the header of a file whose
/// rows were collected at an older one.
///
/// The loop is over *every* field the header carries, because the check that
/// was there compared one of them. A field added to `Provenance` and not to
/// `drift` fails to compile in the implementation; a field added here and not
/// mutated fails this test.
#[test]
fn a_resume_refuses_every_provenance_field_that_moved() {
    let corpus = fixture("resume_drift");
    enumerate(&corpus);
    write_truth(&corpus);
    let path = truth_path(&corpus, "oracle");

    measure_core::check_resumable(&path, &fixture_provenance(&corpus))
        .expect("the header this run would write is the header already on disk");

    /// Named so the table below reads as what it is: one field of the header,
    /// moved.
    type Moved = fn(&mut measure_core::Provenance);

    let moved: [(&str, Moved); 7] = [
        ("repository", |header| header.repository = "two".into()),
        ("commit", |header| header.commit = "0".repeat(40).into()),
        ("language", |header| header.language = "python".into()),
        ("server", |header| header.server = "other".into()),
        ("server_version", |header| {
            header.server_version = "1".into();
        }),
        ("grammar", |header| {
            header.grammar = "tree-sitter-rust 9.9.9 (deadbeef)".into();
        }),
        ("measure_version", |header| {
            header.measure_version = "9".into();
        }),
    ];

    for (field, moved) in moved {
        let mut wanted = fixture_provenance(&corpus);
        moved(&mut wanted);

        let refused = measure_core::check_resumable(&path, &wanted)
            .expect_err("a resume against a header this run would not have written was allowed");
        let Error::Config(shared::ConfigError::ProvenanceDrift { field: named, .. }) = &refused
        else {
            panic!("{field} drift was reported as {refused}, not as provenance drift");
        };
        assert_eq!(
            *named, field,
            "a resume whose {field} moved was refused for {named} instead. Half \
             a file collected under one provenance and half under another is \
             the one outcome with no honest header, and which field moved is \
             what tells the operator whether to re-collect or to fix the \
             checkout"
        );
    }
}

/// §7: "**replay refuses to run against a truth file whose repository commit
/// does not match the checkout**, rather than silently reporting metrics for
/// positions that have since moved", and "a partially collected truth file is
/// marked incomplete and is never consumed by replay".
///
/// [`a_resume_refuses_every_provenance_field_that_moved`] holds the *collect*
/// side of the first one — a resume that would write a header over rows
/// gathered under another. This is the replay side, which is the one the
/// sentence is actually about and which that test cannot reach: it drives
/// `check_resumable`, and a replay does not call it.
///
/// Three refusals, because a truth file can be untrustworthy in three ways and
/// two of them do not touch the header at all. The third is the one
/// `data-collection.md` §1 says matters most: an untracked file changes byte
/// offsets and **does not change `HEAD`**, so a checkout that passes the commit
/// check can still be a checkout the recorded offsets do not describe.
#[test]
fn a_replay_refuses_a_truth_file_it_cannot_trust() {
    let moved = fixture("commit_moved");
    enumerate(&moved);
    let elsewhere = "0".repeat(40);
    write_truth_headed(&moved, "oracle", &header("oracle", &elsewhere, true));

    let refused = replay_result(&moved, measure_core::Format::Json)
        .expect_err("a truth file collected at another commit was replayed");
    let Error::Config(shared::ConfigError::CommitMismatch {
        expected, found, ..
    }) = &refused
    else {
        panic!("a truth file from another commit was refused as {refused}");
    };
    assert_eq!(
        (&**expected, &**found),
        (&*elsewhere, &*moved.commit),
        "the refusal names the wrong pair of commits, and which one is the \
         file's is what tells an operator whether to re-collect or to check out"
    );

    let partial = fixture("incomplete_truth");
    enumerate(&partial);
    write_truth_headed(
        &partial,
        "oracle",
        &header("oracle", &partial.commit, false),
    );

    let refused = replay_result(&partial, measure_core::Format::Json)
        .expect_err("a truth file whose collection never finished was replayed");
    assert!(
        matches!(
            refused,
            Error::Config(shared::ConfigError::ArtifactIncomplete { .. })
        ),
        "a half-collected truth file was refused as {refused}. Its rows stop \
         wherever the run died, so every stratum below that point reads as a \
         corpus the oracle answered less of — a regression that never happened"
    );

    let dirty = fixture("dirty_checkout");
    enumerate(&dirty);
    write_truth(&dirty);
    fs::write(
        dirty
            .split
            .join("rust")
            .join("repos")
            .join("one")
            .join("src")
            .join("extra.rs"),
        "pub fn gamma() {}\n",
    )
    .expect("an untracked file");

    let refused = replay_result(&dirty, measure_core::Format::Json)
        .expect_err("a dirty checkout was replayed");
    assert!(
        matches!(
            refused,
            Error::Config(shared::ConfigError::DirtyCheckout { .. })
        ),
        "an untracked file in the checkout was refused as {refused}. It does \
         not move HEAD and it is not gitignored, so the file list finds a file \
         the truth has never heard of — and an edit to a tracked one moves \
         every byte offset after it while the commit check still passes"
    );
}

/// The replay-side half of "a truth file is never silently merged with
/// another's" (§7). `truth/<server>/` is a path and not a check: a file copied
/// into it replays under a server name it was never collected against, and
/// every metric is then attributed to the wrong oracle.
#[test]
fn a_replay_refuses_a_truth_file_collected_against_another_server() {
    let corpus = fixture("provenance_mismatch");
    enumerate(&corpus);
    write_truth_as(&corpus, "oracle", "other");

    let refused = replay_result(&corpus, measure_core::Format::Json)
        .expect_err("a truth file whose header names another server was replayed");
    let Error::Config(shared::ConfigError::ProvenanceDrift { field, .. }) = &refused else {
        panic!("the wrong oracle's truth file was refused as {refused}");
    };
    assert_eq!(*field, "server");
}

/// §7's "the table is not enough": `replay --records <path>` writes the
/// per-query JSONL "unchanged and unfiltered", and digesting those into
/// something readable is the harness's job — the same split that keeps
/// `measure_core` ignorant of `state/`.
///
/// Unfiltered is the half that can be lost silently. A `--records` that wrote
/// only the failures would be smaller, would look right, and would make every
/// group's *share of its stratum* wrong — and the share is what the section
/// says turns a thousand failures into a finding. Here every row is a mutual
/// "no definition here", so a file filtered to anything would be empty.
#[test]
fn the_records_file_holds_every_replayed_query_and_not_only_the_failures() {
    let corpus = fixture("unfiltered_records");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let truth = fs::read_to_string(truth_path(&corpus, "oracle")).expect("the truth file");
    // Every row but the provenance header, since the fixture's oracle answered
    // on all of them and none is `error` or `timeout`.
    let replayed = truth.lines().count() - 1;

    assert!(replayed > 1, "the fixture enumerated {replayed} positions");
    assert_eq!(
        text.lines().count(),
        replayed,
        "replay wrote {} records for {replayed} queries. core.md §7 makes the \
         records the *unfiltered* per-query JSONL, because the digest leads \
         with each group's count and its share of its stratum, and a share \
         computed over a filtered file is wrong in a way nothing downstream \
         can see",
        text.lines().count()
    );
}

/// §7's "the table is not enough" names the digest's key and says it is
/// "available mechanically": abstentions by `(stratum_prior, reason, stages)`.
///
/// Two things about that key are the measurement's to hold, and neither was.
/// The **reason** is not a column — `abstain_label` folds it into `stages`,
/// because `stages` is the field §7 makes the handler's account of what it did
/// and a second reason column would be two vocabularies for one question — so a
/// digest that could not find it there would have no coverage key at all. And
/// the grouping is "an exact string group-by rather than … anybody's judgement
/// about similarity", which holds only if two queries that abstained the same
/// way produce the *same* log and two that abstained differently produce
/// different ones.
///
/// The template handler is what makes this assertable without a language: it
/// abstains `NotAnIdentifier` where the cursor is not on an identifier and
/// `UnsupportedRole` where it is, so one fixture produces exactly two clusters
/// and the corpus enumerates both classes.
#[test]
fn the_digests_abstention_key_falls_out_of_an_exact_string_group_by() {
    let corpus = fixture("abstention_key");
    enumerate(&corpus);
    write_truth(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let mut clusters: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for line in text.lines() {
        let key = (
            between(line, "\"stratum_prior\":\"", "\""),
            between(line, "\"stages\":[", "]"),
        );
        *clusters.entry(key).or_default() += 1;
    }

    let found: Vec<(&str, &str, usize)> = clusters
        .iter()
        .map(|((stratum, stages), count)| (*stratum, *stages, *count))
        .collect();

    assert_eq!(
        found.len(),
        2,
        "the template abstains for two reasons and an exact group-by found {} \
         cluster(s). core.md §7 keys the coverage half of the digest on \
         (stratum_prior, reason, stages) and says the clusters fall out \
         mechanically; a key that collapses two reasons into one, or splits one \
         into many, is a key that groups nothing.\nclusters: {found:?}",
        found.len()
    );

    for (index, (stratum, stages, count)) in found.iter().enumerate() {
        let expected = [
            "\"abstain:not_an_identifier\"",
            "\"abstain:unsupported_role\"",
        ][index];
        assert_eq!(
            (*stratum, *stages),
            ("unimplemented", expected),
            "an abstention cluster is keyed on {stratum}/{stages}. §7 carries \
             the reason inside `stages` rather than in a column of its own, so \
             a reason that never reaches the record leaves the digest with \
             nothing to group coverage loss by"
        );
        assert!(
            *count > 1,
            "the {stages} cluster holds one query, so this test would pass \
             against a records file with no clusters in it at all"
        );
    }
}

/// The precision half of the same key: mismatches by `(stratum_final,
/// agreement, severity, stages)`, "with `match_contained` kept apart from
/// `mismatch` since they are different problems: one is a ranking failure, the
/// other a candidate-generation failure".
///
/// Every fixture truth row before this one was a `null` answer that the handler
/// abstained on, so `severity` was `null` in every record any test had ever
/// read and `Agreement::classify` was never reached through a replay at all.
/// `crates/shared/tests/agreement.rs` holds the predicate; what was held
/// nowhere is that its verdict *arrives in the record*, which is the only place
/// a digest can read it from.
///
/// Five clusters from one corpus, because the key has to separate what the
/// section says are different problems. Two of them differ only in `severity`,
/// which is the field a record could drop without any other assertion noticing.
#[test]
fn the_digests_precision_key_separates_every_way_an_answer_can_be_wrong() {
    let corpus = fixture("precision_key");
    enumerate(&corpus);
    write_truth_answered(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay_with(&MismatchingHandler, &corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let mut clusters: BTreeMap<(&str, &str, &str, &str), usize> = BTreeMap::new();
    for line in text.lines() {
        let key = (
            between(line, "\"stratum_final\":\"", "\""),
            between(line, "\"agreement\":\"", "\""),
            between(line, "\"severity\":", "}"),
            between(line, "\"stages\":[", "]"),
        );
        *clusters.entry(key).or_default() += 1;
    }

    let found: Vec<(&str, &str, &str, &str)> = clusters.keys().copied().collect();
    assert_eq!(
        found,
        vec![
            (
                "explicitly_imported",
                "match_contained",
                "null",
                "\"ref:Value\""
            ),
            ("explicitly_imported", "match_top1", "null", "\"ref:Value\""),
            (
                "explicitly_imported",
                "mismatch",
                "\"near_module\"",
                "\"ref:Value\""
            ),
            (
                "explicitly_imported",
                "mismatch",
                "\"same_file\"",
                "\"ref:Value\""
            ),
            (
                "explicitly_imported",
                "mismatch",
                "\"unrelated\"",
                "\"ref:Value\""
            ),
        ],
        "the oracle answered five different ways against one ranked list and \
         the record keys them as {} group(s). core.md §7 keys precision loss on \
         (stratum_final, agreement, severity, stages): match_contained is a \
         ranking failure and mismatch a candidate-generation failure, and the \
         three severities are what a divergence budget is spent against, so a \
         key that cannot tell them apart digests them into one finding that \
         names no cause",
        found.len()
    );

    for (key, count) in &clusters {
        assert!(
            *count > 1,
            "the {key:?} cluster holds one query, so an assertion about \
             grouping would hold against a records file with no groups in it"
        );
    }
}

/// §7's "the table is not enough": each group carries "its count, its **share
/// of that stratum**". The count is the digest's, computed from the records
/// file; the denominator is the stratum's, and the stratum's numbers are the
/// table's. So a share means something only if the two artifacts a replay
/// writes are two accounts of the *same* run.
///
/// Nothing joined them, and every way they can drift is silent: a `--records`
/// that dropped a decision, a `Table::observe` that counted a row into the
/// wrong half, a `stratum_prior`/`stratum_final` swap applied to one side only.
/// Each leaves a table that reads fine sitting beside a digest whose every
/// share is wrong, and the digest is the artifact a tuning campaign acts on.
///
/// Three handlers, because no one run reaches all seven counters: the template
/// only abstains, [`MismatchingHandler`] commits into all three agreement
/// counters, and a handler that returns `Err` is the only source of `failed`.
#[test]
fn the_records_and_the_table_are_the_same_run_counted_twice() {
    let corpus = fixture("records_reconcile");
    enumerate(&corpus);
    write_truth_answered(&corpus);

    // A reconciliation of zero against zero holds trivially, so every counter
    // has to be reached by at least one of the three runs.
    let mut exercised: BTreeMap<&str, u64> = BTreeMap::new();

    for handler in [
        &TestHandler as &dyn LanguageHandler,
        &MismatchingHandler,
        &FailingHandler,
    ] {
        let records = corpus.scratch.join("records.jsonl");
        replay_with(handler, &corpus, Some(&records));
        let report = replay_report(handler, &corpus, measure_core::Format::Json);
        let text = fs::read_to_string(&records).expect("replay wrote the records file");

        // The rule that makes the reconciliation above possible at all, and the
        // one a replay got wrong: §6 classifies *the shim's answer* against the
        // child's, so a query the shim never answered "has no answer of ours to
        // compare, which is a different fact from the two sides disagreeing".
        // Classifying one anyway reads as `mismatch` on every abstention the
        // oracle answered — a precision loss where §7 counts a coverage loss,
        // and a divergence report to a user who was sent nowhere at all.
        assert_eq!(
            tally(&text, |line| decision_of(line) != "committed"
                && !(agreement_of(line).is_empty()
                    && between(line, "\"severity\":\"", "\"").is_empty())),
            0,
            "a record the handler did not answer carries an oracle verdict. \
             core.md §6 makes agreement and severity properties of an answer, \
             and a table that judges only commits beside a records file that \
             judges everything is two accounts of one run that cannot both be \
             right\n{text}"
        );

        for stratum in STRATUM_NAMES {
            for (field, counted) in [
                ("queries", tally(&text, |line| prior_of(line) == stratum)),
                (
                    "committed",
                    tally(&text, |line| {
                        prior_of(line) == stratum && decision_of(line) == "committed"
                    }),
                ),
                (
                    "abstained",
                    tally(&text, |line| {
                        prior_of(line) == stratum && decision_of(line) == "abstained"
                    }),
                ),
                (
                    "failed",
                    tally(&text, |line| {
                        prior_of(line) == stratum && decision_of(line) == "failed"
                    }),
                ),
                (
                    "match_top1",
                    tally(&text, |line| {
                        settled_of(line) == stratum && agreement_of(line) == "match_top1"
                    }),
                ),
                (
                    "match_contained",
                    tally(&text, |line| {
                        settled_of(line) == stratum && agreement_of(line) == "match_contained"
                    }),
                ),
                (
                    "mismatch",
                    tally(&text, |line| {
                        settled_of(line) == stratum && agreement_of(line) == "mismatch"
                    }),
                ),
            ] {
                assert_eq!(
                    counted,
                    reported(&report, stratum, field),
                    "the records file counts {counted} for {stratum}/{field} and \
                     the table reports {}. core.md §7's digest gives every group \
                     its count and its share of that stratum: the count comes \
                     from the records and the denominator from the table, so two \
                     artifacts of one replay that disagree make every share in \
                     the digest wrong with nothing downstream able to see it",
                    reported(&report, stratum, field)
                );
                *exercised.entry(field).or_default() += counted;
            }
        }
    }

    for (field, total) in &exercised {
        assert!(
            *total > 0,
            "no run reached {field}, so the equality asserted for it above was \
             zero against zero and holds against a records file and a table that \
             share nothing at all.\ncounters: {exercised:?}"
        );
    }
}

/// The reconciliation above is against `--format json`, which is what the
/// harness consumes. A person reads the other one, and nothing said they hold
/// the same numbers — so a digest's shares could have been computed from a
/// denominator nobody could see.
///
/// The two renderings do not carry the same columns, which is why this is not
/// a string comparison: the text table prints `coverage` and `precision` as
/// percentages where the JSON carries the three agreement counters they are
/// computed from. Recomputing them here is the assertion that matters, because
/// it is the one place §7's two-field stratum is visible in a *rendering*:
/// coverage is reported on `stratum_prior` and precision on `stratum_final`, so
/// a handler that refines puts the two halves of one query in two rows.
///
/// [`ReportingHandler`] against a truth file the oracle answered `null`
/// everywhere is exactly that case, and it is what makes the check bite. Its
/// prior is `explicitly_imported` and its settled stratum `ambiguous_name`, and
/// an empty commit against a `null` answer is the mutual "no definition here"
/// §6 calls a match. So one row is all coverage and no judgement and the other
/// is all judgement and no coverage — and `Row::precision`'s denominator, which
/// its doc comment argues at length must be the three agreement counters and
/// not `committed`, is the difference between 100% and 0% in the second row.
#[test]
fn the_printed_table_and_the_json_report_are_one_table() {
    let corpus = fixture("one_table");
    enumerate(&corpus);
    write_truth(&corpus);

    let printed = replay_report(&ReportingHandler, &corpus, measure_core::Format::Table);
    let report = replay_report(&ReportingHandler, &corpus, measure_core::Format::Json);

    let replayed = reported(&report, "explicitly_imported", "queries");
    assert!(replayed > 1, "the fixture replayed {replayed} queries");
    assert_eq!(
        reported(&report, "ambiguous_name", "match_top1"),
        replayed,
        "the refined half of every query should be judged under the stratum the \
         search settled on, and the fixture is built so that all of them match"
    );
    assert_eq!(
        reported(&report, "ambiguous_name", "queries"),
        0,
        "the refined stratum should carry no coverage denominator at all, which \
         is what makes the precision denominator below distinguishable from \
         `committed`"
    );

    let mut rows = 0;
    for line in printed.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        let [
            stratum,
            queries,
            committed,
            abstained,
            failed,
            coverage,
            precision,
            contained,
        ] = columns[..]
        else {
            continue;
        };
        if !STRATUM_NAMES.contains(&stratum) {
            continue;
        }
        rows += 1;

        let counted = |field: &str| reported(&report, stratum, field);
        for (column, printed, field) in [
            ("queries", queries, "queries"),
            ("commit", committed, "committed"),
            ("abstain", abstained, "abstained"),
            ("fail", failed, "failed"),
            ("contained", contained, "match_contained"),
        ] {
            assert_eq!(
                printed.parse::<u64>().expect("a printed count"),
                counted(field),
                "the {stratum} row prints {printed} for {column} and the report \
                 carries {} for {field}. §7's command line offers both formats \
                 of one table; two renderings that disagree mean a gate reading \
                 the JSON and a person reading the text are looking at different \
                 runs",
                counted(field)
            );
        }

        assert_eq!(
            coverage.trim_end_matches('%'),
            percent(counted("committed"), counted("queries")),
            "the {stratum} row prints {coverage} coverage over {} committed of \
             {} queries. §7 reports coverage on stratum_prior so the denominator \
             is fixed by the reference and does not move when the implementation \
             changes",
            counted("committed"),
            counted("queries")
        );
        assert_eq!(
            precision.trim_end_matches('%'),
            percent(
                counted("match_top1"),
                counted("match_top1") + counted("match_contained") + counted("mismatch"),
            ),
            "the {stratum} row prints {precision} precision. §7 reports precision \
             on stratum_final, so on a refined query the coverage and the \
             judgement live in different rows and `committed` is the wrong row's \
             number — which is why the denominator is the three agreement \
             counters"
        );
    }

    assert_eq!(
        rows,
        STRATUM_NAMES.len(),
        "the printed table has {rows} of the nine strata, so the reconciliation \
         above skipped rows the JSON carries\n{printed}"
    );
}

/// §7's "the table is not enough" on what a group shows after its count and its
/// share: "a **small seeded sample** of concrete cases — repository, file, line,
/// the identifier, what we returned, what the server said".
///
/// Not one of those six is a column of the record, and the line deliberately
/// cannot be: §7 makes `position` a byte offset so that the two halves of the
/// metric join exactly, and says a line/column pair "would need a conversion in
/// the one place the two halves of the metric have to line up exactly". So what
/// the measurement owes the digest here is not a column but a *reachability* —
/// all six assemblable from the records file, the corpus artifacts beside it,
/// and the checkout a replay already refuses to run against unless it is clean.
///
/// The repository holds two files whose identifiers sit at the same byte
/// offsets and spell different things, so the join that produces the identifier
/// has to be on `(file, offset)`. Every other fixture here is one file, and a
/// one-file repository cannot tell that join from one on the offset alone —
/// which would name a real identifier from the wrong file, and read as a
/// finding rather than as a bug.
#[test]
fn a_digest_group_names_a_case_a_person_can_open() {
    let corpus = fixture_of(
        "digest_sample",
        &[("src/lib.rs", SOURCE), ("src/other.rs", OTHER_SOURCE)],
    );
    enumerate(&corpus);
    write_truth_answered(&corpus);

    let records = corpus.scratch.join("records.jsonl");
    replay_with(&MismatchingHandler, &corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    let positions = positions_of(&corpus);
    let repositories = corpus.split.join("rust").join("repos");

    let mut groups: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut sampled: BTreeMap<String, usize> = BTreeMap::new();

    for line in text.lines() {
        // Repository and file. The record names an absolute uri; the corpus
        // layout is `<split>/<language>/repos/<name>/<file>`, which is what a
        // digest already has and what the record therefore does not repeat.
        let uri =
            shared::DocumentUri::parse(between(line, "\"uri\":\"", "\"")).expect("a record's uri");
        let path = uri.to_file_path().expect("a record naming a file");
        let inside = path
            .strip_prefix(&repositories)
            .expect("a record naming a file outside the corpus it was replayed from");
        let repository = inside
            .components()
            .next()
            .expect("a repository directory")
            .as_os_str()
            .to_string_lossy()
            .into_owned();
        let file = inside
            .strip_prefix(&repository)
            .expect("a file inside its repository")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            repository, "one",
            "the record named a repository the corpus does not hold"
        );

        // What we returned and what the server said, both already spelled
        // `uri:line` by `shared::record` so the two sides of a sample case
        // cannot describe different things.
        for (field, answer) in [
            ("heuristic_locations", "what we returned"),
            ("lsp_locations", "what the server said"),
        ] {
            assert!(
                !between(line, &format!("\"{field}\":["), "]").is_empty(),
                "a sample case has no {answer}. §7's digest shows both sides of \
                 every case it names, and a group whose examples show one side \
                 is a count with an anecdote attached\nrecord: {line}"
            );
        }

        // The identifier, joined to the corpus on (file, offset): the position
        // file recorded the text at enumeration and the record carries the
        // offset it was queried at.
        let offset: usize = between(line, "\"position\":", ",")
            .parse()
            .expect("a byte offset");
        let joined = positions
            .lines()
            .find(|position| {
                between(position, "\"file\":\"", "\"") == file
                    && between(position, "\"offset\":", ",") == offset.to_string()
            })
            .unwrap_or_else(|| panic!("no corpus position for {file} at {offset}"));

        // A non-identifier probe's `text` is one escaped scalar and the field
        // §7 names is "the identifier", so the join is asserted where the
        // corpus says there is one to name.
        if between(joined, "\"class\":\"", "\"") != "identifier" {
            continue;
        }
        let identifier = between(joined, "\"text\":\"", "\"");

        // The line, counted in the checkout — which is sound because a replay
        // refuses a dirty one, so the bytes the offsets describe are the bytes
        // on disk.
        let source = fs::read_to_string(&path).expect("the file the record names");
        let number = source[..offset].matches('\n').count();
        let opened = source
            .lines()
            .nth(number)
            .expect("the line the offset falls on");
        assert!(
            opened.contains(identifier),
            "a sample case names {file}:{number} and `{identifier}`, and that \
             line reads `{opened}`. §7's sample exists to make a group concrete, \
             and a case whose file, line and identifier do not describe one place \
             is worse than no case at all — the offset is the record's and the \
             identifier is the corpus's, so a join on the offset alone finds a \
             real identifier from the wrong file"
        );

        *groups
            .entry((settled_of(line), agreement_of(line)))
            .or_default() += 1;
        *sampled.entry(file).or_default() += 1;
    }

    assert_eq!(
        sampled.len(),
        2,
        "the cases came from {} of the repository's two files, so the join that \
         produced them is not distinguishable from one on the offset alone — \
         which is what this fixture's second file exists to distinguish.\n\
         files: {sampled:?}",
        sampled.len()
    );
    assert!(
        groups.len() > 1,
        "every case fell in one group, so `each group carries … a sample` is \
         asserted over a single group.\ngroups: {groups:?}"
    );
}

/// The other half of that flag: "with no `--records` it **writes nothing**, so
/// the default stays a pure function of its inputs and `measure_core` still
/// needs no knowledge of `state/`".
///
/// Asserted over the whole corpus tree rather than the one path the flag would
/// have written, because "writes nothing" is the claim and a run that dropped
/// a file somewhere else would satisfy the narrower reading. Paths rather than
/// contents: `verify_checkout` runs `git status`, which refreshes the index.
#[test]
fn a_replay_given_no_records_path_writes_nothing() {
    let corpus = fixture("no_records");
    enumerate(&corpus);
    write_truth(&corpus);

    let before = files_under(&corpus.root);
    let table = table_of(&corpus, measure_core::Format::Json);
    assert!(
        table.contains("unimplemented"),
        "the replay produced no table, so the comparison below would hold \
         whatever it wrote.\n{table}"
    );

    assert_eq!(
        before,
        files_under(&corpus.root),
        "a replay with no --records changed what is on disk. core.md §7 makes \
         the default a pure function of its inputs, which is what lets the \
         table be a gate rather than a report and keeps measure_core ignorant \
         of where a harness would put a digest"
    );
}

/// §7's command line: "Exit status is about whether the run happened, not about
/// whether the numbers are good: `replay` exits zero having printed a table
/// full of zeroes. Judging the table is the gate's job, not the measurement's."
///
/// A corpus the oracle never answered is the sharpest form of that, and it is
/// the one every other test in this file is the opposite of. It also holds the
/// two rules that only fire here: `error` and `timeout` are not ground truth,
/// so they are excluded from the metrics and reported beside them as a coverage
/// figure for the *collection* — a quality signal about the repository's build
/// setup rather than about the handler — and `nothing measured` is a third
/// template state rather than `replaced`, since an empty table is not evidence
/// the placeholder is gone.
#[test]
fn a_corpus_the_oracle_never_answered_is_a_table_of_zeroes_and_a_zero_exit() {
    let corpus = fixture("uncollected");
    enumerate(&corpus);
    let rows = write_truth_uncollected(&corpus);
    assert!(rows > 1, "the fixture enumerated {rows} positions");

    let report = replay_result(&corpus, measure_core::Format::Json)
        .expect("a replay whose oracle answered nothing still ran");

    assert_eq!(
        report.matches("\"queries\": 0").count(),
        9,
        "the nine strata should all be zero and {} of them are. core.md §7 \
         excludes error and timeout rows from the metrics, so a corpus the \
         oracle never answered measures nothing rather than measuring badly\n\
         {report}",
        report.matches("\"queries\": 0").count()
    );
    for expected in [
        format!("\"uncollected\": {rows}"),
        // The JSON spelling, which is `serde`'s snake_case rather than the
        // text table's "nothing measured": one is what the harness matches on
        // and the other is what a person reads.
        "\"template\": \"nothing_measured\"".to_owned(),
    ] {
        assert!(
            report.contains(&expected),
            "the report does not carry {expected}. The uncollected count is a \
             quality signal about the corpus and is never folded into the \
             table, and an empty table read as `replaced` would pass a gate on \
             every corpus that measured nothing at all\n{report}"
        );
    }

    let printed = table_of(&corpus, measure_core::Format::Table);
    assert!(
        printed.contains(&format!("positions the oracle never answered: {rows}")),
        "the printed table dropped the uncollected count, which is the figure \
         that tells a reader whether a table of zeroes is a handler that \
         resolved nothing or a collection that never happened\n{printed}"
    );
}

#[test]
fn a_run_given_one_split_cannot_reach_its_sibling() {
    let corpus = fixture("isolation");
    enumerate(&corpus);
    write_truth(&corpus);

    // The sibling exists and holds a repository this run must never see.
    let held_out = corpus.root.join("test").join("rust").join("repos");
    fs::create_dir_all(held_out.join("secret")).expect("the held-out split");

    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));

    let text = fs::read_to_string(&records).expect("replay wrote the records file");
    assert!(
        !text.contains("/test/") && !text.contains("secret"),
        "a replay given the training split named something under its sibling. \
         core.md §7 makes test/ a sibling of training/ rather than a \
         subdirectory precisely so that this is a path check rather than a \
         convention, and loops.md §12 relies on it being a filesystem boundary"
    );
}

#[test]
fn every_non_identifier_position_is_one_the_handler_also_declines() {
    let corpus = fixture("identifier_rule");
    enumerate(&corpus);

    let path = corpus
        .split
        .join("rust")
        .join("positions")
        .join("one.jsonl");
    let text = fs::read_to_string(&path).expect("enumerate wrote positions");

    let identifiers = text.matches("\"class\":\"identifier\"").count();
    assert!(
        identifiers >= 4,
        "the fixture declares alpha, beta, u32 and a call, and enumerate found \
         {identifiers} identifier positions in it"
    );
    assert!(
        text.contains("\"class\":\"other\""),
        "no non-identifier positions were enumerated, so the NotAnIdentifier \
         path has nothing in the corpus to fire on (data-collection.md §2)"
    );

    // The claim the test is named for. `core.md`'s template section: "That rule
    // is one function in `shared`, not two implementations that agree… and if
    // those two ever disagree, the corpus contains positions the tool does not
    // consider queries, or the reverse, and the resulting miscount looks like a
    // resolution failure rather than a definitional one."
    //
    // One function, two entry points, and they do *not* look alike: enumeration
    // walks the tree with `shared::identifiers` and the handler asks
    // `named_descendant_for_byte_range` through `shared::identifier_at`. That
    // they share a private predicate is not enough on its own — the lookups
    // could still select different nodes at the same offset — so this is the
    // join, position by position: the corpus wrote a class and the handler
    // reached an abstention reason, at the same offset, through the other door.
    write_truth(&corpus);
    let records = corpus.scratch.join("records.jsonl");
    replay(&corpus, Some(&records));
    let replayed = fs::read_to_string(&records).expect("replay wrote the records file");

    let mut checked = 0;
    for line in text.lines() {
        let offset = between(line, "\"offset\":", ",");
        let class = between(line, "\"class\":\"", "\"");
        let record = replayed
            .lines()
            .find(|record| between(record, "\"position\":", ",") == offset)
            .unwrap_or_else(|| panic!("position {offset} was enumerated and never replayed"));

        let expected = match class {
            "identifier" => "abstain:unsupported_role",
            "other" => "abstain:not_an_identifier",
            unknown => panic!("position {offset} was enumerated as {unknown}"),
        };
        assert!(
            record.contains(expected),
            "the corpus enumerated position {offset} as `{class}` and the \
             handler reached it through the other entry point and disagreed. \
             The two are meant to be one function: a disagreement puts \
             positions in the corpus the tool does not consider queries, and \
             the miscount reads as a resolution failure rather than a \
             definitional one.\nrecord: {record}"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        text.lines().count(),
        "the join skipped positions, so it holds less than it reads as"
    );
}

struct Fixture {
    root: PathBuf,
    split: PathBuf,
    scratch: PathBuf,
    commit: String,
}

/// A corpus root with the layout `data-collection.md` §0 describes, holding
/// one repository at a known commit with a clean tree.
fn fixture(name: &str) -> Fixture {
    fixture_of(name, &[("src/lib.rs", SOURCE)])
}

/// The same, with the repository's files spelled out.
///
/// A parameter for one test: a digest's sample joins a record back to the
/// corpus on `(file, offset)`, and a single-file repository cannot tell that
/// join from one on `offset` alone.
fn fixture_of(name: &str, sources: &[(&str, &str)]) -> Fixture {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    let split = root.join("training");
    let repository = split.join("rust").join("repos").join("one");
    for (relative, text) in sources {
        let path = repository.join(relative);
        fs::create_dir_all(path.parent().expect("a source directory"))
            .expect("the fixture repository");
        fs::write(&path, text).expect("the fixture source");
    }

    let scratch = root.join("scratch");
    fs::create_dir_all(&scratch).expect("the scratch directory");

    git(&repository, &["init", "--quiet"]);
    git(&repository, &["add", "."]);
    git(
        &repository,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let commit = git(&repository, &["rev-parse", "HEAD"]).trim().to_owned();

    Fixture {
        root,
        split,
        scratch,
        commit,
    }
}

/// A limit well above what the fixture holds, so the sample is every position
/// and the tests that follow it are not also tests of the sampler.
fn enumerate(corpus: &Fixture) {
    enumerate_with(corpus, "500", "7");
}

fn enumerate_with(corpus: &Fixture, limit: &str, seed: &str) {
    let cli = measure_core::Cli::parse_from([
        "measure-test",
        "enumerate",
        "--corpus",
        &corpus.split.to_string_lossy(),
        "--limit",
        limit,
        "--seed",
        seed,
    ]);
    measure_core::run(&TestHandler, cli).expect("enumerate");
}

fn positions_of(corpus: &Fixture) -> String {
    fs::read_to_string(
        corpus
            .split
            .join("rust")
            .join("positions")
            .join("one.jsonl"),
    )
    .expect("enumerate wrote positions")
}

fn replay(corpus: &Fixture, records: Option<&Path>) {
    replay_with(&TestHandler, corpus, records);
}

/// The handler is a parameter for the same reason the file writes its own
/// rather than taking `lang_rust`'s: `measure_core` takes a
/// `&dyn LanguageHandler` and depends on no language, so what a replay does
/// with what a handler reports has to be assertable against any handler.
fn replay_with(handler: &dyn LanguageHandler, corpus: &Fixture, records: Option<&Path>) {
    let mut arguments = vec![
        "replay".to_owned(),
        "--corpus".to_owned(),
        corpus.split.to_string_lossy().into_owned(),
        "--server".to_owned(),
        "oracle".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];
    if let Some(path) = records {
        arguments.push("--records".to_owned());
        arguments.push(path.to_string_lossy().into_owned());
    }
    let cli =
        measure_core::Cli::parse_from(std::iter::once("measure-test".to_owned()).chain(arguments));
    measure_core::run(handler, cli).expect("replay");
}

/// The rendered table, as a value. `measure_core::run` prints it to a `stdout`
/// handle that `cargo test` does not capture, which is why `replay_table` is
/// public at all.
fn table_of(corpus: &Fixture, format: measure_core::Format) -> String {
    replay_result(corpus, format).expect("replay")
}

/// The same run, as a `Result`, for the assertions about what a replay
/// *refuses*: a corpus-integrity failure is not a panic, and a test that could
/// only observe one through `expect` could not say which failure it expected.
fn replay_result(corpus: &Fixture, format: measure_core::Format) -> Result<String, Error> {
    measure_core::replay_table(
        &TestHandler,
        &shared::SystemClock,
        &replay_arguments(corpus, format),
    )
}

/// The rendered table for a handler other than the template's, which is what a
/// reconciliation against the records file needs: the table and the records
/// have to be two accounts of the *same* run.
fn replay_report(
    handler: &dyn LanguageHandler,
    corpus: &Fixture,
    format: measure_core::Format,
) -> String {
    measure_core::replay_table(
        handler,
        &shared::SystemClock,
        &replay_arguments(corpus, format),
    )
    .expect("replay")
}

fn replay_arguments(corpus: &Fixture, format: measure_core::Format) -> measure_core::Replay {
    let cli = measure_core::Cli::parse_from([
        "measure-test",
        "replay",
        "--corpus",
        &corpus.split.to_string_lossy(),
        "--server",
        "oracle",
        "--format",
        match format {
            measure_core::Format::Table => "table",
            measure_core::Format::Json => "json",
        },
    ]);
    match cli.command {
        measure_core::Command::Replay(arguments) => arguments,
        measure_core::Command::Enumerate(_) | measure_core::Command::Collect(_) => {
            panic!("`replay` parsed as another subcommand")
        }
    }
}

/// A truth file whose oracle answered `null` everywhere. The handler abstains
/// everywhere, so every row is a mutual "no definition here" — which §6 calls
/// a match, and which makes the expected table computable without a server.
fn write_truth(corpus: &Fixture) {
    write_truth_as(corpus, "oracle", "oracle");
}

fn truth_path(corpus: &Fixture, directory: &str) -> PathBuf {
    corpus
        .split
        .join("rust")
        .join("truth")
        .join(directory)
        .join("one.jsonl")
}

/// The header `write_truth_as` writes, as a value: what a resume of that
/// fixture would have to match, field for field. `complete` is deliberately
/// the opposite of the file's, since a resume is what happens *because* the
/// file on disk is incomplete, and a check that compared it would refuse every
/// resume there is.
fn fixture_provenance(corpus: &Fixture) -> measure_core::Provenance {
    measure_core::Provenance {
        repository: "one".into(),
        commit: corpus.commit.as_str().into(),
        language: "rust".into(),
        server: "oracle".into(),
        server_version: "0".into(),
        grammar: "fixture".into(),
        measure_version: "0".into(),
        complete: false,
    }
}

/// A truth file whose oracle *answered*, cycling through the five ways an
/// answer can land against [`MismatchingHandler`]'s two locations: on its top,
/// on its second, far away in the same file, in a sibling file, and somewhere
/// else entirely.
///
/// Written as a `Location` on the wire rather than through a projection,
/// because §8.2 gives `DefinitionResult` no `Serialize` — the truth file keeps
/// the bytes the server sent, and this is the fixture standing in for a server.
fn write_truth_answered(corpus: &Fixture) {
    let positions = fs::read_to_string(
        corpus
            .split
            .join("rust")
            .join("positions")
            .join("one.jsonl"),
    )
    .expect("enumerate ran first");

    let lib = corpus
        .split
        .join("rust")
        .join("repos")
        .join("one")
        .join("src")
        .join("lib.rs");
    let here = shared::DocumentUri::from_file_path(&lib).expect("a file uri");
    let sibling =
        shared::DocumentUri::from_file_path(&lib.with_file_name("other.rs")).expect("a file uri");

    let answers = [
        answer(&here.to_string(), 0),
        answer(&here.to_string(), 4),
        answer(&here.to_string(), 40),
        answer(&sibling.to_string(), 0),
        answer("file:///elsewhere/other.rs", 0),
    ];

    let mut text = format!(
        "{{\"repository\":\"one\",\"commit\":\"{}\",\"language\":\"rust\",\
         \"server\":\"oracle\",\"server_version\":\"0\",\"grammar\":\"fixture\",\
         \"measure_version\":\"0\",\"complete\":true}}\n",
        corpus.commit
    );
    for (index, line) in positions.lines().enumerate() {
        let file = between(line, "\"file\":\"", "\"");
        let offset = between(line, "\"offset\":", ",");
        text.push_str(&format!(
            "{{\"file\":\"{file}\",\"offset\":{offset},\"outcome\":\"resolved\",\
             \"answer\":{},\"latency_us\":1234}}\n",
            answers[index % answers.len()]
        ));
    }

    let path = truth_path(corpus, "oracle");
    fs::create_dir_all(path.parent().expect("a truth directory")).expect("the truth directory");
    fs::write(path, text).expect("the truth file");
}

/// A truth file the oracle never answered: every row `error`, which
/// `data-collection.md` §4 keeps distinct from `none` because collapsing the
/// two gives the heuristic credit for abstaining where the oracle merely
/// failed.
fn write_truth_uncollected(corpus: &Fixture) -> usize {
    let positions = fs::read_to_string(
        corpus
            .split
            .join("rust")
            .join("positions")
            .join("one.jsonl"),
    )
    .expect("enumerate ran first");

    let mut text = format!(
        "{{\"repository\":\"one\",\"commit\":\"{}\",\"language\":\"rust\",\
         \"server\":\"oracle\",\"server_version\":\"0\",\"grammar\":\"fixture\",\
         \"measure_version\":\"0\",\"complete\":true}}\n",
        corpus.commit
    );
    let mut rows = 0;
    for line in positions.lines() {
        let file = between(line, "\"file\":\"", "\"");
        let offset = between(line, "\"offset\":", ",");
        text.push_str(&format!(
            "{{\"file\":\"{file}\",\"offset\":{offset},\"outcome\":\"error\",\
             \"answer\":null,\"latency_us\":1234}}\n"
        ));
        rows += 1;
    }

    let path = truth_path(corpus, "oracle");
    fs::create_dir_all(path.parent().expect("a truth directory")).expect("the truth directory");
    fs::write(path, text).expect("the truth file");
    rows
}

fn answer(uri: &str, line: u32) -> String {
    format!(
        "{{\"uri\":\"{uri}\",\"range\":{{\"start\":{{\"line\":{line},\"character\":0}},\
         \"end\":{{\"line\":{line},\"character\":1}}}}}}"
    )
}

/// `directory` is the `truth/<server>/` the file is written under and
/// `recorded` is the server its header claims. They differ in exactly one test,
/// which is the one asserting that the path is not the check.
fn write_truth_as(corpus: &Fixture, directory: &str, recorded: &str) {
    write_truth_headed(corpus, directory, &header(recorded, &corpus.commit, true));
}

/// The provenance header as a line, so a test can move the two fields a replay
/// checks for itself — the commit, and whether the collection ever finished.
fn header(server: &str, commit: &str, complete: bool) -> String {
    format!(
        "{{\"repository\":\"one\",\"commit\":\"{commit}\",\"language\":\"rust\",\
         \"server\":\"{server}\",\"server_version\":\"0\",\"grammar\":\"fixture\",\
         \"measure_version\":\"0\",\"complete\":{complete}}}"
    )
}

fn write_truth_headed(corpus: &Fixture, directory: &str, header: &str) {
    let positions = fs::read_to_string(
        corpus
            .split
            .join("rust")
            .join("positions")
            .join("one.jsonl"),
    )
    .expect("enumerate ran first");

    let mut text = format!("{header}\n");
    for line in positions.lines() {
        let file = between(line, "\"file\":\"", "\"");
        let offset = between(line, "\"offset\":", ",");
        text.push_str(&format!(
            "{{\"file\":\"{file}\",\"offset\":{offset},\"outcome\":\"none\",\
             \"answer\":null,\"latency_us\":1234}}\n"
        ));
    }

    let path = truth_path(corpus, directory);
    fs::create_dir_all(path.parent().expect("a truth directory")).expect("the truth directory");
    fs::write(path, text).expect("the truth file");
}

/// The keys of a flat JSON object, in the order they appear. Written by hand
/// rather than through `serde_json::Value`, which `clippy.toml` disallows.
fn field_order(line: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = line;
    let mut depth = 0_i32;

    while let Some(quote) = rest.find('"') {
        for byte in rest[..quote].bytes() {
            match byte {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                _ => {}
            }
        }
        let after = &rest[quote + 1..];
        let Some(close) = after.find('"') else { break };
        let (name, tail) = after.split_at(close);
        // Depth 1 is the record's own object; anything deeper is a nested
        // value, and anything after a `:` is a string value rather than a key.
        if depth == 1 && tail[1..].trim_start().starts_with(':') {
            found.push(name);
        }
        rest = &tail[1..];
    }
    found
}

/// The four columns a records file is aggregated on, spelled once so that the
/// reconciliation and the sample read them the same way.
fn prior_of(line: &str) -> &str {
    between(line, "\"stratum_prior\":\"", "\"")
}

fn settled_of(line: &str) -> &str {
    between(line, "\"stratum_final\":\"", "\"")
}

fn decision_of(line: &str) -> &str {
    between(line, "\"decision\":\"", "\"")
}

/// `""` for a row with no oracle verdict, since there is no `"agreement":"` in
/// `"agreement":null`. That is the right answer rather than a coincidence: an
/// unjudged row belongs in none of the three agreement counters.
fn agreement_of(line: &str) -> &str {
    between(line, "\"agreement\":\"", "\"")
}

/// A ratio as the text table prints it, computed here rather than read from
/// `Row`: an assertion against the code that produced the number asserts
/// nothing.
#[expect(
    clippy::cast_precision_loss,
    reason = "two counts bounded by the fixture, formatted for comparison against a printed column"
)]
fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0.0".to_owned();
    }
    format!("{:.1}", part as f64 / whole as f64 * 100.0)
}

fn tally(text: &str, wanted: impl Fn(&str) -> bool) -> u64 {
    u64::try_from(text.lines().filter(|line| wanted(line)).count()).expect("a count of rows")
}

/// One counter out of `--format json`, which is the format the harness
/// consumes. Read forward from the named row, which is sound because `Row`
/// writes `stratum` first and all seven counters after it.
fn reported(report: &str, stratum: &str, field: &str) -> u64 {
    let row = report
        .find(&format!("\"stratum\": \"{stratum}\""))
        .map(|at| &report[at..])
        .unwrap_or_else(|| panic!("the report names no {stratum} row:\n{report}"));
    let marker = format!("\"{field}\": ");
    let at = row
        .find(&marker)
        .unwrap_or_else(|| panic!("the {stratum} row has no {field}:\n{row}"))
        + marker.len();
    row[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or_else(|error| panic!("{stratum}/{field} is not a count: {error}\n{row}"))
}

fn between<'a>(line: &'a str, open: &str, close: &str) -> &'a str {
    let Some(start) = line.find(open) else {
        return "";
    };
    let rest = &line[start + open.len()..];
    match rest.find(close) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// Every file under `root`, relative and sorted.
fn files_under(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("a fixture directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }

    found.sort();
    found
}

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .unwrap_or_else(|error| panic!("running git {arguments:?}: {error}"));
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `heuristic_latency_us` is the one field §7 says a replay does *not*
/// reproduce exactly — it is the same handler on the same snapshot, so it is
/// recorded, but nothing in the run branches on it and it needs a quiet
/// machine to mean anything. Masking it is what leaves the determinism claim
/// testable instead of flaky; masking anything else would be hiding a bug.
fn without_the_clock(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut masked = String::with_capacity(line.len());
            let mut rest = line;
            while let Some(at) = rest.find("\"heuristic_latency_us\":") {
                let (before, after) = rest.split_at(at + "\"heuristic_latency_us\":".len());
                masked.push_str(before);
                masked.push('_');
                rest = after.trim_start_matches(|scalar: char| scalar.is_ascii_digit());
            }
            masked.push_str(rest);
            masked
        })
        .collect::<Vec<_>>()
        .join("\n")
}
