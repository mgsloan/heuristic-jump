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

## Campaign 18835da5 — measure_core's replay half (the command line, the two
## modes, the failure listing)

Eight commits, no reverts. The assignment was three gaps and one unjudged
section; the first hour was spent discovering that the gaps described code
that no longer exists.

### The tree was red when I opened it, and the previous campaign closed `confirmed`

`harness/gate core` fails at `9190b44`. `driver::seam`'s §9 check asserts that
`heuristic_jump` is the only crate naming `tracing_subscriber`, and `087fa45`
(campaign 7aa74ea9, "measure had no log subscriber") gave `measure_core` one,
manifest edge and all, without touching the test. So a campaign that closed
`confirmed` left every subsequent session a broken build to diagnose first.

**Run `harness/gate core` before you believe the tree.** Do not infer it from
`cargo test -p <your crate>`: the crate under work was green throughout — the
failing assertion lives in another crate's test, about a *third* crate's
source, which is exactly the shape a per-crate test run cannot see.

The resolution was the test's, not the code's: `deps.md` §9 never says "the
binary and nowhere else" — that is the seam test's gloss — and what it is
about is a library having an opinion about where logs go. Two crates own a
command line (`heuristic_jump`, `measure_core`), and the check now reads one
list for both halves.

### The audit's gap text goes stale in hours, not weeks

Three of my four targets were closed by commits made *after* the audit read
them. The stamps are in `state/audit/core.toml` as `last_audited`, in UTC; the
commits are in local time (-06:00), which is what made the comparison look
wrong at first glance. Compare them before reading a gap as a statement about
the code:

* `the-command-line[d2a209c7a8]` (p50/p99 in the table) — gone in `a96ffb2`,
  four hours after the stamp.
* `two-modes[f2e74dce26]` (grammar literal) and `[eb424449b6]`
  (`check_resumable` compares one field) — both gone in `55de8a2`, twenty
  minutes after the stamp.

That does not make the campaign empty; it makes the *target* the section
rather than the sentence. What a re-audit would still have found was three
different things, and none of them was in the list.

### What was actually wrong, and how it was found

All three came from reading a whole file rather than the sites the gaps named.

* **`collect` checkpointed one row in two hundred.** `writer.append` was
  inside `if rows.len() % CHECKPOINT_EVERY == 0`, so the file on disk held
  every two-hundredth answer while the vector held all of them. A completed
  run rewrites the file and is fine; a *crash* leaves a file whose five rows
  are positions 200, 400, 600, 800 and 1000 — and `done` is read as
  `existing.rows.len()`, so the resume asks positions 5 onward and the corpus
  reports itself complete with 995 positions never asked. The fix is
  ownership: `Writer` holds the rows, `append` is the only way in and does
  both, so "in memory and not on disk" is unspellable.
* **`replay` skipped a row whose file it could not read**, with a `warn!`.
  Those positions leave the table entirely — not as abstentions, not as
  `uncollected` — so the denominator shrinks and coverage *rises*. It is now
  refused, which is the rule §7 already states for the commit.
* **`stage_us` is a second wall clock in the record**, and §7 said
  `heuristic_latency_us` was "the one field" a replay does not reproduce.
  Nothing failed because every fixture handler reports a constant; the first
  real handler with a timed stage would have made the determinism test flaky,
  and the cheap repair for that is one more mask. CHANGE-core-005, and the
  mask is now a list with `the_mask_is_not_the_whole_record` holding its size.

### Approaches considered and not taken

* **Moving `install_logging` into `measure_rust`'s `main`.** It is the
  literal reading of "the subscriber goes in the binary", and it is wrong
  here: §7 makes a `measure_<lang>` four lines *because* `clap`, the flag set
  and `run` are `measure_core`'s, so the log setup would be the one thing
  copied per language — seven chances for one binary to be quiet where the
  others are not. It would also have cost `a_replay_reports_its_own_wall_clock`
  its `has_been_set()` assertion, replacing a behavioural check with a source
  scan.
