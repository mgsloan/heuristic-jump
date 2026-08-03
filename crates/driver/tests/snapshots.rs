//! `design/core.md` §2: the dispatch wrapper — not `core`, and not the handler
//! — turns a `SnapshotSeed` into the `DocumentSnapshot` a handler is given.
//!
//! What that buys is a property nothing else in the build would notice losing.
//! `dispatch` taking an already-parsed snapshot compiles, runs, and passes
//! every other test in this crate; what it gives up is that the parse is paid
//! on the worker thread and inside the deadline, which only shows up as a
//! single-threaded `core` stalling on somebody's large generated file.
//!
//! So the assertion here is about *ordering*: the handler is not reached at
//! all when the parse runs out of time. A wrapper that parsed after calling
//! the handler, or that let `core` parse, cannot produce that.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the fixture builders below are free functions. Failing loudly is the point: a half-built fixture leaves an empty file list, which every assertion here passes against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use driver::{Dispatched, Request, dispatch};
use shared::proto::PositionEncoding;
use shared::{
    ByteOffset, Clock, CommitPolicy, Confidence, Deadline, DocumentUri, DocumentVersion, Error,
    FileExtension, FileList, LanguageHandler, LanguageId, Outcome, ProjectView, Query, Rope,
    ServerProfile, SnapshotSeed, Stratum, SystemClock,
};
use tree_sitter::Language;

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

    let dispatched = dispatch(
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

    match dispatched {
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

    let dispatched = dispatch(
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

    match dispatched {
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

/// Abstains, and records that it was asked. An `AtomicBool` rather than a cell
/// because `LanguageHandler` is `Sync` and the seam has no interior
/// mutability to borrow — and because `CLAUDE.md` has no locks anywhere.
#[derive(Default)]
struct Recording {
    called: AtomicBool,
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
        Ok(query
            .policy
            .decide(Stratum::LocalBinding, Confidence::ONE, Vec::new()))
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

fn view(root: &Path, deadline: &Deadline) -> ProjectView {
    let roots = [root.to_path_buf()];
    let files = FileList::enumerate(&roots).expect("enumerating the fixture");
    ProjectView::new(Arc::new(files), deadline.clone(), grammar())
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
