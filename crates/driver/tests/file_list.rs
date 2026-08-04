//! `design/core.md` §4's three claims about the file list, over a real
//! directory and a real background thread.
//!
//! A real thread because "refreshed in the background" is the claim: a rescan
//! driven from the calling thread would satisfy every assertion below except
//! the one that matters, which is that a query arriving mid-walk gets the list
//! in hand rather than waiting for the new one.
//!
//! A real directory for the reason `shared/tests/project.rs` gives — the
//! walker's exclusions are the thing being tested — and because the point of a
//! rescan is that a file appeared on disk after the first walk.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the fixture builder and answer helpers below are free functions. Failing loudly is the point: a half-built fixture leaves an empty file list, which several assertions here would pass against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use driver::{Answer, DebounceMs, Dispatched, FileListCache, LateStrata};
use shared::{
    AbstainReason, Clock, Confidence, Error, FileList, Generation, Outcome, ProjectError, Strata,
    Stratum, TestClock, Trace,
};

/// Long enough that "before the window closes" is a state a test can be in
/// without racing the clock — which it cannot anyway, since the clock below is
/// driven rather than read.
const DEBOUNCE: DebounceMs = DebounceMs::new(500);

/// How long a negative assertion waits before believing that no rescan was
/// requested. It bounds a thread that would otherwise have finished walking a
/// six-file fixture many times over.
const QUIET: Duration = Duration::from_millis(250);

#[test]
fn the_list_is_walked_once_on_first_need_and_handed_back_by_refcount_after() {
    let root = fixture("first_need");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);

    let first = cache.list().expect("the first walk");
    let second = cache.list().expect("the list in hand");

    assert!(
        Arc::ptr_eq(&first, &second),
        "core.md §4 caches the list; a second walk on the second query is the \
         hundreds of milliseconds the cache exists to avoid"
    );
    assert_eq!(first.generation(), Generation::FIRST);
    assert!(
        relative_paths(&first).contains(&"src/lib.rs".to_owned()),
        "the fixture's files are not in the list, so nothing below is testing a walk"
    );
}

#[test]
fn a_watcher_frame_schedules_a_rescan_that_a_query_never_waits_for() {
    let root = fixture("watcher");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);

    let before = cache.list().expect("the first walk");
    assert!(!relative_paths(&before).contains(&"src/added.rs".to_owned()));
    fs::write(root.join("src/added.rs"), "fn gamma() {}\n").expect("a file created after the walk");

    // The tee takes no payload, which is the claim: one of these frames can
    // carry thousands of events after a branch switch.
    cache.watched_files_changed();

    cache.refresh_if_due();
    assert!(
        cache.rescans().recv_timeout(QUIET).is_err(),
        "the rescan went out before the debounce window closed, so a burst of \
         frames is a walk per frame"
    );

    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();

    // Mid-flight: §4's "a query that arrives while a rescan is in flight uses
    // the list it has". Deterministic rather than racy — the held list changes
    // only in `install`, which has not been called.
    let during = cache.list().expect("the list in hand");
    assert!(Arc::ptr_eq(&before, &during));
    assert_eq!(during.generation(), Generation::FIRST);

    let rescan = cache.rescans().recv_timeout(patience()).expect("a rescan");
    cache.install(rescan);

    let after = cache.list().expect("the refreshed list");
    assert_eq!(
        after.generation(),
        Generation::FIRST.next(),
        "a refresh replaces the list, and the generation is what lets a caller \
         holding candidates say which walk they came from"
    );
    assert!(
        relative_paths(&after).contains(&"src/added.rs".to_owned()),
        "the rescan did not see a file created after the first walk, which is \
         the whole of what it is for"
    );
}

