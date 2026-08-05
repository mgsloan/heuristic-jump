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
//! **Nothing here caches.** `resolution.md` §3 asks for a per-query read cache
//! and routes `parse` through a parse LRU, and both are the same mistake: the
//! view is reached through `&Query` from several fan-out threads at once, so a
//! cache on it is shared mutable state behind `&self` — a lock, in a design
//! that has none. `conformance-005` ruled it, and ruled it for the reason that
//! also covers the LRU and the bounded pool: `CLAUDE.md` withholds caching,
//! indexing and optimisation until the corpus harness shows the change is
//! worth it and there is a benchmark, and there is no corpus. So a repeat read
//! is a repeat syscall, a parse is a fresh parse, and `scan` is sequential.
//!
//! `bytes_scanned` counts bytes *actually read*, which is that ruling's other
//! half: the counter's job is to be a deterministic machine-independent proxy
//! for latency between gates, a re-read costs latency, and a deduplicated
//! count would systematically under-predict.

use std::cmp::{Ordering, Reverse};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use rope::{ByteLen, ByteRange, LineIndex, Offset, Rope};
use tree_sitter::{Language, Parser, Point, Tree};

use crate::Set;
use crate::deadline::Deadline;
use crate::error::{Error, HandlerError, ProjectError};
use crate::identifier::{identifier_continue, is_identifier_text};
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
/// Bumped by [`FileList::superseding`], which is the refreshing owner's one
/// move — `driver::FileListCache` in this workspace.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Generation(pub u64);

impl Generation {
    pub const FIRST: Self = Self(0);

