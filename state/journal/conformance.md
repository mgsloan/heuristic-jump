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
