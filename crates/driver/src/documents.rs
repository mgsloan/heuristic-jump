//! The open-document map, and with it `design/core.md` §8.6's fail-closed
//! rule.
//!
//! §8.6 is the argument that makes hand-written protocol projections an
//! acceptable risk. Detection is explicitly *not* the plan — "a field that
//! appears in no captured message is untested by construction, and that is
//! exactly the long tail" — so what the section asks for is that the
//! consequence be safe:
//!
//! > Any failure or detected inconsistency while deserializing a state-bearing
//! > message marks that document **untrusted**. Queries against an untrusted
//! > document abstain, unconditionally, until a `didClose`/`didOpen` resyncs
//! > it.
//!
//! **"Unconditionally" is spelled as an absence rather than as a check.**
//! Distrust drops the rope: a text we have stopped believing is not one to
//! keep answering from, and every route back to trust supplies a whole new one
//! anyway, since `didOpen` carries the document. So an untrusted row has
//! nothing to build a query out of, [`Documents::query`] can only hand back a
//! [`Trusted`] for a row that still has a text, and [`OpenDocument::new`] —
//! the only constructor, and the only route to a `SnapshotSeed` and therefore
//! to `dispatch` — takes one. There is no code path that abstains on an
//! untrusted document because there is no code path that does anything else.
//!
//! **The four notifications are read here, not by the caller.** §8.6's rule
//! starts at "any failure *while deserializing*", and a caller that
//! deserialized first would have nowhere to report one: by the time it holds a
//! `serde_json::Error` it does not hold the params, and the whole point is
//! that the document named by a message we could not read is the document that
//! has drifted. So these methods take the raw params — valid JSON, since a
//! frame that is not JSON is `CodecError`'s and not ours — and project inside.
//!
//! What is missing is the actor and the transport. `shim.md` §5's `core` would
//! own this map and feed it from the reader thread; there is no reader thread,
//! so it is fed by its caller. That is the wiring. The ownership is already
//! right: nothing here is shared, nothing is locked, and every mutation needs
//! `&mut self`, which only the owner has.

use rustc_hash::FxHashMap;
use serde_json::value::RawValue;
use shared::proto::{
    ContentChange, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, NotifiedDocument, PositionEncoding,
};
use shared::{
    ByteLen, DocumentError, DocumentNotification, DocumentUri, DocumentVersion, LanguageId, Rope,
};

use crate::dispatch::Registry;

/// One row per document the editor has opened and not closed.
#[derive(Debug, Default)]
pub struct Documents {
    open: FxHashMap<DocumentUri, Open>,
}

/// A row, in the two states §8.6 gives it.
///
/// `Untrusted` keeps the failure and nothing else. Dropping the text with the
/// trust is what makes the rule structural rather than checked: see the module
/// documentation.
#[derive(Debug)]
enum Open {
    Trusted(Believed),
    Untrusted(DocumentError),
}

/// What we believe about a document we still believe about.
#[derive(Debug)]
struct Believed {
    text: Rope,
    version: DocumentVersion,
    language_id: LanguageId,
}

/// A document the map still believes in, borrowed from it.
///
/// Nothing constructs one but [`Documents::query`], which is what
/// [`OpenDocument::new`](crate::OpenDocument::new) requiring one buys: a query
/// against an untrusted document is unspellable rather than refused.
#[derive(Debug)]
pub struct Trusted<'a> {
    uri: &'a DocumentUri,
    believed: &'a Believed,
}

impl<'a> Trusted<'a> {
    /// The map's own key, so a caller does not have to keep its copy alive to
    /// hold a `Trusted`.
    pub fn uri(&self) -> &'a DocumentUri {
        self.uri
    }

    pub fn text(&self) -> &'a Rope {
        &self.believed.text
    }

    pub fn version(&self) -> DocumentVersion {
        self.believed.version
    }

    pub fn language_id(&self) -> LanguageId {
        self.believed.language_id
    }
}

