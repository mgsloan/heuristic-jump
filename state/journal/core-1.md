# Journal — core, worker 1

## 918d4544 — `shared`'s export surface, and the rest of `deps.md`

Six sections, six commits, no reverts. What follows is what a diff does not say.

### The audit lags, and one target was already closed

`core.md#vocabulary-types[fbe658c158]` — the three missing `rope` re-exports —
was **already done** at `shared/src/shared.rs:49` and already held by
`seam.rs:87`, committed before this campaign opened. The audit's stamp on that
section predates the commit. Confirming that cost one `grep`; taking it as the
target would have cost a campaign. The check the last findings file recommends
is right and cheap: verify the gap against the code before working it.

### What I looked at and deliberately did not take

**`deps.md#11-cli-parsing-clap[521f7f6b96]`** — "the Cli carries `trace:
Option<PathBuf>` and `--trace=<path>` writes JSONL metric records". The flag
half is fifteen minutes. The writing half is not reachable: §7's record is
emitted "once both answers are known", which needs the pending-query path, and
`driver` has no run loop. The record type is `measure_core::QueryRecord`, and
`driver` may not depend on `measure_core` (§9's graph), so a `driver`-side
writer would need the type moved to `shared` first. **Do not take §11 until the
request path exists** — the flag alone leaves `--trace=/tmp/x` silently
creating no file, which is worse than the flag being absent, and it does not
make the section clean either way.

**`deps.md#3`** — left. It is `shared::proto`, a file nothing else in this
campaign touched.

### Two assertions with no negative control, both found the same way

This keeps recurring and the pattern is now three-for-three across campaigns:
**a mutation that cargo or rustc rejects produces no `test result` line, and a
control that produces no test result is not a control.**

* `notify`'s `optional = true`: dropping it while `watch = ["dep:notify"]`
  stands makes cargo refuse to parse the manifest. No test runs.
* `Box<dyn Error>` in `shared::Error`: it costs `Error` its `Send`, and
  `files.rs` moves one into the scanner thread, so `driver` stops compiling.

The second one has a sharp edge worth keeping: **`Box<dyn Error + Send +
Sync>` compiles clean**, and that is the form anyone reaching for an escape
hatch actually writes, since it is what the error ecosystem hands out. So the
`Box<dyn` scan is not redundant with the compiler — it covers precisely the
case the compiler misses. I nearly deleted it as redundant after the first
control.

### §8 was absent rather than unheld, which is the interesting one

I predicted all six sections were right-but-unheld. Five were. `deps.md#8` was
not: `TreeCache` was an unbounded `FxHashMap` with a `forget` on `didClose`,
so a long session held every tree it ever parsed for a document nobody closed.
The section was not describing something implemented differently — it was
describing something not implemented.

Three things that cost time there, so the next person does not rediscover them:

* **`seed` had to become `&mut self`.** Reading an LRU is a write. A `get` that
  does not promote leaves the eviction order recording when each tree was
  *parsed* rather than when one was *wanted*, which is the opposite of the
  bound's purpose. Every call site already had a `mut cache`.
* **`(uri, version)` keys break `seed`**, which is handed a document and has to
  find "the newest cached tree for it". `shim.md` §5 solves this by putting
  `parsed_at` on the `Document` row — which does not exist. I kept a
  `Map<DocumentUri, DocumentVersion>` index inside `TreeCache` instead, and
  eviction must remove the index row with the tree. **Do not "fix" that by
  making the index survive eviction**: a row naming a tree that is gone is a
  lookup miss dressed as a hit. A row disappearing while an older version of
  the same document is still cached is a cold miss, and `shim.md` §5 says cold
  misses are correct.
* **`lru::LruCache::push`, not `put`.** `put` returns the old value *for that
  key* and silently drops the evicted entry; `push` returns whichever pair left.
  Byte accounting needs the one that left, and getting this wrong makes the
  running total drift up until the cache evicts everything forever. That is the
  third assertion in the bound test, and it is there because I wrote `put`
  first.
* The byte quantity is the **source text length**, taken in `Parsed::of` where
  the document still exists. A `tree_sitter::Tree` exposes no size, and
  `shim.md` §5's worry ("a single generated file can be enormous") is about the
  file.

### The `TestClock` feature question, decided against a feature

`deps.md` §12 wants `TestClock` "in `shared`, not a dependency". I exported it
unconditionally rather than behind `test-support`. `#[cfg(test)]` is invisible
to an integration test in another crate, which is every caller; a feature means
two build configurations of `shared` plus a self-referential dev-dependency,
and `CLAUDE.md` asks the build matrix not to grow. The price is a clock
production code could drive, so `seam.rs` scans for `TestClock` in any `src/`
file — exempting `deadline.rs` and, on the re-export line only, `shared.rs`.
The first version of that scan failed on its own re-export, which is worth
knowing before writing the next one of these.

The five doubles it replaced were not equivalent: `file_list.rs`'s advanced in
whole milliseconds via `as_millis`, so a suite advancing by 500µs advanced by
nothing. The shared one carries nanoseconds.

