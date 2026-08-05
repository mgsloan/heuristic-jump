//! The one system-wide error enum (`deps.md` §10). Not `anyhow`: `shim.md`
//! §11's failure handling is a table keyed by class of failure, and that table
//! is only enforceable if the classes are a closed set the compiler knows
//! about.
//!
//! `Error` itself is deliberately **not** `#[non_exhaustive]` — within one
//! workspace an exhaustive match on the top level is a feature — while every
//! sub-enum is, so adding a leaf is not a breaking change to the table.
//!
//! All nine of `deps.md` §10's arms are present. Each arrived with its
//! producer, which is the same rule the dependency set follows — a variant
//! nothing can return is a row in `shim.md` §11's table that nothing can
//! exercise. `Encoding` arrived with §8.3's position resolution, `Config`,
//! `Codec` and `Child` with `measure_core` — the corpus scan is the first
//! thing in the workspace that parses arguments, frames JSON-RPC and spawns a
//! child, and it reaches them a whole phase before the shim does — and
//! `Document` with `driver::Documents`, the open-document map §8.6's
//! fail-closed rule is written against.

use std::fmt;
use std::io;
use std::path::PathBuf;

use rope::{ByteLen, LineIndex, Offset};
use thiserror::Error;

use crate::handler::{FileListEvidence, Stratum};
use crate::proto::PositionEncoding;
use crate::vocabulary::{DocumentUri, DocumentVersion, LanguageId};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Child(#[from] ChildError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error(transparent)]
    Encoding(#[from] EncodingError),
}

impl Error {
    /// The other half of `core.md` §4's on-demand trigger, beside
    /// [`crate::AbstainReason::file_list_evidence`]: a *failure* that is
    /// itself evidence the list names a file that is no longer there.
    ///
    /// Without it a deletion has no backstop at all. §4 says the editor's
    /// watcher is opportunistic and nothing depends on it, and a removed
    /// candidate fails every later query over the same candidate set — so in
    /// standalone, where `deps.md` §7 defers `notify` and no watcher exists,
    /// that failure is permanent rather than the one failed read §4 describes.
    ///
    /// Here rather than in `driver` for the reasons that put the abstention's
    /// twin here: what an error means is this enum's business, and every
    /// sub-enum is `#[non_exhaustive]`, so the same match written in `driver`
    /// would need the wildcard arm `CLAUDE.md` bans.
    pub fn file_list_evidence(&self) -> FileListEvidence {
        match self {
            Self::Project(project) => project.file_list_evidence(),
            // None of these is reached with a `ProjectPath` in hand, so none
            // of them can be a fact about the walk. `Handler` is the one worth
            // naming: `DeadlineExpired` never arrives here at all — `driver`
            // converts it to an abstention before anything observes it — and
            // rescanning on an expiry is what §4 rules out by name.
            Self::Child(_)
            | Self::Codec(_)
            | Self::Config(_)
            | Self::Document(_)
            | Self::Encoding(_)
            | Self::Handler(_)
            | Self::Parse(_)
            | Self::Protocol(_) => FileListEvidence::Inconclusive,
        }
    }
}

/// The run was asked for something that does not exist or does not agree with
/// itself: a corpus root, a server name, a checkout that has moved.
///
/// `measure_core`'s corpus-integrity failures are here rather than in a tenth
/// arm because they are all the same class — *what this run was pointed at is
/// not what it says it is* — and `deps.md` §10 fixes the arm count at nine so
/// `shim.md` §11's table stays a table.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("no corpus split at {path}")]
    CorpusMissing { path: PathBuf },
    #[error("no repository at {path}")]
    RepositoryMissing { path: PathBuf },
    #[error("reading {path}")]
    ManifestUnreadable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The reason is the parser's own message, rendered rather than carried:
    /// `shared` is the seam crate and `core.md` §9's dependency list for it is
    /// authoritative, so a `toml::de::Error` here would put a parser in the
    /// graph of every crate that names an error.
    #[error("{path} is not a server manifest: {reason}")]
    ManifestMalformed { path: PathBuf, reason: Box<str> },
    /// `shim.md` §10's dispatch pool could not be spawned.
    ///
    /// Fatal at startup and never lazy, for the reason `core.md` §8.4 gives:
    /// answering a query on `core`'s thread reads the filesystem there, which
    /// §2 forbids outright — so a shim with no pool has no path it is allowed
    /// to run a handler on, and falling back to one would be the failure the
    /// pool exists to prevent, taken deliberately.
    ///
    /// `Config` and not `Handler` for the same reason as the variant below:
    /// nothing about a query has happened yet.
    /// `threads` is §10's sizing rather than the index that failed, because
    /// what a reader needs to know is what was asked for: a machine that
    /// refuses the twenty-eighth thread of thirty is a different report from
    /// one that refuses the first of one.
    #[error("spawning a dispatch pool of {threads}")]
    PoolUnavailable {
        threads: usize,
        #[source]
        source: io::Error,
    },
    /// A `measure_<lang>` binary whose handler declares no `languageId`.
    ///
    /// `core.md` §7 makes the binary per-language and the language the
    /// handler's — "there is no flag that could disagree with it" — so a
    /// handler that names none leaves the run with no corpus directory to
    /// read and nothing to write into a provenance header. It is a
    /// build-time mistake in four lines of `main`, which is why it is
    /// `Config` and not `Handler`: nothing about a query has happened yet.
    #[error("the handler this binary was built with declares no language")]
    HandlerDeclaresNoLanguage,
    #[error("{manifest} names no server {name:?} for {language_id}")]
    UnknownServer {
        manifest: PathBuf,
        name: Box<str>,
        language_id: LanguageId,
    },
    /// `data-collection.md` §1: a modified or extra file changes byte offsets
    /// and does not change `HEAD`, so this is the check that actually matters.
    #[error("{repository} has uncommitted changes, so its byte offsets are not the ones collected")]
    DirtyCheckout { repository: PathBuf },
    #[error("{repository} is at {found}, and the truth file was collected at {expected}")]
    CommitMismatch {
        repository: PathBuf,
        expected: Box<str>,
        found: Box<str>,
    },
    #[error("running git in {repository}")]
    GitUnavailable {
        repository: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no {path}: run collect first")]
    ArtifactMissing { path: PathBuf },
    /// A truth file whose collection was interrupted. `core.md` §7 refuses to
    /// consume one rather than reporting metrics over a prefix of the corpus.
    #[error("{path} is marked incomplete: re-run collect")]
    ArtifactIncomplete { path: PathBuf },
    #[error("{path}:{line} is not a record this file may hold")]
    ArtifactMalformed {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    /// A truth row whose stored answer is not one `DefinitionResult` reads.
    /// `core.md` §7 has a replay hand the oracle's frozen bytes to the same
    /// deserializer the shim reads a live answer with, so a row that will not
    /// deserialize is a corrupt artifact rather than an oracle that answered
    /// nothing — and reading it as `null` scores the corruption as the mutual
    /// "no definition here" §6 calls a match. Named by `(file, offset)`, which
    /// is the corpus's own identity for a position and the key §7's join uses,
    /// rather than by a line number: a truth file is regenerated and never
    /// edited, so what an operator needs is the position to re-collect.
    #[error("{path} holds an answer for {file} at {offset} that is not a definition result")]
    AnswerMalformed {
        path: PathBuf,
        file: Box<str>,
        offset: usize,
        #[source]
        source: serde_json::Error,
    },
    /// `data-collection.md` §4: resuming a truth file whose provenance has
    /// moved underneath it is refused, because half a file from one commit —
    /// or one server version, or one grammar — and half from another is the
    /// one outcome with no honest provenance header. Also what `core.md` §7's
    /// "never silently merged with another's" is on the replay side.
    #[error("{path} was collected with {field} {recorded}, and this run has {found}")]
    ProvenanceDrift {
        path: PathBuf,
        field: &'static str,
        recorded: Box<str>,
        found: Box<str>,
    },
    /// `core.md` §7 puts the grammar revision in that header, and the revision
    /// is the lockfile's: `tree_sitter::Language` reports an ABI version, which
    /// every grammar built against the same runtime shares. A language whose
    /// grammar crate is not locked has no revision to record, and inventing
    /// one is what this refuses.
    #[error("the workspace lockfile locks no {package}, so no grammar revision could be recorded")]
    GrammarNotLocked { package: Box<str> },
    #[error("the workspace lockfile locks {package} with neither a checksum nor a source revision")]
    GrammarUnidentified { package: Box<str> },
    /// `--trace=<path>` (`deps.md` §11). Refused at startup rather than
    /// reported per query: a run whose observability was asked for and is
    /// silently absent is the one failure mode §7's records exist to prevent,
    /// and the moment the flag is resolved is the only cheap place to say so.
    #[error("opening the trace at {path}")]
    TraceUnwritable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Framing, in both directions. The shim's codec and `measure_core`'s client
/// speak the same one.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CodecError {
    #[error("frame header {text:?} is not `Name: value`")]
    MalformedHeader { text: Box<str> },
    #[error("frame header block ended with no Content-Length")]
    MissingContentLength,
    #[error("Content-Length {text:?} is not a length")]
    BadContentLength { text: Box<str> },
    /// `core.md` §10's "oversized headers", which is a memory bound rather
    /// than a shape check: reading a header line buffers until a line ending
    /// arrives, so a peer that sends megabytes without one is an
    /// out-of-memory rather than an error. The bound belongs to the codec,
    /// because a caller only ever sees the line that was completed.
    #[error("a frame header ran past {limit} bytes with no line ending")]
    HeaderTooLong { limit: usize },
    /// `core.md` §10's "bogus `Content-Length`", in the half that parses. A
    /// length is a claim about bytes that have not arrived, and allocating
    /// for a claimed four gigabytes aborts the process before anything can
    /// decide the frame was nonsense.
    #[error("Content-Length {length} is past the {limit}-byte frame limit")]
    FrameTooLarge { length: usize, limit: usize },
    #[error("frame body ended after {read} of {expected} bytes")]
    Truncated { expected: usize, read: usize },
    #[error("frame body is not valid UTF-8")]
    BodyNotUtf8,
    /// The length is `deps.md` §10's "which frame": a `serde_json::Error`
    /// carries a line and column within a body nobody kept, so on its own it
    /// names an offset into text that no longer exists.
    #[error("a {length}-byte frame body is not the JSON its length claimed")]
    BodyNotJson {
        length: usize,
        #[source]
        source: serde_json::Error,
    },
    /// The outgoing direction. Unreachable for the types this workspace
    /// serializes — none holds a map with non-string keys or a non-finite
    /// float — which is exactly why it must be a variant rather than an
    /// `unwrap`: `deps.md` §10's rule is that a failure mode is a variant, and
    /// the alternative here is a panic in a hundred-hour corpus run.
    #[error("{what} could not be written as JSON")]
    NotSerializable {
        what: &'static str,
        #[source]
        source: serde_json::Error,
    },
}

/// The proper language server, as a process. `measure_core` waits for it where
/// the shim races it, but the ways it can fail to be there are the same.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ChildError {
    #[error("spawning {command}")]
    Spawn {
        command: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{command} was spawned without the pipes it was asked for")]
    StdioUnavailable { command: PathBuf },
    #[error("{command} exited before answering")]
    Exited { command: PathBuf },
    #[error("talking to {command}")]
    Io {
        command: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{method} failed: {code} {message}")]
    Failed {
        method: Box<str>,
        code: i64,
        message: Box<str>,
    },
    /// `data-collection.md` §4: querying before the index is built returns
    /// confidently wrong answers, usually empty ones, and nothing
    /// distinguishes them from a real "no definition here". A server that
    /// claims ready and answers nothing is a condition to detect at position
    /// zero, not at position 20,000.
    #[error("{command} never resolved a probe query, so its index is not usable")]
    NeverReady { command: PathBuf },
}

/// Unexpected message shape: a field that does not hold what the protocol says
/// it holds.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProtocolError {
    #[error("malformed document URI {text:?}")]
    MalformedUri {
        text: Box<str>,
        #[source]
        source: url::ParseError,
    },
}

/// Our model of an open document drifting away from the editor's
/// (`core.md` §8.6, and `deps.md` §10's ninth arm: "didChange for unopened
/// doc, bad range, ...").
///
/// **None of these is ever returned to a caller.** §8.6's whole argument is
/// that the *consequence* is what makes hand-rolled projections an acceptable
/// risk: a detected inconsistency marks the document untrusted, and queries
/// against it abstain until a `didClose`/`didOpen` resyncs it. `driver`'s
/// `Documents` performs that conversion, explicitly and with a log line, which
/// is what `deps.md` §10 asks of the one place an `Error` becomes an
/// abstention. What the variants buy is that the log — and the test — says
/// *which* self-check fired, and the three are different findings: a bad range
/// means an earlier change was applied wrongly, a stale version means we and
/// the editor disagree about what is open, and a `didSave` mismatch means the
/// whole tracking pipeline is wrong in a way neither of the others caught.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DocumentError {
    /// §8.6's second check, in the half that has no document to be about: a
    /// notification for a URI we hold no row for.
    #[error("{notification} for {uri}, which is not open")]
    NotOpen {
        notification: DocumentNotification,
        uri: DocumentUri,
    },
    /// §8.6's second check. LSP versions increase; one that does not means the
    /// editor and the shim are describing different documents.
    #[error("didChange for {uri} at version {}, which does not increase on {}", arriving.0, held.0)]
    VersionDidNotIncrease {
        uri: DocumentUri,
        held: DocumentVersion,
        arriving: DocumentVersion,
    },
    /// §8.6's first check: "an incremental range outside our rope is proof we
    /// have already diverged. It cannot happen if every prior change was
    /// applied correctly."
    #[error("a change to {uri} names a range the document does not have")]
    RangeOutsideDocument {
        uri: DocumentUri,
        #[source]
        source: EncodingError,
    },
    /// The same check, in the half no encoding conversion can catch: both ends
    /// resolve, and the range still is not one.
    #[error("a change to {uri} starts at {start} and ends at {end}")]
    RangeInverted {
        uri: DocumentUri,
        start: Offset,
        end: Offset,
    },
    /// §8.6's third check, the free end-to-end one: immediately after a save
    /// the buffer and the file are identical by definition, so a length that
    /// differs invalidates the whole document-tracking pipeline at the one
    /// point where the answer is known.
    #[error("{uri} holds {held} bytes after didSave, and the text saved is {found}")]
    SavedTextDiffers {
        uri: DocumentUri,
        held: ByteLen,
        found: ByteLen,
    },
    /// The general case §8.6 is written for, and the one that does not care
    /// which modelling mistake occurred: a forgotten `rename_all`, a missing
    /// `default`, a numeric width wrong at the edges. Any of them surfaces
    /// here as a projection that would not read the message it was given.
    #[error("a state-bearing message could not be read as {notification}")]
    Unreadable {
        notification: DocumentNotification,
        #[source]
        source: serde_json::Error,
    },
    /// [`Unreadable`](DocumentError::Unreadable) with the document unknown
    /// too: a message that did not parse and did not even name a
    /// `textDocument`. Since it cannot be attributed, §8.6's direction says it
    /// applies to everything open — "we do not know which one" is not a reason
    /// to keep trusting all of them.
    #[error("a {notification} named no document, so nothing open is still trusted")]
    Unattributable { notification: DocumentNotification },
}

/// Which state-bearing notification a [`DocumentError`] arrived on.
///
/// An enum and not the method string, because `deps.md` §10 wants typed
/// context on every variant — and because these four are exactly the messages
/// §8.6 calls state-bearing, so a fifth one being added to the set should be a
/// decision rather than a new string literal.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DocumentNotification {
    DidOpen,
    DidChange,
    DidSave,
    DidClose,
}

