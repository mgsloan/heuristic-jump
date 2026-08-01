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
the latency budget (p50 <= 50ms, p99 <= 400ms, hard cap 750ms), the >=97%
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

* The forwarding path never depends on heuristic state. Handler panics, blown
  deadlines, a wedged `core`, and unparseable documents all resolve to
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

Five long-lived tasks plus a worker pool, communicating only over channels.
There is no shared mutable state and no lock anywhere in the design.

```
  editor stdin --> [reader:editor] --+--> (to-child) --> [writer:child]
                                     |
                                     +--> [core] -- owns ALL state
                                            ^  |
  child stdout --> [reader:child] ----------+  |
                          |                    v
                          |              [worker pool]
                          |                    |
                          +--> (to-editor) <---+--> [writer:editor]
```

* **`reader:editor`** parses frames from the editor. For each frame it pushes
  the raw bytes to `to-child` **first**, then sends a classified event to
  `core`. Forwarding never waits on `core`.
* **`reader:child`** the same in the other direction.
* **`writer:editor`** / **`writer:child`** each own one pipe exclusively.
  Not optional: frames must not interleave, so exactly one task may ever write
  to a given fd and everything else reaches it through an mpsc channel.
* **`core`** is a single-threaded actor owning documents, the parse cache,
  pending queries, and health. It processes one ordered event stream and
  performs **only O(1) state transitions**. It never parses, never searches,
  never touches the filesystem.
* **Worker pool** runs handlers against immutable snapshots handed to them at
  dispatch. Workers own nothing shared; results return to `core` as events.

