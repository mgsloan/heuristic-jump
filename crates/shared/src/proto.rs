//! The hand-written LSP wire types (`design/core.md` §8), and with them the
//! one module in the workspace where a position encoding is applied
//! (`core.md` §3).
//!
//! This module is `pub` where the rest of `shared`'s modules are private with
//! re-exports, because §8.7 places these types at `shared::proto` by name and
//! §8.3 spells the payoff type `proto::WirePosition`. Keeping the path means a
//! reader of either section can find the thing it names.
//!
//! **What this adds over `rope`.** `rope` already measures in UTF-16 code
//! units, so the arithmetic is its cursor seek rather than anything here. What
//! it does not do is *refuse*: `point_utf16_to_offset` clips an out-of-range
//! or mid-surrogate position to the nearest valid one and returns the result
//! with no signal that it moved. For a position that arrived over the wire
//! that is the exact failure §3 exists to prevent — a UTF-16 column read as
//! bytes is in-range and plausible on almost every line, so a clip would
//! answer about the wrong place instead of reporting that the two ends
//! disagree. Every conversion here therefore converts back and compares, and a
//! position that did not survive the round trip is an `EncodingError` rather
//! than a nearby position. `state/decisions/conformance-006.md` is where that
//! diverges from LSP 3.17's clamping rule.
//!
//! **The inventory splits three ways, and the split is the discipline.** §8.2
//! makes the incoming types *read-only projections*: partial structs naming
//! the handful of fields we read, ignoring everything else, and — decisively —
//! never serialized, because §1 of `shim.md` forbids deserializing a forwarded
//! message and writing it back. A field we did not model therefore cannot be
//! lost. The types we *construct* are a much smaller set, and they are
//! serialize-only for the same reason read from the other end: nothing
//! round-trips. The third group is the handful of value types that genuinely
//! cross in both directions — a position, a range, a location, the negotiated
//! encoding — and those are the only ones that carry both derives.
//!
//! `crates/shared/tests/proto.rs` scans this file and asserts all three lists,
//! so the split is a failing test rather than a habit, and so §8.2's table and
//! the code cannot drift apart without something saying so.
//!
//! **Untagged is permitted only where §8.5's rule allows it**: variants
//! disjoint by JSON *kind*, or by a *required* field the others lack. Never by
//! an optional field and never by declaration order — which is why
//! `contentChanges` has a hand-written `Deserialize` below rather than an
//! untagged enum whose `Full` variant would swallow every incremental change
//! and replace the document with the characters just typed.

use rope::{
    Bias, ByteColumn, ByteLen, LineIndex, Offset, Point, PointUtf16, Rope, Unclipped, Utf16Column,
};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::fmt;

use crate::error::EncodingError;
use crate::vocabulary::{DocumentUri, DocumentVersion, EditorRequestId, LanguageId};

/// What the child negotiated, which is not necessarily what we would prefer
/// (`core.md` §3). Settled once from `InitializeResult` and never inferred, so
/// there is deliberately no `Default` — a default here would be an inference.
///
/// `Utf32` is modelled even though the shim will rarely see it: this value
/// holds what the *other* end chose, and a variant set that cannot represent a
/// legal negotiation turns an unusual server into an unrepresentable state.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub enum PositionEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "utf-16")]
    Utf16,
    #[serde(rename = "utf-32")]
    Utf32,
}

/// A position exactly as it appeared on the wire. `character` is in the
/// negotiated encoding, which this type does not know — so it exposes no way
/// to be used as an offset (`core.md` §8.3).
///
/// `character` is a bare `u32` and not one of `rope`'s column newtypes on
/// purpose: `ByteColumn` and `Utf16Column` each name a unit, and naming a unit
/// is precisely what this value cannot do until [`PositionEncoding`] arrives.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct WirePosition {
    #[serde(deserialize_with = "line_index", serialize_with = "line_number")]
    line: LineIndex,
    character: u32,
}

