//! `design/core.md` §8.2's inventory — "roughly thirty small structs" — and
//! the two properties that make hand-writing them safer than taking
//! `lsp-types`, rather than merely smaller.
//!
//! **Why part of this reads the module's own source.** The claims §8.2 makes
//! are about *absence*: the incoming projections are read-only, so they must
//! not implement `Serialize`; untagged is permitted only on the unions §8.5
//! clears, so it must not appear anywhere else. Neither can be asserted by
//! calling anything — a `Serialize` impl that should not exist is invisible
//! to every test that does not use it, and a `#[serde(untagged)]` added to a
//! sixth enum compiles and passes every positive test. So the derive
//! discipline is checked against the text of `proto.rs`, which is the same
//! move `driver/tests/seam.rs` makes against a manifest and for the same
//! reason: what is being asserted is that a line is *not* there.
//!
//! The inventory list below is therefore also the §8.2 table, transcribed. A
//! type added to `proto` without a decision about which direction it travels
//! in fails here.
//!
//! The rest is behaviour, and it concentrates on the two places §8.5 and §8.6
//! say the money is: `contentChanges`, where getting it wrong replaces the
//! document with the characters just typed, and the definition-result union,
//! where the shapes have to discriminate on required fields rather than on
//! declaration order.

use std::marker::PhantomData;
use std::path::Path;

use serde::Serialize;
use serde_json::json;
use shared::proto::{
    CancelParams, ContentChange, DefinitionOptions, DefinitionParams, DefinitionProvider,
    DefinitionResult, DidChangeTextDocumentParams, DidOpenTextDocumentParams, ErrorCode,
    InitializeParams, InitializeResult, MessageType, PositionEncoding, ProgressParams,
    ProgressToken, Response, ResponseError, ShowMessageParams, StandaloneInitializeResult,
    StandaloneServerCapabilities, StandaloneServerInfo, TextDocumentSync, TextDocumentSyncKind,
    TextDocumentSyncOptions, WireLocation, WireLocationLink, WirePosition, WireRange,
};
use shared::{DocumentUri, EditorRequestId, Rope};

/// The Read half of §8.2's table: deserialized, never written back.
const READ: &[&str] = &[
    "CancelParams",
    "ChildFrame",
    "ChildResponseError",
    "ClientCapabilities",
    "ClientInfo",
    "ContentChange",
    "DefinitionClientCapabilities",
    "DefinitionOptions",
    "DefinitionParams",
    "DefinitionProvider",
    "DefinitionResult",
    "DidChangeTextDocumentParams",
    "DidCloseTextDocumentParams",
    "DidOpenTextDocumentParams",
    "DidSaveTextDocumentParams",
    "GeneralClientCapabilities",
    "InitializeParams",
    "InitializeResult",
    "NotifiedDocument",
    "ProgressParams",
    "ProgressToken",
    "ServerCapabilities",
    "ServerInfo",
    "ShowDocumentClientCapabilities",
    "ShowMessageRequestClientCapabilities",
    "TextDocumentClientCapabilities",
    "TextDocumentIdentifier",
    "TextDocumentItem",
    "TextDocumentSync",
    "TextDocumentSyncOptions",
    "VersionedTextDocumentIdentifier",
    "WindowClientCapabilities",
    "WireLocationLink",
    "WorkspaceFolder",
];

/// The Construct half: serialized, never read. Standalone's
/// `InitializeResult` is a different type from the one we read from a child
/// precisely so that this list and the one above can be disjoint.
///
/// The `Client*` block is `measure_core`'s outgoing half — the corpus scan is
/// a plain LSP client, so it constructs what the shim reads. Each has a read
/// twin two lists up, and they are separate types for the same reason
/// `StandaloneInitializeResult` is: a projection that can be written back is
/// the round trip §8.2 removes.
const CONSTRUCT: &[&str] = &[
    "ClientDefinitionCapabilities",
    "ClientDefinitionParams",
    "ClientDidCloseParams",
    "ClientDidOpenParams",
    "ClientGeneralCapabilities",
    "ClientIdentity",
    "ClientInitializeParams",
    "ClientNotification",
    "ClientReply",
    "ClientOfferedCapabilities",
    "ClientRequest",
    "ClientTextDocumentCapabilities",
    "ClientTextDocumentIdentifier",
    "ClientTextDocumentItem",
    "ClientWorkspaceFolder",
    "ErrorCode",
    "JsonRpcVersion",
    "MessageActionItem",
    "MessageType",
    "Response",
    "ResponseError",
    "ResponseOutcome",
    "ShowDocumentParams",
    "ShowMessageParams",
    "ShowMessageRequestParams",
    "StandaloneInitializeResult",
    "StandaloneServerCapabilities",
    "StandaloneServerInfo",
];

