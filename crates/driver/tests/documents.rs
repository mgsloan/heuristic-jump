//! `design/core.md` §8.6: a modelling error must fail closed.
//!
//! The section's argument is that detection is *not* the plan — "a field that
//! appears in no captured message is untested by construction, and that is
//! exactly the long tail" — so what has to be true is that the consequence is
//! safe:
//!
//! > Any failure or detected inconsistency while deserializing a state-bearing
//! > message marks that document **untrusted**. Queries against an untrusted
//! > document abstain, unconditionally, until a `didClose`/`didOpen` resyncs
//! > it.
//!
//! Half of that is a compile-time property and is not asserted here, because
//! it cannot be: `OpenDocument::new` takes a `Trusted`, `Documents::query`
//! produces one only for a document that still has a text, and those are the
//! only routes to a `SnapshotSeed` and therefore to `dispatch`. A query
//! against an untrusted document does not fail a test — it fails to compile.
//! What is left to assert is the half a type cannot hold: that each of §8.6's
//! self-checks actually fires, that they fire on the right input, and that a
//! document recovers on exactly the two messages the section names.
//!
//! **The positive control is the first test and is not optional.** Every
//! assertion below would pass against a map that distrusted every message it
//! was ever given, which is the failure mode a suite of refusals invites.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and the assertion helpers and fixture builders below are free functions. A fixture that failed to build silently would leave an empty map, which every distrust assertion here passes against."
)]

use std::path::Path;
use std::sync::Arc;

use driver::{Documents, Queried, Registry, Saved, Synced};
use serde_json::value::RawValue;
use shared::proto::PositionEncoding;
use shared::{
    AbstainReason, DocumentError, DocumentNotification, DocumentUri, DocumentVersion, Error,
    FileExtension, Language, LanguageHandler, LanguageId, Outcome, Query, Strata, Stratum, Trace,
};

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

const TEXT: &str = "fn main() {}\n";

