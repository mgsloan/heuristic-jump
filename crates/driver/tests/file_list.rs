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
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use driver::{Answer, Classified, DID_CHANGE_WATCHED_FILES, DebounceMs, Dispatched, FileListCache};
use shared::{
    AbstainReason, Clock, Confidence, Deadline, DocumentUri, Error, FileExtension, FileList,
    Generation, Language, Outcome, ProjectError, ProjectPath, ProjectRoot, ProjectView, RelPath,
    ScanRequest, SearchOrigin, Strata, Stratum, TestClock, Trace,
};

const RUST: FileExtension = FileExtension::new("rs");

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
    assert_eq!(
        relative_paths(&first),
        vec![
            "notes.md".to_owned(),
            "src/lib.rs".to_owned(),
            "src/util.rs".to_owned()
        ],
        "§4's first bullet is that the walk is the `ignore` crate's, so `.gitignore` \
         is respected for free — `vendored/copy.rs` is in the fixture and must not be \
         in the list. Asserted as the whole set rather than as a membership, because \
         every test in this file builds a fixture with that `.gitignore` and nothing \
         was reading it back: a walk that ignored the file would have passed here and \
         would have handed the scan below a candidate outside the project"
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

/// §4's proxy-mode invalidation is a routing row keyed by a method name, and
/// the name is a `const` here with no caller: `shim.md` §3's router does not
/// exist yet, and its own doc calls the string "the entire coupling between
/// that row and this module".
///
/// A typo in it costs nothing today, and on the day the row is written costs
/// the whole proxy-mode path *silently* — the tee never fires, the list never
/// goes stale, and the only symptom is recall on files created since the walk,
/// which §4 has already decided is an acceptable cost of a stale list. There is
/// no failure to notice.
///
/// Held against `reference/lsp-3.17/metaModel.json`, the protocol
/// machine-readable, rather than against a second copy of the string somewhere
/// else in the workspace — which would only assert that two things we wrote
/// agree. The direction is asserted with it because it is the half §4's
/// argument rests on: these frames flow editor → child and the shim forwards
/// them anyway, which is why teeing them costs one row and no descriptors.
#[test]
#[expect(
    clippy::disallowed_types,
    reason = "`serde_json::Value` is banned because it allocates a whole tree per frame and \
              forwarded frames must not be materialized. There is no frame here: this is the \
              vendored meta model, read once in a test, and the typed struct the lint suggests \
              would put a `serde` dev-dependency on `driver` for one deserialize. Same \
              reasoning as `seam.rs`'s `cargo metadata` reader"
)]
fn the_teed_notification_is_the_one_the_protocol_names() {
    let model: serde_json::Value = serde_json::from_str(&meta_model()).expect("the meta model");
    let notifications = model["notifications"]
        .as_array()
        .expect("the meta model lists notifications");
    assert!(
        notifications.len() > 10,
        "only {} notification(s) parsed out of the meta model, so this lookup is \
         not reading what it thinks it is",
        notifications.len()
    );

    let teed = notifications
        .iter()
        .find(|notification| notification["method"] == DID_CHANGE_WATCHED_FILES)
        .unwrap_or_else(|| panic!("LSP 3.17 has no notification {DID_CHANGE_WATCHED_FILES:?}"));
    assert_eq!(
        teed["messageDirection"], "clientToServer",
        "§4 tees this because it already flows editor → child through the shim. A \
         server-originated notification would have to be watched for rather than \
         forwarded, and the bullet's whole argument — no descriptors, correct \
         scoping for nothing — is about the editor having paid for it already"
    );
}

fn meta_model() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/lsp-3.17/metaModel.json");
    fs::read_to_string(&path).expect("the vendored LSP 3.17 meta model")
}

