//! `design/core.md` §10's third bullet and §8.5's second mitigation: every
//! message in the golden corpus deserialized with both `shared::proto` and
//! `lsp-types`, asserting the fields we model agree.
//!
//! This is the condition on which dropping `lsp-types` was acceptable, and
//! `deps.md` §3 says so in as many words: hand-rolled wire types are safe for
//! flat structs and go wrong on JSON unions, so the reason to keep the crate
//! at all is as a second reading of the same bytes.
//!
//! **What the corpus is, precisely.** §8.5 asks for traffic "captured from Zed
//! and VS Code against rust-analyzer, pyright, and gopls". These messages are
//! not that: they are hand-authored against `reference/lsp-3.17/metaModel.json`
//! and shaped like the servers they name. That is a weaker artifact and it is
//! weaker in a specific way — a hand-written corpus contains the fields
//! somebody thought of, which is exactly the population §8.6 warns is not the
//! long tail. Captured traffic remains open, and adding it is adding lines to
//! `golden-traffic.jsonl`: nothing below knows where a line came from.
//!
//! **What the oracle cannot see.** `WirePosition`'s `character` is private and
//! has no accessor, because §8.3 makes a wire position inert — resolvable only
//! when handed an encoding and a document. `lsp_types::Position.character` is
//! a bare `u32`, which is the exact defect §3 gives as the reason we do not
//! use it. So the two cannot be compared field to field, and comparing only
//! the line would leave the more error-prone number unchecked. They are
//! compared through [`GRID`] instead: an ASCII document of known geometry, in
//! which resolving a position yields a byte offset the column can be recovered
//! from exactly. The asymmetry is the point rather than an obstacle — the
//! oracle has to reach through the design to read a number the design will not
//! hand out.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and every comparison below is a free function. Failing loudly is the point: a corpus line one side cannot read is the finding, and swallowing it would leave the differential asserting agreement over the messages that happened to parse."
)]

use std::sync::LazyLock;

use lsp_types::{OneOf, TextDocumentSyncCapability};
use serde::Deserialize;
use serde_json::value::RawValue;
use shared::proto::{
    ContentChange, DefinitionProvider, DefinitionResult, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    InitializeParams, InitializeResult, PositionEncoding, ProgressParams, ProgressToken,
    TextDocumentSync, TextDocumentSyncKind, WirePosition, WireRange,
};
use shared::{Offset, Rope};

/// The corpus, one JSON message per line, `include_str!`'d rather than read so
/// that a corrupted corpus is a build failure and not a test that quietly
/// asserts over nothing.
const CORPUS: &str = include_str!("golden-traffic.jsonl");

#[derive(Deserialize, Debug)]
struct Entry {
    kind: Kind,
    /// Where the message came from, and `CAPTURED` when it came off a wire.
    /// Required rather than optional so a line cannot be added with no
    /// provenance, and read rather than decorative: §8.5 asks for captured
    /// traffic specifically, so which half a line is in is a property with a
    /// test.
    source: Box<str>,
    message: Box<RawValue>,
}

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum Kind {
    InitializeParams,
    InitializeResult,
    DefinitionResult,
    DidOpen,
    DidChange,
    DidSave,
    DidClose,
    Progress,
}

const KINDS: &[Kind] = &[
    Kind::InitializeParams,
    Kind::InitializeResult,
    Kind::DefinitionResult,
    Kind::DidOpen,
    Kind::DidChange,
    Kind::DidSave,
    Kind::DidClose,
    Kind::Progress,
];

#[test]
fn every_message_in_the_corpus_reads_the_same_both_ways() {
    let corpus = corpus();
    for entry in &corpus {
        let message = entry.message.get();
        match entry.kind {
            Kind::InitializeParams => initialize_params(message),
            Kind::InitializeResult => initialize_result(message),
            Kind::DefinitionResult => definition_result(message),
            Kind::DidOpen => did_open(message),
            Kind::DidChange => did_change(message),
            Kind::DidSave => did_save(message),
            Kind::DidClose => did_close(message),
            Kind::Progress => progress(message),
        }
    }
}

