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
