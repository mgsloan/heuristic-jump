# Dependency plan

Decided before any code, because two of these choices (the runtime and the
rope) are load-bearing for the architecture in `core-implementation-design.md`
and expensive to reverse.

Scope: the core driver only — `hj-shared`, `hj-core`, `hj-cli`, plus the
vendored text crates. `hj-resolve` and `hj-lang-*` dependencies are named
where they are already implied, but not settled here.

Versions are what crates.io resolves to as of 2026-08-02, against
`rustc 1.95.0`. Zed's pins (`../zed/Cargo.toml` at `90d024b8`) are listed
where they differ, since the readme commits to matching Zed's grammars and
it is worth knowing when we are ahead of them.

## 0. Summary table

| Crate | Version | Where | Verdict |
|---|---|---|---|
| `crossbeam-channel` | 0.5.16 | hj-core | **chosen** over tokio — see §1 |
| `rayon` | 1.12.0 | hj-core, vendored rope | chosen |
| `serde` | 1.0.229 | hj-shared, hj-core | chosen |
| `serde_json` | 1.0.151 (`raw_value`) | hj-shared, hj-core | chosen |
| `lsp-types` | 0.95.1 | **dev only** | **rejected as a runtime dep** — see §3 |
| `url` | 2.5.8 | hj-shared | chosen, now a direct dep |
| `tree-sitter` | 0.26.11 | hj-shared, hj-core | chosen (Zed: 0.26.9) |
| `ignore` | 0.4.31 | hj-core | chosen |
| `notify` | 8.2.0 | hj-core | **deferred** behind a feature — see §7 |
| `lru` | 0.18.1 | hj-core | chosen, with a caveat — see §8 |
| `thiserror` | 2.0.19 | hj-shared | chosen; `anyhow` explicitly rejected — see §10 |
| `tracing` | 0.1.44 | all | chosen |
| `tracing-subscriber` | 0.3.23 | hj-cli | chosen |
| `rustc-hash` | 2.x | hj-core | chosen (small win, no cost) |
| `heapless` | 0.9.3 | vendored rope/sum_tree | forced by rope |
| `unicode-segmentation` | 1.13.3 | vendored rope | forced by rope |
| `log` | 0.4.x | vendored rope/sum_tree/util | forced by rope |
| `memchr` | 2.8.3 | hj-resolve (not yet) | noted, out of scope |
| `insta` | 1.48.0 | dev | chosen |
| `proptest` | 1.11.0 | dev | chosen |
| `tempfile` | 3.x | dev | chosen |
| `anyhow` | — | — | **rejected** |
| `clap` | — | — | **rejected** — see §11 |
| `num_cpus` | — | — | **rejected**, `available_parallelism` |
| `tokio` | — | — | **rejected** — see §1 |

## 1. Async runtime: none

**Chosen: `std::thread` + `crossbeam-channel` + `rayon`. No async runtime.**

`core-implementation-design.md` §2 used to list `tokio`. It has been amended;
this section records why.

That design's task diagram was five long-lived tasks that never finish, plus
a CPU-bound worker pool, communicating only over channels. Nothing in it is
async-shaped:

* The four pipe tasks each do a blocking read or write on one fd forever. A
  dedicated OS thread is the natural expression of that; `async` buys nothing
  when a task never yields for any reason other than the one fd it owns.
* `core` is a single thread in a `recv` loop. Its whole point is that it is
  serial.
* The worker pool is tree-sitter parsing and byte scanning — CPU-bound, which
  is what `rayon` is for and what an async executor is explicitly not for.
* `ignore`'s parallel walker is thread-based.
* `notify` is sync-native.
* Deadlines are `Instant` + `AtomicBool` polled cooperatively — the design
  already rejects `tokio::time::timeout` in §9 on the grounds that it does not
  actually stop CPU-bound work. So even the timer story does not want tokio.
* The remaining timers (divergence-report batching window, file-list rescan
  debounce) are `crossbeam::select!` with `after(dur)` inside the `core` loop.

Against that, tokio costs ~25 transitive crates, a scheduler between our bytes
and the pipe, and a class of "why is this 3ms late" question that a blocking
read on a dedicated thread does not have. Under a prime invariant whose whole
content is "be a thin, predictable pipe," that is the wrong trade.

Total thread count is ~5 + `max(1, cpus - 2)`, all created at startup, all
long-lived.

