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

use std::sync::Arc;

use rope::Rope;
use tree_sitter::{InputEdit, Language, Parser, Point, Tree};

use crate::error::{Error, ParseError};
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

    /// Reparses incrementally from `base`, or parses from scratch if there is
    /// none. Called by the worker, never by `core`.
    ///
    /// An unparseable document therefore fails at dispatch and never reaches a
    /// handler, which is what makes `DocumentSnapshot::tree` infallible.
    pub fn realise(self) -> Result<DocumentSnapshot, Error> {
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
        // `ParseOptions` carries tree-sitter's own progress callback, which is
        // where a deadline check belongs once a parse is long enough to need
        // one. Nothing measures that yet.
        let tree = parser
            .parse_with_options(&mut read, base.as_ref(), None)
            .ok_or(ParseError::NoTree {
                uri: self.uri.clone(),
            })?;

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