impl DocumentNotification {
    /// The LSP method name, so a log line reads in the vocabulary of the
    /// traffic it is about.
    pub fn method(self) -> &'static str {
        match self {
            DocumentNotification::DidOpen => "textDocument/didOpen",
            DocumentNotification::DidChange => "textDocument/didChange",
            DocumentNotification::DidSave => "textDocument/didSave",
            DocumentNotification::DidClose => "textDocument/didClose",
        }
    }
}

impl fmt::Display for DocumentNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.method())
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The grammar the handler supplied is not one this tree-sitter runtime
    /// can load — an ABI mismatch, in practice.
    #[error("grammar for {language_id} rejected by the tree-sitter runtime")]
    GrammarRejected {
        language_id: LanguageId,
        #[source]
        source: tree_sitter::LanguageError,
    },
    /// tree-sitter returning no tree at all, which it does on cancellation and
    /// on a parser with no language set.
    #[error("parse of {uri} produced no tree")]
    NoTree { uri: DocumentUri },
}

/// Failures of the handler's view of the world outside its own document
/// (`resolution.md` §3).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProjectError {
    #[error("reading {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not valid UTF-8")]
    NotUtf8 { path: PathBuf },
    #[error("enumerating {root}")]
    Enumerate {
        root: PathBuf,
        #[source]
        source: ignore::Error,
    },
    /// `core.md` §4's background rescan thread could not be spawned. Fatal at
    /// construction rather than degrading to a list that never refreshes,
    /// which would cost recall silently and forever.
    #[error("spawning the file-list scanner for {} root(s)", roots.len())]
    Scanner {
        roots: Box<[PathBuf]>,
        #[source]
        source: io::Error,
    },
    /// A URI the project view cannot turn back into one of its own files.
    ///
    /// Reached from the wire conversion (`core.md` §8.4), which is handed a
    /// `Location` and has to find the target file's text again. A handler holds
    /// a `ProjectPath` for every file it read, so a URI that fails to resolve
    /// means the file list moved under the query — not that a handler reached
    /// outside its scope, which `ProjectPath` makes unspellable.
    #[error("{uri} is not a file this project view can resolve")]
    Unresolvable { uri: DocumentUri },
}

