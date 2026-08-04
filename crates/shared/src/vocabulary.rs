//! `design/core.md` §1's vocabulary types — the ones `shared` defines. The
//! text-shaped half is `rope`'s and is only re-exported (`shared.rs`).
//!
//! Almost every value crossing the seam is an offset, an index, or an
//! identifier, and those are exactly the things that silently substitute for
//! each other.
//!
//! **The wire-facing ones are the deserialization targets themselves**, which
//! is §1's closing sentence and the whole motive §8.1 gives for not taking
//! `lsp-types`: with a foreign types crate a `DocumentUri` could only ever be
//! what a conversion layer produced *after* the fact, and that layer is
//! optional in a way nothing in review catches. Here `proto`'s projections
//! name these types directly, so normalization happens on the way in or not at
//! all. Three types are wire-facing — `DocumentUri`, `DocumentVersion`,
//! `EditorRequestId` — and the rest deliberately are not: `LanguageId` and
//! `FileExtension` are interned and resolve against the registry, so an
//! unknown language fails at the boundary rather than deserializing into
//! something that matches nothing.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::iter;
use std::path::{Path, PathBuf};

use rope::{ByteRange, LineIndex, Offset, Rope};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer};
use tree_sitter::Node;
use url::Url;

use crate::error::ProtocolError;

/// LSP document version, from `didOpen`/`didChange`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize)]
pub struct DocumentVersion(pub i32);

/// Interned LSP `languageId`.
///
/// §1 says "an unknown language cannot be constructed at all", and what holds
/// that up is the `&'static str` — not `new`'s visibility, which is public.
/// A `languageId` off a wire is a `Box<str>` and cannot become a `&'static
/// str`, so the only text that can reach this type is text written into some
/// crate's source, and a `lang_*` crate writing one is that handler declaring
/// it. `Registry::language_id` is the lookup that turns an incoming string
/// into an `Option<LanguageId>`, so an unknown language fails at the boundary
/// instead of travelling inward as a string that matches nothing.
///
/// Comparison is `str` equality on the interned text and deliberately not
/// pointer identity: two crates may each write `"rust"` into their own
/// `&'static str`, and an id that compared unequal to itself across a crate
/// boundary would fail to resolve a handler that had declared it. The cheap
/// comparison this type buys is over a short string with no allocation, not
/// over an address.
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
/// `Url::parse` does the normalization — scheme and host case,
/// percent-encoding, dot segments — and `Deserialize` runs it *inside* the
/// visitor, so a URI that reached a wire type has already been normalized and
/// there is no unnormalized form for anything downstream to hold (`core.md`
/// §1, §8.1).
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

impl<'de> Deserialize<'de> for DocumentUri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UriVisitor;

        impl Visitor<'_> for UriVisitor {
            type Value = DocumentUri;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a document URI")
            }

            // The typed `ProtocolError` is flattened into serde's error here
            // and rebuilt at the frame boundary: `Deserializer::Error` is the
            // format's type, not ours, and `custom` takes a `Display`. What
            // the caller needs is that it *failed*, which §8.6 turns into an
            // untrusted document.
            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
                DocumentUri::parse(text).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(UriVisitor)
    }
}

/// The editor's request id, as it arrived: JSON's `number | string`, and the
/// key `core` holds a pending query under (`shim.md` §7).
///
/// Stored as the id's **JSON** text, which is what "normalized text form" has
/// to mean for a value with two kinds: the number `42` is `42` and the string
/// `"42"` is `"42"`, so an editor whose ids are numbers and one whose ids are
/// digit strings cannot alias in the pending map — and §8.2's response
/// envelope can echo the id back in the kind it arrived in, which LSP requires
/// and which a decoded form cannot do. Escapes are decoded and the string
/// requoted by the rule `serde_json` itself uses, so an id spelled with a
/// `b` escape and the same id spelled with a `b` are one key.
///
/// **The two producers agreeing is the point.** `shim.md` §3.1's bounded
/// prefix scan declines on any id it is not certain about — a backslash, a
/// fraction, an exponent — so on everything it does accept, its raw slice and
/// the `serde_json` fallback beside it arrive here at the same text.
///
/// Distinct from the shim's own outgoing ids, which are `"hj-<random>-<n>"`
/// and are minted rather than received (`shim.md` §Request id namespacing):
/// the two cannot be confused because nothing converts one into the other.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EditorRequestId(Box<str>);