Alternatives considered:

* **`tokio` 1.53.1** — the design's choice. Genuinely better if we later
  supervise and restart the child (readme future question 7), because process
  supervision, backoff timers, and racing a restart against in-flight requests
  are all things `tokio::select!` expresses well. Not v1. Reversible: the
  channel-and-thread structure maps onto tokio tasks nearly mechanically,
  which is the reason this is safe to defer rather than a reason to adopt now.
* **`smol` 2.0** — Zed's runtime, so it would ease sharing code with Zed. But
  we are sharing data structures with Zed, not I/O code, so the benefit does
  not materialize.
* **`std::sync::mpsc`** instead of crossbeam — plausible; std's channel is
  crossbeam-derived now. Rejected because we need `select!` over
  (editor events, child events, worker results, timer) in one loop, and std
  has no select. Also crossbeam gives `Receiver::len()`, which §13's "no
  heuristic work while `core` is behind" rule needs to be able to read.

**Consequence:** `core-implementation-design.md` §2 has been amended
accordingly — tasks to threads, the tokio dependency removed, and the
no-async-runtime argument recorded there rather than only here.

## 2. Channels

`crossbeam-channel` 0.5.16, `unbounded()` everywhere, per design §2.

One thing to get right: the design says unbounded because a bounded channel
could stall a reader. That is correct but it means memory is bounded only by
the shed-load rule in §13, so the `core` inbox length is a number we should
log and watch, not just assert about.

## 3. LSP types: our own, not `lsp-types`

**Chosen: hand-written wire types in `hj-shared::proto`. `lsp-types` 0.95.1
stays as a dev-dependency oracle only.**

This reverses an earlier decision in this document, on grounds that are not
about dependency count. Design section 18 has the full argument; the
dependency-relevant part:

* **The motive is the newtypes.** `claude.md` asks for newtypes on primitive
  fields, and the driver's correctness rests on several of them —
  `ByteOffset`, `DocumentUri`, `DocumentVersion`, `EditorRequestId`,
  `LanguageId`. With a foreign types crate those can only be produced by a
  conversion layer *after* deserialization, which makes the discipline
  optional: holding an `lsp_types::Position` a few functions inward
  typechecks, and does not look wrong in review. With our own types the
  newtype *is* the deserialization target.
* **The sharp case is `Position`.** `lsp_types::Position.character` is a bare
  `u32` whose unit — UTF-16 code units, UTF-8 bytes, UTF-32 code points —
  depends on a negotiation that happened elsewhere. Design section 4 calls
  that the highest-risk correctness detail in the driver. Our `WirePosition`
  has private fields and converts only when handed the encoding and the text,
  which makes the bug unrepresentable rather than merely tested for.
* **The surface is smaller than a general LSP crate.** We never round-trip a
  message (design section 1 forbids it), so incoming types are read-only
  partial projections and serde ignores what we did not model. Roughly thirty
  small structs.
* **The spec knowledge is kept without the dependency.** `lsp-types` becomes a
  dev-dependency used as a differential oracle against a golden corpus of real
  Zed / VS Code / rust-analyzer traffic. That is the part of the crate that was
  actually valuable, and it is the same mitigation used for the hand-written
  peek scanner in section 4 below.

Version 0.95.1 for the oracle, for the reason the previous version of this
section gave: 0.97.0 swapped `url::Url` for a pinned `fluent-uri` 0.1.4, and
0.95.1's `Url` matches what we compare against. As a dev-dependency the
`bitflags` 1.x it drags in no longer duplicates in the shipped binary.

**`url` 2.5.8 becomes a direct dependency** rather than a transitive one. It is
needed for `DocumentUri` normalization and for `file:` URI to path conversion,
which is where the percent-encoding and Windows drive-letter bugs live and is
not worth hand-rolling.

Alternatives considered:

* **Keeping `lsp-types` 0.95.1 at runtime.** What this document previously
  chose. The argument was that `InitializeParams` is large and deeply
  optional, and a hand-rolled struct would silently miss a nested field like
  `general.positionEncodings`. That argument is weaker than it looked: a field
  we fail to read is a `None` we can test for against a golden corpus, whereas
  a field we fail to *write* would be data loss — and we never write one,
  because we never round-trip.
* **Zed's fork, `async-lsp`, `tower-lsp-server`.** Rejected for the reasons
  previously given, all of which still apply and now apply more strongly.
