//! `design/core.md` §2: the dispatch wrapper — not `core`, and not the handler
//! — turns a `SnapshotSeed` into the `DocumentSnapshot` a handler is given.
//!
//! What that buys is a property nothing else in the build would notice losing.
//! `dispatch` taking an already-parsed snapshot compiles, runs, and passes
//! every other test in this crate; what it gives up is that the parse is paid
//! on the worker thread and inside the deadline, which only shows up as a
//! single-threaded `core` stalling on somebody's large generated file.
//!
//! So the first assertions here are about *ordering*: the handler is not
//! reached at all when the parse runs out of time. A wrapper that parsed after
//! calling the handler, or that let `core` parse, cannot produce that.
//!
//! The rest close §2's loop — `TreeCache::seed`, `dispatch`, `Parsed`,
//! `TreeCache::insert`, and a seed that reparses from what the last one
//! produced. That half needs `ParseKind` to be observable at all, because an
//! incremental reparse and a full one produce the same tree for the same text;
//! the safety of the optimisation is exactly what hides whether it happened.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the fixture builders below are free functions. Failing loudly is the point: a half-built fixture leaves an empty file list, which every assertion here passes against."
)]

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use driver::{
    CacheBytes, CacheEntries, DebounceMs, Dispatched, Documents, FileListCache, OpenDocument,
    Queried, Registry, Request, Synced, TreeCache, dispatch,
};
use proptest::prelude::{Just, ProptestConfig, Strategy, prop_assert_eq};
use proptest::prop_oneof;
use proptest::test_runner::{FileFailurePersistence, TestCaseError, TestRunner};
use serde_json::value::RawValue;
use shared::proto::PositionEncoding;
use shared::{
    ByteLen, ByteRange, Clock, CommitPolicy, Confidence, Deadline, DocumentUri, DocumentVersion,
    Error, FileExtension, InputEdit, LanguageHandler, LanguageId, Offset, Outcome, ParseKind,
    ProjectView, Query, Rope, ServerProfile, SnapshotSeed, Strata, Stratum, SystemClock, TestClock,
    Trace, input_edit,
};
use tree_sitter::{Language, Parser, Point, Tree};

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// The parse is in front of the handler, and the deadline is in front of the
/// parse.
///
/// Asserted by the handler *not being called*, which is the only way to tell
/// the wrapper's ordering apart from `hard_cap`'s: a wrapper that ran the
/// handler and then noticed the expiry returns the same `DeadlineExpired`, and
/// has already spent the whole query budget doing it.
#[test]
fn a_parse_that_runs_out_of_time_never_reaches_the_handler() {
    let root = fixture("expired_parse");
    let clock = Arc::new(TestClock::new());
    let started = clock.now();
    let budget = Duration::from_millis(20);
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, started, budget);
    clock.advance(budget + Duration::from_millis(1));
    let view = view(&root, &deadline);
    let handler = Recording::default();

    let completed = dispatch(
        &handler,
        request(
            seed(&root),
            &view,
            &deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );

    match completed.dispatched {
        Dispatched::DeadlineExpired(_) => {}
        other @ (Dispatched::Decided(_) | Dispatched::Failed(_)) => panic!(
            "a parse abandoned on the deadline came back as {other:?}, where core.md §2 \
             pays it inside the deadline and §1 maps that one class back to an abstention"
        ),
    }
    assert!(
        !handler.called.load(Ordering::Relaxed),
        "the handler ran, so the parse happened after the call or not at all: core.md §2 \
         puts it in front, which is what makes DocumentSnapshot::tree infallible"
    );
}