#[test]
fn no_candidates_is_the_only_abstention_that_invalidates_the_list() {
    let root = fixture("no_candidates");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);
    drop(cache.list().expect("the first walk"));

    // Every other shape a dispatch can end in. `Deadline` is the one that
    // matters: it means the search was cut off, which is evidence about
    // nothing, and rescanning on it would spend I/O in the window that just
    // proved to be short of it.
    for quiet in [
        committed(),
        abstention(AbstainReason::NotAnIdentifier),
        abstention(AbstainReason::UnsupportedRole),
        abstention(AbstainReason::Deadline),
        abstention(AbstainReason::External {
            name: "std::vec::Vec".into(),
        }),
        Dispatched::DeadlineExpired(LateStrata::Unclassified),
        Dispatched::Failed(Error::Project(ProjectError::NotUtf8 {
            path: root.join("notes.md"),
        })),
    ] {
        cache.observe(&quiet);
    }

    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    assert!(
        cache.rescans().recv_timeout(QUIET).is_err(),
        "an abstention that is not NoCandidates scheduled a rescan: §4 makes \
         the trigger that reason specifically, because it is the one that \
         means an exhaustive search found nothing"
    );

    cache.observe(&abstention(AbstainReason::NoCandidates));
    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    cache
        .rescans()
        .recv_timeout(patience())
        .expect("NoCandidates schedules the rescan");
}

#[test]
fn the_two_triggers_share_one_debounce_rather_than_one_each() {
    let root = fixture("shared_debounce");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);
    drop(cache.list().expect("the first walk"));

    // A burst spanning both triggers, inside one window.
    cache.watched_files_changed();
    clock.advance(Duration::from_millis(50));
    cache.observe(&abstention(AbstainReason::NoCandidates));
    clock.advance(Duration::from_millis(50));
    cache.watched_files_changed();

    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    let rescan = cache
        .rescans()
        .recv_timeout(patience())
        .expect("one rescan");
    cache.install(rescan);

    // A second window's worth of time with nothing new to invalidate the list.
    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    assert!(
        cache.rescans().recv_timeout(QUIET).is_err(),
        "four triggers produced more than one walk, so the debounce is per \
         trigger rather than the single one §4 describes"
    );
}

fn cache(root: &Path, clock: &Arc<TestClock>) -> FileListCache {
    let clock: Arc<dyn Clock> = Arc::clone(clock) as Arc<dyn Clock>;
    FileListCache::new(vec![root.to_path_buf()], clock, DEBOUNCE).expect("the scanner thread")
}

fn relative_paths(list: &FileList) -> Vec<String> {
    let mut paths: Vec<String> = list
        .paths()
        .map(|path| path.rel().as_path().to_string_lossy().replace('\\', "/"))
        .collect();
    paths.sort();
    paths
}

/// How long a *positive* assertion waits for a thread that is doing real I/O.
/// Generous on purpose: a slow machine must fail this suite by timing out
/// rather than by flaking, and nothing here is measuring latency.
fn patience() -> Duration {
    Duration::from_secs(10)
}

fn committed() -> Dispatched {
    decided(Outcome::Committed {
        locations: Vec::new(),
        confidence: Confidence::ONE,
        strata: Strata::from_reference(Stratum::LocalBinding),
        trace: Trace::new(),
    })
}

fn abstention(reason: AbstainReason) -> Dispatched {
    decided(Outcome::Abstain {
        reason,
        strata: Strata::from_reference(Stratum::AmbiguousName),
        trace: Trace::new(),
    })
}

fn decided(outcome: Outcome) -> Dispatched {
    Dispatched::Decided(
        Answer::without_locations(outcome).expect("an outcome with nothing to encode"),
    )
}

/// The same shape `shared/tests/project.rs` uses: a real directory with an
/// empty `.git/`, without which `ignore` skips `.gitignore` entirely.
const FILES: &[(&str, &str)] = &[
    ("src/lib.rs", "fn alpha() {}\n"),
    ("src/util.rs", "fn beta() {}\n"),
    ("notes.md", "alpha\n"),
    ("vendored/copy.rs", "fn alpha() {}\n"),
];

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