/// `rope` does not depend on `serde` and should not start: these are two
/// functions against one field, where the alternative is a patch that widens
/// every future re-sync (`vendor/README.md`).
fn line_index<'de, D: Deserializer<'de>>(deserializer: D) -> Result<LineIndex, D::Error> {
    u32::deserialize(deserializer).map(LineIndex)
}

fn line_number<S: Serializer>(line: &LineIndex, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u32(line.0)
}

impl WirePosition {
    /// The row, which is the one part of a wire position that is not in the
    /// negotiated encoding — every encoding LSP offers counts *columns*.
    ///
    /// This does not weaken §8.3's inertness: what that section withholds is a
    /// way to be used as an offset, and a row is not one. It exists because
    /// §6's predicate compares `(uri, line)` and has to be able to read the
    /// child's answer without reading the child's document.
    pub fn line(self) -> LineIndex {
        self.line
    }

    /// The only way out. Requires naming the encoding and the document, which
    /// is exactly the information a correct conversion needs.
    pub fn resolve(self, encoding: PositionEncoding, text: &Rope) -> Result<Offset, EncodingError> {
        let last_line = text.max_point().row;
        if self.line > last_line {
            return Err(EncodingError::LineOutOfRange {
                line: self.line,
                last_line,
            });
        }
        let out_of_range = || EncodingError::CharacterOutOfRange {
            line: self.line,
            character: self.character,
            encoding,
        };
        // `conformance-006` (answered). Each arm clips and compares
        // rather than converting: `point_to_offset` and
        // `point_utf16_to_offset` reach a `debug_panic!` on a position that is
        // out of range or inside a scalar value, which panics in debug and
        // clips in release. Neither is wanted, so the clip is made explicit
        // and a position that moved is refused — where LSP 3.17 says it
        // "defaults back to the line length".
        let offset = match encoding {
            PositionEncoding::Utf8 => {
                let point = Point::new(self.line, ByteColumn(self.character));
                if text.clip_point(point, Bias::Left) != point {
                    return Err(out_of_range());
                }
                text.point_to_offset(point)
            }
            PositionEncoding::Utf16 => {
                let point = PointUtf16::new(self.line, Utf16Column(self.character));
                if text.clip_point_utf16(Unclipped(point), Bias::Left) != point {
                    return Err(out_of_range());
                }
                text.point_utf16_to_offset(point)
            }
            // Scalar values are the one unit `rope` does not carry as a
            // sum-tree dimension, so this walks the line — and gets its
            // exactness from the walk rather than from a clip. It is the
            // encoding almost nobody negotiates; paying a line scan for it is
            // better than a fourth dimension in every `TextSummary`.
            PositionEncoding::Utf32 => {
                let line_start = text.point_to_offset(Point::new(self.line, ByteColumn::ZERO));
                let mut scalars = text.chars_at(line_start);
                let mut offset = line_start;
                for _ in 0..self.character {
                    match scalars.next() {
                        Some('\n') | None => return Err(out_of_range()),
                        Some(scalar) => offset += ByteLen(scalar.len_utf8()),
                    }
                }
                offset
            }
        };
        Ok(offset)
    }

    /// The only constructor other than deserialization, which is what makes
    /// encoding something applied in two functions rather than everywhere a
    /// position is built (`core.md` §8.3).
    pub fn encode(
        offset: Offset,
        encoding: PositionEncoding,
        text: &Rope,
    ) -> Result<Self, EncodingError> {
        // `conformance-006` (answered). Refusing rather than
        // clamping, in the direction that has no LSP rule to conform to: an
        // offset this crate produced is always a boundary, so one that is not
        // came from somewhere that has already gone wrong. `rope`'s
        // `is_char_boundary` is documented to cover both halves — past the end
        // of the document, and inside a UTF-8 sequence.
        if !text.is_char_boundary(offset) {
            return Err(EncodingError::OffsetOutOfRange {
                offset,
                len: text.len(),
            });
        }
        let position = match encoding {
            PositionEncoding::Utf8 => {
                let point = text.offset_to_point(offset);
                Self {
                    line: point.row,
                    character: point.column.0,
                }
            }
            PositionEncoding::Utf16 => {
                let point = text.offset_to_point_utf16(offset);
                Self {
                    line: point.row,
                    character: point.column.0,
                }
            }
            PositionEncoding::Utf32 => {
                let line = text.offset_to_point(offset).row;
                let line_start = text.point_to_offset(Point::new(line, ByteColumn::ZERO));
                // Cannot overflow: a scalar value is at least one byte, and
                // `Point::column` already bounds a line's byte length to `u32`.
                let mut character = 0;
                let mut at = line_start;
                for scalar in text.chars_at(line_start) {
                    if at >= offset {
                        break;
                    }
                    at += ByteLen(scalar.len_utf8());
                    character += 1;
                }
                Self { line, character }
            }
        };
        Ok(position)
    }
}

