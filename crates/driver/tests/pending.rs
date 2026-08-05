//! `design/core.md#both-sides-are-sets`, from the driver's side.
//!
//! Two claims, and neither is visible to the compiler once the code is written
//! the wrong way. A pending-query record that stored the shim's answer as a set
//! — or sorted it, or deduped it — would compile, would classify every query,
//! and would report `match_top1` for whichever location happened to hash first;
//! the number that gets optimised would then be measuring nothing. And a
//! divergence report emitted on `match_contained` would be a notification
//! telling the user they were misled when the location they were shown was the
//! correct one, which §6 says would train them to ignore the reports that
//! matter.
//!
//! So both are asserted by *contrast* rather than by presence. The rank test
//! classifies the same two locations twice, in the two orders, and requires the
//! answers to differ; the reporting test walks all three of §6's
//! classifications and requires the report to appear on exactly one.
//!
//! The grammar is a dev-dependency for the reason `wire_locations.rs` gives:
//! an `Answer` carrying locations can only come out of `dispatch`, `dispatch`
//! parses, and a parse needs one. Going through `dispatch` is deliberate —
//! `PendingQuery::answered_by_shim` takes what the wrapper produced, so a test
//! that hand-built an answer would be testing a path the driver does not have.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builder and handler doubles below are free functions and trait impls. Failing loudly is the point: a half-built fixture leaves an empty file list, and every assertion here passes against a query that never resolved."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use driver::{
    DebounceMs, Dispatched, FileListCache, PendingQueries, PendingQuery, Request, Resolution,
    dispatch,
};
use shared::proto::{
    DefinitionResult, MessageType, PositionEncoding, WireLocation, WirePosition, WireRange,
};
use shared::{
    AbstainReason, Agreement, Clock, CommitPolicy, Confidence, Deadline, DocumentUri,
    DocumentVersion, EditorRequestId, Error, FileExtension, LanguageHandler, LanguageId, Location,
    Offset, Outcome, ProjectPath, ProjectRoot, ProjectView, Query, RelPath, Rope, ServerProfile,
    Severity, SnapshotSeed, Strata, Stratum, SystemClock, Trace,
};
use tree_sitter::Language;

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// The document the query arrives against, and one of the two definition
/// sites: `fn target` on line 4.
const DOCUMENT: &str = "fn caller() {\n    target();\n}\n\nfn target() {}\n";

/// The other, in a different file and on line 0 — so the two sites differ in
/// both of the fields §6's predicate compares, and no tolerance window can make
/// them match each other.
const TARGET: &str = "fn target() {}\n";

const DEFINITION: &str = "fn target() {}";

/// §6: "`top1` — the shim's *first* location matches. Cannot be improved by
/// returning more, so it is the number that gets optimized."
///
/// The same two locations, the same child answer, the two orders. If the record
/// collapsed the ranked list to a set — or sorted it, or handed it to the
/// predicate through anything that does not preserve order — these two
/// classifications would be equal, and `top1` would have stopped being a
/// property of the shim's ranking.
#[test]
fn the_stored_rank_decides_top1() {
    let root = fixture("rank");
    let view = view(&root);
    let first = definition_in(&view, &root, "src/target.rs");
    let second = definition_in(&view, &root, "src/lib.rs");
    let child = child_answered(&second, DOCUMENT);

    let ranked_second = resolution(&view, &root, vec![first.clone(), second.clone()], &child);
    let ranked_first = resolution(&view, &root, vec![second, first], &child);

    assert_eq!(
        ranked_first.agreement(),
        Agreement::MatchTop1,
        "the child's location was ranked first by the shim and did not classify as \
         match_top1, so the stored list is not the ranked one core.md#both-sides-are-sets \
         requires"
    );
    assert_eq!(
        ranked_second.agreement(),
        Agreement::MatchContained,
        "the same location ranked second still classified as {}, so the pending record \
         collapsed the shim's ranked list to a set and top1 cannot be computed from it",
        ranked_second.agreement()
    );
}