impl EditorRequestId {
    pub fn from_number(id: i64) -> Self {
        Self::from_integer_text(id.to_string())
    }

    /// `text` is what the JSON string *denotes*, not how it was spelled — the
    /// decoded content, as a deserializer hands it over and as §3.1's scanner
    /// has it for the escape-free ids it accepts.
    pub fn from_string(text: &str) -> Self {
        let mut json = String::with_capacity(text.len() + 2);
        json.push('"');
        for scalar in text.chars() {
            match scalar {
                '"' => json.push_str("\\\""),
                '\\' => json.push_str("\\\\"),
                '\u{8}' => json.push_str("\\b"),
                '\u{c}' => json.push_str("\\f"),
                '\n' => json.push_str("\\n"),
                '\r' => json.push_str("\\r"),
                '\t' => json.push_str("\\t"),
                // Exactly JSON's mandatory set, and no wider: escaping DEL or
                // U+0085 as well would still be valid JSON but would stop this
                // being byte-identical to what `serde_json` writes, which is
                // what makes the echo above a copy rather than a re-encoding.
                control if control < '\u{20}' => {
                    let code = u32::from(control);
                    json.push_str(&format!("\\u{code:04x}"));
                }
                other => json.push(other),
            }
        }
        json.push('"');
        Self(json.into_boxed_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_integer_text(text: String) -> Self {
        Self(text.into_boxed_str())
    }
}

impl fmt::Display for EditorRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EditorRequestId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor;

        impl Visitor<'_> for IdVisitor {
            type Value = EditorRequestId;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON-RPC request id: an integer or a string")
            }

            fn visit_i64<E: serde::de::Error>(self, id: i64) -> Result<Self::Value, E> {
                Ok(EditorRequestId::from_number(id))
            }

            // Ids above `i64::MAX` are not something any editor emits, but
            // they are legal JSON and their text is unambiguous, so there is
            // nothing to gain by refusing one.
            fn visit_u64<E: serde::de::Error>(self, id: u64) -> Result<Self::Value, E> {
                Ok(EditorRequestId::from_integer_text(id.to_string()))
            }

            fn visit_str<E: serde::de::Error>(self, id: &str) -> Result<Self::Value, E> {
                Ok(EditorRequestId::from_string(id))
            }
        }

        // No `visit_f64`, so `1.0` is refused rather than keyed: JSON-RPC 2.0
        // says an id number "SHOULD NOT contain fractional parts", §3.1's
        // scanner declines on one, and a fraction that reached the map would
        // be a key the scanner can never reproduce. Failing closed here is
        // §8.6's rule applied to the one field routing depends on.
        deserializer.deserialize_any(IdVisitor)
    }
}

/// A definition site, as handlers speak of it: byte offsets, always. The wire
/// form is `proto::WireLocation` and only the driver builds it (`core.md`
/// §8.4).
///
/// `line` is redundant with `range` but is not encoding — row plus byte range
/// is still entirely byte-space. It is carried because a handler gets it for
/// free from the tree-sitter node it already verified, because §6's predicate
/// compares `(uri, line)` and reads nothing, and because it is the witness
/// that detects a target file having moved between the handler's read and the
/// driver's ([`line_in`](Location::line_in)).
///
/// The fields are private and the only constructor is `at_node`, so the row
/// and the range are derived from one node and cannot drift apart — a line
/// that disagrees with its range is a confidently wrong jump a few lines off,
/// which is this tool's value proposition inverted
/// (`state/decisions/conformance-004.md`, answered).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Location {
    uri: DocumentUri,
    range: ByteRange,
    line: LineIndex,
}