/// The LSP `PositionEncodingKind` strings, so that the one place a mismatch is
/// reported reads in the vocabulary the negotiation used.
impl fmt::Display for PositionEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            PositionEncoding::Utf8 => "utf-8",
            PositionEncoding::Utf16 => "utf-16",
            PositionEncoding::Utf32 => "utf-32",
        };
        formatter.write_str(kind)
    }
}

fn encoding_kind(kind: &str) -> Option<PositionEncoding> {
    match kind {
        "utf-8" => Some(PositionEncoding::Utf8),
        "utf-16" => Some(PositionEncoding::Utf16),
        "utf-32" => Some(PositionEncoding::Utf32),
        _ => None,
    }
}

/// A range as it appeared on the wire, inert for the reason
/// [`WirePosition`] is: its ends name a column in an encoding this type does
/// not know.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct WireRange {
    pub start: WirePosition,
    pub end: WirePosition,
}

/// What goes on the wire where a handler speaks of a
/// [`Location`](crate::Location) (`core.md` §8.4).
///
/// §8.4 says handlers "never see a `WireLocation` and cannot construct one",
/// and it is worth being exact about what holds that up, because it is not the
/// types. A `WireRange` is made of `WirePosition`s and their only constructor
/// is `WirePosition::encode`, which demands a [`PositionEncoding`] — but that
/// enum's variants are public unit variants in a public module, so a handler
/// that wanted one could simply write `PositionEncoding::Utf16` and be right
/// by luck or wrong in silence. What the *compiler* enforces is only the
/// inbound half: `Query` has no encoding field, so nothing hands one to a
/// handler.
///
/// The outbound half is enforced by a source scan,
/// `driver/tests/seam.rs::no_language_crate_can_name_the_wire_vocabulary`,
/// which fails if any `lang_*` crate names this vocabulary at all. That is a
/// weaker mechanism than a private type and it is deliberate: making the
/// encoding unnameable would also stop `measure_core`, which is an LSP client
/// and legitimately encodes the position it *sends*
/// (CHANGE-conformance-012).
///
/// The conversion therefore happens where the encoding already is, which is
/// the dispatch wrapper on the worker thread.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct WireLocation {
    #[serde(serialize_with = "uri_text")]
    uri: DocumentUri,
    range: WireRange,
}

impl WireLocation {
    pub fn new(uri: DocumentUri, range: WireRange) -> Self {
        Self { uri, range }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn range(&self) -> WireRange {
        self.range
    }
}

/// The `LocationLink` shape a server may answer `textDocument/definition`
/// with. Read-only: we answer in the `Location` shape, so nothing constructs
/// one — but the oracle we are measured against may return them, and the
/// agreement predicate has to be able to read its answer (§6).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireLocationLink {
    pub origin_selection_range: Option<WireRange>,
    pub target_uri: DocumentUri,
    pub target_range: WireRange,
    pub target_selection_range: WireRange,
}