* **Adding `line` and `identifier` to `QueryRecord`** so the failure digest's
  sample could be read off one line. Rejected twice over: §7 gives the record
  a byte offset deliberately ("a line/column pair here would need a conversion
  in the one place the two halves of the metric have to line up exactly"), and
  the field list is asserted against §7's order by
  `a_replay_row_carries_section_7s_field_set_in_section_7s_order`. The join is
  the answer — `positions/<repo>.jsonl` already carries the token text, so the
  identifier is a lookup on `(file, offset)` and not a second definition of
  what an identifier is.
* **A determinism test over a two-repository corpus.** It would not hold the
  claim: `read_dir` order is stable enough on one machine that two runs agree
  whether or not `repositories()` sorts. The order itself is what is
  asserted — `alpha` before `one`, created second.

### What `measure_core` still has no test for, and why

Everything `collect` does with a live server. `Collection::run` takes a
`Client`, which spawns a process, and the suite has no server to spawn: so
`--restart`, the probe loop, the resume arithmetic and the checkpoint fix
above are held by construction and by reading. A fixture server speaking
enough LSP to answer `initialize` and `textDocument/definition` would close
all of them at once, and it is the one piece of test infrastructure this crate
is missing. It is a campaign, not a corner of one.

## Campaign 44773a93 — §8's protocol types: the corpus is real now, both halves

Targets were `core.md#85-the-untagged-unions-are-the-actual-risk[cfd3a3fbdc]`
and `core.md#83-the-wire-position-type-is-inert[f9ad1766b7]`. Seven commits.

### The gap was reachable because the machine has the servers on it

This is the thing worth carrying forward. §8.5's corpus gap had been open
since the corpus was written, and every campaign that looked at it treated
"captured from Zed and VS Code against rust-analyzer, pyright and gopls" as
aspirational. It is not. On this machine:

* `rust-analyzer` is in `~/.cargo/bin`.
* `gopls` installs with `go install golang.org/x/tools/gopls@latest` — `go` is
  present and the network reaches proxy.golang.org.
* `pyright` runs with `npx --yes pyright@latest`; the npm package is
  `pyright`, **not** `pyright-langserver` (that name 404s), and the binary
  inside it is `pyright-langserver --stdio`.
* `emacs` 30.2 with built-in eglot, which is the only LSP *client* here that
  runs headless.
* `zed` is installed and `DISPLAY=:0` exists, but :0 is the user's real Xorg
  session under gdm and there is no Xvfb. Driving Zed would put a window on
  somebody's screen. Not done, and not because it would not have worked.

Check before assuming. Twenty minutes of `which` opened a gap that had been
read as blocked.

### What was actually captured, and the two traps in doing it

Server half, by a stdio client: gopls and pyright, one `initializeResult`,
definition answers including a real `null` on a keyword, and gopls's
`$/progress` whose token is a **string of digits** — §8.5's first union at its
worst.

Client half, by a recording proxy between eglot and a real server: an editor's
own `initialize`, `didOpen`, three separate `didChange`s, `didSave` and
`didClose`. Two traps, each of which cost a run:

* **`eglot-ensure` does nothing under `--batch`.** It defers the connect to
  `post-command-hook`, and batch mode has no command loop. Use
  `(apply #'eglot--connect (eglot--guess-contact))`.
* **eglot coalesces buffer edits into one notification.** Three edits arrived
  as one merged change. `(eglot--signal-textDocument/didChange)` between edits
  gets three, including the deletion whose `text` is empty — which is the
  shape that matters.

Also: `project-current` prompts without a `.git`, so `git init` the fixture;
and a server that does not advertise `save` in `textDocumentSync` never makes
the editor send a `didSave` (pyright does not, gopls does).

The scripts are in `/tmp/hj-capture`. That is the second campaign to write
them there — `core-018` escalates where they should live, and the recipe is in
the corpus header meanwhile.

### Approaches considered and not taken

* **Trusting the previous campaign's `/tmp/hj-capture/new-lines.jsonl`.** It
  was sitting there, correct, and would have saved four turns. Re-captured
  instead, because a corpus line's whole value is that somebody watched it
  come off a wire, and committing one on the strength of a stale JSON file
  from a session I cannot see is the exact thing the `CAPTURED` label is
  supposed to mean.
* **A `PositionEncoding::settle` in `shared`.** §8.3's minor says nothing
  settles an encoding. Stale: `measure_core::client::settled_encoding` does,
  applying LSP's "omitted means utf-16", and both newly captured servers omit
  it. Adding a second settling site in `shared` would have been a duplicate
  with a better docstring.
* **Removing `WirePosition::line()` to satisfy §8.3's "no accessors".** Not
  implementable. §6 compares `(uri, line)` on the child's answer and **reads
  nothing**; the child's row arrives only inside a `WirePosition`, so without
  the accessor the only route is `resolve`, which needs the target document's
  text — the read §6 forbids two sentences earlier. The document was wrong,
  not the code (CHANGE-core-007).
* **Making "handlers cannot construct a `WireLocation`" true at the type
  level.** The doc claimed it followed from `PositionEncoding` never reaching
  the seam. It does not: the variants are public unit variants, so a handler
  naming `PositionEncoding::Utf16` compiles. Making it type-level needs a
  newtype only the driver can build, which is a seam change and Class B for
  no gain — the property is already asserted by `driver/tests/seam.rs`. The
  doc now says what holds it.
* **Claiming `core.md#vocabulary-types[fbe658c158]`.** Granted, and there was
  nothing to do: `shared.rs:54` re-exports all seven and `driver/tests/seam.rs`
  asserts it, committed in `9e581f7` — **four minutes after** the audit stamped
  the section. Third campaign in a row to hit this; see the findings file.

### What the corpus still lacks

48 messages, 23 captured, every kind represented in both halves. Missing: any
editor other than Emacs, any server-originated request beyond
`window/workDoneProgress/create` and `workspace/configuration` (neither
modelled), and a `didSave` without text off a real editor. None of those is
worth a campaign on its own; they are lines to append when somebody is in
here anyway.