/// The same query with time left decides normally, so the test above is about
/// the deadline rather than about the fixture being unparseable.
#[test]
fn the_same_query_with_time_left_reaches_the_handler() {
    let root = fixture("live_parse");
    let deadline = Deadline::none();
    let view = view(&root, &deadline);
    let handler = Recording::default();

    let completed = dispatch(
        &handler,
        request(
            seed(&root),
            &view,
            &deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );

    match completed.dispatched {
        Dispatched::Decided(_) => {}
        other @ (Dispatched::DeadlineExpired(_) | Dispatched::Failed(_)) => {
            panic!("the query did not decide: {other:?}")
        }
    }
    assert!(
        handler.called.load(Ordering::Relaxed),
        "the handler was not reached even with an unbounded deadline"
    );
}

/// §2's loop, one full turn: `core` builds a seed from the cache, the worker
/// realises it and hands back a `Parsed`, the cache takes it, and the *next*
/// seed for that document reparses from it.
///
/// The kind is asserted rather than the tree, because the two paths produce
/// the same tree for the same text — which is what makes the optimisation safe
/// and what makes it invisible. Before this, nothing in the workspace ever
/// filled `SnapshotSeed::incremental`'s base, and every test still passed.
#[test]
fn the_tree_a_worker_parsed_is_what_the_next_seed_reparses_from() {
    let root = fixture("warm_cache");
    let deadline = Deadline::none();
    let view = view(&root, &deadline);
    let handler = Recording::default();
    let uri = uri_of(&root.join("src").join("lib.rs"));
    let mut cache = TreeCache::default();
    let mut documents = Documents::new();
    let registry = registry();

    let first = cache.seed(&open(
        &mut documents,
        &registry,
        &uri,
        &Rope::from(document().as_str()),
        1,
        &no_edits(),
    ));
    assert_eq!(
        first.parse_kind(),
        ParseKind::Full,
        "an empty cache produced an incremental seed, so its base is a tree nobody parsed"
    );
    assert_eq!(cache.version(&uri), None);

    let completed = dispatch(
        &handler,
        request(
            first,
            &view,
            &deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    let parsed = completed
        .parsed
        .expect("a query whose parse succeeded hands the tree back");
    assert_eq!(parsed.uri(), &uri);
    assert_eq!(parsed.version(), DocumentVersion(1));
    cache.insert(parsed);

    assert_eq!(
        cache.version(&uri),
        Some(DocumentVersion(1)),
        "the cache took the tree and then did not have it"
    );

    let addition = "fn appended() {}\n";
    let edited = Rope::from(format!("{}{addition}", document()).as_str());
    let edits = Arc::new(vec![InputEdit {
        start_byte: document().len(),
        old_end_byte: document().len(),
        new_end_byte: document().len() + addition.len(),
        start_position: Point::new(800, 0),
        old_end_position: Point::new(800, 0),
        new_end_position: Point::new(801, 0),
    }]);

    let second = cache.seed(&open(&mut documents, &registry, &uri, &edited, 2, &edits));
    assert_eq!(
        second.parse_kind(),
        ParseKind::Incremental,
        "core.md §2: the cached tree is what the next query starts warm from, and \
         nothing else fills SnapshotSeed::incremental's base"
    );

    let completed = dispatch(
        &handler,
        request(
            second,
            &view,
            &deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    match completed.dispatched {
        Dispatched::Decided(_) => {}
        other @ (Dispatched::DeadlineExpired(_) | Dispatched::Failed(_)) => {
            panic!("the reparse from a cached base did not decide: {other:?}")
        }
    }
    assert_eq!(
        ByteLen(handler.spanned.load(Ordering::Relaxed)),
        edited.len(),
        "the handler was given a tree that stops somewhere other than the end of its \
         text, which is the disagreement core.md §2 says cannot happen"
    );
}

/// `deps.md` §8's whole content, which is a caveat rather than a crate choice:
///
/// > **`lru`**, with a caveat: `shim.md` §5 wants the cache bounded by *both*
/// > entry count and total bytes, and `lru` bounds only entries. So `driver`
/// > wraps it — track a running byte total, and after each `put`, `pop_lru`
/// > until under the byte ceiling.
///
/// Both bounds are asserted, and separately, because either one alone passes a
/// cache that has the other. The cache was previously bounded by neither: an
/// unbounded map with a `forget` on `didClose`, which holds every tree a long
/// session ever parsed for a document nobody closed.
///
/// The eviction is asserted through `version()` — a `None` for a document that
/// was cached a moment ago is exactly the cold miss `shim.md` §5 calls correct.
/// What would *not* be correct is the byte total drifting, which is why the
/// second half fills past the ceiling twice: a total that only ever grew would
/// evict on every insert forever, and one entry surviving each round is what
/// says it did not.
#[test]
fn the_parse_cache_is_bounded_by_entries_and_by_bytes() {
    let root = fixture("bounded_cache");
    let deadline = Deadline::none();
    let mut shared = Fixture {
        documents: Documents::new(),
        registry: registry(),
        view: view(&root, &deadline),
        deadline: &deadline,
        handler: Recording::default(),
    };

    let text = Rope::from(document().as_str());
    let uris: Vec<DocumentUri> = (0..3)
        .map(|index| uri_of(&root.join("src").join(format!("file{index}.rs"))))
        .collect();

    // Three documents, two slots, no byte pressure at all.
    let mut by_entries = TreeCache::new(
        CacheEntries::new(NonZeroUsize::new(2).expect("two is not zero")),
        CacheBytes::new(ByteLen::MAX),
    );
    for uri in &uris {
        warm(&mut by_entries, &mut shared, uri, &text);
    }
    assert_eq!(
        by_entries.version(&uris[0]),
        None,
        "the third document did not push the first out of a two-entry cache: lru's own bound \
         is the half deps.md §8 does not wrap, so a cache that ignores it is not an lru at all"
    );
    for uri in &uris[1..] {
        assert_eq!(
            by_entries.version(uri),
            Some(DocumentVersion(1)),
            "a two-entry cache holding fewer than two: eviction is running past the bound, and \
             a cache that evicts what it just took is a parse paid for twice every time"
        );
    }

    // Room for three by entry count, room for two by bytes. `document()` is
    // the same text every time, so the ceiling is stated in whole documents.
    let ceiling = ByteLen(text.len().0.saturating_mul(2));
    let mut by_bytes = TreeCache::new(
        CacheEntries::new(NonZeroUsize::new(16).expect("sixteen is not zero")),
        CacheBytes::new(ceiling),
    );
    for uri in &uris {
        warm(&mut by_bytes, &mut shared, uri, &text);
    }
    assert_eq!(
        by_bytes.version(&uris[0]),
        None,
        "three documents fit under a two-document ceiling: shim.md §5 wants bytes bounded \
         because a single generated file can be enormous, and an entry count does not see size"
    );
    assert_eq!(
        by_bytes.version(&uris[2]),
        Some(DocumentVersion(1)),
        "the document that was just inserted is not in the cache, so pop_lru ran past the \
         ceiling rather than until under it"
    );

    // A second round over the same ceiling. If the running total had not been
    // decremented on eviction it would now sit above the ceiling permanently,
    // and every insert from here on would evict everything.
    let more: Vec<DocumentUri> = (3..6)
        .map(|index| uri_of(&root.join("src").join(format!("file{index}.rs"))))
        .collect();
    for uri in &more {
        warm(&mut by_bytes, &mut shared, uri, &text);
    }
    assert_eq!(
        by_bytes.version(&more[2]),
        Some(DocumentVersion(1)),
        "after six inserts the cache holds nothing: the byte total is drifting up on eviction, \
         which turns the ceiling into a cache that discards every tree it is given"
    );
}

/// A tree older than the cached one is dropped rather than stored.
///
/// Not decoration: dispatch is parallel, so two workers can be realising seeds
/// for the same document at once and the one that finishes last is not the one
/// parsed from the newest text. Overwriting would leave the base at a version
/// the edit log no longer describes, and tree-sitter's incremental parse is
/// only correct when the edits it is handed are.
#[test]
fn a_tree_older_than_the_cached_one_does_not_replace_it() {
    let root = fixture("stale_tree");
    let deadline = Deadline::none();
    let view = view(&root, &deadline);
    let handler = Recording::default();
    let uri = uri_of(&root.join("src").join("lib.rs"));
    let mut cache = TreeCache::default();
    let mut documents = Documents::new();
    let registry = registry();
    let text = Rope::from(document().as_str());

    for version in [4, 2] {
        let seed = cache.seed(&open(
            &mut documents,
            &registry,
            &uri,
            &text,
            version,
            &no_edits(),
        ));
        let completed = dispatch(
            &handler,
            request(
                seed,
                &view,
                &deadline,
                &ServerProfile::standalone(),
                &CommitPolicy::permissive(),
            ),
            PositionEncoding::Utf16,
        );
        cache.insert(completed.parsed.expect("the tree the worker parsed"));
    }

    assert_eq!(
        cache.version(&uri),
        Some(DocumentVersion(4)),
        "a v2 tree arriving after a v4 one replaced it, so the next incremental parse \
         reconciles the wrong base against the edit log"
    );
}

/// `didClose`. A cache that only ever grows leaks in a process that outlives
/// many editors' worth of open files.
#[test]
fn a_forgotten_document_goes_back_to_a_full_parse() {
    let root = fixture("forgotten");
    let deadline = Deadline::none();
    let view = view(&root, &deadline);
    let handler = Recording::default();
    let uri = uri_of(&root.join("src").join("lib.rs"));
    let mut cache = TreeCache::default();
    let mut documents = Documents::new();
    let registry = registry();
    let text = Rope::from(document().as_str());

    let seed = cache.seed(&open(
        &mut documents,
        &registry,
        &uri,
        &text,
        1,
        &no_edits(),
    ));
    let completed = dispatch(
        &handler,
        request(
            seed,
            &view,
            &deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    cache.insert(completed.parsed.expect("the tree the worker parsed"));
    cache.forget(&uri);

    assert_eq!(cache.version(&uri), None);
    assert_eq!(
        cache
            .seed(&open(
                &mut documents,
                &registry,
                &uri,
                &text,
                2,
                &no_edits()
            ))
            .parse_kind(),
        ParseKind::Full,
        "a forgotten document still produced an incremental seed"
    );
}

/// `core.md` §10's first bullet: "for a randomised sequence of edits and
/// dispatches, assert that `snapshot.tree()` always parses to a tree whose
/// extent matches `snapshot.text`, whatever version the cached base was at".
///
/// The randomisation is not decoration, and the reason is the sentence after
/// it: violating this "produces confidently wrong answers rather than errors".
/// Every fixed sequence above dispatches at a staleness somebody chose. What
/// breaks an incremental reparse is a base at a staleness nobody chose — two
/// edits since the cached tree, or none, or a tree the cache refused because a
/// newer one had already landed — and those are states a script written by
/// hand does not think to visit.
///
/// Two assertions per dispatch, and the second is the one with teeth:
///
/// * The tree's extent is the text's length, which is §10's own wording.
/// * The tree is *structurally identical* to a from-scratch parse of the same
///   text. Extent is a scalar and a wrong edit log can preserve it by
///   accident; the shape cannot be preserved by accident. This is what makes
///   the whole `base`-plus-edits design assertable, since §2's point is that
///   the two paths are deliberately indistinguishable in their result — so
///   "indistinguishable" is the property, and it is checked rather than
///   assumed.
///
/// It runs its own `TestRunner` rather than sitting in a `proptest!` block
/// because the fixture and the `ProjectView` cost a directory and a scanner
/// thread apiece, and a macro body would build both once per generated case.
#[test]
fn the_tree_matches_the_text_at_every_staleness() {
    let root = fixture("staleness");
    let deadline = Deadline::none();
    let view = view(&root, &deadline);
    let uri = uri_of(&root.join("src").join("lib.rs"));
    let registry = registry();

    let mut runner = TestRunner::new(ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/snapshots.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    });
    runner
        .run(&script(), |script| {
            edit_and_dispatch(&script, &uri, &view, &deadline, &registry)
        })
        .expect("the tree and the text agree at every staleness");
}

/// One generated script, run against one document.
fn edit_and_dispatch(
    script: &[Step],
    uri: &DocumentUri,
    view: &ProjectView,
    deadline: &Deadline,
    registry: &Registry,
) -> Result<(), TestCaseError> {
    let handler = Recording::default();
    let mut cache = TreeCache::default();
    let mut documents = Documents::new();
    let mut text = Rope::from(EDITABLE);
    let mut version = 1_i32;
    // Each edit tagged with the version it produced, so a dispatch can hand
    // over exactly the ones the cached tree has not seen. This is the
    // bookkeeping `core` will own once it exists — `TreeCache::version` is
    // there for it — and doing it here is what makes the staleness vary.
    let mut log: Vec<(DocumentVersion, InputEdit)> = Vec::new();

    for step in script {
        match step {
            Step::Replace { at, span, insert } => {
                let whole: String = text.chunks().collect();
                let start = boundary(&whole, at % (whole.len() + 1));
                let end = boundary(&whole, start + span % 64);
                let replaced = ByteRange {
                    start: Offset(start),
                    end: Offset(end),
                };
                log.push((
                    DocumentVersion(version + 1),
                    input_edit(&text, replaced, insert),
                ));
                text.replace(replaced, insert);
                version += 1;
            }
            Step::Dispatch => {
                let cached = cache.version(uri);
                let outstanding = Arc::new(
                    log.iter()
                        .filter(|(at, _)| cached.is_none_or(|cached| *at > cached))
                        .map(|(_, edit)| *edit)
                        .collect::<Vec<InputEdit>>(),
                );
                let seed = cache.seed(&open(
                    &mut documents,
                    registry,
                    uri,
                    &text,
                    version,
                    &outstanding,
                ));
                let completed = dispatch(
                    &handler,
                    request(
                        seed,
                        view,
                        deadline,
                        &ServerProfile::standalone(),
                        &CommitPolicy::permissive(),
                    ),
                    PositionEncoding::Utf16,
                );
                let parsed = completed
                    .parsed
                    .expect("a query whose parse succeeded hands the tree back");
                prop_assert_eq!(
                    ByteLen(handler.spanned.load(Ordering::Relaxed)),
                    text.len(),
                    "the handler was given a tree that stops somewhere other than the end \
                     of its text, at version {} with {} outstanding edit(s)",
                    version,
                    outstanding.len()
                );
                prop_assert_eq!(
                    handler.shape.load(Ordering::Relaxed),
                    fingerprint_of(&text),
                    "the reparse from a stale base produced a different tree than parsing \
                     the same text from scratch, at version {} with {} outstanding edit(s)",
                    version,
                    outstanding.len()
                );
                cache.insert(parsed);
            }
        }
    }
    Ok(())
}

/// A step in a generated script. `at` and `span` are resolved against the
/// document as it is when the step runs, because a strategy cannot know how
/// long it will be by then — a script is generated once and the eighth edit
/// lands in a document seven edits have already moved.
#[derive(Clone, Debug)]
enum Step {
    Replace {
        at: usize,
        span: usize,
        insert: String,
    },
    Dispatch,
}

/// Fragments to splice in. Deliberately a mix: some keep the document valid
/// Rust and some do not, because a tree with `ERROR` nodes in it is exactly
/// the state an editor is in while somebody types, and it is the state where
/// an incremental reparse has the most to get wrong. The astral character is
/// there because a column is bytes and a wrong conversion shows up as a
/// mismatch only when a character is wider than one.
const FRAGMENTS: &[&str] = &[
    "",
    " ",
    "\n",
    "fn spliced() {}\n",
    "// 😀 a comment\n",
    "{",
    "}",
    "let value = \"é\";\n",
    "(argument: u32)",
    "\n\n",
];

fn script() -> impl Strategy<Value = Vec<Step>> {
    let insert = proptest::collection::vec(proptest::sample::select(FRAGMENTS), 0..3)
        .prop_map(|parts| parts.concat());
    let step = prop_oneof![
        3 => (0_usize..100_000, 0_usize..100_000, insert)
            .prop_map(|(at, span, insert)| Step::Replace { at, span, insert }),
        2 => Just(Step::Dispatch),
    ];
    proptest::collection::vec(step, 1..12)
}

/// The next character boundary at or after `offset`, clamped to the end. An
/// offset inside a scalar value is not a place a replacement can start, and
/// `rope::Rope::replace` would be entitled to anything if handed one.
fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// The shape of a from-scratch parse of `text`, as a number two trees can be
/// compared by. A hash rather than the s-expression itself because the handler
/// records it from another thread through a `Sync` seam, where an `AtomicU64`
/// is available and a `String` would need the lock this design does not have.
fn fingerprint_of(text: &Rope) -> u64 {
    let whole: String = text.chunks().collect();
    let mut parser = Parser::new();
    parser
        .set_language(&grammar())
        .expect("the grammar the fixture is written in");
    let tree = parser
        .parse(&whole, None)
        .expect("a parse with no deadline and no cancellation produces a tree");
    fingerprint(&tree)
}

/// Every node's kind, byte range and point range, in walk order.
///
/// An s-expression was the obvious fingerprint and is the wrong one: `to_sexp`
/// prints kinds and nesting and no offsets at all, so a reparse that produced
/// the right shape at the wrong place would compare equal, which is most of
/// what this test exists to catch.
///
/// What it does **not** catch, established by mutating each field of
/// `input_edit` in turn and watching this pass: the edit's three *point*
/// fields. `realise`'s read callback is byte-based, and tree-sitter recomputes
/// the position of everything it re-lexes from the text, so all three can be
/// wrong together and every tree in this file still agrees with a from-scratch
/// parse. They are pinned against a reference implementation instead, in
/// `shared`'s `an_input_edit_describes_the_replacement_it_was_built_from`.
/// Point ranges stay in the hash anyway, because they cost nothing and the
/// callback is not guaranteed to stay byte-based forever.
fn fingerprint(tree: &Tree) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        node.kind_id().hash(&mut hasher);
        (node.start_byte(), node.end_byte()).hash(&mut hasher);
        let (start, end) = (node.start_position(), node.end_position());
        (start.row, start.column, end.row, end.column).hash(&mut hasher);

        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return hasher.finish();
            }
        }
    }
}

/// Small, so a generated script's edits reach all of it, and multi-line, so a
/// row is something an edit can get wrong. `document()` above is 800 lines
/// because a deadline has to be observable while parsing it; nothing here
/// wants that, and paying it sixty-four times over would.
const EDITABLE: &str = "\
fn first(argument: u32) -> u32 {
    argument + 1
}

fn second(argument: u32) -> u32 {
    first(argument) + 2
}

struct Held {
    field: u32,
}
";

/// One full turn of §2's loop for one document, which is what filling the
/// cache takes: a seed, a dispatch that realises it, and the `Parsed` going
/// back in. `insert` is the only write and consumes a value only `dispatch`
/// mints, so a bound cannot be exercised by putting trees in directly.
fn warm(cache: &mut TreeCache, fixture: &mut Fixture<'_>, uri: &DocumentUri, text: &Rope) {
    let edits = no_edits();
    let seed = cache.seed(&open(
        &mut fixture.documents,
        &fixture.registry,
        uri,
        text,
        1,
        &edits,
    ));
    let completed = dispatch(
        &fixture.handler,
        request(
            seed,
            &fixture.view,
            &fixture.deadline,
            &ServerProfile::standalone(),
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    cache.insert(
        completed
            .parsed
            .expect("a query whose parse succeeded hands the tree back"),
    );
}

/// Everything `warm` needs that is the same for every document in one test.
struct Fixture<'a> {
    documents: Documents,
    registry: Registry,
    view: ProjectView,
    deadline: &'a Deadline,
    handler: Recording,
}

/// A document in the map at a version, and the seed input built from it.
///
/// It goes through `Documents` rather than assembling an `OpenDocument`
/// directly because there is no longer a way to assemble one: `core.md` §8.6's
/// rule that an untrusted document cannot be queried is spelled as
/// `OpenDocument::new` taking a `Trusted`, which only `Documents::query`
/// produces. A fixture therefore reaches a seed the way `core` does, or not at
/// all.
fn open<'a>(
    documents: &'a mut Documents,
    registry: &Registry,
    uri: &DocumentUri,
    text: &Rope,
    version: i32,
    edits: &'a Arc<Vec<InputEdit>>,
) -> OpenDocument<'a> {
    let params = did_open(uri, text, version);
    assert_eq!(
        documents.opened(&params, registry),
        Synced::Applied,
        "the fixture's didOpen was not applied, so every assertion below is about an \
         empty document map"
    );
    match documents.query(uri) {
        Queried::Trusted(trusted) => OpenDocument::new(trusted, grammar(), edits),
        other @ (Queried::NotOpen | Queried::Untrusted(_)) => {
            panic!("a document opened a line ago is not queryable: {other:?}")
        }
    }
}

