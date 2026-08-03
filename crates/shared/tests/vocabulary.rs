//! `design/core.md` §1's closing sentence about the vocabulary types — "these
//! are the *deserialization targets*, not wrappers applied after the fact" —
//! and §8.1's argument that this is the whole reason `lsp-types` is not a
//! dependency.
//!
//! The load-bearing assertion is not any single `assert_eq!`: it is that the
//! projections below name `DocumentUri`, `DocumentVersion` and
//! `EditorRequestId` as *field types* and deserialize straight from a frame.
//! A conversion layer reappearing between the wire and the vocabulary means
//! those fields become `String` and `i32` again, and this file stops
//! compiling — which is the difference between the discipline being a compiler
//! property and being a convention maintained by attention.
//!
//! `EditorRequestId`'s normalization is asserted through `as_str`, because the
//! claim it carries is about two *producers* agreeing (`shim.md` §3.1's
//! bounded scanner and the `serde_json` fallback), and the scanner does not
//! exist yet. Pinning the text is what lets it be written against a fact
//! rather than against this file's author's memory.

use serde::Deserialize;
use shared::{DocumentUri, DocumentVersion, EditorRequestId};

/// The fields of `textDocument/definition`'s params and of `didOpen`'s
/// `textDocument` that `core` actually reads (`core.md` §8.2's inventory),
/// with the request envelope's `id` alongside them.
#[derive(Deserialize)]
struct DefinitionRequest {
    id: EditorRequestId,
    params: DefinitionParams,
}

#[derive(Deserialize)]
struct DefinitionParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
}

#[derive(Deserialize)]
struct TextDocumentIdentifier {
    uri: DocumentUri,
    version: DocumentVersion,
}

#[test]
fn a_frame_deserializes_into_the_newtypes_with_no_conversion_layer() {
    let frame = r#"{
        "jsonrpc": "2.0",
        "id": 7,
        "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": "file:///project/src/main.rs", "version": 3 },
            "position": { "line": 0, "character": 0 }
        }
    }"#;

    let request: DefinitionRequest = serde_json::from_str(frame).unwrap();

    assert_eq!(request.id, EditorRequestId::from_number(7));
    assert_eq!(
        request.params.text_document.uri,
        DocumentUri::parse("file:///project/src/main.rs").unwrap()
    );
    assert_eq!(request.params.text_document.version, DocumentVersion(3));
}

/// The half of the sentence that says *when* normalization happens. `Url`
/// removes dot segments and lowercases the scheme; the point is that it has
/// already happened by the time a `DocumentUri` exists, so there is no
/// unnormalized form for anything downstream to hold or to compare against.
#[test]
fn a_uri_is_normalized_during_deserialization_rather_than_after_it() {
    let unnormalized: DocumentUri =
        serde_json::from_str(r#""FILE:///project/./src/../src/main.rs""#).unwrap();

    assert_eq!(
        unnormalized,
        DocumentUri::parse("file:///project/src/main.rs").unwrap()
    );
    assert_eq!(unnormalized.as_str(), "file:///project/src/main.rs");
}

#[test]
fn a_uri_that_is_not_a_uri_is_refused_rather_than_carried() {
    let refused = serde_json::from_str::<DocumentUri>(r#""../src/main.rs""#);

    assert!(refused.is_err(), "a relative path is not a URI");
}

/// The kind is part of the key. Both of these are legal LSP ids and they are
/// different requests; a normalization that dropped to the decoded content
/// would collide them in `core`'s pending map (`shim.md` §7).
#[test]
fn a_number_id_and_a_string_id_spelling_the_same_digits_are_different_keys() {
    let number: EditorRequestId = serde_json::from_str("42").unwrap();
    let string: EditorRequestId = serde_json::from_str(r#""42""#).unwrap();

    assert_ne!(number, string);
    assert_eq!(number.as_str(), "42");
    assert_eq!(string.as_str(), r#""42""#);
}

/// The other direction: two spellings of one id are one key. This is the case
/// §3.1's scanner declines on, so the fallback path has to reach the same text
/// the scanner would have produced from the unescaped form.
#[test]
fn two_spellings_of_one_string_id_normalize_to_one_key() {
    // JSON permits an escaped solidus and gives it no separate meaning, so
    // this is the id `a/b`, spelled the other way.
    let escaped: EditorRequestId = serde_json::from_str("\"a\\/b\"").unwrap();

    assert_eq!(escaped, EditorRequestId::from_string("a/b"));
    assert_eq!(escaped.as_str(), "\"a/b\"");
}

/// Requoting uses JSON's mandatory escape set and nothing wider, so the stored
/// text is what `serde_json` would have written and §8.2's response envelope
/// can echo it as bytes rather than re-encoding it.
#[test]
fn a_string_id_is_requoted_by_json_rules() {
    let awkward = EditorRequestId::from_string("a\"b\\c\nd\te");

    assert_eq!(awkward.as_str(), "\"a\\\"b\\\\c\\nd\\te\"");
    assert_eq!(
        serde_json::from_str::<EditorRequestId>(awkward.as_str()).unwrap(),
        awkward,
        "the stored text is itself a JSON id, and parses back to the same key"
    );

    // The four-hex-digit branch, asserted by the round trip rather than by
    // pinning the text: a raw control character inside a JSON string is
    // invalid, so an id that parses back is one that was escaped.
    let control = EditorRequestId::from_string(&format!("a{}b", char::from(1u8)));

    assert_eq!(
        serde_json::from_str::<EditorRequestId>(control.as_str()).unwrap(),
        control
    );
}

/// Fail closed on the shapes the protocol does not define an id to be
/// (`core.md` §8.6). A fraction is the one that matters: §3.1's scanner
/// declines on it, so an id keyed from one could never be matched by the fast
/// path — the two producers would disagree exactly where the design says they
/// must not.
#[test]
fn an_id_that_is_neither_an_integer_nor_a_string_is_refused() {
    for shape in ["1.5", "null", "true", "[1]", "{}"] {
        let refused = serde_json::from_str::<EditorRequestId>(shape);

        assert!(refused.is_err(), "{shape} is not a JSON-RPC request id");
    }
}

/// Above `i64::MAX` still keys, because the text is unambiguous and refusing
/// would be a limit LSP does not have.
#[test]
fn a_large_integer_id_keys_by_its_digits() {
    let large: EditorRequestId = serde_json::from_str("18446744073709551615").unwrap();

    assert_eq!(large.as_str(), "18446744073709551615");
    assert_ne!(large, EditorRequestId::from_string("18446744073709551615"));
}

/// A negative id is legal JSON and the sign is part of the text, so it cannot
/// collide with the positive one.
#[test]
fn a_negative_id_keeps_its_sign() {
    let negative: EditorRequestId = serde_json::from_str("-7").unwrap();

    assert_eq!(negative.as_str(), "-7");
    assert_ne!(negative, EditorRequestId::from_number(7));
}