/// The corpus covers every kind the comparison knows how to make.
///
/// Without this a kind can be dropped from the corpus — or added to `Kind` and
/// never exercised — and the differential still passes, reporting agreement it
/// never checked. That is the failure mode of every corpus-driven test and it
/// is silent by construction.
#[test]
fn the_corpus_exercises_every_kind_the_differential_can_compare() {
    let corpus = corpus();
    for kind in KINDS {
        assert!(
            corpus.iter().any(|entry| entry.kind == *kind),
            "no {kind:?} message in golden-traffic.jsonl, so nothing compared one"
        );
    }
    assert!(
        corpus.len() >= KINDS.len() * 2,
        "a corpus with one message per kind exercises no union's second shape"
    );
}

/// §8.5 does not ask for a corpus, it asks for **captured** traffic: "real
/// `initialize` / `InitializeResult` pairs and document traffic captured from
/// Zed and VS Code against rust-analyzer, pyright, and gopls".
///
/// The distinction is the whole argument. A hand-authored message contains the
/// fields somebody thought of, and §8.6 says in as many words that "a field
/// that appears in no captured message is untested by construction, and that is
/// exactly the long tail". So a corpus that drifted back to being entirely
/// hand-written would satisfy the differential and not the section, and nothing
/// would say so — which is what this asserts instead.
///
/// The three kinds named are the server-to-client ones, and that is the whole
/// of what a loop can produce: `initialize` params and document traffic are
/// composed by an editor, and a message this project wrote to look like VS
/// Code's is hand-authored however it is labelled. Whether that leaves §8.5's
/// condition met is `state/decisions/core-018.md`, which also says why the
/// missing half is the one that matters — every `didChange` here is
/// hand-authored, and `contentChanges` is the union §8.5 spends its longest
/// passage on.
// DECISION-core-018: provisional
#[test]
fn the_corpus_holds_traffic_nobody_here_composed() {
    let corpus = corpus();
    let captured: Vec<&Entry> = corpus
        .iter()
        .filter(|entry| entry.source.starts_with("CAPTURED"))
        .collect();
    assert!(
        !captured.is_empty(),
        "no captured message in golden-traffic.jsonl: core.md §8.5 makes real traffic one of \
         the two conditions on which hand-rolled wire types are acceptable, and hand-authored \
         messages are the population it says is not the long tail"
    );

    for kind in [
        Kind::InitializeResult,
        Kind::DefinitionResult,
        Kind::Progress,
    ] {
        assert!(
            captured.iter().any(|entry| entry.kind == kind),
            "no captured {kind:?}: the header of golden-traffic.jsonl says how to record one, \
             and a server saying something we did not predict is the only way this corpus \
             finds a field nobody modelled"
        );
    }
}

/// §8.5 does not name servers loosely. It asks for traffic against
/// "rust-analyzer, pyright, and gopls", and the three are not
/// interchangeable: the captured `initialize` answers disagree about which
/// shape *every* union arrives in. rust-analyzer sends `textDocumentSync` as
/// an options object and pyright as the bare integer; rust-analyzer sends
/// `definitionProvider` as `true` and pyright as an options object; gopls'
/// `$/progress` token is a string of digits where rust-analyzer's is a name.
/// A corpus holding one server's traffic three times over would satisfy the
/// test above and would have found none of that.
///
/// So the server is read out of `source` rather than trusted to a comment.
/// The format is `CAPTURED from <server> <version>...`, which is what the
/// corpus header tells a future capture to write, and this is what makes that
/// instruction load-bearing instead of advisory.
#[test]
fn the_captured_half_covers_every_server_the_section_names() {
    let corpus = corpus();
    let servers: Vec<&str> = corpus
        .iter()
        .filter_map(|entry| captured_server(&entry.source))
        .collect();

    for named in ["rust-analyzer", "pyright", "gopls"] {
        assert!(
            servers.contains(&named),
            "no captured traffic from {named}, which core.md §8.5 names by hand: the three \
             servers answer the same initialize in different shapes, so one of them missing \
             is a union with no real message behind it — {servers:?}"
        );
    }
}

/// The server a `CAPTURED` line came from, or `None` for a hand-authored one.
///
/// Panics on a `CAPTURED` line that does not name one, rather than treating it
/// as hand-authored: a line claiming provenance it did not record is worse
/// than a line claiming none, since the test above would then pass over it.
fn captured_server(source: &str) -> Option<&str> {
    if !source.starts_with("CAPTURED") {
        return None;
    }
    let server = source
        .strip_prefix("CAPTURED from ")
        .and_then(|named| named.split([' ', ':', ',']).next())
        .filter(|server| !server.is_empty());
    Some(server.unwrap_or_else(|| {
        panic!(
            "a CAPTURED line names no server: the format is `CAPTURED from <server> <version>`\n{source}"
        )
    }))
}

