//! `design/core.md` §4's file list and `resolution.md` §3's `ProjectView`: the
//! handler's entire view of the world outside its own document.
//!
//! A concrete struct in `shared` rather than a trait in `driver`, for a reason
//! that is about measurement rather than about layering: scope rules decide
//! which candidates a search can find at all, so a second implementation on
//! the measurement path would mean the corpus scores a tool that is not the
//! one that ships. `measure_core` also exists a whole phase before `driver`
//! does.
//!
//! There is also no per-query read cache, which `resolution.md` §3 asks for.
//! The view is reached through `&Query` from several fan-out threads at once,
//! so a cache on it is shared mutable state behind `&self` — a lock, in a
//! design that has none. `conformance-005` has the three shapes that avoid
//! one; until it is answered a repeat read is a repeat syscall, which is why
//! `bytes_scanned` is not counted here yet either.
//!
//! Neither the parse LRU nor the bounded worker pool that `resolution.md` §3
//! routes `parse` and `scan` through exists, and neither turned out to be
//! load-bearing: `conformance-005` already ruled that this type gets no cache
//! until a corpus justifies one, `CLAUDE.md`'s performance posture says the
//! same about the pool, and what is left after dropping both is a fresh parse
//! and a sequential scan. Both are the slow simple version on purpose.

use std::cmp::{Ordering, Reverse};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use rope::{ByteLen, Rope};
use rustc_hash::FxHashSet;

use crate::deadline::Deadline;
use crate::error::{Error, HandlerError, ProjectError};
use crate::vocabulary::{DocumentUri, FileExtension};

/// One workspace folder. Search scope is these and nothing else — external
/// dependency sources are out of scope per `high-level.md`, and that is also
/// what keeps the walk small enough for the no-index approach to work.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ProjectRoot(Arc<Path>);

