//! `design/core.md` §8.4: the dispatch wrapper converts a handler's byte-space
//! `Location` into the `WireLocation` that goes out, the dispatch result
//! carries **both** forms, and the `PositionEncoding` is handed to the wrapper
//! alongside the query.
//!
//! Nothing in the build notices any of that being wrong. A conversion that
//! encoded in the wrong units still compiles and still produces a plausible
//! line and character; the answer just lands a few columns off in any file
//! with a character outside ASCII, which is a confidently wrong jump — this
//! tool's value proposition inverted. So the fixture puts a four-byte
//! character before the definition and asserts that the three encodings
//! produce three *different* wire positions, each of which resolves back to
//! the byte offset the handler returned.
//!
//! The grammar is a dev-dependency, which is not the language edge §9's graph
//! forbids: `shared` and `measure_core` both take one for the same reason, and
//! `seam.rs` asserts the distinction rather than assuming it. Taking the
//! grammar directly rather than reaching for `lang_rust` is what keeps this
//! honest about `&dyn LanguageHandler` — and `lang_rust`'s own handler
//! abstains on everything, so it cannot produce the answer under test anyway.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builder and handler doubles below are free functions and trait impls. Failing loudly is the point: a half-built fixture leaves an empty file list, which every assertion here passes against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use driver::{Dispatched, dispatch};
use shared::proto::{PositionEncoding, WirePosition};
use shared::{
    AbstainReason, ByteOffset, CommitPolicy, Confidence, Deadline, DocumentSnapshot, DocumentUri,
    DocumentVersion, Error, FileExtension, FileList, LanguageHandler, LanguageId, Location,
    Outcome, ProjectError, ProjectPath, ProjectRoot, ProjectView, Query, RelPath, Rope,
    ServerProfile, SnapshotSeed, Stratum,
};
use tree_sitter::Language;

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// The document a query arrives against. It holds a definition of its own, so
/// the fixture covers both halves of the conversion's lookup: a target in the
/// open document, whose rope the snapshot already has, and a target in a
/// closed file, which costs a read.
const DOCUMENT: &str = "fn caller() {\n    target();\n}\n\nfn target() {}\n";

/// The definition's file. The block comment is three ASCII bytes, one
/// four-byte scalar and four more ASCII bytes, so `fn target` starts at byte
/// 11, UTF-16 column 9 and UTF-32 column 8 — three numbers no confusion of
/// encodings can produce by accident.
const TARGET: &str = "/* \u{1d11e} */ fn target() {}\n";

const DEFINITION: &str = "fn target() {}";

/// §8.4's two consequences at once: the answer comes back in both forms, and
/// the byte-space half is exactly what the handler returned — `core` retains
/// it for §6's predicate, which compares `(uri, line)` and reads nothing.
#[test]
fn a_decided_query_comes_back_in_both_forms() {
    let root = fixture("both_forms");
    let view = view(&root);
    let document = snapshot(&root);
    let expected = vec![
        definition_in(&view, &root, "src/target.rs"),
        definition_in(&view, &root, "src/lib.rs"),
    ];
    let handler = Committing {
        locations: expected.clone(),
    };

    let answer = decided(&handler, &document, &view, PositionEncoding::Utf16);

    match answer.outcome() {
        Outcome::Committed {
            locations,
            confidence: _,
            stratum: _,
        } => assert_eq!(
            locations, &expected,
            "the byte-space locations did not survive the conversion, and they are the \
             half core keeps: core.md §6's predicate compares (uri, line) and never sees \
             the wire form again"
        ),
        other @ Outcome::Abstain {
            reason: _,
            stratum: _,
        } => panic!("a commit came back as {other:?}"),
    }

    assert_eq!(
        answer.wire().len(),
        expected.len(),
        "core.md §8.4: the dispatch result carries both forms, and the wire form is \
         derived from the byte form rather than supplied beside it"
    );
    for (wire, location) in answer.wire().iter().zip(&expected) {
        assert_eq!(
            wire.uri(),
            location.uri(),
            "the conversion moved a location"
        );
        assert_eq!(
            wire.range().start.line(),
            location.line(),
            "the wire row disagrees with the row Location::at_node derived from the \
             same node (core.md §8.4)"
        );
    }
}