impl Location {
    pub fn at_node(uri: DocumentUri, node: &Node<'_>) -> Self {
        // A document with more than u32::MAX lines is not one tree-sitter
        // parsed: `Point.row` is a u32 on the C side already, so this
        // saturation is unreachable rather than lossy.
        let row = u32::try_from(node.start_position().row).unwrap_or(u32::MAX);
        Self {
            uri,
            range: ByteRange {
                start: Offset(node.start_byte()),
                end: Offset(node.end_byte()),
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

    /// The row `range.start` actually falls on in `text` — which is what
    /// [`line`](Location::line) claims, read back from a document rather than
    /// from the node the location was built at.
    ///
    /// `core.md` §8.4 names the risk this answers: "a `line` that disagrees
    /// with `range`". `at_node` derives both from one node, so they cannot
    /// drift apart *while the text stands still*. What `at_node` cannot cover
    /// is the text moving underneath them, and `conformance-005`'s accepted
    /// ruling makes that reachable: with no per-query read cache, the driver
    /// re-reads the target file to convert, so a file edited between the
    /// handler's read and the conversion yields offsets that are stale and
    /// still in range. Comparing the two rows is what turns that into a
    /// detected inconsistency instead of an answer pointing somewhere else.
    ///
    /// It lives here because `rope::Point` is deliberately not re-exported:
    /// the row conversion belongs beside the type that carries the row.
    pub fn line_in(&self, text: &Rope) -> LineIndex {
        text.offset_to_point(self.range.start).row
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
///
/// Interned in the same sense `LanguageId` is: [`ServerId::KNOWN`] is the only
/// place one is built, so an id that names no server in the matrix cannot be
/// constructed and "we have no profile for this server" stays a `None` rather
/// than an id nothing will ever match.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ServerId(&'static str);

impl ServerId {
    /// The server matrix, as `servers.toml` at the repository root names it.
    ///
    /// A second copy of that file's table keys, which is normally the thing
    /// `core.md` §7 refuses ("a directory tree in two documents is a directory
    /// tree that will disagree with itself"). It is a copy because the
    /// manifest is data read at runtime by `measure_core` and this is a
    /// compile-time interning table in a crate that reads no files, and
    /// because `servers.toml` is in no loop's write list — a loop that could
    /// edit its own oracle list would be choosing its own examiner.
    /// `driver/tests/oracle.rs` asserts the two agree in both directions, so
    /// the copy cannot drift silently.
    pub const KNOWN: &'static [Self] = &[
        Self("rust-analyzer"),
        Self("gopls"),
        Self("clangd"),
        Self("pyright"),
        Self("basedpyright"),
        Self("pylsp"),
        Self("typescript-language-server"),
        Self("vtsls"),
    ];

    /// The server named as `servers.toml` spells it — a `truth.jsonl`
    /// provenance header or `measure`'s `--server`, where the oracle is
    /// recorded by name because there is no child process to look at.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::KNOWN.iter().copied().find(|known| known.0 == name)
    }

    /// The shim's half: the child's command line, which is what the user's
    /// editor was configured with rather than anything we chose.
    ///
    /// Matching is over the path components of every word and not the program
    /// alone, because half the matrix is launched through an interpreter —
    /// `node …/pyright/…/langserver.index.js --stdio` has a program name of
    /// `node`, and a resolver that read only the program would fail to
    /// identify exactly the servers a profile is most likely to be needed for.
    ///
    /// Two distinct matches resolve to `None`. A command line naming both
    /// `pyright` and `basedpyright` is one we cannot identify, and answering
    /// with whichever came first would attach a profile to the wrong server —
    /// where `None` is already the documented state for a server we have no
    /// profile for.
    pub fn from_command(program: &OsStr, arguments: &[OsString]) -> Option<Self> {
        let words = iter::once(program).chain(arguments.iter().map(OsString::as_os_str));
        let mut found: Option<Self> = None;

        for word in words {
            for component in Path::new(word).components() {
                let Some(text) = component.as_os_str().to_str() else {
                    continue;
                };
                let Some(id) = Self::from_name(text).or_else(|| {
                    Path::new(text)
                        .file_stem()
                        .and_then(OsStr::to_str)
                        .and_then(Self::from_name)
                }) else {
                    continue;
                };
                match found {
                    Some(already) if already != id => return None,
                    Some(_) => {}
                    None => found = Some(id),
                }
            }
        }

        found
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
