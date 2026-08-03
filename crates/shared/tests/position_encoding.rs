//! `design/core.md` §3's "exhaustive property tests against a reference
//! implementation", and §10's "random text with astral-plane characters,
//! round-tripped UTF-8/UTF-16/byte offsets against a reference".
//!
//! The reference is the whole point, so it is written the slow obvious way:
//! `&str`, one scalar at a time, no sum tree and no bitmaps. `shared::proto`
//! is fast because `rope` measures UTF-16 as a dimension of its summary; that
//! machinery is exactly what these tests are not allowed to trust.
//!
//! The alphabet below is chosen so that the four cases that differ are all
//! reachable in a forty-character document: ASCII (1 byte, 1 UTF-16 unit),
//! Latin-1 (2 bytes, 1 unit), CJK (3 bytes, 1 unit) and astral (4 bytes, **2**
//! units). Astral is the one that matters — a surrogate pair is the only place
//! a UTF-16 column can land *inside* a scalar value, and `resolve` must refuse
//! there rather than answer about the scalar's first byte. `\r` is in the
//! alphabet so that a `\r\n` line ending is generated: `rope` splits lines on
//! `\n` alone and leaves the `\r` in the line, and the reference has to agree
//! with it about that.
//!
//! Constructing a `WirePosition` here goes through `serde_json`, because
//! deserialization is the only door in that is not `encode` — which is
//! `core.md` §8.3's design and not an inconvenience to work around. It means
//! these tests exercise the wire form the shim will actually receive.

use proptest::prelude::{Just, ProptestConfig, Strategy, prop_assert, prop_assert_eq};
use proptest::{prop_oneof, proptest};
use rope::{Offset, Rope};
use shared::proto::{PositionEncoding, WirePosition};

const ALPHABET: &[char] = &[
    'a', 'b', 'Z', ' ', '\t', '\n', '\n', '\r', '{', 'é', 'ü', 'ß', '中', '日', '😀', '𝄞', '👨',
];

fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(ALPHABET), 0..40)
        .prop_map(|scalars| scalars.into_iter().collect())
}

fn ascii_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(&['a', 'b', 'Z', ' ', '\t', '\n', '{'][..]),
        0..40,
    )
    .prop_map(|scalars| scalars.into_iter().collect())
}

fn encoding() -> impl Strategy<Value = PositionEncoding> {
    prop_oneof![
        Just(PositionEncoding::Utf8),
        Just(PositionEncoding::Utf16),
        Just(PositionEncoding::Utf32),
    ]
}

/// The only way to build one, and the one the shim will use.
#[expect(
    clippy::panic,
    reason = "the JSON is built here from two u32s, so the failure is \
              unreachable and there is no value to fall back to — a \
              WirePosition cannot be constructed any other way, which is the \
              point of the type. clippy's allow-panic-in-tests does not reach \
              a helper in an integration test: it looks for an enclosing \
              #[test], and there is none here."
)]
fn wire(line: u32, character: u32) -> WirePosition {
    let json = format!(r#"{{"line":{line},"character":{character}}}"#);
    match serde_json::from_str(&json) {
        Ok(position) => position,
        Err(error) => panic!("{json} is not a WirePosition: {error}"),
    }
}

// The two conversions below saturate rather than unwrapping, and both are
// unreachable in a forty-scalar document: `unwrap`, `expect`, `panic` and
// `unreachable` are all denied outside a `#[test]` body, and clippy's
// `allow-*-in-tests` does not reach a helper function in an integration test.
// Saturating is the safe direction — `u32::MAX` is not a position any of these
// documents has, so a value that did saturate fails an assertion rather than
// passing one.

fn small(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn index(line: u32) -> usize {
    usize::try_from(line).unwrap_or(usize::MAX)
}

fn units(scalar: char, encoding: PositionEncoding) -> u32 {
    small(match encoding {
        PositionEncoding::Utf8 => scalar.len_utf8(),
        PositionEncoding::Utf16 => scalar.len_utf16(),
        PositionEncoding::Utf32 => 1,
    })
}

/// Byte offset of the first character of each line, including the empty line
/// after a trailing newline.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|&(_, byte)| byte == b'\n')
            .map(|(at, _)| at + 1),
    );
    starts
}

/// The line's text without its terminating newline, which is not part of the
/// line for the purpose of a column.
fn line_text(text: &str, line: usize) -> Option<&str> {
    let starts = line_starts(text);
    let start = *starts.get(line)?;
    let end = starts.get(line + 1).map_or(text.len(), |&next| next - 1);
    Some(&text[start..end])
}

fn reference_encode(text: &str, offset: usize, encoding: PositionEncoding) -> (u32, u32) {
    let starts = line_starts(text);
    // Line 0 starts at 0 and an offset is never negative, so the fallback is
    // unreachable and is also the right answer if it were not.
    let line = starts
        .iter()
        .rposition(|&start| start <= offset)
        .unwrap_or(0);
    let column: u32 = text[starts[line]..offset]
        .chars()
        .map(|scalar| units(scalar, encoding))
        .sum();
    (small(line), column)
}