/// §8.2's third table: the value types that travel in both directions. A
/// position and a range arrive in a request and leave in a response, a
/// location arrives from the oracle and leaves as our answer, and the
/// negotiated encoding is read from a child and written by standalone.
///
/// The list is short because §8.2 bounds it, not because nothing has been
/// added yet: "nothing is ever round-tripped" is a claim about messages, and a
/// value type in both directions does not round-trip anything — an inbound
/// position is resolved to an offset and dropped, an outbound one is built by
/// `encode`. A sixth entry is therefore a claim someone has to make
/// deliberately, here and in §8.2 (CHANGE-core-008).
const BOTH: &[&str] = &[
    "PositionEncoding",
    "TextDocumentSyncKind",
    "WireLocation",
    "WirePosition",
    "WireRange",
];

/// §8.5 clears exactly these for untagged: each pair of variants is disjoint
/// by JSON kind, or — for the definition result — by a required field the
/// others lack. `contentChanges` is deliberately absent.
const UNTAGGED: &[&str] = &[
    "DefinitionProvider",
    "DefinitionResult",
    "ProgressToken",
    "TextDocumentSync",
];

struct Declared {
    name: String,
    deserializes: bool,
    serializes: bool,
    untagged: bool,
}

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/proto.rs");
    std::fs::read_to_string(&path).unwrap_or_default()
}

/// Attribute-block scanning rather than a parser: derives are one line each in
/// this module, and rustfmt keeps them that way.
fn declared(source: &str) -> Vec<Declared> {
    let mut derives: Vec<String> = Vec::new();
    let mut untagged = false;
    let mut found = Vec::new();

    for line in source.lines() {
        let line = line.trim();
        if let Some(list) = line
            .strip_prefix("#[derive(")
            .and_then(|rest| rest.strip_suffix(")]"))
        {
            derives = list.split(',').map(|name| name.trim().to_owned()).collect();
            continue;
        }
        if line.starts_with("#[serde(") {
            untagged = untagged || line.contains("untagged");
            continue;
        }
        if line.starts_with("///") || line.starts_with("//") || line.starts_with("#[") {
            continue;
        }
        if let Some(head) = line
            .strip_prefix("pub struct ")
            .or_else(|| line.strip_prefix("pub enum "))
        {
            let name = head.split(['<', ' ', '{', '(', ';']).next().unwrap_or("");
            found.push(Declared {
                name: name.to_owned(),
                deserializes: derives.iter().any(|derive| derive == "Deserialize")
                    || source.contains(&format!("impl<'de> Deserialize<'de> for {name} ")),
                serializes: derives.iter().any(|derive| derive == "Serialize")
                    || source.contains(&format!("impl Serialize for {name} ")),
                untagged,
            });
        }
        derives = Vec::new();
        untagged = false;
    }
    found
}

#[test]
fn the_inventory_is_the_one_section_82_lists() {
    let source = source();
    let declared = declared(&source);
    let mut names: Vec<&str> = declared.iter().map(|item| item.name.as_str()).collect();
    names.sort_unstable();

    let mut expected: Vec<&str> = READ.iter().chain(CONSTRUCT).chain(BOTH).copied().collect();
    expected.sort_unstable();

    assert_eq!(
        names, expected,
        "shared::proto's public types and core.md §8.2's inventory disagree. A \
         new wire type belongs in READ, CONSTRUCT or BOTH here, and which one \
         is the decision this file exists to make explicit"
    );
}

