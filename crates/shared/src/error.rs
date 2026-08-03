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
    #[error("{path}:{line} is not a key, a table header or a comment")]
    ManifestMalformed { path: PathBuf, line: usize },
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
    /// `data-collection.md` §4: resuming a truth file whose server has moved
    /// underneath it is refused, because half a file from one version and half
    /// from another is the one outcome with no honest provenance header.
    #[error("{path} was collected against {recorded}, and the installed server is {installed}")]
    ServerVersionDrift {
        path: PathBuf,
        recorded: Box<str>,
        installed: Box<str>,
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
    #[error("frame body is not the JSON its length claimed")]
    BodyNotJson {
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
    #[error("spawning the file-list scanner")]
    Scanner {
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
    #[error("deadline expired")]
    DeadlineExpired,
}
