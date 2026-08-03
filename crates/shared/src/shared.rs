//! The shared vocabulary: the types every other crate needs to talk about,
//! and almost no behaviour. `design/core.md` §1 has the handler seam, §8 the
//! hand-written LSP wire types, and §9 why this is a crate of its own rather
//! than the bottom of `driver`.
//!
//! The text-shaped newtypes are re-exported rather than defined: they live in
//! `vendor/rope`, because `shared` depends on `rope` and the dependency cannot
//! run the other way (`rope-modifications.md` §2). Every other crate says
//! `shared::Offset` and never has to know which side of that edge it came
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
pub use deadline::{Clock, Deadline, SystemClock, TestClock};
pub use document::{DocumentSnapshot, ParseKind, SnapshotSeed, input_edit};
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
pub use rope::{ByteColumn, ByteLen, ByteRange, CharCount, LineIndex, Offset, Rope, Utf16Column};
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

/// The workspace's map and set. `deps.md#fxhashmap-and-fxhashset-are-the-default`
/// makes these an alias rather than a naked `use rustc_hash::FxHashMap`, for two
/// reasons that a naked import gives up: switching hashers later is this line
/// rather than a sweep, and the choice is visible at every use site instead of
/// being hidden in an import block.
///
/// Nothing here is keyed by untrusted input — every map is keyed by a
/// `DocumentUri`, an `Offset`, a `LanguageId`, an `EditorRequestId`, a
/// `ProjectPath`, or a small tuple of those, all of which the shim itself
/// constructed from one editor and one language server — so std's SipHash buys
/// no protection anything needs and costs a fixed setup per lookup on the
/// definition path.
///
/// Reach for `std::collections::HashMap` when a key is genuinely external and
/// unbounded, and say so in a comment when you do: `deps.md` wants an
/// unexplained `HashMap` to read as an oversight, and
/// `driver/tests/seam.rs`'s scan is what makes that readable rather than
/// conventional.
pub type Map<K, V> = rustc_hash::FxHashMap<K, V>;
pub type Set<T> = rustc_hash::FxHashSet<T>;

/// The same choice, for the maps whose type we do not own. `lru::LruCache` is
/// a `HashMap` inside and takes its hasher as a parameter, so a cache built
/// with `LruCache::new` would be the SipHash exception without anybody
/// choosing it — and `deps.md` §8's parse cache is keyed by a `DocumentUri` and
/// a `DocumentVersion`, which is the case the section argues about by name.
pub type Hasher = rustc_hash::FxBuildHasher;