#[test]
fn read_projections_are_never_serialized() {
    let source = source();
    for item in declared(&source) {
        if READ.contains(&item.name.as_str()) {
            assert!(
                item.deserializes,
                "{} is listed as a read projection but has no Deserialize",
                item.name
            );
            assert!(
                !item.serializes,
                "{} is a read projection and implements Serialize. §8.2's \
                 forward compatibility rests on nothing writing an incoming \
                 message back: a field we did not model cannot be lost only \
                 while that is true",
                item.name
            );
        }
        if CONSTRUCT.contains(&item.name.as_str()) {
            assert!(
                item.serializes,
                "{} is listed as constructed but has no Serialize",
                item.name
            );
            assert!(
                !item.deserializes,
                "{} is constructed and also deserializes, so it round-trips — \
                 which is the shape §8.2 removes",
                item.name
            );
        }
    }
}

#[test]
fn untagged_appears_only_where_section_85_permits_it() {
    let source = source();
    for item in declared(&source) {
        assert_eq!(
            item.untagged,
            UNTAGGED.contains(&item.name.as_str()),
            "{}'s use of #[serde(untagged)] disagrees with §8.5's rule: \
             untagged is permitted only when the variants are disjoint by \
             JSON kind or by a required field the others lack, never by an \
             optional field and never by declaration order",
            item.name
        );
    }
}

/// §8.3, which is a claim about *absence* and so is checked the way §8.2's
/// derive discipline is: against the text of `proto.rs`.
///
/// The section allows one door per unit. `character` is in the negotiated
/// encoding and has none — it is reachable only through `resolve`, which takes
/// the encoding and the document. `line` is in no encoding, so it has an
/// accessor, and §6's predicate needs it: the child's row arrives only inside a
/// `WirePosition`, and recovering it any other way means reading the target
/// document, which §6 forbids in the same paragraph that requires the
/// comparison (CHANGE-core-007).
///
/// So the surface is exactly three functions, and a fourth is a decision
/// somebody has to make here rather than a line added to an `impl`. A
/// `character()` accessor in particular would restore the failure §3 exists to
/// prevent, and nothing that merely *used* `WirePosition` would notice.
#[test]
fn the_wire_position_has_exactly_one_door_per_unit() {
    let source = source();
    let block = source
        .split_once("impl WirePosition {")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map(|(block, _)| block.to_owned())
        .expect("proto.rs declares one `impl WirePosition` block");

    let mut doors: Vec<&str> = block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub fn "))
        .filter_map(|rest| rest.split(['(', '<']).next())
        .collect();
    doors.sort_unstable();

    assert_eq!(
        doors,
        ["encode", "line", "resolve"],
        "§8.3 gives a wire position one door per unit: `resolve` and `encode` \
         for the column, which take the encoding and the text, and `line` for \
         the row, which is in no encoding. Anything else here hands out a \
         number whose unit the caller has to remember, which is the failure \
         §3 exists to prevent"
    );
}

#[test]
fn initialize_params_project_the_fields_the_shim_reads() {
    let params: InitializeParams = serde_json::from_value(json!({
        "processId": 4242,
        "rootUri": "file:///work/repo",
        "workspaceFolders": [{"uri": "file:///work/repo", "name": "repo"}],
        "clientInfo": {"name": "Zed", "version": "0.1"},
        "capabilities": {
            "window": {"showDocument": {"support": true}, "showMessage": {}},
            "general": {"positionEncodings": ["utf-8", "utf-16", "utf-7"]},
            "textDocument": {"definition": {"linkSupport": true}}
        },
        "initializationOptions": {"anything": [1, 2, 3]}
    }))
    .unwrap();

    assert_eq!(
        params.root_uri.map(|uri| uri.to_string()),
        Some("file:///work/repo".to_owned())
    );
    assert_eq!(
        params.workspace_folders.map(|folders| folders.len()),
        Some(1)
    );
    assert_eq!(params.client_info.map(|info| info.name), Some("Zed".into()));

    let window = params.capabilities.window.unwrap();
    assert!(window.show_document.unwrap().support);
    assert!(window.show_message.is_some());
    assert!(
        params
            .capabilities
            .text_document
            .and_then(|text_document| text_document.definition)
            .and_then(|definition| definition.link_support)
            .unwrap()
    );

    // `utf-7` is not a kind we implement, and dropping it is not the same as
    // failing `initialize` over it: this field is a menu the editor offers.
    let general = params.capabilities.general.unwrap();
    assert_eq!(
        general.position_encodings,
        vec![PositionEncoding::Utf8, PositionEncoding::Utf16]
    );
}

