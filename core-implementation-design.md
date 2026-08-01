# Core implementation design

This covers the core driver only:

* The LSP shim/proxy that sits between the editor and the proper language
  server.
* Knowing the current state of the project's files.
* Dispatching go-to-definition calls to the language-specific handler, in
  parallel with the proper LSP and with each other.

Out of scope for this document: the resolution logic itself, and the shared
utilities that resolution logic is built from. Those sit behind the handler
interface described in [Handler interface](#12-handler-interface), which is the
only part of them this document commits to.

See `readme.md` for the product rationale and the success metrics. The
metrics constrain this design in three concrete ways, noted where they bite:
the latency budget (p50 <= 20ms, p99 <= 150ms, hard cap 250ms), the >=97%
precision floor and its abstention path, and the per-stratum reporting
requirement.

## 1. The prime invariant

> If every heuristic component fails, the shim degrades to a transparent
> proxy, and the editor cannot tell it is there.

This is the top design constraint and it outranks everything else in this
document. The user is putting this process in front of their language server
for their whole working day. A shim that occasionally breaks completion,
diagnostics, or rename is far worse than no shim, because the failure is
mysterious and the natural suspicion falls on the language server.

Concretely this means:

* The forwarding path never depends on heuristic state. Handler panics,
  poisoned locks, blown deadlines, and unparseable documents all resolve to
  "forward and get out of the way."
* Anything the shim does not specifically understand is forwarded byte-for-byte
  without being deserialized into a typed struct and re-serialized. Round-trip
  through `lsp-types` is lossy for unknown extensions, and both rust-analyzer
  and Zed use custom methods.
* The shim adds at most one message-copy of latency to the forwarding path.
  Bookkeeping happens after the bytes are on their way, never before.

A useful implication: the shim should be tested by recording a real editor
session, replaying it, and asserting that every forwarded frame is identical
except the ones deliberately intercepted. See [Testing](#15-testing).

## 2. Process and transport model

`heuristic-jump rust-analyzer --some-flag` treats `argv[1..]` as the command
line of the proper server. The shim:

* Speaks LSP over its own stdin/stdout. From the editor's point of view the
  shim *is* the language server.
* Spawns the proper server as a child process, connected over the child's
  stdin/stdout.
* Forwards the child's stderr to its own stderr unchanged. Editors surface
  server stderr in a log panel and users rely on it.

Framing is standard JSON-RPC with `Content-Length` headers. The shim needs its
own framing codec rather than a full LSP client library, because it must
handle raw frames it does not want to interpret. Zed's `crates/lsp` is a
useful reference for the codec (`crates/lsp/src/lsp.rs:45` onward) but is not
reusable as a dependency: it is coupled to `gpui`, and its client model
assumes it owns both ends of the connection.

**Dependencies.** `tokio` for the runtime, `lsp-types` for the messages the
shim actually inspects, `serde_json` for the rest, Zed's `rope` (vendored, see
[section 16](#16-workspace-layout)) for document text, `tree-sitter` for
parses, `ignore` for file walking, `notify` for the optional watcher.
Deliberately not a framework: the shim's whole job is to be a thin,
predictable pipe.

### Task layout

Five long-lived tasks, communicating over channels:

```
  editor stdin  --> [reader:editor] --+--> [to-child tx] --> [writer:child]
                                      |
                                      +--> [core]  (owns all state)
                                                |
  child stdout  --> [reader:child]  --+---------+
                                      |
                                      +--> [to-editor tx] --> [writer:editor]
```

* **`reader:editor`** parses frames from the editor. For each frame it does the
  minimum classification needed to decide "intercept or forward" (see
  [Message routing](#3-message-routing)), then pushes the raw bytes to
  `to-child` and a classified event to `core`.
* **`reader:child`** the same in the other direction.
* **`writer:editor`** and **`writer:child`** each own one pipe exclusively.
  This is not optional: frames must not interleave, so exactly one task may
  ever write to a given fd, and everything else reaches it through an mpsc
  channel.
* **`core`** owns all mutable state (documents, in-flight requests, server
  health). It is a single-threaded actor processing an ordered event stream.
  It never blocks: heuristic work is handed to a worker pool with a snapshot.

The actor model is chosen specifically because **LSP message order defines
document state**. A `textDocument/definition` must be evaluated against the
document as of every `didChange` that preceded it and none that followed. A
lock-based design makes that ordering accidental; a single ordered event queue
makes it structural.

## 3. Message routing

Classification is by `method` for requests/notifications and by `id` for
responses.

**Editor to child:**

| Message | Action |
|---|---|
| `initialize` | Inspect (root, capabilities), forward |
| `initialized` | Forward |
| `textDocument/didOpen` / `didChange` / `didClose` / `didSave` | Tee to `core`, forward |
| `textDocument/definition` | Tee to `core`, forward. May be answered by the shim |
| `$/cancelRequest` | Tee to `core`, forward |
| `shutdown` / `exit` | Tee to `core`, forward |
| everything else | Forward verbatim |

**Child to editor:**

| Message | Action |
|---|---|
| Response to an id the shim already answered | **Swallow**, hand to `core` for divergence check |
| Response to any other id | Record latency in `core`, forward |
| `$/progress` | Tee to `core` (health), forward |
| `InitializeResult` | Inspect (sync kind, encoding, capabilities), forward |
| everything else | Forward verbatim |

The swallow case is the one that must not be got wrong. Once the shim has
answered request `id` on the editor's behalf, the proper server's eventual
response to `id` **must not** reach the editor, or the editor sees two
responses to one request. That is a protocol violation, and clients react to
it badly, ranging from a log warning to a stuck request slot.

### Request id namespacing

The shim may need to originate its own requests to the editor
(`window/showMessageRequest`, `window/showDocument`). LSP permits string ids,
so all shim-originated requests use `"hj-<counter>"`. This avoids a
bidirectional id remapping table entirely: any numeric id came from the
editor, any `hj-` prefixed id is the shim's own. The shim originates no
requests to the child, so the child's id space is untouched.

## 4. Initialize and capability negotiation

`initialize` is the only message the shim genuinely must understand before it
can do anything, because it carries:

* **`rootUri` / `workspaceFolders`** — the project root, needed for file
  enumeration and for deciding what "the project" means. Multi-root
  workspaces are supported by treating the folder set as the search scope;
  a query resolves within the folder containing the requesting document
  first, then the others.
* **Client capabilities** — specifically `window.showDocument` and
  `window.showMessage`, which determine whether divergence reporting
  (see [section 10](#10-divergence-reporting)) is available at all.

`InitializeResult` from the child carries:

* **`textDocumentSync`** — full vs incremental. The shim must apply changes
  the same way the editor is sending them; this value is what tells it which.
* **`positionEncoding`** — see below.
* **`definitionProvider`** — if the proper server does not provide
  go-to-definition, the shim could advertise it anyway and serve
  heuristic-only. That is a real mode (wrapping a server with weak
  navigation), but it inverts the "wait, then fall back" protocol into
  "always answer." Deferred; v1 passes the capability through unchanged and
  disables itself if the server declares no definition support.

Until `InitializeResult` has been seen, the shim answers nothing itself. It
has no root, no sync kind, and no encoding, so it cannot be correct.

### Position encoding

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
[section 16](#16-workspace-layout).

LSP 3.17 added negotiation: the client advertises `general.positionEncodings`
and the server picks one in `InitializeResult.positionEncoding`. Zed currently
advertises UTF-16 only (`crates/lsp/src/lsp.rs:793`), so with Zed the shim
will be doing conversion.

The rule: **the shim uses whatever the child negotiated, not what it would
prefer.** It is in the middle of a negotiation between two other parties and
does not get a vote on the outcome.

It does get one safe optimization: when forwarding the editor's `initialize`,
the shim may reorder `general.positionEncodings` to put `utf-8` first, as
long as it only lists encodings the editor itself advertised. Encoding is a
wire detail with no semantic content, so a server that picks UTF-8 because of
the reorder behaves identically, and the shim skips conversion entirely.

Conversion itself should live in one module with exhaustive property tests
against a reference implementation, and every position should be converted to
byte offsets at the edge so that no UTF-16 offset ever reaches the handler
interface.

## 5. Document state

The shim keeps a `HashMap<DocumentUri, Document>`:

```rust
struct Document {
    text: Rope,                        // vendored zed rope, utf16-aware
    version: DocumentVersion,          // from didChange; ordering and staleness
    language_id: LanguageId,           // from didOpen
    tree: Option<Tree>,                // tree-sitter; incrementally updated
    parsed_at: Option<DocumentVersion>,
}
```

`parsed_at` is an `Option<DocumentVersion>` rather than a sentinel version,
so "never parsed" and "parsed at some version" are distinguishable without a
magic number. The vocabulary types are defined in
[section 12](#12-handler-interface).

**Open documents** are authoritative from the editor. `didOpen` inserts,
`didChange` applies (full or incremental per the negotiated sync kind),
`didClose` removes. On `didClose` the disk copy becomes authoritative again.

**Closed files** are not tracked. They are read from disk on demand during
search and cached (see below). This is correct by construction: a file with
unsaved modifications is by definition open in the editor, so anything not
open matches disk.

**Should didChange be tracked at all?** `readme.md` leaves this open, hoping to
avoid it for lightness. It should be tracked, for a specific reason: the
value window of this tool overlaps almost exactly with the case where the
user has just typed something. A definition added thirty seconds ago is
precisely the kind the proper LSP has not caught up with, and the kind the
user is most likely to jump to. Serving a disk copy there gives a confidently
wrong answer at the moment the tool most needs to be right. The cost is also
low: incremental `Rope` edits are microseconds and the memory is bounded by
what the editor has open.

### Parse cache

Tree-sitter trees are kept for recently queried or recently edited documents,
as an LRU bounded by both entry count and total bytes (a single generated file
can be enormous). Entries:

* For open documents, keyed by `(url, version)`; a stale `parsed_at_version`
  triggers an incremental reparse via `Tree::edit` plus `Parser::parse` with
  the old tree.
* For disk files, keyed by `(path, mtime, len)`. `didSave` invalidates the
  open-document entry and lets the disk entry take over.

Incremental reparse needs `InputEdit` in byte offsets *and* tree-sitter
`Point`s (row, byte-column). Both come from the same conversion module as
positions.

The parse cache is a cache, not an index: cold misses are correct, just
slower. Nothing may depend on an entry being present.

## 6. Project file enumeration

Whole-project search needs a file list, and this is the one place where "no
index" needs a stated boundary.

There is no *symbol* index — no persisted map from name to definition sites,
which is the thing the readme rules out and the thing that would cost
startup CPU and invalidation complexity. But a cold directory walk of a large
repository can take hundreds of milliseconds on its own, which would consume
the entire 250ms budget before a single file is read. So:

**The driver caches a file list. It does not cache anything about file
contents beyond the parse LRU.**

* Built with the `ignore` crate, the same walker ripgrep uses, so `.gitignore`
  is respected for free. This directly implements the readme's decision that
  gitignored files are out of scope.
* Built in-process rather than by shelling out to ripgrep: subprocess spawn
  plus pipe overhead is a meaningful fraction of a 20ms p50 target, and
  in-process gives direct control over cancellation at the deadline.
* Built lazily on first need, then refreshed in the background. A stale list
  is acceptable — it costs recall on files created in the last few seconds,
  which is a miss, not a wrong answer, and misses are cheap under the
  precision-floored metric.
* Invalidated by a filesystem watcher (`notify`) **only where watching is
  cheap** — a workspace small enough, on a platform with a real recursive
  watch API. Watching a large tree costs descriptors and memory and wakes the
  shim on every build artifact write, which is the exact opposite of staying
  out of the proper LSP's way during its startup.
* Otherwise invalidated on demand: when a query finishes without a good
  candidate, that is itself the signal the file list may be stale, so a
  rescan is kicked off in the background. The query that triggered it still
  abstains, since it cannot wait for a rescan inside the deadline, but the
  next query on that spot sees a fresh list.

  This pairs neatly with the retry protocol: a second query on the same spot
  is already the expected path, so the rescan usually lands exactly when it
  is needed. Rescans are debounced, so a burst of misses triggers at most one.
* The watcher, where enabled, is best-effort and never blocks a query.

Search scope is the workspace folders only. External dependency sources
(`~/.cargo/registry` and equivalents) are excluded per the readme; this is
also what keeps the walk small enough for the no-index approach to be viable
at all.

## 7. Server health model

The readme calls for modelling availability from whatever information is on
hand, rather than only tracking whether a request is outstanding. The state:

```rust
enum ServerHealth {
    Starting,       // spawned, no InitializeResult yet
    Warming,        // initialized, but reporting work-in-progress
    Ready,          // responsive, no outstanding long work
    Slow,           // responsive, but latency well above its own baseline
    Unresponsive,   // requests outstanding beyond threshold, no traffic
    Dead,           // process exited
}
```

Signals, in order of reliability:

1. **Process liveness.** Unambiguous. Child exit moves to `Dead`.
2. **`InitializeResult` received.** `Starting` to `Warming`.
3. **`$/progress`** begin/end pairs with outstanding work tokens. This is the
   generic, cross-server signal for `Warming`. rust-analyzer reports indexing
   this way.
4. **Response latency**, tracked as a rolling distribution over *all*
   request kinds, not just definitions. `Slow` is defined relative to the
   server's own recent baseline rather than an absolute threshold, because
   the absolute numbers differ by orders of magnitude between language
   servers and between repository sizes.
5. **Silence with work outstanding.** Requests pending beyond a threshold with
   no frames of any kind arriving moves to `Unresponsive`.

Server-specific signals (rust-analyzer's `experimental/serverStatus`, for
instance) go behind a small optional adapter keyed by the child's advertised
`serverInfo.name`. This is a driver concern, not a language concern — it is
about the *server process*, not about the language's syntax — so it lives
here and not behind the handler interface.

### What health is for

Health selects the answering policy:

| Health | Policy |
|---|---|
| `Starting`, `Warming` | **Eager.** Answer heuristically on the first request. |
| `Ready` | **Retry-triggered.** Wait; answer only on a repeat query. |
| `Slow` | **Retry-triggered.** |
| `Unresponsive`, `Dead` | **Eager**, and answer `null` rather than abstaining. |

The eager rows follow directly from modelling health rather than just
request-pendency, and they are most of the practical value of doing so. If the
server is provably still indexing, making the user press go-to-definition
twice to discover what the shim already knows is pure friction. Conversely
when the server is `Ready`, waiting costs almost nothing and the proper answer
arrives; the readme's retry rule handles the occasional slow query.

The `Dead`/`Unresponsive` row differs in its abstention behaviour and is
explained in [section 9](#9-deadlines-and-abstention).

### Child death

The shim has hidden the child's crash from the editor: the editor's server
process (the shim) is still alive, so the editor will not restart anything.
The shim has therefore inherited responsibility for the child's lifecycle.

**The shim propagates child death.** On unexpected child exit it logs, reports
via `window/showMessage`, and exits with the child's status. The editor sees
its server die and applies whatever restart policy the user already has
configured and understands.

Supervising the child instead — restarting it, replaying state, and serving
heuristics through the gap — is deliberately not done here; it is tracked as
a future question in `readme.md`. The part that matters for this document is
that nothing in the architecture forecloses it. The shim already holds full
authoritative text for every open document, which is exactly the state a
restarted child would need replayed into it, so the decision can be revisited
without disturbing anything above.

## 8. Go-to-definition lifecycle

The protocol from the readme, stated precisely.

### State

```rust
struct PendingQuery {
    editor_id: EditorRequestId,
    position: ByteOffset,     // where to actually run the query
    spot: Spot,               // identity, for the repeat check
    arrived: Instant,
    answered_by_shim: Option<Vec<Location>>,
}

/// Query identity. Two requests are a repeat of each other exactly when
/// their `Spot`s are equal, so the rule in the flow below is a derived
/// `PartialEq` rather than a comparison open-coded at the call site.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Spot {
    /// Normal case: the position resolved to an identifier token, and any
    /// position inside that token is the same query.
    Token { uri: DocumentUri, span: ByteRange, version: DocumentVersion },
    /// Fallback when no parse is available and there is no token to widen to.
    Offset { uri: DocumentUri, at: ByteOffset, version: DocumentVersion },
}
```

`core` holds these keyed by `editor_id`, plus a secondary index by `Spot`
for the repeat check.

Making `Spot` an enum rather than a struct with an optional span is what lets
the repeat rule be `==`. A struct carrying both a position and an optional
token span cannot derive the right equality: two requests a character apart in
the same token must compare equal, and a derived comparison over the position
field would say otherwise.

### Flow

1. **Request arrives.** Forward to the child immediately (never gate
   forwarding on shim work). Record a `PendingQuery`.
2. **Determine the spot.** Two requests are at the same spot when they share a
   URI and their positions fall within the same identifier token span, with
   the document unchanged between them. Token span comes from the parse
   cache; when there is no parse available, fall back to exact position
   equality. Deliberately not exact-position-only: a user re-triggering
   go-to-definition may land a character off, and treating that as a new
   query silently defeats the whole retry protocol.
3. **Check the policy.** If health says eager, or this is a repeat of a spot
   with a still-pending query, dispatch to the handler. Otherwise do nothing
   and let the child answer.
4. **Handler returns.** On `Committed`, answer *every* pending query at that
   spot — the repeat and the original — as the readme specifies, and mark
   each `answered_by_shim`. On `Abstain`, see [section 9](#9-deadlines-and-abstention).
5. **Child responds later.** The response is swallowed. If the query was
   `answered_by_shim`, compare and possibly report divergence
   ([section 10](#10-divergence-reporting)). Either way, log the pair for the
   metrics ([section 11](#11-observability-and-scan-mode)).

### Cancellation

`$/cancelRequest` is forwarded and also drops the `PendingQuery` and signals
the handler's cancellation token. The shim must not answer a cancelled
request. If the shim had already answered before the cancel arrived, the
cancel is forwarded anyway and the child's response is swallowed as usual.

Note that the shim can also *receive* a stale cancel for a request it already
answered; this is harmless and must not be treated as an error.

## 9. Deadlines and abstention

The 250ms hard cap is enforced by the driver, not trusted to the handler.

**The deadline is absolute and starts at request arrival**, not at handler
entry. Queueing time counts. A handler that gets 250ms of wall clock but
started 200ms late has already blown the budget from the user's point of view,
and the metric in the readme measures the user's point of view.

**Cancellation must be cooperative.** A `tokio::time::timeout` around
CPU-bound work in a blocking pool does not stop the work; it only stops
waiting for it, leaving a thread burning CPU that the proper LSP needs. So the
handler contract requires polling:

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

### What abstention means on the wire

Abstention has a pleasant property here: **the shim usually does not need to
send anything at all.** The original request is still pending with the child,
so abstaining means letting the child answer it, which is exactly the
status quo the metric compares against. No null response, no special case.

The exception is `Unresponsive` and `Dead`, where nothing is coming. There the
shim must answer explicitly with `null` so the request does not hang forever.
This is the only place the shim manufactures an empty answer.

## 10. Divergence reporting

When the shim answered heuristically and the child's later answer differs, the
readme calls for notifying the user. Base LSP has no server-initiated
navigation, but LSP 3.16's `window/showDocument` is exactly the right tool:
it asks the client to display a location, with an optional selection range.

Sequence, when the client advertised `window.showDocument`:

1. `window/showMessageRequest` — "Heuristic jump was wrong. Go to the actual
   definition?" with an action.
2. On acceptance, `window/showDocument` with the child's location and
   `takeFocus: true`.

When `showDocument` is unavailable, degrade to a plain `window/showMessage`
naming the correct location. When `showMessage` is also unavailable, log only.

Two rules keep this from becoming an irritant:

* **Report even when the user has moved on.** Late arrival is not a reason to
  suppress. The point of the report is not just to offer a correction to
  navigate to — it is to tell the user that something they were shown was
  wrong. By the time the proper answer arrives they may have read the wrong
  function, reasoned about it, or edited based on it, and staying silent
  leaves that false belief in place. Since the proper LSP is slow exactly
  when the shim is most active, suppressing late reports would also suppress
  most of them.

  Because a report can arrive long after the jump, it must name the jump it
  refers to: the symbol queried and the location the shim sent them to, not
  just the corrected location. "Heuristic jump was wrong" is meaningless two
  minutes and several files later.
* **Do not report near-misses that are visually identical.** If the two
  locations are in the same file within a line or two, the user is already
  looking at the right place. This maps directly onto the error severity
  split in the readme: the near-miss tier is the tier not worth interrupting
  anyone about, and it should still be *recorded* for the metrics even when
  it is not *reported*.

## 11. Observability and scan mode

The driver is the only component that sees both the heuristic answer and the
proper answer, so it owns the measurement of every metric in the readme.

Each query emits one JSONL record once both answers are known (or the query is
resolved as abstained):

```json
{
  "uri": "...", "position": [12, 30], "language": "rust",
  "server_health": "Warming",
  "decision": "committed",
  "stratum": "explicitly_imported",
  "confidence": 0.94,
  "heuristic_latency_us": 8300,
  "heuristic_locations": ["..."],
  "lsp_latency_us": 4210000,
  "lsp_locations": ["..."],
  "agreement": "exact",
  "severity": null
}
```

This single record type covers coverage, precision, error severity
classification, per-stratum breakdown, latency percentiles, and the
LSP-latency value weighting. `stratum` and `confidence` are reported *by the
handler*, since only it knows which resolution path produced the answer; the
driver classifies `agreement` and `severity`, since only it has both answers.

### Scan mode

The corpus scan in the readme's development plan is the same driver in a
different skin: it needs to drive go-to-definition over every identifier in a
repository, against both the proper LSP and the handler, and write these same
records.

The architectural consequence is worth taking seriously up front: **the
"editor side" must be an interface, not hardcoded stdio.** Two
implementations, an LSP stdio transport and a batch harness that enumerates
identifiers and synthesises requests. If this is retrofitted later it means
untangling the core actor from the transport, which is exactly the kind of
surgery that gets skipped and leaves the eval harness as a divergent
reimplementation that no longer measures the real thing.

Scan mode also needs to *wait* for the proper LSP rather than race it —
ground truth requires the real answer, however long it takes — so the health
policy table must be overridable per-run.

A new underlying LSP server is used for each repo.

## 12. Handler interface

The seam this document commits to; everything behind it is out of scope here.
Per the readme, dispatch is direct — no framework, no config format that
languages must be expressed in.

This trait lives in `hj-shared`, which is deliberately *not* `hj-core`. See
[section 16](#16-workspace-layout) for why that separation matters.

### Vocabulary types

`hj-shared` newtypes the primitives rather than passing bare integers and
strings across the seam. Almost every value here is an offset, an index, or an
identifier, and those are exactly the things that silently substitute for each
other.

```rust
/// Byte offset into a document. Never a UTF-16 offset — those are
/// converted at the edge and this type is the proof.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ByteRange { pub start: ByteOffset, pub end: ByteOffset }

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
/// with percent-encoding and case rules smuggled in.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocumentUri(Url);

/// Request id as it arrived from the editor. Distinct from the shim's own
/// outgoing ids, which cannot be confused with it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EditorRequestId(NumberOrString);

/// Invariant: 0.0..=1.0, enforced by the constructor.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Confidence(f32);
```

### The trait

```rust
pub trait LanguageHandler: Send + Sync {
    /// LSP `languageId` values, for open documents.
    fn language_ids(&self) -> &'static [LanguageId];

    /// File extensions, for candidate files found by search. Closed files
    /// arrive as a bare path with no languageId attached.
    fn file_extensions(&self) -> &'static [FileExtension];

    /// The tree-sitter grammar, supplied at runtime so that `hj-core` can
    /// maintain its parse cache without depending on any grammar crate.
    fn grammar(&self) -> tree_sitter::Language;

    fn goto_definition(&self, q: &Query<'_>) -> Outcome;
}

pub struct Query<'a> {
    pub doc: &'a DocumentSnapshot,       // rope + tree, immutable
    pub position: ByteOffset,
    pub project: &'a dyn ProjectView,    // file list, roots, scoped reads
    pub deadline: &'a Deadline,
}

pub enum Outcome {
    Committed {
        locations: Vec<Location>,
        confidence: Confidence,
        stratum: Stratum,
    },
    Abstain { reason: AbstainReason, stratum: Stratum },
}
```

Notes on the shape:

* **`Outcome` is not `Result`.** Abstention is a normal, expected, frequently
  correct outcome — it is the mechanism that holds the precision floor — and
  it should not share a type with "something went wrong."
* **`Stratum` is reported on both arms**, because coverage per stratum is
  meaningless without knowing which stratum the abstentions belonged to.
* **`Confidence` exists now** even though only logged in v1, because the
  readme's future work item on marking heuristic results with a probability
  estimate needs it, and retrofitting a confidence notion into handlers that
  were written without one means revisiting every resolution path. It is a
  newtype rather than a bare `f32` so the 0.0..=1.0 invariant is checked once
  in the constructor instead of assumed at every comparison — and so that a
  confidence can never be silently swapped with a score, a threshold, or a
  latency.
* **`LanguageId` and `FileExtension` are interned, not strings.** A handler
  declares its ids as consts; the driver resolves an incoming LSP `languageId`
  against the registry and gets `Option<LanguageId>`. Unknown languages fail
  to resolve at the boundary rather than travelling inward as a string that
  matches nothing, and lookup becomes pointer comparison.
* **Handlers get a snapshot, not a lock.** `DocumentSnapshot` holds cloned
  `Rope` and `Tree` handles, both O(1) via structural sharing, so the core
  actor is never blocked by a running handler.
* **Handlers do their own disk reads through `ProjectView`**, so the driver can
  enforce the scope rules (workspace only, gitignore respected), cache reads
  within a query, and account I/O against the deadline.
* **Handlers are `Send + Sync` and re-entrant.** The same handler serves
  concurrent queries; per-query mutable state lives in locals.
* **`grammar()` is what keeps `hj-core` language-free.** The driver needs to
  parse — for the parse cache in [section 5](#5-document-state) and the
  token-span check in [section 8](#8-go-to-definition-lifecycle) — but
  `tree_sitter::Language` is a runtime value, so the grammar arrives through
  the registry rather than through a `tree-sitter-<lang>` build dependency.
  Without this, `hj-core` would have to depend on every language crate, which
  is exactly the edge the workspace layout forbids.
* **`ProjectView` is a trait, not a struct.** Handlers consume it; `hj-core`
  implements it, because the file list cache and scope rules live there.

## 13. Parallel dispatch and resource limits

Three kinds of concurrency, deliberately distinguished:

1. **Heuristic against the proper LSP.** Structural: the request is forwarded
   to the child before any heuristic work starts, so the two always overlap
   and the heuristic never delays the real answer.
2. **Across concurrent queries.** Multiple definition queries can be in flight
   — editors issue speculative requests, and scan mode issues many. Each
   dispatches to the pool with its own snapshot and deadline.
3. **Within a single query.** Fanning out across candidate files is the
   handler's business, using the pool the driver provides.

All three draw from **one bounded pool**, sized `max(1, num_cpus - 2)`.

The sizing is the point. The entire justification for having no index is not
competing with the proper LSP for CPU during its startup — and startup is
exactly when the shim is busiest. An unbounded pool would take back with
scheduling pressure everything the no-index decision was supposed to save,
and would do it precisely in the window that matters. Rayon is a reasonable
fit since handlers want data-parallel fan-out over candidate files.

Additional limits:

* **Max in-flight heuristic queries** (start at 4). Beyond that, new queries
  abstain immediately rather than queueing. Queueing cannot help under a
  250ms wall-clock deadline; it only guarantees the queued queries blow it.
* **Per-query byte budget** on files read, so one query over a pathological
  repository cannot monopolise the pool.
* **No heuristic work while `core` is behind.** If the event queue is backed
  up, forwarding takes priority. The prime invariant again.

## 14. Failure handling

| Failure | Response |
|---|---|
| Handler panics | `catch_unwind` at the dispatch boundary, treat as abstain, log, and disable that handler after repeated panics |
| Handler exceeds deadline | Drop the result, abstain, log |
| Document not in cache / unparseable | Abstain |
| Child writes a malformed frame | Log; cannot recover framing, so exit rather than corrupt the stream |
| Editor writes a malformed frame | Same |
| Child exits | [Section 7](#child-death) |
| Shim's own internal error | Fall back to pure-proxy mode for the rest of the session and log loudly |

That last row deserves emphasis. A permanent "just be a proxy" degraded mode
should be a real, tested code path with a flag that forces it on. It is the
mechanism that makes the prime invariant true rather than aspirational, and
it is also what a user needs when they are trying to work out whether the
shim is responsible for something.

## 15. Testing

* **Transparency golden tests.** Record real editor/server sessions as frame
  traces, replay them, assert every non-intercepted frame is forwarded
  byte-identically. This is the primary defence for the prime invariant and
  should run against traces from more than one editor.
* **Protocol race tests** with an injected clock and a scripted fake child, so
  the retry/answer/swallow/divergence sequences are deterministic. The
  interesting cases are all orderings: child answers between the two editor
  requests; child answers between the handler starting and finishing; cancel
  arrives after the shim answered; two spots interleaved.
* **Double-response assertion.** A test harness invariant, enforced globally
  across every protocol test: the editor side must never see two responses
  with the same id. This is the single failure mode most likely to escape
  review and most damaging in the field.
* **Position encoding property tests.** Random text with astral-plane
  characters, round-tripped UTF-8/UTF-16/byte offsets against a reference.
* **Health state machine tests** driven by synthetic signal sequences.
* **Fuzz the frame codec**, including split reads, oversized headers, and
  bogus `Content-Length`.

## 16. Workspace layout

A cargo workspace with `crates/` for our code and `vendor/` for copied-in Zed
crates, kept separate so provenance and licensing stay obvious.

```
Cargo.toml              workspace root
vendor/
  rope/                 copied from zed, GPL-3.0-or-later
  sum_tree/             copied from zed, Apache-2.0
  util/                 cut down to only what rope needs
crates/
  hj-shared/            handler trait + query/outcome types
  hj-resolve/           shared resolution utilities
  hj-lang-rust/         one crate per language
  hj-lang-python/
  hj-lang-typescript/
  hj-core/              the LSP driver
  hj-cli/               the binary
```

### The dependency graph

The shape is dictated by one rule from the outset: **`hj-core` must not depend
on any language crate.** Wiring happens in `hj-cli`.

```
             hj-shared  <-- rope, tree-sitter, lsp-types
            /    |    \
           /     |     \
  hj-resolve     |      hj-core  <-- tokio, ignore, notify, serde_json
           \     |          |
       hj-lang-* /          |
             \              |
              \             |
               +--> hj-cli <+
```

Every edge, and why:

* **`hj-shared` depends on nothing of ours.** The shared vocabulary: it holds
  `LanguageHandler`, `Query`, `Outcome`, `Stratum`, `Deadline`,
  `DocumentSnapshot`, and the `ProjectView` trait — types every other crate
  needs to talk about, and no behaviour. Its own dependencies are just `rope`,
  `tree-sitter`, and `lsp-types` for `Location`/`Url`.
* **`hj-resolve` depends on `hj-shared`.** The shared *resolution* utilities —
  search, candidate filtering, and so on. Distinct from `hj-shared` in that
  this is code languages call, not types they are described in. Out of scope
  for this document beyond its position in the graph.
* **`hj-lang-*` depend on `hj-shared` and `hj-resolve`**, plus their own
  `tree-sitter-<lang>` grammar crate. Nothing depends on them except
  `hj-cli`.
* **`hj-core` depends on `hj-shared` only.** Everything in sections 1 through 15
  lives here. It is generic over the handler set.
* **`hj-cli` depends on `hj-core` and every `hj-lang-*`.** It is the single
  place where the language list is enumerated:

```rust
fn main() -> anyhow::Result<()> {
    let registry = HandlerRegistry::new(vec![
        Arc::new(hj_lang_rust::Handler::new()),
        Arc::new(hj_lang_python::Handler::new()),
        Arc::new(hj_lang_typescript::Handler::new()),
    ]);
    hj_core::run(registry, std::env::args_os().skip(1))
}
```

### Why `hj-shared` is separate from `hj-core`

The trait could have lived in `hj-core` — languages would depend on `hj-core`,
`hj-core` would depend on no language, and the stated rule would still hold.
It is split anyway, for two reasons:

* **Compile times.** Otherwise every language crate transitively pulls in
  tokio, the codec, and the whole proxy just to implement one trait, and every
  edit to the proxy rebuilds every language crate. With ten languages that
  dominates the edit-test loop.
* **It keeps the rule honest.** With `hj-core` at the bottom of the graph,
  "handlers may as well reach into the driver for this one thing" is always
  one import away. With `hj-shared` at the bottom and `hj-core` a sibling, the
  layering violation does not typecheck.

### Adding a language

New `crates/hj-lang-<x>/` depending on `hj-shared` + `hj-resolve` + its grammar,
implementing `LanguageHandler`; then one line in `hj-cli`. Nothing else in the
workspace changes. That is the whole cost, and keeping it at that is the point
of the graph above.

### Module layout inside `hj-core`

```
crates/hj-core/src/
  lib.rs            run(), task wiring, child spawn
  codec.rs          Content-Length framing, raw frame type
  router.rs         classification, forwarding, id namespacing
  actor/
    mod.rs          the event loop, state ownership
    pending.rs      PendingQuery table, spot index, cancellation
    health.rs       ServerHealth, signals, policy table
    adapters.rs     optional per-server signal hooks
  docs/
    store.rs        Document map, didOpen/didChange application
    parse.rs        tree-sitter LRU, incremental reparse
    encoding.rs     UTF-16 / UTF-8 / byte offset conversion
  project/
    files.rs        ignore-crate walk, file list cache, watcher
    view.rs         ProjectView impl: scoped disk reads, per-query cache
  dispatch/
    pool.rs         bounded worker pool, deadline enforcement
    registry.rs     languageId / extension -> handler, grammar lookup
  report/
    diverge.rs      showMessageRequest / showDocument reporting
    trace.rs        JSONL metric records
  transport/
    mod.rs          editor-side interface
    stdio.rs        real LSP transport
    batch.rs        scan-mode harness
```

### Vendoring the Zed crates

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
`regex`, and `gpui_util` to support a text data structure. So **`vendor/util`
is cut down to only those items** — on the order of sixty lines.

The important part is that it keeps the crate name `util` and the same paths.
That way `rope`'s `use util::...` lines are untouched and **`rope` needs no
patching at all for this**, which keeps re-syncing against upstream a clean
diff rather than a merge. Trimming the dependency is strictly better than
rewriting the dependent.

`ztracing` is not vendored. Its `instrument` is either `tracing::instrument`
or a no-op passthrough depending on a cfg, and `rope` already depends on
`tracing`, so the one import is redirected there. That is a single-line patch
to `rope`, recorded as such.

`sum_tree` needs no patching. Its `tree_map.rs` is unused here and can be
dropped; a whole-file deletion still leaves a clean diff.

`vendor/README.md` records, per crate, the upstream revision it was taken at,
the exact patches applied, and — for `util` — the list of items kept, so that
a future re-sync can tell at a glance whether upstream changed anything that
matters.

**Licensing consequence, stated plainly:** `rope` is GPL-3.0-or-later, so the
resulting binary is GPL-3.0-or-later. `sum_tree` is Apache-2.0. This is a
project-level commitment that follows from vendoring, not a detail — it means
no part of this workspace can later be offered as a permissively licensed
library.