impl ProjectError {
    /// `core.md` §4's rule that a failed read is the deletion signal, applied
    /// to the two variants that can only mean the list is wrong.
    ///
    /// The narrowness is the point. A read that failed because the file is
    /// gone is a fact about the walk; a read that failed for any other reason
    /// is a fact about the file, and the walker will hand the same entry back
    /// on the next pass — so marking stale on one would be a rescan per query
    /// forever, which is the spin `FileListCache::install` refuses elsewhere.
    pub fn file_list_evidence(&self) -> FileListEvidence {
        match self {
            // The list named a file that is not there, which is the one thing
            // a rescan fixes.
            Self::Read { path: _, source } if source.kind() == io::ErrorKind::NotFound => {
                FileListEvidence::Stale
            }
            // A `Location` whose file the view cannot resolve back. This
            // variant's own documentation above says what it means — the file
            // list moved under the query — and that is the same evidence by a
            // different route.
            Self::Unresolvable { uri: _ } => FileListEvidence::Stale,
            // Permissions, a directory where a file was, an I/O error on a
            // file that is still enumerated: the entry is not stale, the read
            // is.
            Self::Read {
                path: _,
                source: _,
            }
            // The file is there and is not text. A rescan returns it.
            | Self::NotUtf8 { path: _ }
            // Both are failures *of* a walk rather than evidence about one,
            // and rescanning on either is an immediate retry of the thing
            // that just failed.
            | Self::Enumerate { root: _, source: _ }
            | Self::Scanner { roots: _, source: _ } => FileListEvidence::Inconclusive,
        }
    }
}

