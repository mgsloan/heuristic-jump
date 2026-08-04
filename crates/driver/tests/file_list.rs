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

use driver::{Answer, Classified, DebounceMs, Dispatched, FileListCache};
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

/// `deps.md` §2: "`crossbeam-channel`, `unbounded()` everywhere" — with this
/// file's subject as the one stated exception, "`driver/src/files.rs` uses
/// `bounded(1)` and states why, and that remains correct".
///
/// It is asserted here, in the exception's own suite, because the exception is
/// what makes the rule unenforceable by a lint: §2 withdrew
/// `clippy.toml`'s ban on `unbounded` precisely because "the right answer
/// genuinely differs per channel", which leaves the bound "a per-channel
/// judgement" that nothing records. This is the record.
///
/// The trap it guards is not tidiness. In the transport a full channel does not
/// apply backpressure, it **deadlocks**: the sender is a pipe-reader thread, so
/// blocking it stops the fd being drained, which blocks the child's write, and
/// `shim.md` §1 forbids a stalled reader outright. A `bounded` that appears
/// there one day will look like ordinary care.
#[test]
fn the_only_bounded_channel_is_the_one_deps_md_names() {
    let sources = crate_sources();
    assert!(
        sources.len() > 10,
        "only {} source file(s) walked, so this scan would pass against almost anything",
        sources.len()
    );

    let mut bounded_in = Vec::new();
    let mut unbounded = 0_usize;
    for (file, text) in &sources {
        for line in text.lines() {
            let code = line.trim_start();
            // Doc comments quote both spellings — this file's subject explains
            // its own `bounded(1)` twice — and a scan that read those would be
            // measuring the prose beside the code rather than the code.
            if code.starts_with("//") {
                continue;
            }
            if code.contains("unbounded(") {
                unbounded += 1;
            } else if code.contains("bounded(") {
                bounded_in.push(file.clone());
            }
        }
    }

    assert!(
        unbounded > 0,
        "no `unbounded()` anywhere in crates/*/src, so the scan is not reading what it \
         thinks it is"
    );
    bounded_in.sort();
    bounded_in.dedup();
    assert_eq!(
        bounded_in,
        vec!["driver/src/files.rs".to_owned()],
        "deps.md §2 names one bounded channel and gives one reason for it — at most one \
         walk is ever outstanding. A second is either a channel whose bound nobody argued, \
         or the transport's, where a full channel deadlocks the pipe reader instead of \
         applying backpressure"
    );
}

/// Every source file of every `crates/*` member, as `(crate/src/name.rs, text)`.
///
/// Reached by following each crate root's `mod` declarations rather than by
/// walking the directory, which is `clippy.toml`'s rule — `std::fs::read_dir`
/// bypasses gitignore semantics — and which is also what `seam.rs` does for its
/// own scans. It reads what the crate actually compiles rather than what
/// happens to be on disk, so a file left behind by a rename cannot make this
/// pass or fail.
///
/// `vendor/` is out of scope: its channels are upstream's, and `deps.md` §2 is a
/// rule about ours.
fn crate_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/driver is two levels below the workspace root")
        .to_owned();

    let mut sources = Vec::new();
    for member in workspace_members(&root) {
        // `core.md` §9's convention, which `seam.rs` asserts: the library root
        // is named for the crate rather than left as `lib.rs`.
        let entry = format!("crates/{member}/src/{member}.rs");
        let Ok(text) = fs::read_to_string(root.join(&entry)) else {
            continue;
        };
        for line in text.lines() {
            let declared = line
                .trim()
                .strip_prefix("mod ")
                .or_else(|| line.trim().strip_prefix("pub mod "))
                .and_then(|rest| rest.strip_suffix(';'));
            if let Some(module) = declared {
                let path = format!("crates/{member}/src/{module}.rs");
                let source = fs::read_to_string(root.join(&path)).expect("a declared module");
                sources.push((format!("{member}/src/{module}.rs"), source));
            }
        }
        sources.push((format!("{member}/src/{member}.rs"), text));
    }
    sources
}

/// The `crates/*` entries of `[workspace] members`, by name.
fn workspace_members(root: &Path) -> Vec<String> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest");
    manifest
        .lines()
        .filter_map(|line| {
            let quoted = line.trim().trim_end_matches(',');
            let member = quoted.strip_prefix('"')?.strip_suffix('"')?;
            member.strip_prefix("crates/").map(str::to_owned)
        })
        .collect()
}

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
        Dispatched::DeadlineExpired(Classified::Nothing),
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