/// What a query may do with a document, which is the whole of §8.6's
/// consequence.
///
/// An enum rather than an `Option` because the two non-answers are different
/// facts and the log lines differ: a document nobody opened is an editor and a
/// shim that disagree about what exists, where an untrusted one is a document
/// we tracked and stopped being able to.
#[derive(Debug)]
pub enum Queried<'a> {
    /// Dispatch may proceed.
    Trusted(Trusted<'a>),
    /// No row at all: never opened, or closed since.
    NotOpen,
    /// §8.6: abstain, without reaching a handler, until a `didClose`/`didOpen`
    /// resyncs. The error is what fired, kept so the abstention can say why.
    Untrusted(&'a DocumentError),
}

/// What one state-bearing notification did to the map.
///
/// Returned rather than swallowed so that §8.6's three self-checks are
/// assertable at all: they are the kind of code whose absence nothing else in
/// the build would notice.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Synced {
    /// Our model now matches the message.
    Applied,
    /// There was no trusted model to change — an unclaimed language, a
    /// document never opened, or one already untrusted. Not a distrust:
    /// nothing here stopped being believed, because nothing here was.
    Untracked,
    /// §8.6: at least one document is untrusted as of this message. Which, and
    /// why, is [`Documents::distrust`].
    Distrusted,
}

/// [`Documents::saved`]'s answer, split because §8.6 puts half of it on
/// another thread.
///
/// The `didSave` checksum "costs a read, so it belongs in a worker, off the
/// critical path" — but only when the notification did not carry the text
/// itself, which it does whenever the server asked for it in its save options.
/// The free case is settled here; the other is handed back as a job.
#[derive(Debug)]
pub enum Saved {
    /// `text` was in the notification, so the check was free and is done.
    Checked(Synced),
    /// The check needs the file. Read it in a worker and come back through
    /// [`Documents::checked`].
    NeedsRead(SaveCheck),
}

/// The outstanding half of a `didSave` check.
///
/// Its field is private and it has no constructor, so [`Documents::checked`]
/// cannot be reached except by having been told a read was needed — which is
/// what stops the expensive check being run speculatively against text nobody
/// saved.
#[derive(Debug)]
pub struct SaveCheck {
    uri: DocumentUri,
}

impl SaveCheck {
    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }
}

impl Documents {
    pub fn new() -> Self {
        Self::default()
    }

    /// `didOpen`, which is one of the two messages §8.6 says resyncs a
    /// document: it carries the whole text, so whatever we believed before —
    /// including that the document was not to be believed — is replaced rather
    /// than reconciled.
    ///
    /// The registry is here because `TextDocumentItem::language_id` is a
    /// `Box<str>` and a [`LanguageId`] is interned (`core.md` §8.2): resolving
    /// one is a registry lookup, and a language nothing claims is not a
    /// document this map has any business modelling.
    pub fn opened(&mut self, params: &RawValue, registry: &Registry) -> Synced {
        let opened = match serde_json::from_str::<DidOpenTextDocumentParams>(params.get()) {
            Ok(opened) => opened,
            Err(source) => return self.unreadable(params, DocumentNotification::DidOpen, source),
        };
        let item = opened.text_document;
        let Some(language_id) = registry.language_id(&item.language_id) else {
            // Removed rather than left alone: the same URI can be reopened
            // under a different languageId, and a stale row for one nothing
            // handles is a rope kept up to date for no reader.
            self.open.remove(&item.uri);
            return Synced::Untracked;
        };
        self.open.insert(
            item.uri,
            Open::Trusted(Believed {
                text: Rope::from(&*item.text),
                version: item.version,
                language_id,
            }),
        );
        Synced::Applied
    }