fn corpus() -> Vec<Entry> {
    CORPUS
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<Entry>(line)
                .unwrap_or_else(|error| panic!("golden-traffic.jsonl line {}: {error}", index + 1))
        })
        .collect()
}

/// Both readings of `initialize`, reduced to what the shim actually consumes.
///
/// `positionEncodings` is compared after dropping the encodings we do not
/// model: `known_encodings` filters them out during deserialization, where
/// `lsp-types` keeps every string it is given, and both behaviours are right.
/// What must agree is the set of encodings that mean something to us and the
/// order they arrived in, since LSP makes the order the client's preference.
fn initialize_params(message: &str) {
    let ours: InitializeParams = read(message, "shared::proto::InitializeParams");
    let theirs: lsp_types::InitializeParams = read(message, "lsp_types::InitializeParams");

    assert_eq!(
        ours.root_uri.as_ref().map(|uri| uri.as_str().to_owned()),
        theirs.root_uri.as_ref().map(ToString::to_string),
        "rootUri"
    );
    // Their `name` has no counterpart: §8.2 makes an incoming type a partial
    // projection, and the shim has no use for what a folder is called. "The
    // fields we model agree" is the claim, so the comparison is over ours.
    assert_eq!(
        ours.workspace_folders.as_ref().map(|folders| folders
            .iter()
            .map(|folder| folder.uri.as_str().to_owned())
            .collect::<Vec<String>>()),
        theirs.workspace_folders.as_ref().map(|folders| folders
            .iter()
            .map(|folder| folder.uri.to_string())
            .collect::<Vec<String>>()),
        "workspaceFolders"
    );
    assert_eq!(
        ours.client_info.as_ref().map(|info| (
            info.name.to_string(),
            info.version.as_deref().map(str::to_owned)
        )),
        theirs
            .client_info
            .as_ref()
            .map(|info| (info.name.clone(), info.version.clone())),
        "clientInfo"
    );

    let their_encodings: Vec<PositionEncoding> = theirs
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref())
        .map(|kinds| kinds.iter().filter_map(modelled_encoding).collect())
        .unwrap_or_default();
    assert_eq!(
        ours.capabilities
            .general
            .as_ref()
            .map(|general| general.position_encodings.clone())
            .unwrap_or_default(),
        their_encodings,
        "general.positionEncodings, after dropping the ones we do not model"
    );

    assert_eq!(
        ours.capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.definition.as_ref())
            .and_then(|definition| definition.link_support),
        theirs
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.definition.as_ref())
            .and_then(|definition| definition.link_support),
        "textDocument.definition.linkSupport"
    );
    assert_eq!(
        ours.capabilities
            .window
            .as_ref()
            .and_then(|window| window.show_document.as_ref())
            .map(|show| show.support),
        theirs
            .capabilities
            .window
            .as_ref()
            .and_then(|window| window.show_document.as_ref())
            .map(|show| show.support),
        "window.showDocument.support"
    );
}

/// `InitializeResult`, which carries two of §8.5's five unions.
fn initialize_result(message: &str) {
    let ours: InitializeResult = read(message, "shared::proto::InitializeResult");
    let theirs: lsp_types::InitializeResult = read(message, "lsp_types::InitializeResult");

    assert_eq!(
        ours.capabilities.position_encoding,
        theirs
            .capabilities
            .position_encoding
            .as_ref()
            .and_then(modelled_encoding),
        "capabilities.positionEncoding"
    );
    assert_eq!(
        our_sync(ours.capabilities.text_document_sync.as_ref()),
        their_sync(theirs.capabilities.text_document_sync.as_ref()),
        "capabilities.textDocumentSync"
    );
    assert_eq!(
        our_definition(ours.capabilities.definition_provider.as_ref()),
        their_definition(theirs.capabilities.definition_provider.as_ref()),
        "capabilities.definitionProvider — absent, false and an options object are three answers"
    );
    assert_eq!(
        ours.server_info.as_ref().map(|info| (
            info.name.to_string(),
            info.version.as_deref().map(str::to_owned)
        )),
        theirs
            .server_info
            .as_ref()
            .map(|info| (info.name.clone(), info.version.clone())),
        "serverInfo"
    );
}

