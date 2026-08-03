# Conformance loop journal

Approaches tried and abandoned, and why. `design/loops.md` section 4: this is
the single most valuable file for preventing the loop from rediscovering the
same dead end every third campaign.

Written at campaign *close*, not per experiment — an entry written after each
experiment is written at the moment of least understanding.

Newest last. One heading per campaign, with its id.

## b59733c6 — core.md#9-workspace-layout

The first campaign, with an empty repository: no crates, no `vendor/`, no
`Cargo.toml`. Target picked by dependency rather than by distance — every
section was unjudged, and no other one can be satisfied before there is
somewhere to put code.

**The gate is red before you touch anything, and it is not your fault.**
`state/sessions.jsonl` and `state/interventions.jsonl` are written by the
harness when it opens the campaign, are in `DENIED_ALWAYS`, and `check-scope`
without `--rev` looks at the whole working tree including untracked files. So
step 4 fails on two paths you may not write, may not revert, and may not
commit. Do not go looking for what you broke; `harness/gate conformance --rev
HEAD` after committing only owned paths is the verdict that means anything.
Decision `conformance-001` has the three ways out and why none of them are the
loop's to take.

**A skeleton commit cannot pass step 3.** `cargo nextest run` exits 4 on an
empty test list ("error: no tests to run"), and the gate has no `--no-tests`
escape, so the first commit that creates an owned crate must carry a real test
— and CLAUDE.md rules out the unit test that would be the obvious way to
manufacture one. What this campaign used instead was a test that asserts a
claim of the *section*: `CARGO_BIN_EXE_heuristic-jump`'s file name. That is a
usable pattern for other structural sections. It is not a general escape: most
of §9's siblings make claims about the dependency graph, and the honest test
for those needs `cargo metadata`, whose `Command::output` is banned in
`clippy.toml` and would need an `#[expect]`. Consider that before assuming a
manifest-shaped section is cheap.

**Approaches considered and dropped:**

* *Target `core.md#vendoring-the-zed-crates` first*, which was tempting
  because `../zed` is checked out at exactly the revision `deps.md` names
  (`90d024b8`) and because copying it in would have brought thousands of lines
  of upstream tests, solving the nextest problem for free. Dropped for two
  reasons, both worth knowing before retrying it: the section also claims the
  newtype rewrite in `rope-modifications.md`, which is a document outside this
  phase's audit scope and a campaign several times this size, so the section
  would not have gone clean; and `vendor/rope` arrives with
  `LICENSE-GPL -> ../../LICENSE-GPL`, whose target no loop is allowed to
  create (decision `conformance-002`). Vendoring is blocked on that answer,
  not on effort.
* *Creating all six owned crates*, matching §9's layout tree. Dropped because
  `deps.md` §14 scopes this piece of work to `shared`, `driver`,
  `heuristic_jump` and leaves `lang_*`/`measure_*` to
  `core.md#adding-a-language`, which is separately audited. Creating them
  early would have meant six empty crates and a claim on a section this
  campaign was not working on.
* *Declaring the external dependencies (`rope`, `tree-sitter`, `serde`, …) in
  `[workspace.dependencies]` up front*, since §14 says every version lives
  there. Dropped: nothing uses them, so they would not enter `Cargo.lock`, and
  a pin nobody resolves is a pin nobody has checked. They arrive with their
  first user.
* *`[profile.dev.package]`'s opt-level bumps for `tree-sitter` and
  `serde_json`* (§14). Cargo rejects a profile override naming a package
  outside the graph, and neither is a dependency yet. Deferred with a comment
  in the manifest saying so, rather than silently omitted.

**One spec claim was falsified by experiment**, and it is the kind that reads
as obviously true: §9 said package `heuristic_jump` produces a binary called
`heuristic-jump` "without a `[[bin]]` rename". Cargo copies the package name
to the binary target verbatim — `cargo new bin_name` builds
`target/debug/bin_name`. The hyphen mapping people remember runs the other way
and applies to library names, which are Rust identifiers. The Zed analogy in
the sentence is what makes it invisible: `zed`'s crate and binary are both
`zed`, so that case cannot distinguish the two rules. CHANGE-conformance-001,
and the `[[bin]]` block now exists.

**A footnote with a sharp edge:** `hj record`'s `provisional_decisions`
counter greps `crates` and `vendor` only, so a `DECISION-...: provisional` tag
in the root `Cargo.toml` counts as zero. This campaign's first metrics row
reads `provisional_decisions: 0` with one provisional choice in force. Do not
repair that by adding a tagged comment somewhere inside `crates/` — the number
would be right and the site would be fiction. It is written up in
`conformance-002` where the tag actually is.

## 4ba19af5 — core.md#vendoring-the-zed-crates

**Why an unjudged section was picked over nine open gaps.** Because all nine
were the same gap wearing different hats. `core.md` §1 defines the seam's text
vocabulary as *defined in* `vendor/rope` and re-exported by `shared`, so §1's
`Location`, §2's `DocumentSnapshot`, §3's `WirePosition::resolve(enc, &Rope)`
and §10's first two suites were unwritable; §4 and §5 each need a dispatch
site, which needs the seam, which needs the same thing. If you are reading this
because you are about to pick "the gap whose section is closest to going
clean", check first whether it is blocked on something no gap names — that
check is cheap and it is what this campaign is.

The block had just cleared: `conformance-002` was answered and a human placed
`LICENSE-GPL`, which `vendor/rope`'s `LICENSE-GPL -> ../../LICENSE-GPL`
symlink needs a target for. `cp -a` preserved it, as `deps.md` §14 says.

**The vendoring is much smaller than 7,400 lines suggests.** `deps.md` §5's
four patches are the whole of it and each is genuinely small: three `util`
items (one `const fn`, one macro, one test iterator), three `ztracing` imports,
nine `#[gpui::test]` attributes, one deleted `#[ctor]` logger per crate. Two
hours, most of it reading. It compiled and all 29 upstream tests passed on the
first run. Do not budget a whole campaign for the copy itself — budget it for
the newtype sweep, which is the part that is actually large.

**The `#[gpui::test]` substitution is a nothing-burger, as the spec says.**
`seeded(N, f)` — twenty lines reading `SEED`/`ITERATIONS` and looping
`StdRng::seed_from_u64` — is a faithful replacement, because rope's randomised
tests take `mut rng: StdRng` and nothing else. Verified against gpui's
`calculate_seeds` (`../zed/crates/gpui/src/test.rs`) rather than guessed.

**The benchmark is a sixth `util` import site and the spec's list of five is
wrong.** `benches/rope_benchmark.rs:10` is `use util::RandomCharIter;`, and a
bench is compiled as its own crate, so it can see neither `util` (not vendored)
nor rope's `#[cfg(test)]` module. Upstream gets away with it only because rope
dev-depends on `util = { features = ["test-support"] }`, which patch 1 deletes.
`cargo build -p rope` is *green* while `--all-targets` fails, so this is
invisible unless you build all targets — do that on any vendored crate that
has a bench. Fixed by making `test_support` a file and `#[path]`-including it
from the bench: one copy, no public API change, no dependency moved out of
dev. CHANGE-conformance-002.

**Approaches considered and dropped:**

* *Targeting `core.md#5-deadlines-and-abstention` instead*, which looked like
  the cheapest gap on the board — `Deadline { at: Instant, cancelled:
  Arc<AtomicBool> }` needs no dependency at all. Dropped because the section's
  *other* gap is "the driver hard-caps by dropping the result of any handler
  that returns after the deadline", and a dispatch site needs `LanguageHandler`
  and `Outcome`, which need `Location`, which needs `ByteRange`. Half a section
  does not move the number. The same reasoning kills §4: its watcher-tee gap
  needs message routing that does not exist before phase 2b.
* *Doing the newtype sweep in the same campaign.* Costed rather than assumed:
  ~51 public signatures, 54 `Point::new` call sites, all 17 of `ChunkSlice`'s
  public functions, plus `TextSummary`'s nine fields, plus the test bodies that
  follow. `rope-modifications.md` §3 forbids the shortcut that would make it
  cheap — no `Add<u32>`/`PartialEq<u32>` on the newtypes — so every site is a
  real edit. It is a campaign, and starting it with a third of a context left
  would have ended in a revert.
* *Making `shared` depend on `rope` in this campaign*, so that the gate would
  at least compile the vendored crates. Dropped: the only honest thing to
  re-export today is `Rope` itself, and `core.md` §1's claim is about
  `ByteOffset` and friends appearing in rope's signatures — which they do not
  yet. A dependency edge added to make a gate greener, with a re-export that
  asserts something untrue, is worse than the gap it papers over.
* *Adding `rope`/`sum_tree` to the loop's gate crate list.* Not possible
  (`state/phase.toml` is denied) and, more usefully, **not desirable as
  stated**: gate step 2 is `cargo clippy --all-targets -- -D warnings`, and
  `deps.md` §14 deliberately withholds the workspace lints from `vendor/*`
  while the root `clippy.toml` still applies. Measured: five errors on
  unedited upstream code. So the request has to be "build and test but do not
  lint", which the gate has no notion of — `conformance-003`.

**The gate does not touch `vendor/` at all.** Not fmt, not clippy, not tests.
A green `harness/gate conformance` says nothing about the 29 tests that are
the *only* check on the newtype sweep. Until `conformance-003` is answered,
run `cargo nextest run -p rope -p sum_tree` by hand in any campaign that
touches `vendor/`, and say in your journal entry that you did.

**One small trap in the folding.** `debug_panic!` must be defined *before*
`mod chunk;` in `rope.rs`, because `macro_rules!` scoping is textual and
`chunk.rs` is its only caller. Putting it at the bottom of the crate root
compiles as a definition and then fails at every call site.

## e3b8dbf4 — core.md#1-handler-interface

The seam landed: `LanguageHandler`, `Query`, `Outcome`, `Stratum`,
`AbstainReason`, `ServerProfile`, `CommitPolicy` and the vocabulary newtypes in
`shared`; `Deadline`, `SnapshotSeed`/`DocumentSnapshot` and
`ProjectView`/`ProjectPath` alongside them because `Query` names all three;
`Registry` plus a direct `handler.goto_definition(&query)` in `driver`. Four
commits, no reverts, `shared` 0 → ~910 lines.