/// The encoding is handed to the wrapper and is the one actually applied.
///
/// Asserted by round trip rather than by reading a column, because
/// `WirePosition` deliberately exposes no way to be used as an offset
/// (§8.3): `resolve` is the only way out and it demands the encoding, so an
/// answer encoded in one and read in another lands somewhere else.
#[test]
fn the_encoding_handed_to_the_wrapper_is_the_one_applied() {
    let root = fixture("encodings");
    let view = view(&root);
    let document = snapshot(&root);
    let location = definition_in(&view, &root, "src/target.rs");
    let handler = Committing {
        locations: vec![location.clone()],
    };
    let text = Rope::from(TARGET);

    let mut encoded: Vec<(PositionEncoding, WirePosition)> = Vec::new();
    for encoding in [
        PositionEncoding::Utf8,
        PositionEncoding::Utf16,
        PositionEncoding::Utf32,
    ] {
        let answer = decided(&handler, &document, &view, encoding);
        let start = answer
            .wire()
            .first()
            .expect("the one location the handler committed")
            .range()
            .start;
        assert_eq!(
            start.resolve(encoding, &text).ok(),
            Some(location.range().start),
            "a {encoding} wire position did not resolve back to the byte offset the \
             handler returned"
        );
        encoded.push((encoding, start));
    }

    for (index, (encoding, position)) in encoded.iter().enumerate() {
        for (other_encoding, other) in &encoded[index + 1..] {
            assert_ne!(
                position, other,
                "{encoding} and {other_encoding} produced the same wire position for a \
                 line with a four-byte character in it, so whatever the wrapper is \
                 encoding with, it is not the encoding it was handed (core.md §8.4)"
            );
        }
    }
}

/// An abstention has no locations, so it has no wire form — and `core` must
/// not be able to tell the difference between "declined" and "answered with
/// nothing on the wire" by looking at the wire half.
#[test]
fn an_abstention_carries_no_wire_locations() {
    let root = fixture("abstention");
    let view = view(&root);
    let document = snapshot(&root);

    let answer = decided(&Declining, &document, &view, PositionEncoding::Utf16);

    match answer.outcome() {
        Outcome::Abstain {
            reason: _,
            stratum: _,
        } => {}
        other @ Outcome::Committed {
            locations: _,
            confidence: _,
            stratum: _,
        } => panic!("an abstention came back as {other:?}"),
    }
    assert!(
        answer.wire().is_empty(),
        "an abstention acquired {} wire locations",
        answer.wire().len()
    );
}

/// The scope rule survives the round trip through a `Location`.
///
/// `ProjectPath` makes a path outside the project unspellable on the way in,
/// but the conversion is handed a bare `DocumentUri` and has to find the
/// target file's text again — so it goes back through `lookup`, and a URI the
/// file list does not know resolves to nothing rather than to a disk read.
#[test]
fn a_location_the_file_list_does_not_know_is_a_failure_and_not_an_answer() {
    let root = fixture("unresolvable");
    let view = view(&root);
    let document = snapshot(&root);
    let handler = Committing {
        locations: vec![elsewhere(&view, &root)],
    };
    let deadline = Deadline::none();
    let policy = CommitPolicy::permissive();
    let server = ServerProfile { id: None };
    let query = Query {
        doc: &document,
        position: ByteOffset(0),
        project: &view,
        deadline: &deadline,
        server: &server,
        policy: &policy,
    };

    match dispatch(&handler, &query, PositionEncoding::Utf16) {
        Dispatched::Failed(Error::Project(ProjectError::Unresolvable { uri: _ })) => {}
        other @ (Dispatched::Failed(_) | Dispatched::Decided(_) | Dispatched::DeadlineExpired) => {
            panic!(
                "a location naming a file outside the project became {other:?}, where \
                 core.md §8.4's conversion has no text to encode against"
            )
        }
    }
}

