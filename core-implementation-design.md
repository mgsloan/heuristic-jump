# Core implementation design

This covers the core driver only:

* The LSP shim/proxy that sits between the editor and the proper language
  server.
* Knowing the current state of the project's files.
* Dispatching go-to-definition calls to the language-specific handler, in
  parallel with the proper LSP and with each other.

Sections 1 through 16 describe **proxy mode**, which is the primary mode and
the one the metrics are defined against.
[Section 17](#17-standalone-mode) describes **standalone mode**, where there is
no proper language server at all and the shim is the whole language server.
Standalone is a policy variation on the same driver, not a second
implementation, and section 17 states exactly which of the rules below it
changes.

Out of scope for this document: the resolution logic itself, and the shared
utilities that resolution logic is built from. Those sit behind the handler
interface described in [Handler interface](#12-handler-interface), which is the
only part of them this document commits to.

See `readme.md` for the product rationale and the success metrics. The
metrics constrain this design in three concrete ways, noted where they bite:
the latency budget (p50 <= 50ms, p99 <= 400ms, hard cap 750ms), the abstention
path, and the per-stratum reporting requirement.

**On precision.** `readme.md` places its >=97% precision floor in future work.
v1 answers whenever the handler has a candidate and **measures** precision
rather than gating on it; the headline number is plain coverage. The
difference shows up in three places — abstention exists for *latency and emptiness*, not for low
confidence ([section 9](#9-deadlines-and-abstention)); divergence reporting
([section 10](#10-divergence-reporting)) is the only thing protecting the
user, so it carries more weight here than it would under a floor; and
`Confidence` is recorded but never compared against a threshold
([section 12](#12-handler-interface)).

Nothing in the architecture assumes the floor is absent. Reinstating it is a
policy change at the commit decision plus a threshold table, both of which sit
behind the handler seam. The plumbing that would feed it — per-stratum
records, the agreement predicate, confidence on every answer — is built now
precisely so the floor can later be set from measurements instead of guesses.

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
  without being deserialized into a typed struct and re-serialized. A typed
  round-trip is lossy for unknown extensions, and both rust-analyzer and Zed
  use custom methods. This holds so strongly that the shim's own protocol types
  ([section 18](#18-protocol-types)) are read-only projections with no
  round-trip capability at all — a field we did not model cannot be dropped,
  because nothing ever writes one back.
* The shim adds at most one message-copy of latency to the forwarding path.
  Bookkeeping happens after the bytes are on their way, never before. In
  practice it comes in under that ceiling: a forwarded frame is allocated once
  as an `Arc<[u8]>`, never copied, and in the steady state never inspected
  either. [Section 3.1](#31-how-little-inspection-the-forwarding-path-needs)
  works out why that is achievable, which is less obvious than it sounds.

A useful implication: the shim should be tested by recording a real editor
session, replaying it, and asserting that every forwarded frame is identical
except the ones deliberately intercepted. See [Testing](#15-testing).

In standalone mode there is no forwarding path, so this invariant is vacuous
and a different one takes its place. See
[section 17.2](#172-the-standalone-invariant).

## 2. Process and transport model

`heuristic-jump -- rust-analyzer --some-flag` treats everything after `--` as
the command line of the proper server. The separator is **required**: it is
POSIX's own stop-parsing marker, so the child's arguments — `--help` and
`--version` among them — reach it byte-for-byte with no argument parser
guessing where our flags end and its begin. `dependency-plan.md` §11 has the
flag list and the verification that this holds. The shim:

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

**Dependencies.** `crossbeam-channel` for the channels, `rayon` for the worker
pool, `serde` and `serde_json` for the messages the shim actually inspects
(against our own protocol types — [section 18](#18-protocol-types) — not
`lsp-types`), `url` for URI normalization, Zed's `rope` (vendored, see
[section 16](#16-workspace-layout)) for document text, `tree-sitter` for
parses, `ignore` for file walking, `notify` for the optional watcher.
Deliberately not a framework: the shim's whole job is to be a thin,
predictable pipe. `dependency-plan.md` settles each of these and records what
was rejected.

**No async runtime.** Every thread below is either a blocking read or write on
one fd, a serial channel loop, or CPU-bound parsing — none of which an executor
improves. Deadlines are already cooperative rather than timer-driven
([section 9](#9-deadlines-and-abstention)), the file walker and the watcher are
thread-based, and a scheduler between our bytes and the pipe is the opposite of
the prime invariant. The structure below maps onto async tasks mechanically if
child supervision (`readme.md` future question 7) ever makes one worthwhile.

### Thread layout

Six long-lived threads plus a worker pool, communicating only over channels.
There is no shared mutable state and no lock anywhere in the design. Three of
the six are absent in standalone mode, where there is no child
([section 17.4](#174-health-policy-and-what-disappears)).

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

* **`reader:editor`** splits the editor's byte stream into frames. For each
  frame it pushes the bytes to `to-child` **first**, then sends the same
  `Arc<[u8]>` to `core` to be classified there. Forwarding never waits on
  `core`, and it inspects nothing at all — the shim modifies no frame it
  forwards, so there is nothing to decide before the bytes move. See
  [section 3.1](#31-how-little-inspection-the-forwarding-path-needs).
* **`reader:child`** the same in the other direction, and it inspects nothing
  ever: the one child → editor decision that must precede forwarding belongs to
  `writer:editor`
  ([section 3.2](#32-the-swallow-decision-belongs-to-writereditor)).
* **`writer:editor`** / **`writer:child`** each own one pipe exclusively.
  Not optional: frames must not interleave, so exactly one thread may ever write
  to a given fd and everything else reaches it through a channel.
  `writer:editor` additionally owns the set of ids it has already responded to,
  which is what makes a double response structurally impossible rather than
  merely avoided
  ([section 3.2](#32-the-swallow-decision-belongs-to-writereditor)).

  `writer:child` looks redundant — the shim originates nothing to the child
  ([section 3](#request-id-namespacing)), so `reader:editor` is its only
  source and could write the fd directly. It exists anyway, because a child
  that has stopped reading its stdin would then block `reader:editor`, which
  would stop the tee to `core`, which would stop document tracking and
  therefore stop heuristic answers. Serving heuristics while the child is
  wedged is the `Unresponsive` case in [section 7](#what-health-is-for) — the
  scenario the tool exists for. The unbounded channel is what decouples the
  two, and this thread is not an abstraction to optimize away.
* **`core`** is a single-threaded actor owning documents, the parse cache,
  pending queries, and health. It processes one ordered event stream and
  performs **only O(1) state transitions**. It never parses, never searches,
  never touches the filesystem. Its loop is a `select!` over the event channel
  and a timer, the latter driving the report window
  ([section 10](#10-divergence-reporting)) and the rescan debounce
  ([section 6](#6-project-file-enumeration)).
* **`stderr:child`** copies the child's stderr to ours, unmodified. It appears
  in none of the diagrams because it touches nothing else in the system — no
  channel, no state — but it is a thread and it is the sixth.
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

**`parsed` must be the thread-safe cell.** Handlers may fan out across
candidate files ([section 13](#13-parallel-dispatch-and-resource-limits)),
which means `&Query` — and therefore `&DocumentSnapshot` — crosses threads,
which requires `DocumentSnapshot: Sync`. So `parsed` is a
`std::sync::OnceLock<Tree>`, not the unsync variant. This works because
tree-sitter declares `Tree` both `Send` and `Sync`
(`binding_rust/lib.rs:3908`); `Node<'tree>` is likewise `Sync`, so nodes
borrowed from a shared tree can be passed between fan-out workers.

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
responses. [Section 3.1](#31-how-little-inspection-the-forwarding-path-needs)
establishes that in the steady state this classification happens *after* the
bytes have been forwarded, and costs the forwarding path nothing; read the
tables below as describing where each frame ends up, not as work done before
it moves.

**Editor to child:**

| Message | Action |
|---|---|
| `initialize` | Forward, then inspect (root, capabilities). Never modified — see [section 4](#position-encoding) |
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
[Section 3.2](#32-the-swallow-decision-belongs-to-writereditor) makes that
structurally impossible rather than protocol-tracked.

### 3.1 How little inspection the forwarding path needs

The tables above look like they describe per-frame work before forwarding.
They do not, and the difference matters: LSP has some very large frames — a
`didChange` under full sync, a completion response with thousands of items, a
`workspace/didChangeWatchedFiles` listing a whole build tree — and any
inspection proportional to frame size lands directly in the budget
[section 1](#1-the-prime-invariant) sets at one message-copy.

Start from the question of what actually has to be decided *before* bytes move,
since bytes cannot be unsent. In proxy mode there is exactly **one** such
decision in the entire protocol: **dropping a response the shim already
answered**, which
[section 3.2](#32-the-swallow-decision-belongs-to-writereditor) relocates to
the one component that can make it race-free.

That is the whole list; the editor -> child direction has no pre-forward
decision at all, because the shim modifies nothing it forwards
([section 4](#position-encoding)). Every other classification — which document changed,
which health signal arrived, what latency to record — exists to update `core`,
and `core` is downstream of the wire. It can be done after the frame is on its
way, on the same buffer, with no effect on what the editor or the child
experiences.

So the rule is stronger than "forward first, then classify." It is:

> In the steady state, a forwarded frame is not inspected at all. It is read,
> its `Content-Length` is parsed, and it is handed to a writer and to `core`
> as a shared `Arc<[u8]>`.

Both directions reach that state, by different routes:

* **Editor → child.** Nothing is ever decided pre-forward, so nothing is ever
  inspected pre-forward. Every frame from the editor, `initialize` included,
  is read and handed straight to `writer:child` and to `core`. Not "scans
  cheaply" — does not scan, ever, for the whole session.
* **Child → editor.** The sole pre-forward case is the swallow, and
  [section 3.2](#32-the-swallow-decision-belongs-to-writereditor) moves it to
  `writer:editor`, which owns the set and can therefore check whether it is
  empty with a local field read. It is empty whenever the shim has no
  outstanding answer of its own, which is almost always — the shim answers at
  most a handful of definition requests per session, and each entry lives only
  until the child's response to that id arrives.

**No copies, either.** The frame is allocated once by the reader as an
`Arc<[u8]>`; the writer derefs it to `&[u8]` and the `core` event holds a
second handle. Section 1's "at most one message-copy" is a ceiling this comes
in under.

### Deserialization happens in `core`, and only for the frames it wants

`core` inspects perhaps eight message kinds out of everything that crosses.
For those it needs `method` and `id`; for the document and definition messages
it also needs typed `params`, which is a real deserialization into the
projections of [section 18](#18-protocol-types) and is fine, because by then
the bytes have long since been forwarded.

The cheap peek is:

```rust
#[derive(Deserialize)]
struct FramePeek<'a> {
    #[serde(borrow, default)] method: Option<&'a str>,
    #[serde(borrow, default)] id: Option<&'a RawValue>,
}
```

Borrowed, so it allocates nothing — but it is **not** free, and the reason is
worth stating because it is the thing that makes the question in this section
non-obvious. `serde_json` has to find those two fields, and to do that it
lexes and validates every other member it passes on the way, `params`
included. On a 2 MB completion response that is a full validating walk of 2 MB
to extract an integer. Allocation-free is not the same as cheap.

Since this now happens off the forwarding path, that cost is paid by `core`'s
queue depth rather than by the editor's latency, which is the right place for
it — but `core` is also required to do only O(1) work per event
([section 2](#thread-layout)), and a walk proportional to frame size is not
O(1). So it still wants fixing.

### The bounded structural prefix scan

The fix exploits an empirical regularity: `id` and `method` come before
`params`. Both serializers that matter emit them in struct declaration order —
Zed's `Request` is `{jsonrpc, id, method, params}`
(`crates/lsp/src/lsp.rs:243`), and rust-analyzer's `lsp-server` `Request` is
`{id, method, params}` (`lsp-server-0.10.0/src/msg.rs:73`). `vscode-jsonrpc`
does the same.

JSON guarantees nothing about member order, so this is a fast path with a
correct fallback, never a parser:

* Scan at most the first **1 KiB** of the frame. Sixty bytes is the realistic
  requirement; the rest is headroom.
* Walk **top-level members only**, tracking string state and backslash escapes
  so that a `"method"` occurring inside a value can never be mistaken for the
  member. Nested values are skipped by depth counting.
* Stop as soon as both `method` (or its confirmed absence) and `id` are known.
* **Decline, don't guess.** If the prefix runs out, if a backslash appears
  inside the method string, if `id` is a number with a fraction or exponent —
  any input the scanner is not certain about — it returns "unknown" and the
  caller falls back to `FramePeek`. Declining is always available and always
  correct.
* Method names are compared as raw ASCII bytes against the small known set. No
  unescaping, no `str` validation, no allocation.
* A counter records how often the fallback fires, **per direction**, and it is
  logged. If some peer does put `params` first, that shows up as a number
  rather than as an unexplained latency profile. That counter is not just
  diagnostics — it is the trigger condition for
  [the suffix variant](#the-suffix-variant-is-not-built-yet) below.

**This is a hand-written scanner over input from another process, so it is
only acceptable with a differential fuzz target**: for every input, either the
scanner declined, or its answer equals `serde_json`'s. That property is what
makes the fast path safe, and it is cheap to state and cheap to run.
[Section 15](#15-testing) carries it.

#### Whether to scan at all is a per-server property

Field order is a property of the peer's **JSON serializer**, not of the
language it analyzes. That distinction is load-bearing: a language routinely
has several servers — Rust has rust-analyzer today and could have others,
TypeScript has `tsserver` and `vtsls`, Python has pyright, pylsp, and
jedi-language-server — and they share nothing about how they serialize. So the
setting is attached to the *server*, and the natural home is the
`ServerAdapter` of [section 7](#per-server-adapters), which is already keyed on
`serverInfo.name` for exactly this reason.

```rust
pub enum PeekMode {
    /// Bounded prefix scan, fall back on anything uncertain. The default:
    /// every serializer observed so far emits `id`/`method` before `params`.
    Prefix,
    /// Never scan; go straight to `FramePeek`. For a server known to defeat
    /// the prefix scan, so it does not pay for a scan that always declines.
    Off,
}
```

This mirrors the structure [section 7](#the-generic-warming-signal) already
uses for health, and for the same reason: **an adapter is for precision, but
nothing may depend on one existing.** So there is also a generic backstop that
needs no server knowledge — if a direction's fallback rate stays high over a
window, that direction stops scanning for the rest of the session. It
self-corrects for unrecognised servers, and it fails in the cheap direction,
since a wrong guess costs a scan of at most 1 KiB.

**The editor direction has no adapter at all.** There is no client-adapter
registry and there should not be one; `clientInfo.name` would be a second
registry serving one setting. Editor → child relies purely on the generic
backstop above.

#### The suffix variant is not built yet

If a server did put `method` after `params`, the symmetric fix would be to
scan backward from the end of the frame. It is **deliberately not
implemented**, and the reason is not only that no such server has been
observed.

A backward scan cannot establish string context locally. Whether a given `"`
opens or closes a string depends on the parity of unescaped quotes from the
*start* of the frame, so a scanner arriving from the end does not know whether
it is inside a string literal, and therefore cannot know whether a `"method"`
it finds is a member name or the contents of some value. Escapes make it
worse: `\"` is an escaped quote and `\\"` is a real one, so even locally the
answer needs a backward run over the preceding backslashes.

That does not make it impossible — a restrictive tail pattern (a known method
name immediately before the closing brace) plus the same differential fuzz
target would be sound enough. It makes it *strictly weaker* than the prefix
scan, for a case that does not exist. Build it when the per-direction fallback
counter says some real server needs it, and not before; the counter exists so
that this is a measurement rather than a guess.

**Sequencing.** The zero-inspection forwarding structure above is a design
property and should be built that way from the start — retrofitting it means
unpicking where decisions are made, which is exactly the kind of change that
reintroduces a double-response bug. The prefix scanner is an *optimization*
behind an already-stable interface (`fn peek(&[u8]) -> Option<Classified>`),
and should be written second, after there is a measurement saying `core`'s
per-event cost matters. Starting with `FramePeek` and a `None`-returning
scanner is a correct, complete implementation.

### 3.2 The swallow decision belongs to `writer:editor`

The swallow is the one child → editor decision that must precede forwarding,
and locating it correctly turns out to be forced.

`reader:child` cannot make it: the set of ids the shim has answered is `core`'s
state, and `reader:child` is deliberately not coupled to `core`. Publishing
the set to the reader — through an atomic, a lock, or a channel — reintroduces
a race that is small, real, and catastrophic: `core` answers id 5 at time *T*,
the child's response to id 5 arrives at *T + ε*, and if the publication has not
landed the editor receives two responses to one request.

Routing all child → editor responses through `core` instead would close the
race but put `core`'s queue on the latency path of every completion and hover
response, which is what [section 2](#thread-layout) exists to avoid.

The place where the race cannot exist is the thread that owns the file
descriptor:

> **`writer:editor` holds the set of request ids it has already emitted a
> response for. A second response to the same id is dropped.**

Both candidate responses — `core`'s and the child's — arrive at `writer:editor`
through channels, so it sees them in some definite order, and whichever it
writes first wins. There is no window in which two can be written, because
there is exactly one writer and it checks its own local state. It then tells
`core` which one won, over a channel, so `core` can record `answered_by_shim`
correctly and run the divergence check
([section 10](#10-divergence-reporting)) against the right pair.

Three properties fall out, and they are the reason this is the right shape
rather than merely a working one:

* **The double-response invariant becomes structural.**
  [Section 15](#15-testing) calls it "the single failure mode most likely to
  escape review and most damaging in the field." Making it a property of the
  one component that can write to the editor turns it from something the
  protocol logic must get right everywhere into something it cannot get wrong.
* **`reader:child` needs no `core` state and no inspection at all**, which is
  what lets the child → editor direction reach the zero-inspection steady
  state described above.
* **The check is free when the set is empty**, which is its normal condition.
  `writer:editor` reads a local `is_empty()` and writes the bytes.

The set is bounded by construction — the shim answers at most
`max_in_flight` (4, [section 13](#13-parallel-dispatch-and-resource-limits))
definition requests concurrently — but entries are still evicted oldest-first
past a cap and cleared on child exit, so a child that dies owing responses
cannot leak them.

In standalone mode ([section 17](#17-standalone-mode)) this same component
enforces the exactly-one-response invariant of
[section 17.2](#172-the-standalone-invariant), which is the same mechanism
serving a different rule.

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

**The shim modifies nothing.** In proxy mode every frame in the
editor -> child direction is forwarded byte-identical, `initialize` included.

It is tempting to reorder `general.positionEncodings` when forwarding
`initialize`, putting `utf-8` first so the child picks it and conversion is
skipped entirely. The shim does not do this, for three reasons that compound:

* **It means modifying the one message whose fidelity matters most.** A typed
  round-trip would drop any client capability the shim did not model, and Zed
  sends custom ones; doing it safely needs a `serde_json::Value` round-trip or
  raw byte splicing, both fiddly.
* **There is little to gain.** With `WirePosition`
  ([section 18.3](#183-the-wire-position-type-is-inert)) the conversion path is
  a rope cursor seek behind a type that cannot be misused, so avoiding it buys
  microseconds against a risk that does not exist.
* **Declining keeps the editor -> child direction free of pre-forward
  decisions** — zero per session, not one.
  [Section 3.1](#31-how-little-inspection-the-forwarding-path-needs) stays
  simple, [section 15](#15-testing)'s byte-identity assertion has no exemption
  there, and the prime invariant is unconditional rather than asserted with a
  footnote.

The shim still *reads* `positionEncodings`, since it must know what was
negotiated. In standalone mode it does pick UTF-8 when offered
([section 17.3](#173-initialize-is-ours-now)), because there it is a party to
the negotiation rather than a bystander.

Conversion lives in one module with exhaustive property tests against a
reference implementation, and every position is converted to byte offsets at
the edge so that no UTF-16 offset ever reaches the handler interface. That
last rule is enforced by the type system rather than by discipline:
`proto::WirePosition` has private fields and yields a `ByteOffset` only when
handed the negotiated encoding and the text
([section 18.3](#183-the-wire-position-type-is-inert)).

## 5. Document state

`core` owns a `HashMap<DocumentUri, Document>`. No lock: it is actor-internal
state, reached only through the event queue.

```rust
struct Document {
    text: Rope,                   // vendored zed rope, utf16-aware
    version: DocumentVersion,     // from didChange
    language_id: LanguageId,      // from didOpen
    /// Edits applied since the cached tree was parsed, and the version
    /// that log starts from. Handed to workers for incremental reparse;
    /// a returned tree consumes a PREFIX of this, never all of it.
    /// `Arc` so a snapshot shares it rather than copying.
    edits_since_parse: Arc<Vec<InputEdit>>,
    parsed_at: DocumentVersion,
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
  measured-precision posture.
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
    Ready,          // has answered a real definition request
    Unresponsive,   // requests outstanding beyond threshold, no traffic
}
```

Signals, in order of reliability:

1. **`InitializeResult` received.** `Starting` to `Warming`.
2. **Silence with work outstanding.** Requests pending beyond a threshold with
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

Implementations live in `driver`, one per language server —
`rust_analyzer.rs` reading `experimental/serverStatus` and recognising the
indexing progress title, `pyright.rs`, `gopls.rs`, and so on. This is a driver
concern rather than a language concern: it is about a specific *server
process*, not about a language's syntax, so it stays here rather than behind
the handler interface. Two servers for the same language can want different
adapters, and routinely will — TypeScript has `tsserver` and `vtsls`, Python
has pyright, pylsp, and jedi-language-server, and nothing about one predicts
the other.

`peek_mode` ([section 3.1](#whether-to-scan-at-all-is-a-per-server-property))
is the clearest case of why the key is the server rather than the language:
frame field order is a property of the server's JSON serializer and has no
relationship whatsoever to the language being analyzed.

### The generic warming signal

Adapters are for precision, but the tool cannot depend on one existing — an
unrecognised server would otherwise never reach `Warming`, and eager
answering, the main payoff of modelling health at all, would be dark by
default.

So there is one generic rule, and it needs no server knowledge:

> **The child is `Warming` until it has answered its first
> `textDocument/definition` request in a way the adapter counts as real.**

Two clarifications, because both wrong readings are plausible and both are
serious:

* **Swallowed responses count.** During `Warming` the shim answers eagerly, so
  the child's response is swallowed and never reaches the editor. The signal
  is that *the child produced a response*, not that the editor received one.
  Keyed on the latter, the shim would stay `Warming` for the entire session
  and answer heuristically forever.
* **What counts as a real answer is server-specific**, so it belongs to the
  adapter. A `null` or empty result is the ambiguous case: some servers reply
  `null` immediately during indexing rather than blocking, and treating that
  as "answered" would close the eager window milliseconds into startup —
  deleting the behaviour exactly where it was meant to apply. Others return
  `null` only for symbols that genuinely have no definition, where it is a
  perfectly good signal of readiness.

```rust
pub trait ServerAdapter: Send + Sync {
    /// Does this definition response indicate the server is ready?
    /// Default: any non-empty result counts; null and empty do not.
    fn definition_indicates_ready(&self, resp: &DefinitionResponse) -> bool {
        !resp.is_empty()
    }

    /// How to classify this server's frames. A property of the server's
    /// JSON serializer, not of the language -- one language commonly has
    /// several servers. See section 3.1.
    fn peek_mode(&self) -> PeekMode { PeekMode::Prefix }
}
```

The default errs toward staying `Warming`, and therefore toward eager. That
trades precision for coverage: the shim answers more queries with its own
guess instead of waiting for the child's. That is the direction this version
of the tool wants ([intro](#core-implementation-design)) — and it is
self-limiting, because a single real answer from the child ends the eager
window for the rest of the session.

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
| `Warming` | **Eager.** Answer heuristically on the first request. |
| `Ready` | **Retry-triggered.** Wait; answer only on a repeat query. |
| `Unresponsive` | **Eager**, and answer an error rather than abstaining. |

`Starting` is absent deliberately: no definition request can arrive before
`InitializeResult`, so it never selects a policy. It stays in the enum for
logging, but — like the removed `Dead` and `Slow` — it is not allowed to
imply behaviour it never drives.

The eager rows follow directly from modelling health rather than just
request-pendency, and they are most of the practical value of doing so. If the
server is provably still indexing, making the user press go-to-definition
twice to discover what the shim already knows is pure friction. Conversely
when the server is `Ready`, waiting costs almost nothing and the proper answer
arrives; the readme's retry rule handles the occasional slow query.

The `Unresponsive` row differs in its abstention behaviour and is explained
in [section 9](#9-deadlines-and-abstention).

There is no `Dead` state: child exit means the shim exits too (see below), so
there is no interval worth modelling in which the child is gone and the shim
is still answering. There is likewise no `Slow` state — a state that selects
the same policy as `Ready` is weight without effect. Whether a slow-but-alive
server should be pre-empted is a future question in `readme.md`; the state
can come back if that answer is yes.

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
    /// Byte-space, as the handler returned it. The wire form is built when
    /// the response is sent and is not retained -- see section 10.
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
5. **Child responds later.** `writer:editor` drops it, since it has already
   emitted a response for that id, and tells `core` which answer actually
   reached the editor
   ([section 3.2](#32-the-swallow-decision-belongs-to-writereditor)). If the
   shim's answer was the one that won, `core` compares the two and possibly
   reports divergence ([section 10](#10-divergence-reporting)). Either way it
   logs the pair for the metrics
   ([section 11](#11-observability-and-the-corpus-scan)).

### Cancellation

`$/cancelRequest` is forwarded and also drops the `PendingQuery` and signals
the handler's cancellation token. The shim must not answer a cancelled
request. If the shim had already answered before the cancel arrived, the
cancel is forwarded anyway and the child's response is swallowed as usual.

Note that the shim can also *receive* a stale cancel for a request it already
answered; this is harmless and must not be treated as an error.

## 9. Deadlines and abstention

The hard cap is enforced by the driver, not trusted to the handler.

**The cap is configurable**, via `--deadline-ms` (`dependency-plan.md` §11).
It defaults to **750ms proxying** — the readme's number — and **2000ms in
standalone**, where an abstention costs the user the answer entirely rather
than a wait they were already having
([section 17.6](#176-the-budgets-change-because-what-they-are-traded-against-changed)).
Nothing below depends on the specific value; "the deadline" means whichever is
in effect.

**The deadline is absolute and starts at request arrival**, not at handler
entry. Queueing time counts. A handler that gets the full budget of wall clock
but started 200ms late has already blown it from the user's point of view, and
the metric in the readme measures the user's point of view.

**Cancellation must be cooperative.** Wrapping CPU-bound work in a timeout
does not stop the work; it only stops waiting for it, leaving a thread burning
CPU that the proper LSP needs. This is why there is no timer-driven deadline
and, in turn, part of why there is no async runtime
([section 2](#2-process-and-transport-model)). The handler contract requires
polling instead:

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

There are two exceptions, and they are the same case: **nothing else is going
to answer.** `Unresponsive` ([section 7](#what-health-is-for)), where the child
has stopped responding, and standalone
([section 17.5](#175-abstention-must-say-something)), where there is no child
at all. In both the request would otherwise hang forever, so the shim answers
with a `RequestFailed` error rather than `null`.

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

This predicate is not a reporting detail. It *is* how precision is measured,
so getting it wrong corrupts the numbers that a future precision floor would
be derived from — and, right now, decides whether the user gets told they were
sent to the wrong place.

Both sides are first normalized: `textDocument/definition` may answer with
`Location`, `Location[]`, `LocationLink[]`, or null, and which one depends on
whether the client advertised `linkSupport`. All shapes collapse to a set of
`Location` — `(DocumentUri, ByteRange)` — taking `targetSelectionRange` for
links.

**The predicate reads nothing.** Both sides carry a line: the shim's answer
because `Location` does
([section 18.4](#184-location-is-byte-based-and-this-fixes-a-real-inconsistency)),
and the child's because that is what came off the wire. So the comparison is
URI, then line, then range, with no file text needed — which matters because
divergence is classified when the child responds, seconds after the answer,
when the per-query read cache is long gone and the target document may never
have been open.

Ranges themselves are compared in byte space when both sides refer to the same
line, like everything else inside the shim
([section 18](#18-protocol-types)); the child's columns are converted at the
edge, using the one line rather than the file.

Then, comparing the shim's answer against the child's. The `agreement` and
`severity` values below are the exact strings written to those fields in
[section 11](#11-observability-and-the-corpus-scan) — the classifier and the
metric must not have separate vocabularies, or the number that ships and the
number that gets measured stop being the same number.

| Relation | `agreement` | `severity` |
|---|---|---|
| Same file, ranges overlap | `match` | — |
| Same file, within 3 lines | `match` | — |
| Same file, more than 3 lines apart | `mismatch` | `same_file` |
| Different file, same module tree | `mismatch` | `near_module` |
| Different file, unrelated | `mismatch` | `unrelated` |
| Child answered null or empty, shim committed | `mismatch` | `unrelated` |
| Both empty | `match` | — |

The 3-line tolerance is deliberate: at that distance the correct definition is
on screen and the user is already reading it, so scoring it as wrong would
measure something nobody experiences as wrong. The tiers below it are the
error severity classes `readme.md` reports, and are what a future budget would
be attached to.

When the child returns multiple locations, agreement means the shim's answer
matches *any* of them — the LSP itself is expressing ambiguity there, so
picking one of its candidates is not an error.

### Reporting

**This is the safety mechanism.** With the precision floor deferred
([intro](#core-implementation-design)), nothing stops the shim answering a
query it has only a weak guess at, so telling the user when that guess was
wrong is the entire protection they have. Under a floor this would be a
secondary nicety; here it is load-bearing, and it should be built and tested
as such rather than left until last.

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

### How much to report

Every divergence is reported. **This is a tool that is sometimes wrong and
tells you so**, and the telling is not optional — with no precision floor
([intro](#core-implementation-design)) it is the only thing standing between a
wrong jump and a false belief the user acts on. Suppressing reports to be
polite would remove the one property that makes answering-on-a-guess
defensible.

So there is no batching window and no politeness cooldown. One
`window/showMessage` per divergence, naming the symbol and where the user was
sent.

Two things follow that are easy to get wrong.

**A notification and a prompt dismiss differently, and that is the whole
reason to distinguish them.** It is tempting to assume `window/showMessage`
lands somewhere passive like a log panel. In Zed it does not: both
`showMessage` and `showMessageRequest` become the same notification card in
the workspace stack (`crates/project/src/lsp_store.rs:1177` and `:1227`). The
difference is dismissal — a message with no actions auto-dismisses after
`dismiss_timeout_ms`, defaulting to 5000
(`crates/project/src/project_settings.rs:174`), while one with actions never
auto-dismisses and sits until clicked.

That is what makes unbatched reporting workable. A stream of `showMessage`
notifications ages out on its own; a stream of `showMessageRequest` prompts
would accumulate permanently. So:

* Every divergence emits a `window/showMessage`.
* The interactive prompt-and-navigate sequence above runs **only when no other
  prompt is outstanding**, so at most one sticky card exists at a time.

Nothing is dropped in either case — this is a choice between two LSP
mechanisms, not a rate limiter.

**There is no flood guard, deliberately.** Nothing in this system produces
bursts of go-to-definition: unlike completion or diagnostics it is
user-initiated, one keypress at a time. A cold start realistically produces a
handful of divergences, not hundreds. A rate limiter here would be machinery
for an imagined problem, and the kind that reveals itself as wrong only by
silently hiding the reports this version depends on. If measurement ever shows
a burst, add one then.

Every divergence is recorded for the metrics whether or not the user sees a
notification — display policy is a UI concern and must not reach the numbers.

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
  "mode": "proxy",
  "server_health": "Warming",
  "decision": "committed",
  "stratum": "explicitly_imported",
  "confidence": 0.94,
  "heuristic_latency_us": 8300,
  "heuristic_locations": ["..."],
  "lsp_latency_us": 4210000,
  "lsp_locations": ["..."],
  "agreement": "match",
  "severity": null
}
```

This single record type covers coverage, precision, error severity
classification, per-stratum breakdown, latency percentiles, and the
LSP-latency value weighting. `stratum` and `confidence` are reported *by the
handler*, since only it knows which resolution path produced the answer; the
driver classifies `agreement` and `severity`, since only it has both answers.

`mode` is `"proxy"` or `"standalone"`. In standalone, `server_health`,
`lsp_latency_us`, `lsp_locations`, `agreement`, and `severity` are all `null`,
because there is no second answer to compare against
([section 17.7](#177-what-this-does-to-measurement)). Without the `mode`
field a mixed log silently pollutes the precision numerator with rows that
could never have had an `agreement`.

### The corpus scan is a separate program

The scan in the readme's development plan is **not** a mode of the shim. It is
its own crate, `scan`, building the binary `heuristic-jump-lsp-scan`. `driver`
has no batch path, no transport abstraction, and no awareness that it exists.

The requirements are opposed at nearly every point:

| | `driver` | `scan` |
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
test — resolution accuracy is. So as long as `scan` builds its `Query` and
`DocumentSnapshot` the same way, the code under test is genuinely identical.
Snapshot construction therefore lives in `shared`, which makes that
structural rather than a matter of discipline.

`scan` spawns a fresh language server per repository, opens documents,
enumerates identifiers with the handler's own grammar, asks both sides, and
writes the records above. The one thing it shares with the shim beyond
`shared` is the agreement predicate from
[section 10](#10-divergence-reporting) — the definition of "match" must not
fork, or the shipped metric and the measured metric stop being the same
number.

### Two modes: collect and replay

Driving a real language server over ten repositories is hours of wall clock,
and it produces an answer that does not change when the handler changes. The
proper LSP's answer for a given position at a given commit is a *fact about
the corpus*, not about our code, so it is collected once and frozen.

`scan` therefore has two subcommands, and only the first needs a server:

* **`collect`** — spawn the server, drive `didOpen` across the repository,
  enumerate identifiers, ask the LSP, write `truth.jsonl`. Slow, run rarely,
  output is a checked-in artifact.
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
every language: the target is a full replay over one language's tuning corpus
in under a minute, so that iteration is bounded by thinking rather than by
I/O. It is stated here rather than left implicit because nothing else in this
document requires `scan` to be able to run without a server, and discovering
the requirement later means discovering it after the corpus has been
collected in a shape that cannot be replayed.

Constraints that make a replay trustworthy:

* **`truth.jsonl` carries its provenance in a header record**: repository
  path and commit, language server name and version, grammar revision, and
  the `scan` version that wrote it. Replay refuses to run against a truth
  file whose repository commit does not match the checkout, rather than
  silently reporting metrics for positions that have since moved.
* **Only the heuristic side is re-measured.** `heuristic_latency_us` is
  produced by replay and is meaningful, because it is the same handler code
  on the same snapshot. `lsp_latency_us` comes from `collect` and is a
  property of the frozen truth — which is exactly what the readme's value
  weighting wants, since it is a fact about how slow the real server was, not
  about this run.
* **Replay measures the handler, not the driver**, same as `collect` — the
  paragraph above applies unchanged. Nothing in the proxy, the health model,
  or the retry protocol is under test in either mode.
* **A truth file is regenerated, never edited.** Metrics compared across two
  corpus versions are not comparable, and a partially refreshed corpus is the
  worst case: it looks like a regression.

### Where the corpus lives

Not inside the repository. Two roots, outside the workspace, passed by path:

```
../heuristic-jump-corpus/       tuning corpus
  rust/
    repos/<name>/               checkout, pinned commit
    truth/<name>.jsonl
  python/
    ...
../heuristic-jump-heldout/      held-out corpus, same shape
```

Three reasons for the split, in ascending order of importance:

* Repository checkouts are large and are not our source. Keeping them out of
  the workspace keeps `cargo` and the `ignore` walk away from them, and keeps
  them from being duplicated by every `git worktree`.
* The two are used by different commands at different times — the tuning
  corpus on every iteration, the held-out corpus rarely — so they have no
  reason to share a directory.
* **Separate roots make held-out isolation a path check rather than a
  convention.** The readme's development plan holds repositories back
  precisely because tuning sessions will otherwise learn them, and a rule that
  says "do not look at these subdirectories" is enforced by whoever remembers
  it. A rule that says "this session is given one path and never the other" is
  enforced by not having the path. `implementation-loop-design.md` §11 relies
  on this being a filesystem boundary.

`--corpus <dir>` on both subcommands; no default, because a defaulted corpus
path is one that eventually points at the wrong one.

## 12. Handler interface

The seam this document commits to; everything behind it is out of scope here.
Per the readme, dispatch is direct — no framework, no config format that
languages must be expressed in.

This trait lives in `shared`, which is deliberately *not* `driver`. See
[section 16](#16-workspace-layout) for why that separation matters.

### Vocabulary types

`shared` newtypes the primitives rather than passing bare integers and
strings across the seam. Almost every value here is an offset, an index, or an
identifier, and those are exactly the things that silently substitute for each
other.

```rust
/// Byte offset into a document. Never a UTF-16 offset — those are
/// converted at the edge and this type is the proof.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

/// A quantity of bytes, distinct from a position. `offset + len` advances,
/// `offset - offset` measures, `offset + offset` does not typecheck.
/// Also what `resolve` counts a query's byte budget in.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteLen(pub usize);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ByteRange { pub start: ByteOffset, pub end: ByteOffset }

impl ByteRange {
    pub fn contains(self, at: ByteOffset) -> bool;
    pub fn overlaps(self, other: ByteRange) -> bool;
    /// Shift by an edit's length delta; None if the edit fell inside,
    /// which invalidates the range. Used for spot anchoring, section 8.
    pub fn shifted_by(self, edit: &InputEdit) -> Option<ByteRange>;
}

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
/// during deserialization, not afterwards -- see section 18.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocumentUri(Url);

/// Request id as it arrived from the editor, number or string, stored in
/// normalized text form so the fast peek path (section 3.1) and the
/// serde_json path produce the same key. Distinct from the shim's own
/// outgoing ids, which cannot be confused with it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EditorRequestId(Box<str>);

/// Zero-based line, as it appears on the wire in both encodings.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineIndex(pub u32);

/// A definition site, as handlers speak of it: byte offsets, always.
/// The wire form is `proto::WireLocation` and only the driver builds it.
///
/// `line` is redundant with `range` but is not encoding: it is row plus
/// byte-range, still entirely byte-space. It is carried because a handler
/// gets it for free from the tree-sitter node it already verified, and it
/// saves the driver a whole-file line index later -- see section 18.4.
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
That is the whole reason [section 18](#18-protocol-types) exists.

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
  correct outcome — the query genuinely had nothing to return, or the deadline
  expired — and it should not share a type with "something went wrong." Under
  the future precision floor it also becomes the mechanism that holds the
  floor, which is a further reason not to model it as an error.
* **`Stratum` is reported on both arms**, because coverage per stratum is
  meaningless without knowing which stratum the abstentions belonged to.
* **`Confidence` exists now** even though nothing compares it against a
  threshold in v1. It is recorded on every answer and never gates one. Two
  reasons it is not deferred along with the floor: the readme's future work
  item on marking heuristic results with a probability estimate needs it, and
  — more importantly — a floor can only be *derived* from
  `(stratum, confidence, agreed?)` triples that were collected while nothing
  was being gated. Retrofitting a confidence notion into handlers written
  without one means revisiting every resolution path, and doing it after the
  fact means the first calibration is computed from answers the old code
  chose to give, which is not the same distribution. It is a
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
* **`grammar()` is what keeps `driver` language-free.** The driver needs to
  parse — for the parse cache in [section 5](#5-document-state) and the
  token-span check in [section 8](#8-go-to-definition-lifecycle) — but
  `tree_sitter::Language` is a runtime value, so the grammar arrives through
  the registry rather than through a `tree-sitter-<lang>` build dependency.
  Without this, `driver` would have to depend on every language crate, which
  is exactly the edge the workspace layout forbids.
* **`ProjectView` is a trait, not a struct.** Handlers consume it; `driver`
  implements it, because the file list cache and scope rules live there.

## 13. Parallel dispatch and resource limits

Three kinds of concurrency, deliberately distinguished:

1. **Heuristic against the proper LSP.** Structural: the request is forwarded
   to the child before any heuristic work starts, so the two always overlap
   and the heuristic never delays the real answer.
2. **Across concurrent queries.** Multiple definition queries can be in flight
   — editors issue speculative requests, and the user may retrigger while one
   is running. Each dispatches to the pool with its own snapshot and deadline.
3. **Within a single query.** Fanning out across candidate files is the
   handler's business, using the pool the driver provides.

All three draw from **one bounded pool**, sized
`max(1, available_parallelism() - 2)` when proxying.

The sizing is the point. The entire justification for having no index is not
competing with the proper LSP for CPU during its startup — and startup is
exactly when the shim is busiest. An unbounded pool would take back with
scheduling pressure everything the no-index decision was supposed to save,
and would do it precisely in the window that matters. Rayon is a reasonable
fit since handlers want data-parallel fan-out over candidate files.

In standalone there is no proper LSP to leave headroom for, so the pool is
sized at `available_parallelism()`
([section 17.6](#176-the-budgets-change-because-what-they-are-traded-against-changed)).
The `- 2` exists for a reason that does not apply there, and keeping it would
be cargo-culting.

Additional limits:

* **Max in-flight heuristic queries** (start at 4). Beyond that, new queries
  abstain immediately rather than queueing. Queueing cannot help under a
  wall-clock deadline; it only guarantees the queued queries blow it.
* **Per-query byte budget** on files read, so one query over a pathological
  repository cannot monopolise the pool.
* **No heuristic work while `core` is behind.** If the event queue is backed
  up, forwarding and state transitions take priority. The prime invariant
  again.

## 14. Failure handling

| Failure | Response |
|---|---|
| Handler panics | `catch_unwind` at the dispatch boundary, treat as abstain, log. After repeated panics disable that handler **and tell the user via `window/showMessage`** — a silently disabled language looks like the tool simply not working |
| Handler exceeds deadline | Drop the result, abstain, log |
| Document unparseable | Abstain. A parse *cache* miss is not a failure — `tree()` parses on demand |
| Protocol projection fails, or document state drifts | Mark the document untrusted; queries against it abstain until a `didClose`/`didOpen` resyncs it. Forwarding is unaffected. [Section 18.6](#186-modelling-errors-must-fail-closed) |
| Child writes a malformed frame | Log; cannot recover framing, so exit rather than corrupt the stream |
| Editor writes a malformed frame | Same |
| Child exits | [Section 7](#child-death) |
| Request the shim does not handle, **standalone only** | Answer `MethodNotFound`. In proxy mode this cannot happen — it is forwarded. [Section 17.2](#172-the-standalone-invariant) |
| Shim's own internal error | Fall back to pure-proxy mode for the rest of the session and log loudly. **In standalone there is nothing to degrade to**, so the fallback is instead to answer every subsequent request with `MethodNotFound` and every definition with `RequestFailed` — still honouring the exactly-one-response invariant, and still saying loudly that it has given up |

That last row deserves emphasis. A permanent "just be a proxy" degraded mode
should be a real, tested code path with a flag that forces it on. It is the
mechanism that makes the prime invariant true rather than aspirational, and
it is also what a user needs when they are trying to work out whether the
shim is responsible for something.

## 15. Testing

* **Transparency golden tests.** Record real editor/server sessions as frame
  traces, replay them, assert every non-intercepted frame is forwarded
  byte-identically. This is the primary defence for the prime invariant and
  should run against traces from more than one editor. In the editor -> child
  direction there are **no exemptions at all**: the shim modifies nothing it
  forwards ([section 4](#position-encoding)), so every frame the editor sends
  must reach the child byte-identical, `initialize` included. In the child -> editor
  direction the only exemptions are the definition responses the shim answered
  itself, which are dropped rather than altered. `initialized` and
  `InitializeResult` must match byte for byte.
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
  [Section 3.2](#32-the-swallow-decision-belongs-to-writereditor) makes it a
  property of `writer:editor` rather than of the protocol logic, so this
  assertion is checking that the structure holds, not policing every code
  path. It should still be global — the point of a structural guarantee is
  that a test for it never fires, and a test that never fires is cheap.
* **Differential fuzz of the frame peek.** The bounded prefix scanner in
  [section 3.1](#the-bounded-structural-prefix-scan) is hand-written parsing of
  another process's output, so it gets a fuzz target with one property: for
  every input, either the scanner declined, or its `(method, id)` equals what
  `serde_json` produces. Nothing weaker is sufficient — a scanner that is
  merely "usually right" about `method` misroutes messages, and about `id`
  misroutes responses. This property is also what makes it safe to add the
  scanner later as an optimization rather than up front.
* **Zero-inspection assertion.** Instrument the readers with a counter of
  frames inspected before forwarding, and assert that across a recorded
  session it is 1 in the editor direction (the `initialize`) and 0 in the
  child direction except while the shim has an outstanding answer. This is the
  executable form of
  [section 3.1](#31-how-little-inspection-the-forwarding-path-needs), and
  without it that property will quietly decay the first time someone needs
  "just one more field" on the forwarding path.
* **Snapshot version invariant.** For a randomised sequence of edits and
  dispatches, assert that `snapshot.tree()` always parses to a tree whose
  extent matches `snapshot.text`, whatever version the cached base was at.
  This is the invariant that makes the private-`base` design worth having,
  and violating it produces confidently wrong answers rather than errors.
* **Edit-log prefix consumption.** Dispatch at v3, apply edits to v7, return
  a tree parsed at v5, assert the log retains exactly the v5..v7 edits and
  the next incremental reparse produces the same tree as a full parse of v7.
  The failure mode is silent divergence that no single-edit test catches.
* **Spot anchoring.** For edits before, after, and overlapping a pending
  spot's token, assert the spot shifts, stays put, and invalidates
  respectively — and that a retry after a formatter-style reindent is still
  recognised as a repeat.
* **Position encoding property tests.** Random text with astral-plane
  characters, round-tripped UTF-8/UTF-16/byte offsets against a reference.
* **Protocol type differential tests.** Every message in the golden corpus
  deserialized with both `shared::proto` and `lsp-types` (a dev-dependency
  only), asserting the fields we model agree. Plus a dedicated case per
  untagged union in [section 18.5](#185-the-untagged-unions-are-the-actual-risk),
  since those are where a hand-written wire type actually goes wrong.
* **Untrusted-document tests.** Feed a `didChange` whose range is outside the
  rope, a non-increasing version, and a `didChange` for an unopened document;
  assert each marks the document untrusted, that subsequent queries against it
  abstain, that a `didClose`/`didOpen` clears it, and — the part that matters
  most — that every frame is still forwarded byte-identically throughout
  ([section 18.6](#186-modelling-errors-must-fail-closed)).
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
  shared/           handler trait, vocabulary newtypes, proto, Error
  resolve/          shared resolution utilities
  lang_rust/        one crate per language
  lang_python/
  lang_typescript/
  driver/           the LSP driver
  heuristic_jump/   the shim binary -- `heuristic-jump`
  scan/             corpus scan -- `heuristic-jump-lsp-scan`, section 11
```

Crate names carry no project prefix, matching the vendored Zed crates
alongside them (`rope`, `sum_tree`) and the workspace-wide `publish = false`.
Two of the names are chosen rather than mechanical:

* **`driver`, not `core`.** A crate named `core` shadows Rust's own, and this
  document already uses "`core`" throughout for the single-threaded actor in
  [section 2](#thread-layout). Two different things called `core` in one
  system is a needless ambiguity; `driver` is what the prose calls the crate
  anyway.
* **`heuristic_jump`** for the binary crate, so the produced binary is
  `heuristic-jump` without a `[[bin]]` rename — the same relationship Zed has
  between its `zed` crate and its `zed` binary.

### The dependency graph

The shape is dictated by one rule from the outset: **`driver` must not depend
on any language crate.** Wiring happens in `heuristic_jump`.

```
                    shared  <-- rope, tree-sitter, serde, serde_json, url
                   /   |   \
                  /    |    \
           resolve     |     driver  <-- crossbeam, rayon, ignore, clap
                  \    |        |
                lang_* /        |
                     \          |
                      +--> heuristic_jump <--+
```

Every edge, and why:

* **`shared` depends on nothing of ours.** The shared vocabulary: it holds
  `LanguageHandler`, `Query`, `Outcome`, `Stratum`, `Deadline`,
  `DocumentSnapshot`, the `ProjectView` trait, and `Error` — types every other
  crate needs to talk about, and no behaviour. It also holds `proto`, the
  hand-written LSP wire types ([section 18](#18-protocol-types)); there is no
  `lsp-types` dependency, so that the vocabulary newtypes are what
  deserialization *produces* rather than what a conversion layer produces
  afterwards. Its own dependencies are `serde`, `serde_json`, `url`, `rope`,
  and `tree-sitter`.

  **`Error` is one enumerated type covering every failure in the system**, not
  an `anyhow`-style boxed `dyn Error`. It lives here rather than in `driver`
  precisely because it spans crates: a handler's failures and the driver's are
  variants of the same enum, which is what lets
  [section 14](#14-failure-handling)'s response table be an exhaustive match
  rather than a set of string comparisons. `dependency-plan.md` section 10
  gives the shape and the rules that keep it closed — no `Other(String)`, no
  boxed variant, foreign `io`/`serde_json` errors only as `#[source]` beside
  our own typed context. Abstention is emphatically not in it: `Outcome` and
  `AbstainReason` stay separate, per
  [section 12](#12-handler-interface).
* **`resolve` depends on `shared`.** The shared *resolution* utilities —
  search, candidate filtering, and so on. Distinct from `shared` in that
  this is code languages call, not types they are described in. Out of scope
  for this document beyond its position in the graph.
* **`lang_*` depend on `shared` and `resolve`**, plus their own
  `tree-sitter-<lang>` grammar crate. Nothing depends on them except
  `heuristic_jump`.
* **`driver` depends on `shared` only.** Everything in sections 1 through 15
  lives here. It is generic over the handler set.
* **`heuristic_jump` depends on `driver` and every `lang_*`.** It is the single
  place where the language list is enumerated:

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

* **Compile times.** Otherwise every language crate transitively pulls in
  the channels, the codec, and the whole proxy just to implement one trait, and every
  edit to the proxy rebuilds every language crate. With ten languages that
  dominates the edit-test loop.
* **It keeps the rule honest.** With `driver` at the bottom of the graph,
  "handlers may as well reach into the driver for this one thing" is always
  one import away. With `shared` at the bottom and `driver` a sibling, the
  layering violation does not typecheck.

* **`scan` depends on `shared` and every `lang_*`** — but *not* on
  `driver`. It is an LSP client, not a proxy, so none of the driver applies
  to it.

### Adding a language

New `crates/lang_<x>/` depending on `shared` + `resolve` + its grammar,
implementing `LanguageHandler`; then one line in `heuristic_jump`. Nothing else in the
workspace changes. That is the whole cost, and keeping it at that is the point
of the graph above.

### Module layout inside `driver`

```
crates/driver/src/
  lib.rs            run(), thread wiring, child spawn, mode selection
  config.rs         Mode, deadline and pool sizing (clap lives in heuristic_jump)
  codec.rs          Content-Length framing, raw frame type
  peek.rs           bounded prefix scan for method/id, serde_json fallback
  router.rs         classification, forwarding, id namespacing
  standalone.rs     synthesized InitializeResult, MethodNotFound catch-all
  actor/
    mod.rs          the event loop, state ownership, snapshot-on-dispatch
    pending.rs      PendingQuery table, is_repeat_of scan, cancellation
    health.rs       Child, ServerHealth, generic signals, policy table
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

Beyond the mechanical patches below, `vendor/rope` gets one substantive change:
its public API speaks in newtypes — `ByteOffset`, `ByteLen`, and `ByteRange`
instead of `usize` and `Range<usize>`, and `LineIndex` / `ByteColumn` /
`Utf16Column` / `CharCount` instead of the bare `u32`s in `Point`,
`PointUtf16`, and `TextSummary` — so the
vocabulary survives contact with the text rather than being unwrapped at the
boundary. It is a design decision with its own re-sync cost and its own
document: **`vendored-rope-design.md`**. Read it before touching `vendor/`.

Three things stated here because they change this section's own claims:

* `ByteOffset`, `ByteLen`, `ByteRange`, `LineIndex`, `ByteColumn`,
  `Utf16Column`, and `CharCount` are *defined in* `rope` and re-exported by
  `shared`, since the dependency direction forbids the reverse. `ByteLen` is
  also what `resolve` uses for its per-query byte budget — one byte quantity,
  not two.
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

`sum_tree` needs no patching, and the newtype work in
`vendored-rope-design.md` does not change that: `sum_tree::Dimension` is
generic over the summary type, so `ByteOffset`'s impls live in `rope`. Its
`tree_map.rs` is unused here and can be dropped; a whole-file deletion still
leaves a clean diff.

`vendor/README.md` records, per crate, the upstream revision it was taken at,
the exact patches applied, and — for `util` — the list of items kept, so that
a future re-sync can tell at a glance whether upstream changed anything that
matters.

**Licensing consequence, stated plainly:** `rope` is GPL-3.0-or-later, so the
shipped binary is GPL-3.0-or-later. That is a project-level commitment
following from vendoring, not a detail, and the readme should say so.

It does **not** follow that our own crates are GPL. `crates/*` are `MIT`:
vendoring GPL code does not transfer copyright in code we wrote, and MIT is
GPL-3.0-compatible, so an MIT crate combines into a GPL binary needing no
extra grant. Marking them GPL would volunteer a restriction that `rope`
imposes on the *combination* only.

The point of keeping them MIT is that `rope` is the sole GPL input —
`sum_tree` and the cut-down `util` are Apache-2.0, which is one-way compatible
into GPL-3.0. So if `ropey` ever wins the argument in
`dependency-plan.md` §5, the whole workspace becomes permissively licensable
without relicensing a line. Relicensing later requires every contributor's
agreement; declaring MIT now costs nothing. `dependency-plan.md` §5 has the
per-crate table and the caveats.

## 17. Standalone mode

`heuristic-jump` with no `-- <server>` after it runs with no proper language
server. The shim is the whole language server: it answers `initialize` itself, serves
go-to-definition from the heuristic alone, and there is nothing to race, swallow,
or diverge from.

### 17.1 Why it exists

Four reasons, in descending order of how much they should influence the design:

* **Languages with no usable server.** Plenty of languages have no language
  server, or one heavy enough that a user will not run it for a quick read
  through a codebase. There the heuristic is not a stopgap for something
  better — it is the only thing on offer, and the comparison it has to win is
  against no navigation at all.
* **Developing and debugging the heuristic.** Running resolution with no
  server in the picture removes every source of variability that is not the
  handler, which is exactly what you want when a jump is landing in the wrong
  place and you are trying to find out why.
* **Editors that support several servers per language.** Zed and VS Code both
  do, and both merge definition results. Running standalone alongside
  rust-analyzer is a lower-effort deployment than proxying. It is a genuinely
  worse one — the editor shows a picker instead of jumping, the retry protocol
  cannot exist, and divergence goes undetected, so precision loses its only
  ground truth — but it is the fallback when proxying is not
  practical, and it should work rather than be blocked.
* **Servers with weak navigation.** Section 4 defers the case of a child that
  declares no `definitionProvider`. Standalone answers most of that question
  without needing the mixed mode: a user in that position can run standalone
  as a second server.

### 17.2 The standalone invariant

[Section 1](#1-the-prime-invariant) is about not breaking a language server
the user depends on. With no child there is nothing to break, so it says
nothing, and it must be replaced rather than quietly dropped — the failure mode
has inverted, not disappeared.

> Every request the editor sends receives exactly one response, always.

In proxy mode an unhandled request is impossible: whatever the shim does not
understand goes to the child, and the child answers. Standalone removes that
backstop. A request the shim neither answers nor forwards is a request slot the
editor holds open forever, and in most clients that is a spinner that never
stops and a feature that stays broken for the rest of the session.

So the catch-all inverts. Where proxy mode's rule is "anything not understood
is forwarded byte-for-byte," standalone's is:

| Message | Action |
|---|---|
| `initialize` | Answer with a synthesized `InitializeResult` — [17.3](#173-initialize-is-ours-now) |
| `initialized` | Ignore |
| `textDocument/didOpen` / `didChange` / `didClose` / `didSave` | To `core`, as in [section 5](#5-document-state) |
| `textDocument/definition` | To `core`. Always answered — [17.5](#175-abstention-must-say-something) |
| `$/cancelRequest` | To `core`. Drops the pending query; no response is owed to a cancelled request |
| `$/setTrace`, `$/logTrace` | Ignore |
| `shutdown` | Answer `null` |
| `exit` | Exit 0 (or 1, if no `shutdown` preceded it, per spec) |
| any other **notification** | Ignore |
| any other **request** | Answer `MethodNotFound` (`-32601`) |

The last row is the one that carries the invariant, and it is worth being blunt
about the shape of it: the correct behaviour is a response, not silence, and
the test for it is a fuzz-ish one — send every method name in the LSP spec plus
a few invented ones, assert a response comes back for each request and no
response for any notification.

A well-behaved client should send almost none of these, because
[17.3](#173-initialize-is-ours-now) advertises almost no capabilities. Clients
are not uniformly well-behaved, and `MethodNotFound` is both correct and cheap.

### 17.3 `initialize` is ours now

[Section 4](#4-initialize-and-capability-negotiation) treats `initialize` as
something to inspect in passing and `InitializeResult` as something the child
produces. In standalone the shim produces it:

```jsonc
{
  "capabilities": {
    "textDocumentSync": 2,              // Incremental
    "definitionProvider": true,
    "positionEncoding": "utf-8"         // if the client offered it
  },
  "serverInfo": { "name": "heuristic-jump", "version": "..." }
}
```

Three things follow, each a change from proxy mode:

* **Nothing else is advertised.** No completion, no hover, no rename, no
  symbols. Advertising a capability the shim cannot serve is how a user ends
  up with broken hover and no idea why. This is also what keeps the
  `MethodNotFound` row above mostly theoretical.
* **The shim finally gets a vote on position encoding.**
  [Section 4](#position-encoding) is emphatic that in proxy mode the shim is in
  the middle of someone else's negotiation and does not get one. Here it is a
  party to the negotiation, so it picks `utf-8` whenever the client advertised
  it in `general.positionEncodings`, and the whole conversion path
  ([section 4](#position-encoding)) goes dark. That is a real reduction in the
  driver's highest-risk surface, and it is the one respect in which standalone
  is safer than proxy mode rather than weaker.
* **Sync kind is chosen, not observed.** `Incremental`, because the shim
  supports it and it is cheaper. Proxy mode has to accept whatever the child
  negotiated; standalone does not.

`rootUri` / `workspaceFolders` are read exactly as in
[section 4](#4-initialize-and-capability-negotiation) — that part is unchanged,
and it is most of why standalone is a policy variation rather than a fork.

**Standalone announces itself.** Immediately after answering `initialize`, the
shim emits one `window/showMessage` and one log line:

> heuristic-jump: standalone mode — no language server was given, so
> go-to-definition is heuristic-only and no other language features are
> available.

This is what replaces the `--standalone` flag
([section 17.8](#178-invocation)). It is not suppressible. Standalone behaves
materially differently — every abstention becomes an error response
([17.5](#175-abstention-must-say-something)), the deadline is longer
([17.6](#176-the-budgets-change-because-what-they-are-traded-against-changed)),
and there is no ground truth ([17.7](#177-what-this-does-to-measurement)) — so
a user in this mode should know they are in it, whether they chose it or
arrived by accident. One line at session start, in the log panel rather than a
modal, is a small price for that.

LSP permits this: `window/showMessage` is one of the few notifications a
server may send before receiving `initialized`.

**Unsupported languages are reported once.** A `didOpen` whose `languageId`
resolves to no handler means every query in that file will abstain. In proxy
mode that is invisible and correct — the child serves the file. In standalone
it presents as the tool being broken, so the first such `didOpen` per language
emits one `window/showMessage` naming the language, and subsequent ones are
silent.

### 17.4 Health, policy, and what disappears

Standalone is, in policy terms, **proxy mode against a child that is
permanently `Unresponsive`**. That is not a coincidence to be smoothed over —
[section 7](#what-health-is-for) already gives `Unresponsive` the two
properties standalone needs: eager answering, and errors rather than silent
abstention. So standalone adds no new policy row.

What it does add is a mode distinction above health, because health is a claim
about a child that does not exist:

```rust
enum Child {
    Proxied(ServerHealth),
    None,                    // standalone
}
```

and the policy function takes a `Child` rather than a `ServerHealth`.
`Child::None` selects the same row as `Unresponsive`.

Consequently the following are dark in standalone, and should be structurally
absent rather than conditionally skipped:

* **The retry protocol** ([section 8](#8-go-to-definition-lifecycle)). Every
  request is answered on first arrival, so there is no second press to detect.
  `PendingQuery`, `Spot`, and `is_repeat_of` still exist — cancellation and
  the trace record need them — but `is_repeat_of` is never consulted.
* **Response swallowing** ([section 3](#3-message-routing)). No child
  responses exist to swallow. The double-response hazard the swallow rule
  guards against is replaced by the exactly-one-response invariant in
  [17.2](#172-the-standalone-invariant), which the same test harness assertion
  covers.
* **Divergence reporting** ([section 10](#10-divergence-reporting)). Nothing
  to compare against, so the agreement predicate is unused and **no mismatch
  message is ever sent.** That is correct rather than a gap. In proxy mode the
  reports exist because the user believes they are talking to a real language
  server and needs telling when they were not. A standalone user was told at
  startup that this is heuristic-only ([17.3](#173-initialize-is-ours-now)) and
  has no reason to expect otherwise, so there is nothing to correct them about.
  "Sometimes wrong and tells you so" is a proxy-mode property; standalone is
  "sometimes wrong, and you already know."
* **Server adapters** ([section 7](#per-server-adapters)). No server.
* **`reader:child`, `writer:child`, and `stderr:child`**
  ([section 2](#thread-layout)). Three of the six threads are simply not
  spawned.
* **Child death handling** ([section 7](#child-death)).

Everything else — documents, the parse cache, spot anchoring, file
enumeration, `ProjectView`, the worker pool, the deadline, the handler
interface, the trace record — is byte-identical to proxy mode. That is the
test of whether this stayed a variation: if `core`'s document and dispatch
code needs to know which mode it is in, something has been wired wrong. The
mode should be visible in exactly three places — thread spawning, the router's
catch-all, and the policy function.

### 17.5 Abstention must say something

This is the substantive behavioural change, and it inherits an argument
already made.

[Section 9](#what-abstention-means-on-the-wire) observes that in proxy mode
abstention is free: the request is still pending with the child, so the shim
says nothing and the child answers. Standalone has no child, so silence is a
hung request slot — the exact thing [17.2](#172-the-standalone-invariant)
forbids.

The shim therefore answers every abstention, and it answers with a
`RequestFailed` error rather than `null`, for the reason section 9 already
gives for the `Unresponsive` case: `null` is a definite claim — "this symbol
has no definition" — which editors render as "no definition found", and it is
a claim the shim has no basis for making. An error says the request could not
be served, which is true, and which clients surface as a transient failure
rather than as an answer.

The error message names the abstention reason, because in standalone the user
has no second opinion to fall back on and "could not resolve `parse_config`:
ambiguous, 7 candidates" is actionable in a way that a bare failure is not.

**This is worth revisiting once there is usage.** An error per abstention may
be noisy, and a mode whose baseline is "no navigation at all" might tolerate
`null` better than proxy mode does. Recorded as an open question below rather
than settled here, because the honest answer depends on what editors actually
do with each and on how often standalone abstains, neither of which is known.

### 17.6 The budgets change, because what they are traded against changed

Two of the numbers in `readme.md` are justified by the existence of a proper
LSP, and both justifications evaporate here. Neither should be silently
carried over.

* **The latency cap.** The 750ms hard cap exists because blowing it degrades
  to an abstention, and an abstention costs the user a wait they were already
  having. In standalone an abstention costs them the answer entirely. So the
  cap is raised — 2000ms is the starting number — and made configurable via
  `--deadline-ms`. It is not removed: a wedged handler must still be
  bounded, and an editor that has been spinning for five seconds is its own
  kind of broken.
* **The pool size.** [Section 13](#13-parallel-dispatch-and-resource-limits)
  sizes the pool at `max(1, num_cpus - 2)` specifically to avoid competing
  with the proper LSP for CPU during its startup. With no proper LSP there is
  nothing to leave headroom for, so standalone sizes at `num_cpus`. The
  reasoning behind the original number is the whole reason this one differs;
  keeping `num_cpus - 2` here would be cargo-culting a constraint that no
  longer applies.

**Precision does not differ by mode, because in v1 it is not enforced in
either.** Both modes answer whenever the handler has a candidate.

The question still needs an answer eventually, and it is sharper than it
looks. When the floor arrives it will be tempting to set a *looser* one for standalone, on
the grounds that a wrong answer there competes against no answer rather than
against a correct one. The counter is that standalone is the mode with **no
proper LSP to correct the record**: divergence reporting, which
[section 10](#reporting) identifies as the entire safety mechanism, is dark
here ([17.4](#174-health-policy-and-what-disappears)). A wrong answer in proxy
mode gets contradicted a few seconds later; a wrong answer in standalone
stands forever. That argues for the floor being *tighter* in standalone, not
looser, and it is the opposite of the intuitive conclusion — which is why it
is written down now rather than rediscovered later.

The mechanism for either is already in place: the commit policy is a table
(`resolution-design.md` section 7), so a per-mode table is a data change
rather than a code change.

### 17.7 What this does to measurement

Standalone has **no ground truth**. The whole observability design in
[section 11](#11-observability-and-the-corpus-scan) rests on the driver seeing
both answers, and here there is one. So:

* The trace record is still written, with `lsp_latency_us`, `lsp_locations`,
  `agreement`, `severity`, and `server_health` all `null`.
* **The record gains a `mode` field** (`"proxy"` / `"standalone"`). Without
  it, a mixed log silently pollutes the precision numerator with rows that
  could never have an `agreement`, and the headline metric quietly stops
  meaning what it says.
* Coverage, latency, and per-stratum breakdown are all still measurable, since
  none of them need the child. Precision and error severity are not.

The corpus scan is unaffected: `scan` is an LSP *client* that drives a real
server for ground truth ([section 11](#the-corpus-scan-is-a-separate-program)),
and it has no proxy in it at all. Calibration therefore continues to come from
proxy-mode-equivalent measurement even for users who only ever run standalone,
which is the right arrangement — the mode with no ground truth borrows its
thresholds from the one that has it.

### 17.8 Invocation

```
heuristic-jump [OPTIONS] -- <SERVER> [SERVER ARGS...]    # proxy
heuristic-jump [OPTIONS]                                 # standalone
```

**There is no `--standalone` flag.** The mode is whether a server was given,
and nothing else. `--` is required before the child command
(`dependency-plan.md` §11), so the two forms cannot be confused.

The obvious alternative is an explicit `--standalone` flag, so that a user who
lost `-- rust-analyzer` to a shell quoting accident gets a usage error rather
than a server that starts cleanly, reports healthy, and serves nothing but
guesses.

It is not worth it. That accident announces itself overwhelmingly — no
diagnostics, no completion, no hover, no inlay hints, no formatting. The user
will not diagnose the *cause*, but they will know within seconds that
something is badly wrong. Against that, a flag is a redundant input that can
contradict the rest of the command line, so it needs a conflict rule and a
hand-written check for the neither-flag-nor-server case that clap cannot
express.

**The accident is handled instead by making the mode announce itself**, which
is better than preventing it:

* It reaches the user in the editor, where they are, rather than in a shell
  they never see — this tool is normally launched from an editor config.
* It *explains* rather than merely forbids, so the user who hits it by
  accident learns the cause instead of only being blocked.
* It also serves the user who chose standalone deliberately and forgot, which
  a flag cannot.

See [section 17.3](#173-initialize-is-ours-now) for what is emitted.

One residual case does get a usage error: **a bare `--` with nothing after
it.** `heuristic-jump --` is the likeliest remaining shell accident
(`heuristic-jump -- $SERVER` with `$SERVER` unset), and unlike a bare
`heuristic-jump` it carries positive evidence that a server was intended. It
is rejected with "`--` given with no server command." `dependency-plan.md` §11
notes that clap does not catch this and it is a three-line check.

### 17.9 Testing

The proxy-mode suite in [section 15](#15-testing) is mostly inapplicable —
there are no forwarded frames to compare. What replaces it:

* **Exhaustive response coverage.** For every request method in the LSP spec
  plus a set of invented ones, assert exactly one response comes back; for
  every notification, assert none does. This is the executable form of
  [17.2](#172-the-standalone-invariant).
* **The double-response assertion carries over unchanged.** It is a harness
  invariant, not a proxy-mode one, and here it pairs with the coverage test
  above to make "exactly one" literal in both directions.
* **Capability honesty.** Assert the advertised capability set and the set of
  methods that do not answer `MethodNotFound` are the same set. This is the
  test that stops the two from drifting, which is how a client ends up routing
  hover to a server that cannot do hover.
* **Mode equivalence.** Run the same document/query script through both modes
  with a child scripted to never respond, and assert the heuristic answers are
  identical. This is what enforces the claim in
  [17.4](#174-health-policy-and-what-disappears) that standalone is a policy
  variation: if resolution behaves differently, some mode knowledge has leaked
  into `core`.
* **Encoding.** Assert that with a client advertising `utf-8`, no conversion
  runs at all, and that with a client advertising only `utf-16`, the
  proxy-mode conversion tests apply unchanged.

## 18. Protocol types

`shared::proto` defines every LSP message and field the shim touches. There
is no `lsp-types` dependency.

### 18.1 Why not `lsp-types`

The obvious objection is that hand-writing wire types to save a dependency is a
poor trade. That is not the trade. The motive is that **the newtypes in
[section 12](#vocabulary-types) should be what deserialization produces, not
what a conversion layer produces afterwards** — and with a foreign types crate
they can only ever be the latter.

With `lsp-types`, every message boundary yields foreign primitives that a
conversion layer then has to launder into `DocumentUri`, `DocumentVersion`,
`EditorRequestId`, `LanguageId`, `ByteOffset`. That layer is real code, it is
where the encoding bugs live, and — decisively — **it is optional.** Nothing
stops a later change from holding an `lsp_types::Position` a few functions
inward, and nothing about that change looks wrong in review. The newtype
discipline `claude.md` asks for becomes a convention enforced by attention
rather than by the compiler, in exactly the part of the system
[section 4](#position-encoding) singles out as the highest-risk.

The concrete case is `Position`:

```rust
pub struct Position { pub line: u32, pub character: u32 }   // lsp-types
```

`character` is a count of UTF-16 code units, or UTF-8 bytes, or UTF-32 code
points, depending on a negotiation that happened in a different function at a
different time. It is a bare `u32` and every one of those readings typechecks.
That is the precise shape of the bug class section 4 describes: invisible on
ASCII, wrong by a few columns on any line with a non-ASCII character.

### 18.2 What replaces it, and why it is smaller than it sounds

Two properties make the hand-written version much less work than a general
LSP types crate, and both come from decisions already made:

* **Nothing is ever round-tripped.** [Section 1](#1-the-prime-invariant)
  forbids deserializing a forwarded message and re-serializing it, and
  [section 3.1](#31-how-little-inspection-the-forwarding-path-needs) means we
  do not even inspect most of them. So the incoming types are **read-only
  projections**: partial structs that name the handful of fields we read and
  ignore everything else, which is serde's default behaviour. A field we did
  not model cannot be lost, because nothing writes it back. That removes the
  main hazard of hand-rolled wire types.
* **Only a small set is ever constructed.** Definition responses, error
  responses, `window/showMessage`, `window/showMessageRequest`,
  `window/showDocument`, and — in standalone
  ([section 17.3](#173-initialize-is-ours-now)) — one `InitializeResult`.

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

### 18.3 The wire position type is inert

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
so the failure mode in [section 4](#position-encoding) — using a UTF-16 column
as a byte index — is not something to be careful about. It does not compile.

The same applies outbound: `WirePosition::encode(ByteOffset, enc, &Rope)` is
the only constructor. Encoding is therefore applied in exactly two functions
in the whole system, both of which take the encoding explicitly, and
`PositionEncoding` itself is set once from `InitializeResult` and never
inferred.

### 18.4 `Location` is byte-based, and this fixes a real inconsistency

[Section 12](#the-trait) has handlers return `Outcome::Committed { locations:
Vec<Location> }` while also stating that "no UTF-16 offset ever reaches the
handler interface." With `lsp_types::Location` those two cannot both be true:
an `lsp_types::Location` holds line/character positions, so a handler would
have to encode them itself, which means a handler needs the rope, the
negotiated encoding, and the conversion code — the exact thing the sentence
forbids.

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
* The agreement predicate ([section 10](#the-agreement-predicate)) is
  line-based by definition — every severity tier is "within 3 lines" or
  "further than 3 lines". With the line in hand it compares URI, then line,
  then range, and **reads nothing at all**, including in the same-file case
  where the document is closed and the divergence arrives seconds later with
  no per-query cache left.

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

### 18.5 The untagged unions are the actual risk

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
[Section 18.2](#182-what-replaces-it-and-why-it-is-smaller-than-it-sounds)
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
  [section 3.1](#31-how-little-inspection-the-forwarding-path-needs).
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

* **A golden corpus.** Real `initialize`/`InitializeResult` pairs and document
  traffic captured from Zed and VS Code against rust-analyzer, pyright, and
  gopls, checked in, and asserted against. Wanted for
  [section 15](#15-testing)'s transparency tests anyway, so the marginal cost
  is near zero.
* **`lsp-types` as a dev-dependency oracle.** For every message in the corpus,
  deserialize with both and assert the fields we model agree. We drop the
  runtime dependency and keep the spec knowledge, which is the part that was
  actually valuable. Same shape as the differential fuzz target for the peek
  scanner in [section 3.1](#the-bounded-structural-prefix-scan): a
  hand-written fast path is acceptable when a trusted implementation checks it
  in tests.

Plus, specifically for the unions, **negative tests**: for each union, assert
that each shape parses as the intended variant *and* that it fails to parse as
each of the others. The positive half is what everyone writes and it is the
half that would have passed while `contentChanges` silently destroyed
documents.

### 18.6 Modelling errors must fail closed

[Section 18.5](#185-the-untagged-unions-are-the-actual-risk) is the sharpest
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
is more general than anything in 18.5: it does not care *which* modelling
mistake occurred. It converts the entire class from "confidently wrong" to
"abstain" — the axis the whole tool is built on, since `readme.md` prices an
abstention at approximately nothing and a wrong answer at the tool's
credibility.

Note the failure being guarded is specifically *silent state drift*. The frame
was already forwarded ([section 3.1](#31-how-little-inspection-the-forwarding-path-needs)),
so the child and the editor are unaffected and the proper LSP still answers
correctly; the only casualty is the shim's own model of the document. Left
undetected, that model produces confident answers about text the user does not
have — which is [section 2](#text-and-tree-can-never-disagree)'s failure mode
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

`readme.md`'s future question 6 asks what the shim should do when the editor
misbehaves — `didOpen` for an already-open document, `didChange` for one never
opened. This is the answer to the half of that question that matters: not
"ignore," but "stop trusting the document, keep proxying perfectly, and say so
in the log."

### 18.7 Where it lives

`shared::proto`, not a separate crate. [Section 16](#the-dependency-graph)
already has `scan` depending on `shared`, and `Location`,
`DocumentUri`, and the vocabulary newtypes have to be in `shared`
regardless, since they are in the handler seam. Splitting the wire types into
their own crate would separate them from newtypes they are defined in terms
of, for no gain.

`shared`'s dependencies become `serde`, `serde_json`, `url` (for
`DocumentUri` normalization and `file:` path extraction, which is where the
percent-encoding and Windows drive-letter bugs live and is not worth
hand-rolling), `rope`, and `tree-sitter`.