/// The control. A map that never applied anything would pass every refusal
/// below, so this is what makes the rest mean "these messages and not the
/// others".
///
/// It is also the one place the incremental path is exercised end to end:
/// the range arrives in the negotiated encoding, `WirePosition::resolve` is
/// the only way out of one (`core.md` §8.3), and the replacement lands where
/// the editor put it.
#[test]
fn the_changes_the_editor_sends_are_the_document_we_hold() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();

    assert_eq!(
        documents.opened(&did_open(&uri, TEXT, 1), &registry),
        Synced::Applied
    );

    assert_eq!(
        documents.changed(&did_change(&uri, 2, r#"{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}},"text":"start"}"#), PositionEncoding::Utf16),
        Synced::Applied,
        "an ordinary incremental change was refused, so every refusal below is vacuous"
    );
    assert_held(&documents, &uri, "fn start() {}\n", 2);

    assert_eq!(
        documents.changed(
            &did_change(&uri, 3, r#"{"text":"fn other() {}\n"}"#),
            PositionEncoding::Utf16
        ),
        Synced::Applied,
        "a whole-document change was refused"
    );
    assert_held(&documents, &uri, "fn other() {}\n", 3);
}

/// Two incremental changes in one notification, the second against the text
/// the first left. LSP's own rule, and the reason the ranges are resolved
/// inside the loop: a range read against the text as it arrived would land in
/// the wrong place as soon as the first change moved anything.
#[test]
fn changes_in_one_notification_apply_in_order() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    let changes = concat!(
        r#"{"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}},"text":"a"},"#,
        r#"{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"text":"pub fn"}"#
    );
    assert_eq!(
        documents.changed(&did_change(&uri, 2, changes), PositionEncoding::Utf16),
        Synced::Applied
    );
    assert_held(&documents, &uri, "pub fn a() {}\n", 2);
}

/// §8.6's first self-check: "an incremental range outside our rope is proof we
/// have already diverged. It cannot happen if every prior change was applied
/// correctly."
#[test]
fn a_range_outside_the_document_stops_it_being_trusted() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    let change =
        r#"{"range":{"start":{"line":9,"character":0},"end":{"line":9,"character":1}},"text":"x"}"#;
    assert_eq!(
        documents.changed(&did_change(&uri, 2, change), PositionEncoding::Utf16),
        Synced::Distrusted
    );
    match documents.distrust(&uri) {
        Some(DocumentError::RangeOutsideDocument { uri: _, source: _ }) => {}
        other => panic!("a range past the last line was not §8.6's first check: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// The same check in the half no encoding conversion can catch: both ends
/// resolve to real offsets and the range is still not a range.
///
/// Worth its own test because it is the one that does not merely lose the
/// document: `Rope::replace` takes a `Range<usize>`, and an inverted one is a
/// panic on the notification path rather than an abstention on the query path.
#[test]
fn a_range_that_ends_before_it_starts_stops_the_document_being_trusted() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    let change =
        r#"{"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":3}},"text":"x"}"#;
    assert_eq!(
        documents.changed(&did_change(&uri, 2, change), PositionEncoding::Utf16),
        Synced::Distrusted
    );
    match documents.distrust(&uri) {
        Some(DocumentError::RangeInverted { .. }) => {}
        other => panic!("a range that ends before it starts was accepted: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// §8.6's second self-check: "a version that does not increase ... means we
/// and the editor disagree about what is open".
///
/// Equal rather than lower, because equal is the one an implementation that
/// wrote `<` would let through, and it is also the shape a duplicated
/// notification has.
#[test]
fn a_version_that_does_not_increase_stops_the_document_being_trusted() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 7), &registry);

    assert_eq!(
        documents.changed(
            &did_change(&uri, 7, r#"{"text":"replaced"}"#),
            PositionEncoding::Utf16
        ),
        Synced::Distrusted
    );
    match documents.distrust(&uri) {
        Some(DocumentError::VersionDidNotIncrease {
            uri: _,
            held: DocumentVersion(7),
            arriving: DocumentVersion(7),
        }) => {}
        other => panic!("a repeated version was not refused as one: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// The other half of §8.6's second check: "a `didChange` for a document never
/// opened".
///
/// The row is created untrusted rather than ignored. There is nothing to stop
/// believing, but there is something to say: `open-questions.md` question 6
/// asks what to do when the editor misbehaves, and §8.6 answers "not ignore,
/// but stop trusting the document and say so in the log".
#[test]
fn a_change_to_a_document_never_opened_stops_it_being_trusted() {
    let mut documents = Documents::new();
    let uri = uri();

    assert_eq!(
        documents.changed(
            &did_change(&uri, 2, r#"{"text":"replaced"}"#),
            PositionEncoding::Utf16
        ),
        Synced::Distrusted
    );
    match documents.distrust(&uri) {
        Some(DocumentError::NotOpen {
            notification: DocumentNotification::DidChange,
            uri: _,
        }) => {}
        other => panic!("a change to an unopened document was not refused: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// §8.6's third self-check, in the half that costs nothing: "immediately after
/// a save, the buffer and the file on disk are identical by definition".
///
/// When the server asked for the text in its save options it arrives with the
/// notification, so the check is settled without a read.
#[test]
fn a_save_that_carries_its_text_checks_the_document_for_free() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    match documents.saved(&did_save(&uri, Some(TEXT))) {
        Saved::Checked(Synced::Applied) => {}
        other @ (Saved::Checked(Synced::Untracked | Synced::Distrusted) | Saved::NeedsRead(_)) => {
            panic!("a save matching what we hold was not applied: {other:?}")
        }
    }
    assert_held(&documents, &uri, TEXT, 1);

    match documents.saved(&did_save(&uri, Some("fn different() {}\n"))) {
        Saved::Checked(Synced::Distrusted) => {}
        other @ (Saved::Checked(Synced::Applied | Synced::Untracked) | Saved::NeedsRead(_)) => {
            panic!("a save that disagrees with our rope was accepted: {other:?}")
        }
    }
    match documents.distrust(&uri) {
        Some(DocumentError::SavedTextDiffers { .. }) => {}
        other => panic!("the save mismatch was not §8.6's third check: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// The same check when the notification carried no text. §8.6 puts the read in
/// a worker, "off the critical path", so the map hands back a job rather than
/// reading anything itself — and there is no way to reach `checked` except by
/// having been handed one.
#[test]
fn a_save_without_text_hands_the_read_to_a_worker() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    let check = match documents.saved(&did_save(&uri, None)) {
        Saved::NeedsRead(check) => check,
        other @ Saved::Checked(_) => {
            panic!("a save with no text settled without reading the file: {other:?}")
        }
    };
    assert_eq!(check.uri(), &uri);
    assert_eq!(documents.checked(check, TEXT), Synced::Applied);
    assert_held(&documents, &uri, TEXT, 1);

    let check = match documents.saved(&did_save(&uri, None)) {
        Saved::NeedsRead(check) => check,
        other @ Saved::Checked(_) => panic!("the second save did not ask for a read: {other:?}"),
    };
    // Same length, different bytes: the length is the cheap half of the check
    // and it is not the check, because two texts of the same length are
    // exactly the drift a whole pipeline can produce.
    assert_eq!(
        documents.checked(check, "fn nain() {}\n"),
        Synced::Distrusted
    );
    assert_abstains(&documents, &uri);
}

/// §8.6's rule at its first clause — a *deserialization* failure, not a
/// detected inconsistency — and §8.5's `range: null` is the case that produces
/// one on traffic that is otherwise well formed.
///
/// The two sections meet here: 8.5 makes `ContentChange` refuse a null range
/// rather than reading it as a whole-document replacement, and 8.6 is what
/// makes that refusal safe instead of merely loud.
#[test]
fn a_message_that_does_not_parse_stops_its_document_being_trusted() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);

    assert_eq!(
        documents.changed(
            &did_change(&uri, 2, r#"{"range":null,"text":"replaced"}"#),
            PositionEncoding::Utf16
        ),
        Synced::Distrusted
    );
    match documents.distrust(&uri) {
        Some(DocumentError::Unreadable {
            notification: DocumentNotification::DidChange,
            source: _,
        }) => {}
        other => panic!("a change that did not parse was not refused as one: {other:?}"),
    }
    assert_abstains(&documents, &uri);
}

/// The same rule when the message did not even say which document it was
/// about. Nothing rules any of them out, so nothing stays trusted — "we do not
/// know which one" is not a reason to keep answering confidently about all of
/// them.
#[test]
fn a_message_that_names_no_document_stops_every_open_one_being_trusted() {
    let mut documents = Documents::new();
    let registry = registry();
    let first = uri();
    let second = uri_of("/workspace/src/other.rs");
    documents.opened(&did_open(&first, TEXT, 1), &registry);
    documents.opened(&did_open(&second, TEXT, 1), &registry);
    assert_held(&documents, &first, TEXT, 1);
    assert_held(&documents, &second, TEXT, 1);

    let params = raw(r#"{"contentChanges":[{"text":"replaced"}]}"#);
    assert_eq!(
        documents.changed(&params, PositionEncoding::Utf16),
        Synced::Distrusted
    );
    for uri in [&first, &second] {
        match documents.distrust(uri) {
            Some(DocumentError::Unattributable {
                notification: DocumentNotification::DidChange,
            }) => {}
            other => panic!("{uri} survived a message nothing could attribute: {other:?}"),
        }
        assert_abstains(&documents, uri);
    }
}

/// "Until a `didClose`/`didOpen` resyncs it" — the `didOpen` half. The
/// notification carries the whole text, so there is nothing to reconcile and
/// the previous belief, including the disbelief, is replaced.
#[test]
fn a_did_open_resyncs_an_untrusted_document() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);
    distrust(&mut documents, &uri);

    assert_eq!(
        documents.opened(&did_open(&uri, "fn resynced() {}\n", 9), &registry),
        Synced::Applied
    );
    assert_held(&documents, &uri, "fn resynced() {}\n", 9);
    assert!(documents.distrust(&uri).is_none());
}

/// The `didClose` half: the row goes, so the next query is about a document
/// nobody opened rather than one nobody believes.
#[test]
fn a_did_close_forgets_an_untrusted_document() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);
    distrust(&mut documents, &uri);

    assert_eq!(documents.closed(&did_close(&uri)), Synced::Applied);
    match documents.query(&uri) {
        Queried::NotOpen => {}
        other @ (Queried::Trusted(_) | Queried::Untrusted(_)) => {
            panic!("a closed document is still in the map: {other:?}")
        }
    }
}

/// "Unconditionally" is the word this one is about. A perfectly well-formed
/// change arriving after the distrust does not restore anything: only the two
/// messages §8.6 names do, and a change is not one of them.
#[test]
fn a_well_formed_change_does_not_restore_trust() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri();
    documents.opened(&did_open(&uri, TEXT, 1), &registry);
    distrust(&mut documents, &uri);

    assert_eq!(
        documents.changed(
            &did_change(&uri, 99, r#"{"text":"fn recovered() {}\n"}"#),
            PositionEncoding::Utf16
        ),
        Synced::Untracked,
        "a change was applied to a document with no text to apply it to"
    );
    assert_abstains(&documents, &uri);
}

/// A language no handler claims is not a document this map models, and that is
/// not a distrust: nothing stopped being believed, because nothing was.
///
/// The `languageId` stays a `Box<str>` until the registry interns it
/// (`core.md` §8.2), which is what makes this a lookup rather than an
/// invention.
#[test]
fn a_language_no_handler_claims_is_not_tracked() {
    let mut documents = Documents::new();
    let registry = registry();
    let uri = uri_of("/workspace/src/main.py");

    let params = raw(&format!(
        r#"{{"textDocument":{{"uri":{},"languageId":"python","version":1,"text":"pass\n"}}}}"#,
        json_string(uri.as_str())
    ));
    assert_eq!(documents.opened(&params, &registry), Synced::Untracked);
    match documents.query(&uri) {
        Queried::NotOpen => {}
        other @ (Queried::Trusted(_) | Queried::Untrusted(_)) => {
            panic!("a language nothing handles was modelled anyway: {other:?}")
        }
    }
}

/// Every route out of `query` that is not a document, seen as what §8.6 says
/// it is: an abstention, decided before a handler exists.
///
/// The reason is `AbstainReason::Deadline`'s opposite in one respect worth
/// naming — it is not a fact about the code — but the point here is only that
/// there is no fourth arm. A `Trusted` is the sole way to a seed, so the two
/// non-answers are the whole of what a query against a document we do not hold
/// can produce.
fn assert_abstains(documents: &Documents, uri: &DocumentUri) {
    match documents.query(uri) {
        Queried::Untrusted(_) | Queried::NotOpen => {}
        Queried::Trusted(trusted) => panic!(
            "core.md §8.6: a document we stopped believing is still queryable at version {:?}",
            trusted.version()
        ),
    }
}

fn assert_held(documents: &Documents, uri: &DocumentUri, text: &str, version: i32) {
    match documents.query(uri) {
        Queried::Trusted(trusted) => {
            let held: String = trusted.text().chunks().collect();
            assert_eq!(held, text);
            assert_eq!(trusted.version(), DocumentVersion(version));
        }
        other @ (Queried::NotOpen | Queried::Untrusted(_)) => {
            panic!("{uri} is not queryable: {other:?}")
        }
    }
}

/// One distrust, by the cheapest route, for the tests that are about what
/// happens *after* one.
fn distrust(documents: &mut Documents, uri: &DocumentUri) {
    let change =
        r#"{"range":{"start":{"line":9,"character":0},"end":{"line":9,"character":1}},"text":"x"}"#;
    assert_eq!(
        documents.changed(&did_change(uri, 2, change), PositionEncoding::Utf16),
        Synced::Distrusted,
        "the fixture did not distrust anything, so the assertions after it are vacuous"
    );
}

fn did_open(uri: &DocumentUri, text: &str, version: i32) -> Box<RawValue> {
    raw(&format!(
        r#"{{"textDocument":{{"uri":{},"languageId":"rust","version":{version},"text":{}}}}}"#,
        json_string(uri.as_str()),
        json_string(text)
    ))
}

fn did_change(uri: &DocumentUri, version: i32, changes: &str) -> Box<RawValue> {
    raw(&format!(
        r#"{{"textDocument":{{"uri":{},"version":{version}}},"contentChanges":[{changes}]}}"#,
        json_string(uri.as_str())
    ))
}

fn did_save(uri: &DocumentUri, text: Option<&str>) -> Box<RawValue> {
    let text = match text {
        Some(text) => format!(r#","text":{}"#, json_string(text)),
        None => String::new(),
    };
    raw(&format!(
        r#"{{"textDocument":{{"uri":{}}}{text}}}"#,
        json_string(uri.as_str())
    ))
}

fn did_close(uri: &DocumentUri) -> Box<RawValue> {
    raw(&format!(
        r#"{{"textDocument":{{"uri":{}}}}}"#,
        json_string(uri.as_str())
    ))
}

fn raw(json: &str) -> Box<RawValue> {
    RawValue::from_string(json.to_owned()).expect("a fixture's params are JSON")
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("a str is always serializable")
}

fn uri() -> DocumentUri {
    uri_of("/workspace/src/lib.rs")
}

fn uri_of(path: &str) -> DocumentUri {
    DocumentUri::from_file_path(Path::new(path)).expect("a file URI for an absolute path")
}

fn registry() -> Registry {
    Registry::new(vec![Arc::new(Claiming)])
}

/// Claims `rust` and nothing else, and is never called: this file is about the
/// map in front of the handler, not about a handler.
struct Claiming;

impl LanguageHandler for Claiming {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn goto_definition(&self, _query: &Query<'_>) -> Result<Outcome, Error> {
        Ok(Outcome::Abstain {
            reason: AbstainReason::NotAnIdentifier,
            strata: Strata::from_reference(Stratum::Unimplemented),
            trace: Trace::new(),
        })
    }
}
