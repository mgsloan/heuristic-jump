//! The one system-wide error enum (`deps.md` §10). Not `anyhow`: `shim.md`
//! §11's failure handling is a table keyed by class of failure, and that table
//! is only enforceable if the classes are a closed set the compiler knows
//! about.
//!
//! `Error` itself is deliberately **not** `#[non_exhaustive]` — within one
//! workspace an exhaustive match on the top level is a feature — while every
//! sub-enum is, so adding a leaf is not a breaking change to the table.
//!
//! One of `deps.md` §10's nine arms is still absent: `Document` classifies
//! failures of the open-document map, which does not exist yet. It arrives
//! with its producer, which is the same rule the dependency set follows — a
//! variant nothing can return is a row in `shim.md` §11's table that nothing
//! can exercise. `Encoding` arrived that way with §8.3's position resolution,
//! and `Config`, `Codec` and `Child` with `measure_core`: the corpus scan is
//! the first thing in the workspace that parses arguments, frames JSON-RPC and
//! spawns a child, and it reaches them a whole phase before the shim does.

use std::io;
use std::path::PathBuf;

use rope::{ByteLen, ByteOffset, LineIndex};
use thiserror::Error;

use crate::proto::PositionEncoding;
use crate::vocabulary::{DocumentUri, LanguageId};

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
    OffsetOutOfRange { offset: ByteOffset, len: ByteLen },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HandlerError {
    /// The one error class the dispatch wrapper maps back to an abstention,
    /// because a deadline expiry is a decision rather than a failure
    /// (`core.md` §1, §5). Produced by `ProjectView` refusing to start I/O
    /// that cannot be used, so a handler doing ordinary `?` propagation
    /// surfaces it here.
    #[error("deadline expired")]
    DeadlineExpired,
}
