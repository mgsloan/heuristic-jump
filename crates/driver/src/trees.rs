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
//! every method takes `&mut self`, which only its owner has.
//!
//! The bound is `deps.md` §8's: `lru`, wrapped, because `shim.md` §5 wants the
//! cache bounded by *both* entry count and total bytes and `lru` bounds only
//! entries.

use std::num::NonZeroUsize;

use lru::LruCache;
use shared::{
    ByteLen, DocumentUri, DocumentVersion, Hasher, InputEdit, Language, Map, SnapshotSeed, Tree,
};
use std::sync::Arc;

use crate::dispatch::Parsed;
use crate::documents::Trusted;

/// `shim.md` §5's key for an open document, and `deps.md` §8's reason for
/// writing it down: "a map keyed by our own types, not by attacker-controlled
/// strings". The disk-file half — `(path, mtime, len)`, which `didSave` lets
/// take over — has no cache to be a key of yet, because there is no per-query
/// read cache at all (`conformance-005`, accepted).
///
/// A version in the key rather than beside the tree, which is what makes
/// §5's "entries are immutable once inserted" structural: a v3 tree and a v5
/// tree are different entries, and the v3 one is superseded by ageing out
/// rather than by being overwritten.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct ParseKey {
    uri: DocumentUri,
    version: DocumentVersion,
}

/// How many trees the cache holds at once, which `lru` bounds on its own.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CacheEntries(NonZeroUsize);

/// The ceiling `lru` does not have. Both bounds are needed rather than either
/// one: entries alone lets a handful of generated files hold hundreds of
/// megabytes, and bytes alone lets a million tiny trees accumulate.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct CacheBytes(ByteLen);

impl CacheEntries {
    pub const fn new(count: NonZeroUsize) -> Self {
        Self(count)
    }
}

impl CacheBytes {
    pub const fn new(bytes: ByteLen) -> Self {
        Self(bytes)
    }
}

/// Neither number is measured, and `CLAUDE.md`'s posture says so: no new
/// caching or indexing until the corpus harness shows a change is worth it.
/// What is being fixed here is that the cache had no bound at all, so these are
/// a first ceiling rather than a tuned one — high enough that an editing
/// session on a normal project never reaches them, low enough that a directory
/// of generated files cannot take the process down.
const DEFAULT_ENTRIES: CacheEntries = CacheEntries(match NonZeroUsize::new(64) {
    Some(count) => count,
    None => NonZeroUsize::MIN,
});
const DEFAULT_BYTES: CacheBytes = CacheBytes(ByteLen(64 * 1024 * 1024));

/// An LRU of tree-sitter trees, keyed by `(uri, version)` and bounded twice.
///
/// Holding it costs `core` nothing: every value in it is a refcounted handle,
/// and the expensive half — parsing — happened in a worker (`shim.md` §5).
pub struct TreeCache {
    trees: LruCache<ParseKey, Cached, Hasher>,
    /// The newest version cached per open document, which is what `seed` has
    /// to know to build a key at all. `core` will hold this itself once
    /// `shim.md` §5's `Document` row exists, where it is `parsed_at`.
    ///
    /// An entry here can outlive its tree — eviction removes the tree and the
    /// row together, so what is left is a document with an older tree still
    /// cached and no way to name it. That is a cold miss, and §5 is explicit
    /// that the cache is a cache: "cold misses are correct, just slower".
    newest: Map<DocumentUri, DocumentVersion>,
    bytes: ByteLen,
    ceiling: CacheBytes,
}

impl std::fmt::Debug for TreeCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TreeCache")
            .field("entries", &self.trees.len())
            .field("bytes", &self.bytes)
            .field("ceiling", &self.ceiling)
            .finish()
    }
}

impl Default for TreeCache {
    fn default() -> Self {
        Self::new(DEFAULT_ENTRIES, DEFAULT_BYTES)
    }
}