/// The definition result: four shapes, and the one that would hurt is a
/// `LocationLink[]` read as a `Location[]`, since the ranges mean different
/// things.
fn definition_result(message: &str) {
    let ours: DefinitionResult = read(message, "shared::proto::DefinitionResult");
    let theirs: Option<lsp_types::GotoDefinitionResponse> =
        read(message, "lsp_types::GotoDefinitionResponse");
    assert_eq!(
        our_sites(&ours),
        their_sites(theirs.as_ref()),
        "the definition result"
    );
}

fn did_open(message: &str) {
    let ours: DidOpenTextDocumentParams = read(message, "shared::proto::DidOpenTextDocumentParams");
    let theirs: lsp_types::DidOpenTextDocumentParams =
        read(message, "lsp_types::DidOpenTextDocumentParams");

    assert_eq!(
        ours.text_document.uri.as_str(),
        theirs.text_document.uri.as_str(),
        "textDocument.uri"
    );
    assert_eq!(
        &*ours.text_document.language_id, theirs.text_document.language_id,
        "textDocument.languageId"
    );
    assert_eq!(
        ours.text_document.version.0, theirs.text_document.version,
        "textDocument.version"
    );
    assert_eq!(
        &*ours.text_document.text, theirs.text_document.text,
        "textDocument.text"
    );
}

/// `contentChanges`, which is the union §8.5 spends its longest section on:
/// `{text}` is a subset of `{range, text}`, so an untagged enum reads an
/// incremental change as a whole-document one and destroys the document.
///
/// `lsp-types` models the two as a single struct with an optional `range`,
/// which is a legitimate way out and makes it an unusually good oracle here:
/// its `range: None` is our `Full`, and the two implementations disagreeing
/// about which shape arrived is exactly the bug.
fn did_change(message: &str) {
    let ours: DidChangeTextDocumentParams =
        read(message, "shared::proto::DidChangeTextDocumentParams");
    let theirs: lsp_types::DidChangeTextDocumentParams =
        read(message, "lsp_types::DidChangeTextDocumentParams");

    assert_eq!(
        ours.text_document.uri.as_str(),
        theirs.text_document.uri.as_str(),
        "textDocument.uri"
    );
    assert_eq!(
        ours.text_document.version.0, theirs.text_document.version,
        "textDocument.version"
    );
    assert_eq!(
        ours.content_changes
            .iter()
            .map(our_change)
            .collect::<Vec<Change>>(),
        theirs
            .content_changes
            .iter()
            .map(their_change)
            .collect::<Vec<Change>>(),
        "contentChanges"
    );
}

fn did_save(message: &str) {
    let ours: DidSaveTextDocumentParams = read(message, "shared::proto::DidSaveTextDocumentParams");
    let theirs: lsp_types::DidSaveTextDocumentParams =
        read(message, "lsp_types::DidSaveTextDocumentParams");

    assert_eq!(
        ours.text_document.uri.as_str(),
        theirs.text_document.uri.as_str(),
        "textDocument.uri"
    );
    assert_eq!(
        ours.text.as_deref().map(str::to_owned),
        theirs.text,
        "text — absent is not the empty string, since §8.6's checksum compares it"
    );
}

fn did_close(message: &str) {
    let ours: DidCloseTextDocumentParams =
        read(message, "shared::proto::DidCloseTextDocumentParams");
    let theirs: lsp_types::DidCloseTextDocumentParams =
        read(message, "lsp_types::DidCloseTextDocumentParams");
    assert_eq!(
        ours.text_document.uri.as_str(),
        theirs.text_document.uri.as_str(),
        "textDocument.uri"
    );
}

/// `$/progress`, for its token: number or string, §8.5's one union that is
/// disjoint by JSON kind and therefore genuinely safe untagged.
///
/// The payload is deliberately not compared. Ours keeps it as a `RawValue`
/// because what a progress value means is per-server (`shim.md` §6), and the
/// oracle parsing it at all is the assertion — a message `lsp-types` reads as
/// work-done progress is one whose token we are reading from the right place.
fn progress(message: &str) {
    let ours: ProgressParams = read(message, "shared::proto::ProgressParams");
    let theirs: lsp_types::ProgressParams = read(message, "lsp_types::ProgressParams");

    let ours = match ours.token {
        ProgressToken::Number(number) => Token::Number(number),
        ProgressToken::Text(text) => Token::Text(text.to_string()),
    };
    let theirs = match theirs.token {
        lsp_types::NumberOrString::Number(number) => Token::Number(number.into()),
        lsp_types::NumberOrString::String(text) => Token::Text(text),
    };
    assert_eq!(ours, theirs, "token");
}

