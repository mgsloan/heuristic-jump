//! `resolution.md` §3's `ProjectView`, over a real directory.
//!
//! A real directory rather than a double, because the two properties worth
//! testing are both about the filesystem: that the `ignore` walker's exclusions
//! are what a handler can see, and that candidate order is
//! `resolution.md` §4's tiers rather than the hash-set order the file list is
//! stored in. Neither survives an in-memory stand-in, and `CLAUDE.md`'s
//! no-unit-tests rule rules one out anyway.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the fixture builder below is a free function. Failing loudly is the point: a half-built fixture leaves an empty file list, which every assertion here passes against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shared::{
    Clock, Deadline, Error, FileExtension, FileList, Generation, HandlerError, ProjectPath,
    ProjectRoot, ProjectView, RelPath, ScanRequest, SearchOrigin, SystemClock,
};
use tree_sitter::Language;

const RUST: FileExtension = FileExtension::new("rs");

/// The fixture, as (path, contents) relative to the single root. `notes.md` is
/// the wrong extension and `vendored/copy.rs` is gitignored; both are files the
/// walker sees and a handler must not.
///
/// `alpha` appears as a whole token in two files and as a near miss in three
/// more — `alphabet`, `_alpha`, `alpha1` — which is what the scan has to tell
/// apart.
const FILES: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "fn alpha() {}\nfn alphabet() {}\nfn _alpha() {}\n    alpha();\nlet alpha1 = 0;\n",
    ),
    ("src/util.rs", "fn beta() {}\n"),
    ("src/deep/inner.rs", "fn alphabet() {}\n"),
    ("other/far.rs", "fn beta() {}\nfn alpha() {}\n"),
    ("notes.md", "alpha\n"),
    ("vendored/copy.rs", "fn alpha() {}\n"),
];

#[test]
fn candidates_are_ordered_by_section_4s_tiers() {
    let root = fixture("tiers");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));

    assert_eq!(
        rel_paths(&view, &origin),
        [
            // Tier 2: the requesting document's own directory.
            "src/lib.rs",
            "src/util.rs",
            // Tier 3, by path proximity: `src/deep` shares a component with
            // the document, `other` shares none.
            "src/deep/inner.rs",
            "other/far.rs",
        ],
        "resolution.md §4 orders candidates cheapest and most likely first, \
         and a handler that searches in file-list order searches a hash-set \
         order that is not the same twice"
    );
}

#[test]
fn a_resolved_import_is_searched_first() {
    let root = fixture("resolved");
    let view = view(&root);
    let origin = SearchOrigin::from_import(
        path(&view, &root, "src/lib.rs"),
        path(&view, &root, "other/far.rs"),
    );

    assert_eq!(
        rel_paths(&view, &origin).first().map(String::as_str),
        Some("other/far.rs"),
        "resolution.md §4's tier 1 is the exact file an import resolved to, \
         which is the one candidate the search already has a reason to believe"
    );
}

#[test]
fn candidates_exclude_what_the_walker_excludes() {
    let root = fixture("scope");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));
    let found = rel_paths(&view, &origin);

    assert!(
        !found.iter().any(|found| found == "vendored/copy.rs"),
        "a gitignored file reached a handler through candidates: \
         `high-level.md`'s exclusions hold by construction or not at all, and \
         `ExternalDependency` stops being a measured abstention the moment \
         they do not"
    );
    assert!(
        !found.iter().any(|found| found.ends_with(".md")),
        "candidates ignored the extension filter"
    );
}

#[test]
fn a_candidate_set_names_the_enumeration_it_came_from() {
    let root = fixture("generation");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));

    assert_eq!(
        view.candidates(&[RUST], &origin).generation(),
        Generation::FIRST,
        "resolution.md §3 has candidates carry the generation so a caller can \
         report staleness; nothing refreshes the list yet, so the only \
         generation there is is the first one"
    );
}