/// A wire position that does not name a place in the document it arrived
/// against, read in the encoding the two ends negotiated (`core.md` §3, §8.3).
///
/// These are inconsistencies rather than user errors: the editor and the shim
/// hold the same document at the same version, so a position outside it means
/// one of them is wrong about the text. `shared::proto` reports it rather than
/// clipping to the nearest valid position, which is `conformance-006`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EncodingError {
    #[error("line {line} is past line {last_line}, the last in the document")]
    LineOutOfRange {
        line: LineIndex,
        last_line: LineIndex,
    },
    #[error("character {character} is not a {encoding} boundary within line {line}")]
    CharacterOutOfRange {
        line: LineIndex,
        character: u32,
        encoding: PositionEncoding,
    },
    #[error("byte offset {offset} is not a character boundary in {len} bytes of text")]
    OffsetOutOfRange { offset: Offset, len: ByteLen },
    /// `core.md` §8.4's stated risk — "a `line` that disagrees with `range`" —
    /// detected at the one place both are read against a document.
    ///
    /// `Location::at_node` stops the two being built inconsistently, so this
    /// is not a handler getting the row wrong: it is the text moving between
    /// the handler's read and the driver's. `conformance-005` refused a
    /// per-query read cache, which means the conversion re-reads the target
    /// file, and a file edited in between yields offsets that are stale and
    /// still in range — an answer pointing confidently at the wrong place,
    /// which is the failure shape this design fails closed against.
    #[error("a location carries line {carried}, and its range starts on line {found}")]
    LineDisagreesWithRange {
        carried: LineIndex,
        found: LineIndex,
    },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HandlerError {
    /// The one error class the dispatch wrapper maps back to an abstention,
    /// because a deadline expiry is a decision rather than a failure
    /// (`core.md` §1, §5). Produced by `ProjectView` refusing to start I/O
    /// that cannot be used, so a handler doing ordinary `?` propagation
    /// surfaces it here — and by `SnapshotSeed::realise` abandoning a parse,
    /// which is the one expiry no handler can report because it happens
    /// before there is a handler.
    ///
    /// In `HandlerError` rather than in `ParseError` for that second case
    /// deliberately: the arm decides what the record says, and an abandoned
    /// parse is a query that ran out of time, not a document that would not
    /// parse (`core.md` §7).
    ///
    /// `classified` is `core-025` (accepted, option C): the prior the handler
    /// had assigned when the clock took its answer away, or `None` when nothing
    /// had assigned one. Without it the common shape in the field — §8 assigns
    /// the prior from the reference *before* the search, and the search is where
    /// the I/O and therefore the expiries are — loses the stratum at the seam,
    /// and the query lands in §7's coverage denominator under a class it was
    /// never asked about.
    ///
    /// An `Option` and not a stratum with a "nothing" member, because the two
    /// producers really do differ: a read refused inside a handler may know the
    /// class, and `SnapshotSeed::realise` abandoning a parse cannot — no handler
    /// has run. What is left of the second case is what `core-025` settles with
    /// option B.
    #[error("deadline expired")]
    DeadlineExpired { classified: Option<Stratum> },
}

impl HandlerError {
    /// The expiry no handler could have classified: `SnapshotSeed::realise`
    /// giving up on a parse, and every caller that is not a `ProjectView`
    /// holding a published prior.
    ///
    /// A named constructor rather than `DeadlineExpired { classified: None }` at
    /// each site, because `CLAUDE.md` asks that a call site not read
    /// `foo(None)` — and because the `None` here is a claim ("nothing had
    /// classified this") rather than an absence of interest.
    pub fn expired_unclassified() -> Self {
        Self::DeadlineExpired { classified: None }
    }
}
