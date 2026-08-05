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
routing, server health, divergence reporting, dispatch,
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

**The text-shaped ones are `rope`'s, not `shared`'s.** `Offset`,
`ByteLen`, `ByteRange`, and `LineIndex` are *defined in* the vendored rope and
re-exported here, because `shared` depends on `rope` and the dependency cannot
run the other way — `rope-modifications.md` §2 has the argument, and the same
goes for `ByteColumn`, `Utf16Column`, and `CharCount`, which handlers do not
use. Every other crate says `shared::Offset` and never has to know. They
appear here because this is the seam they are part of:

```rust
// vendor/rope, re-exported by shared
pub struct Offset(pub usize);   // a position; never a UTF-16 offset
pub struct ByteLen(pub usize);      // a quantity, distinct from a position
pub struct ByteRange { pub start: Offset, pub end: Offset }
pub struct LineIndex(pub u32);      // zero-based line

impl ByteRange {
    pub fn contains(self, at: Offset) -> bool;
    pub fn overlaps(self, other: ByteRange) -> bool;
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
/// gets it for free from the tree-sitter node it already verified, and
/// because the redundancy is what section 6's predicate compares and what
/// detects a target file that moved -- see section 8.4.
/// Constructed only via `Location::at_node`, so the two cannot disagree:
/// the fields are private and read through `uri()`, `range()` and `line()`
/// (`state/decisions/conformance-004.md`).
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    uri: DocumentUri,
    range: ByteRange,
    line: LineIndex,
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

    /// `Err` is a *failure*, never a decision. Abstention lives in `Outcome`.
    /// See "Failure and abstention are different returns" below.
    fn goto_definition(&self, q: &Query<'_>) -> Result<Outcome, Error>;
}

pub struct Query<'a> {
    pub doc: &'a DocumentSnapshot,       // rope + tree, immutable
    pub position: Offset,
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
    /// hide that. Private, with one constructor per situation, so that the
    /// third case -- a caller that knows which server it is standing in for
    /// and passes `None` anyway -- is not expressible.
    id: Option<ServerId>,
}

impl ServerProfile {
    /// No oracle at all, so there is no identity to resolve rather than one
    /// we failed to resolve.
    pub const fn standalone() -> Self;
    /// Proxying the child on this command line, as the shim was invoked.
    pub fn proxying_command(program: &OsStr, arguments: &[OsString]) -> Self;
    /// Standing in for the server a corpus run names -- `measure`'s
    /// `--server`, which is a `servers.toml` key because a replay has no
    /// child to look at.
    pub fn proxying_named(name: &str) -> Self;
    pub fn id(&self) -> Option<ServerId>;
}

/// Interned server identity, resolved from the child's command name at
/// startup.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ServerId(&'static str);

pub enum Outcome {
    Committed {
        locations: Vec<Location>,
        confidence: Confidence,
        strata: Strata,
        trace: Trace,
    },
    Abstain { reason: AbstainReason, strata: Strata, trace: Trace },
}

/// Section 7's `stratum_prior` and `stratum_final`, which are two fields and
/// not one. The fields are private because `refine` is the only way to make
/// them differ.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Strata { prior: Stratum, settled: Stratum }

impl Strata {
    /// Before the search: the two agree, because nothing has refined
    /// anything.
    pub fn from_reference(stratum: Stratum) -> Self;
    /// The one refinement section 8 of `resolution.md` permits.
    pub fn refine(self, refinement: Refinement) -> Self;
    pub fn prior(self) -> Stratum;
    /// Section 7's `stratum_final`, spelled `settled` because `final` is
    /// reserved.
    pub fn settled(self) -> Stratum;
}

/// The only two strata a search may refine *to*: neither is knowable before
/// it runs, which is the whole reason a refinement is permitted at all. A
/// two-variant enum rather than a `Stratum`, so that refining to a class the
/// reference already decided does not compile.
pub enum Refinement { AmbiguousName, ExternalDependency }

/// Everything else section 7 calls handler-reported: `margin`, `considered`,
/// `stages`, `stage_us`, `bytes_scanned`, `files_parsed`. Write-only from a
/// handler's side -- the fields are private and the one reader consumes it,
/// which is the strongest available form of section 7's "nothing branches on
/// it, ever". Boxed and not allocated until something is reported, so the
/// commonest abstention does not pay for a channel it never writes to.
pub struct Trace(Option<Box<TraceParts>>);

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
}

/// Stratum -> minimum Confidence. Empty in v1, where `decide` returns
/// `Committed` for every input. Handlers never construct `Outcome::Committed`
/// themselves; every path ends here.
pub struct CommitPolicy { /* ... */ }

impl CommitPolicy {
    pub fn decide(&self, strata: Strata, confidence: Confidence,
                  locations: Vec<Location>, trace: Trace) -> Outcome;
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
*  **Failure and abstention are different returns, and that is why the
  signature is `Result<Outcome, Error>`.** The bullet above says abstention
  must not share a type with failure; the earlier signature `-> Outcome` did
  not give failure a type at all, so it leaked back in the other direction —
  `AbstainReason` grew `HandlerError` and `NoParse`, and a handler doing the
  `?` propagation `CLAUDE.md` requires had nowhere to propagate *to*. Both
  variants are gone. A handler that fails returns `Err(Error)`; a handler that
  declines returns `Ok(Abstain { .. })`; the two are no longer spellable as
  each other.

  What reaches the editor is the same either way — the dispatch wrapper
  converts an `Err` into an abstention on the wire, because a failure is not
  something a user can act on and the shim's job is to get out of the way
  ([`shim.md` §11](shim.md#11-failure-handling)). What differs is the
  *record*: a converted failure is written as `decision: "failed"` with the
  error's class, never as an abstention
  ([section 7](#7-observability-and-the-corpus-scan)). Without that, a
  stratum with no coverage because resolution is hard and a stratum with no
  coverage because the handler is panicking are the same row, which is
  precisely the distinction `resolution.md` §8 says the reasons exist to
  make.

  **One error class is mapped back, and it has to be: the deadline.**
  `ProjectView` fails a read whose deadline has already expired
  (`resolution.md` §3), so a handler doing ordinary `?` propagation surfaces
  an expiry as `Err` — and a deadline expiry is a *decision*
  ([section 5](#5-deadlines-and-abstention)), the one latency-shaped
  abstention `high-level.md` allows. The dispatch wrapper therefore converts
  that class, and only that class, into
  `Abstain { reason: Deadline, .. }` and records it as an abstention. Getting
  this backwards would log every deadline-aborted query on a large repository
  as a handler failure, which is both false and exactly the wrong direction:
  the abstention rate attributable to the deadline is a number
  `resolution.md` open question 15 says to watch from the first corpus run.
* **`AbstainReason` carries no resolution vocabulary.** Earlier revisions had
  `UnsupportedRole { role: ReferenceRole }` and `External { name: Namespace }`,
  which would have dragged two of `resolution.md`'s internal types into the
  seam — and `ReferenceRole`'s variant set is a claim about what kinds of
  reference exist, which is exactly the per-language decision
  [`resolution.md` §1.2] refuses to centralise. The variants are unit or carry
  primitives; the detail a handler knows stays in the handler, and reaches the
  metrics through the trace record rather than the seam.
*  ** `Strata` and `Trace` are reported on both arms**, because coverage per
  stratum is meaningless without knowing which stratum the abstentions belonged
  to — and because a stratum with no coverage and a stratum whose searches all
  cost 40ms before abstaining are different findings, of which the abstaining
  one is the more interesting.

  **The stratum is two fields and not one**, for the reason
  [section 7](#7-observability-and-the-corpus-scan) gives: coverage is reported
  on the prior so the denominator is fixed by the reference and does not move
  when the implementation changes, and precision on the settled one so an
  answer is judged against the class it turned out to be. `Strata` makes that
  rule structural rather than remembered — `refine` takes a `Refinement` and
  not a `Stratum`, so a search cannot claim a class the reference had already
  decided.

  **That the return value is the reporting channel at all is
  `conformance-013`**, which was escalated because widening `Outcome` is a
  change to the frozen seam, and answered in favour of it. The alternative
  weighed was an out-parameter `trace: &mut Trace` on `goto_definition`; the
  deciding argument against was this section's own, that an out-parameter is
  readable *during* the query and section 7 says nothing branches on the trace,
  ever. A trace a handler can read back is one it can condition on, and then
  the record stops describing the run and starts shaping it. The cost accepted
  with it is the one visible above: two more fields at every construction site
  in every language crate, and a fourth parameter on `decide`.
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
  matches nothing.

  **Comparison is `str` equality on the interned text, and must not be pointer
  identity.** The registry resolves an incoming `languageId` against ids a
  `lang_*` crate declared, and those two `"rust"`s are literals in different
  crates: comparing addresses would have them differ, and the language would go
  quietly unhandled — a failure with no error anywhere, since an unresolved id
  is exactly what an unsupported language looks like. What interning buys is
  therefore a cheap comparison over a short string with no allocation, not one
  over an address. `crates/shared/tests/` has
  `a_language_id_compares_by_text_and_not_by_address`, which leaks a
  runtime-built `"rust"` so the compiler merging two equal literals cannot
  answer the question for it.
*  **Handlers get a snapshot, not a lock —literally, with no primitive in it
  at all.** `DocumentSnapshot` holds a cloned `Rope` and a `Tree`, both O(1)
  to clone, taken at dispatch —so a handler is immune to edits that arrive
  while it runs, and `core` is never blocked. It contains no cell and no
  interior mutability, which is what lets `CLAUDE.md`'s "no locks anywhere"
  stay literally true on the one path where it was nearly not
  ([section 2](#snapshots-are-o1-to-take-and-are-parsed-before-a-handler-sees-one)).
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

  **What holds this is review and not the type system, and it is worth being
  exact about which.** `Outcome::Committed` is a public variant with public
  fields, so a `lang_*` crate *can* build one without going through the policy;
  what it cannot do is build one the policy would have refused, because in v1
  the policy refuses nothing. The two become distinguishable only when a floor
  arrives, which is the moment the funnel is for — and the check available
  before then is mechanical rather than architectural: a source scan over
  `crates/lang_*` for the construction, in the shape `driver/tests/seam.rs`
  already uses for the wire vocabulary. Making it type-level instead means a
  private variant and a constructor, which is a change to the frozen seam and
  is not made here on the strength of a rule nothing has yet broken.
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

  Like the commit funnel below, this is held by review rather than by the
  types, and for once that is not obvious from the outside: `ServerId`'s field
  is private and there is no public constructor from a string, which reads as
  though an identity cannot be named at all. It can — `ServerId::KNOWN` is a
  `pub const` of the whole matrix and `ServerId::from_name` is public, both so
  that `measure_core` can resolve `--server` — so the mechanical check is the
  same shape as the other one:
  `crates/shared/tests/handler.rs::no_language_crate_asks_which_server_it_is_standing_in_for`
  scans every `lang_*` source for the identity and for `ServerProfile::id`.

  **The identity is a private field behind a constructor per situation**,
  rather than a public `id: Option<ServerId>`. The absence has to be
  representable — standalone has no oracle, and a proxied server we have no
  profile for is a different thing from one we do — but with a public field a
  third case is representable too, and it is the one that silently loses
  information: a call site that *knows* which server it is standing in for and
  passes `None` anyway. `standalone()`, `proxying_command(..)` and
  `proxying_named(..)` are the three situations that exist, and none of them
  can be spelled as another.

## 2. Document snapshots

The immutable view of one document that a handler is given. `driver` builds
one at dispatch and `measure_core` builds one per recorded position; both go
through the same constructor in `shared`, which is what keeps the corpus
scoring the code that ships (see
[section 7](#7-observability-and-the-corpus-scan)).

### Snapshots are O(1) to take, and are parsed before a handler sees one

Snapshot-on-dispatch is only viable because nothing is copied. It comes in two
steps, and the split is what keeps `core` doing O(1) work while the parse still
happens inside the worker and inside the deadline:

```rust
/// What `core` builds at dispatch. Three refcount bumps and a struct move.
pub struct SnapshotSeed {
    pub uri: DocumentUri,            // which document this is
    pub text: Rope,                  // structural sharing; O(1)
    pub version: DocumentVersion,    // the version above
    pub language_id: LanguageId,
    /// Cached tree at some older version, plus the edits that bring it
    /// up to `version`. Never handed to a handler.
    base: Option<(Tree, Arc<Vec<InputEdit>>)>,
    grammar: tree_sitter::Language,
}