/// §8.5's fourth union, and one it permits untagged: `Location` requires
/// `uri` and `LocationLink` requires `targetUri`, and neither carries the
/// other's field, so each fails the other's deserialization on a missing
/// required field rather than on declaration order.
///
/// `[]` is the one residual ambiguity — it matches both array variants — and
/// it is harmless because both mean "no definitions". `tests/proto.rs` pins
/// which one it lands in, so that is a decision rather than something noticed
/// later.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(untagged)]
pub enum DefinitionResult {
    Null,
    One(WireLocation),
    Many(Vec<WireLocation>),
    Links(Vec<WireLocationLink>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Deprecated in LSP and nullable, which here means the same as absent:
    /// no root was named. `workspace_folders` is the one that supersedes it.
    pub root_uri: Option<DocumentUri>,
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
    pub capabilities: ClientCapabilities,
    pub client_info: Option<ClientInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkspaceFolder {
    pub uri: DocumentUri,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClientInfo {
    pub name: Box<str>,
    pub version: Option<Box<str>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    pub window: Option<WindowClientCapabilities>,
    pub general: Option<GeneralClientCapabilities>,
    pub text_document: Option<TextDocumentClientCapabilities>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowClientCapabilities {
    pub show_document: Option<ShowDocumentClientCapabilities>,
    pub show_message: Option<ShowMessageRequestClientCapabilities>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ShowDocumentClientCapabilities {
    pub support: bool,
}

/// Presence is the whole signal: LSP's own body is a nested capability about
/// action items, and what standalone needs to know is only whether the editor
/// will answer a `window/showMessageRequest` at all (`shim.md` §8).
#[derive(Clone, Debug, Deserialize)]
pub struct ShowMessageRequestClientCapabilities {}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralClientCapabilities {
    /// A kind we do not implement is dropped rather than refused: this field
    /// is a menu the editor offers, and an entry we would never have chosen
    /// is not a reason to fail `initialize`. Failing closed is for the
    /// *negotiated* value, which is `ServerCapabilities::position_encoding`.
    #[serde(default, deserialize_with = "known_encodings")]
    pub position_encodings: Vec<PositionEncoding>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentClientCapabilities {
    pub definition: Option<DefinitionClientCapabilities>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionClientCapabilities {
    pub link_support: Option<bool>,
}

fn known_encodings<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<PositionEncoding>, D::Error> {
    let offered = Option::<Vec<Box<str>>>::deserialize(deserializer)?;
    Ok(offered
        .unwrap_or_default()
        .iter()
        .filter_map(|kind| encoding_kind(kind))
        .collect())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
    pub server_info: Option<ServerInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerInfo {
    pub name: Box<str>,
    pub version: Option<Box<str>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub text_document_sync: Option<TextDocumentSync>,
    /// The negotiated encoding, and the one field in this module whose
    /// absence has a defined meaning: LSP says a server that names none means
    /// UTF-16. That default is applied where the value is settled, not here,
    /// because §3 wants "what the child chose" and "what LSP says when it
    /// chose nothing" to be distinguishable at the point of settling.
    pub position_encoding: Option<PositionEncoding>,
    /// `None` is "unsupported", and is deliberately not `#[serde(default)]`
    /// into `Supported(false)`: absent and `false` are the same answer today
    /// but arrive by different routes, and collapsing them in the projection
    /// is the shape §8.6 warns about (§8.5's third bullet).
    pub definition_provider: Option<DefinitionProvider>,
}

/// §8.5's second union: an integer enum or an options object, disjoint by
/// JSON kind.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(untagged)]
pub enum TextDocumentSync {
    Kind(TextDocumentSyncKind),
    Options(TextDocumentSyncOptions),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentSyncOptions {
    pub open_close: Option<bool>,
    pub change: Option<TextDocumentSyncKind>,
}

/// Hand-written rather than derived because the wire form is `0 | 1 | 2` and
/// `serde` has no integer-enum representation without another dependency. A
/// fourth value is refused, which is §8.6's rule applied to the field that
/// decides whether `contentChanges` carries ranges at all.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TextDocumentSyncKind {
    None,
    Full,
    Incremental,
}

impl<'de> Deserialize<'de> for TextDocumentSyncKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            0 => Ok(TextDocumentSyncKind::None),
            1 => Ok(TextDocumentSyncKind::Full),
            2 => Ok(TextDocumentSyncKind::Incremental),
            other => Err(serde::de::Error::custom(format!(
                "text document sync kind {other} is not one of 0, 1 or 2"
            ))),
        }
    }
}

impl Serialize for TextDocumentSyncKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            TextDocumentSyncKind::None => 0,
            TextDocumentSyncKind::Full => 1,
            TextDocumentSyncKind::Incremental => 2,
        })
    }
}

/// §8.5's third union: a boolean or an options object, disjoint by JSON kind.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(untagged)]
pub enum DefinitionProvider {
    Supported(bool),
    Options(DefinitionOptions),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionOptions {
    pub work_done_progress: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

/// `language_id` is a `Box<str>` and not a [`LanguageId`](crate::LanguageId)
/// on purpose, and it is the one place §8.1's "the newtypes are what
/// deserialization produces" does not apply: a `LanguageId` is interned and
/// only ids some registered handler declared exist, so resolving one is a
/// registry lookup that yields `Option<LanguageId>` and fails an unknown
/// language at the boundary (§1). A `Deserialize` impl would have to invent
/// the id instead.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: DocumentUri,
    pub language_id: Box<str>,
    pub version: DocumentVersion,
    pub text: Box<str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    pub text_document: VersionedTextDocumentIdentifier,
    pub content_changes: Vec<ContentChange>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: DocumentUri,
    pub version: DocumentVersion,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: DocumentUri,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

/// `text` is present only when the server asked for it in its `save` options,
/// and §8.6 wants it: immediately after a save the buffer and the file are
/// identical by definition, so it is a free end-to-end check on the whole
/// document-tracking pipeline at a point where the answer is known.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidSaveTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
    pub text: Option<Box<str>>,
}

/// The one field all four state-bearing notifications carry, projected on its
/// own.
///
/// It exists for §8.6: when a `didChange` or a `didSave` fails to deserialize,
/// the document that message was about is exactly what the fail-closed rule
/// needs and exactly what the failed deserialization did not produce. Reading
/// the identifier separately recovers it in the common case — the failure is
/// somewhere in `contentChanges`, not in `textDocument` — so one document is
/// distrusted rather than every open one.
///
/// Lenient in the way §8.2 makes every projection lenient, and here that is the
/// point rather than a hazard: this is not a variant of an untagged enum, and
/// ignoring everything it did not model is what lets it read the identifier out
/// of a message whose other half is malformed.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifiedDocument {
    pub text_document: TextDocumentIdentifier,
}

/// §8.5's fifth union, and the one that gets a hand-written `Deserialize`:
/// `{text}` is a *subset* of `{range, rangeLength?, text}`, so the variants
/// are not disjoint and untagged would decide by declaration order. The
/// failure that would produce is the worst shape in this design — a `Full`
/// variant accepting an incremental change, ignoring `range` as an unknown
/// field, and replacing the whole document with the characters just typed,
/// with nothing erroring and every later answer confidently about text the
/// user does not have.
///
/// So the dispatch is on the presence of `range`, and `range: null` is a
/// *failure* rather than a `Full`: it is not a shape LSP defines, and the
/// direction to fail in is the one that does not silently discard the
/// document.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ContentChange {
    Incremental { range: WireRange, text: Box<str> },
    Full { text: Box<str> },
}