#[test]
fn lookup_refuses_a_path_the_walker_did_not_return() {
    let root = fixture("lookup");
    let view = view(&root);
    let inside = RelPath::new(Path::new("src/lib.rs")).expect("a relative path");
    let outside = RelPath::new(Path::new("vendored/copy.rs")).expect("a relative path");
    let root_handle = ProjectRoot::new(&root);

    assert!(view.lookup(&root_handle, &inside).is_some());
    assert!(
        view.lookup(&root_handle, &outside).is_none(),
        "lookup minted a ProjectPath for a file the file list does not hold, \
         which is the other half of the scope rule candidates enforces"
    );
}

#[test]
fn a_scan_matches_whole_tokens_only() {
    let root = fixture("tokens");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));
    let candidates = view.candidates(&[RUST], &origin);
    let request = ScanRequest::new("alpha", &candidates).expect("an identifier literal");

    let outcome = view.scan(&request).expect("an unbounded scan");
    let found: Vec<(String, u32)> = outcome
        .hits
        .iter()
        .flat_map(|file| {
            let path = rel_of(&file.path);
            file.hits.iter().map(move |hit| (path.clone(), hit.line.0))
        })
        .collect();

    assert_eq!(
        found,
        [
            ("src/lib.rs".to_owned(), 0),
            ("src/lib.rs".to_owned(), 3),
            // After `src/deep/inner.rs`, which is a nearer candidate and has
            // only the near miss: hits come back in candidate order, not path
            // order, which is what makes the scan's output as reproducible as
            // the candidate list.
            ("other/far.rs".to_owned(), 1),
        ],
        "the literal prefilter is a word-boundary scan: `alphabet`, `_alpha` \
         and `alpha1` all contain the bytes and none of them is the token. A \
         substring match would hand every one of them to the parse stage, \
         which is the cost `resolution.md` §4's prefilter exists to avoid"
    );
    for file in &outcome.hits {
        for hit in &file.hits {
            assert_eq!(
                hit.range.end.0 - hit.range.start.0,
                "alpha".len(),
                "a hit's range must be the token's, since it is what a \
                 Location is built from"
            );
        }
    }
}

#[test]
fn a_scan_reads_every_candidate_and_counts_what_it_read() {
    let root = fixture("exhaustive");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));
    let candidates = view.candidates(&[RUST], &origin);
    let expected_files = candidates.count();
    let request = ScanRequest::new("alpha", &candidates).expect("an identifier literal");

    let outcome = view.scan(&request).expect("an unbounded scan");

    assert_eq!(
        outcome.files_scanned, expected_files,
        "resolution.md §1.3 makes the scan exhaustive, and files_scanned is \
         what says so in the trace record — a scan that stopped early and \
         reported a full count is the one failure the record could not show"
    );
    assert!(
        outcome.bytes_scanned.0 > 0,
        "bytes_scanned is bytes actually read (conformance-005), so a scan \
         that read four files cannot report zero"
    );
    assert_eq!(
        outcome.hits.len(),
        2,
        "every candidate file holds the token, and only the two the fixture \
         puts it in should come back"
    );
}

#[test]
fn a_scan_past_its_deadline_reports_nothing_rather_than_less() {
    let root = fixture("deadline");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));
    let candidates = view.candidates(&[RUST], &origin);
    let request = ScanRequest::new("alpha", &candidates).expect("an identifier literal");

    let arrived_at = SystemClock.now();
    let clock = Arc::new(FrozenClock(arrived_at + Duration::from_millis(1))) as Arc<dyn Clock>;
    let expired = Deadline::new(clock, arrived_at, Duration::ZERO);
    let stopped = ProjectView::new(Arc::new(file_list(&root)), expired, grammar());

    match stopped.scan(&request) {
        Err(Error::Handler(HandlerError::DeadlineExpired)) => {}
        other => panic!(
            "an expired deadline produced {other:?}. resolution.md §4 has no \
             partial-scan outcome to report, because a partial scan cannot \
             tell the only definition of a name from the first of eleven — so \
             the expiry has nowhere to go but the Err"
        ),
    }
}