#[test]
fn a_projection_ignores_every_field_it_did_not_model() {
    let params: DefinitionParams = serde_json::from_value(json!({
        "textDocument": {"uri": "file:///work/repo/src/main.rs", "unmodelled": {"a": 1}},
        "position": {"line": 3, "character": 7},
        "workDoneToken": "token-we-do-not-read",
        "partialResultToken": 9
    }))
    .unwrap();

    assert_eq!(
        params.text_document.uri.as_str(),
        "file:///work/repo/src/main.rs"
    );
    let rope = Rope::from("one\ntwo\nthree\nfour five six\n");
    assert_eq!(
        params
            .position
            .resolve(PositionEncoding::Utf16, &rope)
            .unwrap(),
        rope::Offset(21)
    );
}

#[test]
fn an_incremental_change_is_never_read_as_a_full_one() {
    let params: DidChangeTextDocumentParams = serde_json::from_value(json!({
        "textDocument": {"uri": "file:///work/repo/src/main.rs", "version": 7},
        "contentChanges": [
            {
                "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 2}},
                "rangeLength": 2,
                "text": "hi"
            },
            {"text": "the whole document"}
        ]
    }))
    .unwrap();

    assert_eq!(params.text_document.version, shared::DocumentVersion(7));
    let ContentChange::Incremental { range, text } = &params.content_changes[0] else {
        panic!(
            "an incremental change read as a full one replaces the document \
             with the characters just typed, which is §8.5's worst case"
        );
    };
    assert_eq!(&**text, "hi");
    assert_eq!(range.start, range.start);
    let ContentChange::Full { text } = &params.content_changes[1] else {
        panic!("a change with no range is the full-document form");
    };
    assert_eq!(&**text, "the whole document");
}

#[test]
fn a_null_range_is_refused_rather_than_read_as_a_full_change() {
    let refused = serde_json::from_value::<ContentChange>(json!({
        "range": null,
        "text": "hi"
    }));

    assert!(
        refused.is_err(),
        "`range: null` is not a shape LSP defines, and the direction to fail \
         in is the one that does not silently discard the document (§8.6)"
    );
}

/// The other direction on §8.5's fifth union: `text` is required in *both*
/// shapes, so a change carrying only a range is neither and must not become a
/// `Full` with an empty document.
#[test]
fn a_change_with_no_text_is_neither_shape() {
    let refused = serde_json::from_value::<ContentChange>(json!({
        "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 2}}
    }));

    assert!(
        refused.is_err(),
        "a change with no text was read as one, and the only text it could \
         have is none — which is the whole document deleted"
    );
}

