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
    reason = "`Command::output` is banned so the shim polls cooperatively against its deadline. This is a test building a fixture git checkout, where there is no deadline and the child is `git` on a three-file repository."
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builders below are free functions in a file that is nothing but tests. Failing loudly is the point: a fixture that half-built would leave an empty corpus, and every assertion here passes against an empty corpus."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use shared::{
    AbstainReason, ByteLen, CandidateCount, Confidence, Error, FileCount, FileExtension,
    LanguageHandler, LanguageId, Margin, Micros, Outcome, Query, Refinement, StageLabel, StageName,
    Strata, Stratum, Trace,
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

/// The grammar the workspace pins, read from the manifest rather than from the
/// lockfile the implementation embeds: asserting a value against the file it
/// was computed from asserts nothing.
const WORKSPACE_MANIFEST: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"));

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
    // The claim is that the corpus's notion of "a query" and the handler's are
    // one function. The positions file is written by `shared::identifiers` and
    // the `other` rows by `shared::identifier_at`; a second implementation
    // would show up as a row of one class the other class also matches.
    assert!(
        text.contains("\"class\":\"other\""),
        "no non-identifier positions were enumerated, so the NotAnIdentifier \
         path has nothing in the corpus to fire on (data-collection.md §2)"
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
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    let split = root.join("training");
    let repository = split.join("rust").join("repos").join("one");
    fs::create_dir_all(repository.join("src")).expect("the fixture repository");
    fs::write(repository.join("src").join("lib.rs"), SOURCE).expect("the fixture source");

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

fn enumerate(corpus: &Fixture) {
    let cli = measure_core::Cli::parse_from([
        "measure-test",
        "enumerate",
        "--corpus",
        &corpus.split.to_string_lossy(),
        "--limit",
        "500",
        "--seed",
        "7",
    ]);
    measure_core::run(&TestHandler, cli).expect("enumerate");
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

/// `directory` is the `truth/<server>/` the file is written under and
/// `recorded` is the server its header claims. They differ in exactly one test,
/// which is the one asserting that the path is not the check.
fn write_truth_as(corpus: &Fixture, directory: &str, recorded: &str) {
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
         \"server\":\"{recorded}\",\"server_version\":\"0\",\"grammar\":\"fixture\",\
         \"measure_version\":\"0\",\"complete\":true}}\n",
        corpus.commit
    );
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