impl<'de> Deserialize<'de> for ContentChange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field {
            Range,
            Text,
            #[serde(other)]
            Other,
        }

        struct ChangeVisitor;

        impl<'de> Visitor<'de> for ChangeVisitor {
            type Value = ContentChange;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a content change: {range, text} or {text}")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Self::Value, A::Error> {
                let mut range: Option<WireRange> = None;
                let mut text: Option<Box<str>> = None;
                while let Some(field) = entries.next_key::<Field>()? {
                    match field {
                        Field::Range => {
                            if range.is_some() {
                                return Err(serde::de::Error::duplicate_field("range"));
                            }
                            range = Some(entries.next_value()?);
                        }
                        Field::Text => {
                            if text.is_some() {
                                return Err(serde::de::Error::duplicate_field("text"));
                            }
                            text = Some(entries.next_value()?);
                        }
                        // `rangeLength` among them, which is deprecated and
                        // redundant with `range`.
                        Field::Other => {
                            entries.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                let text = text.ok_or_else(|| serde::de::Error::missing_field("text"))?;
                Ok(match range {
                    Some(range) => ContentChange::Incremental { range, text },
                    None => ContentChange::Full { text },
                })
            }
        }

        deserializer.deserialize_map(ChangeVisitor)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: WirePosition,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CancelParams {
    pub id: EditorRequestId,
}

#[derive(Debug, Deserialize)]
pub struct ProgressParams {
    pub token: ProgressToken,
    /// Left raw, because what a progress value *means* is the per-server
    /// question `ServerAdapter` exists to answer (`shim.md` §6); modelling it
    /// here would commit every server to one shape of it.
    pub value: Box<RawValue>,
}

/// A progress token, untagged because number and string are disjoint by JSON
/// kind. Not an [`EditorRequestId`]: these are the *child's* tokens and are
/// matched against tokens we minted, so normalizing them into the editor's id
/// space would put two unrelated namespaces in one type.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    Number(i64),
    Text(Box<str>),
}

/// Always `"2.0"`. A type rather than a field so that no construction site
/// spells the value, right or wrong.
#[derive(Copy, Clone, Default, Debug)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("2.0")
    }
}

