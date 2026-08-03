//! `design/core.md` §2: the immutable view of one document that a handler is
//! given, in two steps. `driver` builds a seed at dispatch — three refcount
//! bumps and a struct move, regardless of file size — and the worker realises
//! it inside the deadline, so the parse is never `core`'s work and a handler
//! is never handed a tree that does not match its text.
//!
//! Both consumers realise the same way, which is the property `core.md` §7
//! leans on when it argues that the corpus scores the code that ships:
//! `measure_core` builds its snapshots through this constructor exactly as the
//! driver does.

use std::ops::ControlFlow;
use std::sync::Arc;

use rope::Rope;
use tree_sitter::{InputEdit, Language, ParseOptions, ParseState, Parser, Point, Tree};

use crate::deadline::Deadline;
use crate::error::{Error, HandlerError, ParseError};
use crate::vocabulary::{DocumentUri, DocumentVersion, LanguageId};

/// What `core` builds at dispatch.
///
/// The stale tree never leaves the seed: `base` holds it together with the
/// edits that reconcile it with `text`, and `realise` is the only way across.
#[derive(Debug)]
pub struct SnapshotSeed {
    pub uri: DocumentUri,
    pub text: Rope,
    pub version: DocumentVersion,
    pub language_id: LanguageId,
    /// Cached tree at some older version, plus the edits that bring it up to
    /// `version`. Never handed to a handler.
    base: Option<(Tree, Arc<Vec<InputEdit>>)>,
    grammar: Language,
}

/// Which of `SnapshotSeed`'s two constructors a seed came from, and therefore
/// what `realise` will do with it.
///
/// It exists to be asserted on. The two paths are deliberately
/// indistinguishable in their *result* — an incremental reparse and a full one
/// produce the same tree for the same text, which is what makes the
/// optimisation safe — so without this, "the cached tree is actually reused"
/// is unobservable, and the gap `core.md` §2 describes (nothing ever fills
/// `base`) could reopen with every test still passing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParseKind {
    /// From scratch: there was no cached tree for this document.
    Full,
    /// From a cached tree plus the edits since it was parsed.
    Incremental,
}

/// What a handler is given. The tree is already correct for the text.
///
/// No cell and no interior mutability: handlers fan out across candidate
/// files, so `&Query` — and therefore `&DocumentSnapshot` — crosses threads
/// and must be `Sync`. Parsing eagerly is what lets that be true without a
/// `OnceLock`, which would have been a blocking primitive on the query path in
/// a design whose rule is that there are no locks anywhere.
#[derive(Debug)]
pub struct DocumentSnapshot {
    pub uri: DocumentUri,
    pub text: Rope,
    pub version: DocumentVersion,
    pub language_id: LanguageId,
    /// Plain field: no cell, no interior mutability. Private so that the one
    /// tree that matches `text` is the only tree obtainable.
    tree: Tree,
}

impl SnapshotSeed {
    /// No cached tree: `realise` will parse from scratch.
    pub fn fresh(
        uri: DocumentUri,
        text: Rope,
        version: DocumentVersion,
        language_id: LanguageId,
        grammar: Language,
    ) -> Self {
        Self {
            uri,
            text,
            version,
            language_id,
            base: None,
            grammar,
        }
    }

    /// `base` is a tree parsed from an *older* version of this document, and
    /// `edits` are the edits since. Taking them together is what makes it
    /// impossible to supply one without the other.
    pub fn incremental(
        uri: DocumentUri,
        text: Rope,
        version: DocumentVersion,
        language_id: LanguageId,
        grammar: Language,
        base: Tree,
        edits: Arc<Vec<InputEdit>>,
    ) -> Self {
        Self {
            uri,
            text,
            version,
            language_id,
            base: Some((base, edits)),
            grammar,
        }
    }

    pub fn parse_kind(&self) -> ParseKind {
        match self.base {
            Some(_) => ParseKind::Incremental,
            None => ParseKind::Full,
        }
    }

    /// Reparses incrementally from `base`, or parses from scratch if there is
    /// none. Called by the worker, never by `core`.
    ///
    /// An unparseable document therefore fails at dispatch and never reaches a
    /// handler, which is what makes `DocumentSnapshot::tree` infallible.
    ///
    /// The `Deadline` is the whole of "inside the deadline" in `core.md` §2:
    /// tree-sitter's parse is the one piece of work on the query path that a
    /// handler cannot poll around, because it happens before the handler
    /// exists. It is abandoned through `ParseOptions`' progress callback and
    /// reported as `HandlerError::DeadlineExpired`, which is the one class the
    /// dispatch wrapper maps back to an abstention — a document too large to
    /// parse inside the budget costs coverage, and must not be recorded as the
    /// parse *failing* (§1, §7).
    ///
    /// Best-effort rather than tight: the callback fires once per 100 parser
    /// operations (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`,
    /// `tree-sitter/src/parser.c:81`), so a small document finishes inside one
    /// interval and observes no deadline at all. `driver`'s `hard_cap` is what
    /// makes the resulting late answer harmless; this only stops the *work*.
    pub fn realise(self, deadline: &Deadline) -> Result<DocumentSnapshot, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.grammar)
            .map_err(|source| ParseError::GrammarRejected {
                language_id: self.language_id,
                source,
            })?;

        let base = self.base.map(|(mut tree, edits)| {
            // A private clone, edited to match `text` before it is used as a
            // starting point. The seed's copy is the caller's and is not ours
            // to mutate in place.
            for edit in edits.iter() {
                tree.edit(edit);
            }
            tree
        });

        let text = &self.text;
        let mut read = |offset: usize, _position: Point| {
            // `chunks_in_range` slices the first chunk to the range, so this
            // is the text *starting at* `offset`, which is what tree-sitter's
            // callback contract asks for. An offset at the end of the
            // document yields no chunk, and the empty slice ends the parse.
            text.chunks_in_range(offset..text.len())
                .next()
                .unwrap_or("")
        };
        // Set from inside the callback rather than re-read afterwards: a
        // deadline that expires between the parse returning and the check
        // would otherwise be reported as an expiry that did not stop
        // anything, and the two are different facts about the same query.
        let mut abandoned = false;
        let mut progress = |_state: &ParseState| {
            if deadline.expired() {
                abandoned = true;
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let tree = parser.parse_with_options(
            &mut read,
            base.as_ref(),
            Some(ParseOptions::new().progress_callback(&mut progress)),
        );

        let tree = match (tree, abandoned) {
            (Some(tree), _) => tree,
            (None, true) => return Err(HandlerError::DeadlineExpired.into()),
            (None, false) => {
                return Err(ParseError::NoTree {
                    uri: self.uri.clone(),
                }
                .into());
            }
        };

        Ok(DocumentSnapshot {
            uri: self.uri,
            text: self.text,
            version: self.version,
            language_id: self.language_id,
            tree,
        })
    }
}

impl DocumentSnapshot {
    /// Infallible: it is a field, and it was produced from `text`.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}