/// §6: "Divergence is reported to the user on `mismatch` only. A
/// `match_contained` answer showed the user the correct location; telling them
/// they were misled would be false."
///
/// All three classifications, because the interesting one is the middle: a
/// report on `match_top1` would be absurd and somebody would notice, where a
/// report on `match_contained` looks like diligence.
#[test]
fn a_report_is_produced_on_mismatch_only() {
    let root = fixture("mismatch_only");
    let view = view(&root);
    let elsewhere = definition_in(&view, &root, "src/target.rs");
    let correct = definition_in(&view, &root, "src/lib.rs");
    let child = child_answered(&correct, DOCUMENT);

    let cases = [
        (vec![correct.clone()], Agreement::MatchTop1),
        (vec![elsewhere.clone(), correct], Agreement::MatchContained),
        (
            vec![elsewhere],
            // `NearModule` rather than `Unrelated`: both fixture files are in
            // `src/`, and `same_module_tree` reads that as the same containing
            // directory. Named rather than wildcarded so that the severity the
            // report carries is compared against a value this test chose.
            Agreement::Mismatch {
                severity: Severity::NearModule,
            },
        ),
    ];

    for (ranked, expected) in cases {
        let resolved = resolution(&view, &root, ranked, &child);
        assert_eq!(
            resolved.agreement(),
            expected,
            "the fixture no longer produces {expected}, so the assertion below is about \
             some other classification"
        );

        match (resolved.agreement(), resolved.divergence()) {
            (Agreement::Mismatch { severity }, Some(divergence)) => assert_eq!(
                divergence.severity(),
                severity,
                "the report's severity is not the one the predicate classified"
            ),
            (Agreement::Mismatch { severity: _ }, None) => panic!(
                "a mismatch produced no divergence report, and core.md#both-sides-are-sets \
                 makes the report the only protection a user has against a wrong jump"
            ),
            (agreement @ (Agreement::MatchTop1 | Agreement::MatchContained), Some(divergence)) => {
                panic!(
                    "{agreement} produced the report {:?}: core.md#both-sides-are-sets \
                     reports on mismatch only, because a contained match showed the user \
                     the correct location",
                    divergence.message()
                )
            }
            (Agreement::MatchTop1 | Agreement::MatchContained, None) => {}
        }
    }
}

/// `shim.md` §9: a report can arrive long after the jump, so it must name the
/// jump it refers to rather than only the correction.
///
/// This is also the assertion that fails if the stored list is reordered on the
/// way out: the message names the *top-ranked* location, which is where a user
/// who trusts the ordering looked first.
#[test]
fn the_report_names_where_the_user_was_sent() {
    let root = fixture("names_the_jump");
    let view = view(&root);
    let sent_to = definition_in(&view, &root, "src/target.rs");
    let also_wrong = definition_in(&view, &root, "src/away.rs");
    let correct = definition_in(&view, &root, "src/lib.rs");
    let child = child_answered(&correct, DOCUMENT);

    let resolved = resolution(
        &view,
        &root,
        vec![sent_to.clone(), also_wrong.clone()],
        &child,
    );
    let divergence = resolved
        .divergence()
        .expect("a mismatch, since neither of the shim's locations is the child's");
    let message = &divergence.message().message;

    assert_eq!(
        divergence.message().message_type,
        MessageType::Warning,
        "a wrong jump is reported at Log or Info severity, which is not a report the user \
         will see"
    );
    assert!(
        message.contains(sent_to.uri().as_str()),
        "the report does not name where the user was sent: {message}"
    );
    assert!(
        !message.contains(also_wrong.uri().as_str()),
        "the report names the shim's second-ranked location rather than the one the user \
         was actually sent to, which is where somebody who trusts the ordering looked \
         first: {message}"
    );
    assert!(
        !message.contains(correct.uri().as_str()),
        "the report names the child's location where shim.md §9 asks it to name the jump \
         it refers to: {message}"
    );
}

/// §6 classifies two answers. An abstention is not one — the child's reply was
/// the only answer the user ever saw, and there is nothing of ours for it to
/// diverge from.
///
/// Distinct from a commit with an empty location list, which §6 *does* classify
/// ("both empty is a match"), and which the next assertion covers.
#[test]
fn an_abstention_leaves_nothing_to_compare() {
    let root = fixture("abstention");
    let view = view(&root);
    let correct = definition_in(&view, &root, "src/lib.rs");

    let mut query = pending();
    query.answered_by_shim(&answer(&Declining, &view, &root));

    assert!(
        query.resolve(&child_answered(&correct, DOCUMENT)).is_none(),
        "an abstained query resolved to a classification, so a stratum with no coverage \
         and a wrong answer are being counted as the same thing"
    );
}