**Nine tenths of this campaign was reading, not deciding.** `core.md` §1
specifies the seam down to field names, so the work is transcription plus the
handful of places the transcription does not compile. Budget accordingly: the
places it does not compile are listed below and they are what took the time.

**`-D warnings` leaks along a dependency edge, and this is the trap to know
before touching `vendor/` again.** Gate step 2 is
`cargo clippy -p <owned crate> --all-targets -- -D warnings`. `-D warnings` is
a *command-line* flag: it applies to every crate clippy compiles from source,
not to the one named by `-p`. crates.io dependencies are immune because cargo
passes them `--cap-lints allow`; workspace path dependencies are not. So the
moment `shared` depended on `rope`, gate step 2 started linting `rope` — the
crate `vendor/README.md` says out loud is not lint-clean and is not meant to
be — and the gate went red on a crate the loop had not touched. Two lints,
both upstream's: `should_implement_trait` on `Lines::next` (which *cannot*
implement `Iterator`; it lends a `&str` borrowed from `self`) and
`from_over_into` on `impl Into<Chunk> for ChunkSlice`. Fixed with a two-entry
`[lints.clippy]` allow block in `vendor/rope/Cargo.toml`, recorded in
`vendor/README.md`. This does **not** answer `conformance-003`, which is about
the vendored *tests*; it is the same underlying fact arriving through a
different door, and the door will open again for every future crate that
depends on `rope` — `measure_core` next.

**Never `git commit --amend` in this repository.** Two other writers commit to
this branch while a campaign runs: the harness (`hj record` commits the
metrics row itself) and a human at the dashboard (this session saw
`harness: answer conformance-003`, two `spec-change-reviewed` commits and a
hand-authored gate change land inside a five-minute window). `--amend` amends
whatever HEAD happens to be, and by the time this campaign ran it, HEAD was a
dashboard commit — so a decision record and a doc edit were folded into
`harness: spec-drift-reviewed`, and the metrics row that should have keyed on
`318c3f77` keyed on that commit instead, with `campaign: null`. Nothing was
lost and the tree stayed green, but `harness/hj record` only ever records
`HEAD`, so a mis-keyed row cannot be repaired — `check-metrics` walks back to
the most recent commit carrying the loop trailer, which is why the next
commit closed the hole rather than reverting. **Run `commit` and
`harness/hj record` back to back with nothing in between**, and if the gate
fails step 7 alone, read it as bookkeeping rather than as a broken tree.

**Four places `core.md`'s printed types do not compile as printed**, all four
resolved, none of them where you would expect:

* **`Deadline` needs a third field.** §5 prints `{ at: Instant, cancelled:
  Arc<AtomicBool> }` with `expired(&self) -> bool`, and `clippy.toml` bans
  `Instant::now` outside `shared::Clock`. Carrying an `Arc<dyn Clock>` is what
  preserves the printed *signature*, which is the part that matters — the
  alternative, `expired(&self, clock: &dyn Clock)`, is a seam change for a
  detail the seam should not know. `Clock`/`SystemClock` are now in
  `deadline.rs`; `TestClock` is not, because `deps.md` §12 wants it for
  `shim.md` §12's race tests and those do not exist.
* **`DocumentSnapshot` needs a `uri`.** A handler resolving a local binding —
  the commonest answer there is — returns a `Location` in the document it was
  handed, and `Location::at_node` takes a `DocumentUri`. `Query` has no other
  route to one, and `ProjectView::root_of` wants the same value.
  CHANGE-conformance-003; both snapshot types now carry it.
* **`Location`'s `pub` fields contradict §8.4's "constructed only through
  `Location::at_node`".** Private fields with accessors, provisionally, under
  `conformance-004` — the reversible direction, because going the other way
  later means finding every struct literal that accumulated in one `lang_*`
  crate per language.
* **`Registry` cannot derive `Debug`,** because `missing_debug_implementations`
  is on and `dyn LanguageHandler` is not `Debug`. Do not fix this by adding
  `Debug` to the seam's supertraits: that puts a requirement on every `lang_*`
  crate for the sake of a derive. Hand-written, printing the registered
  `language_ids` from the handler vector rather than from either map —
  `iter_over_hash_type` is denied and printing in hash order is exactly the
  irreproducibility it is denied for.

**`Outcome::Committed` being publicly constructible is correct, not a hole.**
§1 says "handlers never construct `Outcome::Committed`; every path ends
through `policy.decide(..)`", which reads like something the type should
enforce. `resolution.md` §7.4 settles it the other way in as many words — it
is "a rule for handler authors", and the driver's redundant re-check that
would catch a violation is explicitly not built in v1 because in v1 there is
nothing for it to catch. Do not spend a campaign making `Outcome` opaque.

**Approaches considered and dropped:**

* *Implementing `ProjectView::candidates`, `parse` and `scan`.* Dropped on
  scope, not on difficulty: their parameter types (`SearchOrigin`,
  `CandidateFiles`, `ScanRequest`, `ScanOutcome`) are `resolution.md` §4's,
  which is not in this phase's audited document set, and `parse`/`scan` need
  the parse LRU and the bounded worker pool that `shim.md` §5 and §10 own.
  Writing them now would mean inventing four types against a document nobody
  is auditing, in a phase that cannot exercise them. `roots`, `root_of`,
  `lookup` and `read` are enough to make `ProjectPath` unforgeable, which is
  the property §1 actually leans on.
* *The per-query read cache `resolution.md` §3 asks for.* It is a lock, and
  saying so is the instruction rather than a way out: the view is reached
  through `&Query` from several fan-out threads, so a cache on it is shared
  mutable state behind `&self`. `conformance-005` has the three shapes that
  avoid one. Note the precedent that does **not** transfer: §2 removed a
  `OnceLock` by parsing eagerly, and a read cache cannot be filled eagerly
  because which files a query reads is what the query is for.
* *`EditorRequestId`.* It is in §1's vocabulary list and it is fifteen lines,
  and it was still dropped. "Stored in normalized text form so the fast peek
  path and the serde_json path produce the same key" has a collision in it —
  the number `5` and the string `"5"` are different ids — and the only
  normalization that avoids it is the JSON literal form, which needs
  `serde_json` for escaping. `serde_json` is not in the graph, nothing
  consumes the type until `shim.md` §3.1's peek path exists, and a key
  normalization guessed a phase early is a protocol bug class. It belongs to
  whoever writes §8.
* *A dispatch smoke test with a fake handler.* Not possible, and the reason is
  worth writing down because it will come up again:
  `LanguageHandler::grammar` returns a `tree_sitter::Language`, and a
  `Language` cannot be constructed without a grammar — there is no `Default`,
  no null value, and the only constructor takes an `unsafe` C entry point. So
  there is no test double for a handler, and there cannot be one until a real
  grammar crate is in the workspace. The first end-to-end dispatch test
  arrives with `lang_rust`, not before. What `tests/seam.rs` asserts instead
  is the pair of *structural* claims nothing else in the build would notice
  breaking: `type_name::<dyn LanguageHandler>()` names the crate the trait is
  defined in (a re-export would not satisfy it), and neither manifest may
  declare a `lang_*` or `tree-sitter-*` dependency.
* *Populating all nine of `deps.md` §10's error sub-enums.* Five —
  `Config`, `Codec`, `Child`, `Document`, `Encoding` — classify failures of
  code that does not exist, so their variants would have been invented from
  prose with nothing able to return them. `shim.md` §11's table is only
  enforceable if its rows are exercisable. They arrive with their producers,
  which is also why `Error`'s match in `dispatch` is written out rather than
  wildcarded: adding an arm has to be a compile error somewhere.
* *`#[derive(Debug)]` on `FileChunks`.* `rope::Chunks` has no `Debug` and a
  448-byte sum-tree cursor trips `large_enum_variant`, which is denied. Both
  handled locally — a hand-written `Debug` and one `#[expect]` with the reason
  — rather than by editing `vendor/rope`, which is the cheaper-looking fix and
  the one that widens the re-sync diff.

## dc1c9639 — core.md#3-position-encoding

Target picked on distance rather than importance: two gaps, both satisfied by
one module and one test file, where every other single-gap section needs a
crate that does not exist (`measure_core`, `similarity`) or a subsystem that
does not (a document map, a golden corpus). The section's audit was one of the
six still stamped `03:47:22`, from before there was any code at all — it says
`vendor/` does not exist on disk. **A stale gap is not a hard gap**; check the
`last_audited` stamp in `state/audit/core.toml` before estimating one, because
the three timestamps in that file are three different worlds.

**`rope` panics on the position you are most likely to hand it, in debug
only.** This is the finding worth the whole campaign. `point_to_offset` and
`point_utf16_to_offset` reach `debug_panic!` when the position is past the end
of its line or inside a scalar value — `chunk.rs:436` and `chunk.rs:560`. In a
release build the same call silently clips and returns a plausible neighbouring
offset. So the two failure modes are *split across profiles*: tests panic,
production answers about the wrong place. Neither is acceptable at a boundary
where the value arrived over a wire, and the fix is not a bounds check of your
own — it is to call `clip_point` / `clip_point_utf16` first and compare the
result with the input, which is the only path through rope that neither panics
nor moves a position without telling you. Anything else in this workspace that
converts a `Point` learned from outside has the same trap waiting.

**`Bias` was not re-exported by rope**, although `clip_offset`, `clip_point`
and `clip_point_utf16` are all public and all take one. Upstream never notices
because every caller inside Zed also depends on `sum_tree`. The tempting fix —
adding `sum_tree` to `shared` — contradicts `core.md` §9, which prints an
authoritative dependency list for `shared` (serde, serde_json, url, rope,
tree-sitter, ignore, rayon, thiserror, rustc-hash) and says §8.7 refers back to
it rather than restating it. So it is a one-line vendor patch instead, recorded
as item 6 in `vendor/README.md`. Read §9's list before adding any dependency to
`shared`: it is more specific than it looks, and it already answers the
question.