    /// Saturating, because the alternative to a stuck counter after 2^64
    /// refreshes is a wrapped one that makes an older list compare as newer.
    /// Neither will happen; only one of them is wrong if it does.
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// The cached list `core.md` §4 describes. A stale list costs recall on files
/// created in the last few seconds, which is a miss rather than a wrong
/// answer, so it is refreshed rather than kept correct.
///
/// The owner that builds it lazily and refreshes it in the background is
/// `driver::FileListCache`, and it is there rather than here for the reason
/// §4 gives: the two things that invalidate the list are a query's abstention
/// and the editor's watcher, neither of which `shared` can see. What is here
/// is the walk and the [`Generation`] a refresh advances.
#[derive(Debug)]
pub struct FileList {
    roots: Vec<ProjectRoot>,
    files: Set<ProjectPath>,
    generation: Generation,
}

impl FileList {
    /// Walks each root with the `ignore` crate — ripgrep's walker, so
    /// `.gitignore` semantics are correct for free — in-process, because
    /// subprocess spawn plus pipe overhead is a meaningful fraction of a 50ms
    /// p50 target.
    pub fn enumerate(roots: &[PathBuf]) -> Result<Self, Error> {
        let mut files = Set::default();
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

    /// This walk replaces `previous`, so it carries the generation after it.
    ///
    /// Takes the list it supersedes rather than a [`Generation`], so a refresh
    /// cannot mint a number that is not one more than the list it is about to
    /// evict — which is the whole of what makes the counter comparable. A
    /// background walk does not know its own generation, because it started
    /// before the owner decided which list it would replace.
    pub fn superseding(mut self, previous: &FileList) -> Self {
        self.generation = previous.generation.next();
        self
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

/// What a handler asks `ProjectView` to search for.
///
/// `resolution.md` §4 splits the search in two: the handler builds the pattern
/// and interprets the matches, the view executes it. This is the pattern half,
/// and it is deliberately not expressive — every search starts from an exact
/// identifier, which is what makes the cheapest possible prefilter available.
#[derive(Debug)]
pub struct ScanRequest<'a> {
    literal: &'a str,
    candidates: &'a CandidateFiles<'a>,
}

impl<'a> ScanRequest<'a> {
    /// `None` when `literal` is not identifier-shaped.
    ///
    /// A checked constructor rather than a plain struct literal because
    /// "every search starts from an exact identifier" is otherwise a sentence
    /// in a document: a request for `foo(` or for the empty string would scan
    /// every candidate file and match nothing, at full cost, and the abstention
    /// would be recorded as `NoCandidates` — a claim about the project rather
    /// than about the query.
    pub fn new(literal: &'a str, candidates: &'a CandidateFiles<'a>) -> Option<Self> {
        is_identifier_text(literal).then_some(Self {
            literal,
            candidates,
        })
    }

    pub fn literal(&self) -> &'a str {
        self.literal
    }

    pub fn candidates(&self) -> &'a CandidateFiles<'a> {
        self.candidates
    }
}

/// One whole-token match of the scanned literal.
///
/// `line` is redundant with `range` and is carried for the same reason
/// [`crate::Location`] carries one (`core.md` §1): the scan has counted the
/// newlines already, and a handler that wants to apply a lexical rule to the
/// matched line would otherwise index the whole file again.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Hit {
    pub range: ByteRange,
    pub line: LineIndex,
}

#[derive(Clone, Debug)]
pub struct FileHits {
    pub path: ProjectPath,
    pub hits: Vec<Hit>,
}

/// `resolution.md` §4. There is no partial variant and no truncation flag:
/// the scan reads every candidate or the query abstains, because a partial
/// scan cannot tell "the only definition of this name in the project" from
/// "the first of eleven", and global uniqueness is the main confidence signal
/// the later stages rank on.
#[derive(Debug)]
pub struct ScanOutcome {
    pub hits: Vec<FileHits>,
    /// For the trace record, not for control flow. Both counters exist so
    /// that a latency regression can be attributed to a diff.
    pub files_scanned: FileCount,
    pub bytes_scanned: ByteLen,
}

/// Instantiated per query, which is what makes the deadline check on every
/// read possible without a deadline argument on every method.
#[derive(Debug)]
pub struct ProjectView {
    files: Arc<FileList>,
    deadline: Deadline,
    /// `conformance-012` (answered). `resolution.md` §3's `parse`
    /// takes a path and text and no grammar, so there is no route to one
    /// except this. Handed over at construction the same way §3 hands over
    /// the worker pool; the view is per query and a query is dispatched to
    /// one handler, so there is exactly one language it could be.
    grammar: Language,
}

impl ProjectView {
    // `conformance-012` (answered). The third parameter.
    pub fn new(files: Arc<FileList>, deadline: Deadline, grammar: Language) -> Self {
        Self {
            files,
            deadline,
            grammar,
        }
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

    /// A parsed tree for a candidate file, which is what decides whether a
    /// literal hit is a definition (`resolution.md` §4: the scan finds and
    /// tree-sitter decides).
    ///
    /// No LRU. `resolution.md` §3 says "from the parse LRU when possible" and
    /// there is no possible: the cache would be shared mutable state behind
    /// the `Sync` `&Query` several fan-out threads hold, which is a lock, and
    /// `conformance-005` already ruled that question the same way for reads —
    /// no corpus, no benchmark, no cache. `path` is taken anyway because it is
    /// the key such a cache would use and because a parse failure has to be
    /// nameable.
    ///
    /// No deadline check, deliberately. The return type is `Option`, so an
    /// expiry here would be indistinguishable from an unparseable file, and
    /// the two must not be merged — a handler's next `read` fails with the
    /// deadline variant, which is where the abstention comes from.
    pub fn parse(&self, path: &ProjectPath, text: &FileText) -> Option<Tree> {
        let mut parser = Parser::new();
        if let Err(error) = parser.set_language(&self.grammar) {
            // Not silently discarded and not propagated either: the signature
            // is §3's and has no error channel, and a grammar the parser
            // rejects is a build-time mistake that would fail identically on
            // every file, so it is worth one loud line rather than a hundred.
            tracing::error!(
                path = %path.to_absolute().display(),
                %error,
                "the view's grammar was rejected by the parser"
            );
            return None;
        }

        match text {
            FileText::Disk(text) => parser.parse(text.as_bytes(), None),
            FileText::Open(text) => {
                let mut read = |offset: usize, _position: Point| {
                    text.chunks_in_range(ByteRange::new(Offset(offset), Offset::ZERO + text.len()))
                        .next()
                        .unwrap_or("")
                };
                parser.parse_with_options(&mut read, None, None)
            }
        }
    }

    /// The literal, word-boundary scan every search starts from
    /// (`resolution.md` §4). Exhaustive: every candidate is read.
    ///
    /// `Result` rather than §3's printed `-> ScanOutcome`, which cannot be
    /// written: `read` fails when the deadline has expired, and §4 forbids
    /// reporting a partial scan, so there is nowhere for the expiry to go
    /// except the `Err` (`state/spec-changelog.md`, CHANGE-conformance-011).
    /// An unreadable or non-UTF-8 candidate fails the scan the same way,
    /// deliberately: skipping it would make coverage depend on a race between
    /// the walk and the read, and there would be nothing in the record saying
    /// the answer was computed from less than the project.
    ///
    /// What keeps that from being permanent for the commonest case — a
    /// candidate deleted since the walk — is `core.md` §4's second on-demand
    /// signal rather than anything here: the failure is classified by
    /// [`Error::file_list_evidence`] as evidence about the list, so the query
    /// after it searches a list without the removed file.
    ///
    /// Sequential, where `resolution.md` §3 has the fan-out run on a bounded
    /// pool. `shim.md` §10's pool now exists — `driver::workers` — but it is
    /// the pool a whole query runs *on*, and nothing hands one to a
    /// `ProjectView`: `new` takes no pool, so there is nothing here to fan out
    /// onto. That is why `rayon` is still undeclared, and `core.md` §9 names it
    /// in the list of dependencies chosen and not yet declared rather than
    /// claiming this method already uses one. `CLAUDE.md` withholds the
    /// optimisation until the corpus harness shows it is worth it and there is
    /// a benchmark. Order is not an optimisation question: `hits` comes back in
    /// candidate order either way.
    pub fn scan(&self, request: &ScanRequest<'_>) -> Result<ScanOutcome, Error> {
        let mut outcome = ScanOutcome {
            hits: Vec::new(),
            files_scanned: FileCount(0),
            bytes_scanned: ByteLen::ZERO,
        };

        for path in request.candidates.paths() {
            let text = self.read(path)?;
            outcome.files_scanned.0 += 1;
            outcome.bytes_scanned.0 += text.len().0;

            let hits = whole_token_matches(&text, request.literal);
            if !hits.is_empty() {
                outcome.hits.push(FileHits {
                    path: path.clone(),
                    hits,
                });
            }
        }

        Ok(outcome)
    }
}

/// Every occurrence of `literal` in `text` that is a whole token, with the
/// line each starts on.
///
/// A single `&str` rather than a chunk-wise match with a carry buffer: a match
/// spanning two chunks is the whole difficulty, and getting it wrong drops
/// definitions silently on exactly the files large enough to have several
/// chunks. Disk reads are one chunk and cost nothing here; the join is paid
/// only by open documents, of which there are none until the driver has a
/// document map.
fn whole_token_matches(text: &FileText, literal: &str) -> Vec<Hit> {
    let mut chunks = text.chunks();
    let first = chunks.next().unwrap_or("");
    match chunks.next() {
        None => matches_in(first, literal),
        Some(second) => {
            let mut joined = String::with_capacity(text.len().0);
            joined.push_str(first);
            joined.push_str(second);
            joined.extend(chunks);
            matches_in(&joined, literal)
        }
    }
}

fn matches_in(text: &str, literal: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut line = LineIndex(0);
    let mut counted = 0;

    for (start, _) in text.match_indices(literal) {
        let end = start + literal.len();
        let before = text
            .get(..start)
            .and_then(|prefix| prefix.chars().next_back());
        let after = text.get(end..).and_then(|suffix| suffix.chars().next());
        if before.is_some_and(identifier_continue) || after.is_some_and(identifier_continue) {
            continue;
        }

        line = advance_lines(line, text.get(counted..start).unwrap_or(""));
        counted = start;
        hits.push(Hit {
            range: ByteRange {
                start: Offset(start),
                end: Offset(end),
            },
            line,
        });
    }

    hits
}

/// Saturating, because `LineIndex` is a `u32` and the alternative at the
/// boundary is a wrapping cast — which reports a plausible wrong line rather
/// than an obviously wrong one, on a file no query was going to resolve in.
fn advance_lines(from: LineIndex, text: &str) -> LineIndex {
    let counted = u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX);
    LineIndex(from.0.saturating_add(counted))
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
            Self::Open(text) => text.len(),
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