/// The one envelope we construct. Generic over the result because the shim
/// answers exactly two kinds of request — a definition, and standalone's
/// `initialize` — and a single `serde_json::Value`-shaped result field would
/// undo the typing everywhere it is used.
#[derive(Debug, Serialize)]
pub struct Response<T> {
    jsonrpc: JsonRpcVersion,
    #[serde(serialize_with = "raw_json_id")]
    id: EditorRequestId,
    #[serde(flatten)]
    outcome: ResponseOutcome<T>,
}

impl<T> Response<T> {
    pub fn result(id: EditorRequestId, result: T) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            outcome: ResponseOutcome::Result(result),
        }
    }

    pub fn failure(id: EditorRequestId, error: ResponseError) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            outcome: ResponseOutcome::Error(error),
        }
    }
}

/// Externally tagged, which is what JSON-RPC's "`result` xor `error`" is: the
/// two cannot both appear, and neither can be absent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseOutcome<T> {
    Result(T),
    Error(ResponseError),
}

#[derive(Clone, Debug, Serialize)]
pub struct ResponseError {
    pub code: ErrorCode,
    pub message: Box<str>,
}

/// JSON-RPC's `ErrorCodes` and LSP's `LSPErrorCodes`, as integers because
/// that is what the wire carries and the set is open — a server may define
/// its own.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
pub struct ErrorCode(pub i32);

impl ErrorCode {
    pub const INVALID_PARAMS: Self = Self(-32602);
    pub const INTERNAL_ERROR: Self = Self(-32603);
    pub const REQUEST_CANCELLED: Self = Self(-32800);
}

/// LSP's `MessageType`, written as the integer the wire carries.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MessageType {
    Error,
    Warning,
    Info,
    Log,
}