#[derive(Debug)]
struct Cached {
    tree: Tree,
    /// The text the tree was parsed from, which is what the byte ceiling
    /// counts. A `Tree` exposes no size of its own, and the source length is
    /// the quantity `shim.md` §5 is worried about — "a single generated file
    /// can be enormous" is a statement about the file.
    bytes: ByteLen,
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
    pub fn new(entries: CacheEntries, ceiling: CacheBytes) -> Self {
        Self {
            trees: LruCache::with_hasher(entries.0, Hasher::default()),
            newest: Map::default(),
            bytes: ByteLen::ZERO,
            ceiling,
        }
    }

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
    ///
    /// `&mut self` because reading an LRU *is* a write: a `get` that did not
    /// promote the entry would leave the eviction order recording the last
    /// time each tree was parsed rather than the last time one was wanted,
    /// which is the opposite of what the bound is for.
    pub fn seed(&mut self, document: &OpenDocument<'_>) -> SnapshotSeed {
        let believed = &document.document;
        let key = self.newest.get(believed.uri()).map(|version| ParseKey {
            uri: believed.uri().clone(),
            version: *version,
        });
        match key.and_then(|key| self.trees.get(&key)) {
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
        let (uri, version, bytes, tree) = parsed.into_parts();
        if let Some(cached) = self.newest.get(&uri)
            && *cached >= version
        {
            tracing::debug!(
                %uri,
                cached = cached.0,
                arriving = version.0,
                "dropping a tree older than the cached one"
            );
            return;
        }

        let key = ParseKey {
            uri: uri.clone(),
            version,
        };
        self.bytes = ByteLen(self.bytes.0.saturating_add(bytes.0));
        if let Some((evicted, cached)) = self.trees.push(key, Cached { tree, bytes }) {
            self.uncount(&evicted, &cached);
        }
        self.newest.insert(uri, version);

        // §8's wrapper, in the words the section uses: "track a running byte
        // total, and after each `put`, `pop_lru` until under the byte ceiling."
        // Bounded by the cache emptying, so a single file larger than the whole
        // ceiling evicts itself and everything else rather than looping.
        while self.bytes > self.ceiling.0 {
            let Some((evicted, cached)) = self.trees.pop_lru() else {
                break;
            };
            self.uncount(&evicted, &cached);
        }
    }

    /// Both halves of dropping an entry: the byte total it was counted in, and
    /// the `newest` row that named it. Split out because eviction happens in
    /// two places and the two accounts have to move together — a byte total
    /// that drifts up makes the cache evict everything forever, and one that
    /// drifts down makes the ceiling stop being a ceiling.
    fn uncount(&mut self, evicted: &ParseKey, cached: &Cached) {
        self.bytes = ByteLen(self.bytes.0.saturating_sub(cached.bytes.0));
        if self.newest.get(&evicted.uri) == Some(&evicted.version) {
            self.newest.remove(&evicted.uri);
        }
    }

    /// The version of the cached tree, for a document that has one. `core`
    /// needs it to know which edits are still outstanding; nothing else here
    /// exposes what is cached, and the tree itself never leaves except inside
    /// a seed.
    pub fn version(&self, uri: &DocumentUri) -> Option<DocumentVersion> {
        self.newest.get(uri).copied()
    }

    /// `didClose`, and the document map dropping a row. A cache that only ever
    /// grows is a leak with a slow fuse in a process that outlives many
    /// editors' worth of open files — which the two bounds now hold on their
    /// own, but bounded is not the same as freed: an editor that closes a file
    /// should not be paying for it until sixty-three others push it out.
    ///
    /// Every version of the document, not just the newest: `(uri, version)`
    /// keys mean a document that was parsed at v3 and again at v5 has two
    /// entries, and `didClose` ends both.
    pub fn forget(&mut self, uri: &DocumentUri) {
        let stale: Vec<ParseKey> = self
            .trees
            .iter()
            .map(|(key, _)| key)
            .filter(|key| &key.uri == uri)
            .cloned()
            .collect();
        for key in stale {
            if let Some(cached) = self.trees.pop(&key) {
                self.bytes = ByteLen(self.bytes.0.saturating_sub(cached.bytes.0));
            }
        }
        self.newest.remove(uri);
    }
}
