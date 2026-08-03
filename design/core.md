# Core needed for measurement

This covers the parts of the system that exist **before there is a shim**:
everything `measure_core` and `measure_<lang>` need in order to run a language
handler against a corpus and score it. It is
[`phases.md`](phases.md)'s phase 1a.

* The handler seam — `LanguageHandler`, `Query`, `Outcome`, and the vocabulary
  newtypes every crate speaks in.
* `DocumentSnapshot`, position encoding, and `ProjectView`: what a handler is
  handed, and the rules about what it may reach.
* The agreement predicate, the trace record, and the corpus scan itself.
* The hand-written LSP wire types, the workspace layout, and the vendoring.

**The LSP shim is [`shim.md`](shim.md)** — the proxy, the actor, message
routing, server health, the retry protocol, divergence reporting, dispatch,
and standalone mode. That is phase 2b, and none of it is needed to measure a
handler. The split is not cosmetic: `measure_core` depends on `shared` and
never on `driver` ([section 9](#the-dependency-graph)), so a language can be
measured a whole phase before a shim exists, and every rule in this document
is one both programs obey.

Out of scope for both: the resolution logic itself, which sits behind the
handler interface below. That is `resolution.md`.

See `high-level.md` for the product rationale and the success metrics. Two of
its decisions constrain almost everything here — precision is *measured*
rather than enforced in v1, and abstention is a normal outcome rather than a
failure — and the plumbing that would let a precision floor be set later from
measurements is built now, deliberately, before anything gates on it.

## 1. Handler interface

The seam this document commits to; everything behind it is out of scope here.
Per `high-level.md`, dispatch is direct — no framework, no config format that
languages must be expressed in.

This trait lives in `shared`, which is deliberately *not* `driver`. See
[section 9](#9-workspace-layout) for why that separation matters.

### Vocabulary types

`shared` newtypes the primitives rather than passing bare integers and
strings across the seam. Almost every value here is an offset, an index, or an
identifier, and those are exactly the things that silently substitute for each
other.

**The text-shaped ones are `rope`'s, not `shared`'s.** `ByteOffset`,
`ByteLen`, `ByteRange`, and `LineIndex` are *defined in* the vendored rope and
re-exported here, because `shared` depends on `rope` and the dependency cannot
run the other way — `rope-modifications.md` §2 has the argument, and the same
goes for `ByteColumn`, `Utf16Column`, and `CharCount`, which handlers do not
use. Every other crate says `shared::ByteOffset` and never has to know. They
appear here because this is the seam they are part of:

```rust
// vendor/rope, re-exported by shared
pub struct ByteOffset(pub usize);   // a position; never a UTF-16 offset
pub struct ByteLen(pub usize);      // a quantity, distinct from a position
pub struct ByteRange { pub start: ByteOffset, pub end: ByteOffset }
pub struct LineIndex(pub u32);      // zero-based line

impl ByteRange {
    pub fn contains(self, at: ByteOffset) -> bool;
    pub fn overlaps(self, other: ByteRange) -> bool;
}

// shared, as an extension trait: `shifted_by` needs tree-sitter's InputEdit,
// and rope must not grow a tree-sitter dependency for one method.
pub trait ByteRangeExt {
    /// Shift by an edit's length delta; None if the edit fell inside,
    /// which invalidates the range. Used for spot anchoring, shim.md section 7.
    fn shifted_by(self, edit: &InputEdit) -> Option<ByteRange>;
}

// shared's own
/// LSP document version, from didOpen/didChange.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentVersion(pub i32);

/// Interned LSP `languageId`. Only ids some registered handler declared
/// exist, so an unknown language cannot be constructed at all.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct LanguageId(&'static str);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FileExtension(&'static str);

/// Normalized document URI, so URI comparison is not string comparison
/// with percent-encoding and case rules smuggled in. Normalization happens
/// during deserialization, not afterwards -- see section 8.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocumentUri(Url);

/// Request id as it arrived from the editor, number or string, stored in
/// normalized text form so the fast peek path (section 3.1) and the
/// serde_json path produce the same key. Distinct from the shim's own
/// outgoing ids, which cannot be confused with it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EditorRequestId(Box<str>);

/// A definition site, as handlers speak of it: byte offsets, always.
/// The wire form is `proto::WireLocation` and only the driver builds it.
///
/// `line` is redundant with `range` but is not encoding: it is row plus
/// byte-range, still entirely byte-space. It is carried because a handler
/// gets it for free from the tree-sitter node it already verified, and it
/// saves the driver a whole-file line index later -- see section 8.4.
/// Constructed only via `Location::at_node`, so the two cannot disagree.
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: DocumentUri,
    pub range: ByteRange,
    pub line: LineIndex,
}

/// Invariant: 0.0..=1.0, enforced by the constructor.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Confidence(f32);
```

These are the *deserialization targets*, not wrappers applied after the fact.
That is the whole reason [section 8](#8-protocol-types) exists.

### The trait

```rust
pub trait LanguageHandler: Send + Sync {
    /// LSP `languageId` values, for open documents.
    fn language_ids(&self) -> &'static [LanguageId];

    /// File extensions, for candidate files found by search. Closed files
    /// arrive as a bare path with no languageId attached.
    fn file_extensions(&self) -> &'static [FileExtension];

    /// The tree-sitter grammar, supplied at runtime so that `driver` can
    /// maintain its parse cache without depending on any grammar crate.
    fn grammar(&self) -> tree_sitter::Language;

    fn goto_definition(&self, q: &Query<'_>) -> Outcome;
}

pub struct Query<'a> {
    pub doc: &'a DocumentSnapshot,       // rope + tree, immutable
    pub position: ByteOffset,
    /// Scoped reads, parses, and search execution -- see below.
    pub project: &'a ProjectView,
    pub deadline: &'a Deadline,
    pub server: &'a ServerProfile,       // which oracle we are standing in for
    /// The commit decision. Inert in v1; the only way to build an Outcome.
    pub policy: &'a CommitPolicy,
}

/// The behavioural differences between language servers for one language,
/// as observed rather than predicted -- see section 7. Empty in v1: a
/// field appears only once the corpus shows a systematic divergence that
/// a field would fix.
pub struct ServerProfile {
    /// `None` in standalone, and when proxying a server we have no profile
    /// for. A handler that branches on this is doing something wrong -- see
    /// below -- but the absence has to be representable, because the two
    /// modes are not the same situation and a synthesised identity would
    /// hide that.
    pub id: Option<ServerId>,
}

/// Interned server identity, resolved from the child's command name at
/// startup.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ServerId(&'static str);

pub enum Outcome {
    Committed {
        locations: Vec<Location>,
        confidence: Confidence,
        stratum: Stratum,
    },
    Abstain { reason: AbstainReason, stratum: Stratum },
}

/// One per row of `high-level.md`'s stratification list, plus a placeholder.
/// What each means, and how a query is assigned one, is `resolution.md` §8.
pub enum Stratum {
    LocalBinding,
    SameFileModule,
    ExplicitImport,
    WildcardImport,
    AmbiguousName,
    ExternalDependency,
    MacroGenerated,
    TypeInferenceRequired,
    /// The language crate template, unmodified. No real handler may return
    /// this -- see section 9. Its presence in a metrics table means the
    /// template has not been replaced, which is a gate check rather than
    /// something anybody has to notice.
    Unimplemented,
}

#[non_exhaustive]
pub enum AbstainReason {
    /// The cursor is not on a resolvable identifier.
    NotAnIdentifier,
    /// An identifier, but of a kind this language does not resolve.
    UnsupportedRole,
    /// Searched exhaustively, found nothing.
    NoCandidates,
    /// The deadline expired mid-search. The one latency-shaped abstention
    /// `high-level.md` allows, and the only reason here that is not a fact
    /// about the code.
    Deadline,
    /// The only plausible target is outside the workspace. Carries the name
    /// because standalone puts it in the error text (`shim.md` §8).
    External { name: Box<str> },
    NoParse,
    HandlerError,
}

/// Stratum -> minimum Confidence. Empty in v1, where `decide` returns
/// `Committed` for every input. Handlers never construct `Outcome::Committed`
/// themselves; every path ends here.
pub struct CommitPolicy { /* ... */ }

impl CommitPolicy {
    pub fn decide(&self, stratum: Stratum, confidence: Confidence,
                  locations: Vec<Location>) -> Outcome;
}

/// A file known to be inside a workspace root and not gitignored.
/// Private field, private constructor: only `ProjectView` mints one.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProjectPath(Arc<ProjectPathInner>);
```

Notes on the shape:

*  ** `Outcome` is not `Result`. ** Abstention is a normal, expected,
  frequently correct outcome —the query genuinely had nothing to return, or
  the deadline expired —and it should not share a type with "something went
  wrong." Under the future precision floor it also becomes the mechanism that
  holds the floor, which is a further reason not to model it as an error.
* **`AbstainReason` carries no resolution vocabulary.** Earlier revisions had
  `UnsupportedRole { role: ReferenceRole }` and `External { name: Namespace }`,
  which would have dragged two of `resolution.md`'s internal types into the
  seam — and `ReferenceRole`'s variant set is a claim about what kinds of
  reference exist, which is exactly the per-language decision
  [`resolution.md` §1.2] refuses to centralise. The variants are unit or carry
  primitives; the detail a handler knows stays in the handler, and reaches the
  metrics through the trace record rather than the seam.
*  ** `Stratum` is reported on both arms**, because coverage per stratum is
  meaningless without knowing which stratum the abstentions belonged to.
*  ** `Confidence` exists now** even though nothing compares it against a
  threshold in v1. It is recorded on every answer and never gates one. Two
  reasons it is not deferred along with the floor: `high-level.md` 's future
  work item on marking heuristic results with a probability estimate needs it,
  and —more importantly —a floor can only be *derived* from
  `(stratum, confidence, agreed?)` triples that were collected while nothing
  was being gated. Retrofitting a confidence notion into handlers written
  without one means revisiting every resolution path, and doing it after the
  fact means the first calibration is computed from answers the old code chose
  to give, which is not the same distribution. It is a newtype rather than a
  bare `f32` so the 0.0..=1.0 invariant is checked once in the constructor
  instead of assumed at every comparison —and so that a confidence can never
  be silently swapped with a score, a threshold, or a latency.
*  ** `LanguageId` and `FileExtension` are interned, not strings.** A handler
  declares its ids as consts; the driver resolves an incoming LSP `languageId`
  against the registry and gets `Option<LanguageId>`. Unknown languages fail
  to resolve at the boundary rather than travelling inward as a string that
  matches nothing, and lookup becomes pointer comparison.
*  **Handlers get a snapshot, not a lock.** `DocumentSnapshot` holds cloned
  `Rope` and `Tree` handles, both O(1), taken at dispatch —so a handler is
  immune to edits that arrive while it runs, and `core` is never blocked.
*  **Handlers do their own disk reads, parses, and searches through
  `ProjectView` **, so the driver can enforce the scope rules (workspace only,
  gitignore respected), cache reads within a query, reuse the parse LRU from
  [section 5](shim.md#5-document-state), and run the literal scan on the
  bounded pool from
  [section 10](shim.md#10-parallel-dispatch-and-resource-limits) rather than
  on threads a handler spawned. `read`, `parse`, and `scan` are all on it;
  `resolution.md` §3 has the full signature list.
*  ** `ProjectPath` is unforgeable, and that is what makes the scope rule true
  rather than customary.** A handler cannot build one from a string —every
  path it holds came from `ProjectView::candidates` or `::lookup`, both of
  which consult the `ignore` -crate file list from
  [section 4](#4-project-file-enumeration). Without this the rule is a
  convention every language author has to remember, and the one-line change
  that peeks at `~/.cargo/registry` would work, pass review, and quietly move
  the tool into a scope whose latency nothing has accounted for.
*  **Handlers never construct `Outcome::Committed`. ** Every path ends through
  `policy.decide(..)`. In v1 that returns `Committed` for every input, so the
  funnel is inert and buys nothing today; what it buys is that the claim in
  [section 14.6](shim.md#146-the-budgets-change-because-what-they-are-traded-against-changed)
  —a per-mode floor is a data change rather than a code change —is true when
  the floor arrives. The alternative is auditing every commit site in every
  `lang_*` crate at the moment when there are the most of them, and
  half-adopting it is worse than either choice. `resolution.md` §7.4 argues it
  at length.
*  **Handlers are `Send + Sync` and re-entrant.** The same handler serves
  concurrent queries; per-query mutable state lives in locals.
*  ** `grammar()` is what keeps `driver` language-free.** The driver needs to
  parse —for the parse cache in [section 5](shim.md#5-document-state) and the
  token-span check in [section 7](shim.md#7-go-to-definition-lifecycle) —but
  `tree_sitter::Language` is a runtime value, so the grammar arrives through
  the registry rather than through a `tree-sitter-<lang>` build dependency.
  Without this, `driver` would have to depend on every language crate, which
  is exactly the edge the workspace layout forbids.
*  ** `ProjectView` is a concrete struct in `shared`, not a trait in `driver`.
  ** An earlier revision put the trait in `shared` and the implementation in
  `driver`, on the grounds that the file list cache and the scope rules live
  there. Both halves of that were wrong.

  *Not in `driver`*, because `measure_core` needs one too, and it must be the
  same one. Scope rules — workspace roots, gitignore, the `..` escape check —
  decide which candidates a search can find at all, so a second implementation
  in the measurement path would mean the corpus scores a tool that is not the
  one that ships. [Section 7](#7-observability-and-the-corpus-scan) makes
  that argument for snapshot construction already; it applies here with more
  force, and it is what forced the move: under
  [`phases.md`](phases.md) the measurement
  binaries exist a whole phase before `driver` does.

  *Not a trait*, because there is exactly one implementation and no prospect
  of a second. The variation a trait would buy is variation we specifically do
  not want. The plausible-sounding second implementations do not survive
  inspection: an in-memory one for tests is ruled out by `CLAUDE.md` 's
  no-unit-tests rule, since fixtures are real directories on disk; standalone
  and proxy share scope rules exactly; and multi-root ordering
  ([`open-questions.md`](open-questions.md) question 8) is configuration, not
  polymorphism. A trait with one impl on a per-file-read hot path is a vtable
  and an indirection bought with a guess about the future.

  Its dependency on `ignore` moves to `shared` with it —see `deps.md` §7.
*  ** `ServerProfile` is data, not a trait, and it is distinct from
  `ServerAdapter`. ** Two different things are keyed by the same server
  identity and it is worth keeping them apart: `ServerAdapter`
  ([section 6](shim.md#6-server-health-model)) lives in `driver` and
  interprets a server's progress notifications for the health model;
  `ServerProfile` lives in `shared` and tells a handler what that server
  considers a definition. The first never reaches a handler; the second never
  reaches the health model. They share only `ServerId`.

  It is data rather than a trait because handlers must not dispatch on server
  *identity* — `if server.id == PYRIGHT` scattered through a handler is the
  per-language configuration format `resolution.md` §1.2 rules out, wearing yet
  another hat.
  A handler reads a field describing a behaviour; it does not ask which
  server it is talking to.

## 2. Document snapshots

The immutable view of one document that a handler is given. `driver` builds
one at dispatch and `measure_core` builds one per recorded position; both go
through the same constructor in `shared`, which is what keeps the corpus
scoring the code that ships (see
[section 7](#7-observability-and-the-corpus-scan)).

### Snapshots are O(1)

Snapshot-on-dispatch is only viable because nothing is copied:

```rust
pub struct DocumentSnapshot {
    pub text: Rope,                  // structural sharing; O(1)
    pub version: DocumentVersion,    // the version above
    pub language_id: LanguageId,
    /// Cached tree at some older version, plus the edits that bring it
    /// up to `version`. Never handed to handlers directly.
    base: Option<(Tree, Arc<Vec<InputEdit>>)>,
    grammar: tree_sitter::Language,
    parsed: OnceLock<Tree>,
}

impl DocumentSnapshot {
    /// A tree for exactly `self.version`. Reparses incrementally from
    /// `base` on first call, or parses from scratch if there is none.
    pub fn tree(&self) -> Result<&Tree, ParseError>;
}
```

* `Rope::clone` shares structure through the sum tree.
* `Tree::clone` is `ts_tree_copy`, which is `ts_subtree_retain` plus a small
  wrapper allocation (`tree-sitter/src/tree.c:22`) — a refcount increment, not
  a node copy. The refcount uses `atomic_inc` (`src/subtree.c:561`), so
  handing a clone to another thread is exactly what the API is designed for.

So a snapshot is three refcount bumps and a struct move, regardless of file
size. The edit log is shared by `Arc` rather than copied for the same reason;
appending to it while a worker holds a snapshot copies the log via
`Arc::make_mut`, which is bounded by edits-since-last-parse and so is a
handful of small structs, never the document.

** `parsed` must be the thread-safe cell.** Handlers may fan out across
candidate files
([section 10](shim.md#10-parallel-dispatch-and-resource-limits)), which means
`&Query` —and therefore `&DocumentSnapshot` —crosses threads, which requires
`DocumentSnapshot: Sync`. So `parsed` is a `std::sync::OnceLock<Tree>`, not
the unsync variant. This works because tree-sitter declares `Tree` both `Send`
and `Sync` (`binding_rust/lib.rs:3908`); `Node<'tree>` is likewise `Sync`, so
nodes borrowed from a shared tree can be passed between fan-out workers.

### Text and tree can never disagree

The cached tree is usually *older* than the text: `core` caches a tree at v3,
the user types, and a query dispatches at v5. Handing a handler both the v5
text and the v3 tree would be a trap — every offset in that tree is wrong for
that text, and the mismatch is invisible until it produces a confidently
wrong answer.

So the stale tree is private. `base` holds it together with the edits that
reconcile it, and `tree()` is the only way to get one:

1. First call applies `edits_since_parse` to a **private clone** of the base
   tree via `Tree::edit`, then reparses against the v5 text with the edited
   tree as the starting point — a normal tree-sitter incremental parse.
2. With no base, it is a full parse.
3. The result is memoised, so a handler that asks repeatedly pays once.

**The handler cannot obtain a tree that does not match `text`.** The parse is
paid inside the worker and inside the deadline, never in `core`.

Getting the result back to `core` is explicit rather than implicit: the worker
owns the `DocumentSnapshot` for the duration of the query and hands it back at
the end, and the dispatch wrapper — not the handler — checks whether `parsed`
was filled and, if so, sends `Parsed { uri, version, tree }` to `core`. The
handler is not involved and cannot forget. `core` caches it, so the next query
on that document starts warm.

## 3. Position encoding

This is the highest-risk correctness detail in the whole driver.

LSP positions are UTF-16 code unit offsets by default. Tree-sitter works in
bytes. Every position crossing the boundary needs conversion, and the bugs it
produces are the worst kind: invisible on ASCII, wrong by a few columns on any
line containing a non-ASCII character, and therefore wrong mainly in comments,
string literals, and non-English codebases.

The vendored rope makes this materially safer than it would otherwise be:
`OffsetUtf16` and `PointUtf16` are first-class dimensions of its `TextSummary`
alongside bytes and `Point`, so a conversion is one sum-tree cursor seek
rather than a hand-rolled index. Below that, each 128-byte chunk carries a
`u128` bitmap of UTF-16 boundaries resolved by popcount, so in-chunk
conversion is nearly free. This was the main reason to vendor it — see
[section 9](#9-workspace-layout).

LSP 3.17 added negotiation: the client advertises `general.positionEncodings`
and the server picks one in `InitializeResult.positionEncoding`. Zed currently
advertises UTF-16 only (`crates/lsp/src/lsp.rs:793`), so with Zed the shim
will be doing conversion.

The rule: **the shim uses whatever the child negotiated, not what it would
prefer.** It is in the middle of a negotiation between two other parties and
does not get a vote on the outcome.

**Whether the shim is a party to that negotiation depends on the mode**, and
that is the one part of encoding that is not settled here: in proxy mode it is
a bystander and must not touch `initialize`
([shim.md section 4](shim.md#4-initialize-and-capability-negotiation)); in
standalone it answers `initialize` itself and picks UTF-8 when offered
([shim.md section 14.3](shim.md#143-initialize-is-ours-now)). Either way what
reaches this document is a settled `PositionEncoding`, set once and never
inferred.

Conversion lives in one module with exhaustive property tests against a
reference implementation, and every position is converted to byte offsets at
the edge so that no UTF-16 offset ever reaches the handler interface. That
last rule is enforced by the type system rather than by discipline:
`proto::WirePosition` has private fields and yields a `ByteOffset` only when
handed the negotiated encoding and the text
([section 8.3](#83-the-wire-position-type-is-inert)).

## 4. Project file enumeration

Whole-project search needs a file list, and this is the one place where "no
index" needs a stated boundary.

There is no *symbol* index — no persisted map from name to definition sites,
which is the thing `high-level.md` rules out and the thing that would cost
startup CPU and invalidation complexity. But a cold directory walk of a large
repository can take hundreds of milliseconds on its own, which would consume
the entire budget before a single file is read. So:

**The driver caches a file list. It does not cache anything about file
contents beyond the parse LRU.**

*  Built with the `ignore` crate, the same walker ripgrep uses, so
  `.gitignore` is respected for free. This directly implements `high-level.md`
  's decision that gitignored files are out of scope.
*  Built in-process rather than by shelling out to ripgrep: subprocess spawn
  plus pipe overhead is a meaningful fraction of a 50ms p50 target, and
  in-process gives direct control over cancellation at the deadline.
*  Built lazily on first need, then refreshed in the background. A stale list
  is acceptable —it costs recall on files created in the last few seconds,
  which is a miss, not a wrong answer, and misses are cheap under the
  measured-precision posture.
*  **In proxy mode, invalidated by the editor's watcher, for free.** The child
  registers file watching with the editor —`client/registerCapability` for
  `workspace/didChangeWatchedFiles` —and the editor's resulting notifications
  flow editor → child *through the shim*, which forwards them anyway. Teeing
  them to `core` costs one routing row ([`shim.md` §3](shim.md#3-message-routing))
  and is strictly better than watching ourselves: no descriptors, since the
  editor has already paid for them and editors do this at scale; and correct
  scoping for nothing, since the editor honours its own exclusions and the
  child's glob patterns, so `target/` and `node_modules/` never wake us. That
  is exactly the failure the next bullet is about.

  **The response is "mark stale", not "apply the delta".** `core` does only
  O(1) work per event ([`shim.md` §2](shim.md#thread-layout)) and one frame can
  carry thousands of events after a branch switch, so the payload is never
  read: any such frame sets the stale flag and schedules the same debounced
  rescan the `NoCandidates` path below uses. Registration ids and glob patterns
  are not tracked either, for the same reason — nothing here needs them, and
  `client/registerCapability` stays pure passthrough
  ([`shim.md` §3](shim.md#server-originated-requests-are-load-bearing)).

  It also catches the one thing the on-demand trigger structurally cannot:
  **deletions**. A rescan discovers files that appeared; a stale entry for a
  file that was removed only ever surfaces as a failed read.

  It is **opportunistic, and nothing depends on it.** A child that does not
  register file watching produces no events, and the on-demand path below is
  the backstop that always works — the same shape as a `ServerAdapter`
  ([`shim.md` §6](shim.md#per-server-adapters)): precision when present, never
  load-bearing. Whether the editors and servers we care about do send these
  frames is a question the golden corpus
  ([section 8.5](#85-the-untagged-unions-are-the-actual-risk)) answers rather
  than one to assume.
*  **A filesystem watcher of our own (`notify`) is deferred**, and the bullet
  above is most of why: in proxy mode it would duplicate a signal already
  arriving on the wire, at the cost of descriptors and memory on a large tree,
  and wakeups on every build artifact write —the exact opposite of staying out
  of the proper LSP's way during its startup. The case that survives is
  **standalone**, which has no editor watching on our behalf; `deps.md` §7 has
  the deferral and `open-questions.md` question 10 has what would reverse it.
*  Otherwise invalidated on demand: when a query finishes without a good
  candidate, that is itself the signal the file list may be stale, so a rescan
  is kicked off in the background. The query that triggered it still abstains,
  since it cannot wait for a rescan inside the deadline, but the next query on
  that spot sees a fresh list.

  The mechanism is `AbstainReason::NoCandidates` specifically, not any
  abstention (`resolution.md` §8). That reason means an *exhaustive* search
  found nothing, which is evidence about the file list; `Deadline` means the
  search was cut off, which is evidence about nothing, and rescanning on it
  would spend I/O in the window that just proved to be short of it.

  This pairs neatly with the retry protocol: a second query on the same spot
  is already the expected path, so the rescan usually lands exactly when it
  is needed. Rescans are debounced, so a burst of misses triggers at most one,
  and the two triggers share one debounce rather than one each.
* **Both invalidation paths are best-effort and neither blocks a query.** A
  query that arrives while a rescan is in flight uses the list it has.

Search scope is the workspace folders only. External dependency sources
(`~/.cargo/registry` and equivalents) are excluded per `high-level.md`; this is
also what keeps the walk small enough for the no-index approach to be viable
at all.

## 5. Deadlines and abstention

The hard cap is enforced by the driver, not trusted to the handler.

**The cap is configurable**, via `--deadline-ms` (`deps.md` §11). It defaults
to **750ms proxying** —`high-level.md` 's number —and **2000ms in
standalone**, where an abstention costs the user the answer entirely rather
than a wait they were already having
([section 14.6](shim.md#146-the-budgets-change-because-what-they-are-traded-against-changed)).
Nothing below depends on the specific value; "the deadline" means whichever is
in effect.

**The deadline is absolute and starts at request arrival**, not at handler
entry. Queueing time counts. A handler that gets the full budget of wall clock
but started 200ms late has already blown it from the user's point of view, and
the metric in `high-level.md` measures the user's point of view.

**Cancellation must be cooperative.** Wrapping CPU-bound work in a timeout
does not stop the work; it only stops waiting for it, leaving a thread burning
CPU that the proper LSP needs. This is why there is no timer-driven deadline
and, in turn, part of why there is no async runtime
([section 2](shim.md#2-process-and-transport-model)). The handler contract
requires polling instead:

```rust
pub struct Deadline {
    at: Instant,
    cancelled: Arc<AtomicBool>,
}

impl Deadline {
    pub fn expired(&self) -> bool { /* both checks */ }
}
```

Handlers must check `expired()` at every loop boundary — per candidate file,
per search result batch. The driver additionally hard-caps by dropping the
result of any handler that returns after the deadline, so a non-cooperative
handler produces a correctness-neutral waste of CPU rather than a late answer.

## 6. The agreement predicate

Before anything can be reported — or measured — "different" needs a
definition, and it cannot be range equality. The proper LSP points at the
identifier in a definition; a dumb-jump style match may point at the start of
the line, at the `fn` keyword, or at a whole item. Exact comparison would
report a divergence on nearly every *correct* answer.

This predicate is not a reporting detail. It *is* how precision is measured,
so getting it wrong corrupts the numbers that a future precision floor would
be derived from — and, right now, decides whether the user gets told they were
sent to the wrong place.

Both sides are first normalized: `textDocument/definition` may answer with
`Location`, `Location[]`, `LocationLink[]`, or null, and which one depends on
whether the client advertised `linkSupport`. All shapes collapse to a set of
`Location` — `(DocumentUri, ByteRange)` — taking `targetSelectionRange` for
links.

**The predicate compares `(uri, line)`, and nothing else.** Both sides carry
a line: the shim's answer because `Location` does
([section 8.4](#84-location-is-byte-based-and-this-fixes-a-real-inconsistency)),
and the child's because that is what came off the wire. So it **reads
nothing** — which matters, because divergence is classified when the child
responds, seconds after the answer, when the per-query read cache is long
gone and the target document may never have been open.

**Columns are deliberately not compared**, and the reason is that the 3-line
tolerance below already settles the question. If landing three lines from the
proper LSP's answer counts as a match — because the definition is on screen
and the user is already reading it — then landing forty columns away on the
*same* line must count too. Comparing ranges would be a stricter test nested
inside a looser one, which is not merely redundant but inconsistent: the row
it would decide is subsumed by the row beneath it, since two overlapping
ranges are on the same line by construction.

What it costs is the case of two definitions on one line — `int x, y;`, a
one-line TypeScript interface, dense generated code — which score as a match
when the shim picked the wrong one. That is a real if small overstatement of
precision, and it is preferred because it is a *uniform stated* tolerance,
strictly tighter than the ±3 lines already granted, rather than a hidden one.
Reopen it if the tolerance is ever tightened below a line; a column
comparison only starts to mean something then.

`Location.range` is unaffected and still earns its place — it is the jump
target on the wire. It simply is not an input to agreement.

Then, comparing one of the shim's locations against one of the child's. This
pairwise relation is not itself the `agreement` field: neither side is a
single location, so [Both sides are sets](#both-sides-are-sets) lifts it to
the three values that actually get written. `severity` below *is* the exact
string written to that field in
[section 7](#7-observability-and-the-corpus-scan) — the classifier and the
metric must not have separate vocabularies, or the number that ships and the
number that gets measured stop being the same number.

| Relation | Pairwise | `severity` |
|---|---|---|
| Same file, within 3 lines | matches | — |
| Same file, more than 3 lines apart | differs | `same_file` |
| Different file, same module tree | differs | `near_module` |
| Different file, unrelated | differs | `unrelated` |
| Child answered null or empty, shim committed | differs | `unrelated` |
| Both empty | matches | — |

The 3-line tolerance is deliberate: at that distance the correct definition is
on screen and the user is already reading it, so scoring it as wrong would
measure something nobody experiences as wrong. The tiers below it are the
error severity classes `high-level.md` reports, and are what a future budget
would be attached to.

### Both sides are sets

The table above compares one location against one location. Neither side is
one location.

The child's side never was: when it returns several, agreement means matching
*any* of them, because the LSP is itself expressing ambiguity and picking one
of its own candidates is not an error. The shim's side is now a set too
—`high-level.md` decides that indistinguishable candidates are all returned,
ranked —so the predicate needs a rule for set against set, and the obvious one
is wrong. "Any of ours matches any of theirs" is a predicate that improves
monotonically as the shim returns more, which is the flaw `high-level.md`
rejects plain match rate for, reappearing inside the classifier.

So the pairwise table is applied twice, against the child's whole set:

* **`top1`** — the shim's *first* location matches. Cannot be improved by
  returning more, so it is the number that gets optimized.
* **`contained`** — any of the shim's locations matches. This is what the user
  could actually reach through the picker, and it is reported only alongside
  the result count, since alone it is gameable.

These three, and only these three, are what `agreement` ever holds. A bare
`match` is not one of them, and the pairwise column above is an input to the
lift rather than a value anything records.

`agreement` therefore takes one of `match_top1`, `match_contained`, or
`mismatch`, and these are ordered: `match_top1` implies `match_contained`.
`severity` is classified from the shim's **top-ranked** location whenever
`agreement` is `mismatch`, since that is where a user who trusts the ordering
looks first, and is undefined otherwise.

**Divergence is reported to the user on `mismatch` only.** A `match_contained`
answer showed the user the correct location; telling them they were misled
would be false, and would train them to ignore the reports that matter.
[The reporting rules below](shim.md#how-much-to-report) are unchanged
otherwise, and this makes them fire meaningfully less often.

One consequence for [section 7](shim.md#7-go-to-definition-lifecycle): the
shim's answer is now a ranked list rather than a location, so the
pending-query record holds the list and its order is load-bearing
—`answered_by_shim` must preserve rank, not collapse to a set, or `top1`
cannot be computed when the child eventually replies.

## 7. Observability and the corpus scan

The driver is the only component that sees both the heuristic answer and the
proper answer, so it owns the measurement of every metric in `high-level.md`.

Each query emits one JSONL record once both answers are known (or the query is
resolved as abstained):

```json
{
  "uri": "...", "position": 4821, "language": "rust",
  "mode": "proxy",
  "server_health": "Warming",
  "decision": "committed",
  "stratum_prior": "explicitly_imported",
  "stratum_final": "explicitly_imported",
  "confidence": 0.94,
  "margin": 0.62,
  "considered": 7,
  "bytes_scanned": 1841203,
  "files_parsed": 14,
  "heuristic_latency_us": 8300,
  "heuristic_locations": ["..."],
  "returned": 3,
  "truncated_list": false,
  "lsp_latency_us": 4210000,
  "lsp_locations": ["..."],
  "agreement": "match_top1",
  "severity": null
}
```

This single record type covers coverage, precision, error severity
classification, per-stratum breakdown, latency percentiles, and the
LSP-latency value weighting. Everything from `stratum_prior` through
`files_parsed` is reported *by the handler*, since only it knows which
resolution path produced the answer and what it cost; the driver classifies
`agreement` and `severity`, since only it has both answers.

**`position` is a byte offset**, like every other position inside the shim
([section 8](#8-protocol-types)). It is what `data-collection.md` records
and what `measure replay` joins on, so a line/column pair here would need a
conversion in the one place the two halves of the metric have to line up
exactly.

**The stratum is two fields, not one.** `resolution.md` §8 assigns a stratum
a-priori from the reference, then permits one refinement during search — to
`AmbiguousName` or `ExternalDependency`, neither of which is knowable before
the search runs. Coverage is reported on `stratum_prior` so the denominator
is fixed by the reference and does not move when the implementation changes;
precision is reported on `stratum_final` so an answer is judged against the
class it turned out to be. One field cannot do both, and collapsing them
makes `high-level.md`'s central table non-comparable across versions — the
one property it needs.

**`margin` and `considered` are the features a floor would be set on.**
Nothing reads them in v1. They are recorded because a threshold can only be
derived from data collected while nothing was being gated, and a corpus run
that kept only the collapsed `confidence` could never answer *what would a
floor have cost?* — which is the question the permissive posture exists to
ask (`resolution.md` §7.1).

**`bytes_scanned` and `files_parsed` are counters, not limits.** Nothing
compares them against a budget and no search stops because of them
(`resolution.md` §1.3); they are here so a latency regression can be
attributed to a diff rather than guessed at. Read as a proxy for cost they
are approximate — parse cost is superlinear in file size for some grammars,
and a cold read is dominated by seek latency rather than length.

`heuristic_locations` is **ordered**, and `returned` is its length —redundant,
and worth carrying anyway, because the result-count distribution is one of the
three numbers `high-level.md` requires and computing it by measuring an array
length in every consumer is how a metric acquires two definitions.
`truncated_list` says the ranked list hit the cap, which is the difference
between "this is everything" and "this is the best of more than we will show"
—and `agreement: match_contained` means something weaker in the second case,
since containment was only ever measured over what survived the cap.

`mode` is `"proxy"` or `"standalone"`. In standalone, `server_health`,
`lsp_latency_us`, `lsp_locations`, `agreement`, and `severity` are all `null`,
because there is no second answer to compare against
([section 14.7](shim.md#147-what-this-does-to-measurement)). Without the `mode`
field a mixed log silently pollutes the precision numerator with rows that
could never have had an `agreement`.

### The corpus scan is a separate program

The corpus scan in `high-level.md`'s development plan is **not** a mode of the
shim. It is its own crate — `measure_core`, plus a four-line `measure_<lang>`
binary per language, below. `driver` has no batch path, no transport
abstraction, and no awareness that any of it exists.

The requirements are opposed at nearly every point:

| | `driver` | `measure` |
|---|---|---|
| The proper LSP | raced against | waited for — it *is* ground truth |
| Optimises for | latency | throughput |
| Deadlines | hard, abstain past them | none |
| Position in the stream | proxy between editor and server | plain LSP client, no editor |
| Documents | whatever the editor opened | drives `didOpen` across a repo |

Building one program that does both means a transport abstraction and a
policy-override switch that exist for a single caller, threaded through the
part of the system with the strictest correctness requirements.

The reason to hesitate is that a separate harness could drift into measuring a
reimplementation rather than the real thing. That concern turns out to be
weaker than it looks: **what `measure` measures is the handler, not the
driver.** The proxy, the health model, and the retry protocol are not under
test — resolution accuracy is. So as long as `measure_core` builds its
`Query` and `DocumentSnapshot` the same way, the code under test is genuinely
identical.
Snapshot construction therefore lives in `shared`, which makes that
structural rather than a matter of discipline.

`measure collect` spawns a fresh language server per repository, opens
documents, enumerates identifiers with the handler's own grammar, asks both
sides, and
writes the records above. The one thing it shares with the shim beyond the
vocabulary is the agreement predicate from
[section 9](shim.md#9-divergence-reporting) — the definition of "match" must not
fork, or the shipped metric and the measured metric stop being the same
number. **That predicate therefore lives in `shared`**, not in `driver`:
`measure_core` does not depend on `driver`
([section 9](#the-dependency-graph)), so there is no other place both can
reach it from.

### One measurement library, one tiny binary per language

The measurement program is not one program. It is a library plus a trivial
binary per language:

* **`measure_core`** — the LSP client, `truth.jsonl` reading and writing,
  snapshot and `Query` construction, the replay driver, agreement
  classification, and the per-stratum table. Depends on `shared` and nothing
  else of ours. It takes the handler as `&dyn LanguageHandler`, so it is
  compiled once and is generic over the language without monomorphising over
  it.
* **`measure_<lang>`** — the binary, and essentially the whole of it:

  ```rust
  fn main() -> Result<(), shared::Error> {
      measure_core::run(&lang_rust::Handler::new(), Cli::parse())
  }
  ```

The alternative — one `measure` binary depending on every `lang_*`, which is
what an earlier draft of [section 9](#9-workspace-layout) specified — makes
every language's measurement depend on every other language *building*. Three
things go wrong with that, and the third is the one that matters:

* **Build cost per measurement.** Collecting a Rust number would compile every
  grammar in the workspace. Grammars are large generated C, so this is the
  dominant cost in a fresh checkout, and it is paid on every iteration of a
  tuning session for no benefit.
* **Fault coupling.** A language crate that does not compile takes every other
  language's metrics down with it, including languages nobody has touched.
* **It defeats the isolation the parallel-tuning plan depends on.**
  `loops.md` runs one session per language, each confined
  to its own crate. Confinement that still requires the other crates to build
  is not confinement; it just moves the coupling into the build graph where it
  is harder to see. A language must be measurable entirely on its own.

The cost is one extra crate per language, whose contents are the four lines
above. That is the right price: it keeps `lang_*` unaware that `measure_core`
exists, so the shipped `heuristic-jump` binary never links an LSP client, and
it keeps
the dependency direction one-way.

Aggregating across languages — the combined table, the frontier — is done over
the emitted records, which are data. Nothing that aggregates needs to link a
handler at all.

### Two modes: collect and replay

Driving a real language server over ten repositories is hours of wall clock,
and it produces an answer that does not change when the handler changes. The
proper LSP's answer for a given position at a given commit is a *fact about
the corpus*, not about our code, so it is collected once and frozen.

`measure` therefore has two subcommands, and only the first needs a server:

* **`collect`** — spawn the server, drive `didOpen` across the repository,
  enumerate identifiers, ask the LSP, write `truth.jsonl`. Slow, run rarely,
  output is a frozen artifact in the corpus root, never in the repository.
* **`replay`** — read `truth.jsonl`, reconstruct the `DocumentSnapshot` and
  `Query` for each recorded position, run the handler, classify agreement,
  emit the metric table. No server, no network, no `didOpen` round trips.

The record in this section is the join. `collect` writes rows with the
`lsp_*` fields populated and the heuristic side null; `replay` fills the
heuristic side and computes `agreement` and `severity` with the same
predicate the driver uses. A completed replay row is byte-comparable with a
row the shim emitted in the field, which is what keeps the measured metric and
the shipped metric the same number.

This is not a convenience. It is the difference between a tuning iteration
costing minutes and costing an afternoon, and it is on the critical path for
every language, because a loop whose feedback is slower than its own thinking
is bounded by I/O rather than by ideas.

**How fast a replay actually is, is a measurement rather than a target.**
No number is set here, and none should be inferred: the cost is dominated by
how often a query falls through to the whole-project search
(`resolution.md` §5), by how much of that survives the lexical prefilter, and
by how much of the corpus stays resident in the parse LRU across queries —
none of which is known before a handler and a corpus both exist. Setting a
target now would mean either designing around a guess or declaring a failure
that has not happened. So `measure replay` reports its own wall clock
alongside the per-query work counters, `loops.md` §9 records both from the
first run, and what to do about the number is decided when there is one.

The requirement that replay run *without a server at all* is stated here
rather than left implicit because nothing else in this document requires it,
and discovering it later means discovering it after the corpus has been
collected in a shape that cannot be replayed.

Constraints that make a replay trustworthy:

* **`truth.jsonl` carries its provenance in a header record**: repository
  path and commit, language server name and version, grammar revision, and
  the `measure` version that wrote it. Replay refuses to run against a truth
  file whose repository commit does not match the checkout, rather than
  silently reporting metrics for positions that have since moved.
* **Replay enforces no deadline at all.** This is the constraint that makes
  replay worth having, and it is easy to get wrong by doing the obvious
  thing. A wall-clock deadline makes abstention depend on machine load: the
  same handler on the same snapshot gives up on a busy machine and finishes
  on an idle one, so *coverage* — not just latency — becomes a property of
  what else was running. Metrics that move with background load cannot be
  compared across runs, and a tuning session cannot tell an improvement from
  a quiet minute.

  Replay therefore runs the handler to completion and records what it found.
  That is sound because a search is **exhaustive**: it reads every candidate
  file and stops only when it runs out of them, so with the clock removed
  there is nothing left that could vary. An earlier revision instead
  substituted a per-query byte budget as a reproducible surrogate for the
  clock, which worked but had to be calibrated against a wall-clock deadline
  to mean anything; `resolution.md` §1.3 drops the budget and gets the
  determinism structurally instead.

  The consequence to be explicit about: **replay reports an upper bound on
  what the shim delivers.** The shim has a deadline and will sometimes abstain
  where replay answered. That gap is a latency fact, and it is measured as
  one — work counters per iteration, wall clock at phase gates
  (`loops.md` §10). Handler coverage from replay is a statement
  about resolution, not a promise about the field.
* **Only the heuristic side is re-measured, and its timing is an observation,
  not a control input.** `heuristic_latency_us` is recorded during replay
  because it is the same handler code on the same snapshot, but nothing in
  the run branches on it. It is therefore the one field in the record that a
  replay does not reproduce exactly, and the one that needs a quiet machine
  to mean anything. `lsp_latency_us` comes from `collect` and is a property
  of the frozen truth — which is exactly what `high-level.md`'s value weighting
  wants, since it is a fact about how slow the real server was, not about
  this run.
* **Replay measures the handler, not the driver**, same as `collect` — the
  paragraph above applies unchanged. Nothing in the proxy, the health model,
  or the retry protocol is under test in either mode.
* **A truth file is regenerated, never edited.** Metrics compared across two
  corpus versions are not comparable, and a partially refreshed corpus is the
  worst case: it looks like a regression.

### The oracle is the server being proxied

Two language servers for the same language do not agree, and the disagreement
is not always one of them being wrong. `go-to-definition` on a re-exported
name has two defensible answers — the re-export site and the original
definition — and servers make different choices. The same is true of
declaration versus definition, of trait method versus impl method, and of
whether a `use` resolves to the import or through it.

The shim's job is to stand in for **one specific server**, and
[section 9](shim.md#9-divergence-reporting) reports divergence against that
server. So the answer that counts as correct is that server's answer, and a
shim that split the difference would be wrong in both deployments rather than
right in either.

Two consequences.

**Every metric is per (language, server).** There is no aggregate across
servers, in the same way and for the same reason that
[`high-level.md`](high-level.md#stratification) refuses a single rolled-up
number across strata: the mix is not a fact about the tool.
`heuristic-jump -- pyright` and `heuristic-jump -- pylsp` are different
products with different scores, and reporting their average would describe
neither.

**The behaviour itself varies**, through a `ServerProfile` reaching handlers
in `Query` ([section 1](#the-trait)). Which brings the cross-server
comparison back with a real job, rather than as a measurement workaround:

* Positions where **every server agrees** are the shared handler's
  responsibility. That is the bulk of the corpus, and it is where resolution
  logic is developed and measured.
* Positions where **servers differ** are the profile's responsibility. Each
  server's divergent set is small, specific, and is exactly the evidence for
  what its profile should say.

This decomposition is worth having because it makes the two surfaces
separately optimisable — a profile change cannot affect another server's
numbers, and a shared-logic change is evaluated where the servers do not
disagree about the answer. It also produces a free finding: the set of
positions where servers differ is, in practice, a map of where re-export and
alias chains matter, which `resolution.md` open question 9 says it
needs data on before deciding whether to follow them.

**The profile must not become a per-language configuration format.**
`resolution.md` §1.2 and §9 rule that out, and a struct of behaviour
knobs is precisely the shape it would take. The rule is the same one §9
applies to sharing generally: it starts empty, and a knob is added only when the
corpus shows a systematic divergence that a knob would fix. Nothing is
predicted.

### Where the corpus lives

Not inside the repository. One root outside the workspace, holding two
sibling splits, each passed by path:

```
../heuristic-jump-corpus/
  training/                     tuning corpus
    rust/
      repos/<name>/             checkout, pinned commit
      positions/<name>.jsonl    enumerated once, shared by every server
      truth/rust-analyzer/<name>.jsonl
      manifest.toml             what was chosen and why
    python/
      repos/<name>/
      positions/<name>.jsonl
      truth/pyright/<name>.jsonl
      truth/pylsp/<name>.jsonl
      manifest.toml
    ...
  test/                         held out, same shape
```

`data-collection.md` owns this layout and the rules that go with it — how
repositories are chosen, why positions are enumerated once rather than per
server, and what the manifest records.

**Truth is per server, not per language.** Repositories are shared across
servers — the checkout is the expensive artifact and the source text is the
same either way — but each server gets its own `truth.jsonl`, because each
server is a different oracle answering the same questions differently. The
provenance header names exactly one server and version, which is what makes a
truth file comparable to itself over time and never silently merged with
another's.

Collapsing them into one file with a per-answer server field was the obvious
alternative and is worse: the two are collected at different times, refreshed
on different schedules as servers release, and consumed by different
measurement runs. A file per server means refreshing pyright's truth does not
touch pylsp's, and a corpus half-refreshed across servers is not even
representable.

Three reasons for the split, in ascending order of importance:

* Repository checkouts are large and are not our source. Keeping them out of
  the workspace keeps `cargo` and the `ignore` walk away from them, and keeps
  them from being duplicated by every `git worktree`.
* The two are used by different commands at different times — the tuning
  corpus on every iteration, the held-out corpus rarely — so they have no
  reason to share a directory.
* **Sibling splits make held-out isolation a path check rather than a
  convention.** The development plan in `high-level.md` holds repositories
  back precisely because tuning sessions will otherwise learn them, and a rule
  that says "do not look at that subdirectory" is enforced by whoever
  remembers it. A rule that says "this session is given one path and never the
  other" is enforced by not having the path. `test/` is a sibling of
  `training/` rather than a subdirectory of it for exactly that reason, and
  `loops.md` §12 relies on it being a filesystem boundary.

### The command line

Three subcommands, one per stage of `data-collection.md`. The binary is
per-language, so the language is never an argument.

```
measure-<lang> enumerate --corpus <dir> [--repo <name>]... [--limit N] [--seed N]
measure-<lang> collect   --corpus <dir> --server <name> [--repo <name>]... [--restart]
measure-<lang> replay    --corpus <dir> --server <name> [--repo <name>]...
                         [--format table|json]
```

* **`enumerate`** parses each repository, samples positions, writes
  `positions/<repo>.jsonl`. `--limit` defaults to 20 000 and `--seed` makes the
  sample reproducible — an unseeded sample is a corpus that cannot be
  regenerated, which defeats freezing it.
* **`collect`** drives the server named in the corpus root's `servers.toml`,
  which carries its command and pinned version. Naming a server rather than
  passing a command line is what lets the provenance header record what was
  actually run without trusting the invocation to be repeated correctly.
  Resuming is the default; `--restart` discards a partial truth file, which is
  the destructive option and therefore the explicit one.
* **`replay`** reads the frozen truth and prints the per-stratum table.
  `--format json` is what the harness consumes. It **writes nothing** — the
  harness decides what to record, so `measure_core` needs no knowledge of
  `state/`.

Three properties the flags are chosen to give:

* **`--corpus <dir>` is required and has no default.** A defaulted corpus path
  is one that eventually points at the wrong one.
* **There is no `--held-out` flag**, and there must not be. Held-out is
  selected by passing a different `--corpus` path, so a session that is not
  given the path cannot reach the data. A flag is something a loop can set;
  a path it was never told is not
  ([`loops.md`](loops.md#12-held-out-integrity)).
* **`replay` is deterministic.** Same corpus, same commit, same table, byte for
  byte — which is what makes it usable as a gate rather than a report, and is
  the property the whole [replay design](#two-modes-collect-and-replay) rests
  on.

Exit status is about whether the run happened, not about whether the numbers
are good: `replay` exits zero having printed a table full of zeroes. Judging
the table is the gate's job, not the measurement's.

## 8. Protocol types

`shared::proto` defines every LSP message and field the shim touches. There
is no `lsp-types` dependency.

### 8.1 Why not `lsp-types`

The obvious objection is that hand-writing wire types to save a dependency is a
poor trade. That is not the trade. The motive is that **the newtypes in
[section 1](#vocabulary-types) should be what deserialization produces, not
what a conversion layer produces afterwards** — and with a foreign types crate
they can only ever be the latter.

With `lsp-types`, every message boundary yields foreign primitives that a
conversion layer then has to launder into `DocumentUri`, `DocumentVersion`,
`EditorRequestId`, `LanguageId`, `ByteOffset`. That layer is real code, it is
where the encoding bugs live, and — decisively — **it is optional.** Nothing
stops a later change from holding an `lsp_types::Position` a few functions
inward, and nothing about that change looks wrong in review. The newtype
discipline `CLAUDE.md` asks for becomes a convention enforced by attention
rather than by the compiler, in exactly the part of the system
[section 3](#3-position-encoding) singles out as the highest-risk.

The concrete case is `Position`:

```rust
pub struct Position { pub line: u32, pub character: u32 }   // lsp-types
```

`character` is a count of UTF-16 code units, or UTF-8 bytes, or UTF-32 code
points, depending on a negotiation that happened in a different function at a
different time. It is a bare `u32` and every one of those readings typechecks.
That is the precise shape of the bug class section 3 describes: invisible on
ASCII, wrong by a few columns on any line with a non-ASCII character.

### 8.2 What replaces it, and why it is smaller than it sounds

Two properties make the hand-written version much less work than a general
LSP types crate, and both come from decisions already made:

*  **Nothing is ever round-tripped.**
  [Section 1](shim.md#1-the-prime-invariant) forbids deserializing a forwarded
  message and re-serializing it, and
  [section 3.1](shim.md#31-how-little-inspection-the-forwarding-path-needs)
  means we do not even inspect most of them. So the incoming types are
  **read-only projections**: partial structs that name the handful of fields
  we read and ignore everything else, which is serde's default behaviour. A
  field we did not model cannot be lost, because nothing writes it back. That
  removes the main hazard of hand-rolled wire types.
*  **Only a small set is ever constructed.** Definition responses, error
  responses, `window/showMessage`, `window/showMessageRequest`,
  `window/showDocument`, and —in standalone
  ([section 14.3](shim.md#143-initialize-is-ours-now)) —one
  `InitializeResult`.

The inventory is roughly thirty small structs:

| Read | Fields we actually need |
|---|---|
| `InitializeParams` | `rootUri`, `workspaceFolders[].uri`, `capabilities.window.{showDocument,showMessage}`, `capabilities.general.positionEncodings`, `capabilities.textDocument.definition.linkSupport`, `clientInfo` |
| `InitializeResult` | `capabilities.textDocumentSync`, `capabilities.positionEncoding`, `capabilities.definitionProvider`, `serverInfo.name` |
| `didOpen` / `didChange` / `didClose` / `didSave` params | uri, languageId, version, text, content changes |
| definition params | uri, position |
| `$/cancelRequest` | id |
| `$/progress` | token, and the value left raw for adapters |
| definition result | `Location` / `Location[]` / `LocationLink[]` / null |

| Construct | |
|---|---|
| response envelope | result or error, with our own id type |
| `showMessage`, `showMessageRequest`, `showDocument` | |
| `InitializeResult` | standalone only |

### 8.3 The wire position type is inert

This is the design's payoff and the reason the change is worth making.

```rust
/// A position exactly as it appeared on the wire. `character` is in the
/// negotiated encoding, which this type does not know — so it exposes no
/// way to be used as an offset.
#[derive(Deserialize)]
pub struct WirePosition { line: LineIndex, character: u32 }

impl WirePosition {
    /// The only way out. Requires naming the encoding and the document,
    /// which is exactly the information a correct conversion needs.
    pub fn resolve(self, enc: PositionEncoding, text: &Rope)
        -> Result<ByteOffset, EncodingError>;
}
```

`WirePosition` has private fields and no accessors. A `ByteOffset` cannot be
obtained from it without supplying both the negotiated encoding and the text,
so the failure mode in [section 3](#3-position-encoding) — using a UTF-16 column
as a byte index — is not something to be careful about. It does not compile.

The same applies outbound: `WirePosition::encode(ByteOffset, enc, &Rope)` is
the only constructor. Encoding is therefore applied in exactly two functions
in the whole system, both of which take the encoding explicitly, and
`PositionEncoding` itself is set once from `InitializeResult` and never
inferred.

### 8.4 `Location` is byte-based, and this fixes a real inconsistency

[Section 1](#the-trait) has handlers return
`Outcome::Committed { locations: Vec<Location> }` while also stating that "no
UTF-16 offset ever reaches the handler interface." With `lsp_types::Location`
those two cannot both be true: an `lsp_types::Location` holds line/character
positions, so a handler would have to encode them itself, which means a
handler needs the rope, the negotiated encoding, and the conversion code —the
exact thing the sentence forbids.

So there are two types, and the distinction is load-bearing:

```rust
/// What a handler returns. Byte offsets, always -- plus the row, which
/// is also byte-space and which the handler gets for free.
pub struct Location {
    pub uri: DocumentUri,
    pub range: ByteRange,
    pub line: LineIndex,
}

/// What goes on the wire. Constructed only by the driver, at the edge.
pub struct WireLocation { uri: DocumentUri, range: WireRange }
```

The driver converts one to the other on the way out, in the same one place
that owns `PositionEncoding`. Handlers never see a `WireLocation` and cannot
construct one.

**Why `Location` carries a line.** It looks redundant with `range`, and
strictly it is. It is there because the alternative is worse in two places:

* To put an answer on the wire the driver needs line and character in the
  *target* file, which without a line means building a whole-file line index
  for a file the handler may only have literal-scanned. With the line
  supplied, only that one line's text is needed, and only to resolve the
  UTF-16 column.
* The agreement predicate ([section 6](#6-the-agreement-predicate)) is
  line-based by definition — every severity tier is "within 3 lines" or
  "further than 3 lines", and columns are not compared at all. With the line
  in hand it compares `(uri, line)` and **reads nothing**, including in the
  same-file case where the document is closed and the divergence arrives
  seconds later with no per-query cache left.

A handler pays nothing for it: it verified the candidate by parsing, so
`node.start_position()` already has the row. And it is not an encoding leak —
row plus byte-column is byte-space, and `PositionEncoding` still exists in
exactly one place.

The risk is a `line` that disagrees with `range`. `Location` is therefore
constructed only through `Location::at_node(uri, node)`, which derives both
from the same node, so the two cannot drift apart by hand.

Keeping them as separate types is what makes the rule survive implementation.
With one shared type the pressure is always to hand handlers the encoding
"just for this one case," and that is how the rule erodes.

### 8.5 The untagged unions are the actual risk

Hand-rolled wire types are safe for flat structs. Where they go wrong is JSON
unions — a field that is one of several shapes with no discriminator field to
say which. LSP has five that matter:

| Field | Shapes |
|---|---|
| `id` | number \| string |
| `textDocumentSync` | integer enum \| `TextDocumentSyncOptions` object |
| `definitionProvider` | boolean \| options object |
| definition result | `Location` \| `Location[]` \| `LocationLink[]` \| null |
| `contentChanges[]` | `{range, rangeLength?, text}` \| `{text}` |

The reflex is `#[serde(untagged)]`. It is the right tool for three of these
and a loaded gun for the other two, and the difference is worth being precise
about, because the failure is silent.

#### How untagged actually behaves

`#[serde(untagged)]` deserializes by **trying each variant in declaration
order and accepting the first that succeeds.** There is no discrimination
step; "succeeds" is the whole test. Two consequences follow immediately:

* If two variants can both accept the same JSON, the earlier one wins and
  nothing reports the ambiguity.
* Whether a variant "can accept" some JSON depends on how lenient its struct
  is — which, for us, is *maximally* lenient.

That last point is the crux, and it is specific to this design.
[Section 8.2](#82-what-replaces-it-and-why-it-is-smaller-than-it-sounds)
makes incoming types read-only projections that ignore unknown fields, because
that is what keeps us forward-compatible with fields we did not model. But
ignoring unknown fields is exactly what destroys an untagged enum's ability to
discriminate: a variant that ignores everything it does not recognise will
happily accept a value meant for a different variant.

So the two policies are in direct tension on the same struct, and
`deny_unknown_fields` is not a way out — turning it on for a projection
reintroduces the brittleness the projection exists to avoid.

#### The case that would actually hurt

`contentChanges`. The incremental form is `{range, rangeLength?, text}`; the
full-document form is `{text}`. Written the obvious way:

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum ContentChange {
    Full { text: String },                    // WRONG: declared first
    Incremental { range: WireRange, text: String },
}
```

an incremental change arrives as `{"range":{...},"text":"foo"}`, `Full`
accepts it because `range` is simply an unknown field it ignores, and the
driver **replaces the entire document with the few characters the user just
typed.** Nothing errors. The version still increments, the rope still holds
text, and every subsequent parse, query, and answer is computed confidently
against a document that no longer exists in the editor. That is the worst
failure shape this design has: not a crash, not an abstention, but a wrong
answer with full confidence.

Reversing the declaration order fixes this particular instance, which is
precisely why it is not an acceptable defence. Correctness that depends on
field-declaration order is correctness that a routine reordering silently
removes, with no test failing unless someone thought to write the negative
one.

#### The rule

> **Untagged is permitted only when the variants are disjoint by JSON *kind*
> or by a *required* field the others lack. Never by an optional field, and
> never by declaration order.**

Against the five:

* **`id`** — number vs string. Disjoint by kind. Untagged is fine.
* **`textDocumentSync`** — number vs object. Disjoint by kind. Fine.
* **`definitionProvider`** — bool vs object. Disjoint by kind. Fine. Note that
  *absent* is a third case meaning "unsupported", and must not collapse into
  `false` by a `#[serde(default)]`; it is an `Option`.
* **definition result** — `Location` requires `uri`, `LocationLink` requires
  `targetUri`, and neither has the other's field, so each fails the other's
  deserialization on a missing required field. Genuinely disjoint. The one
  residual ambiguity is `[]`, which matches both array variants; harmless,
  since both mean "no definitions", but it should be a test rather than a
  thing someone notices later.
* **`contentChanges`** — **not disjoint.** `{text}` is a subset of
  `{range, text}`. So it does not get untagged: it gets a hand-written
  `Deserialize` that looks for `range` and dispatches on its presence. That is
  about fifteen lines, it says what it means, and it cannot be broken by
  reordering.

#### Three lesser problems, worth knowing about

* **Untagged buffers.** serde implements it by first deserializing into an
  internal `Content` tree — effectively a `serde_json::Value` — and then
  replaying it against each variant. So untagged variants allocate and
  generally **cannot borrow** from the input; `&'a str` fields in an untagged
  variant fail. Irrelevant for the small messages above, and a reason not to
  reach for untagged casually elsewhere, particularly not in the peek path of
  [section 3.1](shim.md#31-how-little-inspection-the-forwarding-path-needs).
* **The error message is useless.** When every variant fails, serde reports
  "data did not match any variant of untagged enum X" with no line, no column,
  and no indication of why each variant was rejected. Debugging a real message
  that fails to parse is materially harder than for a plain struct.
* **`deny_unknown_fields` does not compose with `flatten`.** If a projection
  ever grows a `#[serde(flatten)]`, the strictness silently stops applying.
  Another reason the rule above is stated in terms of required fields rather
  than in terms of strictness.

#### What makes this safe to hand-roll

Two mitigations, and they are the condition on which dropping `lsp-types` is
acceptable:

*  **A golden corpus.** Real `initialize` / `InitializeResult` pairs and
  document traffic captured from Zed and VS Code against rust-analyzer,
  pyright, and gopls, checked in, and asserted against. Wanted for
  [section 12](shim.md#12-testing) 's transparency tests anyway, so the
  marginal cost is near zero.
*  ** `lsp-types` as a dev-dependency oracle.** For every message in the
  corpus, deserialize with both and assert the fields we model agree. We drop
  the runtime dependency and keep the spec knowledge, which is the part that
  was actually valuable. Same shape as the differential fuzz target for the
  peek scanner in [section 3.1](shim.md#the-bounded-structural-prefix-scan): a
  hand-written fast path is acceptable when a trusted implementation checks it
  in tests.

Plus, specifically for the unions, **negative tests**: for each union, assert
that each shape parses as the intended variant *and* that it fails to parse as
each of the others. The positive half is what everyone writes and it is the
half that would have passed while `contentChanges` silently destroyed
documents.

### 8.6 Modelling errors must fail closed

[Section 8.5](#85-the-untagged-unions-are-the-actual-risk) is the sharpest
hazard but not the whole class. Hand-written projections have other ways to be
quietly wrong:

* a forgotten `#[serde(rename_all = "camelCase")]` on one struct, so every
  field in it reads as absent;
* a missing `#[serde(default)]`, so an omitted optional is a hard error rather
  than a `None`;
* `null` versus absent treated as the same thing when the protocol
  distinguishes them;
* a numeric width that is wrong at the edges.

The golden corpus and the `lsp-types` oracle catch these where the corpus
exercises them — but a field that appears in no captured message is untested by
construction, and that is exactly the long tail. Detection alone is therefore
not the plan. **The consequence has to be safe.**

> Any failure or detected inconsistency while deserializing a state-bearing
> message marks that document **untrusted**. Queries against an untrusted
> document abstain, unconditionally, until a `didClose`/`didOpen` resyncs it.

This is the mechanism that makes hand-rolled types an acceptable risk, and it
is more general than anything in 8.5: it does not care *which* modelling
mistake occurred. It converts the entire class from "confidently wrong" to
"abstain" — the axis the whole tool is built on, since `high-level.md` prices an
abstention at approximately nothing and a wrong answer at the tool's
credibility.

Note the failure being guarded is specifically *silent state drift*. The frame
was already forwarded
([section 3.1](shim.md#31-how-little-inspection-the-forwarding-path-needs)),
so the child and the editor are unaffected and the proper LSP still answers
correctly; the only casualty is the shim's own model of the document. Left
undetected, that model produces confident answers about text the user does not
have —which is [section 2](#text-and-tree-can-never-disagree) 's failure mode
arriving by a different route.

Three cheap self-checks turn drift into a detectable event rather than a
permanent one, and all three are `core`-side O(1):

* **An incremental range outside our rope** is proof we have already diverged.
  It cannot happen if every prior change was applied correctly.
* **A version that does not increase**, or a `didChange` for a document never
  opened, means we and the editor disagree about what is open.
* **`didSave` is a free end-to-end checksum.** Immediately after a save, the
  buffer and the file on disk are identical by definition, so our rope's length
  — or a hash of it — must match the file. This validates the entire
  document-tracking pipeline against ground truth, at a point where the answer
  is known. It costs a read, so it belongs in a worker, off the critical path,
  and a mismatch marks the document untrusted rather than raising an error.

`open-questions.md` question 6 asks what the shim should do when the editor
misbehaves — `didOpen` for an already-open document, `didChange` for one never
opened. This is the answer to the half of that question that matters: not
"ignore," but "stop trusting the document, keep proxying perfectly, and say so
in the log."

### 8.7 Where it lives

`shared::proto`, not a separate crate. [Section 9](#the-dependency-graph)
already has `measure_core` depending on `shared`, and `Location`,
`DocumentUri`, and the vocabulary newtypes have to be in `shared`
regardless, since they are in the handler seam. Splitting the wire types into
their own crate would separate them from newtypes they are defined in terms
of, for no gain.

The wire types add one dependency of their own: `url`, for `DocumentUri`
normalization and `file:` path extraction, which is where the percent-encoding
and Windows drive-letter bugs live and is not worth hand-rolling.
[Section 9](#the-dependency-graph) has `shared`'s full dependency list.

## 9. Workspace layout

A cargo workspace with `crates/` for our code and `vendor/` for copied-in Zed
crates, kept separate so provenance and licensing stay obvious.

```
Cargo.toml              workspace root
vendor/
  rope/                 copied from zed, GPL-3.0-or-later
  sum_tree/             copied from zed, Apache-2.0
crates/
  shared/           handler trait, vocabulary newtypes, ProjectView, proto, Error
  similarity/       ported from the prior implementation; frozen until phase 3
  lang_rust/        one crate per language
  lang_python/
  lang_typescript/
  driver/           the LSP driver
  heuristic_jump/   the shim binary -- `heuristic-jump`
  measure_core/        corpus scan library -- LSP client, replay, metrics
  measure_rust/        `measure-rust` -- four lines, section 7
  measure_python/
  measure_typescript/
```

Crate names carry no project prefix, matching the vendored Zed crates
alongside them (`rope`, `sum_tree`) and the workspace-wide `publish = false`.
Two of the names are chosen rather than mechanical:

* **`driver`, not `core`.** A crate named `core` shadows Rust's own, and this
  document already uses "`core`" throughout for the single-threaded actor in
  [section 2](shim.md#thread-layout). Two different things called `core` in one
  system is a needless ambiguity; `driver` is what the prose calls the crate
  anyway.
* **`heuristic_jump`** for the binary crate, so the produced binary is
  `heuristic-jump` without a `[[bin]]` rename — the same relationship Zed has
  between its `zed` crate and its `zed` binary.

### The dependency graph

The shape is dictated by one rule from the outset: **`driver` must not depend
on any language crate.** Wiring happens in `heuristic_jump`.

```
              shared  <-- rope, tree-sitter, serde, serde_json, url,
             /  /  |  \      ignore, rayon, thiserror, rustc-hash
            /  /   |   \
measure_core  /  similarity  driver  <-- crossbeam-channel, rayon
       |     /     |          |
       |    lang_* /          |
       |     /  \ /           |
       +--> measure_<lang>   heuristic_jump
```

`measure_core` and `driver` are siblings that never meet; `measure_<lang>` is
the only crate that depends on both `measure_core` and a language, and it
contains four lines.

Every edge, and why:

* **`shared` depends on nothing of ours.** The shared vocabulary: it holds
  `LanguageHandler`, `Query`, `Outcome`, `Stratum`, `Deadline`,
  `DocumentSnapshot`, `ProjectView`, and `Error` — types every other
  crate needs to talk about, and almost no behaviour. It also holds `proto`, the
  hand-written LSP wire types ([section 8](#8-protocol-types)); there is no
  `lsp-types` dependency, so that the vocabulary newtypes are what
  deserialization *produces* rather than what a conversion layer produces
  afterwards. Its own dependencies are `serde`, `serde_json`, `url`, `rope`,
  `tree-sitter`, `ignore` (for `ProjectView`'s walk), `rayon` (for
  `ProjectView::scan`, which executes on the pool it is handed at
  construction — `resolution.md` §3), `thiserror` (for `Error`'s derives),
  and `rustc-hash`. This list is the authoritative one; §8.7 refers back to
  it rather than restating it.

  ** `Error` is one enumerated type covering every failure in the system**,
  not an `anyhow` -style boxed `dyn Error`. It lives here rather than in
  `driver` precisely because it spans crates: a handler's failures and the
  driver's are variants of the same enum, which is what lets
  [section 11](shim.md#11-failure-handling) 's response table be an exhaustive
  match rather than a set of string comparisons. `deps.md` section 10 gives
  the shape and the rules that keep it closed —no `Other(String)`, no boxed
  variant, foreign `io` / `serde_json` errors only as `#[source]` beside our
  own typed context. Abstention is emphatically not in it: `Outcome` and
  `AbstainReason` stay separate, per [section 1](#1-handler-interface).
*  ** `similarity` depends on `shared`, and is frozen.** It holds only what is
  ported from the prior implementation —`Occurrences`, `IdentifierParts`, and
  path–namespace scoring (`resolution.md` §5). Nothing is added to it during
  phase 2. It is a body of known-good code that exists before any language
  does, which is why it can be shared without creating the churn that a
  growing shared library would.
*  **There is no other shared resolution code until phase 3.** Two languages
  that need the same helper each write their own, and the duplication is left
  standing. This is the rule `resolution.md` §9 argues for —sharing derived
  from working handlers rather than predicted —carried to its conclusion: with
  per-language loops running concurrently, a shared resolution crate is not
  merely premature, it is a surface two writers would contend on and a source
  of silent cross-language regressions. Extraction is phase 3's job, under
  phase 3's equality constraint.
*  ** `lang_*` depend on `shared` and `similarity` **, plus their own
  `tree-sitter-<lang>` grammar crate. Nothing depends on them except
  `heuristic_jump` and their own `measure_<lang>`.
*  ** `driver` depends on `shared` only.** Everything `shim.md` describes lives
  here. It is generic over the handler set.
*  ** `heuristic_jump` depends on `driver` and every `lang_*`, plus `clap` and
  `tracing-subscriber`. ** Argument parsing and log setup live here rather than
  in `driver` (`deps.md` §11, `shim.md` §13), so `driver` stays a library with
  no opinion about how it was invoked. It is also the single place where the
  language list is enumerated:

```rust
fn main() -> Result<(), shared::Error> {
    let registry = HandlerRegistry::new(vec![
        Arc::new(lang_rust::Handler::new()),
        Arc::new(lang_python::Handler::new()),
        Arc::new(lang_typescript::Handler::new()),
    ]);
    driver::run(registry, Cli::parse())
}
```

### Why `shared` is separate from `driver`

The trait could have lived in `driver` — languages would depend on `driver`,
`driver` would depend on no language, and the stated rule would still hold.
It is split anyway, for two reasons:

*  **Compile times.** Otherwise every language crate transitively pulls in the
  channels, the codec, and the whole proxy just to implement one trait, and
  every edit to the proxy rebuilds every language crate. With ten languages
  that dominates the edit-test loop.
*  **It keeps the rule honest.** With `driver` at the bottom of the graph,
  "handlers may as well reach into the driver for this one thing" is always
  one import away. With `shared` at the bottom and `driver` a sibling, the
  layering violation does not typecheck.

*  ** `measure_core` depends on `shared` only** —not on `driver`, not on any
  language. It is an LSP client, not a proxy, so none of the driver applies to
  it; and it takes its handler as `&dyn LanguageHandler`, so it does not
  depend on the languages it measures.
*  ** `measure_<lang>` depends on `measure_core` and one `lang_*`. ** One per
  language, four lines each. The reason it is a separate crate rather than a
  `[[bin]]` inside the language crate is that a `[[bin]]` shares its crate's
  dependency list: `lang_rust` would acquire `measure_core`, and
  `heuristic_jump` would then link an LSP client into the shipped binary. The
  dependency has to point the other way, which means a separate crate.

  The property this buys is that **a language can be measured without any
  other language building** —see
  [section 7](#one-measurement-library-one-tiny-binary-per-language).

### Adding a language

New `crates/lang_<x>/` depending on `shared` + `similarity` + its grammar,
implementing `LanguageHandler`; `crates/measure_<x>/`, which is four lines; then
one line in `heuristic_jump`. Nothing else in the workspace changes. That is
the whole cost, and keeping it at that is the point of the graph above.

**Phase 1a builds this as an instantiable template**, not as prose. Adding a
language is then a copy and a rename, and — more importantly — the shape every
language crate inherits is fixed once, by hand, before seven of them exist.

```
crates/lang_<x>/
  Cargo.toml          shared, similarity, tree-sitter-<x>. Nothing else
  src/lang_<x>.rs     the Handler impl, longhand
crates/measure_<x>/
  Cargo.toml          measure_core, lang_<x>
  src/measure_<x>.rs  the four lines
```

**No tests.** The corpus is the oracle, it replays without a language server,
and it is made of real repositories nobody here wrote. Hand-built fixtures are a slower,
weaker oracle graded against expectations the same session authored — and an
empty `tests/fixtures/` directory in the template is an invitation to fill it,
which converts a self-graded oracle into the thing a campaign optimises. The
fixture tests `resolution.md` §11 describes are for pinning a specific
behaviour the corpus cannot isolate, such as a shadowing case with three
instances in ten repositories. They are added deliberately, by a campaign that
can say why, and not by default.

### What the template's handler does

Not nothing, and not a baseline either.

It declares its real `language_ids`, `file_extensions`, and `grammar` — those
come from the grammar crate and are correct from the start — so an instantiated
template compiles, links, runs under `measure_<x> replay`, and produces a
complete per-stratum table. **The first measurement of a new language
exercises the whole pipeline**, rather than failing to build and saying nothing
about whether any of it is wired up.

Beyond that it implements exactly one thing, and it is the one thing that is
genuinely language-independent: **deciding whether the cursor is on an
identifier at all.** Is the byte offset inside a named leaf node whose text is
identifier-shaped? That is a grammar-level question with a grammar-level
answer and it needs no resolution logic. A fresh language therefore starts out
already correct on the `NotAnIdentifier` abstention path and zero everywhere
else, which is an honest starting position rather than a flat one.

**That rule is one function in `shared`, not two implementations that agree.**
`measure_core` uses it to enumerate corpus positions
(`data-collection.md` §2) and a handler uses it to answer
`NotAnIdentifier` — and if those two ever disagree, the corpus contains
positions the tool does not consider queries, or the reverse, and the
resulting miscount looks like a resolution failure rather than a definitional
one. This is the same reasoning that puts `ProjectView` and the agreement
predicate in `shared`: where the measurement and the measured must agree, they
share the code rather than the intention.

Everything else abstains, and the abstention is **self-identifying**. A
template that abstained as `NoCandidates` would be indistinguishable in the
metrics from a real handler that searched and found nothing, so a
half-migrated language would look like a language that was merely bad at its
job. The placeholder reports `Stratum::Unimplemented`, which no real handler may
return (`resolution.md` §8), and its presence in a metrics table means the
template has not been replaced — a gate check rather than something anybody
has to notice.

**It is deliberately not a baseline.** A template that did same-file name
search would give every language a non-zero starting point and would be
inherited by all of them — anchoring every first campaign on one structure,
which is exactly what writing the first handler longhand exists to avoid. The
null model is worth measuring; it is not worth shipping in the template that
seven languages grow out of.

### Vendoring the Zed crates

Beyond the mechanical patches below, `vendor/rope` gets one substantive change:
its public API speaks in newtypes — `ByteOffset`, `ByteLen`, and `ByteRange`
instead of `usize` and `Range<usize>`, and `LineIndex` / `ByteColumn` /
`Utf16Column` / `CharCount` instead of the bare `u32`s in `Point`,
`PointUtf16`, and `TextSummary` — so the
vocabulary survives contact with the text rather than being unwrapped at the
boundary. It is a design decision with its own re-sync cost and its own
document: **`rope-modifications.md`**. Read it before touching `vendor/`.

Three things stated here because they change this section's own claims:

* `ByteOffset`, `ByteLen`, `ByteRange`, `LineIndex`, `ByteColumn`,
  `Utf16Column`, and `CharCount` are *defined in* `rope` and re-exported by
  `shared`, since the dependency direction forbids the reverse. `ByteLen` is
  also what a handler counts `bytes_scanned` in — one byte quantity, not
  two.
* The "re-sync is a clean diff" claim is weakened, in the ways that document
  sets out.
* **Upstream's tests and benchmark are kept, not deleted.** We are editing this
  crate, so its own tests are the only independent check that the edit changed
  nothing.

`vendor/rope` and `vendor/sum_tree` are copied, not git-depended: the workspace
sets `publish = false`, so they are not on crates.io, and pinning a rev of a
monorepo crate with no semver guarantee is worse than owning a copy.

The coupling to the rest of Zed is far smaller than `rope/Cargo.toml`
suggests. It lists `util` and `ztracing`, but uses exactly three items:

* `util::is_utf8_char_boundary` — a one-line `pub const fn`
  (`crates/util/src/util.rs:55`)
* `util::debug_panic` — a small macro
* `ztracing::instrument` — an attribute macro
* plus `util::RandomCharIter` in tests

Vendoring `util` whole would drag in `async_zip`, `rust-embed`, `schemars`,
`regex`, and `gpui_util` to support a text data structure. So **there is no
`vendor/util`: those items are folded into `rope` itself.**

An earlier revision kept them in a cut-down crate still named `util`, so that
`rope`'s `use util::...` lines were untouched and `rope` needed no patching at
all — which kept re-syncing a clean diff rather than a merge. That reasoning
was sound and is now obsolete: `rope-modifications.md` rewrites rope's public
API for the newtypes, so the crate is patched throughout regardless, and five
import lines are not worth a third vendored crate with its own manifest,
license file, and provenance entry — under the most accretion-prone name in
any workspace. `sum_tree` is unaffected; it does not depend on `util` at all.
[That document](rope-modifications.md#folding-vendorutil-in) has the placement
and the attribution rules.

`ztracing` is not vendored. Its `instrument` is either `tracing::instrument`
or a no-op passthrough depending on a cfg, and `rope` already depends on
`tracing`, so the one import is redirected there. That is a single-line patch
to `rope`, recorded as such.

`sum_tree` needs no patching, and the newtype work in
`rope-modifications.md` does not change that: `sum_tree::Dimension` is
generic over the summary type, so `ByteOffset`'s impls live in `rope`. Its
`tree_map.rs` is unused here and can be dropped; a whole-file deletion still
leaves a clean diff.

`vendor/README.md` records, per crate, the upstream revision it was taken at,
the exact patches applied, and — for the items lifted out of `util` — where
each came from and under what license, so that a future re-sync can tell at a
glance whether upstream changed anything that matters.

**Licensing consequence, stated plainly:** `rope` is GPL-3.0-or-later, so the
shipped binary is GPL-3.0-or-later. That is a project-level commitment
following from vendoring, not a detail, and `high-level.md` says so under
"License".

It does **not** follow that our own crates are GPL. `crates/*` are `MIT`:
vendoring GPL code does not transfer copyright in code we wrote, and MIT is
GPL-3.0-compatible, so an MIT crate combines into a GPL binary needing no
extra grant. Marking them GPL would volunteer a restriction that `rope`
imposes on the *combination* only.

The point of keeping them MIT is that `rope` is the sole GPL input —`sum_tree`
is Apache-2.0, which is one-way compatible into GPL-3.0. So if `ropey` ever
wins the argument in `deps.md` §5, the whole workspace becomes permissively
licensable without relicensing a line. Relicensing later requires every
contributor's agreement; declaring MIT now costs nothing. `deps.md` §5 has the
per-crate table and the caveats.

## 10. Testing

The proxy-side suite is [shim.md section 12](shim.md#12-testing). What
belongs here is everything phase 1a can run before a shim exists.

* **Snapshot version invariant.** For a randomised sequence of edits and
  dispatches, assert that `snapshot.tree()` always parses to a tree whose
  extent matches `snapshot.text`, whatever version the cached base was at.
  This is the invariant that makes the private-`base` design worth having,
  and violating it produces confidently wrong answers rather than errors.
* **Position encoding property tests.** Random text with astral-plane
  characters, round-tripped UTF-8/UTF-16/byte offsets against a reference.
* **Protocol type differential tests.** Every message in the golden corpus
  deserialized with both `shared::proto` and `lsp-types` (a dev-dependency
  only), asserting the fields we model agree. Plus a dedicated case per
  untagged union in [section 8.5](#85-the-untagged-unions-are-the-actual-risk),
  since those are where a hand-written wire type actually goes wrong.
* **Fuzz the frame codec**, including split reads, oversized headers, and
  bogus `Content-Length`.
