# Resolution design

This covers everything behind the handler seam:

* How a reference under the cursor becomes a definition location, stage by stage.
* The shared resolution utilities in `similarity` that languages build from,
  and the line between those and per-language code.
* The confidence model, which in v1 is measured rather than enforced, and the
  commit policy that a future precision floor would be expressed in.

Out of scope: the LSP shim, document state, the actor, dispatch, and
observability plumbing. Those are `core.md`, referred to
below as "the core doc." This document takes its seam —
`LanguageHandler`, `Query`, `Outcome`, `DocumentSnapshot`, `Deadline` — as
given.

Three types the core doc names but does not define — `ProjectView`, `Stratum`,
and `AbstainReason` — are defined here, because their shape is determined by
resolution rather than by the driver.

**On precision.** `high-level.md` moved its >=97% precision floor to future work,
and the core doc's intro states what that changes for the driver. It changes
considerably more here, because the floor was this document's organising
constraint. The rule now is `high-level.md`'s:

> If the heuristic has a guess, it returns the guess. Precision is measured,
> not enforced. Two things still cause an abstention, and neither is about
> confidence: there is no candidate at all, or the latency budget ran out.

Every place the earlier version of this document used a confidence threshold,
a margin test, or a uniqueness requirement to *decline*, that mechanism now
computes a number and commits anyway. The numbers are still computed, for the
reason the core doc gives in [section 1]: a floor can only be derived from
`(stratum, confidence, agreed?)` triples collected while nothing was being
gated, and retrofitting a confidence notion into handlers written without one
means revisiting every resolution path.

Two things follow that are easy to get wrong, so they are stated up front:

* **Abstention is now rare, and that is the design.** A handler that finds
  three equally plausible candidates commits to one of them. It records the
  margin, the candidate count, and the stratum, and the corpus run then says
  what that cost. [Section 7](#7-confidence-and-the-commit-decision) is what
  makes reinstating the floor a data change.
* **Divergence reporting is load-bearing, and it is not rate limited.** The
  `shim.md` [section 9] now emits one `window/showMessage` per divergence with
  no batching window and no cooldown. That is worth repeating here, because
  resolution is the thing generating the wrong answers that reporting exists to
  confess, and because it changes what a low-precision stratum *feels* like: a
  stratum at 60% is not a number in a table, it is a notification card every
  other time the user presses the key. [Section 12](#12-predicted-coverage-and-precision)
  predicts two strata in that range.

**Coverage means handler coverage.** `high-level.md` now distinguishes *handler
coverage* — the fraction of corpus queries resolution answers at all, measured
by replay — from *delivered coverage*, the fraction of live queries where an
answer reached the user, which is mostly a fact about the health model. Only the
first is being optimized, and it is the only one this document is about. Everything predicted in
[section 12](#12-predicted-coverage-and-precision) is handler coverage.

## 1. Scope and the seam

### 1.1 What a handler is given, and what it owes

From core doc [section 1], a handler receives a `Query` and returns an
`Outcome`. Restating the obligations that resolution code must actually honour:

* **Byte offsets only.** UTF-16 never reaches here; the driver converts at the
  edge and `ByteOffset` is the proof.
* **Cooperative deadlines.** `deadline.expired()` is polled at every loop
  boundary. A handler that ignores it does not produce a late answer — the
  driver drops it — it produces wasted CPU during the exact window the whole
  no-index decision exists to protect.
* **The clock may stop work, but may not choose the answer.** New, and the
  strongest obligation in this list, and
  [section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)
  is why.
* **`Send + Sync` and re-entrant.** Per-query state lives in locals. No handler
  may memoize across queries; all caching is the driver's, reached through
  `ProjectView`.
* **All I/O through `ProjectView`.** Not a style rule. It is what lets the
  driver enforce scope, count bytes and files for the trace record, and reuse
  the parse cache.
* **Say what you did.** Each stage appends a short label to the query's
  `stages` log — the role it assigned, what it found or missed, how many
  candidates survived (core doc [section 7]). This is not logging and it is
  not optional: it is the only thing that makes a *failure* diagnosable, since
  `AbstainReason` deliberately carries no resolution vocabulary and a stratum
  total says nothing about cause. Failures are grouped by this string, so two
  queries that failed the same way must produce the same labels — which they
  do, the handler being deterministic. Nothing may branch on it.
* **Every failure is `shared::Error`, and there is somewhere to send it.** Per
  `CLAUDE.md` and core doc [section 9](core.md#the-dependency-graph), there is
  one system-wide enum, and resolution's failures are variants of it rather
  than a local error type per crate. `goto_definition` returns
  `Result<Outcome, Error>`, so `?` works and a handler never has to launder a
  failure into a decision. Abstention is emphatically not in `Error` —
  `AbstainReason` is an outcome — and failure is emphatically not in
  `AbstainReason`, which is why the `HandlerError` and `NoParse` variants an
  earlier revision had are gone (core doc [section 1]).

And what a handler may **not** assume:

* ~~That the query document is already parsed.~~ **It is.** Core doc
  [section 2] parses at dispatch, before a handler is called, so `doc.tree()`
  is a field access that cannot fail and a handler never branches on cache
  state. What a handler must still budget for is that the *dispatch* may have
  paid a full parse of a large file out of the same deadline it is now
  spending. An unparseable document never arrives at all — the dispatch
  wrapper fails it first.
* **That the file list is fresh or complete.** It is a cache, refreshed
  lazily. A miss is a miss, not a proof of absence — which is why
  `AbstainReason::NoCandidates` exists as a distinct signal
  ([section 8](#8-strata-and-abstention-reasons)).
* **That it has unlimited time, or that it should manage its own.** The
  search runs to completion and the deadline aborts it
  ([section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)),
  so a handler neither sizes its search nor rations it. What it owes is
  polling `expired()` often enough that the abort is prompt — the deadline
  differs by mode (750ms proxying, 2000ms standalone, `shim.md`
  [section 14.6]) and a handler never learns which.
* **That it is called once per user gesture.** Speculative editor requests, a
  user pressing again, and `measure replay` all produce repeats at the same
  position. Resolution must be deterministic — see below.
* **That it is the only query running.** Fan-out draws from the same bounded
  pool as every other in-flight query.
* **That it will be asked about a document at all.** A document the driver has
  marked untrusted (core doc [section 8.6]) never reaches a handler; the
  driver abstains on its behalf. Handlers therefore do not validate document
  state, and should not try.

### 1.2 The pipeline is a shape, not a type

`high-level.md` is explicit: "Each language implements its own resolution logic.
Dispatch is simple... Sharing and dedupe between languages happens through
shared utilities, not through a common framework or config format that each
language has to be expressed in."

Taken seriously, that rules out the obvious design. There is no
`Pipeline` type with registered stages, no `ResolutionConfig`, no trait
with eight hooks that a shared crate calls in a fixed order. Each `lang_*`
crate writes its `goto_definition` out longhand as a sequence of `if let`
returns, calling shared utilities for the parts that are genuinely mechanical.

The stages in [section 2](#2-the-resolution-pipeline) are therefore a
description of the shape that implementations tend to take, not an interface
they conform to.

This costs perhaps a hundred lines of obvious sequencing per language. What it
buys is that a language can reorder stages, skip one, fuse two, or add a stage
nothing else has, without anyone extending a framework. The failure mode being
avoided is specific and familiar: the second language needs a variation, so the
framework grows a boolean; the fifth language has accumulated a config struct
that is really a small programming language, badly specified, and every
language's real behaviour is now spread across a query file, a config literal,
and the framework's interpretation of both. The old implementation in
`../heuristic_jump_old` went down exactly this road — `LanguageConfig` with
`import_path_strip_regex`, `ignored_import_segments`, and an
`ImportsConfig` of eight optional capture indices
(`src/languages.rs:8-26`) — and it is the limitation `high-level.md`'s "I no longer
want to stick with that limitation" refers to.

The rule of three applies to going the other way: when a second language wants
something the first already wrote, it copies. When a third wants it, it moves
to `similarity`. Deduplication is allowed to lag, because a shared utility
extracted from two examples is usually the wrong shape.

### 1.3 The search is exhaustive, and the clock may only abort it

New in this revision, and everything below is written against it.

**A search reads everything it is entitled to read.** There is no per-query
byte budget, no file cap, and no parse cap. Stage 5 scans every project file
with a matching extension; a stage that has candidates left has not finished.
The only thing that can stop a search early is the deadline, and when it does
the query **abstains entirely** rather than committing from a partial view.

One statement follows, and the whole document is written against it:

> **The committed answer is a function of the snapshot, the position, and
> the project state alone.** No branch that affects the result may read a
> clock. The clock decides *whether* there is an answer, never *which*
> answer.

An earlier revision bought this property with a reproducible byte budget,
carefully calibrated so that replay's early stops matched the shim's. Removing
the budget gets the same property for free and more strongly: **an exhaustive
search has nothing to vary.** There is no stopping rule to reproduce, so
determinism is structural rather than calibrated, and the budget↔deadline
mapping that used to be load-bearing does not have to exist.

The consequences run through the whole document:

* **Replay and live measure different things, and that is now explicit.**
  `measure replay` has no clock, so it always completes and always answers.
  The shim, on a large repository, may not. Replay therefore reports handler
  coverage as an **upper bound** on what the shim delivers, and the gap is a
  *latency* fact rather than a resolution one. That is the right split: phase
  2a optimises resolution against replay, phase 3 optimises cost against the
  wall clock, and neither is measuring the other's problem
  (`loops.md` §10).
* **A deadline expiry is a whole-query abstention.** Not a partial commit,
  and not a marked row to be filtered later — `AbstainReason::Deadline`, and
  nothing is returned. A handler interrupted at an arbitrary point has an
  arbitrary candidate set, so committing from it would be the one answer in
  the system whose quality nothing bounds.
* **Iteration order must be stable.** Fan-out over candidate files is
  parallel, but the *reduction* may not depend on completion order — results
  are collected and then sorted by the total order in
  [section 6.4](#64-the-output-is-a-ranked-list), never reduced as they
  arrive. With the budget gone this is the *only* remaining way to break
  replay determinism, and it passes every test on an idle machine.
* **Nothing may vary with pool occupancy or remaining time.** A handler must
  not sample how much of the deadline is left and search harder when the
  machine is quiet. Reading the clock to decide whether to stop is allowed;
  reading it to decide what to look at is not.

The cost, stated plainly: the deadline is now the only bound on a runaway
query, so on a large enough repository the shim's coverage becomes a
wall-clock outcome. That is a real regression against the budgeted design and
it is accepted deliberately — it converts an unmeasurable calibration problem
into a measurable latency problem, which is the one phase 3 exists to solve.
The abstention rate attributable to the deadline is therefore a number worth
watching from the first corpus run.

The payoff is what `loops.md` is built on: because replay is
deterministic, a metric that moves has a cause in the diff, so per-stratum
numbers can be ratcheted in a baseline file rather than treated as noisy
observations. [Section 11](#11-testing) makes the property a test, and that
document's §14 depends on it.

### 1.4 The correct answer depends on which server

Also new, and it changes what "correct" means in every other section of this
document.

Core doc [section 7] establishes that two language servers for the same
language genuinely disagree, and not because one is wrong: go-to-definition on
a re-exported name has two defensible answers, and so do declaration versus
definition, trait method versus impl method, and whether a `use` resolves to
the import or through it. The shim stands in for **one specific server** and
reports divergence against that server, so:

> The answer that counts as correct is that server's answer. A handler that
> split the difference would be wrong in both deployments rather than right in
> either.

`Query` therefore gains `server: &ServerProfile`, and `truth.jsonl` is
collected per `(language, server)` — `truth/rust-analyzer/<repo>.jsonl`,
`truth/pyright/<repo>.jsonl`, `truth/pylsp/<repo>.jsonl`. Nothing is averaged
across servers.

Four consequences here:

* **`Confidence` is per-oracle.** [Section 7.1](#71-confidence-is-a-calibrated-estimate-not-a-vibe)
  defines it as the estimated probability of matching the proper LSP, and that
  sentence now has a free variable. The same answer to the same query has two
  different correct confidences under pyright and pylsp. A future calibration
  table is therefore indexed by `(language, server, stratum, bucket)`, which
  divides the data per cell — a real cost, and an argument for fewer buckets
  ([open question 10](#open-questions)).
* **The profile is not a config format, and the pressure to make it one is
  strong.** Core doc [section 1] says so, citing
  [section 1.2](#12-the-pipeline-is-a-shape-not-a-type) and
  [section 9](#9-what-is-shared-and-what-is-not) of this document by name. The
  same rule applies: `ServerProfile` starts empty (just a `ServerId`), and a
  field appears only when the corpus shows a systematic divergence a field
  would fix. Nothing is predicted. A handler reads a *behaviour* field; it
  never branches on identity, because `if server.id == PYRIGHT` scattered
  through resolution logic is the rejected config format wearing another hat.
* **The corpus splits into two surfaces, and that is a gift.** Positions where
  every server agrees are the shared logic's responsibility and are the bulk of
  the corpus. Positions where servers differ are the profile's. They are
  separately optimisable: a profile change cannot move another server's
  numbers, and shared logic is evaluated where the servers do not disagree
  about the answer. This is a cleaner decomposition than anything this document
  had before, and it costs nothing to adopt.
* **It hands [open question 9](#open-questions) its data for free.** That
  question asks whether re-export chains should be followed, and says it needs
  corpus evidence first. The set of positions where servers disagree *is*
  largely a map of where re-export and alias chains matter — core doc
  [section 7] makes exactly this observation. So the evidence arrives as a
  by-product of measurement rather than needing its own experiment.

The open end is standalone, which has no server to stand in for and therefore
no oracle at all. `ServerId` is `None` there, and `open-questions.md` question
14 asks whether it should imitate a neutral profile or the most widely deployed
one. That is not a resolution decision, but resolution is where it lands: a
handler must behave *somehow* with an empty profile, and "whatever the shared
logic does" is the current answer by default rather than by choice.

## 2. The resolution pipeline

Stated as `high-level.md`'s steps, made precise. Stage numbers are for reference in
this document; nothing in the code is numbered.

| # | Stage | Owner | I/O |
|---|---|---|---|
| 0 | Reference extraction | language | none |
| 1 | Local scope resolution | language | none |
| 2 | Same-file module resolution | language | none |
| 3 | Import-directed resolution | language rules, shared search | reads |
| 4 | Wildcard expansion | language rules, shared search | reads |
| 5 | Whole-project search | shared | scan + reads |
| 6 | Candidate verification | language | parses |
| 7 | Ranking | shared | none |
| 8 | Commit or abstain | shared | none |

Stages 0–2 are pure syntax on a document already in hand, and should be
sub-millisecond. Everything from 3 on touches the filesystem and is where the
latency tail lives — which is the same split `high-level.md` asks for in its
per-stratum latency reporting.

### Stage 0: reference extraction

From `(doc, position)`, produce:

```rust
pub struct Reference {
    pub name: IdentifierName,
    /// Qualifier segments to the left: `a::b::Thing` at `Thing` gives [a, b].
    pub qualifier: Qualifier,
    pub role: ReferenceRole,
    pub range: ByteRange,
}

pub enum ReferenceRole {
    Value,          // bare identifier in expression position
    Call,           // callee of a call expression
    Type,           // type position
    TypeParameter,
    Field,          // `x.field`, no call
    Method,         // `x.method()`
    Macro,          // `foo!`, `#[derive(Foo)]`
    Module,         // path segment that is itself the target
    Attribute,
    Lifetime,
}
```

Failure here is cheap and common: the cursor on a keyword, a string, or
whitespace yields `AbstainReason::NotAnIdentifier` immediately.

`role` is load-bearing in three places. It gates which later stages are worth
running, it constrains which definition kinds can match in stage 6, and it is
the primary input to the a-priori stratum. `Method` and `Field` in particular
are `high-level.md`'s "requires type inference" class; see
[section 10.5](#105-methods-fields-and-the-type-inference-class) for what can
still be done with them.

This stage needs a tree, and it has one: the parse happened at dispatch (core
doc [section 2]), so `doc.tree()` is free here and the cold-cache cost was
already paid out of the deadline before the handler started. An earlier
revision asked handlers to call `tree()` even on paths that abstain
immediately, so that the driver's cache was warmed for the next query; eager
parsing makes that automatic and the rule is gone.

### Stage 1: local scope resolution

Walk up the tree from the reference, and at each ancestor ask the language
whether that node introduces a binding of `name` visible at the reference.

This is the one stage that is **not heuristic**, and it should be written that
way. It is also the largest stratum by raw count — `high-level.md` notes a complete
identifier scan is dominated by locals — so it is where most of the committed
volume comes from and where a precision defect would do the most damage.

Two rules keep it honest:

* **Unknown node kinds fall through, never claim.** A grammar update that adds
  a binding form the walk does not recognise must cost coverage, not
  precision. Concretely: the walk matches on node kinds it knows, and the
  default arm continues upward rather than concluding "no binding here."
* **Shadowing is resolved by proximity, not by first match.** Innermost
  binding wins, and a language that has order-sensitive shadowing within a
  scope (`let x = x + 1`) must compare positions, not just nesting.

On success this commits at the highest confidence the handler can report, as
`Stratum::LocalBinding`. It still routes through
[section 7](#7-confidence-and-the-commit-decision) rather than constructing an
`Outcome::Committed` itself — there is no bypass, even for the stage that most
deserves one. In v1 that funnel changes nothing about the answer; it exists so
that the stage which will always be exempt from a future floor is exempt
*because the table says so*, not because its code path never learned to ask.

### Stage 2: same-file module resolution

Look for a definition of `name` among the document's own module-level
declarations — items at the top level, and associated items of enclosing
`impl`/`class` blocks.

Uses the same declaration extraction as stage 6, applied to a tree already in
hand, so it costs no I/O. A unique same-file definition of a compatible kind is
a strong signal: the old implementation's retrieval scoring gave same-file
declarations an order of magnitude more weight than any other single signal
(`edit_prediction_context/src/declaration_scoring.rs:81-82`), divided by the
count of same-file candidates — which is the uniqueness idea this design leans
on throughout.

### Stage 3: import-directed resolution

Consult the file's import table for the head of the qualifier, or for the bare
name when unqualified. This is the stage `high-level.md` cares most about, because
it is where an exact answer is available without a project-wide search.

An import resolves to a **module target**:

```rust
pub enum ModuleTarget {
    /// The import names a file directly, e.g. a relative JS/TS import.
    File(ProjectPath),
    /// The import names a module path that the language can map to files
    /// deterministically, e.g. Rust's declared module tree.
    Declared(Vec<ProjectPath>),
    /// A namespace with no deterministic file mapping; candidates are
    /// ranked by path similarity.
    Namespace(Namespace),
    /// Resolves outside the workspace.
    External(Namespace),
}
```

The four arms are not cosmetic; they are four different precision tiers, and
collapsing them is how a heuristic becomes unmeasurable. `File` and `Declared`
justify a top-confidence commit on a single verified candidate with no global
uniqueness check at all. `Namespace` does not — a path-similarity ranking can
be confidently wrong — so it reports a much lower confidence and leans on the
margin ([section 6.4](#64-the-output-is-a-ranked-list)) to say how much
lower. Under the floor those two were "commit" and "must clear the margin
test"; now they are the same action with different recorded numbers, which is
precisely the data a threshold would later be set from.

`External` is the exception that is still a real abstention, because it is an
*emptiness* case rather than a confidence case: the target is outside the
workspace, so there is no candidate to guess at, and `high-level.md`'s rule
therefore applies unchanged. It is kept distinct from `NoCandidates` so the
metric blames the scope decision rather than the search
([section 8](#8-strata-and-abstention-reasons)).

Import extraction itself is per-language, and this is a deliberate departure
from the old implementation, which drove it from an `imports.scm` query with
seven optional captures and a tree-reassembly pass to recover the nesting the
query flattened (`src/imports.rs:282-330`). The data model it produced —
`Import::{Direct, Alias}` over `Module::{SourceExact, SourceFuzzy, Namespace}`
(`src/imports.rs:54-70`) — was right, and `ModuleTarget` above is essentially
it with `Declared` split out and `External` made explicit. What was wrong was
expressing the extraction as a query plus a repair pass instead of as a
function over the syntax tree.

### Stage 4: wildcard expansion

Same as stage 3, over the modules brought in by glob imports. Produces more
candidate files and carries a lower prior, exactly as the old scoring did
(`declaration_scoring.rs:85-90`, wildcard path matches weighted at a third of
exact ones).

The important structural point: wildcards multiply candidates, so this is the
first stage where the answer is routinely ambiguous, and therefore the first
stage whose recorded margin will be small often enough to be worth looking at.

### Stage 5: whole-project search

The fallback. Scan every file with a matching extension for the name, verify
the hits, rank, and commit the winner. This is where the latency budget is
spent and where `high-level.md`'s "p99 <= 400ms" lives. See
[section 4](#4-the-search-primitive).

Under the floor this stage was expected to abstain most of the time. It no
longer does, which makes it both the largest source of new coverage and the
largest source of wrong answers — and the one whose per-stratum row is most
worth reading first.

### Stages 6–8

Verification, ranking, and the commit decision are shared and get their own
sections ([6](#6-candidate-verification), [7](#7-confidence-and-the-commit-decision)).

## 3. `ProjectView`

The handler's entire view of the world outside its own document.

**It is a concrete struct in `shared`, not a trait.** Core doc [section 1] now
settles this, reversing an earlier arrangement that both this document and that
one assumed: the trait lived in `shared` and the implementation in `driver`,
because the file list cache and the scope rules are the driver's. Two things
overturned it, and the second is the one that decides it.

* `measure_core` needs a `ProjectView` too, and it must be *the same one*.
  Scope rules decide which candidates a search can find at all, so a second
  implementation on the measurement path means the corpus scores a tool that
  is not the one that ships — the argument core doc [section 7] already makes
  for snapshot construction, with more force here.
* Under `phases.md` the measurement binaries exist **a whole
  phase before `driver` does**. A `ProjectView` that lives in `driver` is not
  available when the first thing that needs it is built.

There is also no second implementation in prospect: an in-memory test double
is ruled out by the no-unit-tests rule (fixtures are real directories),
standalone and proxy share scope rules exactly, and multi-root ordering is
configuration rather than polymorphism. A trait with one impl on a
per-file-read path is a vtable bought with a guess.

The practical consequence for this document is small but worth stating: the
methods below are inherent, calls are static, and `ignore` becomes a `shared`
dependency. The obligations are unchanged.

```rust
impl ProjectView {
    pub fn roots(&self) -> &[ProjectRoot];

    /// The root containing a document, for scoping searches.
    pub fn root_of(&self, uri: &DocumentUri) -> Option<&ProjectRoot>;

    /// Resolve a relative path against the file list. Returns None if the
    /// path is not a tracked project file — which is how scope is enforced.
    pub fn lookup(&self, root: &ProjectRoot, rel: &RelPath) -> Option<ProjectPath>;

    /// Files with any of the given extensions, in the order given by
    /// `origin`. Carries the generation so a caller can report staleness.
    pub fn candidates(
        &self,
        exts: &[FileExtension],
        origin: &SearchOrigin,
    ) -> CandidateFiles<'_>;

    /// Text of a project file. Open documents return editor state.
    pub fn read(&self, path: &ProjectPath) -> Result<FileText, Error>;

    /// Parsed tree, from the parse LRU when possible.
    pub fn parse(&self, path: &ProjectPath, text: &FileText) -> Option<Tree>;

    /// Literal search over every candidate file, executed on the worker
    /// pool. Exhaustive: see section 1.3.
    ///
    /// `Result`, because a scan reads and a read fails when the deadline has
    /// expired — and section 4 has no partial outcome for the expiry to be
    /// reported as.
    pub fn scan(&self, req: &ScanRequest) -> Result<ScanOutcome, Error>;
}
```

### `ProjectPath` is unforgeable

```rust
/// A file known to be inside a workspace root and not gitignored.
/// Constructible only by `ProjectView`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProjectPath(Arc<ProjectPathInner>);   // private field, private ctor
```

A handler cannot build one from a string. Every path it holds came from
`candidates` or `lookup`, both of which consult the `ignore`-crate file list
from core doc [section 4]. So "search scope is the project's own tracked
source" is enforced by the type system rather than by every language author
remembering it, and `high-level.md`'s exclusions — gitignored files, external
dependency sources — hold by construction.

This matters more than it looks. A Rust handler resolving `serde::Deserialize`
knows perfectly well where `~/.cargo/registry` is, and the one-line change to
peek at it would work, pass review, and quietly move the tool into a scope
whose latency the design has never accounted for. `lookup` returning `None`
for that path is what makes `ExternalDependency` a measured abstention instead
of an accident.

### `read` resolves open documents

`shim.md` [section 5] argues at length that `didChange` must be tracked, because
the tool's value window coincides with the user having just typed something.
That argument only pays off if it reaches the search path: a definition added
thirty seconds ago is in the editor's buffer, not on disk, and a handler that
reads the disk copy of an open file gets a confidently wrong answer at the
moment the tool most needs to be right.

So `read` checks the open-document map first and returns editor state when the
file is open. This is the only place the two halves of core doc section 5 —
authoritative open documents, on-demand disk reads — actually meet.

```rust
pub enum FileText {
    Disk(Arc<str>),
    Open(Rope),
}
```

Both expose `chunks()`, `slice(ByteRange)`, and `len() -> ByteLen`. Handlers
work in chunks where they can, so a large open file is not flattened to a
`String` to check one line.

### Reads are cached per query, and counted but not capped

The view is instantiated per query. Within it:

* Each file is read at most once, so stages 3, 5, and 6 touching the same file
  cost one read.
* Bytes read accumulate in a counter typed `ByteLen` — the `shared` vocabulary
  type, not a parallel one, per `rope-modifications.md` §4. It **is not a
  budget**: nothing compares it against a limit, and a search never stops
  because of it — see
  [section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it).
  It exists so that `bytes_scanned` and `files_parsed` reach the trace record,
  where they are what attributes a latency regression to a diff.
* **The view exposes no remaining-budget number**, and deliberately: a handler
  that could read one would be able to make the answer depend on how much
  earlier stages happened to consume, which is a coupling between stages that
  nothing else in the design has.
* Every read still checks the deadline first and fails with the deadline
  variant of `shared::Error` rather than starting I/O that cannot be used.
  That is an abort, not a limit — the query abstains with
  `AbstainReason::Deadline` and returns nothing.

### Why `scan` lives here rather than in the handler

The split: **the handler builds the pattern and interprets the matches;
`ProjectView` executes the search.** Execution belongs to the view because it
owns the two things a search must respect — the bounded pool from
`shim.md` [section 10](shim.md#10-parallel-dispatch-and-resource-limits) and
the deadline — and because a handler that spawned its own
threads would take back exactly the CPU headroom the no-index decision exists
to preserve.

The earlier version of this section said execution "has to be the driver's."
That is no longer the right word now that `ProjectView` is a `shared` struct:
the pool is handed to it at construction, so `measure_core` builds one the
same way `driver` does, which is precisely the property that forced the
move. The rule for handlers is unchanged, and is the part that matters: no
handler creates threads, opens files, or walks directories itself.

## 4. The search primitive

### Literal first, parse second

Every search starts from an exact identifier, which means the cheapest possible
prefilter is available: a literal, word-boundary byte scan. Only files with a
hit are read further; only lines with a hit are considered; only surviving
candidates are parsed.

This is the main departure from dumb-jump. dumb-jump recognises definitions
with per-language regexes — `fn NAME`, `struct NAME`, `impl.*NAME`. This design
uses the literal scan to *find* and tree-sitter to *decide*. The reasoning is
that both approaches need the literal prefilter anyway, and once a parse is on
the table the regex is strictly worse at the deciding step: it matches inside
block comments, inside string literals, inside `use` statements, and it cannot
tell an `impl` header from a definition.

### Lexical rules may reject, never accept

The parse is not free — call it a millisecond per ten thousand lines — and a
whole-project search for a name like `new`, `get`, or `id` hits thousands of
lines. Parsing every file with a hit blows the deadline on exactly the
queries that are already hardest — and with the scan exhaustive
([section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it))
every file with a hit does reach that stage.

So a language may supply cheap lexical hints that prune the set before parsing:

```rust
pub struct DefinitionHints {
    /// Applied to the matched line. A line matching none of these cannot be
    /// a definition in this language.
    pub line_patterns: Vec<Regex>,
}
```

With one absolute rule: **a lexical hint may only shrink the candidate set,
never confirm a candidate.** Acceptance always requires the parse.

This rule survives the dropped floor unchanged, and it is worth saying why,
since the obvious reading is that a permissive posture should permit this too.
The difference is that a regex allowed to accept does not produce a *guess* —
it produces a category of answer that is not wrong-but-plausible but simply
wrong: a jump into a block comment, or into a `use` line, or into a string
literal that happens to contain the word `fn`. `high-level.md`'s permissive
rule is "if the heuristic has a guess, return the guess", and a hit inside a
comment is
not a guess about where the definition is. It is also the cheapest possible
thing to exclude, since the parse that excludes it is already on the critical
path for the surviving candidates.

A hint that is too narrow costs coverage on the definitions it wrongly
excludes, which is recoverable and shows up in the per-stratum table.

### Candidate ordering

`SearchOrigin` orders the file list into tiers, cheapest and most likely first:

1. The exact file an import resolved to.
2. Files in the same directory as the requesting document.
3. Other files under the same root, ordered by path proximity.
4. Other workspace roots, requesting root's first.

Tier 4 is `open-questions.md` question 8, on multi-root ordering. The
default above is "requesting folder first," and nothing here forecloses the
pagerank-style
ranking it suggests; see [section 6.3](#63-what-ranking-deliberately-does-not-use).

### The scan is exhaustive, and uniqueness is therefore earned

A scan reads every candidate file. It does not stop on a byte budget, a file
count, or a parse count, and there is no partial-scan outcome to report:

```rust
pub struct ScanOutcome {
    pub hits: Vec<FileHits>,
    pub files_scanned: FileCount,
    pub bytes_scanned: ByteLen,
}
```

The two counters are for the trace record, not for control flow
([section 3](#reads-are-cached-per-query-and-counted-but-not-capped)).

This settles a hazard the earlier revision spent most of this section
managing. **A partial scan cannot distinguish "the only definition of this
name in the project" from "the first of eleven"** — and global uniqueness is
the main confidence signal for stages 4 and 5, so a clipped search that
committed was reporting a confidence it had not earned, preferentially on
large repositories, which is exactly where the corpus is least able to catch
it. An exhaustive scan earns the signal instead of approximating it: when it
says a name is unique, it is.

The successive positions this document held are worth recording, because the
question keeps coming back in a new costume. First: a truncated search may
commit only if its confidence does not depend on uniqueness — withdrawn,
because that is a confidence-based abstention and `high-level.md` allows only
two. Then: a truncated search commits its best candidate and marks the row —
withdrawn here, because it made the *measurement* conditional on a flag that
every consumer then had to filter on, and made replay agree with the shim only
by calibration. Neither problem exists once nothing is clipped.

What this costs is real and is paid in latency rather than in correctness: the
search that used to stop now runs to completion, so on a large repository the
deadline is what ends it, and the query abstains outright
([section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)).
That trades a *silently overclaimed* answer for an *absent* one, which is the
direction `high-level.md` prices correctly: an abstention costs the user a
wait they were already having.

Two consequences elsewhere:

* `AbstainReason::TruncatedEmpty` is gone. An empty exhaustive scan is
  `NoCandidates` and means what it says — the name is not in the project —
  which is also what makes it a trustworthy file-list rescan trigger
  ([section 8](#8-strata-and-abstention-reasons)).
* **`DefinitionHints` matters more, not less.** With every hit file now
  reaching the parse stage, the lexical prefilter is the only thing keeping a
  whole-project search for `new` from parsing thousands of files.
  [Open question 11](#open-questions) was "does it earn its complexity"; the
  answer is now closer to yes than it was.

## 5. Reuse from the prior implementation

`high-level.md` flags the old version's text similarity as worth reusing. It is,
but for a narrower purpose than it served there, and the narrowing is the
point.

**Kept: the occurrence-hash machinery.** `Occurrences`, `SmallOccurrences`,
`HashFrom`, `IdentifierParts`, and the Jaccard / weighted-overlap metrics
(`src/text_similarity/`). This is well-factored, generic over the occurrence
source, and its `u32`-hash tradeoff is argued rather than assumed
(`occurrences.rs:41-50`). It ports to `similarity` with the buffer types
swapped out.

**Kept: path/namespace similarity.** Scoring a namespace like `a::b::c` against
a candidate file's path by hashing both into identifier parts and taking
Jaccard (`declaration_scoring.rs:440-469`). This is exactly what
`ModuleTarget::Namespace` needs in stage 3, and it is the piece that is
genuinely hard to rewrite well.

**Kept as a model and not as code: the retrieval score's structure**, not its
constants. It is the one thing on this list that did *not* come across into
`crates/similarity`, because it ranks a candidate set and the candidate set
lives in the handler.
`retrieval_score` (`declaration_scoring.rs:80-97`) is a tiered
`if`-chain — same file, then exact path import, then wildcard path import,
then namespace similarity, then a uniqueness fallback of
`1 / declaration_count`. That tiering is the right model and
[section 6](#6-candidate-verification) reproduces it. Its weights were tuned
for a different objective and are discarded.

**Ported, but not a signal the pipeline uses: body text similarity.** The
crate carries `CodeParts`, `NGram` and `SlidingWindow`; nothing in
[section 2](#2-the-resolution-pipeline)'s stages calls them. An earlier
revision left them behind entirely. Porting the whole toolkit and not wiring
it up is the better trade, because *having the machinery* and *using the
signal* are different decisions, and only the second one is argued below —
[open question 7](#open-questions) is what would revisit it, and it is cheap
to answer with the code present and expensive without it.

The old scoring's largest feature group
compared the *text around the cursor* against a candidate's body and signature
— four Jaccard variants and four weighted-overlap variants
(`declaration_scoring.rs:423-438`). That is the right signal for edit
prediction, whose job is retrieving *related* code to put in a prompt. It is
the wrong signal here, and actively dangerous: it prefers the definition that
most resembles the call site's surroundings, which among several same-named
candidates is a plausible-wrong-answer generator.

Dropping the floor weakens this argument without overturning it. Under a floor
the case was airtight — a feature producing confident wrong answers is worse
than no feature. Under a permissive posture something has to break the tie
anyway, and "most similar body" is not obviously worse than the alternatives.
It is still excluded from v1, on the narrower ground that it is the most
expensive signal on the list (it needs the candidate's body text, not just its
signature line) and the one with the least defensible relationship to what
go-to-definition means. If the corpus shows ties are common and the existing
signals separate them badly, this is the first thing to reconsider —
[open question 7](#open-questions).

**Dropped: the query-driven configuration layer.** `LanguageConfig`,
`ImportsConfig`, the `.scm` files, and `populate_capture_indices`
(`src/languages.rs`). Superseded by [section 1.2](#12-the-pipeline-is-a-shape-not-a-type).
The `.scm` files remain useful as documentation of what facts each language
needs to expose, and should be read that way when writing each handler.

**Dropped: the syntax index.** `edit_prediction_context/src/syntax_index.rs` is
a maintained project-wide declaration index — precisely what `high-level.md` rules
out.

## 6. Candidate verification

A literal hit is a byte offset. Turning it into something rankable is per
language, and it is `high-level.md`'s step about checking "if the candidate parses
(not in a block comment, etc)."

```rust
pub struct Candidate {
    pub path: ProjectPath,
    pub name_range: ByteRange,
    pub item_range: ByteRange,
    pub kind: DefinitionKind,
    /// Enclosing module / class / impl chain, outermost first.
    pub container: ContainerPath,
    pub visibility: Visibility,
}
```

The language implements `classify(tree, text, hit) -> Option<Candidate>`,
returning `None` for anything that is not a definition site: comments, string
literals, import statements, other references, macro invocation arguments it
cannot see into.

**`Candidate` carries the node it was classified from, and that is how the
line gets filled in.** Core doc [section 1] has `Location` carrying
`line: LineIndex` alongside `range`, constructible only via
`Location::at_node`, on the reasoning that a handler gets the row for free from
a tree-sitter node it has already verified and the driver would otherwise
build a whole-file line index later — at divergence-classification time,
seconds after the answer, when the read cache is gone and the file may never
have been open. That reasoning lands exactly here: `classify` is holding the
node, so the winning candidate converts to a `Location` with no extra work and
the two fields cannot disagree. A handler that reconstructs a line by counting
newlines has misunderstood the type.

### 6.1 Compatibility is scored, not enforced

Three predicates relate a `Candidate` to the `Reference`:

* **Role compatibility.** Can a definition of this kind be referenced this way?
  Language-supplied, because the answer is not obvious: in Rust a `Call`
  reference legitimately resolves to a tuple-struct definition, and a `Type`
  reference legitimately resolves to a type alias, a trait, or a generic
  parameter.
* **Qualifier compatibility.** Does the candidate's `container` end with the
  reference's qualifier? This is `high-level.md`'s `Some.Object.Nested` check.
* **Visibility.** Could this item be named from the requesting file at all?

The temptation is to hard-filter on all three. This design scores them instead,
for one specific reason: **re-exports.** A Rust `pub use` or a TypeScript
barrel file means the container path a reference implies is not the container
path the definition has, and a hard qualifier filter would eliminate the
correct answer.

Core doc [section 7] adds a reason this is even more true than it looked: the
re-export case is one where *servers themselves disagree*, some answering the
re-export site and some the original definition, with both defensible. So
"the correct answer" here is not a fact about the language being approximated
imperfectly — it is a per-oracle choice
([section 1.4](#14-the-correct-answer-depends-on-which-server)).
A hard filter would not merely lose coverage; it would encode one server's
convention as if it were the language's. Scoring keeps both candidates alive
so a profile field can later prefer one, which is the shape that
generalises. Eliminating it costs coverage rather than precision, so it
would be "safe" — but it would delete a large and *systematic* slice of the
explicitly-imported stratum, which is the stratum this tool is most likely to
be good at.

So a violated predicate demotes a candidate rather than removing it, and a
demoted candidate can still win if nothing else scores higher. Only `classify`
returning `None` removes a candidate outright.

Under the floor the margin test was what kept a demotion from becoming a guess.
It no longer does, so the honest statement of v1's behaviour is that a
qualifier mismatch is a *ranking penalty and nothing more* — a lone candidate
that violates all three predicates will still be committed. That is the
`high-level.md`'s rule applied consistently, and it is deliberately not softened here,
but it is the place where the permissive posture is furthest from what the
earlier design intended. The penalty magnitudes are therefore worth calibrating
early: they are now the only thing standing between a re-export and a
completely unrelated same-named item.

### 6.2 Ranking signals

Ordered by weight, and deliberately few:

| Signal | Source |
|---|---|
| Same file | stage 2 |
| Import tier: `File` / `Declared` > `Namespace` > wildcard > none | stage 3/4 |
| Path–namespace similarity | ported `IdentifierParts` Jaccard |
| Container path match depth | stage 6 |
| Role/kind compatibility | stage 6 |
| Visibility | stage 6 |
| Directory proximity to the requesting file | file list |
| Competing candidate count | search |

Kept small on purpose. Every signal is a tuning knob, every knob is a way to
overfit to the corpus, and `high-level.md` already flags that Claude sessions
iterating against a fixed corpus make overfitting the default outcome. Eight
signals with held-out validation is defensible; thirty is not.

### 6.3 What ranking deliberately does not use

* **Body text similarity**, per [section 5](#5-reuse-from-the-prior-implementation).
* **Module importance / pagerank.** `open-questions.md` question 1. This got
  substantially more urgent when the floor dropped: under the floor, ambiguous
  cases abstained, so a missing tiebreak signal cost coverage on a stratum
  that was going to be low-coverage anyway. Now ambiguous cases *commit*, so a
  missing tiebreak signal costs precision on every one of them. It is still
  not v1 — it is `open-questions.md` question 1 and the corpus has not yet said
  how much of the answer space is decided by ties — but it is now the ranking
  change with the largest expected effect, rather than a nicety. When it is
  added, the natural implementation reuses the literal scan — inbound
  reference counts per module are a by-product of searching — rather than
  introducing an index.
* **Recency of edit.** Tempting, since the tool's value window is right after
  the user typed something. Rejected because it is a strong prior about *which
  file*, derived from behaviour rather than from the language, and a wrong
  strong prior is the expensive kind of wrong.

### 6.4 The output is a ranked list

```rust
pub struct Ranked {
    /// Best first. Never empty; a stage with no candidates abstains.
    pub candidates: Vec<Candidate>,
    pub margin: Margin,          // normalized gap from first to second
    pub considered: CandidateCount,
    pub truncated_list: bool,    // more candidates existed than the cap allows
}
```

**The answer is the list, not the winner.** This reverses what the previous
revision specified, and the reversal is `high-level.md`'s: indistinguishable
candidates are all returned, ranked, and the editor renders a picker.
`Outcome::Committed` already carried `Vec<Location>` for exactly this, so the
seam does not change — only what this document puts in it.

The argument the previous revision gave against it does not survive the
decision, and it is worth saying why rather than just deleting it. It claimed
a picker "changes the interaction being measured" and makes the agreement
predicate ill-defined once both sides are sets. The first is true and turns out
to be the point: a jump and a picker are different interactions and the
ambiguous case genuinely is the second one, so measuring it as a failed jump
was measuring the wrong thing. The second was a real problem and is now solved
rather than avoided — core doc [section 6] splits agreement into `top1` and
`contained` precisely because the naive set predicate improves by returning
more.

**`Margin` survives, with a changed job.** It is still the primary confidence
input and still the signature of ambiguity — a unique candidate has maximal
margin, near-ties have none. What it no longer does is decide anything. It
describes the shape of the list rather than gating whether there is one, and
it is what a future floor would gate on.

**Ranking is now user-visible, which mostly helps.** The total order below used
to be an internal tiebreak whose arbitrary last component was a 1-in-N coin
flip; now it is the order of a list the user reads, so an arbitrary tail
ordering costs a few seconds of scanning rather than a wrong jump. That is a
real reduction in the cost of getting ranking wrong, and it is the main reason
this decision improves the strata the tool is worst at.

The comparator is still a total order, and determinism is still required —
by `shim.md`'s mode-equivalence test (§14.9) and by `measure replay`
([section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)).
Score, then import tier, then `(ProjectPath, name_range.start)`
lexicographically. Scores are `f32`, so the comparator is
**`f32::total_cmp`** — not `partial_cmp` with an `unwrap`, which
`CLAUDE.md` forbids anyway, and not a hand-rolled epsilon comparison, which
would not be a total order. `total_cmp` is total by construction, including
across `NaN`, which matters less because a `NaN` score is a bug than because
a comparator that is *only usually* a total order produces a sort that is
only usually deterministic. Applied to a *collected* result set, never to candidates
reduced in fan-out completion order — and now the whole ordering matters, not
just which element ends up first, so an unstable comparator that used to be
invisible below the winner is now visible in the picker.

**The list is capped.** Past some length a picker is worse than nothing.
`truncated_list` records that the cap was hit, because it changes what
containment means: the metric was only ever computed over what survived the
cap, so an uncapped and a capped `match_contained` are not the same
observation. Where the cap sits, and whether hitting it should truncate or
abstain, is `open-questions.md` question 12 — truncation is the provisional
choice, on the grounds that it keeps producing data about the case.

## 7. Confidence and the commit decision

In v1 there is no decision: if a candidate survived
[section 6](#6-candidate-verification), it is committed. This section is
therefore about the *number*, and about keeping the shape of a decision in
place so that a floor is later a table rather than a rewrite.

### 7.1 Confidence is a calibrated estimate, not a vibe

`Confidence` means one thing: **the estimated probability that this answer will
match the proper LSP**, under the agreement predicate in core doc [section 6].
That definition is only worth anything if it is measured, so:

* A handler computes confidence from a small, named set of bounded features per
  stratum — margin, import tier, uniqueness, predicate violations. Not a
  hand-tuned float at each call site.
* `measure replay` records `(stratum, confidence, agreed?)` for every committed
  answer, which under the permissive posture is nearly every query with a
  candidate.
* The resulting table *is* the calibration.

Note that calibration is now cheap to redo and does not need a language server.
Core doc [section 7] freezes the LSP's answers into `truth.jsonl` once per
(repo commit, server version), so re-fitting a confidence model is a replay
over stored data rather than a fresh corpus run against a live server. That
is what makes
[section 7.2](#72-how-a-threshold-would-be-derived-when-there-is-one) a
realistic future step rather than an aspiration.

The awkward part, stated rather than hidden: **on day one the number is not a
probability of anything.** There is no data to calibrate against, so the first
implementation emits a monotone score built from the features above and calls
it a confidence. That is a lie of precision, and the only thing that makes it
acceptable is that nothing consumes it as a probability until a corpus run has
mapped score to observed agreement rate. Two rules keep it from becoming a
permanent one:

* **Confidence must be ordinally meaningful from the start.** Within a stratum,
  a higher number must mean "more likely to agree". The mapping from score to
  probability can be fitted later; the *ordering* cannot, because a fitting
  procedure cannot repair a score that ranks wrong answers above right ones.
* **The features are recorded alongside it**, not just the collapsed number.
  `margin` and `considered` go on the trace record for exactly this reason:
  they are what a threshold would later be set on, and a corpus run that kept
  only the collapsed confidence could never answer *what would a floor have
  cost?* — the question the whole permissive posture exists to ask. With them
  stored, recalibration is a re-fit over data rather than a re-run of the
  corpus.

This is the same argument the core doc makes for building `Confidence` at all
before anything gates on it, carried one level down into how the number is
produced.

### 7.2 How a threshold would be derived, when there is one

Not v1. Recorded here because
[section 12](#12-predicted-coverage-and-precision) predicts numbers this
procedure would consume, and because `high-level.md`'s "Coverage at a precision
floor" says the prerequisite is the per-stratum table.

For each stratum, the threshold would be the lowest confidence bucket whose
measured agreement rate clears the floor **at the lower bound of a Wilson
interval**. Requiring the lower bound rather than the point estimate is what
stops a bucket with twelve samples and no observed errors from unlocking a
stratum on the strength of having barely been tested. A stratum with no data
would not commit.

That last clause is the one that could not have been arrived at by starting
strict, and it is worth noticing why: under the old bootstrapping rule, a
stratum with no data did not commit, so it generated no data, so it never
acquired a threshold. The rule was self-sealing.
[Section 7.5](#75-bootstrapping-is-permissive-and-this-reverses-an-earlier-decision)
is where that gets corrected.

### 7.3 Per-stratum, not global

The floor in `high-level.md` is stated globally: >=97% of committed answers must
match. A global constraint permits subsidy — a stratum at 99.5% could carry one
at 95% and the aggregate still clears.

When it arrives it should be enforced **per stratum** anyway, because subsidy is
only stable if the stratum mix is stable, and it is not. The mix depends on the
repository, the language, and the individual user's habits. Tuning on a corpus
whose mix happens to be 60% local bindings and then running on a codebase full
of wildcard imports and ambiguous names would silently drop real precision
below the floor with nothing in the metrics showing it, because the corpus
number would still look fine. Per-stratum enforcement is invariant to the mix.

Deferring the floor makes this cheaper to get right rather than moot: the v1
corpus run measures every stratum's precision directly, so the mix-sensitivity
that motivates the argument becomes something to check rather than something to
assert. [Open question 4](#open-questions).

### 7.4 Where the decision lives

The threshold is data; the language should not see it. But `Outcome` is the
handler's return type, so the handler has to be the one to say `Committed`.
Resolved by passing the policy in and requiring handlers to end every path
through it:

```rust
pub struct CommitPolicy { /* stratum -> minimum Confidence */ }

impl CommitPolicy {
    /// v1's instance returns `Committed` for every input.
    pub fn decide(
        &self,
        stratum: Stratum,
        confidence: Confidence,
        locations: Vec<Location>,
    ) -> Outcome;
}
```

`Query` carries `policy: &'a CommitPolicy` (core doc [section 1]). Handlers
never construct `Outcome::Committed` directly.

**In v1 this funnel is inert**, and that is the honest objection to it: it is a
parameter, a trait method call, and a rule for handler authors, all to express
a policy that currently has no content. The case for building it anyway is
narrow but, I think, sufficient:

* `shim.md` [section 14.6] already asserts that a per-mode floor is "a data
  change rather than a code change," and cites this section by name. Without
  the funnel that claim is false, and the cost of making it true later is
  auditing every commit site in every language crate — the work scaling with
  languages, at the exact moment when there are the most of them.
* The v1 policy object is not entirely vacuous: it is where the per-mode
  distinction of `shim.md` [section 14.6] will land, and having one place that
  is *allowed* to know about modes keeps mode knowledge out of `lang_*`.
* It costs one field and one call.

The driver's redundant re-check — downgrade a `Committed` that the policy would
not have allowed, and log it as a handler bug — is **not** built in v1, because
in v1 there is nothing it could catch. It comes back with the floor.

### 7.5 Bootstrapping is permissive, and this reverses an earlier decision

The previous revision of this section said the opposite, and the reversal is
worth recording rather than quietly overwriting, because the earlier reasoning
was superficially sound.

It said: commit only `LocalBinding`, unique `SameFileModule`, and
`ExplicitImport` via `File`/`Declared`; everything else abstains; low initial
coverage is correct because "starting permissive and tightening means the first
numbers are a fiction and every subsequent comparison is against that fiction."

That argument confuses two different numbers. It is true of the *headline
coverage* number — a permissive v1 reports a coverage figure that a later floor
will cut into, and comparing across that change is meaningless. It is false of
the *per-stratum precision* table, which is the artifact that actually drives
work, and which a strict start leaves almost entirely empty. A stratum that
never commits produces no `(confidence, agreed?)` pairs, so it has no measured
precision, so its threshold has to be guessed, so it either stays dark forever
or gets unlocked on intuition. `high-level.md` now makes exactly this argument, and
it is right.

So the initial policy commits whenever a candidate exists, and the two
abstention paths that remain are structural:

* No candidate — not an identifier, unsupported role, nothing found, or the
  target is outside the workspace.
* The deadline expired with nothing to show for it.

The cost is real and should be named: the first corpus run will report a
coverage number that is not comparable to any later one, and some strata will
show precision low enough to be embarrassing. Both are acceptable, because both
are *measurements*, and the alternative was an empty table plus a guess.

## 8. Strata and abstention reasons

**`Stratum` and `AbstainReason` are defined in `core.md` §1**, with the rest of
the seam, because `Outcome` carries them and `shared` is where the seam lives.
This section says what they mean and how a query gets one — the part that is
resolution's business rather than the driver's.

One stratum per row of `high-level.md`'s stratification list, plus
`Unimplemented` for the untouched language template.

### Assignment is a-priori, with one refinement

The stratum is a property of the **reference**, assigned from stages 0–3 before
any search runs. It is not "which stage produced the answer."

This matters because core doc [section 1] requires a stratum on the `Abstain`
arm too, and if the stratum were assigned by the successful stage, everything
that failed would pile into whichever stage happened to give up. Per-stratum
coverage would then be computed over a denominator that moves whenever the
implementation changes, which makes `high-level.md`'s central table
non-comparable across versions — the one property it needs.

Two classes cannot be known in advance, because they are discovered by
searching:

* `AmbiguousName` — many verified candidates, no margin.
* `ExternalDependency` — the only plausible target resolved outside the
  workspace.

So a query may be refined **once**, from its a-priori class to one of those
two. The trace record carries both:

* **Coverage** is reported on `stratum_prior`, so the denominator is fixed by
  the reference and does not move.
* **Precision** is reported on `stratum_final`, so a committed answer is judged
  against the class it actually turned out to be.

`AmbiguousName` has changed character twice. Under the floor it was mostly an
abstention class. Under the permissive single-answer rule it briefly became a
committed class with an expected-terrible precision row. With a ranked list it
becomes something better than either: **a large-result-set class**, whose
containment should be high and whose top-1 will be near `1/considered`. Its
result-count distribution is now the interesting column, and it is the row that
says most about whether the ranking is doing anything — a stratum where
containment is high and top-1 is at chance is one where the ranking has no
signal, which is a specific and actionable finding rather than a general
disappointment.

### Abstention reasons

Also `core.md` §1. Two things about the shape are worth stating here, since
this is where the reasons are actually produced.

**They carry no resolution vocabulary.** An earlier revision had
`UnsupportedRole { role: ReferenceRole }` and `External { name: Namespace }`.
Both types are this document's, and `ReferenceRole` in particular is a claim
about what kinds of reference a language has — which
[section 1.2](#12-the-pipeline-is-a-shape-not-a-type) refuses to centralise.
So the variants are unit or carry primitives, and a handler that wants the
detail recorded puts it in the trace record.

**`Deadline` is the only one that is not a fact about the code.** Every other
reason is reproducible from the same snapshot; that one depends on the budget,
which is why `core.md` §7 makes replay enforce budgets deterministically
rather than by wall clock.

Five variants from earlier revisions are gone, and their absence is the
clearest single summary of what the last several decisions did.

Three went because the answer changed:

* **`Ambiguous { considered }`** — ambiguity now commits.
* **`BelowThreshold { confidence }`** — there is no threshold.
* **`TruncatedEmpty { by: Truncation }`** — nothing is truncated, so an empty
  search is `NoCandidates` and nothing else
  ([section 4](#the-scan-is-exhaustive-and-uniqueness-is-therefore-earned)).

The first two return with the floor, and `Ambiguous` is the one to reinstate
first, since it is the abstention with the best precision-per-unit-coverage
trade in the predicted table. The third does not return at all.

Two went because they were never decisions in the first place:

* **`HandlerError`** and **`NoParse`** were failures wearing an outcome's
  clothes. `goto_definition` returns `Result<Outcome, Error>` now, so a
  failure has somewhere to go, and the enum is once again what its name says.
  Core doc [section 1] has the argument; the metrics consequence — a broken
  handler must not read as a hard stratum — is the point of it.

`Deadline` is the newcomer, and it is the only reason here that is not a
property of the code: two runs of the same query on the same snapshot can
differ on it, which is why `measure replay` never produces it and why a
nonzero rate of it in the field is a latency finding rather than a resolution
one.

**These strings reach the user.** `shim.md` §14.5 answers every standalone
abstention with a `RequestFailed` error naming the reason, because a standalone
user has no second opinion to fall back on. That is the one place a variant's
payload earns its keep, and it is why `External` carries a name at all — "the
definition is outside the workspace" is less useful than naming what it
declined to leave for. Everything else says enough as a bare variant.

Rendering is `Display` on the enum, in `shared`, not a `match` in the driver —
the driver's job is to put the text in an error response, not to know what the
reasons mean.

It is `#[non_exhaustive]` and, per `CLAUDE.md`, **no wildcard match arms**.
That combination is deliberate for exactly this enum: the reasons the floor
will restore (`Ambiguous`, `BelowThreshold`) must fail to compile at the
`Display` impl and anywhere else that interprets them, rather than silently
falling into a `_ => "could not resolve"` arm and shipping a worse message than
the one they replaced.

One consequence is worth flagging as a live problem rather than a solved one.
`shim.md` §14.5's worked example of a good abstention message is
"could not resolve `parse_config`: ambiguous, 7 candidates" — which is
precisely the variant v1 no longer has. With the floor dropped, standalone's
abstentions are dominated by `NotAnIdentifier`, which fires whenever the user
presses go-to-definition on a keyword, a comment, or whitespace. An error
response for that is noise, and unlike the ambiguity case it teaches the user
nothing. [Open question 6](#open-questions).

Two of these the driver acts on rather than merely logging:

* **`NoCandidates`** triggers the background file-list rescan from core doc
  [section 4](core.md#4-project-file-enumeration), debounced. The query that
  triggered it still abstains; the
  next query there sees a fresh list. This is the mechanism that section assumed, and it
  is trustworthy precisely because the scan was exhaustive: "not found" now
  means the name is not in the file list, which is evidence about the list.
  Under the old partial-scan rule it meant "not found *yet*", which is
  evidence about nothing. **`Deadline` does not trigger it** — the search was
  cut off rather than completed, so it says nothing about the list, and
  rescanning would spend I/O in the window that just proved to be short of
  it.
* **An `Err` return** — not an abstention reason at all, since core doc
  [section 1] removed `HandlerError` from this enum — feeds the
  repeated-failure handler disable in
  `shim.md` [section 11](shim.md#11-failure-handling). It reaches the editor
  as an abstention and the metrics as `decision: "failed"`, which is the
  whole point of separating them.

The rest exist so the per-stratum table can say *why* a stratum has low
coverage, which is the difference between a table that drives work and a table
that reports it.

## 9. What is shared, and what is not

Derived from the Rust handler in [section 10](#10-the-rust-handler), not
predicted in advance. This section is written last on purpose: an inventory of
shared utilities designed before any language exists is a framework wearing a
different hat.

**This rule has hardened into a schedule, and the schedule is stricter than
the rule.** `loops.md` §1 and §13 and
`phases.md` now take it as a hard sequencing constraint rather
than a preference, and they go further than "extract after, not before":

* **Phase 2 has no shared resolution code at all.** Not "a small amount, added
  carefully" — none. Two languages that need the same helper each write their
  own and the duplication is left standing. The reason is one this section did
  not have to consider when it was written: the language loops run
  *concurrently*, so a shared crate is a surface two writers contend on and a
  source of cross-language regressions that neither loop's metrics would
  attribute correctly.
* **Extraction is phase 3's job, under phase 3's equality constraint.**
  `phases.md` requires that the deterministic responses do not
  change at all across that refactor. So extraction is not a design activity
  with judgment in it; it is a mechanical transformation checked by replaying
  the corpus before and after
  ([section 11](#11-testing)).

**So none of the shared modules described below exist during phase 2**, and the
table is an inventory of what phase 3 would be expected to find, not a plan to
build it. Phase 3 grows them inside `similarity` rather than spawning a second
shared crate (`loops.md` §13). One exception, already carved out by core doc
[section 9](core.md#the-dependency-graph):

### `similarity` — the one exception, ported and frozen

The similarity code from [section 5](#5-reuse-from-the-prior-implementation) —
`Occurrences`, `IdentifierParts`, path–namespace scoring — is shared from the
start, and can be *precisely because it is not being written*. It is a
known-good body of code that predates every language crate, so it generates no
churn and no contention. **Nothing is added to it during phase 2.** A language
that wants a similarity helper it does not already have writes that helper
locally, like any other.

### What phase 3 would be expected to extract

| Module | Contents |
|---|---|
| `search` | `ScanRequest` construction, word-boundary literal matching, hit iteration, `DefinitionHints` |
| `ranking` | Score components, combination, `Margin`, the total-order tiebreak |
| `policy` | `CommitPolicy`. The always-commit instance; the calibration table format and loader arrive with the floor |
| `tree` | Ancestor walks, enclosing-node-of-kind, node text extraction |
| `paths` | Path proximity, `SearchOrigin` construction |

This is a prediction, and it is written down so that it can be *wrong* — if
phase 3 finds the real duplication is somewhere else, the table was the
guess and the corpus was the evidence.

`tree` is the one to watch. It holds small mechanical helpers over
`tree_sitter::Node` — walk to the nearest ancestor whose kind is in a set,
extract a node's text, iterate named children. It must not grow into a
declaration extractor or a query loader; those are the framework
[section 1.2](#12-the-pipeline-is-a-shape-not-a-type) rules out, and they will
present themselves as reasonable additions to this module — most persuasively
during phase 3, when extraction is the sanctioned activity and one more
abstraction looks like more of the same good thing.

### Per-language, and why each resists sharing

| Concern | Why it cannot be a config knob |
|---|---|
| Binding forms | The set of node kinds that bind, and their shadowing rules, is the language |
| Import syntax | Nesting, aliases, groups, and re-exports differ structurally, not parametrically |
| Module → path mapping | Rust declares it, Python derives it from directories, TS resolves it through config |
| Definition kinds | And which reference roles each can satisfy |
| Visibility | `pub(crate)`, `_name`, `export`, header/impl splits |
| Qualifier semantics | What `a::b::C` asserts about where `C` lives |

### Explicit non-goals for the shared crate

No pipeline driver. No query-file loader. No per-language config struct. No
trait that languages implement other than `LanguageHandler` itself.

## 10. The Rust handler

The first language, and the worked example the shared utilities are extracted
from.

```
crates/lang_rust/src/
  lang_rust.rs    Handler impl, the pipeline written longhand
  reference.rs    stage 0: role, qualifier
  scope.rs        stage 1: let/match/closure/fn-param/generic binding walk
  items.rs        stage 2 + 6: item extraction, DefinitionKind, visibility
  imports.rs      stage 3/4: use trees, aliases, globs, pub use
  modules.rs      module tree following, Cargo.toml awareness
  compat.rs       role/kind, qualifier, visibility predicates
```

The root is `lang_rust.rs` with `[lib] path` set in `Cargo.toml`, not `lib.rs`,
and there are no `mod.rs` files anywhere — both per `CLAUDE.md`.

### 10.1 Local bindings

`let` (including order-sensitive shadowing within a block), `if let`,
`while let`, `match` arm patterns, `for` patterns, closure parameters, function
parameters, generic parameters, lifetimes, `const`/`static` in blocks, and
loop labels.

The known hole is macro hygiene: an identifier introduced by a
`macro_rules!` expansion is not visible in the unexpanded tree. Stage 1 sees
the invocation, not the binding, and will fall through to later stages rather
than resolving it. That is the right failure — coverage, not precision — and
those references are `Stratum::MacroGenerated`, tracked separately.

### 10.2 The module tree is declared, not guessed

Rust's advantage over path-similarity heuristics: the module tree is written
down. `mod foo;` in `src/lib.rs` means `src/foo.rs` or `src/foo/mod.rs`, and
nothing else. So `crate::a::b::Thing` resolves by following `mod` declarations
from the crate root, reading at most one small file per path segment.

This is `ModuleTarget::Declared`, and it is the difference between Rust's
largest addressable stratum being exact or fuzzy. The cost is a chain of reads
bounded by module depth — typically two or three, all small, all cached in the
per-query view and the parse LRU across queries.

`self::` and `super::` resolve relative to the requesting file's own position
in that tree, which requires knowing it — found by the same walk, from the
root down.

`#[path = "..."]` attributes break the mapping. Handled where the attribute is
visible on the `mod` declaration being followed; not handled otherwise, which
is rare enough to accept as a coverage loss.

### 10.3 Crate roots

`other_crate::Thing` needs to know which workspace member `other_crate` is.
Found by locating `Cargo.toml` files in the file list and reading their
`[package] name` and `[workspace] members`. This is a small parse of files
already enumerated, not a build-system integration, and it is what makes
monorepo cross-crate references land in `ExplicitImport` rather than falling
through to whole-project search.

A crate name that matches no workspace member is `External`: an immediate,
labelled abstention rather than a project-wide hunt for a name that is not
there.

### 10.4 `use` trees and re-exports

Nested groups, `as` aliases, `self` in groups, and globs. `pub use` is recorded
where seen, and is why [section 6.1](#61-compatibility-is-scored-not-enforced)
demotes rather than filters on qualifier mismatch: a re-export means the
qualifier a reference uses is genuinely not where the definition lives, and
following re-export chains eagerly would cost reads on every query to fix a
minority of them.

### 10.5 Methods, fields, and the type-inference class

`x.foo()` is the case `high-level.md` says a heuristic fundamentally cannot compete
on, and predicts may be a permanent abstain class. Two sub-cases are genuinely
tractable:

* **`self.foo()`** — the receiver's type is the enclosing `impl` block's self
  type, which is right there in the tree. This is exact, not heuristic, and it
  is a large share of method calls in real Rust code.
* **Globally unique method names.** If a whole-project search finds exactly one
  `fn foo` anywhere, type inference is not needed to know the answer.

**Returning a list is what makes this class tractable at all**, and it is the
stratum the decision helps most. The previous revision had a real problem here:
with the floor gone and only one location returned, `x.foo()` against eleven
unrelated `fn foo`s committed to one of them — no information, the worst
severity tier, and high volume, which was a strong argument for carving out an
exception. That argument is now moot rather than answered. **Returning all
eleven, ranked, is a correct statement of what the tool knows**, and it needs
no exception to the permissive rule because it is not a guess.

So the expected shape of this row is low top-1 agreement, high containment, and
large result counts. That is not a stratum failing; it is the honest
description of a case where the language requires type inference and the tool
does not do type inference. The two exact sub-cases stay worth implementing,
and now they show up cleanly in the numbers: `self.foo()` returns a
single-element list, so it lands in top-1 rather than being averaged into the
containment figure.

What this class does need is a good *ordering*, since the user will be reading
the list rather than being teleported into it — which moves effort from
"choose correctly" to "rank plausibly", a much easier problem. Receiver-name
heuristics, same-module preference, and visibility all help order eleven
candidates without needing to eliminate ten of them.

Everything in this class is reported as `TypeInferenceRequired` regardless.
Keeping it as its own row, per `high-level.md`, is what stops `self.foo()`'s
exactness from being averaged into the rest and reported as if the class were
solved.

### 10.6 Macros

`foo!(...)` resolves to `macro_rules! foo` or to a `#[proc_macro]` function —
both are named definitions and findable by the ordinary path. Derive macros
in `#[derive(Foo)]` resolve to the derive's definition when it is in the
workspace. What is not resolvable is a reference to an item a macro *generates*;
those are `MacroGenerated` and abstain.

## 11. Testing

Per `CLAUDE.md` there are no unit tests. Everything below is either an
integration test over a fixture repository, a `proptest` property, an `insta`
snapshot, or the corpus metrics themselves. Failing seeds are committed under
`proptest-regressions/` and never deleted.

* **Fixture repositories.** Small, hand-built, per language, in
  `crates/lang_*/tests/fixtures/`. Each fixture pairs a cursor position with
  an expected outcome, including expected *abstentions*. Under the floor that
  was about catching a change that started guessing; with guessing now the
  specified behaviour, the abstention fixtures shrink to the structural cases —
  keywords, unsupported roles, `External`, empty searches — and they matter for
  a different reason: those four are the only paths that may abstain, so a new
  abstention appearing anywhere else is a bug even when it looks conservative.
* **Shadowing torture tests** for stage 1, since it is the stratum with the
  most volume and the least tolerance for error. Property-shaped: generate
  nested scopes with repeated names and assert the innermost binding wins.
* **Determinism.** The property the rest of the system is now built on
  ([section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)).
  Two other documents now depend on this section by name, and the second
  dependency is much heavier than the first:
  * `loops.md` cites it when arguing that metrics can be
    ratcheted in a baseline file rather than tracked as noisy statistics.
  * **`phases.md`'s phase 3 gate is an equality check on
    replayed outcomes** — the optimisation and shared-extraction phase may not
    change any answer, and that is verified by replaying the corpus before and
    after and comparing byte for byte. That converts a judgment call
    ("is this exchange of coverage for latency worth it?") into a yes/no, and
    it only works because the handler is a pure function of its inputs.
    That gate has **no carve-out**, and did not always: an earlier revision
    exempted queries that had exhausted their byte budget and now completed,
    since making the search cheaper legitimately changed their answers. With
    the budget gone there is no such class, so any difference at all fails the
    gate — which is the strongest form the constraint can take and needs no
    flag on the record to administer.

  Three `proptest` properties, not one:
  * **Repeatability.** The same query twice against the same snapshot,
    including deliberately constructed ties, yields byte-identical `Outcome`.
  * **Clock independence.** The same query under an artificially slowed clock
    yields the same `Outcome`, provided the deadline did not fire. This is the
    one that catches a handler branching on remaining time — the failure the
    exhaustive-scan rule exists to make impossible rather than merely
    discouraged.
  * **Order independence.** With fan-out forced to complete in a shuffled
    order, the `Outcome` is unchanged. This catches reduce-as-you-go, which
    is the failure mode that passes every test on an idle machine.
* **Exhaustiveness.** For a fixture repository small enough to enumerate by
  hand, assert that a stage-5 search visited every file with a matching
  extension. This is what the removal of the byte budget is worth: the
  property that used to require a calibration now requires an assertion, and
  it is the premise the uniqueness signal rests on
  ([section 4](#the-scan-is-exhaustive-and-uniqueness-is-therefore-earned)).
* **Deadline abstention is all-or-nothing.** With the clock forced to expire
  part-way through a search, assert the `Outcome` is
  `Abstain { reason: Deadline, .. }` and never a `Committed` built from what
  had been seen. The failure being caught is a partial commit that looks
  exactly like a complete one.
* **Scope escape.** Assert `ProjectView` yields nothing outside the workspace
  roots and nothing gitignored, including via `..` in a `lookup` argument.
  Fuzz-shaped rather than enumerated: the interesting inputs are the ones
  nobody thought of.
* **Per-stratum metrics as the regression suite.** With no calibration table
  there is nothing to regress against, so the corpus run itself is the test:
  committed coverage and measured precision per stratum, with a recorded
  baseline and a threshold on how far either may move without explanation. This
  replaces the calibration-regression test the previous revision specified,
  which cannot exist until a table does.

  Mechanically this is `measure replay` against the frozen `truth.jsonl`, not a
  live corpus run — core doc [section 7](core.md#two-modes-collect-and-replay).
  Two consequences for this suite. It is cheap enough to be a *test*, which is
  the only reason it can sit in this list at all. And it is **deterministic**:
  the handler is deterministic by the property above and the LSP's answers are
  frozen, so a per-stratum number that moves has a cause in the diff. There is
  no run-to-run noise to threshold against, only sampling noise in how many
  queries a stratum has — which is a property of the corpus, not of the run,
  and is what the per-stratum interval is for.
* **Held-out corpus.** Per `high-level.md`'s development plan, **five** of the
  ten repositories per
  language never seen by tuning, in `../heuristic-jump-corpus/test/` — a
  sibling of `training/`, not a subdirectory of it, so isolation is a path a
  session was never given rather than a rule it was asked to respect (core
  doc [section 7], `loops.md` §12). Both numbers reported; the
  gap is the overfitting signal, and it should be reported per stratum, since
  overfitting
  will not distribute evenly across them. This matters more under the
  permissive posture than it would have under a floor: with every stratum
  committing, there is far more surface for a tuning session to overfit the
  ranking weights to, and the ranking weights are now the only thing deciding
  a large fraction of answers.

## 12. Predicted coverage and precision

Written before measurement, so the corpus can falsify it. Rust, share-of-queries
from `high-level.md`'s expectation that a complete identifier scan is dominated by
locals. Coverage here is **handler coverage** in `high-level.md`'s sense — the
fraction of corpus queries resolution answers, measured by `measure replay`.
Delivered coverage is a different and much smaller number, and it judges the
health model rather than anything in this document.

Predictions have been rewritten twice — once when the floor dropped and again
now that the answer is a ranked list. The columns follow `high-level.md`'s three
numbers: coverage, **top-1** agreement, **containment**, and the median result
count that makes containment meaningful.

| Stratum | Share | Coverage | Top-1 | Contained | Median N | Notes |
|---|---|---|---|---|---|---|
| `LocalBinding` | ~45% | >95% | >99.5% | >99.5% | 1 | Exact. A list of one, always |
| `SameFileModule` | ~15% | ~95% | ~95% | ~98% | 1 | Same-name-across-impls is the only case that returns several |
| `ExplicitImport` | ~15% | ~90% | ~90% | ~95% | 1 | Declared module tree is exact when it works |
| `TypeInferenceRequired` | ~10% | ~85% | ~40% | ~90% | 4 | The shape of the decision: chance-level top-1, high containment. `self.foo()` is the single-element part |
| `WildcardImport` | ~5% | ~90% | ~70% | ~90% | 2 | |
| `AmbiguousName` | ~4% | ~95% | ~30% | ~92% | 6 | Containment is the whole value here; top-1 is roughly `1/N` |
| `ExternalDependency` | ~4% | 0% | n/a | n/a | — | Out of scope per `high-level.md`. The one row nothing so far has changed |
| `MacroGenerated` | ~2% | ~40% | ~55% | ~70% | 2 | Macro definitions resolve; generated items do not |

Weighted, roughly 87% coverage at roughly 82% top-1 and 96% containment. The
gap between those last two is the point of the whole decision: it is the share
of queries where the tool cannot pick but can narrow, and under the previous
revision every one of them was scored as a wrong answer.

The predictions most worth being wrong about:

* **The top-1/containment gap.** Predicted at ~14 points. If it is much
  smaller, the ranked list is machinery for a case that rarely arises and the
  single-answer design was right. If it is much larger, ranking quality — not
  candidate generation — is where the remaining work is.
* **Median N on `AmbiguousName` and `TypeInferenceRequired`.** These decide
  whether the picker is usable. A median of 6 is a list; a median of 30 is a
  wall of text that the cap will be doing all the work on, and
  `open-questions.md` question 12 becomes urgent rather than tidy.
* **`AmbiguousName` share.** Predicted at 4% from the a-priori classification,
  but the refinement in [section 8](#8-strata-and-abstention-reasons) moves
  queries *into* it during search, so the measured share could be several times
  larger.
* **`ExplicitImport` coverage**, which the declared-module-tree work in
  [section 10.2](#102-the-module-tree-is-declared-not-guessed) is a bet on.
* **The `LocalBinding` share**, since if it is much higher than 45% the headline
  coverage number will be dominated by cases that never needed a heuristic —
  the exact flattery `high-level.md`'s stratification exists to prevent.

Also worth measuring early, per `high-level.md`'s value weighting: what fraction of
these queries the proper LSP would have answered in under 200ms anyway. If the
slow tail is concentrated in `ExternalDependency` and
`TypeInferenceRequired` — the two classes this design serves worst — the
per-stratum table will look healthy while the tool delivers little, and that
is the result most worth discovering before ten languages are written.

**Error severity, not just precision.** `high-level.md` now measures the three
severity tiers with no budget attached, and under a permissive posture that
distribution is a better health signal than the precision number itself. A
stratum at 90% whose errors are all same-file-further-than-3-lines is fine; a
stratum at 95% whose errors are all wrong-file-unrelated is the trust-destroying
one. The prediction is that whole-project search concentrates errors in the
worst tier, because it is the only stage whose candidates are unconstrained by
file, and that is the specific thing to check first.

## Open questions

Numbers are stable identifiers, not an ordering or an inventory. A resolved
question keeps its number and says what was decided, so references from other
documents do not rot.

1. **Should a truncated search commit?**
   **Resolved — the question is void: nothing truncates.**
   [Section 4](#the-scan-is-exhaustive-and-uniqueness-is-therefore-earned)
   removes the per-query byte budget, so a scan either completes or the
   deadline aborts the whole query. Two answers were held and withdrawn
   before this one — commit only when confidence does not rest on uniqueness,
   then commit always and mark the row — and both were attempts to price an
   unscanned remainder that no recorded feature could describe. Deleting the
   remainder is what actually settles it. The cost moves to latency, which is
   [question 15](#open-questions) and phase 3's problem.

2. **Should `TypeInferenceRequired` be exempted from the permissive rule?**
   **Resolved — no exemption needed.** The question existed because
   multi-candidate `x.foo()` had precision bounded by `1/candidates` while
   returning a single answer, which made it the strongest case for carving out
   an abstention. Returning a ranked list dissolves it: all the candidates go
   back, containment is high, and nothing is being guessed, so there is no
   exception to make. [Section 10.5](#105-methods-fields-and-the-type-inference-class)
   is rewritten accordingly, and the work moves from "decide correctly" to
   "order plausibly."

3. **Is `CommitPolicy` worth threading through `Query` in v1?**
   **Resolved — yes, adopted.** [Section 7.4](#74-where-the-decision-lives)
   keeps it and core doc [section 1] now carries it. It is a parameter and a
   discipline for handler authors, expressing a policy with no content today,
   which is the whole case against. The case for is that the alternative
   migration — auditing every `Outcome::Committed` site in every `lang_*`
   crate when the floor arrives — scales with the number of languages, and
   this project plans to grow a lot of them. Half-adopting it (some handlers
   funnel, some don't) is worse than either choice, which is why it was worth
   deciding rather than leaving to drift.

4. **Per-stratum or global, when the floor arrives?**
   [Section 7.3](#73-per-stratum-not-global) argues per-stratum on
   mix-invariance grounds; `high-level.md` states the floor globally. The dropped
   floor turns this from an argument into a measurement — the v1 corpus gives
   every stratum's precision directly, so the mix sensitivity can be computed
   rather than asserted. Do that before choosing.

5. **Should ambiguity return multiple locations instead of one?**
   **Resolved — yes.** [Section 6.4](#64-the-output-is-a-ranked-list) now
   returns the ranked list and the editor shows a picker. The two objections
   recorded here were that it changes the interaction being measured and makes
   the agreement predicate ill-defined on both sides. The first was correct and
   was the wrong way round — the ambiguous case genuinely *is* a picker, so
   scoring it as a failed jump measured the wrong thing. The second was a real
   gap and is closed rather than tolerated: core doc [section 6] splits
   agreement into `top1` and `contained`, because the naive set predicate
   improves monotonically as the tool returns more, which is the flaw
   `high-level.md` rejects plain match rate for.

   What is left over is the cap, which is `open-questions.md` question 12 and is
   genuinely open in both halves — where it sits, and whether hitting it should
   truncate or abstain.

6. **Is an error response per abstention still right in standalone?**
   `shim.md` §14.5 answers every standalone abstention with
   `RequestFailed` naming the reason, and already flags this as worth
   revisiting. Dropping the floor changes the input to that decision: the
   abstention mix is now dominated by `NotAnIdentifier` — the user pressed
   go-to-definition on a keyword — rather than by the informative
   "ambiguous, 7 candidates" case the core doc uses as its example. An error
   for a keyword press is noise. Options: keep the error only for reasons that
   say something (`External`, `NoCandidates`, `Deadline`) and answer `null`
   for `NotAnIdentifier`, or keep it uniform and accept the noise. The
   core doc's example message should be updated either way, since it names a
   variant v1 does not have.

7. **Does body text similarity come back as a tiebreak?**
   [Section 5](#5-reuse-from-the-prior-implementation) drops it, and the
   argument for dropping it was strongest under a floor. Now that something has
   to break ties and the alternative is an arbitrary total order, "most similar
   body" is no longer obviously worse than what it is competing against. It
   stays out of v1 on cost grounds — it needs candidate body text, not just a
   signature line — but if ties turn out common, this and pagerank-style module
   ranking (`open-questions.md` question 1) are the two signals to evaluate together.

8. **How is confidence produced before any calibration exists?**
   [Section 7.1](#71-confidence-is-a-calibrated-estimate-not-a-vibe) requires
   the day-one number to be ordinally meaningful but admits it is not a
   probability. Two sub-questions with no data behind them yet: whether a
   monotone combination of the ranking features is good enough to be worth
   recording, or whether the features alone should be recorded and the
   confidence field left at a per-stratum constant until there is a fit; and
   whether `Confidence` should be a bucketed ordinal rather than an `f32`,
   which would make the false precision impossible to read into it.

9. **Should re-export chains be followed eagerly?**
   [Section 6.1](#61-compatibility-is-scored-not-enforced) demotes on qualifier
   mismatch rather than resolving the chain, because resolution costs reads on
   every query to fix a minority. If the corpus shows re-exports are a large
   share of `ExplicitImport` errors, a bounded one-hop follow is the natural
   next step. This got worse with the dropped floor — a demotion used to lead
   to an abstention and now leads to a committed wrong answer — and then
   better with the per-server oracle, which supplies the missing evidence for
   free: core doc [section 7] observes that the set of positions where two
   servers disagree is largely a map of where re-export and alias chains
   matter. So this question is answerable from the first multi-server corpus
   run without designing an experiment for it, and it should be one of the
   first things that run is used for. It also splits in two — *whether* to
   follow a chain may be shared logic while *where the chain stops* is a
   profile field.

10. **How many confidence buckets?** Too few and thresholds are coarse; too
    many and each bucket's Wilson bound is too wide to unlock anything. Depends
    on corpus size, which does not exist yet. Deferred along with the floor,
    but it constrains question 8's answer — and the per-server oracle has now
    made it harder, since a table indexed by
    `(language, server, stratum, bucket)` divides the same corpus across
    several times as many cells
    ([section 1.4](#14-the-correct-answer-depends-on-which-server)).
    Pooling across servers is not available, because that is exactly the
    average `high-level.md` refuses.

11. **Does `DefinitionHints` earn its complexity?** It exists to keep the parse
    set small on common names. Deleting it would remove a per-language regex
    surface, and regexes that may only reject are still regexes that can be
    wrong. `CLAUDE.md`'s "implement the slow simple version first" argues for
    starting without it and adding it only if the p99 measurement demands it.

    **The exhaustive-scan decision moved this question a long way toward
    yes.** With no byte budget, every file with a literal hit reaches the
    parse stage, so on a name like `new` the prefilter is the only thing
    between the query and thousands of parses — and the deadline, which is now
    the sole bound, converts that directly into abstentions. Expect to need
    it; measure anyway.

12. **Should stage 5 search all extensions or only the requesting language's?**
    Only its own is the assumption throughout. Polyglot references — a TS file
    referencing a generated binding, an FFI declaration — would need otherwise,
    and the handler registry already knows every extension.

13. **Should the byte budget scale with the deadline, and how?**
    **Resolved — void, there is no byte budget.**
    [Section 1.3](#13-the-search-is-exhaustive-and-the-clock-may-only-abort-it)
    removes it. This had grown from a minor question into a load-bearing one,
    because the budget↔deadline mapping was what made `measure replay`
    reproduce live behaviour rather than merely reproduce itself, and a badly
    chosen mapping would have made every tuning number a measurement of a
    configuration nobody runs. Deleting the budget deletes the calibration:
    replay is deterministic because the search is exhaustive, not because a
    constant was fitted well.

14. **Is the deterministic budget rich enough to be the only control?**
    **Resolved — void, same reason.** The question was whether bytes, files,
    and parses fully describe search cost, given that parse cost is
    superlinear in file size for some grammars and a cold read is dominated by
    seek latency rather than length. That mattered only while those counters
    decided where a search stopped. They are now recorded and nothing branches
    on them, so an incomplete cost model is a reporting imprecision rather
    than a behavioural one. The underlying observation survives as a note to
    whoever reads `bytes_scanned` as if it were latency: it is not.

15. **What happens to a query the clock kills mid-search?**
    **Resolved — it abstains, whole.** `AbstainReason::Deadline`, nothing
    returned, no partial commit
    ([section 4](#the-scan-is-exhaustive-and-uniqueness-is-therefore-earned)).
    A handler interrupted at an arbitrary point has an arbitrary candidate
    set, so committing from it would be the one answer in the system whose
    quality nothing bounds.

    **What remains open is the rate, and it is now more important than the
    rule.** The deadline is the only bound on a search, so this is no longer a
    rare anomaly at the tail of a well-calibrated budget — it is the mechanism
    by which large repositories lose coverage. A nonzero rate is expected; a
    large one means the search is too slow for the budget in
    `high-level.md`, which is a phase 3 finding. Report it per stratum and per
    repository size from the first corpus run, since it is the number that
    says whether the exhaustive-scan decision was affordable.

16. **Does `ProjectView` need to expose remaining budget?**
    **Resolved — no, and there is none to expose.**
    [Section 3](#reads-are-cached-per-query-and-counted-but-not-capped) keeps
    the byte and file counters as trace fields and exposes no remaining-budget
    number. The hazard this avoids was the reason to be nervous about the
    budget in the first place: a handler that can read what is left can make
    the answer depend on how much earlier stages happened to consume, so a
    query that read a large file in stage 3 would search differently in stage 5
    than one that did not. Deterministic, but a coupling between stages that
    nothing else in the design has.

17. **What does a handler do with no oracle?**
    `open-questions.md` question 14 asks which server standalone should imitate
    — a neutral profile, or the most widely deployed one — and notes it also
    covers proxying a server we have no profile for. It is filed as a product
    question, but it lands here, because a handler with an empty
    `ServerProfile` has to do *something* and the current answer is "whatever
    the shared logic happens to do," which is a default rather than a decision.
    The sharp version: shared logic is developed and measured on positions
    where all servers agree
    ([section 1.4](#14-the-correct-answer-depends-on-which-server)),
    so on the positions where they *disagree* it is untuned by construction —
    and those are exactly the positions an empty profile has to answer.
    Standalone is therefore not "the average server", it is "the server nobody
    measured", and it is the mode with no divergence report to catch that. A
    concrete option worth measuring: pick the most widely deployed server's
    profile as the standalone default and report standalone's numbers against
    that server's truth file, which at least makes the choice measurable.

18. **Does the shared/profile split hold, or does it leak?**
    [Section 1.4](#14-the-correct-answer-depends-on-which-server)
    adopts core doc [section 7]'s decomposition: shared logic owns the
    positions where servers agree, profiles own the rest. It is a good split
    and it may not survive contact. The failure mode is a divergence that is
    *not* localisable to a knob — where matching pyright means ranking
    differently throughout rather than choosing differently at one point. Then
    a profile field is not enough and the honest options are a per-server
    ranking weight set (which is the config format everything here rules out)
    or accepting a worse score on one server. Worth watching for from the first
    multi-server run, because the shape of the fix is much cheaper to choose
    early than to retrofit. A related smaller question: are profile fields
    allowed to affect *confidence* as well as the answer, given the calibration
    is already per-server?