#[test]
fn a_scan_request_refuses_a_literal_that_is_not_an_identifier() {
    let root = fixture("literal");
    let view = view(&root);
    let origin = SearchOrigin::from_document(path(&view, &root, "src/lib.rs"));
    let candidates = view.candidates(&[RUST], &origin);

    for literal in ["", "fn alpha", "alpha(", "1alpha"] {
        assert!(
            ScanRequest::new(literal, &candidates).is_none(),
            "{literal:?} was accepted as a search literal. resolution.md §4 \
             starts every search from an exact identifier, and a request that \
             matches nothing at full cost abstains with NoCandidates — a claim \
             about the project rather than about the query"
        );
    }
    assert!(ScanRequest::new("alpha", &candidates).is_some());
}

/// `resolution.md` §4's split, end to end through the view: the literal scan
/// finds and the parse decides. Both halves have to come from the same object
/// or a handler is deciding against a tree it built itself, out of a file it
/// reached some other way.
#[test]
fn the_scan_finds_and_the_parse_decides() {
    let root = fixture("parse");
    let view = view(&root);
    let document = path(&view, &root, "src/lib.rs");

    let text = view.read(&document).expect("reading a candidate");
    let tree = view
        .parse(&document, &text)
        .expect("a tree for a candidate the view itself handed out");

    assert_eq!(tree.root_node().kind(), "source_file");
    assert_eq!(
        tree.root_node().end_byte(),
        text.len().0,
        "the tree does not cover the text it was parsed from, so a byte range \
         from one cannot be read against the other"
    );

    let origin = SearchOrigin::from_document(document.clone());
    let candidates = view.candidates(&[RUST], &origin);
    let request = ScanRequest::new("alpha", &candidates).expect("an identifier literal");
    let outcome = view.scan(&request).expect("an unbounded scan");
    let file = outcome.hits.first().expect("hits in src/lib.rs");
    let hit = file.hits.first().expect("the first hit");

    assert_eq!(file.path, document);
    let node = tree
        .root_node()
        .named_descendant_for_byte_range(hit.range.start.0, hit.range.end.0)
        .expect("a node covering the hit");
    assert_eq!(
        (node.kind(), node.byte_range()),
        ("identifier", hit.range.start.0..hit.range.end.0),
        "a hit's byte range has to land exactly on the token the grammar sees, \
         or the parse cannot be what accepts or rejects it — which is the one \
         thing resolution.md §4 will not let a lexical rule do"
    );
}

fn view(root: &Path) -> ProjectView {
    ProjectView::new(Arc::new(file_list(root)), Deadline::none(), grammar())
}

/// DECISION-conformance-012: provisional. The grammar reaches `parse` through
/// the constructor.
fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn file_list(root: &Path) -> FileList {
    let roots = [root.to_path_buf()];
    FileList::enumerate(&roots).expect("enumerating the fixture")
}

/// The one clock a test may read: `clippy.toml` bans `Instant::now`, and
/// `disallowed_methods` does not honour `allow-*-in-tests`.
#[derive(Debug)]
struct FrozenClock(Instant);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

fn path(view: &ProjectView, root: &Path, relative: &str) -> ProjectPath {
    let rel = RelPath::new(Path::new(relative)).expect("a relative path");
    view.lookup(&ProjectRoot::new(root), &rel)
        .unwrap_or_else(|| panic!("{relative} is not in the fixture file list"))
}

fn rel_paths(view: &ProjectView, origin: &SearchOrigin) -> Vec<String> {
    view.candidates(&[RUST], origin)
        .paths()
        .map(rel_of)
        .collect()
}

fn rel_of(path: &ProjectPath) -> String {
    path.rel().as_path().to_string_lossy().replace('\\', "/")
}

/// One workspace root with a `.gitignore`. The empty `.git` directory is not
/// decoration: the `ignore` crate applies `.gitignore` rules only inside a
/// repository, so without it the exclusion under test silently does not apply
/// and the test passes for the wrong reason.
fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
    fs::write(root.join(".gitignore"), "vendored/\n").expect("the fixture gitignore");
    for (relative, contents) in FILES {
        let file = root.join(relative);
        fs::create_dir_all(file.parent().expect("a parent directory"))
            .expect("a fixture directory");
        fs::write(&file, contents).expect("a fixture file");
    }

    root
}