#[derive(PartialEq, Eq, Debug)]
enum Token {
    Number(i64),
    Text(String),
}

/// One `textDocumentSync`, in whichever of the union's two shapes it arrived.
#[derive(PartialEq, Eq, Debug)]
enum Sync {
    Absent,
    Kind(u8),
    Options {
        open_close: Option<bool>,
        change: Option<u8>,
    },
}

fn our_sync(sync: Option<&TextDocumentSync>) -> Sync {
    match sync {
        None => Sync::Absent,
        Some(TextDocumentSync::Kind(kind)) => Sync::Kind(our_sync_kind(*kind)),
        Some(TextDocumentSync::Options(options)) => Sync::Options {
            open_close: options.open_close,
            change: options.change.map(our_sync_kind),
        },
    }
}

fn their_sync(sync: Option<&TextDocumentSyncCapability>) -> Sync {
    match sync {
        None => Sync::Absent,
        Some(TextDocumentSyncCapability::Kind(kind)) => Sync::Kind(their_sync_kind(*kind)),
        Some(TextDocumentSyncCapability::Options(options)) => Sync::Options {
            open_close: options.open_close,
            change: options.change.map(their_sync_kind),
        },
    }
}

fn our_sync_kind(kind: TextDocumentSyncKind) -> u8 {
    match kind {
        TextDocumentSyncKind::None => 0,
        TextDocumentSyncKind::Full => 1,
        TextDocumentSyncKind::Incremental => 2,
    }
}

fn their_sync_kind(kind: lsp_types::TextDocumentSyncKind) -> u8 {
    match kind {
        lsp_types::TextDocumentSyncKind::FULL => 1,
        lsp_types::TextDocumentSyncKind::INCREMENTAL => 2,
        _ => 0,
    }
}

/// One `definitionProvider`. Three answers rather than two: absent means the
/// server does not support go to definition, and collapsing it into `false`
/// with a `#[serde(default)]` is the mistake §8.5 names.
#[derive(PartialEq, Eq, Debug)]
enum Definition {
    Unsupported,
    Plain(bool),
    Options { work_done_progress: Option<bool> },
}

fn our_definition(provider: Option<&DefinitionProvider>) -> Definition {
    match provider {
        None => Definition::Unsupported,
        Some(DefinitionProvider::Supported(supported)) => Definition::Plain(*supported),
        Some(DefinitionProvider::Options(options)) => Definition::Options {
            work_done_progress: options.work_done_progress,
        },
    }
}

fn their_definition(provider: Option<&OneOf<bool, lsp_types::DefinitionOptions>>) -> Definition {
    match provider {
        None => Definition::Unsupported,
        Some(OneOf::Left(supported)) => Definition::Plain(*supported),
        Some(OneOf::Right(options)) => Definition::Options {
            work_done_progress: options.work_done_progress_options.work_done_progress,
        },
    }
}

/// A definition answer, flattened to what §6's predicate compares: which file,
/// and where in it. `None` and `[]` stay distinguishable, since one is a
/// server declining to answer and the other is a server answering "nowhere".
#[derive(PartialEq, Eq, Debug)]
enum Sites {
    Null,
    At(Vec<(String, Span)>),
}

/// A range as four numbers, with the columns recovered through [`GRID`].
#[derive(PartialEq, Eq, Debug)]
struct Span {
    start: (u32, u32),
    end: (u32, u32),
}

fn our_sites(result: &DefinitionResult) -> Sites {
    match result {
        DefinitionResult::Null => Sites::Null,
        DefinitionResult::One(location) => Sites::At(vec![(
            location.uri().as_str().to_owned(),
            our_span(location.range()),
        )]),
        DefinitionResult::Many(locations) => Sites::At(
            locations
                .iter()
                .map(|location| {
                    (
                        location.uri().as_str().to_owned(),
                        our_span(location.range()),
                    )
                })
                .collect(),
        ),
        DefinitionResult::Links(links) => Sites::At(
            links
                .iter()
                .map(|link| {
                    (
                        link.target_uri.as_str().to_owned(),
                        our_span(link.target_range),
                    )
                })
                .collect(),
        ),
    }
}

