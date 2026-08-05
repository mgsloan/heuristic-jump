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

use rope::{ByteRange, Offset, Point as RopePoint, Rope};
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
            text.chunks_in_range(ByteRange::new(Offset(offset), Offset::ZERO + text.len()))
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
            // Nothing had classified anything: this is the expiry that happens
            // *before* there is a handler, which is why it is the one route
            // `core-025`'s option C cannot empty.
            (None, true) => return Err(HandlerError::expired_unclassified().into()),
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

/// The [`InputEdit`] describing one replacement, computed from the text *as it
/// was before* the replacement.
///
/// This is the other half of [`SnapshotSeed::incremental`], and it is here
/// rather than at each call site because it is the conversion with no second
/// chance. tree-sitter's incremental reparse is only correct when the edits it
/// is handed are, and an edit that is wrong does not fail: it produces a tree
/// that is confidently wrong about a document that parses, which `core.md` §10
/// names as the failure the snapshot invariant exists to catch. Every field
/// below is a place to put a row where a column goes, and there are six of
/// them.
///
/// `before` is required rather than convenient: `start_position` and
/// `old_end_position` are row-and-column in the *old* text, so a caller that
/// replaced first and converted afterwards would be converting against a
/// document where those offsets mean something else.
///
/// The new end is computed from `inserted` alone, without the text that
/// results. That is exact — a replacement moves nothing before it, and what
/// follows is what `inserted` itself contains — and it is what lets an edit be
/// recorded at the moment the change is applied rather than after.
pub fn input_edit(before: &Rope, replaced: ByteRange, inserted: &str) -> InputEdit {
    let start = before.offset_to_point(replaced.start);
    let old_end = before.offset_to_point(replaced.end);
    InputEdit {
        start_byte: replaced.start.0,
        old_end_byte: replaced.end.0,
        new_end_byte: replaced.start.0 + inserted.len(),
        start_position: point(start),
        old_end_position: point(old_end),
        new_end_position: advanced(start, inserted),
    }
}

fn point(point: RopePoint) -> Point {
    Point {
        row: widen(point.row.0),
        column: widen(point.column.0),
    }
}

/// Where `inserted` ends, starting from `start`. A column is a byte offset
/// within its row in both `rope`'s point and tree-sitter's, so the last line's
/// length in bytes is the column and no encoding enters into it.
fn advanced(start: RopePoint, inserted: &str) -> Point {
    match inserted.rfind('\n') {
        None => Point {
            row: widen(start.row.0),
            column: widen(start.column.0) + inserted.len(),
        },
        Some(last) => Point {
            row: widen(start.row.0) + inserted.matches('\n').count(),
            column: inserted.len() - last - 1,
        },
    }
}

/// `u32` to `usize` without an `as`, which the workspace denies for the reason
/// `core.md` §3 gives. `usize: From<u32>` does not exist — 16-bit targets —
/// and saturating is unreachable on any target this builds for.
fn widen(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