/// §4's deletion sentence, over a real removal: "a stale entry for a file that
/// was removed only ever surfaces as a failed read".
///
/// The article is what the sentence turns on. A candidate that vanished
/// between the walk and the read fails the *whole* scan — `resolution.md` §4
/// forbids reporting a partial one, and `ProjectView::scan` says so where it
/// propagates — so if the failure taught the list nothing, every later query
/// over the same candidate set would fail identically for as long as the
/// process lives. That is not a failed read, it is an outage, and standalone
/// has nothing that would end it: `deps.md` §7 defers `notify`, and §4's own
/// claim is that nothing depends on the editor's watcher.
#[test]
fn a_candidate_removed_after_the_walk_costs_one_failed_read_and_not_every_later_one() {
    let root = fixture("removed_candidate");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);

    let view = cache
        .view(Deadline::none(), grammar())
        .expect("the first walk");
    fs::remove_file(root.join("src/util.rs")).expect("removing a candidate after the walk");

    let failure = scan_for_alpha(&view, &root).expect_err("a scan over a candidate that is gone");
    assert!(
        matches!(&failure, Error::Project(ProjectError::Read { path: _, source })
            if source.kind() == io::ErrorKind::NotFound),
        "a removed candidate surfaced as {failure:?} rather than as the failed read \
         §4 describes, so the rest of this test is asserting the wrong mechanism"
    );

    cache.observe(&Dispatched::Failed(failure));
    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    let rescan = cache
        .rescans()
        .recv_timeout(patience())
        .expect("a read that failed because the file is gone schedules the rescan");
    cache.install(rescan);

    let after = cache.list().expect("the refreshed list");
    assert!(
        !relative_paths(&after).contains(&"src/util.rs".to_owned()),
        "the rescan kept an entry for a file that is not there, so the next query \
         fails on the same candidate"
    );

    // The claim, rather than its precondition: the query after the failed one
    // gets an answer.
    let next = cache
        .view(Deadline::none(), grammar())
        .expect("the view a later query is dispatched against");
    let outcome = scan_for_alpha(&next, &root).expect("the scan a later query runs");
    assert_eq!(
        outcome
            .hits
            .iter()
            .map(|file| file
                .path
                .rel()
                .as_path()
                .to_string_lossy()
                .replace('\\', "/"))
            .collect::<Vec<String>>(),
        vec!["src/lib.rs".to_owned()],
        "the later scan did not find the definition that is still on disk"
    );
}

/// The discriminating half. Both of these are read failures on a file the walk
/// returned, and neither is evidence the walk was wrong: the walker will hand
/// the same entry back, so marking stale on one would be a rescan per query for
/// as long as the file stays unreadable — the spin `install` refuses when a
/// walk itself fails.
///
/// The permission error is constructed rather than provoked because it has to
/// be the *same variant* as the one above with a different `ErrorKind`; a
/// fixture that made the read fail some other way would be testing a different
/// arm and would pass against a classifier that keyed on the variant alone.
#[test]
fn a_read_that_failed_for_any_reason_but_a_missing_file_leaves_the_list_alone() {
    let root = fixture("unreadable_candidate");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);

    let view = cache
        .view(Deadline::none(), grammar())
        .expect("the first walk");
    fs::write(root.join("src/util.rs"), [0xff, 0xfe]).expect("a candidate that is not text");

    let not_text = scan_for_alpha(&view, &root).expect_err("a scan over a file that is not UTF-8");
    assert!(
        matches!(&not_text, Error::Project(ProjectError::NotUtf8 { path: _ })),
        "expected the non-UTF-8 failure, got {not_text:?}"
    );

    for quiet in [
        not_text,
        Error::Project(ProjectError::Read {
            path: root.join("src/util.rs"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        }),
    ] {
        cache.observe(&Dispatched::Failed(quiet));
    }

    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    assert!(
        cache.rescans().recv_timeout(QUIET).is_err(),
        "a read failure that a rescan cannot fix scheduled one anyway: the file is \
         still there and the walker still returns it, so this repeats on every \
         query over that candidate"
    );
}

/// The other failure that is a fact about the walk, from the other direction:
/// `ProjectError::Unresolvable` is raised when §8.4's conversion is handed a
/// `Location` whose file the view cannot find, and a handler only ever holds a
/// path the list minted — so the list is what moved.
#[test]
fn a_location_the_view_cannot_resolve_is_the_other_failure_that_marks_the_list_stale() {
    let root = fixture("unresolvable_target");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);
    drop(cache.list().expect("the first walk"));

    let uri = DocumentUri::from_file_path(&root.join("src/gone.rs")).expect("a file URI");
    cache.observe(&Dispatched::Failed(Error::Project(
        ProjectError::Unresolvable { uri },
    )));

    clock.advance(DEBOUNCE.window());
    cache.refresh_if_due();
    cache
        .rescans()
        .recv_timeout(patience())
        .expect("an unresolvable target schedules the rescan");
}