    /// `didChange`, and with it two of §8.6's three self-checks: a version
    /// that does not increase, and a range outside our rope.
    ///
    /// `encoding` is the negotiated one, because a `WireRange`'s columns are
    /// in it and `WirePosition::resolve` is the only way out of one
    /// (`core.md` §8.3). Refusing rather than clipping is exactly what this
    /// check needs: a position that does not survive the round trip is the
    /// divergence, and a clipped one would apply the edit somewhere plausible
    /// and nearby.
    pub fn changed(&mut self, params: &RawValue, encoding: PositionEncoding) -> Synced {
        let changed = match serde_json::from_str::<DidChangeTextDocumentParams>(params.get()) {
            Ok(changed) => changed,
            Err(source) => return self.unreadable(params, DocumentNotification::DidChange, source),
        };
        let uri = changed.text_document.uri;
        let arriving = changed.text_document.version;

        let applied = match self.open.get_mut(&uri) {
            None => Err(DocumentError::NotOpen {
                notification: DocumentNotification::DidChange,
                uri: uri.clone(),
            }),
            // Already untrusted, and it stays that way until a didOpen. There
            // is no text to apply the change to, which is the point.
            Some(Open::Untrusted(_)) => return Synced::Untracked,
            Some(Open::Trusted(believed)) => {
                apply(&uri, believed, arriving, &changed.content_changes, encoding)
            }
        };
        match applied {
            Ok(()) => Synced::Applied,
            Err(error) => self.stop_trusting(uri, error),
        }
    }

    /// `didSave`, which §8.6 calls "a free end-to-end checksum": immediately
    /// after a save the buffer and the file are identical by definition, so
    /// this validates the entire document-tracking pipeline against ground
    /// truth at a point where the answer is known.
    pub fn saved(&mut self, params: &RawValue) -> Saved {
        let saved = match serde_json::from_str::<DidSaveTextDocumentParams>(params.get()) {
            Ok(saved) => saved,
            Err(source) => {
                return Saved::Checked(self.unreadable(
                    params,
                    DocumentNotification::DidSave,
                    source,
                ));
            }
        };
        let uri = saved.text_document.uri;
        match self.open.get(&uri) {
            None | Some(Open::Untrusted(_)) => Saved::Checked(Synced::Untracked),
            Some(Open::Trusted(_)) => match saved.text {
                Some(text) => Saved::Checked(self.compare(uri, &text)),
                None => Saved::NeedsRead(SaveCheck { uri }),
            },
        }
    }

    /// The other half of [`Documents::saved`], once the worker's read has
    /// landed.
    pub fn checked(&mut self, check: SaveCheck, on_disk: &str) -> Synced {
        self.compare(check.uri, on_disk)
    }

    /// `didClose`, the other message §8.6 says resyncs a document — here by
    /// removing the row, so a later query is `NotOpen` rather than untrusted.
    pub fn closed(&mut self, params: &RawValue) -> Synced {
        let closed = match serde_json::from_str::<DidCloseTextDocumentParams>(params.get()) {
            Ok(closed) => closed,
            Err(source) => return self.unreadable(params, DocumentNotification::DidClose, source),
        };
        match self.open.remove(&closed.text_document.uri) {
            Some(_) => Synced::Applied,
            None => Synced::Untracked,
        }
    }

