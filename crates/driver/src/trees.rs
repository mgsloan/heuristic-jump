//! `core`'s parse cache. `design/core.md` §2: `core` builds seeds and never
//! realises one, the worker parses, and what comes back is cached so the next
//! query on that document starts warm.
//!
//! The cache is the *only* consumer of a `Parsed`, and a `Parsed` is the only
//! thing that writes to it. Between them that closes §2's loop as a type
//! property: a seed leaves here, `dispatch` turns it into a tree, and the tree
//! can go nowhere but back here.
//!
//! What is missing is the channel. `shim.md` §10's worker pool and the
//! single-threaded actor that would own this do not exist yet, so `Parsed`
//! travels by return value rather than by `send`. That is the wiring; the
//! ownership is already right — nothing here is shared, nothing is locked, and
//! `TreeCache` needs `&mut self` to change, which only its owner has.

use rustc_hash::FxHashMap;
use shared::{
    DocumentUri, DocumentVersion, InputEdit, Language, LanguageId, Rope, SnapshotSeed, Tree,
};
use std::sync::Arc;

use crate::dispatch::Parsed;

/// One tree per open document, at the newest version anybody has parsed.
#[derive(Debug, Default)]
pub struct TreeCache {
    trees: FxHashMap<DocumentUri, Cached>,
}

#[derive(Debug)]
struct Cached {
    version: DocumentVersion,
    tree: Tree,
}

/// What `core` knows about an open document at the moment a query arrives —
/// everything a seed needs except the tree, which is what the cache adds.
///
/// The open-document map that will own this is `shim.md` §5's and does not
/// exist; this is the row of it that `core.md` §2 needs, passed rather than
/// stored so that the cache holds trees and nothing else.
#[derive(Debug)]
pub struct OpenDocument<'a> {
    pub uri: &'a DocumentUri,
    pub text: &'a Rope,
    pub version: DocumentVersion,
    pub language_id: LanguageId,
    /// From `LanguageHandler::grammar`, which is how `driver` parses every
    /// registered language without depending on a grammar crate (`core.md`
    /// §1).
    pub grammar: Language,
    /// The edits applied since the *cached* tree was parsed, shared by `Arc`
    /// rather than copied. Ignored when nothing is cached, since a full parse
    /// has nothing to reconcile.
    pub edits: &'a Arc<Vec<InputEdit>>,
}

impl TreeCache {
    /// The seed `core` hands a worker: incremental when this document has a
    /// cached tree, fresh otherwise.
    ///
    /// O(1) either way — `Rope::clone` shares structure and `Tree::clone` is a
    /// refcount increment — which is what lets `core` build one without
    /// leaving its O(1) budget (`core.md` §2).
    ///
    /// This is the only place a cached tree is read, and it leaves inside a
    /// seed rather than on its own. A stale tree therefore cannot reach a
    /// handler: `SnapshotSeed` keeps it in `base` and `realise` is the only
    /// way across.
    pub fn seed(&self, document: &OpenDocument<'_>) -> SnapshotSeed {
        match self.trees.get(document.uri) {
            Some(cached) => SnapshotSeed::incremental(
                document.uri.clone(),
                document.text.clone(),
                document.version,
                document.language_id,
                document.grammar.clone(),
                cached.tree.clone(),
                Arc::clone(document.edits),
            ),
            None => SnapshotSeed::fresh(
                document.uri.clone(),
                document.text.clone(),
                document.version,
                document.language_id,
                document.grammar.clone(),
            ),
        }
    }

    /// The cache's only write, and it consumes a value only `dispatch` can
    /// mint. There is no `insert(uri, tree)`, deliberately: a tree that no
    /// dispatch produced has no version anybody checked, and `seed` would hand
    /// it out as a base for edits it does not match.
    ///
    /// An older tree than the one held is dropped rather than stored. Two
    /// workers can be realising seeds for the same document at once — that is
    /// the whole point of dispatching in parallel — and the one that finishes
    /// last is not the one parsed from the newest text. Overwriting would
    /// leave `base` at a version the edit log no longer describes, and
    /// tree-sitter's incremental parse is only correct when the edits handed
    /// to it are.
    pub fn insert(&mut self, parsed: Parsed) {
        let (uri, version, tree) = parsed.into_parts();
        if let Some(cached) = self.trees.get(&uri)
            && cached.version >= version
        {
            tracing::debug!(
                %uri,
                cached = cached.version.0,
                arriving = version.0,
                "dropping a tree older than the cached one"
            );
            return;
        }
        self.trees.insert(uri, Cached { version, tree });
    }

    /// The version of the cached tree, for a document that has one. `core`
    /// needs it to know which edits are still outstanding; nothing else here
    /// exposes what is cached, and the tree itself never leaves except inside
    /// a seed.
    pub fn version(&self, uri: &DocumentUri) -> Option<DocumentVersion> {
        self.trees.get(uri).map(|cached| cached.version)
    }

    /// `didClose`, and the document map dropping a row. A cache that only ever
    /// grows is a leak with a slow fuse in a process that outlives many
    /// editors' worth of open files.
    pub fn forget(&mut self, uri: &DocumentUri) {
        self.trees.remove(uri);
    }
}
