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

use shared::{
    Deadline, FileExtension, FileList, Generation, ProjectPath, ProjectRoot, ProjectView, RelPath,
    SearchOrigin,
};

const RUST: FileExtension = FileExtension::new("rs");

/// The fixture, as paths relative to the single root. `notes.md` is the
/// wrong extension and `vendored/copy.rs` is gitignored; both are files the
/// walker sees and a handler must not.
const FILES: &[&str] = &[
    "src/lib.rs",
    "src/util.rs",
    "src/deep/inner.rs",
    "other/far.rs",
    "notes.md",
    "vendored/copy.rs",
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

fn view(root: &Path) -> ProjectView {
    let files = FileList::enumerate(std::slice::from_ref(&root.to_path_buf()))
        .expect("enumerating the fixture");
    ProjectView::new(Arc::new(files), Deadline::none())
}

fn path(view: &ProjectView, root: &Path, relative: &str) -> ProjectPath {
    let rel = RelPath::new(Path::new(relative)).expect("a relative path");
    view.lookup(&ProjectRoot::new(root), &rel)
        .unwrap_or_else(|| panic!("{relative} is not in the fixture file list"))
}

fn rel_paths(view: &ProjectView, origin: &SearchOrigin) -> Vec<String> {
    view.candidates(&[RUST], origin)
        .paths()
        .map(|path| path.rel().as_path().to_string_lossy().replace('\\', "/"))
        .collect()
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
    for relative in FILES {
        let file = root.join(relative);
        fs::create_dir_all(file.parent().expect("a parent directory"))
            .expect("a fixture directory");
        fs::write(&file, "fn alpha() {}\n").expect("a fixture file");
    }

    root
}