/// §4's second bullet: the walk is "**built in-process rather than by shelling
/// out to ripgrep**: subprocess spawn plus pipe overhead is a meaningful
/// fraction of a 50ms p50 target, and in-process gives direct control over
/// cancellation at the deadline".
///
/// The second half is held by `shared/tests/project.rs`'s
/// `a_scan_past_its_deadline_reports_nothing_rather_than_less`. The first half
/// is held by nothing, and it is the half that fails *silently*: `rg --files`
/// piped into a candidate list returns the same paths, and `rg -w <name>`
/// returns the same hits, so every assertion in this file and in
/// `shared/tests/project.rs` would go on passing. What changes is a latency
/// nothing measures in phase 1a and a cancellation story that stops being
/// ours — which is exactly the shape of a change that works and passes review.
///
/// **Scoped to the query path rather than to `driver`.** The whole of `shared`
/// is in it, since that is where `FileList::enumerate` and `ProjectView::scan`
/// live and there is no other reason for the seam crate to start a process;
/// `driver`'s share is this file's subject, the enumeration cache and its
/// walker thread. The rest of `driver` is deliberately exempt: `shim.md` has
/// it spawning the proxied child, which is why `ServerCommand` already exists
/// in `config.rs`, and a scan that forbade that would have to be weakened by
/// the campaign that writes it — a test whose first real encounter with the
/// design is being relaxed was not holding a claim.
#[test]
fn the_walk_and_the_scan_are_in_process_and_never_shell_out() {
    let on_the_query_path: Vec<(String, String)> = crate_sources()
        .into_iter()
        .filter(|(file, _)| file.starts_with("shared/") || file == "driver/src/files.rs")
        .collect();

    assert!(
        on_the_query_path.len() > 5,
        "only {} query-path source file(s) walked, so this scan would pass \
         against almost anything",
        on_the_query_path.len()
    );
    assert!(
        on_the_query_path
            .iter()
            .any(|(file, _)| file == "driver/src/files.rs"),
        "the enumeration cache itself is not in the scanned set, which is the \
         one file §4's bullet is literally about"
    );

    let offenders: Vec<String> = on_the_query_path
        .iter()
        .flat_map(|(file, text)| {
            spawns_a_subprocess(text)
                .into_iter()
                .map(move |used| format!("{file}: {used}"))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "the file list walk or the literal scan reaches for a subprocess. §4 \
         builds both in-process because \"subprocess spawn plus pipe overhead \
         is a meaningful fraction of a 50ms p50 target\", and because \
         cancelling a child at the deadline is a different and worse problem \
         than returning early from a loop:\n{}",
        offenders.join("\n")
    );
}

/// The control. The scan above passes today and would pass just as well if it
/// were looking for the wrong string, so the marker list has to be shown
/// finding a spawn — and has to be shown *not* finding the two things that
/// look like one: the prose recording that the design refuses it, and
/// `ServerCommand`, which is `driver`'s name for the child `shim.md` requires
/// and appears in `config.rs` as an ordinary type.
#[test]
fn the_subprocess_scan_finds_what_it_is_looking_for() {
    let planted = "
        // Built in-process rather than by shelling out to std::process::Command.
        use std::process::Command;
        pub struct ServerCommand { program: OsString }
        let files = Command::new(\"rg\").arg(\"--files\").output()?;
        let walked = FileList::enumerate(roots)?;
    ";

    assert_eq!(
        spawns_a_subprocess(planted),
        vec!["std::process", "Command::new"],
        "the scan must see both the import and the call, must not read the \
         comment that records the decision, and must not mistake \
         `ServerCommand` for one"
    );
}

/// Every way a source file starts a process, in `text`, skipping comments.
///
/// Two markers and not three: `process::Command` looks like a third but is
/// subsumed, because reaching it needs an import that names `std::process` on
/// its own line. The control below is what established that — it was written
/// expecting three and got the redundant one back.
fn spawns_a_subprocess(text: &str) -> Vec<&'static str> {
    const MARKERS: [&str; 2] = ["std::process", "Command::new"];

    let mut found = Vec::new();
    for line in text.lines() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        for marker in MARKERS {
            if code.contains(marker) && !found.contains(&marker) {
                found.push(marker);
            }
        }
    }
    found
}