    /// The one route from a document to the query against it.
    pub fn query(&self, uri: &DocumentUri) -> Queried<'_> {
        // `get_key_value` rather than `get`, so a `Trusted` borrows the map's
        // own key and the caller's copy does not have to outlive it.
        match self.open.get_key_value(uri) {
            None => Queried::NotOpen,
            Some((_, Open::Untrusted(error))) => Queried::Untrusted(error),
            Some((uri, Open::Trusted(believed))) => Queried::Trusted(Trusted { uri, believed }),
        }
    }

    /// Why a document is untrusted, or `None` if it is not — which includes
    /// not being open.
    pub fn distrust(&self, uri: &DocumentUri) -> Option<&DocumentError> {
        match self.open.get(uri)? {
            Open::Untrusted(error) => Some(error),
            Open::Trusted(_) => None,
        }
    }

    /// §8.6's checksum, against whichever text the save produced.
    ///
    /// Byte equality and not a length: the length is the cheap half and it
    /// short-circuits, but two texts of the same length are exactly the drift
    /// this check exists to find, and the read that produced `saved` already
    /// cost more than the comparison does.
    fn compare(&mut self, uri: DocumentUri, saved: &str) -> Synced {
        let Some(Open::Trusted(believed)) = self.open.get(&uri) else {
            return Synced::Untracked;
        };
        if holds(&believed.text, saved) {
            return Synced::Applied;
        }
        let error = DocumentError::SavedTextDiffers {
            uri: uri.clone(),
            held: ByteLen(believed.text.len()),
            found: ByteLen(saved.len()),
        };
        self.stop_trusting(uri, error)
    }

    /// §8.6's rule at its first clause: the message itself did not parse.
    ///
    /// The document it was about is what the rule needs and what the failed
    /// deserialization did not produce, so the identifier is read on its own.
    /// That succeeds in the case that actually happens — the modelling mistake
    /// is somewhere in `contentChanges`, not in `textDocument` — and one
    /// document is distrusted.
    fn unreadable(
        &mut self,
        params: &RawValue,
        notification: DocumentNotification,
        source: serde_json::Error,
    ) -> Synced {
        match serde_json::from_str::<NotifiedDocument>(params.get()) {
            Ok(named) => self.stop_trusting(
                named.text_document.uri,
                DocumentError::Unreadable {
                    notification,
                    source,
                },
            ),
            // Nothing said which document drifted, so nothing rules any of
            // them out. §8.6's direction is the one that stops answering
            // confidently, and the cost is bounded: each document recovers on
            // its own next didOpen.
            Err(unattributable) => {
                tracing::warn!(
                    %notification,
                    %source,
                    %unattributable,
                    open = self.open.len(),
                    "a state-bearing message named no document"
                );
                let named: Vec<DocumentUri> = self.open.keys().cloned().collect();
                for uri in named {
                    self.stop_trusting(uri, DocumentError::Unattributable { notification });
                }
                Synced::Distrusted
            }
        }
    }

    /// The conversion `deps.md` §10 requires to be "explicit and logged": an
    /// `Error` becoming an abstention. It is the only writer of an untrusted
    /// row, and the insert is what drops the text.
    fn stop_trusting(&mut self, uri: DocumentUri, error: DocumentError) -> Synced {
        tracing::warn!(
            %uri,
            %error,
            "document untrusted: queries against it abstain until a didOpen resyncs it"
        );
        self.open.insert(uri, Open::Untrusted(error));
        Synced::Distrusted
    }
}

/// Applies one `didChange`'s worth of changes, in order and each against the
/// text the previous one left — which is LSP's own rule, and the reason the
/// ranges are resolved inside the loop rather than up front.
fn apply(
    uri: &DocumentUri,
    believed: &mut Believed,
    arriving: DocumentVersion,
    changes: &[ContentChange],
    encoding: PositionEncoding,
) -> Result<(), DocumentError> {
    if arriving <= believed.version {
        return Err(DocumentError::VersionDidNotIncrease {
            uri: uri.clone(),
            held: believed.version,
            arriving,
        });
    }
    for change in changes {
        match change {
            ContentChange::Full { text } => believed.text = Rope::from(&**text),
            ContentChange::Incremental { range, text } => {
                let outside = |source| DocumentError::RangeOutsideDocument {
                    uri: uri.clone(),
                    source,
                };
                let start = range
                    .start
                    .resolve(encoding, &believed.text)
                    .map_err(outside)?;
                let end = range
                    .end
                    .resolve(encoding, &believed.text)
                    .map_err(outside)?;
                if end < start {
                    return Err(DocumentError::RangeInverted {
                        uri: uri.clone(),
                        start,
                        end,
                    });
                }
                believed.text.replace(start.0..end.0, text);
            }
        }
    }
    believed.version = arriving;
    Ok(())
}

/// Whether a rope and a `&str` are the same bytes, without building the
/// rope's text.
fn holds(text: &Rope, saved: &str) -> bool {
    if text.len() != saved.len() {
        return false;
    }
    let mut rest = saved;
    for chunk in text.chunks() {
        match rest.split_at_checked(chunk.len()) {
            Some((head, tail)) if head == chunk => rest = tail,
            Some(_) | None => return false,
        }
    }
    rest.is_empty()
}