fn did_open(uri: &DocumentUri, text: &Rope, version: i32) -> Box<RawValue> {
    let text: String = text.chunks().collect();
    let params = format!(
        r#"{{"textDocument":{{"uri":{},"languageId":"rust","version":{version},"text":{}}}}}"#,
        json_string(uri.as_str()),
        json_string(&text),
    );
    RawValue::from_string(params).expect("the fixture's didOpen params are JSON")
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("a str is always serializable")
}

/// The handler set, for resolving the `languageId` a `didOpen` carries. A
/// second `Recording` from the one under test: this one is never called, and
/// sharing it would make `called` mean two things.
fn registry() -> Registry {
    Registry::new(vec![Arc::new(Recording::default())])
}

fn no_edits() -> Arc<Vec<InputEdit>> {
    Arc::new(Vec::new())
}

/// Abstains, and records that it was asked. An `AtomicBool` rather than a cell
/// because `LanguageHandler` is `Sync` and the seam has no interior
/// mutability to borrow — and because `CLAUDE.md` has no locks anywhere.
#[derive(Default)]
struct Recording {
    called: AtomicBool,
    /// Where the tree it was given stops. Read back to assert that the tree
    /// and the text agree, which is the property an incremental reparse from
    /// a stale base would break.
    spanned: AtomicUsize,
    /// The shape of that tree, as a hash of its s-expression. Extent is a
    /// scalar and a wrong edit log can preserve it by accident; this cannot be
    /// preserved by accident, and it is what `the_tree_matches_the_text_at_
    /// every_staleness` compares against a from-scratch parse.
    shape: AtomicU64,
}