**clippy's `allow-unwrap-in-tests` does not reach a helper function in an
integration test.** It looks for an enclosing `#[test]`, and a free `fn` in
`tests/foo.rs` has none — so `unwrap`, `expect`, `panic` and `unreachable` are
all denied in exactly the place a test's reference implementation wants them.
Two gate cycles went to this. What works: saturating conversions
(`u32::try_from(x).unwrap_or(u32::MAX)`) where the saturated value cannot be a
legitimate answer, so a saturation fails an assertion instead of passing one;
and one `#[expect(clippy::panic, reason = ...)]` where there is genuinely no
value to fall back to. Do not restructure the reference to please the lint —
the reference being obvious is the only reason it is worth having.

**Approaches considered and dropped:**

* *Writing `resolve` as a round trip through `encode`* — convert, convert back,
  compare, one statement of the rule for all three encodings. This was the
  first implementation and it is a nicer piece of code than what shipped. It
  cannot work: the conversion it needs to perform *before* the check is the one
  that panics. The round trip survives as a property test
  (`resolve_inverts_encode_at_every_boundary`), which is where it belonged —
  the code states the rule per encoding, the test states it once.
* *Computing the UTF-8 arm as `line_start + character`* — correct by
  definition, since a UTF-8 column is a byte count, and it avoids `Bias`
  entirely. Dropped on the lint set rather than on the idea: `u32` to `usize`
  needs `as` (denied by the `cast_*` block) or `try_from` (whose `unwrap` is
  denied outside tests), and neither escape is worth a special case in the one
  place the three encodings should read alike.
* *Two variants of `PositionEncoding` instead of three.* `core.md` §10's
  property-test bullet names only UTF-8 and UTF-16, and rope carries no
  scalar-count dimension, so `Utf32` is a line walk rather than a seek. Kept
  anyway: this value holds what the *child* negotiated, LSP 3.17 defines the
  kind, and an enum that cannot represent a legal negotiation turns an unusual
  server into an unrepresentable state rather than a slow one.
* *Deriving `Serialize` on `WirePosition`.* It will be needed by §8.4's
  `WireLocation` and §14.3's standalone `InitializeResult`, and adding it now
  would have made the tests marginally easier to write. Left out: §8.2's rule
  that the incoming projections must *not* implement `Serialize` is a property
  someone will want to check mechanically, and pre-empting it from a section
  that is not this campaign's target is how that check ends up with an
  exception in it. `serde_json` is a dev-dependency for the same reason —
  nothing in `shared` writes a frame yet.

**The limit worth knowing about.** `WirePosition` makes it impossible to use a
UTF-16 column as a byte offset, which is the failure §3 is written about. It
does not make it impossible to read a UTF-16 column as UTF-32: on `"😀x"`,
column 2 is offset 4 in UTF-16 and offset 5 in UTF-32, and both are real
positions, so nothing in the type system objects. The only thing standing
between that and a wrong answer is `PositionEncoding` being settled once from
`InitializeResult` and never inferred — which is why §3 says so twice, and why
`PositionEncoding` deliberately has no `Default`. The last assertion in
`tests/position_encoding.rs` is that case, kept as documentation.

**Another writer commits to this branch while you work.** The journal already
says never to `--amend`; the same fact has a second edge. Four files this
campaign did not touch (`design/data-collection.md`, `readme.md`, `todo.md`,
`harness/corpus-selection.toml`) appeared in the working tree mid-session, and
one of them is denied to every loop, so `git add -A` would have produced a
commit the gate rejects at step 4 for a path you never wrote. **Stage explicit
paths, never `-A`**, and take the verdict from `harness/gate conformance --rev
HEAD` afterwards.

## f08773ec — core.md#5-deadlines-and-abstention

Target picked on distance again, and the estimate held: §5 had two gaps, one
of which (`Deadline` with `at`/`cancelled`/`expired()`) had already been
satisfied by an earlier campaign and was only still listed because its audit
entry is stamped `03:47:22`, from before there was code. **Check
`last_audited` before estimating a section's size** — the five sections still
carrying that stamp (`#2`, `#3`, `#4`, `#5`, `#10`) describe a repository that
no longer exists, and one of them is now a section whose remaining work is
half what the gap list says.

**The cap belongs inside `dispatch`, not above it.** The previous campaign
left a comment saying the hard cap "belongs to the caller, which is the only
thing that knows whether the answer is still wanted". That reasoning does not
survive contact with `Query`: the deadline is *in the query*, so `dispatch`
holds everything the caller would have used to decide, and leaving the check
outside makes it a rule somebody upholds rather than a property. It is now
`hard_cap(query.deadline, call(handler, query))` with `call` private, so there
is no uncapped route to a handler through `driver`. The same move made the
claim testable, which is the part that mattered: `hard_cap` takes
`(&Deadline, Dispatched)` and needs no handler, and **there is still no
handler double** — `grammar()` returns a `tree_sitter::Language`, `Query`
needs a `DocumentSnapshot`, and both need a grammar crate. Anything phase 1a
wants to test about dispatch has to be factored out of it like this.

**A late failure stays a failure.** Tempting to fold `Dispatched::Failed` into
`DeadlineExpired` when the deadline has passed, since the wire sees an
abstention either way. Rejected: `core.md` §7's table needs "no coverage
because the handler is broken" and "no coverage because it was slow" to be
different rows, and that is the entire reason `Dispatched` has three variants
instead of two. The test asserts both a `HandlerError` and a `ParseError`
survive the cap.

**Two lint traps, both cheap once known:**

* `Duration - Duration` is denied workspace-wide
  (`unchecked_time_subtraction`), so "one millisecond inside the deadline" has
  to be built as `Duration::from_millis(PROXYING.get() - 1)` — subtract in the
  integer, not in the `Duration`.
* `tracing_subscriber::fmt()` writes to **stdout** by default, which is the
  JSON-RPC wire. `.with_writer(...)` is mandatory, and the only handle
  available is `std::io::stderr`, which `clippy.toml` bans. One
  `#[expect(clippy::disallowed_methods)]` on a named function, the same shape
  `SystemClock` uses for `Instant::now`: the ban's stated replacement is
  `tracing`, and this is where `tracing` comes out.

**Approaches considered and dropped:**

* *Implementing all four of `deps.md` §11's flags.* `--trace` wants §7's
  record writer, which does not exist, and a flag that parses and is then
  ignored is worse than one that errors — it reads as configured. Dropped it
  from the `Cli` entirely; `--trace=x` is now an "unexpected argument" error,
  which is honest. `--log` was kept because log setup is this crate's
  documented job and because it is the only way the resolved cap is
  observable from outside the process.
* *Adding `driver::run` as a stub for `main` to call.* `core.md` §9 prints
  `driver::run(registry, Cli::parse())`, which cannot compile as written —
  `shim.md` §13 puts clap in `heuristic_jump`, so `driver` cannot name `Cli`
  without depending on clap, which is the exact coupling §9's own prose gives
  as the reason for the split. The resolution that trades nothing off is
  `driver::run(registry, Config::from(Cli::parse()))`, i.e. a driver-side
  config type — which is what `config.rs` now is. **The spec edit was
  deliberately not made**: §9 is not this campaign's target, the campaign was
  already adding the code the edit would describe, and that pairing is the one
  shape the loop prompt says is watched for. Whoever targets
  `#the-dependency-graph` should make it there, where the graph is the thing
  under judgement.
* *Having `main` construct a `Registry` from an empty language list.* §9's
  "the one place the language list is enumerated" is already judged clean, and
  an empty registry passed to nothing is scaffolding that reads as wiring.
* *Skipping the bare-`--` check as `deps.md`'s business rather than §5's.*
  Kept, because the failure it catches is a §5 failure: `heuristic-jump --
  $SERVER` with `$SERVER` unset parses as standalone, which swaps 750 for 2000
  and the oracle along with it. Three lines and one test.

**Handover, for `#the-dependency-graph`.** `heuristic_jump` now declares
`tracing`, which §9's graph annotation does not list for it — the third crate
in that position, after `shared` and `driver`, and the audit already calls the
first one a gap. Do not fix it three times: `deps.md` §0's table says `tracing`
is used by *all*, so the defensible edit is one sentence in §9 saying the
graph names what is distinctive rather than what is ubiquitous. `clap` and
`tracing-subscriber` are both on §9's list for `heuristic_jump` already, so
this campaign added no unlisted dependency beyond that one.

## bc8f02bb — core.md#vocabulary-types

Target picked on distance again, and this time on *freshness first*: the
previous campaign's warning about the `03:47:22` stamps is now the first step
of target selection rather than an afterthought. `#vocabulary-types` carried
the newest stamp in `state/audit/core.toml` (`06:13:37`), two gaps, both in one
file that already existed, with no missing crate underneath them. That
combination is rarer than it sounds and is worth checking for directly.

**A large part of the gap list is not reachable by this loop at all, and the
audit does not say so.** Ruled out during selection, on the write list rather
than on difficulty: `#the-dependency-graph`, `#adding-a-language`,
`#one-measurement-library...`, `#the-corpus-scan-is-a-separate-program`,
`#two-modes-collect-and-replay`, `#the-command-line`,
`#the-table-is-not-enough...`, `#7-observability...` and `#9-workspace-layout`'s
remaining gap all bottom out in a crate that does not exist. `measure_core`,
`measure_rust` and `lang_rust` are ours to create; **`similarity` is not** — it
is on the deny list in every phase, and §9 makes every `lang_*` depend on it.
Filed as `conformance-008` (open). Do not spend another campaign rediscovering
this: read the write list in `state/phase.toml` *before* estimating a section,
the same way you now read `last_audited`.

**What `normalized text form` had to mean.** §1 prints
`EditorRequestId(Box<str>)` with the sentence "stored in normalized text form
so the fast peek path (section 3.1) and the serde_json path produce the same
key", and does not say what the form is. It holds the id's **JSON** text — `42`
and `"42"` are different keys — rather than the decoded content. Three
independent facts force it, and any one of them would have:

* `shim.md` §7 keys `PendingQuery` by this type, and a number id and a string
  id spelling the same digits are different requests.
* §8.2's response envelope echoes the id back and LSP wants the echo identical,
  so the kind has to survive storage. JSON text is copied; decoded content
  would have to be re-encoded from a kind it no longer carries.
