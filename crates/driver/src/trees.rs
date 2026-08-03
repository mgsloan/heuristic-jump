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
use shared::{DocumentUri, DocumentVersion, InputEdit, Language, SnapshotSeed, Tree};
use std::sync::Arc;

use crate::dispatch::Parsed;
use crate::documents::Trusted;

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
/// The row itself belongs to `Documents`, which is `shim.md` §5's map; this is
/// that row plus the two things the map has no business holding — a grammar,
/// which comes from the registry, and the edit log, which is the parse cache's
/// business.
///
/// **The fields are private and `new` takes a [`Trusted`].** That is
/// `core.md` §8.6's rule made structural rather than checked: a `Trusted` is
/// only obtainable from `Documents::query`, which produces none for an
/// untrusted document, and this is the only way to a `SnapshotSeed` and
/// therefore the only way to `dispatch`. A query against a document we have
/// stopped believing does not abstain because something remembered to check —
/// it cannot be built.
#[derive(Debug)]
pub struct OpenDocument<'a> {
    document: Trusted<'a>,
    /// From `LanguageHandler::grammar`, which is how `driver` parses every
    /// registered language without depending on a grammar crate (`core.md`
    /// §1).
    grammar: Language,
    /// The edits applied since the *cached* tree was parsed, shared by `Arc`
    /// rather than copied. Ignored when nothing is cached, since a full parse
    /// has nothing to reconcile.
    edits: &'a Arc<Vec<InputEdit>>,
}

impl<'a> OpenDocument<'a> {
    pub fn new(document: Trusted<'a>, grammar: Language, edits: &'a Arc<Vec<InputEdit>>) -> Self {
        Self {
            document,
            grammar,
            edits,
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        self.document.uri()
    }

    pub fn version(&self) -> DocumentVersion {
        self.document.version()
    }
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
        let believed = &document.document;
        match self.trees.get(believed.uri()) {
            Some(cached) => SnapshotSeed::incremental(
                believed.uri().clone(),
                believed.text().clone(),
                believed.version(),
                believed.language_id(),
                document.grammar.clone(),
                cached.tree.clone(),
                Arc::clone(document.edits),
            ),
            None => SnapshotSeed::fresh(
                believed.uri().clone(),
                believed.text().clone(),
                believed.version(),
                believed.language_id(),
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