/// What a handler is given. The tree is already correct for the text.
pub struct DocumentSnapshot {
    pub uri: DocumentUri,
    pub text: Rope,
    pub version: DocumentVersion,
    pub language_id: LanguageId,
    tree: Tree,                      // plain field: no cell, no interior mutability
}

impl SnapshotSeed {
    /// Reparses incrementally from `base`, or parses from scratch if there
    /// is none. Called by the worker, never by `core`.
    ///
    /// The deadline is how "inside the deadline" above is paid for: the
    /// parse is the one piece of work on the query path that no handler
    /// can poll around, because it happens before there is a handler.
    pub fn realise(self, deadline: &Deadline) -> Result<DocumentSnapshot, Error>;
}

impl DocumentSnapshot {
    pub fn tree(&self) -> &Tree;     // infallible; it is a field
}
```

* `uri` is on both because a handler that resolves a local binding returns a
  `Location` *in the document it was given*, and `Location::at_node` takes a
  `DocumentUri`. [Section 1](#the-trait)'s `Query` carries no other route to
  one — and `ProjectView::root_of` wants the same value — so without it the
  seam cannot express its own commonest answer. It is the document's identity
  rather than its content: one short string clone at dispatch, not a copy of
  anything that scales with the file.
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

**`DocumentSnapshot` contains no synchronisation primitive, and that is the
point of the two-step shape.** Handlers fan out across candidate files
([section 10](shim.md#10-parallel-dispatch-and-resource-limits)), so `&Query`
— and therefore `&DocumentSnapshot` — crosses threads and must be `Sync`. An
earlier revision got that by memoising the parse in a `std::sync::OnceLock`,
which works and is `Sync`, but is a blocking primitive on the query path in a
design whose stated rule is that there are no locks anywhere: two fan-out
workers calling `tree()` at once would have had one of them wait.

Parsing eagerly removes the question instead of excusing it. `tree` is a plain
field, `Sync` follows from tree-sitter declaring `Tree` both `Send` and `Sync`
(`binding_rust/lib.rs:3908`), and there is nothing to contend on.
`Node<'tree>` is likewise `Sync`, so nodes borrowed from the shared tree pass
freely between fan-out workers.

**Eager costs nothing real**, which is why this is a simplification rather
than a trade:

* Every query needs the tree anyway — stage 0 is reference extraction from it
  (`resolution.md` §2), so there is no path that would have skipped the parse.
* `resolution.md` §1.1 already asked handlers to call `tree()` even on paths
  that abstain immediately, so the cache is warm for the next query on that
  document. Eager makes that automatic instead of a rule someone can forget.
* The parse is usually incremental from a cached base, and often the base is
  already current.