impl Serialize for MessageType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            MessageType::Error => 1,
            MessageType::Warning => 2,
            MessageType::Info => 3,
            MessageType::Log => 4,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ShowMessageParams {
    #[serde(rename = "type")]
    pub message_type: MessageType,
    pub message: Box<str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShowMessageRequestParams {
    #[serde(rename = "type")]
    pub message_type: MessageType,
    pub message: Box<str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<MessageActionItem>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageActionItem {
    pub title: Box<str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowDocumentParams {
    #[serde(serialize_with = "uri_text")]
    pub uri: DocumentUri,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_focus: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<WireRange>,
}

/// Standalone's own `InitializeResult` (`shim.md` §14.3): there is no child
/// to relay one from, so this is the whole of what the editor is told.
///
/// A separate type from the read [`InitializeResult`] rather than one type
/// carrying both derives, because a projection that can be written back is
/// exactly the round trip §8.2 removes — and the two have different fields
/// anyway: what we *support* is not optional, where what a child reports is.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneInitializeResult {
    pub capabilities: StandaloneServerCapabilities,
    pub server_info: StandaloneServerInfo,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneServerCapabilities {
    pub position_encoding: PositionEncoding,
    pub text_document_sync: TextDocumentSyncKind,
    pub definition_provider: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct StandaloneServerInfo {
    pub name: &'static str,
    pub version: &'static str,
}

/// The id is echoed as the JSON text it arrived as, which is what
/// [`EditorRequestId`] stores and why it stores it that way: LSP requires the
/// response id to be the request's, and a number that came back as a string
/// is a response the editor cannot match.
fn raw_json_id<S: Serializer>(id: &EditorRequestId, serializer: S) -> Result<S::Ok, S::Error> {
    let raw = RawValue::from_string(id.as_str().to_owned()).map_err(serde::ser::Error::custom)?;
    raw.serialize(serializer)
}

fn uri_text<S: Serializer>(uri: &DocumentUri, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(uri.as_str())
}

// ---------------------------------------------------------------------------
// The client half: what `measure_core` sends to a language server.
//
// §8.2's inventory was written for the shim, which sits between an editor and
// a server and *reads* every request. `measure_core` is a plain LSP client
// (§7's table: "plain LSP client, no editor"), so it constructs the same
// messages the shim reads — and §8.2's rule is that a projection which can be
// written back is the round trip the design removes. These are therefore
// separate types with `Serialize` only, exactly as `StandaloneInitializeResult`
// is separate from the read `InitializeResult` and for the same reason.
//
// They live here rather than in `measure_core` because §8.7 puts the wire
// types in `shared::proto`, and because the alternative is a second vocabulary
// for the same protocol in the one crate whose job is to agree with the shim.
// ---------------------------------------------------------------------------

/// A request we originate. The id is a plain integer and deliberately not an
/// [`EditorRequestId`]: that type is the *editor's* id space, and these are
/// minted by us — the two must not be able to alias.
#[derive(Debug, Serialize)]
pub struct ClientRequest<T> {
    jsonrpc: JsonRpcVersion,
    id: i64,
    method: &'static str,
    params: T,
}

impl<T> ClientRequest<T> {
    pub fn new(id: i64, method: &'static str, params: T) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            method,
            params,
        }
    }
}

/// A notification we originate. No id, which is the whole difference, and
/// therefore no response to correlate.
#[derive(Debug, Serialize)]
pub struct ClientNotification<T> {
    jsonrpc: JsonRpcVersion,
    method: &'static str,
    params: T,
}

impl<T> ClientNotification<T> {
    pub fn new(method: &'static str, params: T) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            method,
            params,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInitializeParams {
    pub process_id: Option<u32>,
    #[serde(serialize_with = "uri_text")]
    pub root_uri: DocumentUri,
    pub workspace_folders: Vec<ClientWorkspaceFolder>,
    pub capabilities: ClientOfferedCapabilities,
    pub client_info: ClientIdentity,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientWorkspaceFolder {
    #[serde(serialize_with = "uri_text")]
    pub uri: DocumentUri,
    pub name: Box<str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientIdentity {
    pub name: &'static str,
    pub version: &'static str,
}

/// What the corpus scan asks a server for, which is much less than an editor
/// asks: one request kind, and the encoding it will read positions in.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientOfferedCapabilities {
    pub general: ClientGeneralCapabilities,
    pub text_document: ClientTextDocumentCapabilities,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientGeneralCapabilities {
    /// Preference order, most preferred first — LSP's rule, and the reason
    /// this is a list rather than a value.
    pub position_encodings: Vec<PositionEncoding>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientTextDocumentCapabilities {
    pub definition: ClientDefinitionCapabilities,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDefinitionCapabilities {
    /// Declared `true`, so a server that prefers `LocationLink[]` sends it —
    /// §6's predicate reads all four shapes, and refusing links here would
    /// silently change what the oracle says rather than what we can read.
    pub link_support: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDidOpenParams {
    pub text_document: ClientTextDocumentItem,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientTextDocumentItem {
    #[serde(serialize_with = "uri_text")]
    pub uri: DocumentUri,
    /// A [`LanguageId`](crate::LanguageId) rather than the read side's
    /// `Box<str>`: an id we *send* is one a registered handler declared, so
    /// unlike an incoming one there is nothing to fail at the boundary.
    #[serde(serialize_with = "language_id_text")]
    pub language_id: LanguageId,
    #[serde(serialize_with = "version_number")]
    pub version: DocumentVersion,
    pub text: Box<str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDidCloseParams {
    pub text_document: ClientTextDocumentIdentifier,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientTextDocumentIdentifier {
    #[serde(serialize_with = "uri_text")]
    pub uri: DocumentUri,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDefinitionParams {
    pub text_document: ClientTextDocumentIdentifier,
    pub position: WirePosition,
}

/// One frame from a child, triaged. A server interleaves its own requests and
/// notifications with the responses we are waiting for, and a client that
/// ignored the requests would hang the ones a server blocks on — so the reader
/// has to tell the three apart, and it has to do it without a second pass over
/// the frame.
///
/// `id` is raw rather than an `i64` because a *server's* request ids are in
/// the server's own space and need not be integers, and re-serializing the
/// text is how a reply echoes one back. That is not a round trip in §8.2's
/// sense: nothing is projected out of it and put back.
///
/// `result` and `error` are both optional even though JSON-RPC says exactly
/// one is present on a response, because deciding that is the reader's job —
/// where it can name the method that failed, which §8.6 wants and a derive
/// cannot do.
#[derive(Debug, Deserialize)]
pub struct ChildFrame<T> {
    pub id: Option<Box<RawValue>>,
    pub method: Option<Box<str>>,
    pub result: Option<T>,
    pub error: Option<ChildResponseError>,
}

impl<T> ChildFrame<T> {
    /// A response to the request we minted, matched on the id's JSON text so
    /// that `7` and `"7"` cannot alias — the same rule
    /// [`EditorRequestId`] applies in the other direction.
    pub fn answers(&self, id: i64) -> bool {
        self.method.is_none()
            && self
                .id
                .as_ref()
                .is_some_and(|raw| raw.get().trim() == id.to_string())
    }

    /// A request *from* the server: it has both a method and an id, and it is
    /// waiting. Returns the id to echo back.
    pub fn awaiting_reply(&self) -> Option<&RawValue> {
        match (&self.method, &self.id) {
            (Some(_), Some(id)) => Some(id),
            (None, _) | (_, None) => None,
        }
    }
}

/// The reply a client owes a server request it does not implement. LSP has no
/// "unhandled" for a request a client advertised no capability for, and a
/// server that gets no answer at all blocks; a null result is what leaves it
/// free to continue.
#[derive(Debug, Serialize)]
pub struct ClientReply<'a> {
    jsonrpc: JsonRpcVersion,
    #[serde(serialize_with = "raw_passthrough")]
    id: &'a RawValue,
    result: Option<()>,
}

impl<'a> ClientReply<'a> {
    pub fn nothing(id: &'a RawValue) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id,
            result: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChildResponseError {
    pub code: i64,
    pub message: Box<str>,
}

fn raw_passthrough<S: Serializer>(raw: &&RawValue, serializer: S) -> Result<S::Ok, S::Error> {
    raw.serialize(serializer)
}

fn language_id_text<S: Serializer>(id: &LanguageId, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(id.as_str())
}

/// Written here rather than as a `Serialize` derive on [`DocumentVersion`]
/// itself: the vocabulary newtypes are the seam `state/phase.toml` freezes,
/// and a wire concern is not a reason to reach into it.
fn version_number<S: Serializer>(
    version: &DocumentVersion,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_i32(version.0)
}
