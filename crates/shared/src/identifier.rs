//! `design/core.md` §9's "one function in `shared`, not two implementations
//! that agree": whether a position is on an identifier at all.
//!
//! Two consumers need the answer and they must not be able to differ.
//! `measure_core` enumerates corpus positions with it
//! (`data-collection.md` §2) and a language handler answers
//! `AbstainReason::NotAnIdentifier` with it. If those disagree, the corpus
//! holds positions the tool does not consider queries, or the reverse, and the
//! resulting miscount reads as a resolution failure rather than a definitional
//! one — which is a failure nothing downstream can name.
//!
//! So both entry points here delegate to the same private predicate, and it is
//! the only thing that decides. The rule is `data-collection.md` §2's, and it
//! is **language-agnostic on purpose**: a named leaf node whose entire text is
//! identifier-shaped. A per-language list of node kinds (`identifier`,
//! `type_identifier`, `field_identifier`, …) is exactly the per-language
//! configuration format `resolution.md` §1.2 refuses.

use std::fmt;

use rope::{Offset, Rope};
use tree_sitter::{Node, TreeCursor};

use crate::document::DocumentSnapshot;

/// The handler's side: is the cursor on an identifier?
///
/// The node is returned rather than a `bool` because every caller that wants
/// the answer also wants the token — its text to search for, its range to
/// build a [`crate::Location`] from.
pub fn identifier_at(doc: &DocumentSnapshot, at: Offset) -> Option<Node<'_>> {
    let node = doc
        .tree()
        .root_node()
        .named_descendant_for_byte_range(at.0, at.0)?;
    is_identifier(&node, &doc.text).then_some(node)
}

/// `measure_core`'s side: every identifier in the document, in source order.
///
/// `data-collection.md` §2 records the identifier's *start* offset, so a
/// sampled position is always `node.start_byte()` and never an interior one —
/// a cursor may sit anywhere inside a token and the handler must behave
/// identically, but that invariance is a property test rather than something
/// to spend corpus positions re-measuring.
pub fn identifiers(doc: &DocumentSnapshot) -> Identifiers<'_> {
    Identifiers {
        cursor: doc.tree().walk(),
        text: &doc.text,
        exhausted: false,
    }
}

pub struct Identifiers<'a> {
    cursor: TreeCursor<'a>,
    text: &'a Rope,
    exhausted: bool,
}

// By hand because `tree_sitter::TreeCursor` has no `Debug`, and the position
// in the walk is what anyone printing this wants anyway.
impl fmt::Debug for Identifiers<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Identifiers")
            .field("at", &self.cursor.node().start_byte())
            .field("exhausted", &self.exhausted)
            .finish()
    }
}

impl<'a> Iterator for Identifiers<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.exhausted {
            let node = self.cursor.node();
            self.advance();
            if is_identifier(&node, self.text) {
                return Some(node);
            }
        }
        None
    }
}

impl Identifiers<'_> {
    /// Pre-order: down, then across, then up until there is somewhere across
    /// to go. Anonymous nodes are walked through rather than skipped, since a
    /// named leaf can hang below one.
    fn advance(&mut self) {
        if self.cursor.goto_first_child() {
            return;
        }
        loop {
            if self.cursor.goto_next_sibling() {
                return;
            }
            if !self.cursor.goto_parent() {
                self.exhausted = true;
                return;
            }
        }
    }
}

/// The whole rule, in one place.
///
/// **Named** excludes keywords and punctuation for free in most grammars,
/// because those are anonymous tokens — the grammar already drew this line.
/// **Leaf** — no named children — is what makes it a token rather than a
/// construct containing one. **Identifier-shaped text** catches what the first
/// two miss: grammars that make `self`, `true` and `super` named nodes, and
/// named leaves that are literals or comments. A string, a number and
/// `// note` all fail the shape test; `self` passes and is kept, because
/// go-to-definition on `self` is a real query with a real answer.
fn is_identifier(node: &Node<'_>, text: &Rope) -> bool {
    if !node.is_named() || node.named_child_count() != 0 {
        return false;
    }

    let mut seen = false;
    for chunk in text.chunks_in_range(node.start_byte()..node.end_byte()) {
        for scalar in chunk.chars() {
            let shaped = if seen {
                identifier_continue(scalar)
            } else {
                identifier_start(scalar)
            };
            if !shaped {
                return false;
            }
            seen = true;
        }
    }
    seen
}

/// The same rule against text rather than a node, for the two callers that
/// have no tree: [`crate::ScanRequest`] refusing a literal that is not an
/// identifier, and the word-boundary test that decides whether a byte match is
/// a whole token.
///
/// It shares the per-character predicates rather than restating them, for the
/// reason at the top of this module: two implementations that agree today are
/// a definitional disagreement waiting to be measured as a resolution one.
pub(crate) fn is_identifier_text(text: &str) -> bool {
    let mut characters = text.chars();
    characters.next().is_some_and(identifier_start) && characters.all(identifier_continue)
}

pub(crate) fn identifier_start(scalar: char) -> bool {
    scalar.is_alphabetic() || scalar == '_'
}

pub(crate) fn identifier_continue(scalar: char) -> bool {
    scalar.is_alphanumeric() || scalar == '_'
}