**And it moves the parse failure somewhere better.** `realise` returns
`Result`, so an unparseable document fails at dispatch and never reaches a
handler — `Err(Error::Parse)`, recorded as a failure
([section 7](#7-observability-and-the-corpus-scan)) rather than as a decision
the handler made. `tree()` is then infallible, which deletes a `Result` from
the busiest call in every handler.

**A parse abandoned on the deadline is the other return, and is emphatically
not that one.** `realise` breaks out through tree-sitter's parse progress
callback and reports `HandlerError::DeadlineExpired`, which the dispatch
wrapper maps back to `Abstain { reason: Deadline }` like any other expiry
([section 1](#the-trait)). Recording it as `Error::Parse` would put "this
repository has a file too large to parse in 40ms" and "this grammar is broken"
in the same row of the metrics table, which is exactly the distinction
[section 7](#7-observability-and-the-corpus-scan) is built to keep.

It is best-effort rather than tight. tree-sitter checks the callback once per
100 parser operations (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `src/parser.c`),
so a document small enough to finish inside one interval observes no deadline
at all and returns a tree however expired the query was. That is not a hole:
[section 5](#5-deadlines-and-abstention)'s hard cap is what makes a late
answer harmless, and this only stops the *work* — the CPU a cancelled query
would otherwise keep burning while the proper LSP waits for it.

### Text and tree can never disagree

The cached tree is usually *older* than the text: `core` caches a tree at v3,
the user types, and a query dispatches at v5. Handing a handler both the v5
text and the v3 tree would be a trap — every offset in that tree is wrong for
that text, and the mismatch is invisible until it produces a confidently
wrong answer.

So the stale tree never leaves the seed. `base` holds it together with the
edits that reconcile it, and `realise` is the only way across:

1. It applies `edits_since_parse` to a **private clone** of the base tree via
   `Tree::edit`, then reparses against the v5 text with the edited tree as the
   starting point — a normal tree-sitter incremental parse.
2. With no base, it is a full parse.
3. The result becomes `DocumentSnapshot.tree`, and the seed is consumed.

**A handler cannot obtain a tree that does not match `text`**, because there
is only one tree and it was produced from that text. Not a rule about how to
use the type — a property of the type. The parse is paid inside the worker and
inside the deadline, never in `core`.

Getting the result back to `core` is explicit rather than implicit: the
dispatch wrapper — not the handler — sends `Parsed { uri, version, tree }` to
`core` after `realise` succeeds. The handler is not involved and cannot
forget. `core` caches it, so the next query on that document starts warm.

**Both consumers realise the same way.** `realise` lives in `shared` alongside
the seed, so `measure_core` builds its snapshots through it exactly as the
driver does — which is the property [section 7](#the-corpus-scan-is-a-separate-program)
depends on when it argues that the corpus scores the code that ships. `core`
builds seeds and never realises one; that is what keeps it free of parsing.

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
`proto::WirePosition` has private fields and yields a `Offset` only when
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

  It also catches **deletions before a query pays for one**, which is the whole
  of what it buys over the on-demand path: a rescan discovers files that
  appeared, and a stale entry for a file that was removed surfaces as a failed
  read first.

  An earlier revision said the on-demand trigger *structurally cannot* catch a
  deletion, which cannot stand beside the next paragraph: if it were true then
  something would depend on the watcher, and standalone — which has none, since
  the bullet after this defers `notify` — would have no backstop at all, and a
  removed candidate would fail every later query over the same candidate set
  for as long as the process lived. What is true is narrower and is the
  *timing*: the on-demand path learns about a deletion the expensive way, one
  failed query later. See the second signal in the trigger below.

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
*  Otherwise invalidated on demand, by what a query itself ran into — it either
  finished without a good candidate or failed to read one that is no longer
  there, and both are the signal that the file list may be stale, so a rescan is
  kicked off in the background. The query that triggered it gets nothing better
  for its trouble, since it cannot wait for a rescan inside the deadline, but
  the next query on that spot sees a fresh list.

  The mechanism is `AbstainReason::NoCandidates` specifically, not any
  abstention (`resolution.md` §8). That reason means an *exhaustive* search
  found nothing, which is evidence about the file list; `Deadline` means the
  search was cut off, which is evidence about nothing, and rescanning on it
  would spend I/O in the window that just proved to be short of it.

  **A failed read is the second signal, and it is the one that covers a
  deletion.** A search reads every candidate and `resolution.md` §4 forbids
  reporting a partial scan, so a candidate that vanished between the walk and
  the read fails the query outright rather than abstaining — there is no
  `NoCandidates` to observe. What that failure names is a file the list holds
  and the filesystem does not, which is evidence about the list in exactly the
  way an exhaustive miss is, so it schedules the same debounced rescan and the
  query after it searches a list the removed file is no longer in. That is what
  makes "a failed read" the whole cost of a deletion rather than the first of
  an unbounded number of them.

  Narrowly: the read has to have failed *because the file is gone*. A
  permissions error, or a candidate that is not text, is a fact about the file
  and not about the walk — the walker returns the same entry on the next pass,
  so rescanning on one would be a rescan per query for as long as it lasts.

  Both halves of the classification live on the types being classified —
  `AbstainReason::file_list_evidence` and `Error::file_list_evidence`, in
  `shared` — because the sub-enums are `#[non_exhaustive]` and the same match
  written at the consumer would need a wildcard arm, which is the arm that
  silently classifies the next variant instead of failing to compile.

  A user who did not get an answer generally asks again, so the rescan
  usually lands about when it is needed. Rescans are debounced, so a burst of
  misses triggers at most one, and the two triggers share one debounce rather
  than one each.
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
`DefinitionSite` — `(DocumentUri, LineIndex)` — taking `targetSelectionRange`
for links.

Not to a set of `Location`, and the reason is the paragraph below: `Location`
is byte-based, a wire range's `character` is in the negotiated encoding, and
converting one to the other needs the target document's text. A predicate that
normalized into byte space would therefore have to read, which is exactly what
this one may not do. `DefinitionSite` is the pair that gets compared and
nothing more, so the normalized form and the comparison cannot drift apart.

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
| Shim committed nothing, child answered | differs | `unrelated` |
| Both empty | matches | — |

The 3-line tolerance is deliberate: at that distance the correct definition is
on screen and the user is already reading it, so scoring it as wrong would
measure something nobody experiences as wrong. The tiers below it are the
error severity classes `high-level.md` reports, and are what a future budget
would be attached to.

The second-to-last row is the one case with no shim location to classify a
severity from, since [below](#both-sides-are-sets) reads severity off the
top-ranked one. It takes `unrelated` rather than a fourth class: it is the
pessimistic choice, so it cannot flatter the precision numbers, and it is
symmetric with the row above it, which is the same situation with the sides
swapped.

**What makes two files "the same module tree" is not settled** —
`state/decisions/conformance-009.md`, where the provisional reading is "the
same containing directory", which is the strongest test available to something
that may not read the disk and does not know the language. It decides the
split between the two severity classes `high-level.md` attaches separate
budgets to, so it is a measurement question rather than a coding one.

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
  "failure": null,
  "stratum_prior": "explicitly_imported",
  "stratum_final": "explicitly_imported",
  "confidence": 0.94,
  "margin": 0.62,
  "considered": 7,
  "stages": ["ref:Type", "scope:miss", "import:Declared(crate::ast)",
             "verify:9->3", "rank:margin=0.62"],
  "bytes_scanned": 1841203,
  "files_parsed": 14,
  "queued_us": 400,
  "stage_us": {"reference": 12, "scope": 40, "imports": 900, "search": 6800,
               "verify": 500, "rank": 48},
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

**`decision` has four values, not two**: `committed`, `abstained`, `failed`
and `shed`. The third is what the handler seam's `Result<Outcome, Error>`
([section 1](#the-trait)) exists to make recordable. On the wire a failure is
served as an abstention, because that is what is useful to a user; in the
record it must not be one, or the per-stratum table cannot tell a hard
stratum from a broken handler. `failure` names the `Error` sub-enum that was
converted — `"Parse"`, `"Project"`, `"Handler"` — and is `null` otherwise.
The whole error is deliberately not carried: the class is what a metrics table
can group on, and the detail is already in the log.

**`shed` is the fourth and is the only one that is not the handler's.**
[`shim.md` §10](shim.md#10-parallel-dispatch-and-resource-limits) gives the
dispatch pool two limits beyond its size — a cap on queries in flight, and no
heuristic work while `core` is behind on its inbox — and both are refusals to
run the query at all. The other three all answer *what did the query say*, and
a query that was never attempted says nothing.

It is not an `AbstainReason`, and that is `core-026`, accepted on option D.
`AbstainReason` is the *handler's* vocabulary — what the language said when it
declined, which is why [section 1](#the-trait) can call four of its variants
facts about the code and single out `Deadline` as the exception. A shed query
is not the handler's event, and a sixth variant there would be one no handler
can ever return, so every `lang_*` match would grow an arm for an unreachable
case. Nor is it `failed`, which would make this column say a shim working
exactly as designed was broken — the merge the paragraph above refuses.

What it buys is a number. `high-level.md`'s posture is that blowing the budget
must cost coverage and never correctness, and that is only auditable if the
coverage lost *to load* is visible as such: a shed rate of its own, rather than
a third meaning in a column `resolution.md` §8 built to separate "this class is
hard" from "this handler is broken". Which limit fired goes in `stages`
(`shed:in_flight`, `shed:core_behind`), beside the abstention reason and for
the same reason a second column is not added for it — and the two are kept
apart there because they are different findings, one saying this process is
answering as many queries at once as it is willing to and the other that the
prime invariant is under pressure.

A shed query has no stratum, so its two stratum columns are `null` by the rule
above. That is not a coincidence of two changes landing together: nothing
classified it because nothing ran.

**`position` is a byte offset**, like every other position inside the shim
([section 8](#8-protocol-types)). It is what `data-collection.md` records
and what `measure replay` joins on, so a line/column pair here would need a
conversion in the one place the two halves of the metric have to line up
exactly.

**The stratum is two fields, not one.** `resolution.md` §8 assigns a stratum
a-priori from the reference, then permits one refinement during search — to
`AmbiguousName` or `ExternalDependency`, neither of which is knowable before
the search runs.

*A-priori* is about the **rule**, not about who evaluates it. The handler
computes both fields and reports them, as the paragraph above says; what makes
the prior stable is that its rule reads only the query and the reference, and
never what the search found. Two consequences follow, and the second is easy
to miss: the prior does not move when the implementation changes, **and it is
knowable without the search finishing.** A query whose outcome is discarded
after the fact — by §5's hard cap, say — has not thereby lost its prior. The
prior was never the outcome's to carry away, and code that reads it off a
completed outcome is taking the only path that happens to be convenient rather
than the only one there is.

Coverage is reported on `stratum_prior` so the denominator
is fixed by the reference and does not move when the implementation changes;
precision is reported on `stratum_final` so an answer is judged against the
class it turned out to be. One field cannot do both, and collapsing them
makes `high-level.md`'s central table non-comparable across versions — the
one property it needs.

**Both stratum fields are nullable, and `null` is not a tenth stratum.** Some
queries end with nothing having classified them: a parse abandoned on the
deadline before any handler ran, and a handler that returned `Err`, which has
no `Outcome` for a stratum to be on. `resolution.md` §8's rule for the prior is
per-language by construction, so the driver cannot evaluate it without the
handler that owns it — there is no value to write, and any name is a guess.

The guess it used to make was `Stratum::Unimplemented`, which is the *language
template's* stratum and is self-identifying on purpose
([section 9](#adding-a-language)): its presence in a metrics table means the
template has not been replaced. So a real handler that missed its deadline, or
one that was thoroughly broken, reported an unreplaced language crate — and
under load a real handler produces that row. `null` says the true thing in the
place the absence actually lives, and it forces each consumer to decide what to
do with it rather than letting it be grouped away silently. This is
`core-025`, accepted on option B; option C is the other half, and narrows what
reaches this state to the abandoned parse by having `ProjectView`'s expiry
carry out the prior the handler had published
(`resolution.md` §3's `classified`).

A consumer that groups on either field therefore gains a bucket that is not a
stratum. It belongs *beside* a per-stratum table rather than in it — a tenth
row would read as a kind of reference, which is exactly what makes `null` the
honest shape and not merely the convenient one — and it must be reported
rather than dropped, because `high-level.md`'s posture is that blowing the
budget costs coverage and never correctness, which is only auditable if the
coverage lost is visible as such. Splitting it by `decision` is what keeps
"the parse ran out of time" and "the handler is broken" from becoming one
number, which is the merge this section spends a paragraph refusing above.

**`margin` and `considered` are the features a floor would be set on.**
Nothing reads them in v1. They are recorded because a threshold can only be
derived from data collected while nothing was being gated, and a corpus run
that kept only the collapsed `confidence` could never answer *what would a
floor have cost?* — which is the question the permissive posture exists to
ask (`resolution.md` §7.1).

**Latency is recorded at every point it can be, and gated at none.**
`high-level.md` reports latency per stratum, `resolution.md` §2 predicts that
stages 0–2 are sub-millisecond and everything from 3 on is where the tail
lives, and `high-level.md`'s value weighting turns on how slow the *real*
server was. None of that is answerable from one number per query, so the
record carries several and the rule is to write down whatever can be measured
wherever it can be measured:

| Field | Measured | Where it comes from |
|---|---|---|
| `queued_us` | request arrival → dispatch into a worker | driver only; zero in replay |
| `stage_us` | wall clock per pipeline stage, handler-supplied | both |
| `heuristic_latency_us` | dispatch → outcome, the handler's whole cost | both |
| `lsp_latency_us` | the real server's send-to-receive time | `collect` only, frozen (`data-collection.md` §4) |

`queued_us` exists because [section 5](#5-deadlines-and-abstention) starts the
deadline at *arrival*, not at handler entry — a handler given its full budget
that started 200ms late has already blown it from the user's point of view, and
without this field that shows up as a fast handler and an unexplained
abstention.

`stage_us` is what makes a latency finding actionable rather than merely true:
it is the difference between "p99 is 700ms" and "p99 is 700ms and 95% of it is
stage 5". It shares `stages`' rules — bounded, nothing branches on it, and it
is an *observation*, so it does not have to be reproducible the way the rest of
the record does.

Two things this deliberately does not do. **Nothing is gated on any of it**:
phase 2a optimises quality and records cost without enforcing it
(`loops.md` §10), and that is unchanged. And **none of it is trusted on a
loaded machine** — under parallel loops these are noisy, which is exactly why
they are reported beside the deterministic work counters rather than instead of
them. A number that is only sometimes meaningful is still worth writing down;
it is not worth thresholding.

**`stages` is the handler's own account of what it did**, and it is the field
that makes a failure diagnosable rather than merely counted. An ordered list of
short labels the handler appends as it goes: which role the reference got, what
each stage found or missed, how many candidates survived verification. The
vocabulary is entirely the handler's — this is the sanctioned channel
[section 1](#the-trait) means when it says the detail a handler knows "reaches
the metrics through the trace record rather than the seam", and it is why
`AbstainReason` can stay free of resolution vocabulary without that detail
being lost.

Three rules keep it from becoming a dumping ground. It is **bounded** — a small
fixed maximum number of short labels, truncated rather than grown. **Nothing
branches on it**, ever, exactly like `bytes_scanned`; a handler that read its
own stage log back would have made the answer depend on it. And it is
**stable across runs for the same input**, because the handler is deterministic
([`resolution.md` §1.3](resolution.md#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)),
which is what lets failures be *grouped* by it rather than merely listed —
see [below](#the-table-is-not-enough-a-replay-has-to-show-its-failures).

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
driver.** The proxy and the health model are not under test — resolution
accuracy is. So as long as `measure_core` builds its
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

**Why not a `[[bin]]` inside `lang_<lang>` instead?** It is the more usual
Rust layout, and it was asked for directly, so the answer belongs here rather
than in a decision record. Cargo has no bin-only dependency: a `[[bin]]` in
`lang_rust` makes `measure_core` and `clap` dependencies of the *crate*, and
`heuristic_jump` depends on every `lang_*`, so the shipped shim would link the
measurement client's LSP stack, JSON handling and CLI. `optional = true` with
`required-features` escapes that on paper, but feature unification across a
workspace can enable the feature for a build that only wanted the library, and
the symptom is a larger binary rather than an error — a guard whose failure is
invisible is not a guard.

Two smaller things go with it. `loops.md`
[section 11](loops.md#measure_lang-size-covers-the-size-gap) makes the
stripped size of `measure_<lang>` the size proxy *because* it is built in
isolation; a binary sharing the shim's dependency set does not measure what
that ratchet reads. And `loops.md`
[section 13](loops.md#mechanics-isolation-in-four-layers) gives
`measure_<lang>` to the conformance loop rather than the language loop,
because a loop must not own the code that scores it — a path glob can carve
`src/bin/` out of a crate the language loop otherwise owns, but a directory
boundary states it once instead of relying on two globs staying in the right
order.

The saving being declined is one directory and one `Cargo.toml`, against a
dependency boundary the shipped binary's size depends on.

Aggregating across languages — the combined table, the frontier — is done over
the emitted records, which are data. Nothing that aggregates needs to link a
handler at all.

### Two modes: collect and replay

Driving a real language server over ten repositories is hours of wall clock,
and it produces an answer that does not change when the handler changes. The
proper LSP's answer for a given position at a given commit is a *fact about
the corpus*, not about our code, so it is collected once and frozen.

`measure` therefore has two *modes* — one that needs a server and one that does
not — across the three subcommands [the command line](#the-command-line) lists:

* **`collect`** — spawn the server, drive `didOpen` across the repository, walk
  the enumerated positions asking the LSP for each, write `truth.jsonl`. Slow,
  run rarely, output is a frozen artifact in the corpus root, never in the
  repository. The only mode that needs a server.
* **`replay`** — read `truth.jsonl`, reconstruct the `DocumentSnapshot` and
  `Query` for each recorded position, run the handler, classify agreement,
  emit the metric table. No server, no network, no `didOpen` round trips.

**Enumerating the positions is neither of these, and that is why it is a third
subcommand rather than the first half of `collect`.** An earlier revision of
this section put it inside `collect`, which cannot be right: positions are
enumerated **once per repository and not once per server**
([`data-collection.md` §2](data-collection.md)), and every server run consumes
the same `positions/<repo>.jsonl`. Enumerating inside `collect` makes the
position set a function of which server was being collected against, and then
two servers' answers cannot be aligned — which takes away the join that the
per-server agreement and divergence split in
[section 7](#7-observability-and-the-corpus-scan) is built on. So the modes are
a two-way split and the subcommands are a three-way one, and they are not the
same partition: `enumerate` shares `replay`'s side of the mode split, since it
reads the grammar and the repository and nothing else.

The record in this section is the join, and the two modes supply its two
halves: `collect` supplies the oracle's — the answer and how long it took —
and `replay` fills the heuristic side and computes `agreement` and `severity`
with the same predicate the driver uses. **Only `replay` writes the record.**
A truth row is its own smaller shape, because the oracle's answer is stored as
the raw JSON the server sent rather than as a projection written back out:
[section 8.2](#82-what-replaces-it-and-why-it-is-smaller-than-it-sounds) gives
the read projections no `Serialize` at all, so a truth file of half-filled
records could not hold what replay has to hand the same deserializer the shim
reads a live answer with. What has to survive the join is the *content* of the
`lsp_*` columns and not their spelling on the way.

A completed replay row is byte-comparable with a
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

* **`truth.jsonl` carries its provenance in a header record**: the
  repository's corpus *name* and its commit, the language, the server name and
  version, the grammar revision, the `measure` version that wrote it, and
  whether the collection ran to completion. Replay refuses to run against a
  truth file whose repository commit does not match the checkout, rather than
  silently reporting metrics for positions that have since moved.

  **The repository is named and not located**, and the distinction is the
  whole reason a truth file is portable. Every other identity in the corpus is
  a name — [`data-collection.md` §0](data-collection.md) keys `repos/<name>/`,
  `positions/<name>.jsonl` and `truth/<server>/<name>.jsonl` all the same way —
  and the root those sit under is supplied at run time by `--corpus`, which
  [the command line](#the-command-line) requires precisely so that it can
  differ. A header that recorded the path it was collected under would make the
  one part of the layout that is deliberately not fixed the part the drift
  check fires on: moving the corpus, or handing the held-out split to a
  different machine, would look exactly like a misfiled truth file.

  **A partially collected file says so in the same header, and replay refuses
  it.** A hundred machine-hours will be interrupted
  ([`data-collection.md` §4](data-collection.md)), so `collect` is resumable
  and a truth file spends much of its life incomplete. The alternative to the
  flag is not a stricter rule but a quieter one — an interrupted collection
  replays as a smaller corpus, which is the shape of a coverage regression that
  never happened.
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
  not a control input.** `heuristic_latency_us` and `stage_us` are recorded
  during replay because it is the same handler code on the same snapshot, but
  nothing in the run branches on either. They are therefore the fields in the
  record that a replay does not reproduce exactly, and the ones that need a
  quiet machine to mean anything — the same two [the record
  above](#7-observability-and-the-corpus-scan) calls observations that do not
  have to be reproducible the way the rest of it does.
  `lsp_latency_us` comes from `collect` and is a property
  of the frozen truth — which is exactly what `high-level.md`'s value weighting
  wants, since it is a fact about how slow the real server was, not about
  this run.

  The distinction this does *not* blur is with the table, which is
  byte-identical across runs and holds no clock reading at all
  ([the command line](#the-command-line)): a replay's own wall clock is
  reported on the log stream, and per-query timings live in the record.
* **Replay measures the handler, not the driver**, same as `collect` — the
  paragraph above applies unchanged. Nothing in the proxy or the health model
  is under test in either mode.
* **A truth file is regenerated, never edited.** Metrics compared across two
  corpus versions are not comparable, and a partially refreshed corpus is the
  worst case: it looks like a regression.

### The table is not enough: a replay has to show its failures

The per-stratum table says *which* stratum is losing and by how much. It does
not say **what the losses look like**, and without that a tuning campaign is
being asked to form a hypothesis about a cause from a summary statistic.
"`ExplicitImport` coverage is 71%" supports no hypothesis; "of the 2,900
misses, 2,400 stopped at `import:Namespace` with zero candidates surviving
qualifier verification" supports exactly one.

This is the difference between a metric that reports work and a metric that
drives it, and it is the same distinction [section 8's abstention
reasons](resolution.md#8-strata-and-abstention-reasons) exist to draw, carried
one level further.

**Replay already computes every failing row** — it has the position, both
answers, the stratum, the abstention reason, and `stages`. It simply prints an
aggregate and discards them. So:

* **`replay --records <path>`** writes the per-query JSONL of
  [the record above](#7-observability-and-the-corpus-scan), unchanged and
  unfiltered. No new schema: a replay row and a field row are the same shape,
  which is the property [the two modes](#two-modes-collect-and-replay) already
  turn on.
* **Digesting those into something readable is the harness's job**, not
  `measure_core`'s — the same split that keeps `measure_core` ignorant of
  `state/`. `harness/measure` runs the replay, prints the table, and writes a
  failure digest beside it.

**The digest groups; it does not sample first.** A thousand individual
failures is not readable in any context window, and a random twenty of them is
an anecdote. Grouping is what turns them into findings, and the key is
available mechanically:

* **abstentions** by `(stratum_prior, reason, stages)` — coverage loss;
* **mismatches** by `(stratum_final, agreement, severity, stages)` — precision
  loss, with `match_contained` kept apart from `mismatch` since they are
  different problems: one is a ranking failure, the other a candidate-generation
  failure.

Each group carries its count, its share of that stratum, and only then a
**small seeded sample** of concrete cases — repository, file, line, the
identifier, what we returned, what the server said. Seeded so two runs of the
same campaign read the same examples, and small because the group's *size* is
the finding and the examples are only there to make it concrete.

Grouping by `stages` is what does the real work here, and it is free: two
queries that failed the same way have the same stage log by construction, so
the clusters fall out of an exact string group-by rather than out of anybody's
judgement about similarity.

Two things to be explicit about, because both are ways this could do harm.

**This is the sharpest overfitting tool in the project.** Handing a tuning loop
the individual corpus positions it is failing is precisely how it learns five
repositories instead of a language — `high-level.md` already says that is the
default outcome rather than a risk. The mitigation is the shape above rather
than a rule: the digest leads with counts and shapes, so the cheapest thing to
act on is a *pattern*, and a fix aimed at three named positions is visibly
worth less than one aimed at a group of four hundred. The tuned/held-out gap
(`loops.md` §12) remains the detector, and this makes watching it matter more,
not less.

**It is a tuning-corpus activity only.** The held-out split is shown as a
verdict and never as rows (`loops.md` §12). That holds here by construction and
not by rule: failures are digested from a `--corpus` path, and a loop is given
the tuning path and never the other one.

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
sibling splits — `training/` and `test/` — each passed by path.

**[`data-collection.md` §0](data-collection.md) owns the layout** and the
rules that go with it: how repositories are chosen, why positions are
enumerated once rather than per server, and what the manifest records. It is
not reproduced here, because a directory tree in two documents is a directory
tree that will disagree with itself. What belongs in *this* document is why
the shape is what it is.

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
                         [--format table|json] [--records <path>]
```

* **`enumerate`** parses each repository, samples positions, writes
  `positions/<repo>.jsonl`. `--limit` defaults to 20 000 and `--seed` makes the
  sample reproducible — an unseeded sample is a corpus that cannot be
  regenerated, which defeats freezing it.
* **`collect`** drives the server named in `servers.toml`, which carries its
  command and pinned version. That file is at the root of the *code*
  repository, not in the corpus:
  [`data-collection.md` §0](data-collection.md) and
  [`external-dependencies.md` §1](external-dependencies.md) both put it there,
  because which servers the corpus is collected against is a decision and
  belongs in the history beside the code that is scored against them, while
  what lives in the corpus root is the several hundred megabytes of installed
  binaries it points at. Naming a server rather than passing a command line is
  what lets the provenance header record what was actually run without
  trusting the invocation to be repeated correctly.
  Resuming is the default; `--restart` discards a partial truth file, which is
  the destructive option and therefore the explicit one.
* **`replay`** reads the frozen truth and prints the per-stratum table.
  `--format json` is what the harness consumes. `--records <path>` additionally
  dumps the per-query JSONL, which is what failure inspection is built from
  ([above](#the-table-is-not-enough-a-replay-has-to-show-its-failures)); with
  no `--records` it **writes nothing**, so the default stays a pure function of
  its inputs and `measure_core` still needs no knowledge of `state/`. Grouping
  those records into a readable digest is the harness's job, not this
  binary's.

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
`EditorRequestId`, `LanguageId`, `Offset`. That layer is real code, it is
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
| request and notification envelopes | `measure_core` only, with an integer id of its own |
| `initialize` params, `didOpen`/`didClose` params, definition params | `measure_core` only |

The last two rows are the corpus scan's, and they are why "only a small set is
ever constructed" needs a second reading rather than a correction. The shim
sits between an editor and a server and *reads* every request; `measure_core`
is a plain LSP client
([section 7](#the-corpus-scan-is-a-separate-program)) and therefore
constructs the same messages the shim reads. Each is a **separate type** from
its read twin, carrying `Serialize` and not `Deserialize` — the same split
standalone's `InitializeResult` already makes, and for the same reason: a
projection that can be written back is exactly the round trip this section
removes, so the two lists have to stay disjoint. They live in `shared::proto`
rather than in `measure_core` per [section 8.7](#87-where-it-lives); the
alternative is a second vocabulary for one protocol, in the one crate whose
job is to agree with the shim.

There is a third and much shorter list, and it is short because it is bounded
rather than because nothing has been added to it yet:

| Both | Why it travels twice |
|---|---|
| `WirePosition`, `WireRange` | [Section 8.3](#83-the-wire-position-type-is-inert) requires `WirePosition::encode`, so the type that arrives in a request is the type an answer is built from |
| `WireLocation` | arrives from the oracle ([section 6](#6-the-agreement-predicate)) and leaves as our answer |
| `PositionEncoding` | read from a child's `InitializeResult`, written by standalone's |
| `TextDocumentSyncKind` | the same, one field further in |

**"Nothing is ever round-tripped" is a claim about messages, and these are
values.** A `WirePosition` that arrives is resolved to a `Offset` and
dropped; one that leaves was built by `encode` from an offset this system
produced. No instance makes the trip, so nothing a field we did not model
could have been attached to is written back — which is the property the first
bullet is protecting. What the rule does forbid is a *projection* carrying
both derives, and the test that enforces the split keeps this list separate
from the other two so that a sixth entry is a claim someone has to make
deliberately.

### 8.3 The wire position type is inert

This is the design's payoff and the reason the change is worth making.

```rust
/// A position exactly as it appeared on the wire. `character` is in the
/// negotiated encoding, which this type does not know — so it exposes no
/// way to be used as an offset.
/// Both derives, because `encode` below means the type an answer is built
/// from is the type a request arrives in -- section 8.2's third list, and
/// the reason that list exists.
#[derive(Deserialize, Serialize)]
pub struct WirePosition { line: LineIndex, character: u32 }

impl WirePosition {
    /// The only way out. Requires naming the encoding and the document,
    /// which is exactly the information a correct conversion needs.
    pub fn resolve(self, enc: PositionEncoding, text: &Rope)
        -> Result<Offset, EncodingError>;

    /// The only way in other than deserialization, and it requires the same
    /// two things `resolve` does.
    pub fn encode(offset: Offset, enc: PositionEncoding, text: &Rope)
        -> Result<Self, EncodingError>;

    /// The row, which is the one part of a wire position that is not in the
    /// negotiated encoding: every encoding LSP offers counts *columns*.
    pub fn line(self) -> LineIndex;
}
```

`WirePosition` has private fields, and `character` — the number that is in the
negotiated encoding — has no accessor at all. A `Offset` cannot be obtained
without supplying both the encoding and the text, so the failure mode in
[section 3](#3-position-encoding) — using a UTF-16 column as a byte index — is
not something to be careful about. It does not compile.

`line` is readable, and that is not a hole in the above: a row is in no
encoding, so it is not a number that can be misread as an offset.
[Section 6](#6-the-agreement-predicate)'s predicate compares `(uri, line)` on
the child's answer as well as on ours and **reads nothing** while doing it —
and the child's row arrives only inside a `WirePosition`. Without the accessor
it could be recovered only by resolving that position against the target
document's text, which is the read section 6 may not do: divergence is
classified seconds after the answer, with the per-query cache gone and the
document possibly never opened. So the accessor is what makes section 6
implementable, and it withholds nothing section 3 protects.

The same applies outbound: `WirePosition::encode(Offset, enc, &Rope)` is
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
/// is also byte-space and which the handler gets for free. Private, and
/// built only by `at_node` -- see "Why `Location` carries a line" below.
pub struct Location {
    uri: DocumentUri,
    range: ByteRange,
    line: LineIndex,
}

/// What goes on the wire. Constructed only by the driver, at the edge.
pub struct WireLocation { uri: DocumentUri, range: WireRange }
```

The driver converts one to the other on the way out, in the same one place
that owns `PositionEncoding`. Handlers never see a `WireLocation` and cannot
construct one.

#### The conversion happens in the worker, not in `core`

Which thread does it is not a detail, because the conversion **reads the
target file**: turning a byte range into a line and a UTF-16 character needs
that line's text, and the target is frequently a file the editor never opened.
`core` may not do that — it does only O(1) state transitions and never touches
the filesystem ([`shim.md` §2](shim.md#thread-layout)) — and `writer:editor`
owns a pipe and nothing else.

So the **dispatch wrapper** does it: `driver` code, on the worker thread,
after the handler returns and before the outcome is sent back to `core`. That
is the same component that already returns a `Parsed` event without the
handler's involvement ([section 2](#text-and-tree-can-never-disagree)), and it
is still the shim doing it — the worker *is* the shim, on a pool thread.

The reason it must be there rather than anywhere later is **proximity to the
read the handler already did.** A handler cannot return a `Location` for a
file it did not read — `Location::at_node` needs a node, which needs a parse,
which needs a read — so at the moment the handler returns, every target file
was read microseconds ago by this thread: the page cache is as warm as it will
ever be, and the bytes on disk are as likely as they will ever be to still be
the bytes the offsets were taken against. One event loop later both decay; by
divergence time ([section 6](#6-the-agreement-predicate)) the document may
never have been open at all.

An earlier revision of this section said something stronger and no longer
true: that "the per-query read cache is only alive inside the query", so the
text was already in the view's cache and the conversion was nearly free.
There is no per-query read cache. `conformance-005` asked for one and was
answered **no** — a cache reached through a `Sync` `&Query` needs a primitive
this project does not have, and `CLAUDE.md` forbids adding caching before a
corpus and a benchmark say it is worth it. So the conversion **re-reads the
target file**, once per location, and the honest price is a syscall and a
UTF-8 validation that the page cache makes cheap rather than free.

**With one exception, which is not a cache.** When the target is the query's
own document, the conversion encodes against the snapshot it was already
handed rather than reading anything: `DocumentSnapshot` holds the rope, and
cloning it is three refcount bumps whatever the file's size
([section 2](#2-document-snapshots)). That is not the cache
`conformance-005` refused — nothing is stored, nothing is keyed, and nothing
outlives the query — it is the query declining to go and find text it is
holding. The case is common rather than exotic: a definition in the file the
cursor is in is the most ordinary answer this tool gives.

That re-read is not only a cost, and the consequence is load-bearing enough
to state here: for a target the editor does not have open, the handler's read
and the conversion's read are two reads of the same path, so a file edited
between them yields offsets that are stale and *still in range*. Nothing
downstream could notice —
`WirePosition::encode` refuses only offsets that are not character
boundaries, and a shifted file offers plenty that are. The carried row is the
witness, and the conversion compares it against the text it actually read
(`EncodingError::LineDisagreesWithRange`). A location whose row has moved is
refused rather than encoded, because an answer pointing confidently at the
wrong place is the failure shape [section 8.6](#86-modelling-errors-must-fail-closed)
exists to prevent.

Two consequences:

* **The dispatch result carries both forms.** `core` sends the `WireLocation`s
  to `writer:editor` and retains the byte-space `Location`s in the pending
  query, since the agreement predicate compares `(uri, line)` and the wire
  form is never needed again ([`shim.md` §7](shim.md#state)).
* **`PositionEncoding` reaches the dispatch wrapper and stops there.** It is a
  `Copy` value settled once from `InitializeResult` and handed to the wrapper
  alongside the query; it does not reach the handler, so
  [section 3](#3-position-encoding)'s rule that no encoding ever crosses the
  handler seam is unaffected.

**The conversion lives in `driver` rather than in `shared` because it has
exactly one consumer.** Not because `measure_core` is innocent of wires — it
is a JSON-RPC client, it negotiates a `PositionEncoding` from the oracle's
`InitializeResult`, and it encodes the position it *asks about* through the
same `WirePosition::encode` this section is built on
([section 8.3](#83-the-wire-position-type-is-inert)). What it never does is put an *answer* on a
wire. It asks and reads the reply; the handler's own `Location`s stay in byte
space all the way into the record, where the position field is a byte offset
([section 7](#7-observability-and-the-corpus-scan)) and the agreement
predicate compares `(uri, line)`
([section 6](#6-the-agreement-predicate)). So `Location -> WireLocation` has
one caller in the whole system, and `shared` would be a home for it shared
with nobody.

**Why `Location` carries a line.** It looks redundant with `range`, and
strictly it is. It is there because the redundancy pays for itself twice:

* It is what detects the target file having moved under the query, which the
  conversion above rests on. The row is derived from the handler's text and
  checked against the conversion's, and two numbers derived from the same
  node can only disagree if the document they describe is not the same
  document.
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
permanent one. The first two are `core`-side and O(1): each compares a number
the message carries against one we already hold. The third is neither, and its
own bullet says why — it costs a read, so the read is a worker's, and what
`core` does with the answer is a comparison against the rope, which is linear in
the document rather than constant. It is cheap in the sense that matters here:
it is on the notification path rather than the query path, so nothing waits for
it and no budget is spent on it.

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
  lang_python/      phase 2
  lang_typescript/  phase 2
  driver/           the LSP driver
  heuristic_jump/   the shim binary -- `heuristic-jump`
  measure_core/        corpus scan library -- LSP client, replay, metrics
  measure_rust/        `measure-rust` -- four lines, section 7
  measure_python/      phase 2
  measure_typescript/  phase 2
```

The four marked `phase 2` do not exist yet and **cannot** be created in phase
1a, which is why they are marked rather than merely absent (CHANGE-core-014).
`loops.md`'s decided question 10 — "The loop may never add a language" — makes
a new `crates/lang_*` outside every loop's owned paths so the gate rejects the
commit, and `state/phase.toml` names `crates/lang_rust/` rather than globbing
for exactly that reason: "naming one path grants the template without granting
the glob". Phase 1a instantiates the template once, and
[adding a language](#adding-a-language) is what the other six cost.

Crate names carry no project prefix, matching the vendored Zed crates
alongside them (`rope`, `sum_tree`) and the workspace-wide `publish = false`.
Two of the names are chosen rather than mechanical:

* **`driver`, not `core`.** A crate named `core` shadows Rust's own, and this
  document already uses "`core`" throughout for the single-threaded actor in
  [section 2](shim.md#thread-layout). Two different things called `core` in one
  system is a needless ambiguity; `driver` is what the prose calls the crate
  anyway.
* **`heuristic_jump`** for the binary crate, with a two-line `[[bin]]` rename
  so that the produced binary is `heuristic-jump`. Cargo names a binary target
  after the package verbatim and does not hyphenate it — package
  `heuristic_jump` builds `heuristic_jump` — so the rename is what makes the
  artifact match the name `deps.md` §11's `clap` command and every invocation
  in these documents already use. Zed needs no such rename only because its
  crate and its binary are both `zed`.

### The dependency graph

The shape is dictated by one rule from the outset: **`driver` must not depend
on any language crate.** Wiring happens in `heuristic_jump`.

```
              shared  <-- rope, tree-sitter, serde, serde_json, url,
             /  /  |  \      ignore, rayon, thiserror, rustc-hash, tracing
            /  /   |   \
measure_core  /  similarity  driver  <-- crossbeam-channel, lru, notify,
       |     /     |          |            rayon, rustc-hash, serde_json,
       |     /     |          |            tracing
       |    lang_* /          |
       |     /  \ /           |
       +--> measure_<lang>   heuristic_jump
```

`measure_core` and `driver` are siblings that never meet; `measure_<lang>` is
the only crate that depends on both `measure_core` and a language, and it
contains four lines.

**Three of the crates named above are chosen and not yet declared, and this is
the complete list of them.** `deps.md` §14 has each dependency arrive with its
first user, so a crate this section names and no manifest declares is the
intended state rather than a drift. But left implicit that rule forgives too
much in the other direction — a dependency that *vanishes* from a manifest is
indistinguishable from one that has not arrived yet — so the set is named here
and `crates/driver/tests/seam.rs` reads it, which turns the difference between
the two into an equality it can check:

* `rayon` in `shared` — for `ProjectView::scan`. The fan-out onto a bounded
  pool is the arrangement `resolution.md` §3 settles, and "executes on the pool
  it is handed at construction" describes that arrangement rather than the code
  as it stands: `ProjectView::new` takes no pool, and `scan` is a sequential
  loop over candidates. Parallelising it is an optimisation, and `CLAUDE.md`
  withholds those until the corpus harness shows the change is worth it and
  there is a benchmark — so the dependency arrives with the benchmark, not
  before it.
* `rayon` in `driver` — the same fan-out, seen from the side that owns the pool
  and hands it over.
* `rustc-hash` in `driver` — `deps.md` §0 places it here, and every map
  `driver` owns so far is small enough that nothing has reached for it.

Every edge, and why:

* **`shared` depends on no crate of ours in `crates/`.** The vendored text
  crates are not an exception to that and are not covered by it: `rope` is on
  the list below and `shared` depends on it, and "ours" throughout this section
  means the code in `crates/` rather than every workspace member. Keeping the
  two apart is what `vendor/` is for — `deps.md` §14's tree separates them so
  provenance and licensing stay obvious — and a `vendor/` crate is a dependency
  like any other except for who wrote it. Where the distinction has teeth is
  the other edges: `measure_core` "depends on `shared` and nothing else of
  ours" is the same word used the same way, and
  `crates/driver/tests/seam.rs::the_measurement_crates_have_the_edges_section_9_gives_them`
  reads it strictly, quantifying over `vendor/` too — not because §9's sentence
  demands it, but because the text vocabulary reaches `measure_core` through
  `shared`'s re-export, so a direct `rope` edge there would be a divergence
  worth failing on rather than a spelling of the same thing.

  The shared vocabulary: it holds
  `LanguageHandler`, `Query`, `Outcome`, `Stratum`, `Deadline`,
  `DocumentSnapshot`, `ProjectView`, and `Error` — types every other
  crate needs to talk about, and almost no behaviour. It also holds `proto`, the
  hand-written LSP wire types ([section 8](#8-protocol-types)); there is no
  `lsp-types` dependency, so that the vocabulary newtypes are what
  deserialization *produces* rather than what a conversion layer produces
  afterwards. Its own dependencies are `serde`, `serde_json`, `url`, `rope`,
  `tree-sitter`, `ignore` (for `ProjectView`'s walk), `rayon` (for
  `ProjectView::scan` — `resolution.md` §3, and the first of the three entries
  above that are chosen and not yet declared), `thiserror` (for `Error`'s
  derives),
  `rustc-hash`, and `tracing` — which is in the graph regardless, since `rope`
  and `sum_tree` depend on it and two logging facades would be silly
  (`deps.md` §9). This list is the authoritative one; §8.7 refers back to it
  rather than restating it, and
  `crates/driver/tests/seam.rs::shared_declares_only_the_dependencies_section_9_lists`
  fails on a dependency that is not on it *and* on one that is on it, is not in
  the deferred list above, and is missing from the manifest.

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
*  ** `heuristic_jump` depends on `driver` and every `lang_*`, plus `clap`,
  `tracing-subscriber`, and `shared` for the error type its `main` returns. **
  Argument parsing and log setup live here rather than in `driver`
  (`deps.md` §11, `shim.md` §13), so `driver` stays a library with no opinion
  about how it was invoked. It is also the single place where the language list
  is enumerated:

```rust
fn main() -> Result<(), shared::Error> {
    let cli = Cli::parse();
    // `Cli` is this crate's type and `driver` has no `clap` dependency, so
    // what crosses is `Config`: the same argv in `driver`'s vocabulary.
    let config = Config::new(/* mode and deadline, resolved from `cli` */);

    let registry = Registry::new(vec![
        Arc::new(lang_rust::Handler::new()),
        Arc::new(lang_python::Handler::new()),
        Arc::new(lang_typescript::Handler::new()),
    ]);
    driver::run(registry, config)
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
one line in `heuristic_jump`. **No crate other than `heuristic_jump` changes**,
which is the whole cost and the point of the graph above.

The workspace manifest changes too, and it is bookkeeping rather than design:
cargo needs each new directory in `[workspace] members`, and `lang_<x>` in
`[workspace.dependencies]` so that the two crates naming it inherit one version
(§14 of `deps.md`). `measure_<x>` needs no entry there — nothing depends on a
binary. Four lines, none of which is a decision (CHANGE-core-013).

**Phase 1a builds this as an instantiable template**, not as prose. Adding a
language is then a copy and a rename, and — more importantly — the shape every
language crate inherits is fixed once, by hand, before seven of them exist.

```
crates/lang_<x>/
  Cargo.toml          shared, similarity, tree-sitter, tree-sitter-<x>
  src/lang_<x>.rs     the Handler impl, longhand
crates/measure_<x>/
  Cargo.toml          measure_core, lang_<x>, clap, shared
  src/measure_<x>.rs  the four lines
```

Every one of those six is forced by a signature and none is a choice
(CHANGE-core-012). The tree-sitter *runtime* is there because
`LanguageHandler::grammar` returns a `tree_sitter::Language`, which is a name
the grammar crate cannot supply; `clap` and `shared` are there because the four
lines are `measure_core::run(&Handler::new(), Cli::parse())` inside a
`fn main() -> Result<(), shared::Error>`, and a trait method needs its trait in
scope. This is the same omission `CHANGE-conformance-009` found in §9's printed
`main`, which is the other place a manifest was derived from a code block by
reading it rather than compiling it.

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
its public API speaks in newtypes — `Offset`, `ByteLen`, and `ByteRange`
instead of `usize` and `Range<usize>`, and `LineIndex` / `ByteColumn` /
`Utf16Column` / `CharCount` instead of the bare integers in `Point`,
`PointUtf16`, and `TextSummary` — so the
vocabulary survives contact with the text rather than being unwrapped at the
boundary. It is a design decision with its own re-sync cost and its own
document: **`rope-modifications.md`**. Read it before touching `vendor/`.

Three things stated here because they change this section's own claims:

* `Offset`, `ByteLen`, `ByteRange`, `LineIndex`, `ByteColumn`,
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
or a no-op passthrough depending on a cfg, and both crates already depend on
`tracing`, so each import is redirected there. That is one line in `rope` and
one line in each of `sum_tree`'s two instrumented files — three in all,
recorded as such.

**`sum_tree` is patched, minimally, and the newtype work is not why.**
`sum_tree::Dimension` is generic over the summary type, so `Offset`'s impls
live in `rope` and `rope-modifications.md` costs `sum_tree` nothing — that is
the claim that matters here and it holds. What the crate does carry is the
mechanical fix-ups every vendored copy needs: the two `ztracing` redirects
above, the `#[ctor::ctor]` logging initialiser deleted along with `ctor` and
`zlog`, and `tree_map.rs` dropped as unused. Each is a whole-line or
whole-file change that still leaves a clean diff, and `vendor/README.md` lists
them.

`vendor/README.md` records, per crate, the upstream revision it was taken at,
the exact patches applied, and — for the items lifted out of `util` — where
each came from and under what license, so that a future re-sync can tell at a
glance whether upstream changed anything that matters.

**Licensing consequence, stated plainly:** vendoring `rope` makes the shipped
binary GPL-3.0-or-later, while `crates/*` stay MIT. That is a project-level
commitment following from a decision in this section, which is why it is
mentioned here at all.

**[`deps.md` §5](deps.md) owns it** — the per-crate table, why our own crates
are MIT rather than GPL, and what that preserves. Not restated, because a
licensing rule that exists in four documents is one that will be wrong in at
least one of them.

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