#[test]
fn the_definition_result_union_discriminates_on_required_fields() {
    let location = json!({
        "uri": "file:///work/repo/src/main.rs",
        "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 4}}
    });
    let link = json!({
        "targetUri": "file:///work/repo/src/main.rs",
        "targetRange": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 4}},
        "targetSelectionRange": {
            "start": {"line": 1, "character": 0},
            "end": {"line": 1, "character": 4}
        }
    });

    let one: DefinitionResult = serde_json::from_value(location.clone()).unwrap();
    assert!(matches!(one, DefinitionResult::One(_)));

    let many: DefinitionResult = serde_json::from_value(json!([location])).unwrap();
    assert!(matches!(many, DefinitionResult::Many(found) if found.len() == 1));

    let links: DefinitionResult = serde_json::from_value(json!([link])).unwrap();
    assert!(matches!(links, DefinitionResult::Links(found) if found.len() == 1));

    let null: DefinitionResult = serde_json::from_value(json!(null)).unwrap();
    assert!(matches!(null, DefinitionResult::Null));

    // The one residual ambiguity §8.5 names. Both array variants accept it and
    // both mean "no definitions"; this pins which, so that a later reader
    // finds a decision rather than an accident.
    let empty: DefinitionResult = serde_json::from_value(json!([])).unwrap();
    assert!(matches!(empty, DefinitionResult::Many(found) if found.is_empty()));

    // The negative half, which is the half that makes the positive one mean
    // anything. §8.5 permits untagged here *because* each shape fails the
    // others' deserialization on a missing required field — so the assertion
    // is against the variant payloads directly, where a reordering of
    // `DefinitionResult` cannot hide the answer.
    assert!(
        serde_json::from_value::<WireLocation>(link.clone()).is_err(),
        "a LocationLink parsed as a Location, so the union discriminates by \
         declaration order and a reordering silently changes what we read"
    );
    assert!(
        serde_json::from_value::<WireLocationLink>(location.clone()).is_err(),
        "a Location parsed as a LocationLink, so `targetUri` is not the required \
         field §8.5 relies on"
    );
    assert!(serde_json::from_value::<Vec<WireLocation>>(json!([link])).is_err());
    assert!(serde_json::from_value::<Vec<WireLocationLink>>(json!([location])).is_err());
}

/// The two capability unions §8.5 clears as "disjoint by JSON kind", asserted
/// as the kind rather than as the order: each variant's payload is fed the
/// other's shape and has to refuse it.
///
/// Without this, `definitionProvider` and `textDocumentSync` read correctly
/// today for the reason §8.5 says is not acceptable — the variant that happens
/// to be declared first is the one that happens to fit.
#[test]
fn the_capability_unions_refuse_each_others_shapes() {
    let options = json!({"workDoneProgress": true});
    assert!(
        serde_json::from_value::<bool>(options).is_err(),
        "an options object read as a boolean would make an unsupported server \
         look like a supporting one, or the reverse"
    );
    assert!(serde_json::from_value::<DefinitionOptions>(json!(false)).is_err());

    assert!(
        serde_json::from_value::<TextDocumentSyncKind>(json!({"openClose": true})).is_err(),
        "an options object read as a sync kind decides whether contentChanges \
         carry ranges at all"
    );
    assert!(serde_json::from_value::<TextDocumentSyncOptions>(json!(2)).is_err());
}

/// §8.5's first union, in the one place it is modelled as an enum rather than
/// normalized into text. A progress token is a number or a string, and the
/// two must not be readable as each other — a token we match against one we
/// minted is useless if `7` and `"7"` can both land in the same variant.
#[test]
fn the_progress_token_union_is_disjoint_by_json_kind() {
    let number: ProgressToken = serde_json::from_value(json!(7)).unwrap();
    assert!(matches!(number, ProgressToken::Number(7)));

    let text: ProgressToken = serde_json::from_value(json!("7")).unwrap();
    assert!(matches!(text, ProgressToken::Text(token) if &*token == "7"));

    assert!(serde_json::from_value::<Box<str>>(json!(7)).is_err());
    assert!(serde_json::from_value::<i64>(json!("7")).is_err());
}

#[test]
fn absent_and_false_stay_distinguishable_in_server_capabilities() {
    let silent: InitializeResult = serde_json::from_value(json!({"capabilities": {}})).unwrap();
    assert!(silent.capabilities.definition_provider.is_none());
    assert!(silent.capabilities.position_encoding.is_none());

    let refuses: InitializeResult =
        serde_json::from_value(json!({"capabilities": {"definitionProvider": false}})).unwrap();
    assert!(matches!(
        refuses.capabilities.definition_provider,
        Some(DefinitionProvider::Supported(false))
    ));

    let options: InitializeResult = serde_json::from_value(
        json!({"capabilities": {"definitionProvider": {"workDoneProgress": true}}}),
    )
    .unwrap();
    assert!(matches!(
        options.capabilities.definition_provider,
        Some(DefinitionProvider::Options(_))
    ));
}