* **Generating types from the LSP metamodel JSON.** Microsoft publishes a
  machine-readable spec, and a build script could emit exactly the subset we
  name. Rejected as more machinery than thirty structs deserve, and it would
  generate types over bare primitives — reintroducing the problem this change
  exists to solve.

The real cost is the untagged unions — `id`, `textDocumentSync`,
`definitionProvider`, the definition result, and `contentChanges` — which is
where hand-rolled wire types actually break. Design section 18.5 makes the
golden corpus and the `lsp-types` differential test the explicit condition on
which this choice is acceptable.

## 4. JSON

`serde` 1.0.229 + `serde_json` 1.0.151 with the **`raw_value`** feature.

`raw_value` is not optional here. Frame classification needs `method` and `id`
out of a frame we are otherwise forwarding untouched, and the cheap way to do
that is:

```rust
#[derive(Deserialize)]
struct FramePeek<'a> {
    #[serde(borrow, default)] method: Option<&'a str>,
    #[serde(borrow, default)] id: Option<&'a RawValue>,
}
```

which borrows from the frame buffer and allocates nothing. Deserializing to
`serde_json::Value` instead would allocate a whole tree per frame, which
design §1 budgets at "one message-copy."

Two qualifications, both from design §3.1:

* **This peek is not on the forwarding path.** A forwarded frame is not
  inspected at all in the steady state; `FramePeek` runs inside `core`, after
  the bytes have gone. So its cost shows up as `core` queue depth, not as
  editor latency.
* **Allocation-free is not the same as cheap.** `serde_json` finds those two
  fields by lexing and validating every member it passes on the way, `params`
  included, so a 2 MB completion response costs a 2 MB walk to extract an
  integer. Design §3.1 adds a bounded structural prefix scan in front of it
  as a fast path with `FramePeek` as the fallback. `serde_json` stays as the
  correctness oracle for the scanner's differential fuzz target, so this
  dependency does not go away — the scanner is measured against it forever.

The scanner itself is hand-written and needs no dependency; `memchr` would
accelerate the quote search but the scan is bounded at 1 KiB, so it is not
obviously worth an import that `hj-core` otherwise does not need.

Deliberately **not** enabling `preserve_order` (Zed does). We never re-serialize
a forwarded frame, so map order cannot leak, and `preserve_order` swaps in
`indexmap` for no benefit.

Alternatives: `simd-json`, `sonic-rs`. Both are faster at bulk parsing and
neither matters, because we do not bulk-parse — the whole point is that we
touch a handful of bytes per frame.

## 5. Text: vendored Zed `rope`

**Chosen: vendor `rope` + `sum_tree` + a cut-down `util`, per design §16.**

The argument in design §4 is the real one: `OffsetUtf16` and `PointUtf16` are
first-class dimensions of `TextSummary`, so a UTF-16↔byte conversion is one
sum-tree cursor seek, and `chunk.rs` resolves the in-chunk part with a `u128`
boundary bitmap and a popcount. Position encoding is named as the
highest-risk correctness detail in the driver, and this is the mitigation.

What vendoring actually costs, measured rather than assumed:

* `rope` is 4,132 lines across six files; `sum_tree` is 3,295 across four
  (`tree_map.rs`, 531 of those, is unused and gets deleted).
* Non-Zed deps that come with it: `heapless` 0.9.3, `unicode-segmentation`
  1.13.3, `rayon`, `log`, `tracing`. All ordinary.
* Zed deps that do not: `util`, `ztracing`, and — in **dev**-dependencies —
  `gpui`, `zlog`, `ctor`, `criterion`.

Three patches, each recorded in `vendor/README.md`:

1. **`vendor/util`, cut down.** Confirmed by grep: `rope` and `sum_tree`
   together use exactly `util::is_utf8_char_boundary` (a 4-line `const fn`),
   `util::debug_panic!` (a 12-line macro, needs `log`), and — tests only —
   `util::RandomCharIter`. Note the design says `debug_panic` lives in
   `util`; upstream has moved it to `gpui_util` and `util` re-exports it via
   `pub use gpui_util::*`. The cut-down copy inlines it directly. Keeping the
   crate name `util` means `rope`'s `use util::…` lines are untouched.
2. **`ztracing::instrument` → `tracing::instrument`.** Three call sites
   (`rope.rs:15`, `sum_tree.rs:13`, `cursor.rs:4`). One-line patch each; both
   crates already depend on `tracing`.