/// A handler that commits what it was built with, which `lang_rust`'s template
/// cannot do: it abstains on everything by design, so the answer §8.4 is about
/// has no producer in the workspace yet.
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
            Stratum::LocalBinding,
            Confidence::ONE,
            self.locations.clone(),
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
            stratum: Stratum::LocalBinding,
        })
    }
}

fn decided(
    handler: &dyn LanguageHandler,
    document: &DocumentSnapshot,
    view: &ProjectView,
    encoding: PositionEncoding,
) -> driver::Answer {
    let deadline = Deadline::none();
    let policy = CommitPolicy::permissive();
    let server = ServerProfile { id: None };
    let query = Query {
        doc: document,
        position: ByteOffset(0),
        project: view,
        deadline: &deadline,
        server: &server,
        policy: &policy,
    };

    match dispatch(handler, &query, encoding) {
        Dispatched::Decided(answer) => answer,
        other @ (Dispatched::DeadlineExpired | Dispatched::Failed(_)) => {
            panic!("the query did not decide: {other:?}")
        }
    }
}

/// A `Location` for the `fn target` definition, however the file spells it.
///
/// Through `ProjectView`'s own `read` and `parse`, because that is the only
/// route a real handler has and `Location::at_node` is the only constructor —
/// a location built any other way would not be one the conversion can be
/// handed.
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
        "the grammar does not give the definition in {relative} a node of its own, so \
         this fixture is testing something other than what it says"
    );

    Location::at_node(uri_of(&path.to_absolute()), &node)
}

/// A `Location` whose node is real and whose URI is not a project file: what a
/// handler would hold if the file list had moved under the query.
fn elsewhere(view: &ProjectView, root: &Path) -> Location {
    let inside = definition_in(view, root, "src/target.rs");
    let path = project_path(view, root, "src/target.rs");
    let text = view.read(&path).expect("reading a fixture file");
    let tree = view.parse(&path, &text).expect("parsing a fixture file");
    Location::at_node(
        uri_of(&root.join("..").join("not-a-project-file.rs")),
        &tree
            .root_node()
            .descendant_for_byte_range(inside.range().start.0, inside.range().end.0)
            .expect("the node the definition already resolved to"),
    )
}

fn project_path(view: &ProjectView, root: &Path, relative: &str) -> ProjectPath {
    let rel = RelPath::new(Path::new(relative)).expect("a relative path");
    view.lookup(&ProjectRoot::new(root), &rel)
        .unwrap_or_else(|| panic!("{relative} is not in the fixture file list"))
}

fn uri_of(path: &Path) -> DocumentUri {
    DocumentUri::from_file_path(path).expect("a file URI for a fixture path")
}

fn snapshot(root: &Path) -> DocumentSnapshot {
    SnapshotSeed::fresh(
        uri_of(&root.join("src").join("lib.rs")),
        Rope::from(DOCUMENT),
        DocumentVersion(1),
        LanguageId::new("rust"),
        grammar(),
    )
    .realise()
    .expect("parsing the document")
}

fn view(root: &Path) -> ProjectView {
    let roots = [root.to_path_buf()];
    let files = FileList::enumerate(&roots).expect("enumerating the fixture");
    ProjectView::new(Arc::new(files), Deadline::none(), grammar())
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

/// One workspace root. The empty `.git` directory is what makes it a
/// repository for the `ignore` crate, which the file list needs before it will
/// return anything at all.
fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
    fs::create_dir_all(root.join("src")).expect("the fixture source directory");
    fs::write(root.join("src").join("lib.rs"), DOCUMENT).expect("the fixture document");
    fs::write(root.join("src").join("target.rs"), TARGET).expect("the fixture target file");

    root
}
