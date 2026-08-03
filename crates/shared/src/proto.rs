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

use rope::{Bias, ByteLen, ByteOffset, LineIndex, Point, PointUtf16, Rope, Unclipped};
use serde::{Deserialize, Deserializer};
use std::fmt;

use crate::error::EncodingError;

/// What the child negotiated, which is not necessarily what we would prefer
/// (`core.md` §3). Settled once from `InitializeResult` and never inferred, so
/// there is deliberately no `Default` — a default here would be an inference.
///
/// `Utf32` is modelled even though the shim will rarely see it: this value
/// holds what the *other* end chose, and a variant set that cannot represent a
/// legal negotiation turns an unusual server into an unrepresentable state.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
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
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct WirePosition {
    #[serde(deserialize_with = "line_index")]
    line: LineIndex,
    character: u32,
}

/// `rope` does not depend on `serde` and should not start: this is one
/// function against one field, where the alternative is a patch that widens
/// every future re-sync (`vendor/README.md`).
fn line_index<'de, D: Deserializer<'de>>(deserializer: D) -> Result<LineIndex, D::Error> {
    u32::deserialize(deserializer).map(LineIndex)
}

impl WirePosition {
    /// The only way out. Requires naming the encoding and the document, which
    /// is exactly the information a correct conversion needs.
    pub fn resolve(
        self,
        encoding: PositionEncoding,
        text: &Rope,
    ) -> Result<ByteOffset, EncodingError> {
        let last_line = LineIndex(text.max_point().row);
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
        // DECISION-conformance-006: provisional. Each arm clips and compares
        // rather than converting: `point_to_offset` and
        // `point_utf16_to_offset` reach a `debug_panic!` on a position that is
        // out of range or inside a scalar value, which panics in debug and
        // clips in release. Neither is wanted, so the clip is made explicit
        // and a position that moved is refused — where LSP 3.17 says it
        // "defaults back to the line length".
        let offset = match encoding {
            PositionEncoding::Utf8 => {
                let point = Point::new(self.line.0, self.character);
                if text.clip_point(point, Bias::Left) != point {
                    return Err(out_of_range());
                }
                text.point_to_offset(point)
            }
            PositionEncoding::Utf16 => {
                let point = PointUtf16::new(self.line.0, self.character);
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
                let line_start = text.point_to_offset(Point::new(self.line.0, 0));
                let mut scalars = text.chars_at(line_start);
                let mut offset = line_start;
                for _ in 0..self.character {
                    match scalars.next() {
                        Some('\n') | None => return Err(out_of_range()),
                        Some(scalar) => offset += scalar.len_utf8(),
                    }
                }
                offset
            }
        };
        Ok(ByteOffset(offset))
    }

    /// The only constructor other than deserialization, which is what makes
    /// encoding something applied in two functions rather than everywhere a
    /// position is built (`core.md` §8.3).
    pub fn encode(
        offset: ByteOffset,
        encoding: PositionEncoding,
        text: &Rope,
    ) -> Result<Self, EncodingError> {
        // DECISION-conformance-006: provisional. Refusing rather than
        // clamping, in the direction that has no LSP rule to conform to: an
        // offset this crate produced is always a boundary, so one that is not
        // came from somewhere that has already gone wrong. `rope`'s
        // `is_char_boundary` is documented to cover both halves — past the end
        // of the document, and inside a UTF-8 sequence.
        if !text.is_char_boundary(offset.0) {
            return Err(EncodingError::OffsetOutOfRange {
                offset,
                len: ByteLen(text.len()),
            });
        }
        let position = match encoding {
            PositionEncoding::Utf8 => {
                let point = text.offset_to_point(offset.0);
                Self {
                    line: LineIndex(point.row),
                    character: point.column,
                }
            }
            PositionEncoding::Utf16 => {
                let point = text.offset_to_point_utf16(offset.0);
                Self {
                    line: LineIndex(point.row),
                    character: point.column,
                }
            }
            PositionEncoding::Utf32 => {
                let line = LineIndex(text.offset_to_point(offset.0).row);
                let line_start = text.point_to_offset(Point::new(line.0, 0));
                // Cannot overflow: a scalar value is at least one byte, and
                // `Point::column` already bounds a line's byte length to `u32`.
                let mut character = 0;
                let mut at = line_start;
                for scalar in text.chars_at(line_start) {
                    if at >= offset.0 {
                        break;
                    }
                    at += scalar.len_utf8();
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
