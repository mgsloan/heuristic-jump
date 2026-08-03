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
    CancelParams, ContentChange, DefinitionParams, DefinitionProvider, DefinitionResult,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, ErrorCode, InitializeParams,
    InitializeResult, MessageType, PositionEncoding, ProgressParams, ProgressToken, Response,
    ResponseError, ShowMessageParams, StandaloneInitializeResult, StandaloneServerCapabilities,
    StandaloneServerInfo, TextDocumentSync, TextDocumentSyncKind, WireLocation, WirePosition,
    WireRange,
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

/// The value types that genuinely travel in both directions: a position and a
/// range arrive in a request and leave in a response, a location arrives from
/// the oracle and leaves as our answer, and the negotiated encoding is read
/// from a child and written by standalone. This list is short on purpose —
/// every entry is a type §8.2's read-only rule does not cover, so a fifth one
/// appearing is a claim someone should have to make deliberately.
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
        rope::ByteOffset(21)
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
    let start = WirePosition::encode(rope::ByteOffset(3), PositionEncoding::Utf16, &rope).unwrap();
    let end = WirePosition::encode(rope::ByteOffset(7), PositionEncoding::Utf16, &rope).unwrap();
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