* §3.1's scanner sees raw bytes. On everything it accepts — it declines on
  backslashes, fractions and exponents — its slice and `serde_json`'s parse
  reach the same text with no conversion in between, which is the "same key"
  the sentence is about.

Requoting uses JSON's *mandatory* escape set and nothing wider (so not DEL, not
U+0085), because the point of the stored text is that it is byte-identical to
what `serde_json` would write. Escaping more is still valid JSON and would
quietly turn the echo into a re-encoding.

**Approaches considered and dropped:**

* *A kind tag plus decoded content* (`n42`, `s42`). Same information, no
  escaping code, and it was tempting for exactly that reason. Dropped because
  the tag has to be stripped before the id can be echoed, which puts a
  conversion back on the one path §8.1 exists to remove it from — and because
  "normalized text form" does not describe a private tagging scheme.
* *Accepting a fractional id.* `visit_f64` is deliberately not implemented, so
  `1.5` is refused. Accepting it would key a request the §3.1 scanner declines
  on, i.e. produce exactly the disagreement between the two producers that this
  type exists to prevent. §8.6's fail-closed rule, applied to the one field
  routing depends on.
* *Deriving `Deserialize` on `LanguageId`/`FileExtension`.* They are interned
  `&'static str`; an impl would have to leak or intern an arbitrary string, and
  §1 requires an unknown language to fail at the boundary. The absence is the
  design, and a future campaign should not read it as an oversight.
* *Adding `Serialize` alongside.* Left out for the same reason the previous
  campaign left it off `WirePosition`: §8.2's rule that the incoming
  projections must not implement `Serialize` is a property someone will check
  mechanically, and pre-empting it from a section that is not the target is how
  that check ends up with an exception in it. The JSON-text representation is
  what keeps the option open.

**The tooling rewrites `\uXXXX` sequences in file writes.** Two edits were lost
to this: a test input spelled with a `a` escape arrived in the file as a
literal `a`, and a control character written as `\u{1}` came back as a raw
control byte that no subsequent `Edit` could match. Both were silent. If a test
needs an escaped JSON string, express it without typing a `\u` sequence — an
escaped solidus (`"a\/b"` and `"a/b"` are one id) covers the decode-and-requote
case, and a control character is better asserted by round-tripping
`from_string(&format!("a{}b", char::from(1u8)))` through `serde_json`, which
proves the escaping happened because a raw control byte inside a JSON string
does not parse.

**`conformance-004` reconciled, and the spec edit is the ruling's own.** The
answer said in as many words that "the code blocks are what is wrong, and
removing those three pubs is the Class A follow-up", so §1 and §8.4 lost them
and the tag came off `Location::at_node`. The type did not change: the document
moved toward code written under the tag a campaign earlier, which is the
opposite direction from the one being watched for, and `CHANGE-conformance-004`
says so in those terms. A reconciliation is cheap when the code already took
the accepted reading — it is one commit and it lowers `provisional_decisions`.

**The `loc:` trailer on `7c9bca1` says `+2` where the real figure is `-2`.**
Left uncorrected because the metrics row is computed from the tree rather than
read from the trailer, and `--amend` is off the table. Compute it with the
`loc_per_crate` rule (non-blank, non-`tests/`) rather than from
`git diff --numstat`, which counts blank lines and test files and will always
disagree.

## 25be160b — core.md#8.2

Target picked by the same three criteria the last two campaigns used, and the
first one paid again: the §8 stamps in `state/audit/core.toml` were the oldest
in the file (`04:41:48`, `05:10:16`), old enough that `#83` and `#87` still
say "no `proto` module exists" about a module that has held `WirePosition` for
two campaigns. **Read `last_audited` before believing a gap.** Two sections in
the list handed to this campaign were probably already clean.

**What the inventory turned out to be.** 31 read projections, 13 constructed
types, 5 that travel both ways. §8.2 says "roughly thirty small structs" and
that is the read half; the constructed set is the part that is easy to
underestimate, because a response envelope drags in an error code, a JSON-RPC
version and — for standalone — a *second* `InitializeResult`.

**The three-way split is the whole design, and it is not in the document as a
split.** §8.2 states two properties separately: incoming types are read-only
projections, and only a small set is ever constructed. Implementing them makes
a third category unavoidable — a `WirePosition` arrives in a definition
request and leaves in a response, and so do `WireRange`, `WireLocation`,
`PositionEncoding` and `TextDocumentSyncKind`. Those five carry both derives.
The rule that survives is therefore not "no type has both", which is false,
but "the message projections have exactly one direction, and the value types
that have two are enumerated". `tests/proto.rs` holds all three lists, which
is what stops the third one being where an exception goes to hide.

**Approaches considered and dropped:**

* *One `InitializeResult` with both derives.* Tempting — the fields overlap
  almost entirely. Dropped because a projection that can be written back is
  the round trip §8.2 removes, and because the two genuinely differ: what we
  *support* is not optional, where what a child reports is. Standalone's is
  `StandaloneInitializeResult` and the two lists stay disjoint.
* *`#[serde(untagged)]` for `contentChanges` with `Incremental` declared
  first.* This is the reordering §8.5 says is not an acceptable defence, and
  writing it out made the reason concrete: nothing in the file would say why
  the order matters, so the next rustfmt-adjacent tidy-up destroys documents.
  The hand-written `Deserialize` is 40 lines with the visitor boilerplate,
  not the 15 §8.5 estimates.
* *Treating `range: null` as the full-document form.* It falls out of
  `next_value::<Option<WireRange>>` for free and is wrong in the one direction
  that costs a document. It is refused instead, and there is a test.
* *`Vec<PositionEncoding>` for `capabilities.general.positionEncodings`.* A
  strict `Deserialize` there fails `initialize` outright when an editor offers
  a kind we do not implement. That field is a *menu*, so unknown entries are
  dropped; failing closed belongs on the negotiated value, which is
  `ServerCapabilities::position_encoding`. The two are opposite policies on
  the same enum and the distinction is worth keeping in mind elsewhere.
* *Deriving `Deserialize` for `LanguageId` on `TextDocumentItem`.* The same
  conclusion the previous campaign reached from the other side: `language_id`
  stays a `Box<str>` because interning is a registry lookup that must be able
  to fail. This is the one field where §8.1's "the newtypes are what
  deserialization produces" deliberately does not apply, and it now says so in
  a doc comment so it is not read as an oversight a third time.

**A `Serialize` bound in a turbofish-only generic is a clippy error.**
`fn is_serializable<T: Serialize>() -> bool` trips `extra_unused_type_parameters`
under `-D warnings`. Returning `PhantomData<T>` uses the parameter and keeps
the property: naming a read projection in that position does not compile.

**The pending-tree gate cannot go green while a human is mid-intervention.**
`crates/similarity/` appeared untracked during this session — the human
answering `conformance-008` — and `harness/gate conformance` checks staged,
unstaged *and untracked* paths, so its diff-scope step fails on files the loop
is forbidden to touch and equally forbidden to delete. There is no move
available inside the loop's own paths that clears it. What works, and what the
gate is built for, is `harness/gate conformance --rev HEAD` after committing
only your own paths: `cmd_check_scope` in forensic mode reads the commit's
paths rather than the working tree. Do not "fix" this by reverting or
stashing someone else's port. Steps 1–3 are unaffected either way, so the
compile-and-test half of the gate is still a real signal while it lasts.

**Handover.** §8.6 is now the only §8 section whose gap is genuinely
unreachable from `shared` alone: untrusted-document state needs `driver`'s
document map, which does not exist. §8.5's remaining gap is the golden corpus
and the `lsp-types` dev-dependency oracle — the corpus is captured traffic
from real editors and servers, so it is closer to an intervention than to a
campaign, and a campaign that targets §8.5 should decide that first rather
than discovering it after writing the differential harness.

## ec5303a7 — core.md#vocabulary-types

The section's last gap, and a small one: `shared` re-exported four of rope's
seven text newtypes. `ByteColumn`, `Utf16Column` and `CharCount` were missing.
One line of manifest-free change, plus the test that makes it stay true.

**Why the gap survived a campaign that targeted this same section.**
`bc8f02bb` closed `#vocabulary-types` and the audit reopened it here. The
three missing names are exactly the three §1 describes as "which handlers do
not use" — so they are the ones a campaign writing the *seam* has no reason to
reach for, and nothing in the workspace could report their absence, because
`shared` itself depends on `rope` and can always name them by either path. The
general shape: **a re-export list is unfalsifiable from inside the crate that
owns it.** If a section says "X is re-exported so that a crate which cannot
depend on Y can name it", the test belongs in a crate that cannot depend on Y,
or it is not testing the claim. `driver` is that crate here (`rustc-hash`,
`shared`, `tracing`), which is why the test landed in
`crates/driver/tests/seam.rs` and not in `shared`'s own tests.

`type_name` earns its place beside the `use`: the `use` catches a *missing*
name, and `type_name().starts_with("rope::")` catches the other failure — a
`shared`-side redefinition that compiles, satisfies every use site, and is not
the type rope's own signatures speak in. The same trick already carries §1's
"the trait lives in shared" assertion in that file, so this is a second use of
a pattern rather than a new one.

**Considered and not done.** Asserting the seven by *value* (constructing a
`ByteColumn(3)` and so on) instead of by `type_name`. It compiles, it uses the
imports, and it asserts less: a redefinition in `shared` would pass it. It
would also have quietly pinned each type's constructor shape — `pub` tuple
field, one field — which is `#vendoring-the-zed-crates`'s business and would
have put a second file in the way of the newtype sweep that section still
needs. Nameability is the whole of what §1 claims here.

**What this campaign is evidence about, beyond its own diff.** The other
sections judged clean and then reopened were reopened for the same reason —
the claim was satisfied by code that a later campaign had no compiler reason
to preserve. Every claim in `core.md` §1 that is now clean has a test that
fails *at compile time* if it stops being true, and the ones that keep
reopening are the ones checked by reading. That is the cheapest available
signal about which of a section's claims are worth spending a test on, and it
argues for spending the test on the where-claims specifically: a where-claim
has no runtime behaviour to assert, so it has nothing else holding it up.

**Cost.** One experiment, no reverts. The gate was green on the first run, so
the guidance about the pending-tree gate from `25be160b` was not exercised
again — `crates/similarity/` is committed now and the working tree was clean
at open.