impl ProjectRoot {
    pub fn new(path: &Path) -> Self {
        Self(Arc::from(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// A path below a `ProjectRoot`. Normal components only: an absolute path, a
/// prefix, or a `..` is rejected at construction, so the escape check happens
/// once here rather than at every join.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RelPath(Box<Path>);

impl RelPath {
    pub fn new(path: &Path) -> Option<Self> {
        let normal = path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
        (normal && path.components().next().is_some()).then(|| Self(Box::from(path)))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// A file known to be inside a workspace root and not gitignored. Private
/// field, private constructor: only `FileList` mints one, and it only mints
/// one for a path the `ignore` walker returned.
///
/// A Rust handler resolving `serde::Deserialize` knows perfectly well where
/// `~/.cargo/registry` is, and the one-line change to peek at it would work
/// and pass review. Not being able to name the file is what makes
/// `ExternalDependency` a measured abstention rather than an accident.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ProjectPath(Arc<ProjectPathInner>);

#[derive(PartialEq, Eq, Hash, Debug)]
struct ProjectPathInner {
    root: ProjectRoot,
    rel: RelPath,
}

impl ProjectPath {
    fn new(root: ProjectRoot, rel: RelPath) -> Self {
        Self(Arc::new(ProjectPathInner { root, rel }))
    }

    pub fn root(&self) -> &ProjectRoot {
        &self.0.root
    }

    pub fn rel(&self) -> &RelPath {
        &self.0.rel
    }

    pub fn to_absolute(&self) -> PathBuf {
        self.0.root.path().join(self.0.rel.as_path())
    }
}

/// Which enumeration of the file list a [`CandidateFiles`] was drawn from, so
/// that a caller holding one can say how stale it is (`resolution.md` §3).
///
/// Nothing bumps it. A refreshing owner would mint the next one, and there is
/// no owner — that is `core.md` §4's own gap, not this one's. The field is
/// here rather than added later because `candidates` is specified to carry it,
/// and a return type that acquires a field is a change to every call site.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Generation(pub u64);

impl Generation {
    pub const FIRST: Self = Self(0);
}

/// The cached list `core.md` §4 describes. Built lazily on first need and
/// refreshed in the background by whoever owns it — a stale list costs recall
/// on files created in the last few seconds, which is a miss rather than a
/// wrong answer.
#[derive(Debug)]
pub struct FileList {
    roots: Vec<ProjectRoot>,
    files: FxHashSet<ProjectPath>,
    generation: Generation,
}

impl FileList {
    /// Walks each root with the `ignore` crate — ripgrep's walker, so
    /// `.gitignore` semantics are correct for free — in-process, because
    /// subprocess spawn plus pipe overhead is a meaningful fraction of a 50ms
    /// p50 target.
    pub fn enumerate(roots: &[PathBuf]) -> Result<Self, Error> {
        let mut files = FxHashSet::default();
        let mut enumerated = Vec::with_capacity(roots.len());

        for root_path in roots {
            let root = ProjectRoot::new(root_path);
            for entry in WalkBuilder::new(root_path).build() {
                let entry = match entry {
                    Ok(entry) => entry,
                    // One unreadable directory must not cost the whole list:
                    // a partial walk is the same failure mode as a stale one,
                    // and both cost recall rather than correctness.
                    Err(error) => {
                        tracing::warn!(root = %root_path.display(), %error, "skipping entry");
                        continue;
                    }
                };
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    continue;
                }
                let Ok(relative) = entry.path().strip_prefix(root_path) else {
                    tracing::warn!(
                        path = %entry.path().display(),
                        root = %root_path.display(),
                        "walker returned a path outside its own root"
                    );
                    continue;
                };
                if let Some(rel) = RelPath::new(relative) {
                    files.insert(ProjectPath::new(root.clone(), rel));
                }
            }
            enumerated.push(root);
        }

        Ok(Self {
            roots: enumerated,
            files,
            generation: Generation::FIRST,
        })
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    /// Every file the walk found.
    ///
    /// Not on `ProjectView`: a *handler* reaches candidate files through
    /// `candidates`, which filters and ranks them and does not exist yet
    /// (`resolution.md` §4). This is the other consumer — `measure_core`
    /// enumerating corpus positions — which wants the whole list precisely
    /// because it is not searching. Keeping it here rather than widening the
    /// seam is what stops it becoming a way for a handler to see everything.
    ///
    /// The order is the walk's, which is a hash-set order and therefore not
    /// stable; a caller that needs determinism sorts, and every caller here
    /// does.
    pub fn paths(&self) -> impl Iterator<Item = &ProjectPath> {
        self.files.iter()
    }
}

/// A number of files. Distinct from [`ByteLen`], which is the other counter a
/// scan reports and is the one that is easy to reach for by mistake.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FileCount(pub usize);

/// Where a search starts, which is the whole of what orders the candidate
/// list: `resolution.md` §4's four tiers are a function of the requesting
/// document's path and of whether an import already resolved to a file.
///
/// A handler builds one; `ProjectView` reads it. The tiers are cheapest and
/// most likely first, so a search that finds its answer early has read the
/// fewest files, and an exhaustive one gets the same set in a stable order
/// either way — which is what `resolution.md` §1.3 needs for replay to be
/// deterministic.
#[derive(Clone, Debug)]
pub struct SearchOrigin {
    document: ProjectPath,
    /// Tier 1. Not an argument to `candidates`, because "there is a resolved
    /// import" and "there is not" are two different searches and a `None` at
    /// the call site says neither.
    resolved: Option<ProjectPath>,
}

impl SearchOrigin {
    /// A search with nothing resolved yet: tiers 2 through 4 only.
    pub fn from_document(document: ProjectPath) -> Self {
        Self {
            document,
            resolved: None,
        }
    }

    /// An import already resolved to `resolved`, so that file is tier 1 and is
    /// searched before anything else.
    pub fn from_import(document: ProjectPath, resolved: ProjectPath) -> Self {
        Self {
            document,
            resolved: Some(resolved),
        }
    }

    pub fn document(&self) -> &ProjectPath {
        &self.document
    }

    /// `resolution.md` §4's tiers, low first. Tier 4 — other workspace roots —
    /// is `open-questions.md` question 8, and the ordering within it here is
    /// the root path, which is deterministic and nothing more.
    fn tier(&self, path: &ProjectPath) -> u8 {
        if self.resolved.as_ref().is_some_and(|first| first == path) {
            return 1;
        }
        if path.root() != self.document.root() {
            return 4;
        }
        if path.rel().as_path().parent() == self.document.rel().as_path().parent() {
            return 2;
        }
        3
    }

    /// Path proximity within a tier: how many leading directory components the
    /// candidate shares with the requesting document. Zero across roots, where
    /// the components are not comparable.
    fn proximity(&self, path: &ProjectPath) -> usize {
        if path.root() != self.document.root() {
            return 0;
        }
        path.rel()
            .as_path()
            .components()
            .zip(self.document.rel().as_path().components())
            .take_while(|(candidate, document)| candidate == document)
            .count()
    }

    fn order(&self, left: &ProjectPath, right: &ProjectPath) -> Ordering {
        (self.tier(left), Reverse(self.proximity(left)))
            .cmp(&(self.tier(right), Reverse(self.proximity(right))))
            // A total order, not just a tier order: two files in the same tier
            // at the same proximity must still come back in the same sequence
            // on every run, or replay stops being byte-comparable.
            .then_with(|| left.root().path().cmp(right.root().path()))
            .then_with(|| left.rel().as_path().cmp(right.rel().as_path()))
    }
}

/// The ordered candidate set a search runs over. Borrows the file list rather
/// than cloning out of it, since the whole-project stage hands every matching
/// file in the workspace to `scan`.
#[derive(Debug)]
pub struct CandidateFiles<'a> {
    generation: Generation,
    ordered: Vec<&'a ProjectPath>,
}

impl<'a> CandidateFiles<'a> {
    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn count(&self) -> FileCount {
        FileCount(self.ordered.len())
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &'a ProjectPath> {
        self.ordered.iter().copied()
    }
}

/// Instantiated per query, which is what makes the deadline check on every
/// read possible without a deadline argument on every method.
#[derive(Debug)]
pub struct ProjectView {
    files: Arc<FileList>,
    deadline: Deadline,
}

impl ProjectView {
    pub fn new(files: Arc<FileList>, deadline: Deadline) -> Self {
        Self { files, deadline }
    }

    pub fn roots(&self) -> &[ProjectRoot] {
        &self.files.roots
    }

    /// The root containing a document, for scoping searches. The longest
    /// matching prefix wins, so a root nested inside another resolves to the
    /// inner one; which root a *search* prefers when several could serve is a
    /// different question and is `open-questions.md` question 8.
    pub fn root_of(&self, uri: &DocumentUri) -> Option<&ProjectRoot> {
        let path = uri.to_file_path()?;
        self.files
            .roots
            .iter()
            .filter(|root| path.starts_with(root.path()))
            .max_by_key(|root| root.path().as_os_str().len())
    }

    /// Resolve a relative path against the file list. `None` if the path is
    /// not a tracked project file — which is how scope is enforced.
    pub fn lookup(&self, root: &ProjectRoot, rel: &RelPath) -> Option<ProjectPath> {
        let probe = ProjectPath::new(root.clone(), rel.clone());
        self.files.files.get(&probe).cloned()
    }

    /// Files with any of `extensions`, ordered by `origin`.
    ///
    /// With `lookup`, one of the two ways a handler can come to hold a
    /// [`ProjectPath`] — which is what makes "search scope is the project's
    /// own tracked source" a property of the type rather than a rule every
    /// language author remembers (`resolution.md` §3).
    ///
    /// Every matching file is returned. There is no cap and no cheaper
    /// prefilter: `resolution.md` §1.3's exhaustive search is what earns the
    /// uniqueness signal that stages 4 and 5 rank on, and a clipped list
    /// cannot tell "the only definition of this name" from "the first of
    /// eleven".
    pub fn candidates(
        &self,
        extensions: &[FileExtension],
        origin: &SearchOrigin,
    ) -> CandidateFiles<'_> {
        let mut ordered: Vec<&ProjectPath> = self
            .files
            .files
            .iter()
            .filter(|path| {
                extensions
                    .iter()
                    .any(|extension| extension.matches(path.rel().as_path()))
            })
            .collect();
        ordered.sort_unstable_by(|left, right| origin.order(left, right));

        CandidateFiles {
            generation: self.files.generation,
            ordered,
        }
    }

    /// Text of a project file.
    ///
    /// Open documents will return editor state here rather than disk state —
    /// `shim.md` §5's argument only pays off if it reaches the search path,
    /// since a definition added thirty seconds ago is in the buffer and not on
    /// disk. The open-document map that makes that possible is the driver's
    /// and does not exist yet, so today every read is a disk read.
    pub fn read(&self, path: &ProjectPath) -> Result<FileText, Error> {
        // Checked first rather than after: starting I/O whose result cannot be
        // used spends the window that has already proved to be short of it.
        if self.deadline.expired() {
            return Err(HandlerError::DeadlineExpired.into());
        }

        let absolute = path.to_absolute();
        let bytes = fs::read(&absolute).map_err(|source| ProjectError::Read {
            path: absolute.clone(),
            source,
        })?;
        let text =
            String::from_utf8(bytes).map_err(|_| ProjectError::NotUtf8 { path: absolute })?;
        Ok(FileText::Disk(Arc::from(text)))
    }
}

/// `resolution.md` §3: handlers work in chunks where they can, so a large open
/// file is not flattened to a `String` to check one line.
#[derive(Clone, Debug)]
pub enum FileText {
    Disk(Arc<str>),
    Open(Rope),
}

impl FileText {
    pub fn len(&self) -> ByteLen {
        match self {
            Self::Disk(text) => ByteLen(text.len()),
            Self::Open(text) => ByteLen(text.len()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == ByteLen::ZERO
    }

    pub fn chunks(&self) -> FileChunks<'_> {
        match self {
            Self::Disk(text) => FileChunks::Disk(Some(&**text)),
            Self::Open(text) => FileChunks::Open(text.chunks()),
        }
    }
}

/// An enum rather than `Box<dyn Iterator>`: this is per-chunk on the scan
/// path, and the two variants are the only two there will ever be.
#[expect(
    clippy::large_enum_variant,
    reason = "rope::Chunks is a 448-byte sum-tree cursor. The lint is denied for `Error`, which sits in the Err of every Result and is moved on every `?`; this is a short-lived iterator in a local, and boxing it would trade a stack write for a per-file allocation."
)]
pub enum FileChunks<'a> {
    Disk(Option<&'a str>),
    Open(rope::Chunks<'a>),
}

// By hand because `rope::Chunks` has no `Debug` and giving it one is a
// vendored-crate edit for the sake of a derive.
impl fmt::Debug for FileChunks<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disk(remaining) => formatter
                .debug_tuple("FileChunks::Disk")
                .field(&remaining.map(str::len))
                .finish(),
            Self::Open(_) => formatter.write_str("FileChunks::Open(..)"),
        }
    }
}

impl<'a> Iterator for FileChunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Disk(text) => text.take(),
            Self::Open(chunks) => chunks.next(),
        }
    }
}