/// §8.6's fail-closed rule at the one field where "lenient" and "closed" point
/// in opposite directions on the same JSON string.
///
/// A *client* offering an encoding we do not model has it dropped: that field
/// is a menu, and refusing `initialize` over an entry we would never have
/// chosen would be a modelling error failing open in the other direction. A
/// *server* naming one is different — it is the negotiated value, every
/// position on the wire is in it, and there is no reading of a position we
/// cannot convert. So it takes the whole `InitializeResult` down with it, and
/// that is deliberate: the fields beside it say what the server can do, and
/// acting on them while unable to read a single position is worse than having
/// no answer.
#[test]
fn a_negotiated_encoding_we_cannot_honour_fails_the_whole_initialize_result() {
    let offered: InitializeParams = serde_json::from_value(json!({
        "rootUri": "file:///work/repo",
        "capabilities": {"general": {"positionEncodings": ["utf-64", "utf-16"]}}
    }))
    .unwrap();
    assert_eq!(
        offered.capabilities.general.unwrap().position_encodings,
        vec![PositionEncoding::Utf16],
        "a client's list is a menu, and an entry we would not have chosen is dropped"
    );

    let refused = serde_json::from_value::<InitializeResult>(json!({
        "capabilities": {"positionEncoding": "utf-64", "definitionProvider": true}
    }));
    assert!(
        refused.is_err(),
        "a server named an encoding we cannot convert and the result was still read. Every \
         position after this point is in that encoding, so `definitionProvider: true` beside \
         it is an invitation to answer about the wrong place"
    );
}

/// The three `PositionEncodingKind` strings, written twice: once by `serde`'s
/// renames and once by `Display`. They have to agree, because one is what
/// standalone advertises and the other is what a mismatch is reported in — and
/// a report naming an encoding nobody negotiated sends the reader after the
/// wrong thing.
#[test]
fn an_encoding_is_spelled_the_same_by_serde_and_by_display() {
    for encoding in [
        PositionEncoding::Utf8,
        PositionEncoding::Utf16,
        PositionEncoding::Utf32,
    ] {
        assert_eq!(
            serde_json::to_value(encoding).unwrap(),
            json!(encoding.to_string()),
            "the wire spelling and the reported spelling of {encoding} have drifted"
        );
        assert_eq!(
            serde_json::from_value::<PositionEncoding>(json!(encoding.to_string())).unwrap(),
            encoding
        );
    }
}

#[test]
fn the_sync_union_reads_both_shapes_and_refuses_a_fourth_kind() {
    let integer: InitializeResult =
        serde_json::from_value(json!({"capabilities": {"textDocumentSync": 2}})).unwrap();
    assert!(matches!(
        integer.capabilities.text_document_sync,
        Some(TextDocumentSync::Kind(TextDocumentSyncKind::Incremental))
    ));

    let options: InitializeResult = serde_json::from_value(
        json!({"capabilities": {"textDocumentSync": {"openClose": true, "change": 1}}}),
    )
    .unwrap();
    let Some(TextDocumentSync::Options(options)) = options.capabilities.text_document_sync else {
        panic!("an object textDocumentSync is the options shape");
    };
    assert_eq!(options.change, Some(TextDocumentSyncKind::Full));

    let unknown = serde_json::from_value::<InitializeResult>(
        json!({"capabilities": {"textDocumentSync": 9}}),
    );
    assert!(
        unknown.is_err(),
        "a sync kind outside 0..=2 decides whether contentChanges carry \
         ranges at all, so guessing is the failure §8.6 forbids"
    );
}

#[test]
fn a_document_open_names_the_vocabulary_types_and_leaves_the_language_id_to_the_registry() {
    let params: DidOpenTextDocumentParams = serde_json::from_value(json!({
        "textDocument": {
            "uri": "file:///work/repo/src/main.rs",
            "languageId": "rust",
            "version": 1,
            "text": "fn main() {}\n"
        }
    }))
    .unwrap();

    let document = params.text_document;
    assert_eq!(
        document.uri,
        DocumentUri::parse("file:///work/repo/src/main.rs").unwrap()
    );
    assert_eq!(document.version, shared::DocumentVersion(1));
    // A `LanguageId` is interned and only ids a registered handler declared
    // exist, so this stays a string until the registry resolves it (§1).
    assert_eq!(&*document.language_id, "rust");
}