---

## 5314b0c3 — `core.md#6-the-agreement-predicate` — confirmed

**Why this target, since the reasoning is the reusable part.** The gap list
had 41 entries across 27 dirty sections, and the number moves per *section*.
Ranking one-gap sections by whether their work is reachable: four of them
(`#vocabulary-types`, `#87-where-it-lives`, `#84`, `#85`) turned out to have
**stale gaps** — `shared` already re-exports all seven rope newtypes and
`shared::proto` is 809 lines with `WireLocation`, `WireRange`,
`WireLocationLink` and an untagged `DefinitionResult`. That is the standing
finding paying for itself a third time; check the code before believing the
audit. Four more (`#one-measurement-library`, `#where-the-corpus-lives`,
`#the-command-line`, `#the-table-is-not-enough`) are all blocked behind
`measure_core` existing, and creating it would move none of them alone.
`#the-trait` is inside the frozen seam. That left §6, which is pure logic and
whose one prerequisite had landed since the audit ran.

**The obstruction that shaped the whole campaign: a test cannot build a
`Location`.** `Location::at_node` is the only constructor since the ruling on
`conformance-004`, it needs a `tree_sitter::Node`, and no grammar crate is in
the workspace. This is the *third* campaign to hit it (`driver/tests/deadline.rs`
says so at its head, and `#10-testing`'s snapshot gap is the same wall). §6 is
the one section where an untested reading corrupts the numbers a precision
floor would later be derived from, so shipping it untested was not an option.

Three ways out were considered and two rejected:

* **A `tree-sitter-rust` dev-dependency on `shared`.** Rejected. It is a Class
  B escalation on its own (dependency set, plus grammar revisions are pinned
  to Zed's), and it would put a grammar on `shared`, which is the one crate
  §9's graph keeps language-free. It also solves the problem in the most
  expensive possible place: `lang_rust` is coming and will bring a grammar
  legitimately.
* **Test only what needs no `Location`** — the `Display` strings and the
  empty-shim rows. Rejected: that is nine of thirteen assertions gone,
  including the whole `top1`/`contained` lift, which is the part §6 says the
  obvious implementation gets wrong.
* **Take the normalised pair as the input.** Taken.
  `Agreement::classify(&[DefinitionSite], &DefinitionResult)`, with
  `DefinitionSite::of(&Location)` as the projection. This is *not* a
  testability hack bolted on: §6 already had to stop normalising into byte
  space (see CHANGE-conformance-005 below), so the pair is what the section
  now says both sides collapse to, and the type names it. The tolerance, the
  severity table and the set lift all stay in the one function.

  **Generalise this.** When a type's invariant makes it unconstructible in a
  test, look at whether the function actually needs the *type* or only a
  projection of it. §6 needed `(uri, line)`; it never needed a range. Taking
  the projection is not weakening the predicate, and it is much cheaper than
  the dependency the type would otherwise force.

**The spec contradiction is worth knowing about because it will recur.** §6
says both sides collapse to a set of `Location` — `(DocumentUri, ByteRange)` —
and three paragraphs later says the predicate *reads nothing*. Those cannot
both hold: a wire range's `character` is in the negotiated encoding, and
converting it to a byte offset needs the target document's text. The section
resolves itself, though — it *also* says "`Location.range` … simply is not an
input to agreement". The pattern: where two claims in one section conflict,
check whether the section already says which one is load-bearing before
inventing a resolution. It usually does.

**What §6 genuinely does not decide, and is now `conformance-009`.** What
makes two files "the same module tree". It cannot be answered from `shared`:
the predicate reads nothing and has no handler, so it has two URIs and no
language. `resolution.md` §10.2's declared module tree — the right answer — is
unreachable from here by construction. Provisional reading is "same containing
directory". A future campaign should **not** try to make this better inside
`shared`; the only improvements are a tunable path-prefix depth (needs corpus
data nobody has) or reaching a handler (forbidden by §9's graph). The one
thing worth checking is the consequence noted in the record: as long as §7's
record keeps both `heuristic_locations` and `lsp_locations`, severity is
recomputable from stored rows and this decision is cheap to answer late.

**Two clippy traps, both new.**

* `serde_json::Value` is in `clippy.toml`'s `disallowed_types`, so a test
  helper *returning* one fails the gate even though `json!(…)` used inline is
  fine — the lint is on the written path, not the value. `-> impl Serialize`
  does not work either, because `from_value` wants the concrete type. The way
  through is a `macro_rules!` helper: it builds the value without naming the
  type. `tests/proto.rs` avoids this only by never factoring the helper out.
* `let_underscore_drop` is denied workspace-wide, so the `let _ = (a, b);`
  idiom for silencing unused bindings in a test fails the build. Delete the
  bindings.

**Cost.** One commit. The two non-committing detours were the `Location`
constructor wall and the `Value` ban, both diagnosed and worked around without
a revert; the gate was red once, at step 2, on the second of them.

## ff3e1a40 — the corpus scan exists

**Target: eleven sections, one fact.** `measure_core` and `measure_rust` did
not exist, and neither did any `lang_*`. Twelve gaps across §2, §7 and §9 were
that one fact seen from different sides. They were taken together because none
can be closed alone: every claim about `measure_core` names the four-line
`measure_<lang>` binary, which needs a handler to pass, which is the language
template. Two commits: `864661d` (`lang_rust` + `shared::identifier`),
`7c89f8d` (`measure_core` + `measure_rust`).

**The wall three campaigns hit is gone.** `lang_rust` brings
`tree-sitter-rust`, so a test can now build a `DocumentSnapshot` and a
`Location`. The pin is Zed's `0.24.2` from `../zed/Cargo.toml`; the runtime
stays ours and the two meet through `tree-sitter-language`'s ABI rather than a
version constraint, so they can differ. The registry cache already had the
crate, so this needed no network.

**The `proto` question that shaped the campaign, and the answer.** An LSP
client has to *construct* `initialize`, `didOpen` and definition requests —
which are exactly the messages the shim *reads*. `shared/tests/proto.rs` has
`read_projections_are_never_serialized` asserting that a Read projection has no
`Serialize`, because §8.2's forward compatibility rests on nothing writing an
incoming message back. So adding a derive was not available. The route is a
parallel `Client*` set with `Serialize` only, which is the split
`StandaloneInitializeResult` already makes against the read `InitializeResult`.
**Generalise this**: in `proto`, a new direction of travel is a new type, never
a new derive — and the inventory test makes that a decision somebody has to
write down rather than one that can be slipped in.

**Approaches considered and rejected, so nobody re-derives them.**

* **`serde_json::json!` for the outgoing frames**, avoiding the proto question
  entirely. Rejected: it puts untyped JSON on the one path whose job is to
  agree with the shim, and §8.7 says the wire types live in `shared::proto`.
* **Storing the oracle's answer as a re-serialized `DefinitionResult` in
  `truth.jsonl`.** Rejected for the same rule, and the alternative is better
  anyway: the row keeps the server's `result` bytes as a `RawValue`, so replay
  deserializes the oracle's answer with the same code the shim reads a live one
  with. That is what §6's "must not fork" actually asks for.
* **`ProjectView::candidates` for the file walk `enumerate` needs.** Rejected:
  `ProjectView` is inside the frozen seam. `FileList::paths()` is not, and is
  the honest home — a *handler* reaches files through `candidates`, which
  filters and ranks; `measure_core` wants the whole list precisely because it
  is not searching.
* **`Serialize` on `DocumentVersion` and `Stratum`.** Rejected: both are
  vocabulary/seam types, and a wire spelling is not a reason to reach into the
  seam. `serialize_with` and a `StratumName` newtype cost three lines each.

**`servers.toml` has no parser and there is no TOML crate.** Escalated as
`conformance-010`; the provisional choice is a ~90-line reader for the two
shapes the manifest is documented to have, confined to one private module so
answering it the other way is a deletion. Note that `servers.toml` does not
exist and is in no loop's write list, so the reader has nothing to test
against — a fixture is a follow-up campaign the day the file lands.

**Three clippy traps, one of them new and expensive.**

* `panic!`, `expect` and `unwrap` are denied in **free functions** in
  `tests/*.rs` — `clippy.toml`'s `allow-*-in-tests` reaches only `#[test]`
  bodies. The findings already said this; what is new is that a fixture
  builder is exactly such a free function, so a file-level `#![expect(...)]`
  with a reason is the way through, not restructuring.
* `integer_division` fires on `len() / 16`. `div_ceil` passes.
* `unreachable_pub` is denied workspace-wide, so a crate with private modules
  needs `pub(crate)` on everything not re-exported. Writing a crate with `pub`
  throughout and fixing it afterwards cost a whole build cycle; write
  `pub(crate)` from the start and promote what the lib root re-exports.

**A determinism test must exclude `heuristic_latency_us`.** §7 says so in
words — "the one field in the record that a replay does not reproduce exactly"
— and the test failed the first time for exactly that reason. Masking anything
else would be hiding a bug; masking this one is what leaves the claim testable.

**What was left, deliberately.** The handler-reported half of §7's record
(`margin`, `considered`, `stages`, `stage_us`, `bytes_scanned`, `files_parsed`,
and `stratum_prior` distinct from `stratum_final`) is written at empty values,
because `Outcome` carries none of it and widening the seam is Class B. The
record's *shape* is already §7's, so that campaign changes values rather than
columns — which is the cheap order to do it in.

**Gate, again mid-intervention.** `harness/hj` was modified in the working tree
by a concurrent human session, which makes `harness/gate conformance` (no
`--rev`) unusable — it inspects the pending commit including untracked and
unstaged paths. `harness/gate conformance --rev <sha>` after committing only
one's own paths is the way through, and it is worth reaching for immediately
rather than diagnosing twice.

## de2706af — the wiring edge: `heuristic_jump` -> `lang_rust` -> `driver::run`

Targets: `#the-dependency-graph`'s three gaps and `#adding-a-language`'s one.
All four are the same claim seen from four sides — an edge in §9's graph — and
the missing edge was the one that makes the shipped binary contain a handler.

**What was actually wrong.** `main` resolved a `Config`, logged it and exited.
`lang_rust` had existed since `864661d` and was in nothing that builds for
release, so the crate whose stated job is "the single place the language list
is enumerated" enumerated nothing, and `Registry` — written two campaigns
earlier — had no construction site outside a test. Fixing it is twenty lines.
The reason it was worth a campaign is everything around it: §9's printed `main`
did not compile against the code, and nothing failed when a language was
missing from the list.

**`driver::run` is a stub, and that was the judgement call.** `shim.md` §13
puts `run()` in `driver.rs` beside the thread wiring and the child spawn; there
is no transport, no codec and no actor, so the body reports the resolved
configuration and returns `Ok(())`. Writing it anyway is what gives the
registry an owner — a `Registry` built and dropped in `main` would satisfy the
same manifest scan while linking nothing that matters. The alternative
considered was leaving `main` self-contained until the transport exists, which
keeps §9's `main` uncompilable for however many campaigns that takes, and makes
the eventual transport campaign move the language list as well as add the loop.

**§9's printed `main` could not be satisfied as printed** — CHANGE-conformance-009.
It named `HandlerRegistry`, which appears in no other document, and passed
`Cli::parse()` to a crate that `deps.md` §11 and `shim.md` §13 both keep `clap`
out of. Note that `Config::from(cli)` is *not* available as the crossing:
`From` is foreign and `Config` is `driver`'s, so an `impl From<Cli> for Config`
in `heuristic_jump` is an orphan-rule violation. The snippet elides the
resolution rather than printing a method pair that the code does not have.

**The test that is worth copying.** The compiler checks the edge for a language
that is *named* and cannot check the one that is not: a `crates/lang_python/`
added and never wired builds perfectly, ships nothing and reports no error —
the first sign is a metrics table with no Python rows.
`the_language_list_is_enumerated_in_heuristic_jump` reads the workspace members
and requires each `crates/lang_*` in both the manifest and the registry
literal. It scans for `<crate>::Handler::new()` rather than the crate name,
because a language that reached the manifest and a doc comment but not the
registry is precisely the failure.

**`vec![Arc::new(h)]` needs the `as Arc<dyn LanguageHandler>`.** Element
coercion inside `vec!` did not fire from the parameter type, so `heuristic_jump`
names `shared::LanguageHandler` — which it needs `shared` for anyway, since
§9's own `main` returns `Result<(), shared::Error>` and `From`/`Into` cannot
launder it.

**The authoritative-list gap was four campaigns old and cost nothing to close**
— CHANGE-conformance-010. `shared` has declared `tracing` since `ProjectView`
first logged; §9's list, in the sentence calling itself authoritative, did not
have it, and `deps.md` §9 calls `tracing` unavoidable because `rope` depends on
it. Class A, and deliberately argued as such in the changelog: it selects no
dependency and widens nothing. The test is a **subset** assertion — §9 lists
`rayon`, which `shared` will not declare until `ProjectView::scan` exists, and
`deps.md` §14 has each dependency arrive with its first user. An equality
assertion would have failed on the intended state and been "fixed" by adding
`rayon` with no user.

**What the campaign could not close: `similarity` has no `shared` edge.** §9
draws one and its bullet asserts it; `crates/similarity/Cargo.toml` declares
four crates and none of them is `shared`, and nothing in the ported code names
a `shared` type. `crates/similarity/**` is denied to every loop in every phase,
so the manifest cannot be fixed here, and deleting the edge from §9 is a spec
edit toward the code on a layering claim. Escalated as `conformance-011` with
*neither* option taken provisionally, which is the unusual shape: there is no
site this loop may write that the answer would change, so the record is the
tag. Do not spend another campaign rediscovering this — it is a human write or
a human ruling.

**Splitting one gate-green tree into two commits.** Both experiments were
written before the first commit, and the second commit's test asserts a list
the first commit's spec did not yet have. Lifting the second test out to a
temp file, committing, then splicing it back is three minutes and leaves each
commit's tests matching its own spec state. Worth doing rather than collapsing
to one commit, because the audit reads commits.

## 0faab934 — `ProjectView` grows `candidates`, `scan` and `parse`

Target: `#the-trait[93f2f340e6]`, the section's only gap. Three methods, one
object, one campaign.

**The gap's stated blocker was not one, and this is the transferable part.**
It read "their implementations need the parse LRU and the bounded pool that
`shim.md` owns" — which is what the *module doc* said, which is what the
previous campaign wrote. It was wrong in the same way twice: `conformance-005`
had already been answered "no cache, and `CLAUDE.md` line 112 decides it —
no new caching or indexing until the corpus harness shows the change is worth
it", and that ruling covers the parse LRU and the pool as squarely as it
covers the read cache. Once both are dropped there is nothing left to be
blocked on. **Before believing an audit gap that names an absent subsystem as
its blocker, check whether a decision record has already deleted the
subsystem.** A gap can be a fossil of a doc comment.

**What `scan` cannot be.** §3 prints `-> ScanOutcome`. It cannot: `read` fails
on an expired deadline, and §4 spent most of a section removing every way to
report a partial scan (no `TruncatedEmpty`, no marked row, no flag), so there
is nowhere for the expiry to land except the `Err`. CHANGE-conformance-011.
Worth noticing that the contradiction is between §3 and §4 of the *same*
document and survived nine thousand lines of review — the printed signature
block is the least-reread part of a design doc.

**`parse` has no route to a grammar and this is a real hole in the seam.**
`resolution.md` §3's signature was written when `ProjectView` was a trait
implemented in `driver`, which holds the registry and therefore every
`handler.grammar()`. §3 then moved the struct to `shared` — correctly, and for
a stated reason — and nobody re-derived where the grammar comes from.
`conformance-012`, provisionally a constructor parameter. Do not "fix" this by
adding a parameter to `parse` without reading the record: the reversal cost is
one `ProjectView::new` call site today versus one per resolution stage per
language crate forever, and that asymmetry is the whole argument.

**Approach abandoned: chunk-wise matching with a carry buffer.** `FileText` is
`Disk(Arc<str>)` or `Open(Rope)`, and a rope is chunks. Matching per chunk
needs a carry of the previous chunk's tail so a token spanning the boundary is
not dropped, plus UTF-8 boundary care in the carry, plus a line counter that
survives the join. All of it is invisible when wrong — it drops definitions
only in files large enough to have several chunks, which is exactly the corpus
half nobody eyeballs. Replaced by: take the first chunk; if there is no second
one, match against it directly (every disk read is one chunk, so this is the
whole cost today); otherwise join and match once. Eight lines, no boundary
cases. If open documents ever make the join hot, that is a benchmark, not a
guess.

**Word-boundary matching reuses `identifier.rs`'s per-character predicates.**
Not for tidiness: that module's own doc says two implementations of "what is
an identifier" that agree today become a *definitional* disagreement that gets
measured as a resolution failure. `measure_core` enumerates positions with one
rule and a scan finds tokens with the other; if they diverge, the corpus holds
positions the scan cannot hit and the miss is attributed to the handler.

**`ScanRequest::new` is fallible on purpose.** A request for `foo(` scans every
candidate at full cost, matches nothing, and abstains `NoCandidates` — which
is a claim about the *project*, not about the query. Rejecting a non-identifier
literal at construction is the cheapest place to keep that reason honest.

**The seam test had to be widened, and the shape of the widening matters.**
`neither_driver_nor_shared_depends_on_a_language` scanned every line of the
manifest for `tree-sitter-`, so `shared` could not dev-depend on a grammar —
and without one, `ProjectView::parse` cannot be tested at all, since
`tree_sitter::Language` has no constructor outside a grammar crate. It now
checks `[dependencies]` for grammars (the reading `measure_core`'s own
manifest already wrote down) and keeps `lang_*` banned in *every* table. Do
not relax the `lang_*` half: a language crate in a test lets `driver` or
`shared` be written against one language's behaviour and still pass.

**Gate red once**, on `redundant_clone` firing at
`FileList::enumerate(std::slice::from_ref(&root.to_path_buf()))`. It is a
`warn` lint in the workspace table but the gate compiles with `-D warnings`,
so a nursery lint is a hard failure there. Bind the `[PathBuf; 1]` to a local
instead.

**Not attempted, and each is its own campaign:** `DefinitionHints` (wants
`regex`, which is not a dependency, and §9 makes it a phase-3 extraction),
`rayon` on the scan, the driver's open-document map that would make
`FileText::Open` reachable, and `resolution.md` §3's "each file is read at
most once", which `conformance-005`'s ruling says is wrong as written and
which the answer explicitly hands to a later campaign as a Class A edit.

## b62bf25e — `core.md` §8.4, the wire conversion and the premise under it

**Target:** both gaps of `#84-location-is-byte-based`, taken together because
one is the justification the other rests on. Confirmed, three commits, no
reverts.

**What was actually blocking it: nothing.** Every piece existed —
`WirePosition::encode`, `ProjectView::{read,lookup}`, `Location::{uri,range}`.
The conversion had never been written because `dispatch` had no encoding
parameter and nobody had a reason to add one. This is the third campaign in a
row where the gap's apparent blocker dissolved on inspection; the pattern is
now strong enough to lead with.

**The dead end I nearly walked into, and why it was not one.** The plan was to
put the end-to-end test in `crates/heuristic_jump/tests/`, on the reasoning
that it is the only crate depending on both `driver` and a language, and that
`driver` may not name `tree_sitter::Language` — which an `impl
LanguageHandler` must, to write `fn grammar`. That reasoning was wrong and
`deadline.rs`'s own module doc had been repeating it since campaign e3b8dbf4
("there is no handler double in phase 1a"). `seam.rs`'s grammar ban reads
`[dependencies]` only, and its doc comment says why in as many words: "§9's
graph is the graph the shipped binary has, and a `[dev-dependencies]` grammar
is not in it." `shared` and `measure_core` were already relying on exactly
that. So `driver` took `tree-sitter` + `tree-sitter-rust` as dev-deps and the
test sits next to the code it tests. **Do not re-derive this.** Handler
doubles are available in `driver`, `shared` and `measure_core`; what stays
banned in every table, including dev, is a `lang_*` edge.

**Rejected: a public `Answer::new(outcome, wire)`.** It would have made
`deadline.rs` trivial to update and would have thrown away the only property
worth having — the wire half is *derived* from the byte half, so the two
cannot disagree. What made privacy affordable is that the no-locations case is
provably consistent: `Answer::without_locations` returns `Option`, `None` for
a commit that has locations. Every test that needs a `Decided` without a
document goes through it.

**Rejected: asserting the encoding by reading the character out of a
`WirePosition`.** There is no accessor and there should not be — §8.3 makes
the type inert on purpose, and `line()` exists only because §6's predicate
needs a row. Serialising to JSON would have meant `serde_json` in `driver`'s
dev-deps and a fixture-path-dependent string. What works instead: round-trip
through `resolve(encoding, &rope)` and assert the *three encodings disagree
with each other*. That needs a four-byte scalar on the definition's own line —
a `/* 𝄞 */` prefix gives columns 11 / 9 / 8, three numbers no confusion
produces by accident. A fixture with the astral character on a *different*
line proves nothing, because the definition's column is unaffected.

**§8.4's economics do not hold and that is recorded rather than fixed.** The
section prices the conversion at nearly free because "every target file's text
is already in the view's cache". `conformance-005` refused that cache. So each
closed target file costs a read, and several locations in one file cost
several. Adding memoisation here would be that ruling reversed on the same
missing evidence, so it is a comment at the loop instead.

**Not attempted, and separate campaigns:** the read-free conversion §8.4
actually describes needs a second `WirePosition` constructor taking a line
plus that line's text — a §8.3 change, since §8.3's whole claim is that there
is exactly *one* constructor and it takes a whole `Rope`. Also untouched: the
open-document map that would make `target_text` return `FileText::Open`, and
therefore make `ProjectError::Unresolvable` harder to reach.

## e017e797 — `core.md` §2, the snapshot path through `driver`

**Target:** three gaps in three sections — `#2-document-snapshots`,
`#snapshots-are-o1`, `#text-and-tree-can-never-disagree` — every one of them
saying the same thing in different words: nothing in `driver` ever realised a
`SnapshotSeed`. Confirmed, three commits, no reverts. Fourth campaign in a row
where the gap's stated blocker was not one.

**The parse *can* be interrupted, and the granularity is the interesting
part.** tree-sitter 0.26 has `ParseOptions::progress_callback`, and the C side
calls it once per **100 parser operations**
(`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `src/parser.c:81`) — an operation
count, not a byte count, so there is no document size at which interruption is
*guaranteed*. Measured: a 46 KB generated file is reliably abandoned; a
60-byte one parses to completion with the deadline already expired. Both are
asserted, the second deliberately — if it ever fails, tree-sitter has become
more eager and that is a decision to record rather than a break to fix.

**The error arm mattered more than the parameter.** A parse abandoned on time
returns `HandlerError::DeadlineExpired`, not `ParseError::NoTree`, because
`dispatch::classify` already maps exactly that one class back to an
abstention. Putting it in `ParseError` would have compiled, passed, and logged
every large file in every corpus repository as a *handler failure* — the one
distinction §7's record exists to make, destroyed by picking the wrong
variant. Detected only because the two arms were written side by side.

**Rejected: `dispatch(&mut cache, ...)`.** The obvious way to make "core
caches the tree" true is to hand the cache to the wrapper. It is wrong for the
reason the whole design is built around: the cache is `core`'s state and
`dispatch` runs on a worker, so sharing it is a lock in a codebase that has
none. §2 already says the right answer — "getting the result back to `core` is
explicit" — so `dispatch` returns `Completed { dispatched, parsed }` and the
owner does the write. **The channel is still missing** and that is the honest
gap: `Parsed` travels by return value because `core` and the worker pool do
not exist. The ownership is already right; only the wiring is absent.

**`Parsed` is returned even when the query failed or expired.** It is tempting
to fold it into `Dispatched::Decided`, and that is wrong twice over: the parse
succeeded independently of what the handler decided, and the query most likely
to be asked again in a moment is precisely the one that just abstained on its
deadline.

**The property that needed inventing: `ParseKind`.** An incremental reparse
and a full parse produce the *same tree for the same text* — that is what
makes the optimisation safe, and it is also what makes "the cached tree is
actually reused" unobservable. Asserting on the resulting tree proves nothing;
asserting on `TreeCache::version` proves the cache has an entry, not that the
seed carries it, so a `seed` that always returned `fresh` would have passed. A
two-variant `ParseKind` on `SnapshotSeed` makes the branch assertable, and
breaking `TreeCache::seed` was verified to fail the test before it was kept.
Reach for this shape whenever a claim is about *which path ran* rather than
about the answer.

**`TreeCache::insert` refuses a tree older than the one it holds**, which is
not in §2. Dispatch is parallel by design, so two workers can be realising
seeds for the same document at once and the one that finishes last is not the
one parsed from the newest text. Overwriting would leave `base` at a version
the edit log no longer describes, and tree-sitter's incremental parse is only
correct when the edits handed to it are. Treated as an implementation
invariant rather than a spec change: §2 says the cache makes the next query
warm, and a cache that can go backwards does not.

**CHANGE-conformance-013, and the shape being watched for.** §2 printed
`realise(self)` while the paragraph above it said the parse happens "inside
the deadline", and `SnapshotSeed`'s fields gave it no route to one. Fixed by
adding the parameter, which makes the section *harder* to satisfy — before the
edit, a `realise` that ignored deadlines conformed. That is the direction a
spec edit made by the implementing campaign should always go; the changelog
entry says so plainly and expects to be asked.

**Not attempted, deliberately.** `heuristic_jump`'s `run` still wires none of
this: there is no transport, no codec and no actor, so `TreeCache` has no
owner outside the tests. Building that owner is the driver-cluster campaign
(`#4-project-file-enumeration`, `#86-modelling-errors`, `#both-sides-are-sets`,
`#10-testing[ddadbddae0]`), and it is one campaign per gap at least.

## Campaign 7a30ee1a — `#7-observability-and-the-corpus-scan[c4505d900b]`

Confirmed in one experiment. The gap was the seam widening `record.rs`'s
`HandlerReport` had already named and deferred: `Outcome` carried `locations`,
`confidence` and one `Stratum`, so §7's `margin`, `considered`, `stages`,
`stage_us`, `bytes_scanned`, `files_parsed` and the two strata had no route
across. `Outcome`'s arms now carry `Strata` and `Trace`
(`state/decisions/conformance-013.md`, Class B on the frozen seam).

**The section's other gap, `[10d2239070]`, was entirely stale** — it reads "no
record type, no serialization and no emission site exists" against a tree where
`measure_core/src/record.rs` is §7's record, `replay::write_records` serializes
it, and `measure_core` is a workspace member. Half a campaign's budget goes on
re-reading gaps like this. The findings file has said "check the code before
believing a gap" for three campaigns now and it is still the highest-value line
in it.

**What cost time, and will again: `result_large_err`.** Widening `Outcome` with
six inline fields pushed `driver`'s `Dispatched` past clippy's 128-byte
threshold, and the lint fires at three `Result<_, Dispatched>` signatures in
`dispatch.rs` — none of which the diff touched, so the error reads as unrelated
breakage. The fix that keeps the call sites honest is to box *inside* the new
type (`Trace(Option<Box<TraceParts>>)`), never at the use site: the seam type
stays one pointer wide, and `None` means the `NotAnIdentifier` path — the
commonest abstention there is — allocates nothing for a channel it never writes
to. **Any future widening of a seam enum will hit this**, because `Dispatched`
is now close to the line again.

**A bulk edit through `python3` does not get formatted.** The PostToolUse hook
formats what `Edit`/`Write` touch; a script that rewrites four test files leaves
them unformatted and the gate fails at step 1 before compiling anything. Run
`cargo fmt -p <crate>` on every crate the script touched, immediately.

**Abandoned: an in-process assertion on the table's coverage/precision split.**
§7 says coverage is reported on `stratum_prior` and precision on
`stratum_final`, so `Table::observe` now puts the coverage counters in the prior
row and the agreement counters in the settled one, and `precision()`'s
denominator becomes the three agreement counters rather than `committed` (on a
refined query, `committed` is the other row's number). No test asserts it and
none can from where the code sits: `Table` is `pub(crate)`, and `measure_core`'s
`report` writes the JSON with `std::io::stdout().write_all`, which bypasses
cargo's test capture — so a test cannot read back the table it just caused.
Verified by eye against the JSON `cargo test` prints: 14 queries and 14 commits
under `explicitly_imported`, 14 `match_top1` under `ambiguous_name`.

**If a future campaign wants that assertion**, the cheap route is a
`pub(crate)`-to-`pub` on `Table` plus a `replay` that returns its table rather
than only printing it — not a stdout capture, and not a second table
implementation in the test, which would fork the metric §7 exists to keep
single.

---

## Campaign 571b1bb0 — `core.md#4-project-file-enumeration`, all three gaps

Confirmed. Two commits: `FileListCache` in `driver` plus a small widening of
`shared`, then routing `driver`'s existing dispatch tests through the cache.

**The stated blocker was, for the fifth or sixth campaign running, not one.**
All three gaps read as "there is no owner" / "blocked on `shim.md`'s routing",
and none of that mattered. The owner is a plain struct with `&mut self`
methods and a named background thread; it needs neither the actor loop, nor
the transport, nor the codec. The rule from the previous findings held exactly:
*ask what remains after removing the named blocker, and it is usually ordinary
code*. This is now the sixth data point and it should be treated as the
default reading of a `found:` that names a missing subsystem.

**What made the three claims mechanical rather than asserted.** This is the
part worth copying, because two of the three could easily have been written as
a comment plus a test that passes vacuously:

* *"never reads the payload"* — `watched_files_changed()` takes no argument.
  There is nothing to read, so no test is needed and no future edit can quietly
  start reading. A `watched_files_changed(&DidChangeWatchedFilesParams)` with a
  comment saying it ignores its argument would have been the same claim and
  worth nothing.
* *"`NoCandidates` specifically, not any abstention"* —
  `AbstainReason::file_list_evidence` is an exhaustive match, and it had to go
  in `shared`: the enum is `#[non_exhaustive]`, so the same match written in
  `driver` needs the wildcard arm `CLAUDE.md` bans, and that arm would classify
  the next variant as inconclusive instead of failing to compile. **This is a
  general rule for the seam's `#[non_exhaustive]` enums** — a consumer that
  wants to case-split on one is asking for a method on it, not a `match`.
* *"the two triggers share one debounce rather than one each"* — both triggers
  call one private `mark_stale`, and the debounce is one `Refresh` field. There
  is nowhere for a second timer to live, so the claim is structural.

**Both mutation-checked before committing**, which is the only reason to
believe the tests. Flipping `file_list_evidence` to return `Stale` for every
variant fails `no_candidates_is_the_only_abstention…`; making `install` leave
the state `Pending` fails `the_two_triggers_share_one_debounce…`. Do this — a
test written against code that already passes proves nothing about whether it
would notice.

**Abandoned: a test that a failed rescan leaves the list in hand.** It cannot
be written, because `FileList::enumerate` never returns `Err`: an unreadable
entry is logged and skipped, and a root that does not exist walks to an empty
list. So the failure mode is not "the walk errors", it is "the walk succeeds
and returns nothing", and *that* one does replace a good list with an empty
one. That is the posture `enumerate`'s own comment already takes (a partial
walk is the same failure mode as a stale one, and both cost recall rather than
correctness), so it was left alone rather than special-cased — but a future
campaign that wants a rescan to refuse to install a suspiciously empty walk
should know the `Err` arm in `install` is close to dead code, not the guard it
looks like.

**Deliberately not done: constructing the cache in `driver::run`.** `run` has
no roots — workspace folders arrive in `initialize`, which nothing handles —
so a cache built there would be over `vec![]`. The cache's real caller is the
`core` actor, and that is `#both-sides-are-sets`' gap. What *was* done instead
is `FileListCache::view`, making the cache the only route to a `ProjectView`
inside `driver`, and moving the two dispatch suites onto it; that is a real
caller on the real query path rather than a stub.

**Two mechanical notes.**

* `crossbeam-channel` arrived with its first user, as `deps.md` §14 wants. It
  is on §9's graph annotation for `driver` already, so this narrows the
  `#the-dependency-graph[7f3a1bb4ec]` divergence rather than widening it.
  `clippy.toml` bans blocking `Receiver::recv`; the scanner thread needs it and
  carries an `#[expect]` saying why (it owes no answer, and the channel closing
  is its shutdown signal). `unbounded` is banned too — both channels are
  `bounded(1)`, which is sound because `Refresh::InFlight` keeps at most one
  walk outstanding.
* A test that needs time to *move* cannot use `FrozenClock`. `DrivenClock` is a
  base `Instant` read once from `SystemClock` plus an `AtomicU64` of elapsed
  milliseconds — atomic rather than a cell because `Clock` is `Sync`, and not a
  lock. Copy it; every debounce, health-probe and report-window test will want
  the same thing.

## 576c2c6f — `core.md#the-command-line[d2a209c7a8]` — confirmed

**The gap in one sentence.** `Table` held `latencies: Vec<u64>` and `render`
printed `heuristic latency: p50 … p99 …` in both formats, so §7's "same corpus,
same commit, same table, byte for byte" was false of the artifact the gate
consumes. Removed the field, moved the run's own wall clock to `tracing`, made
the rendered artifact a value (`measure_core::replay_table`) so a test can hold
it, added `the_printed_table_is_byte_identical_across_runs`.

**Nearly all of the campaign was spent establishing that deleting the
percentiles trades nothing off, and that is the part worth recording**, because
the same shape will come up again: the fix was fifteen minutes and the licence
to make it was two hours. Three things had to be true at once, and if any one
had failed this was a Class B escalation rather than a Class A-free fix:

* §7 already owns the latency numbers. "This single record type covers …
  latency percentiles" (core.md:880) — the *record*, not the table. And the
  record's `heuristic_latency_us` is per query, so it is strictly more than a
  global p50/p99: `loops.md` §10 asks for **per-stratum** percentiles, which
  the deleted global pair could never have produced.
* Nothing consumes them. `grep heuristic_latency_us harness/` is empty, so the
  dashboard and the metrics row were not reading the field that was removed.
* `loops.md` §9's "`measure replay` reports its own wall clock" is about a
  number read as a trend, not about the gate's artifact. It survives on the log
  stream, and that is where it now goes.

**The spec is not self-contradictory here, which is worth saying explicitly**
because it reads as though it is. §"Two modes" calls `heuristic_latency_us`
"the one field **in the record** that a replay does not reproduce exactly", and
§"The command line" calls the table byte-identical. Both hold simultaneously
once the clock is not in the table. A campaign that reads only one of the two
will reach for `state/spec-changelog.md`; there was nothing to change.

**What the existing test was doing and why it was not enough.**
`replay_is_deterministic_byte_for_byte` compares two `--records` files with
`heuristic_latency_us` masked. That is a real property and it stays. It is not
this section's property: the records file is not the table, and it needs a mask
to pass at all, so it could never have caught the p50/p99 line.

**`replay_table` is public for a reason that will recur.** `measure_core::run`
writes the table into a raw `std::io::stdout()` handle, and cargo does not
capture raw stdout writes — only `println!`. So an in-process test cannot see
the table at all unless something returns it. Any future assertion about what
`measure` *prints* needs the same treatment; do not spend a second campaign
rediscovering that the output is invisible from `tests/`.

**Mutation-checked before committing**, per the standing rule: re-added a
`SystemTime::now()` nanos field to `Report` and a `mutation clock:` line to
`as_text`, ran the test, watched it fail on `Table` and then (with the text
mutation removed) on `Json`. Both halves fire. The test loops over both formats
deliberately — `--format json` is the one the harness consumes, so a check that
only covered the text table would guard the wrong artifact.

**Also asserted the table is non-empty** (`once.contains("unimplemented")`)
before comparing. Two empty strings are equal; without that line the test would
pass against a corpus that produced nothing, which is exactly the failure a
fixture change would introduce silently.

## 2fdda442 — `core.md#the-oracle-is-the-server-being-proxied[eb6f4618da]` — confirmed

**The gap in one sentence.** `ServerId` was a newtype nobody built: `new` had
zero callers, and all nine `ServerProfile`s in the workspace were the literal
`ServerProfile { id: None }` — including the one on `measure replay`'s real
query path, which had `--server <name>` in hand and threw it away.

**Why it had stalled, and it is not the reason the audit implies.** The audit
points at `driver` never mapping `ServerCommand::program()` to an id, which
reads as "blocked on the driver actor". It is not. The actual obstruction is
that `ServerId(&'static str)` cannot be built from a runtime string without a
table of the servers that exist, and writing that table is a decision nobody
had made. Once you see it as "where does the canonical list live", the answer
is already in the repository: `servers.toml`, which is deliberately in no
loop's write list precisely because it names the oracle each loop is scored
against. So `ServerId::KNOWN` is a compile-time copy of its eight table keys,
and `driver/tests/oracle.rs` asserts the copy against the file in both
directions. That two-copies-plus-a-test shape is normally the thing `core.md`
refuses ("a directory tree in two documents will disagree with itself") — it is
justified here only because the test makes the disagreement a build failure,
and it is worth saying that out loud in any future case that looks similar.

**The spec claim is wrong in a way worth knowing about, and I did not change
the document.** §7 says the id is "resolved from the child's **command name**".
Four of the eight servers in `servers.toml` are launched as
`node .../<server>/.../langserver.index.js --stdio`, so their command *name* is
`node`. A resolver reading only the program would fail on exactly the servers a
profile is most likely to be wanted for. `from_command` therefore matches over
the path components of every word of the argv, which strictly subsumes reading
the program and needs no spec edit — "command name" broadened to "command line"
gives up nothing. The one thing it does introduce is ambiguity, so two
*distinct* matches resolve to `None` rather than to whichever came first;
`basedpyright` ships npm bin names `pyright` and `pyright-langserver` beside
its own, so a tree holding both is ordinary rather than contrived.

**What made it structural rather than asserted.** `ServerProfile.id` went
private behind `standalone()`, `proxying_command()` and `proxying_named()`.
With a public `Option<ServerId>` there are three states and only two of them
are real: no oracle, an oracle we cannot identify, and — the one that was
actually in the tree — a call site that *knows* its server and passes `None`
anyway. The third stops being expressible when the constructor takes the name.
This is the same move `Location` (conformance-004) and `Strata` already make in
this seam, which is why it was not escalated as a Class B seam change: private
fields with named constructors is the established idiom here, and it narrows
rather than trades.

**Six mutations, all fire.** Drop `vtsls` from `KNOWN`; make `from_command`
read the program only; drop the ambiguity rule so the first match wins; make
`matrix()` return nothing; make `from_name` return the first entry regardless.
Each kills a different assertion, and the `matrix.len() >= 8` guard is what
stops an empty scan from passing the whole loop vacuously — the same trap as
comparing two empty tables in 576c2c6f.

**A process mistake that cost real time, and it will recur.** The mutation loop
restored files with `git checkout <path>` — which restores from HEAD, not from
before the mutation, so it silently deleted the campaign's own uncommitted work
in that file. The next three mutations then "passed" by failing to compile, and
the empty output looked like a run. **Use `cp` to a backup, and read the actual
output rather than the absence of a FAILED line.** Better still: commit the
green state first, then mutate.

**Found and deliberately not fixed — the next campaign should know.**
`measure_core`'s `manifest::parse` (`corpus.rs`) cannot read the real
`servers.toml`. It expects a `[[server]]` array of tables with `name` and
`language` keys and one `key = value` per line; the file has `[server.<name>]`
tables with a `languages` list and `command` arrays spread over several lines.
The first `[server.rust-analyzer]` line has no `=`, so `parse` returns
`ManifestMalformed` and `measure collect --server <anything>` cannot resolve a
server at all. Nothing catches this because no test reads the real file — the
pipeline fixture writes its own truth with server `"oracle"` and never calls
`resolve_server`. `driver/tests/oracle.rs` now reads the real file, but with
its own scan, on purpose. This belongs to `#where-the-corpus-lives` /
`#two-modes-collect-and-replay`, and it is a genuine open gap that the audit
has not noticed.
