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
//! `candidates`, `parse` and `scan` are not here yet. Their parameter types
//! (`SearchOrigin`, `ScanRequest`, `ScanOutcome`) are `resolution.md` §4's and
//! their implementations need the parse LRU and the bounded worker pool that
//! `shim.md` owns, so they arrive with the first handler that searches. What
//! is here is what makes `ProjectPath` unforgeable, which is the property the
//! seam depends on.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use rope::{ByteLen, Rope};
use rustc_hash::FxHashSet;

use crate::deadline::Deadline;
use crate::error::{Error, HandlerError, ProjectError};
use crate::vocabulary::DocumentUri;

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

/// The cached list `core.md` §4 describes. Built lazily on first need and
/// refreshed in the background by whoever owns it — a stale list costs recall
/// on files created in the last few seconds, which is a miss rather than a
/// wrong answer.
#[derive(Debug)]
pub struct FileList {
    roots: Vec<ProjectRoot>,
    files: FxHashSet<ProjectPath>,
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
        })
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