/// The other half of the same distinction: a commit with no locations is an
/// answer — "there is no definition here" — and the child disagreeing with it
/// is a divergence the user is owed.
#[test]
fn a_commit_with_no_locations_is_still_an_answer() {
    let root = fixture("empty_commit");
    let view = view(&root);
    let correct = definition_in(&view, &root, "src/lib.rs");

    let mut query = pending();
    query.answered_by_shim(&answer(&Committing { locations: vec![] }, &view, &root));

    let resolved = query
        .resolve(&child_answered(&correct, DOCUMENT))
        .expect("a commit is an answer, whether or not it had locations in it");
    assert!(
        resolved.divergence().is_some(),
        "the shim said there was no definition, the child found one, and nobody told the \
         user: {}",
        resolved.agreement()
    );
}

/// `shim.md` §7: `$/cancelRequest` drops the `PendingQuery`, and "the shim must
/// not answer a cancelled request" — nor report on one, since the answer it
/// would be reporting on never reached the editor.
#[test]
fn a_cancelled_query_resolves_to_nothing() {
    let root = fixture("cancellation");
    let view = view(&root);
    let correct = definition_in(&view, &root, "src/lib.rs");
    let child = child_answered(&correct, DOCUMENT);
    let elsewhere = definition_in(&view, &root, "src/target.rs");

    let mut table = PendingQueries::new();
    table.record(pending());
    assert!(
        table.answered_by_shim(
            &EditorRequestId::from_number(1),
            &answer(
                &Committing {
                    locations: vec![elsewhere],
                },
                &view,
                &root,
            ),
        ),
        "the query was recorded and the answer did not find it"
    );

    assert!(
        table.cancelled(&EditorRequestId::from_number(1)).is_some(),
        "the cancel did not find the pending query"
    );
    assert!(
        table
            .child_answered(&EditorRequestId::from_number(1), &child)
            .is_none(),
        "a cancelled query was still resolved when the child replied, so the user gets a \
         report about an answer they were never shown"
    );
    assert!(
        table.is_empty(),
        "{} pending queries survived a cancel and a reply",
        table.len()
    );
}

/// The child's reply ends the query's life: one resolution per id, so a
/// duplicate or replayed response cannot report the same divergence twice.
#[test]
fn the_childs_reply_ends_the_query() {
    let root = fixture("one_resolution");
    let view = view(&root);
    let correct = definition_in(&view, &root, "src/lib.rs");
    let child = child_answered(&correct, DOCUMENT);
    let elsewhere = definition_in(&view, &root, "src/target.rs");

    let mut table = PendingQueries::new();
    table.record(pending());
    table.answered_by_shim(
        &EditorRequestId::from_number(1),
        &answer(
            &Committing {
                locations: vec![elsewhere],
            },
            &view,
            &root,
        ),
    );

    let first = table.child_answered(&EditorRequestId::from_number(1), &child);
    assert!(
        first
            .flatten()
            .is_some_and(|resolved| resolved.divergence().is_some()),
        "the first reply did not resolve to a divergence, so the second assertion is \
         vacuous"
    );
    assert!(
        table
            .child_answered(&EditorRequestId::from_number(1), &child)
            .is_none(),
        "a second reply under the same id resolved again, and every divergence report is \
         a notification the user sees"
    );
}

/// One dispatch, one record, one resolution — the whole of §7's steps 1, 3 and
/// 4 for a query whose answer is `ranked`.
fn resolution(
    view: &ProjectView,
    root: &Path,
    ranked: Vec<Location>,
    child: &DefinitionResult,
) -> Resolution {
    let mut query = pending();
    query.answered_by_shim(&answer(&Committing { locations: ranked }, view, root));
    query
        .resolve(child)
        .expect("a committed answer is one the predicate classifies")
}

