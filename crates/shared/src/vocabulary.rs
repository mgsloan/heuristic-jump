//! `design/core.md` §1's vocabulary types — the ones `shared` defines. The
//! text-shaped half is `rope`'s and is only re-exported (`shared.rs`).
//!
//! Almost every value crossing the seam is an offset, an index, or an
//! identifier, and those are exactly the things that silently substitute for
//! each other.

use std::fmt;
use std::path::{Path, PathBuf};

use rope::{ByteOffset, ByteRange, LineIndex};
use tree_sitter::Node;
use url::Url;

use crate::error::ProtocolError;

/// LSP document version, from `didOpen`/`didChange`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DocumentVersion(pub i32);

/// Interned LSP `languageId`.
///
/// A handler declares its ids as consts, which is the only place one is built;
/// an incoming `languageId` is resolved against the registry and produces an
/// `Option<LanguageId>`, so an unknown language fails at the boundary instead
/// of travelling inward as a string that matches nothing. Comparison is then a
/// pointer comparison rather than a `str` one.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct LanguageId(&'static str);

impl LanguageId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// A file extension without its leading dot, as `Path::extension` yields it.
///
/// Closed files arrive from a search as a bare path with no `languageId`
/// attached, so this is the other half of the registry lookup.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FileExtension(&'static str);

impl FileExtension {
    pub const fn new(extension: &'static str) -> Self {
        Self(extension)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    pub fn matches(self, path: &Path) -> bool {
        path.extension().is_some_and(|found| found == self.0)
    }
}

impl fmt::Display for FileExtension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Normalized document URI, so URI comparison is not string comparison with
/// percent-encoding and case rules smuggled in.
///
/// `Url::parse` does the normalization every caller here needs today — scheme
/// and host case, percent-encoding, dot segments. The rest of what `core.md`
/// §1 means by normalized is a deserialization concern and arrives with
/// `proto` (§8), which is where the wire form is parsed at all.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DocumentUri(Url);

impl DocumentUri {
    pub fn parse(text: &str) -> Result<Self, ProtocolError> {
        Url::parse(text)
            .map(Self)
            .map_err(|source| ProtocolError::MalformedUri {
                text: text.into(),
                source,
            })
    }

    /// `None` for a URI that is not a `file:` URI, or that names a path this
    /// platform cannot express.
    pub fn from_file_path(path: &Path) -> Option<Self> {
        Url::from_file_path(path).ok().map(Self)
    }

    pub fn to_file_path(&self) -> Option<PathBuf> {
        self.0.to_file_path().ok()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for DocumentUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

/// A definition site, as handlers speak of it: byte offsets, always. The wire
/// form is `proto::WireLocation` and only the driver builds it (`core.md`
/// §8.4).
///
/// `line` is redundant with `range` but is not encoding — row plus byte range
/// is still entirely byte-space. It is carried because a handler gets it for
/// free from the tree-sitter node it already verified, and it saves the driver
/// a whole-file line index later.
///
/// The fields are private and the only constructor is `at_node`, so the row
/// and the range are derived from one node and cannot drift apart. See
/// `state/decisions/conformance-004.md`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Location {
    uri: DocumentUri,
    range: ByteRange,
    line: LineIndex,
}

impl Location {
    // DECISION-conformance-004: provisional. `core.md` §1 and §8.4 print this
    // struct with `pub` fields while §8.4's prose says it is "constructed only
    // through `Location::at_node`"; both cannot hold, and this is the reading
    // under which the invariant is a property of the type.
    pub fn at_node(uri: DocumentUri, node: &Node<'_>) -> Self {
        // A document with more than u32::MAX lines is not one tree-sitter
        // parsed: `Point.row` is a u32 on the C side already, so this
        // saturation is unreachable rather than lossy.
        let row = u32::try_from(node.start_position().row).unwrap_or(u32::MAX);
        Self {
            uri,
            range: ByteRange {
                start: ByteOffset(node.start_byte()),
                end: ByteOffset(node.end_byte()),
            },
            line: LineIndex(row),
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn range(&self) -> ByteRange {
        self.range
    }

    pub fn line(&self) -> LineIndex {
        self.line
    }
}

/// A calibrated estimate that an answer is the one the user wanted
/// (`resolution.md` §7.1). Recorded on every answer and, in v1, gating none of
/// them.
///
/// A newtype so the `0.0..=1.0` invariant is checked once here instead of
/// assumed at every comparison, and so a confidence can never be silently
/// swapped with a score, a threshold, or a latency.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Confidence(f32);

impl Confidence {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    /// `None` outside `0.0..=1.0`, and for a NaN, which is neither inside the
    /// range nor comparable against a threshold.
    pub fn new(value: f32) -> Option<Self> {
        (0.0..=1.0).contains(&value).then_some(Self(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

/// Interned server identity, resolved from the child's command name at
/// startup. What a server *does* differently is `ServerProfile`; this is only
/// the key (`core.md` §1).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ServerId(&'static str);

impl ServerId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ServerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}
