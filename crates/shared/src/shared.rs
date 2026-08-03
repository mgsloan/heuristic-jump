//! The shared vocabulary: the types every other crate needs to talk about,
//! and almost no behaviour. `design/core.md` §1 has the handler seam, §8 the
//! hand-written LSP wire types, and §9 why this is a crate of its own rather
//! than the bottom of `driver`.
//!
//! The text-shaped newtypes are re-exported rather than defined: they live in
//! `vendor/rope`, because `shared` depends on `rope` and the dependency cannot
//! run the other way (`rope-modifications.md` §2). Every other crate says
//! `shared::ByteOffset` and never has to know which side of that edge it came
//! from.

mod agreement;
mod deadline;
mod document;
mod error;
mod handler;
mod identifier;
mod project;
mod vocabulary;

// The one public module rather than a re-export: `core.md` §8.3 and §8.7 name
// the path `shared::proto`, and the wire types are a namespace rather than
// vocabulary — `proto::WirePosition` is meant to read as "the wire's idea of a
// position" at every use site.
pub mod proto;

pub use agreement::{Agreement, DefinitionSite, Severity};
pub use deadline::{Clock, Deadline, SystemClock};
pub use document::{DocumentSnapshot, ParseKind, SnapshotSeed};
pub use error::{
    ChildError, CodecError, ConfigError, DocumentError, DocumentNotification, EncodingError, Error,
    HandlerError, ParseError, ProjectError, ProtocolError,
};
pub use handler::{
    AbstainReason, CandidateCount, CommitPolicy, FileListEvidence, LanguageHandler, Margin, Micros,
    Outcome, Query, Refinement, ServerProfile, StageLabel, StageName, Strata, Stratum, Trace,
    TraceParts,
};
pub use identifier::{Identifiers, identifier_at, identifiers};
pub use project::{
    CandidateFiles, FileChunks, FileCount, FileHits, FileList, FileText, Generation, Hit,
    ProjectPath, ProjectRoot, ProjectView, RelPath, ScanOutcome, ScanRequest, SearchOrigin,
};
// All seven of §1's text-shaped newtypes, not just the four `shared` itself
// uses: `ByteColumn`, `Utf16Column` and `CharCount` are on the same list, and
// the reason the list is re-exported is that a crate which may not depend on
// `rope` still has to be able to name them. `driver`'s seam test is that
// crate, and asserts it.
pub use rope::{
    ByteColumn, ByteLen, ByteOffset, ByteRange, CharCount, LineIndex, Rope, Utf16Column,
};
// The tree-sitter types the seam already speaks in, re-exported for exactly
// the reason `rope`'s newtypes are: §9's graph gives `driver` no tree-sitter
// edge, and §1 gives it a parse cache — which cannot be written without naming
// `Tree`. Nothing new crosses the seam here, since `LanguageHandler::grammar`
// already hands over a `Language`; what changes is that a crate which may not
// depend on tree-sitter can name what it is handed.
pub use tree_sitter::{InputEdit, Language, Tree};
pub use vocabulary::{
    Confidence, DocumentUri, DocumentVersion, EditorRequestId, FileExtension, LanguageId, Location,
    ServerId,
};