/// The record as `shim.md` §7's step 1 makes it: the request has been forwarded
/// and nothing has answered yet.
fn pending() -> PendingQuery {
    PendingQuery::new(
        EditorRequestId::from_number(1),
        uri_of(Path::new("/fixture/src/lib.rs")),
        Offset(21),
        SystemClock.now(),
    )
}

/// The child's `textDocument/definition` response, in the one-location shape.
///
/// Through `WirePosition::encode` rather than a hand-built row, because that is
/// the only constructor there is (§8.3) — and it is what the predicate has to
/// cope with: the child's side arrived over a wire in the negotiated encoding,
/// and §6 compares rows precisely because resolving a `character` would need
/// the target document's text.
fn child_answered(location: &Location, text: &str) -> DefinitionResult {
    let rope = Rope::from(text);
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

fn answer(handler: &dyn LanguageHandler, view: &ProjectView, root: &Path) -> driver::Answer {
    let deadline = Deadline::none();
    let policy = CommitPolicy::permissive();
    let server = ServerProfile::standalone();
    let request = Request {
        seed: seed(root),
        position: Offset(21),
        project: view,
        deadline: &deadline,
        server: &server,
        policy: &policy,
    };

    match dispatch(handler, request, PositionEncoding::Utf16).dispatched {
        Dispatched::Decided(answer) => answer,
        other @ (Dispatched::DeadlineExpired(_) | Dispatched::Failed(_) | Dispatched::Shed(_)) => {
            panic!("the query did not decide: {other:?}")
        }
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

/// A `Location` for the `fn target` definition, however the file spells it.
///
/// Through `ProjectView`'s own `read` and `parse`, because `Location::at_node`
/// is the only constructor and a location built any other way would not be one
/// a handler could have returned.
fn definition_in(view: &ProjectView, root: &Path, relative: &str) -> Location {
    let path = project_path(view, root, relative);
    let text = view.read(&path).expect("reading a fixture file");
    let tree = view.parse(&path, &text).expect("parsing a fixture file");
    let flat: String = text.chunks().collect();
    let start = flat
        .find(DEFINITION)
        .unwrap_or_else(|| panic!("{relative} does not contain the definition"));
    let end = start + DEFINITION.len();

    let node = tree
        .root_node()
        .descendant_for_byte_range(start, end)
        .unwrap_or_else(|| panic!("no node spans the definition in {relative}"));
    assert_eq!(
        (node.start_byte(), node.end_byte()),
        (start, end),
        "the grammar does not give the definition in {relative} a node of its own, so this \
         fixture is testing something other than what it says"
    );

    Location::at_node(uri_of(&path.to_absolute()), &node)
}

fn project_path(view: &ProjectView, root: &Path, relative: &str) -> ProjectPath {
    let rel = RelPath::new(Path::new(relative)).expect("a relative path");
    view.lookup(&ProjectRoot::new(root), &rel)
        .unwrap_or_else(|| panic!("{relative} is not in the fixture file list"))
}

fn uri_of(path: &Path) -> DocumentUri {
    DocumentUri::from_file_path(path).expect("a file URI for a fixture path")
}

fn seed(root: &Path) -> SnapshotSeed {
    SnapshotSeed::fresh(
        uri_of(&root.join("src").join("lib.rs")),
        Rope::from(DOCUMENT),
        DocumentVersion(1),
        LanguageId::new("rust"),
        grammar(),
    )
}

fn view(root: &Path) -> ProjectView {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    FileListCache::new(vec![root.to_path_buf()], clock, DebounceMs::RESCAN)
        .expect("the scanner thread")
        .view(Deadline::none(), grammar())
        .expect("enumerating the fixture")
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
    fs::create_dir_all(root.join("src")).expect("the fixture source directory");
    fs::write(root.join("src").join("lib.rs"), DOCUMENT).expect("the fixture document");
    fs::write(root.join("src").join("target.rs"), TARGET).expect("the fixture target file");
    // A third file holding the same definition, so a report can be checked
    // against two locations the shim ranked and only one it sent the user to.
    fs::write(root.join("src").join("away.rs"), TARGET).expect("the fixture second target");

    root
}