3. **Delete the `#[cfg(test)]` modules.** This is a bigger deletion than the
   design anticipated: `rope`'s tests use `#[gpui::test]` (9 sites) and
   `zlog::init_test()`, so keeping them means vendoring `gpui`, which is
   absurd. Deleting whole `mod tests` blocks keeps the re-sync diff clean and
   costs us nothing we can maintain anyway — these are Zed's tests of Zed's
   code. Our own invariants get tested at our own seam (design §15).

Alternatives considered:

* **`ropey` 1.6** (MIT). The serious alternative. It has UTF-16 support
  (`char_to_utf16_cu` / `utf16_cu_to_char`), so the conversions are possible —
  but they route through char indices, so a byte↔UTF-16 conversion is two
  O(log n) seeks rather than one, and UTF-16 is not a summary dimension the
  cursor can seek on directly. Given that design §4 calls encoding the
  highest-risk detail in the driver, that decides it. **Settled: vendored
  `rope`.** The licensing consequence is handled below rather than by
  reopening the choice.
* **`crop`** — smaller and cleaner than ropey, but byte/char/line only, no
  UTF-16 dimension at all. Rejected.
* **`String` + manual line index.** Fine for small files, quadratic on the
  `didChange` path for large ones. Rejected.

### Licensing: our crates are MIT, the binary is GPL

These are two different questions and conflating them gives up something for
nothing.

**The binary is GPL-3.0-or-later, unavoidably.** `rope` is
GPL-3.0-or-later, everything links it transitively through
`DocumentSnapshot`, and a distributed binary is a combined work. That is a
project-level commitment and it should be stated plainly in the readme, with
`rope`'s license text shipped alongside. `../heuristic_jump_old` is already
GPL-3.0-or-later, so nothing about the project's position changes.

**Our own crates are `license = "MIT"` anyway.** Vendoring GPL code does not
transfer copyright in code we wrote, and MIT is GPL-3.0-compatible, so an MIT
crate combines into a GPL binary with no friction and no extra grant needed.
Marking them GPL would be volunteering a restriction the license of `rope`
imposes on the *combination* only.

What that buys, concretely: the portable and valuable part of this project is
`hj-resolve` and the `hj-lang-*` handlers — resolution logic that has nothing
to do with which rope is underneath. Marking those MIT means anyone who
supplies a different text layer can lift them, and it means that if `ropey`
ever wins the argument above, the whole workspace becomes permissively
licensable **without relicensing a line**. That option costs nothing today and
is awkward to recover later, since relicensing needs every contributor's
agreement.

The honest caveat: `hj-shared`'s `DocumentSnapshot` names `Rope` in its public
API, so MIT source is not *drop-in* usable without rope — a taker would have
to modify it. Under MIT they may. It is a real option, just not a free one.

Not proposed: isolating `rope` behind a trait so `hj-shared` could avoid the
dependency. That would put dynamic dispatch or a generic parameter through the
hottest data structure in the system for a licensing reason rather than a
technical one, and it would not even change the binary's license.

Summary of the per-crate `license` fields:

| | License |
|---|---|
| `crates/*` — everything we write | `MIT` |
| `vendor/rope` | `GPL-3.0-or-later` (Zed's, unchanged) |
| `vendor/sum_tree` | `Apache-2.0` (Zed's, unchanged) |
| `vendor/util` | `Apache-2.0`, matching upstream |
| the shipped binary | GPL-3.0-or-later |

`sum_tree` and `util` being Apache-2.0 is worth noting: Apache-2.0 is one-way
compatible into GPL-3.0, and neither is the constraint here. **`rope` is the
only GPL input**, which is exactly why keeping everything else permissive
keeps the exit open.

## 6. Tree-sitter

`tree-sitter = "0.26.11"`. Zed pins 0.26.9; semver-compatible, and the
readme's requirement is that the *grammars* match Zed's pinned revisions, not
that the runtime version does. Grammar crates are `hj-lang-*` business and
out of scope here, except to note that the old repo's pins
(`../heuristic_jump_old/Cargo.toml`) are the starting list, including the two
that must stay as git revs: `tree-sitter-typescript` (zed-industries fork) and
`tree-sitter-cpp`.

`hj-core` depends on `tree-sitter` but on **no** grammar crate — that is the
rule design §16 exists to enforce, and `LanguageHandler::grammar()` returning
a runtime `tree_sitter::Language` is what makes it possible.

Add `[profile.dev.package.tree-sitter] opt-level = 3` (Zed does this). Parsing
in a debug build is slow enough to distort every latency observation we make
while developing.

## 7. File enumeration and watching

**`ignore = "0.4.31"`** — chosen, no real alternative. It is ripgrep's walker,
so `.gitignore` semantics are correct for free, which is directly what the
readme's "gitignored files are out of scope" needs. `walkdir` plus a
hand-rolled ignore implementation would be reimplementing the hard part.

**`notify = "8.2.0"`** — **deferred behind a non-default `watch` feature.**

Design §6 already describes the watcher as best-effort and enabled "only where
watching is cheap." Given that, and given the design provides a second
invalidation path that does not need it (`AbstainReason::NoCandidates`
triggers a debounced background rescan, which pairs with the retry protocol),
v1 should ship the rescan path and leave the watcher unbuilt. Reasons:

* It is the invalidation path that must work anyway, so building it first
  means the watcher is a pure optimization rather than load-bearing.
* `notify` on Linux is inotify, which needs a watch descriptor per directory
  and hits `max_user_watches` on large repos — exactly the failure the design
  wants to avoid, and one that manifests as a silent partial watch.
* Zed uses a fork (`zed-industries/notify` at `0890bbb8`), which is a signal
  that upstream needed patching for their use.

The dependency is written into `Cargo.toml` as optional so the decision is
visible, not lost.

One thing that weakens this: in **standalone mode** (design §17) a stale file
list costs a *permanent* miss, not one the proper LSP quietly covers. The
`NoCandidates` rescan still repairs it on the next query, so the deferral
holds, but standalone is the likeliest reason the watcher eventually gets
built — recorded as design §17.10 question 4.

## 8. Parse cache

**`lru = "0.18.1"`**, with a caveat: design §5 wants the cache bounded by
*both* entry count and total bytes, and `lru` bounds only entries. So
`hj-core` wraps it — track a running byte total, and after each `put`, `pop_lru`
until under the byte ceiling. That is about fifteen lines and is fine.

Alternatives:

* **`schnellru`** — supports custom limiters, so a byte-based bound is native
  rather than bolted on. Genuinely the better fit for this specific
  requirement. Rejected only on maturity/usage grounds; worth revisiting if
  the wrapper gets awkward.
* **`hashlink`** — `LinkedHashMap`, same entry-only limitation, no advantage.
* **Hand-rolled.** The wrapper above is already most of a hand-rolled cache.
  If the `lru` API fights the byte accounting, dropping to `HashMap` + a
  `VecDeque` of keys is not a large loss.

Note the cache is keyed by `(uri, version)` for open docs and `(path, mtime,
len)` for disk files, so it is a `HashMap<CacheKey, Tree>` shape, not a
string-keyed one — `rustc-hash`'s `FxHashMap` is the right hasher for it
(keys are our own types, not attacker-controlled).

## 9. Logging and tracing

`tracing` 0.1.44 + `tracing-subscriber` 0.3.23 (`env-filter`, `fmt`), writing
to **stderr**.

`tracing` is not really a choice — `rope` and `sum_tree` depend on it, so it
is in the graph regardless, and having two logging facades would be silly.

The thing to be careful about: the child's stderr is forwarded verbatim to our
stderr (design §2), so our own log lines interleave with rust-analyzer's in
the editor's log panel. Every line we emit gets a distinguishing prefix, and
the default filter is `warn` so we are quiet unless asked.

The JSONL metric records of design §11 are **not** tracing output. They go to
their own file via `serde_json`, because they are structured data with a fixed
schema that `hj-scan` also writes, and routing them through a log subscriber
would make the schema a formatting concern.

Alternative: `log` + `env_logger`. Simpler, but `tracing` is already in the
graph and its spans are the natural way to attribute the per-stratum latency
the readme asks for.

## 10. Errors: one enumerated type, no `anyhow`

**`anyhow` is rejected. `hj-shared` defines a single total error enum, nested
one level by subsystem.**

The granularity is settled rather than left open: a flat enum of ~60 variants
would make "all possible errors" more literal, but design §14's failure
handling is a table keyed by *class* of failure, and nesting is what lets that
table be an exhaustive match on nine arms instead of a sixty-arm match that
has to be re-checked every time a variant is added. Nesting still enumerates
every leaf; it just groups them the way the code that consumes them groups
them.

`anyhow::Error` is a boxed `dyn Error` — the set of things that can go wrong
is not written down anywhere, cannot be matched on, and grows silently every
time someone adds a `?`. For this system that is the wrong default twice over:
the driver's failure handling (design §14) is a *table* mapping each failure
class to a specific response, and that table is only enforceable if the
failure classes are a closed set the compiler knows about.

```
hj_shared::Error            // the total enum; every failure in the system
├─ Config(ConfigError)      // argv, missing child command, bad trace path
├─ Codec(CodecError)        // framing: bad header, bad Content-Length, ...
├─ Protocol(ProtocolError)  // unexpected message shape, double response, ...
├─ Child(ChildError)        // spawn failed, exited, stdio unavailable
├─ Document(DocumentError)  // didChange for unopened doc, bad range, ...
├─ Encoding(EncodingError)  // position out of bounds, bad UTF-16 offset
├─ Parse(ParseError)        // grammar rejected, tree-sitter timeout
├─ Project(ProjectError)    // read failed, outside scope, budget exceeded
└─ Handler(HandlerError)    // panicked, deadline blown, bad confidence
```

Rules, so this stays a real closed set rather than `anyhow` with extra steps:

* **No `Other(String)`, no `Message(String)`, no `Box<dyn Error>` variant.**
  Adding a failure mode means adding a variant, which is the point.
* **Every variant carries typed context**, not a formatted string. A path is a
  `PathBuf`, an offset is a `ByteOffset`, a URI is a `DocumentUri`.
* **Foreign errors are the one unavoidable leak.** `std::io::Error` and
  `serde_json::Error` are themselves open. They are wrapped as
  `#[source]` fields on our own variants, always alongside our own context
  (which path, which frame), so the *classification* is ours even though the
  detail is theirs.
* **`Result` is not the abstention path.** `Outcome::Abstain` /
  `AbstainReason` stay entirely separate, per design §12 — abstention is a
  correct outcome and must not share a type with failure. Some `hj-core` code
  will convert an `Error` into an abstention; that conversion is explicit and
  logged.
* `#[non_exhaustive]` on the sub-enums but **not** on `Error` itself — within
  one workspace, an exhaustive match on the top level is a feature.

**`thiserror = "2.0.19"`** supplies the `Display`/`Error`/`From` derives. It is
a proc-macro with no runtime presence and does not weaken the "enumerate
everything" property — the enum is still written out by hand; thiserror only
writes the boilerplate impls. Hand-writing `Display` for ~60 variants is the
alternative and is pure transcription.

`hj-cli::main` returns `Result<(), hj_shared::Error>` (or exits with the
child's status), so the top-level match is exhaustive.

## 11. CLI parsing: none

`clap` is rejected. The command line is `heuristic-jump <child> [child args…]`,
where the child args must reach the child byte-for-byte — including things
like `--version` and `--help` that clap would intercept. `trailing_var_arg`
plus `allow_hyphen_values` can be made to work, but it is a lot of
configuration to make an argument parser stop parsing.

Manual: our own flags are recognized only before the child command, all
`hj`-prefixed so they cannot collide, and everything from the first
non-`--hj-` argument onward is the child's. A bare `--` also ends our flags.

| Flag | Meaning |
|---|---|
| `--hj-standalone` | No child at all; design §17. Mutually exclusive with a child command line, and required — a bare `heuristic-jump` is a usage error, per §17.8 |
| `--hj-proxy-only` | Design §14's permanent pure-proxy degraded mode, which the design specifically asks to be a real, tested path. Mutually exclusive with `--hj-standalone`, since together they describe a process that does nothing |
| `--hj-deadline-ms=<n>` | Overrides the hard cap. Defaults to 750 in proxy mode and 2000 in standalone, per §17.6 |
| `--hj-trace=<path>` | JSONL metric records, design §11 |
| `--hj-log=<filter>` | `tracing-subscriber` env-filter string |

The mutual-exclusion rules are the reason this stays hand-written rather than
becoming a clap `ArgGroup`: they are three lines of validation against a
usage error, and clap's version of the same thing still would not solve the
trailing-args problem that rejected clap in the first place.

## 12. Testing

| Crate | Version | Use |
|---|---|---|
| `insta` | 1.48.0 | Frame-trace golden tests (design §15). Snapshot review is the right workflow for "assert every forwarded frame is byte-identical" |
| `proptest` | 1.11.0 | Position-encoding property tests; edit-log prefix consumption; spot anchoring |
| `tempfile` | 3.x | Fixture repositories for `ProjectView` scope tests |
| `lsp-types` | 0.95.1 | Differential oracle for `hj-shared::proto`, per §3 and design §18.5. Dev only — it must never appear in a non-dev dependency table, and that is worth a CI check, since the whole point of §3 is defeated the moment a runtime `use lsp_types::` appears |

Deliberately not adding:

* **`criterion`** — no benchmarks yet, and the latency numbers that matter are
  end-to-end against a real repo, which is `hj-scan`'s job, not a
  microbenchmark's.
* **`arbitrary` / `cargo-fuzz`** — design §15 asks for codec fuzzing. `proptest`
  covers the split-read / bogus-`Content-Length` cases well enough to start;
  add `cargo-fuzz` as a separate non-workspace target if the codec ever gets
  complicated enough to warrant it.
* **`mockall`** and friends — the fake child is a scripted frame list, which is
  a plain struct.
* **`pretty_assertions`** — nice, but `insta` covers the cases where diff
  quality actually matters.

The injected clock for design §15's protocol race tests is a `trait Clock`
with a `TestClock` impl in `hj-shared`, not a dependency.

## 13. Explicitly not depended on

* **`tokio`** — §1.
* **`anyhow`** — §10.
* **`clap`** — §11.
* **`num_cpus`** — `std::thread::available_parallelism()` has been stable
  since 1.59 and is what the pool sizing in design §13 needs.
* **`once_cell`** — `std::sync::OnceLock` / `LazyLock` are stable, and design
  §2 specifically requires the `std` `OnceLock` for `DocumentSnapshot: Sync`.
* **`parking_lot`** — design §2 states there is no lock anywhere. If a
  `parking_lot` import ever appears, something has gone wrong architecturally
  and the fix is not a faster mutex.
* **`dashmap`** — same, more so.
* **`regex`** — `DefinitionHints` in the resolution design wants it, so it
  will land in `hj-resolve`. Nothing in the driver needs it.
* **`memchr` / `aho-corasick` / `grep-searcher`** — the literal scan primitive
  is `hj-resolve`'s. `hj-core` executes the scan on its pool (resolution
  design §3) but the matching itself lives behind that seam. `memchr` is the
  likely pick when we get there.
* **`jiff` / `chrono` / `time`** — trace timestamps are
  `SystemTime::UNIX_EPOCH.elapsed()` as micros. A date-time library for one
  integer is not worth it.
* **`gix` / `git2`** — `ignore` reads `.gitignore` files directly; we never
  need to talk to git.

## 14. Workspace `Cargo.toml` shape

Edition 2024, `rust-version = "1.95"`, `resolver = "3"`. All versions in
`[workspace.dependencies]` so the vendored crates and ours cannot drift.
`publish = false` workspace-wide, which is what makes vendoring `rope` and
`sum_tree` (both `publish = false` upstream) legitimate.

`license` is set **per crate**, not inherited from the workspace, because the
two answers differ — see §5. Ours are `MIT`; the vendored crates keep the
license they arrived with.

```
Cargo.toml
rust-toolchain.toml     pin 1.95.0, so grammar/rope behaviour is reproducible
LICENSE-MIT             covers crates/*
LICENSE-GPL             covers the combined binary, via vendor/rope
vendor/
  README.md             upstream rev, patches applied, items kept
  rope/                 GPL-3.0-or-later
  sum_tree/             Apache-2.0
  util/                 Apache-2.0, cut down to 3 items
crates/
  hj-shared/            MIT -- types, LanguageHandler, proto, Error
  hj-core/              MIT -- the driver
  hj-cli/               MIT -- the binary crate; the artifact it builds is GPL
```

A `cargo-deny` config asserting that `GPL` appears in the graph only via
`vendor/rope` is worth having from the start: it is the check that notices if
a second GPL input ever sneaks in, which is the thing that would quietly
foreclose the exit §5 is preserving.

`hj-resolve`, `hj-lang-*`, and `hj-scan` are in design §16's layout but are
not created by this piece of work.