/// `None` where there is no such position: past the last line, past the end of
/// its line, or inside a scalar value in the negotiated encoding.
fn reference_resolve(
    text: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Option<usize> {
    let line = index(line);
    let start = *line_starts(text).get(line)?;
    let mut at = start;
    let mut counted = 0;
    for scalar in line_text(text, line)?.chars() {
        if counted == character {
            return Some(at);
        }
        counted += units(scalar, encoding);
        at += scalar.len_utf8();
    }
    (counted == character).then_some(at)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The outbound half. Note what is *not* asserted: that `encode` clamps.
    /// An offset that is not a character boundary — including one past the end
    /// of the document — has no wire position, and saying so is the whole
    /// difference between this and calling `rope` directly.
    #[test]
    fn encode_agrees_with_the_reference_or_refuses(
        (text, offset) in text().prop_flat_map(|text| {
            let past_the_end = text.len() + 3;
            (Just(text), 0..=past_the_end)
        }),
        encoding in encoding(),
    ) {
        let rope = Rope::from(text.as_str());
        let boundary = text.is_char_boundary(offset);

        match WirePosition::encode(Offset(offset), encoding, &rope) {
            Ok(position) => {
                prop_assert!(boundary, "encoded {offset}, which is not a character boundary");
                let (line, character) = reference_encode(&text, offset, encoding);
                prop_assert_eq!(position, wire(line, character));
            }
            Err(_) => prop_assert!(!boundary, "refused {offset}, which is a character boundary"),
        }
    }

    /// The inbound half, and the one that carries the claim. The generated
    /// `character` is deliberately not derived from a real position, so most
    /// cases are out of range, mid-surrogate, or on a line that does not
    /// exist — the cases where `rope` alone would have returned a plausible
    /// neighbouring offset.
    #[test]
    fn resolve_agrees_with_the_reference_or_refuses(
        text in text(),
        line in 0u32..6,
        character in 0u32..20,
        encoding in encoding(),
    ) {
        let rope = Rope::from(text.as_str());
        let expected = reference_resolve(&text, line, character, encoding);

        match (expected, wire(line, character).resolve(encoding, &rope)) {
            (Some(offset), Ok(resolved)) => prop_assert_eq!(resolved, Offset(offset)),
            (None, Err(_)) => {}
            (expected, resolved) => prop_assert!(
                false,
                "{line}:{character} in {encoding}: reference says {expected:?}, proto says \
                 {resolved:?}"
            ),
        }
    }

    /// Every boundary in the document, in every encoding, rather than one
    /// sampled offset per case: this is the round trip §10 asks for, and it is
    /// cheap enough to be exhaustive within the generated document.
    #[test]
    fn resolve_inverts_encode_at_every_boundary(text in text(), encoding in encoding()) {
        let rope = Rope::from(text.as_str());

        for offset in (0..=text.len()).filter(|&at| text.is_char_boundary(at)) {
            let offset = Offset(offset);
            let position = WirePosition::encode(offset, encoding, &rope)
                .expect("a character boundary always has a wire position");
            // `.ok()` throughout: `EncodingError` is not `PartialEq` and
            // should not become so — the assertion is about which offset came
            // back, and an error's identity is not part of any claim here.
            prop_assert_eq!(position.resolve(encoding, &rope).ok(), Some(offset));
        }
    }

    /// The encoding bug this section exists to prevent is "invisible on ASCII,
    /// wrong by a few columns on any line containing a non-ASCII character"
    /// (`core.md` §3). These two properties are the two halves of that
    /// sentence: on ASCII the three encodings must be indistinguishable, and
    /// on an astral scalar they must not be.
    #[test]
    fn ascii_positions_are_the_same_in_every_encoding(text in ascii_text()) {
        let rope = Rope::from(text.as_str());

        for offset in 0..=text.len() {
            let offset = Offset(offset);
            let utf8 = WirePosition::encode(offset, PositionEncoding::Utf8, &rope).ok();
            prop_assert_eq!(
                utf8,
                WirePosition::encode(offset, PositionEncoding::Utf16, &rope).ok()
            );
            prop_assert_eq!(
                utf8,
                WirePosition::encode(offset, PositionEncoding::Utf32, &rope).ok()
            );
        }
    }
}

/// The other half of §3's sentence, spelled out on the one scalar where all
/// three encodings disagree. A worked example rather than a property, because
/// the numbers are the documentation: `😀` is 4 bytes, 2 UTF-16 code units and
/// 1 scalar value, so the position just after it is column 4, 2 or 1 depending
/// on a negotiation neither this file nor a handler gets a vote in.
#[test]
fn an_astral_scalar_is_a_different_column_in_every_encoding() {
    let rope = Rope::from("😀x");
    let after = Offset(4);

    assert_eq!(
        WirePosition::encode(after, PositionEncoding::Utf8, &rope).ok(),
        Some(wire(0, 4))
    );
    assert_eq!(
        WirePosition::encode(after, PositionEncoding::Utf16, &rope).ok(),
        Some(wire(0, 2))
    );
    assert_eq!(
        WirePosition::encode(after, PositionEncoding::Utf32, &rope).ok(),
        Some(wire(0, 1))
    );

    // Those three columns all name offset 4. Read in the wrong encoding they
    // name somewhere else, and this is the shape of the bug §3 is about: two
    // of the six misreadings are refused for landing inside the scalar, and
    // the third is refused for running off the end of the line.
    assert!(wire(0, 1).resolve(PositionEncoding::Utf16, &rope).is_err());
    assert!(wire(0, 2).resolve(PositionEncoding::Utf8, &rope).is_err());
    assert!(wire(0, 4).resolve(PositionEncoding::Utf16, &rope).is_err());

    // The one misreading that is *not* refused, kept here because it is the
    // honest limit of what a type can do: column 2 in UTF-32 is a real
    // position, so reading a UTF-16 column as UTF-32 answers about offset 5
    // rather than 4. Nothing detects that but the negotiation being settled
    // once, which is why §3 insists it never be inferred.
    assert_eq!(
        wire(0, 2).resolve(PositionEncoding::Utf32, &rope).ok(),
        Some(Offset(5))
    );
}
