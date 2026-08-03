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
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the fixture builders below are free functions. Failing loudly is the point: a half-built fixture leaves an empty file list, which every assertion here passes against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use driver::{DebounceMs, Dispatched, FileListCache, OpenDocument, Request, TreeCache, dispatch};
use shared::proto::PositionEncoding;
use shared::{
    ByteOffset, Clock, CommitPolicy, Confidence, Deadline, DocumentUri, DocumentVersion, Error,
    FileExtension, InputEdit, LanguageHandler, LanguageId, Outcome, ParseKind, ProjectView, Query,
    Rope, ServerProfile, SnapshotSeed, Strata, Stratum, SystemClock, Trace,
};
use tree_sitter::{Language, Point};

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// The clock the deadline tests read, since `clippy.toml` bans `Instant::now`.
#[derive(Debug)]
struct FrozenClock(Instant);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

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
    let started = SystemClock.now();
    let budget = Duration::from_millis(20);
    let clock = FrozenClock(started + budget + Duration::from_millis(1));
    let deadline = Deadline::new(Arc::new(clock), started, budget);
    let view = view(&root, &deadline);
    let handler = Recording::default();

    let completed = dispatch(
        &handler,
        request(
            seed(&root),
            &view,
            &deadline,
            &ServerProfile { id: None },
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );

    match completed.dispatched {
        Dispatched::DeadlineExpired => {}
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
            &ServerProfile { id: None },
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );

    match completed.dispatched {
        Dispatched::Decided(_) => {}
        other @ (Dispatched::DeadlineExpired | Dispatched::Failed(_)) => {
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

    let first = cache.seed(&open(
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
            &ServerProfile { id: None },
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

    let second = cache.seed(&open(&uri, &edited, 2, &edits));
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
            &ServerProfile { id: None },
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    match completed.dispatched {
        Dispatched::Decided(_) => {}
        other @ (Dispatched::DeadlineExpired | Dispatched::Failed(_)) => {
            panic!("the reparse from a cached base did not decide: {other:?}")
        }
    }
    assert_eq!(
        handler.spanned.load(Ordering::Relaxed),
        edited.len(),
        "the handler was given a tree that stops somewhere other than the end of its \
         text, which is the disagreement core.md §2 says cannot happen"
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
    let text = Rope::from(document().as_str());

    for version in [4, 2] {
        let seed = cache.seed(&open(&uri, &text, version, &no_edits()));
        let completed = dispatch(
            &handler,
            request(
                seed,
                &view,
                &deadline,
                &ServerProfile { id: None },
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
    let text = Rope::from(document().as_str());

    let seed = cache.seed(&open(&uri, &text, 1, &no_edits()));
    let completed = dispatch(
        &handler,
        request(
            seed,
            &view,
            &deadline,
            &ServerProfile { id: None },
            &CommitPolicy::permissive(),
        ),
        PositionEncoding::Utf16,
    );
    cache.insert(completed.parsed.expect("the tree the worker parsed"));
    cache.forget(&uri);

    assert_eq!(cache.version(&uri), None);
    assert_eq!(
        cache.seed(&open(&uri, &text, 2, &no_edits())).parse_kind(),
        ParseKind::Full,
        "a forgotten document still produced an incremental seed"
    );
}

fn open<'a>(
    uri: &'a DocumentUri,
    text: &'a Rope,
    version: i32,
    edits: &'a Arc<Vec<InputEdit>>,
) -> OpenDocument<'a> {
    OpenDocument {
        uri,
        text,
        version: DocumentVersion(version),
        language_id: LanguageId::new("rust"),
        grammar: grammar(),
        edits,
    }
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
        position: ByteOffset(0),
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