/// §4's last paragraph: "**Search scope is the workspace folders only.**
/// External dependency sources (`~/.cargo/registry` and equivalents) are
/// excluded per `high-level.md`; this is also what keeps the walk small enough
/// for the no-index approach to be viable at all."
///
/// `shared/src/project.rs` says what rests on it: "A Rust handler resolving
/// `serde::Deserialize` knows perfectly well where `~/.cargo/registry` is, and
/// the one-line change to peek at it would work and pass review. Not being
/// able to name the file is what makes `ExternalDependency` a measured
/// abstention rather than an accident."
///
/// `lookup_refuses_a_path_the_walker_did_not_return` holds the half where the
/// path is *inside* a root and gitignored. The escape is the other half and
/// nothing held it: every `RelPath::new` call site in this workspace —
/// fourteen of them, in five test files — `expect`s success, so the rejection
/// that makes the scope rule true had no test at all. It is asserted here
/// rather than in `shared` because `FileListCache::view` is `driver`'s only
/// route to a `ProjectView`, which is what a query is actually dispatched
/// against.
#[test]
fn no_project_path_names_a_file_outside_the_workspace_roots() {
    let root = fixture("outside_the_roots");
    let clock = Arc::new(TestClock::new());
    let mut cache = cache(&root, &clock);
    let view = cache
        .view(Deadline::none(), grammar())
        .expect("the first walk");

    // A real file, really outside, really readable, and really the sort of
    // thing a Rust handler would want: a sibling of the workspace root.
    let outside = root
        .parent()
        .expect("the fixture root has a parent")
        .join("registry_stand_in.rs");
    fs::write(&outside, "fn alpha() {}\n").expect("a readable file outside the roots");

    for escape in ["../registry_stand_in.rs", "src/../../registry_stand_in.rs"] {
        assert!(
            RelPath::new(Path::new(escape)).is_none(),
            "`RelPath::new` accepted {escape:?}, so a handler can name a file \
             outside the workspace by spelling the way out. §4 excludes \
             external dependency sources, and `ProjectPath` being unforgeable \
             is the whole mechanism — a `..` that survives makes it a \
             convention again"
        );
    }

    // The absolute path is not a second route: `lookup` takes a `RelPath`, so
    // the escape above is the only spelling there is, and the walker never
    // returned this file for the relative one to be found under.
    let relative = RelPath::new(Path::new("registry_stand_in.rs")).expect("a relative path");
    assert!(
        view.lookup(&ProjectRoot::new(&root), &relative).is_none(),
        "lookup minted a ProjectPath for a file that is not under the root it \
         was asked about, which would make the scope rule depend on the caller \
         passing the right root"
    );

    assert!(
        !relative_paths(&cache.list().expect("the list in hand"))
            .iter()
            .any(|path| path.contains("registry_stand_in")),
        "the walk reached outside its root, which is what §4 says keeps the \
         no-index approach viable at all"
    );
}

/// Every `.rs` candidate, scanned for the one identifier the fixture defines.
fn scan_for_alpha(view: &ProjectView, root: &Path) -> Result<shared::ScanOutcome, Error> {
    let origin = SearchOrigin::from_document(project_path(view, root, "src/lib.rs"));
    let candidates = view.candidates(&[RUST], &origin);
    let request = ScanRequest::new("alpha", &candidates).expect("an identifier literal");
    view.scan(&request)
}

fn project_path(view: &ProjectView, root: &Path, relative: &str) -> ProjectPath {
    let rel = RelPath::new(Path::new(relative)).expect("a relative path");
    view.lookup(&ProjectRoot::new(root), &rel)
        .expect("a fixture file the walk found")
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
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