### The log prefix went into `driver`, and why

`deps.md` §9 puts the subscriber in `heuristic_jump`. A binary crate exports
nothing, so a prefix implemented there can only be checked by a source scan,
which cannot tell a per-line prefix from a per-event one — and per-event is the
wrong implementation, since the continuation lines are exactly the ones that
read as the child's output. `PrefixedWriter` therefore lives in
`driver::config` beside `DEFAULT_LOG_FILTER`, with the subscriber install still
in the binary. `cargo run -p heuristic_jump -- --log info` shows it working end
to end, which is worth doing once because the writer's `write` contract (return
the bytes of `buf` consumed, never counting the prefix) is easy to get wrong
and the tests would not notice a stream that merely looked right.

## Campaign 7aa74ea9 — the request path (§5, §6, §7, deps §11)

Four gaps, one missing thing: `driver::run` logged its configuration and
returned, so nothing minted a `Deadline`, nothing recorded a `PendingQuery`,
and no query emitted §7's record. Everything else was already there and said so
— `documents.rs`, `trees.rs` and `files.rs` each open with a paragraph saying
what they lack is an owner. Writing the owner was mostly reading those three
headers and doing what they said.

### The thing I decided not to build, and why it is the right split

**The transport.** `shim.md` §2's codec, §3's router, the child spawn and the
swallow decision are the wire that would feed the actor, and none of it exists.
I built the actor with a `crossbeam-channel` inbox instead, so the state
machine is complete and tested and `driver::run` drives it — the loop returns
immediately because nothing sends. That is one missing edge rather than a
missing path, and it kept the campaign inside `core.md`, which is what this
phase audits; `shim.md` joins at 2b.

Whoever picks the transport up: `Actor::run(&Receiver<Event>)` and
`Sender<Outbound>` are the seam, `Event` has the six things `core` is told and
`Outbound` the two it says. Nothing in the actor knows about framing.

The alternative I rejected was building standalone-mode stdio first, on the
grounds that it needs no child. It would not have closed
`#both-sides-are-sets`: standalone has no oracle, so no pending query ever
resolves, and the section is entirely about resolving one.

### Dead ends and near misses

* **The record type had to move to `shared` before anything else could
  start.** `deps.md` §9's graph forbids `driver -> measure_core`. The previous
  campaign's findings said this and were right; it is about forty minutes of
  mechanical work and there is no way around it. `Answered::of` came out of the
  move: the three endings of a dispatch are now classified once instead of once
  per producer, which is what §7's "byte comparable" needs.
* **`Documents::changed` consumes the content changes**, so `core` has no edit
  log to hand `TreeCache::seed`, so incremental reparse is unreachable. I chose
  to `forget` the document's trees on every change rather than pass an empty
  edit log with a stale base — the empty log is then true rather than a lie.
  Do not "fix" this by passing the edits from a second deserialization of the
  same params: §8.6's rule is that the projection happens once, inside
  `Documents`. The real fix is `Believed` carrying its own log, keyed so
  `seed` can take the edits *since the cached version*, and it is a campaign.
* **A `didOpen` must forget the parse cache**, and I did not see it until I
  went back to read `notified` as a whole. It is a resync: same URI, possibly
  shorter text, possibly the same version number as a tree already cached. The
  seed then goes incremental with a base parsed from other text and no edits,
  and tree-sitter hands that tree straight back. Nothing else in the build
  notices. The test is a handler double that reports its tree's `end_byte`
  beside its text's length; it fails without the fix. Worth remembering as a
  shape: **the parse cache's key is `(uri, version)` and versions are only
  monotone within one open.**
* **`serde_json::Value` is denied by `clippy.toml`, in tests too**, and adding
  `serde` to `driver`'s dev-dependencies would show up in `seam.rs`'s reading
  of §9's graph. So the trace assertions scan the record's text. That is not a
  compromise: §7 fixes the field order and the spelling, so the text *is* the
  record.
* **`allow-expect-in-tests` reaches only `#[test]` bodies** — again. Every
  fixture builder and handler double needs the crate-level `#![expect(...)]`.
  Third campaign to hit this; it is in `CLAUDE.md`-adjacent territory but the
  existing test files all carry the attribute with a reason, so copy one.

### What the record still cannot say honestly

`hard_cap` drops a late answer *and the strata with it*, so a deadline-expired
row lands in `unimplemented` and §7's coverage denominator moves by one query.
Both repairs are Class B — a new `Stratum` variant is the frozen seam, a
nullable `stratum_prior` is the metric's shape — so it is `core-017`, open,
with the current behaviour tagged at the one site that invents a stratum.

`server_health` is `null` on every row, including proxied ones where §7's
example shows `"Warming"`. That is `shim.md` §6's health model, which does not
exist; it is not a hole in the record.

`Traces::outstanding` holds a row per proxied query until the child answers. A
child that dies mid-session leaks them. `PendingQueries` has the same shape and
the same bound, which is `shim.md` §10's shed-load rule — not built either.
