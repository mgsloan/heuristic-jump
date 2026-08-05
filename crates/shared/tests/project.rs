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
use std::time::Duration;

use shared::{
    Clock, Deadline, Error, FileExtension, FileList, Generation, HandlerError, ProjectPath,
    ProjectRoot, ProjectView, RelPath, ScanRequest, SearchOrigin, Stratum, TestClock,
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

    let clock = Arc::new(TestClock::new());
    let arrived_at = clock.now();
    let expired = Deadline::new(
        Arc::clone(&clock) as Arc<dyn Clock>,
        arrived_at,
        Duration::ZERO,
    );
    clock.advance(Duration::from_millis(1));
    let stopped = ProjectView::new(Arc::new(file_list(&root)), expired, grammar());

    match stopped.scan(&request) {
        Err(Error::Handler(HandlerError::DeadlineExpired { classified: None })) => {}
        other => panic!(
            "an expired deadline produced {other:?}. resolution.md §4 has no \
             partial-scan outcome to report, because a partial scan cannot \
             tell the only definition of a name from the first of eleven — so \
             the expiry has nowhere to go but the Err"
        ),
    }
}

/// `core-025` (accepted, option C): "`ProjectView`'s expiry carries the strata
/// the handler had, as a change to `Error`".
///
/// The case the record says will dominate in the field. `resolution.md` §8
/// assigns the prior from the reference *before* the search, and the search is
/// where the I/O is — so a handler that knew its stratum and then `?`-propagated
/// an expired read is the common shape, and the seam's `Result<Outcome, Error>`
/// gives that `Err` no outcome to carry the class out on. Without this the query
/// lands in `core.md` §7's coverage denominator under a class nobody asked
/// about, which is `core-017`'s defect one layer down and behind the seam.
///
/// Both halves are asserted, because either alone passes on a mistake: an
/// expiry that carries a stratum nothing published would be a synthesised
/// answer, and one that drops a published stratum is the defect itself.
#[test]
fn an_expired_read_carries_out_the_prior_the_handler_published() {
    let root = fixture("classified_expiry");
    let candidate = path(&view(&root), &root, "src/lib.rs");

    for published in [None, Some(Stratum::ExplicitImport)] {
        let clock = Arc::new(TestClock::new());
        let arrived_at = clock.now();
        let expired = Deadline::new(
            Arc::clone(&clock) as Arc<dyn Clock>,
            arrived_at,
            Duration::ZERO,
        );
        clock.advance(Duration::from_millis(1));
        let stopped = ProjectView::new(Arc::new(file_list(&root)), expired, grammar());
        if let Some(prior) = published {
            stopped.classified(prior);
        }

        match stopped.read(&candidate) {
            Err(Error::Handler(HandlerError::DeadlineExpired { classified })) => assert_eq!(
                classified, published,
                "a read refused on the deadline reported {classified:?} for a handler that \
                 published {published:?}. core-025 is accepted on option C precisely so this \
                 survives: core.md §7 reports coverage on stratum_prior, and an expiry that \
                 loses it moves the query into a row it was never asked about"
            ),
            other => panic!(
                "a read past its deadline produced {other:?} rather than the one error class \
                 core.md §1 maps back to an abstention"
            ),
        }
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

/// `deps.md` §8, as CHANGE-core-017 corrected it: the disk-file half of the
/// parse cache key names no cache, because `conformance-005` was answered no
/// to one reached through the `Sync` `&Query`. So a second parse of a path is
/// a fresh parse and a second read is a fresh read.
///
/// The rewrite is a *different text of the same length*, which is the one edit
/// `(path, mtime, len)` — the key §8 named — cannot see on a filesystem whose
/// mtime is second-granular (`open-questions.md` question 5, which the
/// deferral leaves open). A cache under that key serves the first tree for the
/// second text, so this is the assertion that discriminates against it rather
/// than against caching in general.
#[test]
fn a_second_parse_of_the_same_path_is_a_fresh_parse() {
    let root = fixture("freshness");
    let view = view(&root);
    let document = path(&view, &root, "src/util.rs");

    let before = view.read(&document).expect("reading a candidate");
    let first = view
        .parse(&document, &before)
        .expect("a tree for a candidate the view itself handed out");
    assert_eq!(
        first.root_node().named_child(0).map(|node| node.kind()),
        Some("function_item"),
        "the fixture's src/util.rs is a function, and the rest of this test \
         means nothing if the first parse did not see one"
    );

    // Same byte length, different shape: `fn beta() {}` and `struct Beta;`.
    let rewritten = "struct Beta;\n";
    fs::write(document.to_absolute(), rewritten).expect("rewriting a fixture file");
    let after = view.read(&document).expect("re-reading the rewritten file");
    assert_eq!(
        after.len(),
        before.len(),
        "the rewrite changed the file's length, so a (path, mtime, len) key \
         would notice it and this test no longer discriminates against one"
    );

    assert_eq!(
        after.chunks().collect::<String>(),
        rewritten,
        "the second read returned the first read's text. There is no per-query \
         read cache (conformance-005, accepted), and bytes_scanned counts bytes \
         actually read on the strength of that"
    );
    assert_eq!(
        view.parse(&document, &after)
            .expect("a tree for the rewritten file")
            .root_node()
            .named_child(0)
            .map(|node| node.kind()),
        Some("struct_item"),
        "the second parse returned the first parse's tree. deps.md §8's \
         disk-file key names a cache that does not exist and may not be added \
         here: ProjectView is reached through the Sync &Query several fan-out \
         threads hold, so a cache on it is a lock in a design that has none"
    );
}

/// `core.md` §1's bullet on `ProjectView`, as CHANGE-core-035 corrected it:
/// the view caches nothing and fans out onto nothing, and both are rulings
/// rather than omissions — `conformance-005` answered **no** to a cache reached
/// through the `Sync` `&Query`, and `CLAUDE.md` withholds the pool until a
/// corpus and a benchmark exist.
///
/// The behavioural tests above discriminate against a cache that changes an
/// answer, which is the cache somebody adds deliberately. This is for the one
/// that arrives with a plausible reason and no visible effect — a memoised
/// root, a small map of paths already read — on a type every handler holds and
/// where memoisation therefore looks free. `document.rs`'s equivalent scan
/// bans a list of primitives by name; that shape is unavailable here, because
/// `classified` is an `AtomicU8` on purpose (`core-025`, option C) and a
/// blocklist that admits `Atomic` admits the memoisation too.
///
/// So it is an equality, and over `name: Type` rather than names alone: the
/// likelier hole is not a fifth field but one of these four changing type, and
/// a scan reading names would call `files: Arc<Mutex<FileList>>` unchanged.
/// That is the mistake CHANGE-core-032 found in `handler.rs`'s variant
/// comparison, which compared `AbstainReason`'s variant names and never its
/// payloads.
#[test]
fn the_view_holds_no_cache_and_no_pool() {
    assert_eq!(
        fields(&source(), "pub struct ProjectView {"),
        [
            // The walk, shared by every query in the same generation.
            "files: Arc<FileList>",
            // Per query, which is what lets `read` check it without a
            // deadline argument on every method.
            "deadline: Deadline",
            // `conformance-012` (answered): the one language a query can be for.
            "grammar: Language",
            // `core-025` (accepted, option C): the prior an expiry carries out.
            "classified: AtomicU8",
        ],
        "`ProjectView`'s fields are not the four `core.md` §1 names. A fifth, or \
         one of these four acquiring a map or a pool, is `conformance-005` \
         reversed by a diff rather than by a ruling: the view is reached through \
         the `Sync` `&Query` that fan-out threads hold, so state added here is a \
         lock in a design that has none, and `rayon`'s absence from `shared` \
         (§9's deferred list) is the same answer's other half"
    );
}

/// The fields of a declaration, as `name: Type`, in order. Scanning rather than
/// parsing, for the reason `handler.rs` gives: the source is rustfmt's output,
/// so a field is a line and its type is what follows the first colon.
fn fields(text: &str, header: &str) -> Vec<String> {
    let start = text
        .find(header)
        .map(|at| at + header.len())
        .unwrap_or_else(|| panic!("`{header}` is not declared in the source this scan reads"));
    let mut declared = Vec::new();
    for line in text[start..].lines() {
        let trimmed = line.trim();
        if trimmed == "}" {
            break;
        }
        // The doc comments on these fields name `conformance-012` and a lock,
        // and a scan that read prose would report the reason a thing is absent
        // as the thing.
        if trimmed.starts_with("//") || trimmed.starts_with("#[") || trimmed.is_empty() {
            continue;
        }
        let Some((name, kind)) = trimmed.trim_end_matches(',').split_once(':') else {
            continue;
        };
        declared.push(format!(
            "{}: {}",
            name.trim().trim_start_matches("pub "),
            kind.trim()
        ));
    }
    declared
}

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/project.rs");
    fs::read_to_string(&path).expect("the view this section describes")
}

fn view(root: &Path) -> ProjectView {
    ProjectView::new(Arc::new(file_list(root)), Deadline::none(), grammar())
}

/// `conformance-012` (answered). The grammar reaches `parse` through
/// the constructor.
fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn file_list(root: &Path) -> FileList {
    let roots = [root.to_path_buf()];
    FileList::enumerate(&roots).expect("enumerating the fixture")
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