All channels are unbounded. A bounded channel would eventually make a reader
wait, and a stalled reader stops forwarding — the one thing
[section 1](#1-the-prime-invariant) forbids. Memory is bounded instead by the
shed-load rule in
[section 13](#13-parallel-dispatch-and-resource-limits), which drops heuristic
work rather than protocol traffic.

### Why an actor

**LSP message order defines document state.** A `textDocument/definition` must
be evaluated against the document as of every `didChange` that preceded it and
none that followed. A single ordered event queue makes that structural; a
lock-based design makes it accidental, and the resulting bugs are timing
dependent and unreproducible.

The decisive property is **snapshot-on-dispatch**. When `core` processes a
definition event it clones the document state *at that instant* and hands the
clone to a worker. The worker is then immune to everything that happens next:
a `didChange` arriving a microsecond later cannot change what it sees. With
shared state the snapshot would instead be taken whenever the worker happened
to acquire the lock, so a query could silently end up answering about text the
user had already edited — resolving whatever moved into the requested offset
rather than what was there when they asked.

### Which version a query runs against

**LSP requests do not carry a document version.** `textDocument/definition`
takes a `TextDocumentIdentifier`, which is a bare URI —
`VersionedTextDocumentIdentifier` appears only on `didChange`. The version a
query means is implicit: it is the state after every notification the client
sent before it, guaranteed by LSP's in-order processing.

That implicit version is exactly what the ordered event queue makes explicit.
When `core` reaches a definition event, `documents[uri].version` *is* the
version the user was looking at when they pressed the key — no inference, no
staleness check, no window. This is the concrete payoff of the actor, and it
is why "which version?" has an answer at all.

### Snapshots are O(1)

Snapshot-on-dispatch is only viable because nothing is copied:

```rust
pub struct DocumentSnapshot {
    pub text: Rope,                  // structural sharing; O(1)
    pub version: DocumentVersion,    // the version above
    pub language_id: LanguageId,
    /// Cached tree at some older version, plus the edits that bring it
    /// up to `version`. Never handed to handlers directly.
    base: Option<(Tree, Arc<[InputEdit]>)>,
    grammar: tree_sitter::Language,
    parsed: OnceCell<Tree>,
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

So a snapshot is two refcount bumps and a struct move, regardless of file size.

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
paid inside the worker and inside the deadline, never in `core`. When the
worker finishes, a filled `parsed` cell is returned to `core` as a
`Parsed { uri, version, tree }` event, so the next query starts warm.

### `core` never mutates a shared tree

Cached trees are **immutable once inserted**. On `didChange`, `core` applies
the edit to the `Rope` and appends the `InputEdit` to the document's
`edits_since_parse`; it does *not* call `Tree::edit` on the cached tree. A
worker doing an incremental reparse applies those edits to **its own private
clone**, then reparses against it and sends the new tree back to `core` to
cache.

`ts_subtree_edit` does copy-on-write via `ts_subtree_make_mut`
(`src/subtree.c:688`), so editing a shared tree would in fact be safe. This
design avoids doing it anyway, for two reasons: correctness stops depending on
a C library internal that could change, and `core` avoids paying copy-on-write
along the edit path on every keystroke for a tree shared with N workers.

### Cache misses do not block `core`

`core` never parses, so a definition event that misses the parse cache is
dispatched with `tree: None`. Two consequences, both benign:

* The `Spot` for that query cannot be widened to a token, so it falls back to
  exact-offset identity ([section 8](#8-go-to-definition-lifecycle)).
* The worker parses for its own use, then returns the tree to `core` as a
  `Parsed { uri, version, tree }` event to be cached.

This gives the retry protocol exactly what it needs: the first query at a spot
warms the cache, so by the time a user retries, the tree is present and the
retry's `Spot` *can* be widened.

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
| **Response to a child-originated request** | **Forward verbatim** — see below |
| everything else | Forward verbatim |

**Child to editor:**

| Message | Action |
|---|---|
| Response to an id the shim already answered | **Swallow**, hand to `core` for divergence check |
| Response to any other id | Record latency in `core`, forward |
| `$/progress` | Tee to `core` (adapters), forward |
| `InitializeResult` | Inspect (sync kind, encoding, capabilities), forward |
| **Server-originated request** (`workspace/configuration`, `client/registerCapability`, `window/workDoneProgress/create`, `workspace/applyEdit`, …) | **Forward verbatim** — see below |
| everything else | Forward verbatim |

The swallow case is the one that must not be got wrong. Once the shim has
answered request `id` on the editor's behalf, the proper server's eventual
response to `id` **must not** reach the editor, or the editor sees two
responses to one request. That is a protocol violation, and clients react to
it badly, ranging from a log warning to a stuck request slot.

### Server-originated requests are load-bearing

LSP is bidirectional: the server sends requests *to* the client and waits for
answers. `workspace/configuration` is how rust-analyzer fetches its settings;
`client/registerCapability` is how it registers file watching; there is also
`workspace/applyEdit` and the various `refresh` requests. These flow
child → editor, and the editor's responses flow back editor → child.

Both directions are pure passthrough here, and they are called out explicitly
rather than left to "everything else" because this is the documented way a
comparable proxy broke. `lspmux` — a multiplexer letting several editors share
one server — remaps ids to route responses, and its README states that because
not all messages can be tracked that way it "drops some, notably it drops any
requests from the server," warning users to report "issues which are
definitely not present in the language server alone."

That is precisely the class of failure
[section 1](#1-the-prime-invariant) exists to prevent: a proxy-induced bug
that presents as the language server misbehaving. The shim avoids it by having
no reason to touch these messages at all — see the id namespacing below — but
"we never had a reason to break it" is not a guarantee, so it is a test
([section 15](#15-testing)) rather than an assumption.

### Request id namespacing

The shim may need to originate its own requests to the editor
(`window/showMessageRequest`, `window/showDocument`). LSP permits string ids,
so all shim-originated requests use `"hj-<random>-<counter>"`, the random
component fixed per process so an editor that happens to use string ids
cannot collide. The shim originates no requests to the child, so the child's
id space is untouched entirely.

**No id remapping table.** Ids pass through in both directions unchanged; the
shim only needs to *recognise* the small set it has answered itself, and its
own outgoing ids are self-identifying. This is affordable only because the
shim is strictly 1:1 — one editor, one child. `lspmux` multiplexes several
clients onto one server and therefore has no choice but to rewrite ids, which
is where its dropped-message limitation comes from: once you are rewriting,
every message shape has to be understood, and the ones you cannot track get
discarded.

Two future changes would forfeit this. Supervising and restarting the child
(future question 3 in `readme.md`) means the editor's ids outlive the child's
id space, so post-restart ids need remapping. Multiplexing would mean it too.
Neither is planned; both should be understood as giving up a property, not
just adding a feature.

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

`core` owns a `HashMap<DocumentUri, Document>`. No lock: it is actor-internal
state, reached only through the event queue.

```rust
struct Document {
    text: Rope,                   // vendored zed rope, utf16-aware
    version: DocumentVersion,     // from didChange
    language_id: LanguageId,      // from didOpen
    /// Edits applied since the cached tree was parsed. Handed to workers
    /// for incremental reparse; cleared when a fresh tree comes back.
    edits_since_parse: Vec<InputEdit>,
}
```

The vocabulary types are defined in
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

Also owned by `core`: an LRU of tree-sitter trees, bounded by both entry count
and total bytes (a single generated file can be enormous). Keys:

* Open documents: `(uri, version_parsed_at)`.
* Disk files: `(path, mtime, len)`. `didSave` lets the disk entry take over.

Because `core` only ever *gets* and *puts* refcounted handles, holding the
cache costs it nothing — the expensive half, parsing, happens in workers, and
the results arrive as `Parsed` events.

**Entries are immutable once inserted**, per
[section 2](#core-never-mutates-a-shared-tree). A tree cached at v3 stays a v3
tree forever; it is superseded, never edited in place.

**Staleness is explicit, not invalidation.** A `didChange` bumps the document
version but leaves the v3 tree cached and appends to `edits_since_parse`. A
dispatch at v5 therefore carries the v3 tree plus two edits, which is exactly
the input an incremental reparse needs. Nothing is ever invalidated, and there
is no window in which a stale tree is mistaken for a fresh one — the version
travels with it.

**Returned trees consume a prefix of the edit log, never all of it.** A worker
that was handed the v3 tree plus edits `[e4, e5]` returns a tree for v5. By
then the log may hold `[e4, e5, e6, e7]`, because the user kept typing while
the worker ran. `core` must drop only the edits the returned tree already
accounts for — the prefix up to v5 — and keep `[e6, e7]` for the next
reparse. Clearing the whole log here would silently lose two edits and every
subsequent incremental reparse would be built on a wrong base, producing a
tree that disagrees with the text in ways no test of a single edit would
catch.

Concretely: the log is stored alongside the version it starts from, a
returned tree carries the version it was parsed at, and consuming the prefix
is a comparison of the two. A tree returned for a version *older* than the
cache's current entry is simply dropped — a slow worker lost the race, and
its result is stale rather than wrong.

The cache is a cache, not an index: cold misses are correct, just slower.
Nothing may depend on an entry being present — which is exactly why
[section 8](#8-go-to-definition-lifecycle) treats a missing tree as "no token
widening available" rather than as a reason to parse.

## 6. Project file enumeration

Whole-project search needs a file list, and this is the one place where "no
index" needs a stated boundary.

There is no *symbol* index — no persisted map from name to definition sites,
which is the thing the readme rules out and the thing that would cost
startup CPU and invalidation complexity. But a cold directory walk of a large
repository can take hundreds of milliseconds on its own, which would consume
the entire budget before a single file is read. So:

**The driver caches a file list. It does not cache anything about file
contents beyond the parse LRU.**

* Built with the `ignore` crate, the same walker ripgrep uses, so `.gitignore`
  is respected for free. This directly implements the readme's decision that
  gitignored files are out of scope.
* Built in-process rather than by shelling out to ripgrep: subprocess spawn
  plus pipe overhead is a meaningful fraction of a 50ms p50 target, and
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
}
```

Signals, in order of reliability:

1. **`InitializeResult` received.** `Starting` to `Warming`.
2. **Response latency**, tracked as a rolling distribution over *all*
   request kinds, not just definitions. `Slow` is defined relative to the
   server's own recent baseline rather than an absolute threshold, because
   the absolute numbers differ by orders of magnitude between language
   servers and between repository sizes.
3. **Silence with work outstanding.** Requests pending beyond a threshold with
   no frames of any kind arriving moves to `Unresponsive`.

Note what is *not* on that list: `$/progress`. It looks like the obvious
generic signal for `Warming`, and it is a trap. rust-analyzer emits progress
for indexing but also for every `cargo check` and flycheck run, so "any
outstanding progress token means warming" marks the server as warming
more or less continuously during ordinary editing. That would make eager
answering — meant for cold start — fire all day, inverting the tool's whole
risk profile. Nothing generic can distinguish "still starting up" from
"running a background check," because the distinction lives entirely in the
work-done title and token, which are server-specific strings.

### Per-server adapters

So interpreting progress, and any other server-specific signal, is the job of
an adapter:

```rust
pub trait ServerAdapter: Send + Sync {
    /// Matched against `serverInfo.name` from InitializeResult.
    fn server_name(&self) -> &'static str;

    /// Interpret a server-specific notification. Returning None means
    /// "no opinion" and leaves health to the generic signals.
    fn observe(&self, msg: &IncomingMessage) -> Option<HealthSignal>;
}
```

Implementations live in `hj-core` beside the driver, one per language server —
`rust_analyzer.rs` reading `experimental/serverStatus` and recognising the
indexing progress title, `pyright.rs`, `gopls.rs`, and so on. This is a driver
concern rather than a language concern: it is about a specific *server
process*, not about a language's syntax, so it stays here rather than behind
the handler interface. Two servers for the same language can want different
adapters.

### The generic warming signal

Adapters are for precision, but the tool cannot depend on one existing — an
unrecognised server would otherwise never reach `Warming`, and eager
answering, the main payoff of modelling health at all, would be dark by
default.

So there is one generic rule, and it needs no server knowledge:

> **The child is `Warming` until it has successfully answered its first
> `textDocument/definition` request.**

If a language server has not yet answered a single go-to-definition, it is
plausibly still indexing. The moment it answers one, health becomes `Ready`
and the retry rule takes over for the rest of the session.

This calibrates itself across every server, repository size, and machine,
with no timer to tune and nothing to configure. It also fails in the safe
direction: a server that is genuinely fast answers its first definition in
milliseconds, so the eager window closes almost immediately and costs at most
a handful of early queries — which are cheap, because the child's answer
still arrives and any divergence is still reported.

Note this makes `Starting` a state that exists for logging and never gates
policy: no definition request can arrive before `InitializeResult`, because
the editor waits for the `initialize` response before sending anything else.

### What health is for

Health selects the answering policy:

| Health | Policy |
|---|---|
| `Starting`, `Warming` | **Eager.** Answer heuristically on the first request. |
| `Ready` | **Retry-triggered.** Wait; answer only on a repeat query. |
| `Slow` | **Retry-triggered.** |
| `Unresponsive` | **Eager**, and answer an error rather than abstaining. |

The eager rows follow directly from modelling health rather than just
request-pendency, and they are most of the practical value of doing so. If the
server is provably still indexing, making the user press go-to-definition
twice to discover what the shim already knows is pure friction. Conversely
when the server is `Ready`, waiting costs almost nothing and the proper answer
arrives; the readme's retry rule handles the occasional slow query.

The `Unresponsive` row differs in its abstention behaviour and is explained
in [section 9](#9-deadlines-and-abstention).

There is no `Dead` state. Child exit means the shim exits too (see below), so
there is no interval worth modelling in which the child is gone and the shim
is still answering.

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

/// Query identity for the repeat check.
#[derive(Clone)]
struct Spot {
    uri: DocumentUri,
    at: ByteOffset,
    /// Present once a parse is available to widen the offset to a token.
    token: Option<ByteRange>,
}

impl Spot {
    /// Deliberately NOT `PartialEq`. Repeat-ness is not equality: it is
    /// asymmetric in what it ignores, and the rules below are all cases
    /// where a derived comparison would give the wrong answer.
    fn is_repeat_of(&self, prior: &Spot) -> bool {
        if self.uri != prior.uri {
            return false;
        }
        match (self.token, prior.token) {
            // Anywhere in the same token is the same question. A user
            // re-triggering may land a character off.
            (Some(a), Some(b)) => a.overlaps(b),
            // One side was recorded before a parse existed. Widen using
            // whichever token we do have.
            (Some(a), None) => a.contains(prior.at),
            (None, Some(b)) => b.contains(self.at),
            // No parse on either side; exact offset is all there is.
            (None, None) => self.at == prior.at,
        }
    }
}
```

`core` holds these keyed by `editor_id`, plus a list scanned by
`is_repeat_of`. The list is short — only queries still pending — so a linear
scan beats indexing on something that has no equality relation. When more
than one pending query matches, the **most recent** wins, so the result never
depends on scan order.

### Spots are anchors, not stored offsets

`is_repeat_of` ignores versions, which is what makes a retry survive a
formatter or a stray keystroke between the two presses. But byte offsets are
meaningless across versions — offset 100 in v3 and offset 100 in v5 are
different positions — so comparing a stored offset against a later one would
be comparing coordinates from two different documents.

The fix is to keep pending spots in *current* coordinates. On every
`didChange`, `core` walks its pending queries and translates each `Spot`
through the edit:

* Edit entirely after the spot — unchanged.
* Edit entirely before it — shift `at` and `token` by the length delta.
* **Edit overlapping the token — invalidate the spot.** If the identifier
  itself changed, the user is no longer asking the same question, and a
  later press there is a new query rather than a retry.

This is the anchor pattern editors use for markers, and it is `core`'s kind
of work: a short loop of arithmetic per edit. With it, every `Spot` in the
pending list is expressed against the current document, and `is_repeat_of`
compares like with like.

### Widening only when the tree is current

`core` widens an offset to its enclosing token using the cached tree — but
that tree may be older than the document, and its node boundaries would then
be wrong for the current text.

So the rule is: **widen only when `edits_since_parse` is empty**, meaning the
cached tree matches the current text exactly. Otherwise the `Spot` keeps
`token: None` and relies on offset identity, which the anchoring above keeps
valid. `core` never reparses to widen — that would be expensive work in the
one place this design forbids it.

In practice the tree is current whenever the user is not mid-keystroke, which
is when they are pressing go-to-definition. The `(Some, None)` arm of
`is_repeat_of` covers the rest: the first request at a spot often arrives
before any parse exists, and the worker's parse lands before the retry comes
in, so a widened retry is compared against an un-widened original.

### Flow

1. **Request arrives.** Forward to the child immediately (never gate
   forwarding on shim work). Record a `PendingQuery`.
2. **Determine the spot.** Build a `Spot`, widening the offset to its
   enclosing identifier token if the parse cache already holds a tree for
   this document — a lookup only, never a parse (see
   [section 5](#5-document-state)). Compare against pending queries with
   `is_repeat_of`.
3. **Check the policy.** If health says eager, or this is a repeat of a spot
   with a still-pending query, dispatch to the handler. Otherwise do nothing
   and let the child answer.
4. **Handler returns.** On `Committed`, answer *every* pending query at that
   spot — the repeat and the original — as the readme specifies, and mark
   each `answered_by_shim`. On `Abstain`, see [section 9](#9-deadlines-and-abstention).
5. **Child responds later.** The response is swallowed. If the query was
   `answered_by_shim`, compare and possibly report divergence
   ([section 10](#10-divergence-reporting)). Either way, log the pair for the
   metrics ([section 11](#11-observability-and-the-corpus-scan)).

### Cancellation

`$/cancelRequest` is forwarded and also drops the `PendingQuery` and signals
the handler's cancellation token. The shim must not answer a cancelled
request. If the shim had already answered before the cancel arrived, the
cancel is forwarded anyway and the child's response is swallowed as usual.

Note that the shim can also *receive* a stale cancel for a request it already
answered; this is harmless and must not be treated as an error.

## 9. Deadlines and abstention

The 750ms hard cap is enforced by the driver, not trusted to the handler.

**The deadline is absolute and starts at request arrival**, not at handler
entry. Queueing time counts. A handler that gets 750ms of wall clock but
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

The exception is `Unresponsive`, where nothing is coming and the request would
otherwise hang forever. There the shim answers with a `RequestFailed` error
rather than `null`.

The distinction matters to the user. `null` is a definite statement — "this
symbol has no definition" — which editors render as a flat "no definition
found", and it is a claim the shim has no basis for making. An error says the
request could not be served, which is both true and something clients surface
as a transient failure rather than an answer.

## 10. Divergence reporting

### The agreement predicate

Before anything can be reported — or measured — "different" needs a
definition, and it cannot be range equality. The proper LSP points at the
identifier in a definition; a dumb-jump style match may point at the start of
the line, at the `fn` keyword, or at a whole item. Exact comparison would
report a divergence on nearly every *correct* answer.

This predicate is not a reporting detail. It *is* the precision metric in
`readme.md`, so getting it wrong makes the headline number meaningless.

Both sides are first normalized: `textDocument/definition` may answer with
`Location`, `Location[]`, `LocationLink[]`, or null, and which one depends on
whether the client advertised `linkSupport`. All shapes collapse to a set of
`(uri, range)` before comparison, taking `targetSelectionRange` for links.

Then, comparing the shim's answer against the child's:

| Relation | Classification |
|---|---|
| Same file, ranges overlap | **Match** |
| Same file, within 3 lines | **Match** |
| Same file, more than 3 lines apart | Error, recoverable tier |
| Different file, same module tree | Error, moderate tier |
| Different file, unrelated | Error, trust-destroying tier |
| Child answered null or empty, shim committed | Error, treated as unrelated |
| Both empty | Match |

The 3-line tolerance is deliberate: at that distance the correct definition is
on screen and the user is already reading it, so scoring it as wrong would
measure something nobody experiences as wrong. The tiers below it map onto the
error severity budgets in `readme.md`.

When the child returns multiple locations, agreement means the shim's answer
matches *any* of them — the LSP itself is expressing ambiguity there, so
picking one of its candidates is not an error.

### Reporting

When the shim answered heuristically and the child's later answer does not
match, the readme calls for notifying the user. Base LSP has no
server-initiated navigation, but LSP 3.16's `window/showDocument` is exactly
the right tool: it asks the client to display a location, with an optional
selection range.

Sequence, when the client advertised `window.showDocument`:

1. `window/showMessageRequest` — "Heuristic jump was wrong. Go to the actual
   definition?" with an action.
2. On acceptance, `window/showDocument` with the child's location and
   `takeFocus: true`.

When `showDocument` is unavailable, degrade to a plain `window/showMessage`
naming the correct location. When `showMessage` is also unavailable, log only.

### Rate limiting

Reports fire on any non-`Match` classification, and eager answering means
divergences arrive in bursts — a cold start is exactly when the shim answers
most and is least accurate. One prompt per divergence would be unusable.

So reports are batched rather than emitted per divergence:

* A short window collects divergences. The first one opens it.
* Entries are the **identifiers queried**, not locations — the user thinks in
  terms of "I looked up `parse_config`", not in terms of ranges.
* Duplicates collapse. Looking up the same symbol three times during a cold
  start is one entry.
* The window closes and emits a single message listing the identifiers, up to
  a character budget. Past that it truncates with a count: `parse_config,
  Resolver, TokenKind and 4 more were resolved incorrectly`.
* While a report is outstanding, the window stays open, so a slow user
  response cannot be overtaken by a second prompt.

The batched form loses the ability to offer `window/showDocument` navigation,
since there is no single correct location to jump to. That is the right
trade: a batch means several answers were wrong, and at that point the useful
message is "distrust what you were shown," not "here is one correction." A
single divergence still gets the full prompt-and-navigate treatment.

Every divergence is recorded for the metrics whether or not it appears in a
report — rate limiting is a UI concern and must not reach the numbers.

One further rule:

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


## 11. Observability and the corpus scan

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

### The corpus scan is a separate program

The scan in the readme's development plan is **not** a mode of the shim. It is
its own binary, `hj-scan`, and `hj-core` has no batch path, no transport
abstraction, and no awareness that it exists.

The requirements are opposed at nearly every point:

| | `hj-core` | `hj-scan` |
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
weaker than it looks: **what the scan measures is the handler, not the
driver.** The proxy, the health model, and the retry protocol are not under
test — resolution accuracy is. So as long as `hj-scan` builds its `Query` and
`DocumentSnapshot` the same way, the code under test is genuinely identical.
Snapshot construction therefore lives in `hj-shared`, which makes that
structural rather than a matter of discipline.

`hj-scan` spawns a fresh language server per repository, opens documents,
enumerates identifiers with the handler's own grammar, asks both sides, and
writes the records above. The one thing it shares with the shim beyond
`hj-shared` is the agreement predicate from
[section 10](#10-divergence-reporting) — the definition of "match" must not
fork, or the shipped metric and the measured metric stop being the same
number.

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
  `Rope` and `Tree` handles, both O(1), taken at dispatch — so a handler is
  immune to edits that arrive while it runs, and `core` is never blocked.
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
  750ms wall-clock deadline; it only guarantees the queued queries blow it.
* **Per-query byte budget** on files read, so one query over a pathological
  repository cannot monopolise the pool.
* **No heuristic work while `core` is behind.** If the event queue is backed
  up, forwarding and state transitions take priority. The prime invariant
  again.

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
  should run against traces from more than one editor. The only frames
  exempted are the ones [section 4](#4-initialize-and-capability-negotiation)
  deliberately rewrites — `initialize`, where `positionEncodings` may be
  reordered — and the definition responses the shim answers itself. Everything
  else, including `initialized` and `InitializeResult`, must match byte for
  byte.
* **Server-originated request round-trips.** A dedicated case: the scripted
  child sends `workspace/configuration`, `client/registerCapability`,
  `window/workDoneProgress/create`, and `workspace/applyEdit`; the test
  asserts each reaches the editor unchanged and each editor response reaches
  the child unchanged. This is the failure `lspmux` shipped with
  ([section 3](#3-message-routing)), and it is invisible in any test that only
  exercises client-initiated traffic — which is what most LSP test harnesses
  do.
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
  hj-cli/               the shim binary
  hj-scan/              the corpus scan binary (see section 11)
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

* **`hj-scan` depends on `hj-shared` and every `hj-lang-*`** — but *not* on
  `hj-core`. It is an LSP client, not a proxy, so none of the driver applies
  to it.

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
    mod.rs          the event loop, state ownership, snapshot-on-dispatch
    documents.rs    Document map, didOpen/didChange application
    pending.rs      PendingQuery table, is_repeat_of scan, cancellation
    health.rs       ServerHealth, generic signals, policy table
    adapters/
      mod.rs        ServerAdapter trait, name -> adapter lookup
      rust_analyzer.rs
      pyright.rs
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
    stdio.rs        LSP framing over stdin/stdout
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