#[test]
fn a_cancel_and_a_progress_notification_keep_their_two_id_spaces_apart() {
    let cancel: CancelParams = serde_json::from_value(json!({"id": 42})).unwrap();
    assert_eq!(cancel.id, EditorRequestId::from_number(42));

    let string: CancelParams = serde_json::from_value(json!({"id": "42"})).unwrap();
    assert_ne!(
        string.id, cancel.id,
        "a number id and a string id spelling the same digits are different \
         requests, which is why EditorRequestId stores the JSON text"
    );

    let progress: ProgressParams = serde_json::from_value(json!({
        "token": "hj-progress-1",
        "value": {"kind": "begin", "title": "indexing", "percentage": 10}
    }))
    .unwrap();
    assert_eq!(progress.token, ProgressToken::Text("hj-progress-1".into()));
    // Left raw: what the value means is the adapter's question, and the shim
    // has no reason to have an opinion about a shape it never inspects.
    assert!(progress.value.get().contains("\"kind\":\"begin\""));

    let numeric: ProgressParams =
        serde_json::from_value(json!({"token": 7, "value": null})).unwrap();
    assert_eq!(numeric.token, ProgressToken::Number(7));
}

#[test]
fn a_response_echoes_the_id_in_the_kind_it_arrived_in() {
    let rope = Rope::from("fn main() {}\n");
    let uri = DocumentUri::parse("file:///work/repo/src/main.rs").unwrap();
    let start = WirePosition::encode(rope::Offset(3), PositionEncoding::Utf16, &rope).unwrap();
    let end = WirePosition::encode(rope::Offset(7), PositionEncoding::Utf16, &rope).unwrap();
    let location = WireLocation::new(uri, WireRange { start, end });

    let answer = Response::result(EditorRequestId::from_number(7), vec![location]);
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": [{
                "uri": "file:///work/repo/src/main.rs",
                "range": {
                    "start": {"line": 0, "character": 3},
                    "end": {"line": 0, "character": 7}
                }
            }]
        })
    );

    let failure = Response::<Vec<WireLocation>>::failure(
        EditorRequestId::from_string("7"),
        ResponseError {
            code: ErrorCode::REQUEST_CANCELLED,
            message: "cancelled".into(),
        },
    );
    assert_eq!(
        serde_json::to_value(&failure).unwrap(),
        json!({
            "jsonrpc": "2.0",
            "id": "7",
            "error": {"code": -32800, "message": "cancelled"}
        })
    );
}

#[test]
fn the_constructed_set_writes_the_shapes_lsp_expects() {
    let message = ShowMessageParams {
        message_type: MessageType::Warning,
        message: "no definition found".into(),
    };
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        json!({"type": 2, "message": "no definition found"})
    );

    let standalone = StandaloneInitializeResult {
        capabilities: StandaloneServerCapabilities {
            position_encoding: PositionEncoding::Utf16,
            text_document_sync: TextDocumentSyncKind::Incremental,
            definition_provider: true,
        },
        server_info: StandaloneServerInfo {
            name: "heuristic-jump",
            version: "0.1.0",
        },
    };
    assert_eq!(
        serde_json::to_value(&standalone).unwrap(),
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": 2,
                "definitionProvider": true
            },
            "serverInfo": {"name": "heuristic-jump", "version": "0.1.0"}
        })
    );
}

/// The compile-time half of the read-only claim. The source scan is what
/// catches a projection *gaining* `Serialize`; this is the other direction —
/// naming a type that may be written, in a position where naming a read
/// projection would not compile.
#[test]
fn only_the_outbound_types_can_be_written() {
    fn writable<T: Serialize>() -> PhantomData<T> {
        PhantomData
    }

    let _outbound: PhantomData<WireLocation> = writable();
    let _standalone: PhantomData<StandaloneInitializeResult> = writable();
    // `let _: PhantomData<InitializeParams> = writable();` does not compile,
    // which is the property. An absence cannot be asserted, so it is written
    // here as the sentence a reader needs.
}