fn their_sites(result: Option<&lsp_types::GotoDefinitionResponse>) -> Sites {
    match result {
        None => Sites::Null,
        Some(lsp_types::GotoDefinitionResponse::Scalar(location)) => {
            Sites::At(vec![(location.uri.to_string(), their_span(location.range))])
        }
        Some(lsp_types::GotoDefinitionResponse::Array(locations)) => Sites::At(
            locations
                .iter()
                .map(|location| (location.uri.to_string(), their_span(location.range)))
                .collect(),
        ),
        Some(lsp_types::GotoDefinitionResponse::Link(links)) => Sites::At(
            links
                .iter()
                .map(|link| (link.target_uri.to_string(), their_span(link.target_range)))
                .collect(),
        ),
    }
}

fn our_span(range: WireRange) -> Span {
    Span {
        start: our_position(range.start),
        end: our_position(range.end),
    }
}

fn their_span(range: lsp_types::Range) -> Span {
    Span {
        start: (range.start.line, range.start.character),
        end: (range.end.line, range.end.character),
    }
}

/// A wire position as a row and a column, recovered rather than read.
///
/// `character` is private with no accessor — §8.3's inertness — so the only
/// way to a number is `resolve`, which needs an encoding and a document. The
/// document is [`GRID`], where every line is the same length and every
/// character is one byte, so the offset that comes back is
/// `line * (COLUMNS + 1) + character` and inverting it is exact. UTF-16 is
/// named because the corpus's positions are the ones a real editor sends, and
/// on an all-ASCII document every encoding agrees anyway.
fn our_position(position: WirePosition) -> (u32, u32) {
    let Offset(offset) = position
        .resolve(PositionEncoding::Utf16, &GRID)
        .expect("a corpus position lands inside the grid");
    let line = position.line().0;
    let start = usize::try_from(line).expect("a row fits a usize") * (COLUMNS + 1);
    let column = u32::try_from(offset - start).expect("a column fits a u32");
    (line, column)
}

const COLUMNS: usize = 96;
const ROWS: usize = 96;

/// An ASCII document of known geometry: [`ROWS`] lines of exactly [`COLUMNS`]
/// characters. Large enough to hold every position in the corpus, which is the
/// one thing a new corpus line can break — and it breaks loudly, since
/// `resolve` refuses a position outside the document rather than clipping.
static GRID: LazyLock<Rope> = LazyLock::new(|| {
    let mut text = String::with_capacity(ROWS * (COLUMNS + 1));
    for _ in 0..ROWS {
        text.push_str(&"x".repeat(COLUMNS));
        text.push('\n');
    }
    Rope::from(text.as_str())
});

/// The encodings we model, from `lsp-types`' open string newtype. `None` for
/// one we do not, which is a real answer rather than a failure: a client may
/// offer anything, and `known_encodings` drops what we cannot honour.
fn modelled_encoding(kind: &lsp_types::PositionEncodingKind) -> Option<PositionEncoding> {
    if *kind == lsp_types::PositionEncodingKind::UTF8 {
        Some(PositionEncoding::Utf8)
    } else if *kind == lsp_types::PositionEncodingKind::UTF16 {
        Some(PositionEncoding::Utf16)
    } else if *kind == lsp_types::PositionEncodingKind::UTF32 {
        Some(PositionEncoding::Utf32)
    } else {
        None
    }
}

/// One content change, in whichever shape it arrived.
#[derive(PartialEq, Eq, Debug)]
enum Change {
    Full { text: String },
    Incremental { span: Span, text: String },
}

fn our_change(change: &ContentChange) -> Change {
    match change {
        ContentChange::Full { text } => Change::Full {
            text: text.to_string(),
        },
        ContentChange::Incremental { range, text } => Change::Incremental {
            span: our_span(*range),
            text: text.to_string(),
        },
    }
}

fn their_change(change: &lsp_types::TextDocumentContentChangeEvent) -> Change {
    match change.range {
        None => Change::Full {
            text: change.text.clone(),
        },
        Some(range) => Change::Incremental {
            span: their_span(range),
            text: change.text.clone(),
        },
    }
}

fn read<T: for<'de> Deserialize<'de>>(message: &str, what: &str) -> T {
    serde_json::from_str(message).unwrap_or_else(|error| {
        panic!("{what} could not read a corpus message: {error}\n{message}")
    })
}