impl LanguageHandler for Recording {
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
        self.called.store(true, Ordering::Relaxed);
        self.spanned
            .store(query.doc.tree().root_node().end_byte(), Ordering::Relaxed);
        self.shape
            .store(fingerprint(query.doc.tree()), Ordering::Relaxed);
        Ok(query.policy.decide(
            Strata::from_reference(Stratum::LocalBinding),
            Confidence::ONE,
            Vec::new(),
            Trace::new(),
        ))
    }
}

fn request<'a>(
    seed: SnapshotSeed,
    view: &'a ProjectView,
    deadline: &'a Deadline,
    server: &'a ServerProfile,
    policy: &'a CommitPolicy,
) -> Request<'a> {
    Request {
        seed,
        position: Offset(0),
        project: view,
        deadline,
        server,
        policy,
    }
}

fn seed(root: &Path) -> SnapshotSeed {
    SnapshotSeed::fresh(
        uri_of(&root.join("src").join("lib.rs")),
        Rope::from(document().as_str()),
        DocumentVersion(1),
        LanguageId::new("rust"),
        grammar(),
    )
}

/// Large enough that tree-sitter reports progress while parsing it, which is
/// what makes the deadline observable at all — the callback fires once per 100
/// parser operations (`tree-sitter/src/parser.c`), so a small fixture would
/// parse to completion however expired the query was.
fn document() -> String {
    (0..800)
        .map(|index| {
            format!("fn generated_{index}(argument: u32) -> u32 {{ argument + {index} }}\n")
        })
        .collect()
}

/// Through the cache rather than through `FileList::enumerate`, because
/// `core.md` §4 puts one owner between the walk and every query, and a test
/// that reaches around it is testing a path the driver does not have.
fn view(root: &Path, deadline: &Deadline) -> ProjectView {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    FileListCache::new(vec![root.to_path_buf()], clock, DebounceMs::RESCAN)
        .expect("the scanner thread")
        .view(deadline.clone(), grammar())
        .expect("enumerating the fixture")
}

fn uri_of(path: &Path) -> DocumentUri {
    DocumentUri::from_file_path(path).expect("a file URI for a fixture path")
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

/// One workspace root, with the empty `.git` the `ignore` crate needs before
/// it returns anything at all.
fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        fs::remove_dir_all(&root).expect("clearing a previous run");
    }

    fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
    fs::create_dir_all(root.join("src")).expect("the fixture source directory");
    fs::write(root.join("src").join("lib.rs"), document()).expect("the fixture document");

    root
}
