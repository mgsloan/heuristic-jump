//! The one system-wide error enum (`deps.md` §10). Not `anyhow`: `shim.md`
//! §11's failure handling is a table keyed by class of failure, and that table
//! is only enforceable if the classes are a closed set the compiler knows
//! about.
//!
//! `Error` itself is deliberately **not** `#[non_exhaustive]` — within one
//! workspace an exhaustive match on the top level is a feature — while every
//! sub-enum is, so adding a leaf is not a breaking change to the table.
//!
//! Four of `deps.md` §10's nine arms are absent: `Config`, `Codec`, `Child`
//! and `Document` classify failures of code that does not exist yet (argv
//! parsing, framing, the child process and the open-document map). They arrive
//! with their producers, which is the same rule the dependency set follows — a
//! variant nothing can return is a row in `shim.md` §11's table that nothing
//! can exercise. `Encoding` arrived that way, with §8.3's position resolution.

use std::io;
use std::path::PathBuf;

use rope::{ByteLen, ByteOffset, LineIndex};
use thiserror::Error;

use crate::proto::PositionEncoding;
use crate::vocabulary::{DocumentUri, LanguageId};

#[derive(Debug, Error)]
pub enum Error {
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
