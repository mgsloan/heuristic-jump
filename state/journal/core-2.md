# Journal — core, worker 2

Written for a session that will not remember this one. `state/journal/core.md`
is the other worker's, and neither is a summary of the diff.

## 37a6d098 — `rope-modifications.md` §4 and §7, thirteen unjudged sections

**Take the whole document if the sections are all about one sweep.** Thirteen
sections, one 609-line document read once, one crate. The alternative — one
campaign per section — would have re-read `rope-modifications.md` thirteen
times, and the reading is the whole cost. The named shared context was
`design/rope-modifications.md`, `vendor/rope/src/{offset,point,point_utf16,
chunk,rope}.rs` and `vendor/rope/tests/newtype_api.rs`, and every section
needed all of it.

### The thing I got wrong first, twice, and would get wrong again

**A check whose failure does not compile is not a check, and I nearly shipped
three of them.** The previous campaign's journal says this about `[lints]` in a
vendored manifest; it is more general than it looked there, and it recurs
because the *natural* way to write these assertions is the way that cannot
fail:

- `use gpui;` planted in `chunk.rs` produces no test result at all — the crate
  does not build, nextest reports an error rather than a failure, and grepping
  the output for "FAIL" finds nothing. I scored it as passing for about a
  minute.
- `impl std::ops::Add for LineIndex` *does* compile, and walked straight
  through the operator-table scan, because I matched the trait by its whole
  path instead of its last segment. The control caught it. Without the control
  the scan would have been committed with a hole exactly the shape of the
  mistake it exists to catch.
- `pub fn new(row: impl Into<LineIndex>, ...)` compiles, keeps all 54 call
  sites, and passes both bare-primitive scans. It is the `u32`-shaped hole §4
  rejects by name. Nothing held it until E5.

The rule I would write for myself: **run the control before writing the commit
message, and if the control cannot be made to fail, say so in the test's own
doc comment rather than deleting the test.** Where the compiler is the real
enforcement, the assertion still records that the rule was deliberate — the
compiler's enforcement is incidental and holds only while the mistake happens
to be a build error.

### Floors do not hold lists

E6 parses §4's conversion table out of the document and asserts each function
still exists. My first version asserted only `named.len() >= 30`, and I wrote
in the doc comment that it caught a row being deleted to make the code fit. It
did not — I deleted a row and it passed. A floor catches the parser breaking
and nothing else. The fix was to transcribe the table into the test, which
makes the test a second copy on purpose: a row is one claim about one function,
so removing one means removing it twice. Cost: 31 names. Worth it, and it is
the same shape as the operator-table inventory in E2.

### Settle a count against upstream rather than choosing

§7 and `vendor/README.md` both said `#[gpui::test(iterations = N)]` was on
**nine** functions. There are eight `seeded` sites. That is indistinguishable
by reading from a dropped test, and the dropped test would have been the
serious outcome, since upstream's tests are the *only* verification of the
sweep.

`curl` to `raw.githubusercontent.com` at the pinned revision works from this
environment, and settled it in one turn: nine `#[gpui::test]`, of which eight
carry `iterations` and one is bare and takes no `rng`. All nine survived; the
bare one is a plain `#[test]`. **Fetch upstream when a claim is about
upstream.** I nearly wrote a decision record instead, which would have sat
open for days waiting for something a `curl` answers.

### Approaches considered and not taken

- **Compile-fail tests for the absent operators.** `trybuild` is a new
  dependency and would need asking. Scanning the impl headers covers the same
  claim, needs nothing, and additionally holds the *positive* half of the table
  — which `trybuild` would not.
- **`std::fs::read_dir` to enumerate vendored sources**, in `seam.rs`. It is a
  `clippy.toml` disallowed method (gitignore semantics), and `driver`'s tests
  *are* linted where `vendor/`'s are not. Walking the crate root's `mod`
  declarations is the substitute and is the better answer anyway: a `.rs` file
  the root does not declare is not compiled, so a patch hiding in one is not a
  patch to the crate.
- **Putting E8's vendoring check in `vendor/rope/tests/`.** It is a `core.md`
  §9 claim about `vendor/` as a whole, and `seam.rs` already holds every other
  repository-shaped assertion. Splitting them by which crate the files happen
  to sit under would leave the next campaign looking in two places.

### Two things that surprised me about the repository

- **The gate now runs the vendored crates.** `conformance-003` is accepted and
  `state/phase.toml`'s `crates` list holds `rope` and `sum_tree`; only lint and
  fmt are withheld. `vendor/README.md` still said the opposite, and the audit
  had it as a minor. Fixed in E5. `cargo nextest run -p rope` is still the
  quicker iteration loop, but it is no longer the only thing that runs them.
- **`rustfmt --edition 2024 <file>` is the right per-file sweep.** The
  PostToolUse hook formats on `Edit`, but not on a `perl -0pi` or a `printf >>`,
  and `cargo fmt -p rope` would reformat all of upstream's text.

## e797a506 — the measure pipeline, five sections, an audit that was stale

**The assignment's four gaps were all already closed.** Not one of them needed
work. `grammar_pin` reads the lockfile, `Provenance::drift` destructures every
field, `Table` holds no `Duration`, `Table::template` returns three states —
and each had a test. `ff3e1a40` closed `partial` and its follow-on commits
(`55de8a2`, `933a4aa`, `24e79cd`, `6c65912`) landed at 21:18–21:34Z against
audit stamps of 20:56Z and 09:16Z.

The previous findings file already said "verify a gap against the code before
working it; the audit lags by hours", and it was right, and I still nearly
spent the campaign on the gap list because the *assignment* named those ids.
**Compare `last_audited` against `git log` for the crate before deciding what
the campaign is.** Two commands. The stamps are per section in
`state/audit/core.toml`, and `git log -- crates/<crate>/` gives the other side.
Note the timezone: stamps are UTC and the commits are −06:00.

### What to do when that happens

Do not close. The sections are not clean because you looked at them; they will
go clean at the next audit whatever you do, and a campaign that only *verifies*
has produced nothing the repository keeps. **Read the five sections instead of
the five gaps** and find the claims with no test. There were ten, and one of
them had no implementation either.

The one that had no implementation is the pattern worth remembering: §7 says
"`measure replay` reports its own wall clock", the `tracing::info!` was right
there in `replay_table`, and **nothing installed a subscriber**.
`heuristic_jump::main` installs one; a `measure_<lang>` main is four lines and
does not. So every `info!` and `warn!` in `measure_core` — the wall clock, the
collection checkpoints, the "server errored" warning — went into a facade with
no dispatcher behind it. A claim can be satisfied at the call site and still be
false end to end, and the emitting code reads as correct in isolation. **When a
section says the tool *reports* something, follow the value all the way to a
file descriptor.**

### Approaches taken and not taken

- **Parsing `design/core.md` for §7's usage block** and comparing it to
  `clap`'s flag set, per subcommand. This is how "there is no `--held-out`
  flag, and there must not be" becomes assertable: a test naming `--held-out`
  passes while any *other* reachable flag is added, so the claim is the whole
  set or it is nothing. Controlled both ways — a flag added to `cli.rs`, and a
  flag deleted from the document.
- **A handler reporting through its own `Trace`** is the channel for anything a
  test wants to know about what the handler was *given*. `LanguageHandler`
  takes `&self`, so there is no interior mutability to reach for and no lock to
  add; the stage labels come back out in the record. This is how the unbounded
  deadline is observed.
- **A `MakeWriter` over a file** for capturing log output. The obvious
  `Arc<Mutex<Vec<u8>>>` buffer is banned by `CLAUDE.md`, and a file needs no
  synchronisation because the writer is opened per event in append mode.
- **`tracing::dispatcher::has_been_set()`** holds "the binary installs a
  subscriber" without capturing anything. It is order-dependent under
  `cargo test` if asserted *before* a run; asserted *after* one it is safe
  either way, and the control still fails because nothing else in the workspace
  sets a global dispatcher.
- **Not taken: driving `collect`.** `Collection` is `pub(crate)` and the only
  route in is `resolve_server`, which reads the real `servers.toml` — outside
  this loop's write list, and a fake server has no business in the file that
  records which servers the corpus was collected against. Everything `collect`
  owns beyond the header is unheld. A campaign that wants it needs either a
  write-list change or a public seam taking a `ServerEntry`, and should say
  which before starting.
- **Not taken: a test that the *instantiated* template runs.** `lang_rust`
  declares "no tests, deliberately" and `measure_core` may not depend on a
  language, so the only home is `crates/measure_rust/tests/` with ~200 lines of
  fixture corpus duplicated into it. The claim is currently held by a
  hand-written double in `pipeline.rs` whose comment says it is "deliberately
  the template's behaviour" — true, and nothing enforces it.
- **Not taken: the `#two-modes` minor** that says `collect` writes rows with
  the `lsp_*` fields populated where the code writes a compact five-field truth
  row. The code is right — a §7 record per truth row is mostly nulls and
  duplicates `uri`/`language`/`mode` twenty thousand times — so the fix would
  be a spec edit, in the same campaign as code in the same section, which is
  precisely the shape the loop prompt says is being watched for. It is a minor
  and does not block the section.

### Controls

Every one of the ten was controlled, and two of them told me something:

- The identifier-rule join could *not* be controlled by breaking
  `shared::identifier_at`, because `positions::enumerate` calls the same
  function — both sides move together, which is the property being asserted.
  The control that works is giving the *fixture handler* a rule of its own.
- Dropping the wall-clock event below the filter first failed on
  `read_to_string` of a log file that was never created, with an unhelpful
  message. Creating the log empty first moves the failure onto the assertion
  that says what is missing. **A control that fails for the wrong reason is a
  test whose message will be wrong when it matters.**

## Campaign a9937015 — the last two rope-modifications gaps, and a red tree

### The tree was red before I started, and it will happen again

`harness/gate core` failed at `ac207d4` on a test I had not touched. Two loops
had raced: `12e0d06` (deps, 18:10) asserted that no `crates/*` source but
`heuristic_jump`'s names `tracing_subscriber`, and `087fa45` (core, 18:58) put
`install_logging` in `measure_core::run`. Each campaign's gate was green when
it ran; the tree was not.

**Check `harness/gate core` before writing anything.** A red HEAD costs you the
whole green-or-revert protocol: you cannot revert to green, and you cannot
commit. I spent about fifteen turns on a collision that was nobody's target,
and the only reason it was affordable is that I had not yet made any edits of
my own to disentangle from it. `state/decisions/core-002.md` records both the
question and the fact that the harness let it happen.

The resolution shape, if this recurs: neither side is usually wrong, so it is a
`class-b` record and the provisional choice is **whichever option changes no
code**, since the other campaign's gap is already banked and reverting reopens
it.

### The audit's gap list describes a different tree

`3dba8fae`, the commit the last audit ran against, **is not an ancestor of this
branch.** So the gap list is computed against a sibling loop's tip. Three of
the gaps I looked at were already closed here:
`core.md#vendoring-the-zed-crates[dcb3592e02]` (by `01ee20a`),
`deps.md#7-file-enumeration-and-watching[2b4c370ec5]` (by `12e0d06`), and by
inspection also `deps.md#8`, `deps.md#12` and
`deps.md#fxhashmap-and-fxhashset-are-the-default`.

One of those was in my *assignment*, so the planner is reading the same stale
list. **Verify a gap in the code before planning around it** — one grep, and it
costs a turn where believing it costs a campaign.

### Fetch upstream rather than reasoning about the diff

The `add_newline` gap said the doubling was a conversion edit that changed
arithmetic, which `rope-modifications.md` §3 forbids. It is not: upstream at
`90d024b8` has the same line. `curl` to `raw.githubusercontent.com` works from
this environment, and it is one turn — CHANGE-core-001 settled the nine-versus-
eight test count the same way. **Any claim of the form "the sweep broke this"
is answerable in one request.** The answer changed what the commit was: not a
conversion repair but a recorded upstream bugfix, which is a different patch
class in `vendor/README.md` and a different thing for a re-sync to know.

The corollary is that fixing it needed a second decision the gap did not
mention — `add_newline` also never incremented `chars`, where `newline()`
twelve lines above sets `chars: 1`. Both had to go for the property to hold,
and the property (`from(t)` + newline == `from(t + "\n")`) is what proves it
rather than a hand-written expectation.

### Measured, not recalled: cargo does not reject a bad profile spec

`Cargo.toml` carried a comment saying `serde_json`'s `opt-level` bump was
waiting because "cargo rejects a profile override naming a package that is not
in the graph". It does not — it prints "profile package spec `x` in profile
`dev` did not match any packages" and builds. Planting one took a turn. That
turned a transcription test into a real one: the bumps are now checked against
`Cargo.lock`, because a bump for a renamed or dropped crate reads as applied
forever.

### Not taken

- **`deps.md#15-clippy-in-workspace-toml`.** The threshold §15 says is "tuned
  in clippy.toml" is absent, and `clippy.toml` is denied to every loop. A
  campaign has exactly two moves, one denied and one dishonest (deleting the
  sentence). It is `core-003`, a `harness-request`, with the measurement done
  and the patch verified against a copy of HEAD in `/tmp` so the human edit is
  a paste. **Do not pick this as a target** until the record is answered.
- **`deps.md#11-cli-parsing-clap[521f7f6b96]`** — the `--trace=<path>` flag is
  genuinely absent from `measure_core/src/cli.rs`. Left because it needs
  §11 plus §7 plus the record-writing path, which is fresh reading and so a
  cheaper campaign somewhere else.
- **Widening the `allowed-primitives.txt` check to assert emptiness.** Rejected:
  the file exists for the re-sync case, and a test that fails when someone adds
  a legitimate entry is a test that will be deleted rather than obeyed.

## c601eeec — §14, §5's licensing, `#adding-a-language`, §9's layout

Five commits, all in `crates/driver/tests/seam.rs` plus the documents they
read. Four new tests, five Class A changes (`CHANGE-core-010` … `010`).

### The assignment named three gap ids that do not exist here

`grep`ping `state/audit/gap-log.jsonl` for the four assigned ids found one.
The audit on this branch ran at `3dba8fae`, not an ancestor of `loop/core-2`,
so the planner's gap list describes a sibling tree. The branch's own audit
recorded one gap per assigned section and both were already closed
(`f288bd5296` by `a9937015`, `0858868078` by `b59733c6`).

**What worked, and is the thing to repeat:** take the *sections*, print each
with `harness/hj section-text`, and enumerate its claims against the tree one
by one. §14 has fifteen bullets; six were read by no test. That is a
mechanical procedure and it does not need the ids at all. It cost ~10 turns,
which is what `core-004` (harness-request) is about.

### What was actually unbacked, and what was not

Unbacked, and now scanned: `resolver = "3"`, `[workspace.package]`'s key set
and who inherits it, `[profile.release]`'s three values, `doctest = false`,
the `cargo-machete` table, `rust-version` against `rust-toolchain.toml`, the
root licence texts being *regular files*, `high-level.md`'s licence prose,
both template manifests, `measure_<x>`'s four lines, the absence of a `tests/`
directory, and §9's whole directory tree.

**Not unbacked, though it reads that way:** §14's "each `allow` carries a
comment saying why". For `[workspace.lints.*]` the §15 test already enforces
something stronger — every lint in `Cargo.toml` must be *printed and argued*
in `deps.md` §15 — and `vendor/*` is exempt by §14's own next bullet. Do not
add a comment-proximity scan for it; the versions that hold for all four allow
sites are heuristics that pass tables with one comment and three allows, which
makes the section look covered when it is not.

### Documents as fixtures, twice more

`fenced_toml_of` had one user (§15's lint block). This campaign added
`fenced_block_of` for §9's untagged directory tree and `section_of` for a
markdown section body. The §9 test compares the printed tree against
`[workspace] members` in **both** directions and is the strongest thing here:
a crate added without the document naming it fails, and vice versa.

That test forced `CHANGE-core-014`. §9's tree lists eleven `crates/` entries
and four cannot exist — `loops.md`'s decided question 10 puts a new
`crates/lang_*` outside every loop's owned paths, and `state/phase.toml` names
`crates/lang_rust/` rather than globbing for exactly that reason. Marking them
`phase 2` is what made the comparison possible; without it the test demands
the commit the gate is built to reject.

### A negative check that fired on its own fix

The first version of the licensing test banned the phrase "only GPL" from
`high-level.md`, to catch the superseded "rope is the only GPL input". It
failed on the corrected text, which *quotes* the superseded claim while
recanting it. Replaced with a positive assertion — the section must say "two
GPL inputs, not one". General shape: a ban on a wrong sentence fires on the
paragraph that explains why it was wrong, and a document that cannot explain
its own history is one the next reader re-derives the mistake from.

### Not taken

- **The driver run loop.** Still where the real gaps are, still four sections
  seen from one missing transport, still a separate campaign — it shares no
  file with anything here.
- **`core.md#9`'s four phase-2 crates.** Cannot be built by any loop. If a
  future audit re-opens `ce5dfefab5`, the answer is `CHANGE-core-014` and not
  a campaign.

## Campaign 2dca52ce — the two `actor.rs` gaps, and three stale ones

Four commits: `d1ccba7`, `5e6ebb9`, `ef775ad`, `9602df2`. Both assigned gaps
closed, one escalation filed (`core-022`), one answered decision reconciled
(`core-017`).

### The stale-gap check finally paid off in the other direction

For once the assigned gaps were **real**: `actor.rs` last changed at 01:05 UTC
and the audit stamps are 04:57/05:37, so the audit saw the current file. One
`git log -1` on the `where:` file plus a `grep` of `gap-log.jsonl` settled it in
one turn, and that is the whole of the ritual — it is worth doing even when it
comes back clean, because the clean answer is what lets you stop thinking about
it.

Three *other* listed gaps were verified stale, each in one grep, and they should
not be picked up again:

- `deps.md#8-parse-cache[ffcd948852]` — `trees.rs` is an `LruCache` with
  `CacheEntries` **and** `CacheBytes`, byte-ceiling eviction, keyed by a
  `ParseKey`. `lru` is in `Cargo.toml` and `Cargo.lock`. The gap describes an
  unbounded `FxHashMap` that no longer exists.
- `core.md#7-observability[bd3003d0fb]` — all three particulars are false.
  `actor.rs` emits the record (`traces.finished` / `awaiting_child`), produces
  `queued_us` from the `arrived` instant, and writes proxy rows itself;
  `tests/actor.rs` asserts `queued_us` is `800000` on the queued half of the
  deadline test. What is left of that gap is the transport, which is a separate
  campaign and is not what the gap text says.
- `core.md#two-modes-collect-and-replay[6bd547104d]` — I *claimed* this one, so
  it is worth being explicit: `4c50a45` appends every row rather than every
  `CHECKPOINT_EVERY`th, the partial file is therefore a prefix, and
  `done = rows.len()` is a sound position index. The code carries a comment
  naming the old bug. `PROGRESS_EVERY` is now only a log cadence.

### The gap named one discard site and there were two

`core-017` is answered and its "work left" paragraph is an ordinary target: make
the prior reachable without a completed outcome. The gap points at
`Actor::answer`'s `DeadlineExpired` arm and the hard cap behind it. **That is
not the only place a classification is thrown away.** `dispatch` was written as
`call(..).and_then(|outcome| encode(..).map_err(classify))`, and `encode` reads
the target file — so `ProjectView`, which refuses a read whose deadline has
already expired, ends the query *inside the conversion*, after the handler
classified and after the outcome was moved into `encode`. A late answer whose
definition is in another file never reaches the hard cap at all.

The discriminating experiment is worth repeating on any "N sites construct this
variant" claim: I planted `or_classified_by` returning `Nothing` and the hard-cap
test still passed, which is how each test is *known* to exercise the path it
names rather than the other one. The two tests differ only in which fixture file
the definition is in (`src/lib.rs` short-circuits the second read; `src/target.rs`
does not).

### Two red gates, both from scans reading text rather than code

1. A comment I wrote in `actor.rs` quoted `std::sync::mpsc` — the identifier
   `seam.rs`'s async-shape scan greps for. The scan reads source *text*, so
   naming a banned thing in prose trips it. Reworded to "the standard library's
   channel". **Do not quote a banned identifier in a comment**, anywhere in
   `crates/`; say what it is instead.
2. `std::fs::read_dir` is in `clippy.toml`'s disallowed methods (it bypasses
   gitignore semantics) and the suggested replacement, `ignore::WalkBuilder`, is
   not a dependency any test crate can reach. The sanctioned way to enumerate
   our own sources is `seam.rs`'s: read `crates/<name>/src/<name>.rs` and follow
   its `mod` declarations. It is also better — it reads what the crate compiles,
   so a file orphaned by a rename cannot change the result.

### `driver` may not name `tracing_subscriber`, and that includes its tests

`seam.rs` scans every source file of every member for the string, exempting only
`heuristic_jump` and `measure_core`. So a driver test that wants to read a log
line cannot use `tracing_subscriber::fmt`. A hand-written `tracing::Subscriber`
is about 35 lines: `enabled`/`new_span`/`record`/`record_follows_from`/`event`/
`enter`/`exit`, plus a `field::Visit` whose only required method is
`record_debug`. Fields come out as `name=value`. It sends over a
`crossbeam_channel::Sender` rather than a `Mutex<Vec<_>>` — `Subscriber` is
`Send + Sync` and records through `&self`, which is the one shape in a test that
tempts a lock.

### Not taken, and why

- **A concurrency test for §1's "handlers are re-entrant".** The `Send + Sync`
  half is compile-enforced by the supertrait, and a passing concurrency test
  proves nothing about re-entrancy. Left alone deliberately rather than
  overlooked.
- **A scan for "no `lang_*` crate names `Outcome::Committed`"** (§1's "handlers
  never construct `Committed`"). The natural home is `seam.rs`, which another
  worker held this round. It is a good target for whoever holds it.
- **Driving `collect`.** `Collection::run` returns before `Client::start` when
  the truth file already answers every position, so an "already collected"
  test needs no server — but the header would have to carry the *installed*
  `rust-analyzer` version to survive the drift check, and the test would then
  fail on a machine without it. Left as a decision for someone taking the
  resume arithmetic deliberately; the prefix invariant itself is unreachable
  from an integration test, since `Truth`, `Writer` and `Row` are `pub(crate)`.

## Campaign 88d25014 — §10 and §2, and what a one-gap section was hiding

Five commits: `9c30b0f`, `8eafc87`, `9a5a721`, `01284f8`, `e521118`. Both
assigned sections clean, one `CHANGE-` record, no reverts, 283 → 286 tests.

### The staleness ritual is worth doing even when it comes back clean — and it
### came back *both* ways this round

One turn settled both assigned gaps, and the two answers differed, which is the
first time that has happened here. The check that discriminates is not
`git log -1` on the `where:` file alone: it is **which audit run opened the
gap, and whether that run's `sections_audited` includes the section**.

- `d50e2285d0` (§10) was opened by the 20:33 UTC run; `dispatch.rs` last moved
  at 19:58 UTC. The audit saw the current file — real, and it was.
- `8e707386b4` (§2) was opened by the *05:37* run, `actor.rs` moved after it,
  and `deps.md#2-channels` is absent from the 20:33 run's `sections_audited`.
  So the gap is a *stale row carried forward by a partial audit*, not a
  judgement about the current tree. It was closed by `2dca52ce` at
  `actor.rs:256`.

`gap-log.jsonl` is where both answers are: `opened`/`closed` per run, plus
`sections_audited`. It is one `grep` and it beats reasoning about timestamps.

### The gap list under-reports too, and that is the finding

Every findings file on this loop says the list over-reports and the code is the
oracle. True, and it stayed true. What none of them says is the other half: the
gap is only what the auditor could *see*, so a section with one gap is not a
section with one problem.

§10's listed gap closed in ten lines. The section then had **four** more claims
with no mechanism behind them, none of them listed:

- the nine sub-enum names were transcribed into `seam.rs`, so the test compared
  the code against a *copy* of the document;
- `main`'s return type — §10's closing sentence, and the one that makes every
  other rule in the section reachable from outside the enum;
- the one foreign error not carried as `#[source]` (a listed *minor*);
- (§2) the bound being a per-channel judgement made exactly once.

Procedure that found them: print the section with `harness/hj section-text`,
and take its sentences one at a time asking "what would fail if this stopped
being true". Same method `20bbc1bf` used on §14, and it works on a section the
audit calls nearly clean.

### The conversion is at `classify`, and the reason generalises

§10 says the `Error`→abstention conversion is "explicit and logged". The
auditor named two candidate sites (`classify`, `Actor::answer`) and the right
answer is neither-and-one: `classify` is where an `Error` stops being a
failure, and it is the *only* such place. `Actor::answer` builds the
`Outcome::Abstain` but is handed a `Classified`, and the other thing that
reaches that arm was never an `Error` — a merely-late answer that `hard_cap`
dropped, which already logs. So one line at `classify` covers all three callers
(`realise`, `call`, the conversion) and double-logs nothing.

**Log at the point of conversion, not at the point of construction.** The
construction site sees several origins and cannot name the one that matters.

### The test's second half is the whole test

`converting_an_expiry_into_an_abstention_is_logged` runs two fixtures that
differ only in which file holds the definition. Same-file takes `target_text`'s
free path, builds no `Error`, and is dropped by `hard_cap` under its own line;
cross-file expires inside the read and is converted. Asserting the conversion
line is *absent* on the first is what makes it a test of the conversion rather
than of expiry — planted exactly that (log only in `answer`) and it failed.
Three plants, three correct failures.

### Do not create the comment trap you were burned by

`2dca52ce` cost a red gate because a comment quoted `std::sync::mpsc` and a
text scan reads source text. Writing a new text scan is the chance to create
that trap for somebody else — `the_only_bounded_channel_is_the_one_section_2_argues_for`
would have fired on `actor.rs`'s own paragraph explaining why its inbox is not
bounded. **Skip comment lines**, and say in the doc comment why: a scan that
bans a word is a scan nobody can write the explanation under. Planted both
ways to prove the skip is real and the scan still bites.

### `CHANGE-core-023`, and why it is safe to have made

§10 wraps foreign errors as `#[source]` "always"; `core.md` §9 fixes `shared`'s
dependency list; the enum is in `shared`, so a `#[source]` on a parser's error
declares that parser for every crate naming an `Error`.
`ConfigError::ManifestMalformed` renders a `toml::de::Error` for exactly that
reason, and had said so in its own doc comment since before this campaign.

It is Class A because the boundary is decided by a *test*
(`shared_declares_only_the_dependencies_section_9_lists`) and not by an author,
so the exception cannot widen without something going red. And it is safe to
have made in this campaign specifically because it closes a **minor**, so no
number moves either way — there was no progress available to fake. If a future
campaign finds itself doing this for a *gap*, that is a different situation.

### Not taken, and why

- **The two refused claims.** `core.md#the-trait[218a36571e]` (the stale
  printed block — the same types I had open all campaign) and
  `deps.md#8-parse-cache[fb0aa10250]`. Both held by other workers. Two
  refusals in a row is the fleet saying the round is allocated; a third fishing
  attempt would have been a turn spent on nothing.
- **`core.md#the-trait`'s minor about `Outcome::Committed` being constructible
  by a `lang_*` crate.** Still the right shape (a `seam.rs` source scan, and I
  held `seam.rs`), still recorded in `state/audit/core.toml` — but the section
  is claimed, and closing a minor of a section somebody else is working is how
  two campaigns collide in one file.
- **A second plant for the real binary in `every_main_returns_the_total_error`.**
  Not laziness: the `mains` control already asserts both binaries' roots were
  reached by the scan, and the assertion runs on every main found, so the
  doc-template plant exercised the strictly harder path.

## Campaign 340b4361 — a blocked assignment, and §14 taken as a section

Eight commits, all green, 292 → 294 tests. `deps.md#14` did **not** go clean
and could not have. `deps.md#5` and `#6` minors closed with CHANGE-core-025 and
CHANGE-core-026.

### The assignment was two gaps and neither was workable, which is knowable in one turn

Both were opened by the 05:37 run and that run's `sections_audited` named §14,
so the staleness ritual came back "real" for both — and both were still dead
ends, for different reasons the ritual does not detect:

- `7d21b547b7` (the `[profile.dev.package]` list) was **closed by `c9e5423`**,
  which landed after the audit ran. The ritual answers "did the audit see this
  file"; it does not answer "did somebody fix it in the four hours since". The
  cheap second check is `git log --oneline -8 -- <the where: file>` and reading
  the subjects — `[deps-14] the profile takes exactly §14's list` is not
  ambiguous.
- `d822e97954` (the `cargo-deny` config) is **blocked by an answered
  decision**. `core-023` was answered *accepted A — adopt cargo-deny*, and the
  ruling's own "what is left, and who does it" assigns it to a human: the file
  is `deny.toml` at the root, the check path is `harness/gate`, and both are
  outside every loop's paths. Re-measured rather than assumed:
  `harness/hj check-scope core` → `deny.toml: outside core's owned paths`;
  `cargo deny` → `no such command`.

**So read the decision records for your assigned section before doing
anything.** One `grep -l` over `state/decisions/` for the section number would
have told me in one turn what took three to establish. An *answered* record can
block a gap as hard as an open one — harder, because the question is settled
and there is nothing left to escalate.

### Reconciling an answered decision: do not follow its Consequences section literally

`core-021`'s own Consequences said "if the answer is A, the seam test is
deleted... about 120 lines of test come out". The answer *was* A. I kept the
test, and I think that is right: answer A's replacement (`deny.toml`, plus
something that runs it) does not exist and cannot be built by a loop, so
deleting the only graph-level licence check today trades a real check for a
file nobody has written. A Consequences section is written before the answer
and does not know what the world looks like when it arrives.

I also found the ruling resting on a premise that is false. `core-023`'s
Decision argues that `deny.toml` "is a claim about the resolved graph, which no
test can reach" — and `core-021`'s own provisional test reaches it, with
`cargo metadata --format-version 1 --offline`. The conclusion survives (what
cargo-deny still buys is SPDX parsing, a reasoned exception list, advisories),
so I recorded it in a **Reconciliation** section appended to `core-023` and did
not touch its Decision. Do that rather than editing a ruling: a loop that
rewrites the argument it was answered with has un-answered itself.

### The assertion that failed the build instead of the test

Writing CHANGE-core-025 I claimed §6's "driver parses with tree-sitter and
declares none of it" is true only while `shared` re-exports `Language`, `Tree`
and `InputEdit`, and wrote the assertion. The plant — dropping `InputEdit` from
the re-export — does not fail the test; it fails `cargo build`, because
`driver/src/trees.rs` imports all three by name. Same shape worker 1 hit with
the serde derives. The assertion came out in the same experiment and the fact
went into the document instead, with the part that is *not* compiler-held: the
repair rustc suggests for that build failure is `driver` declaring
`tree-sitter` itself, which is the state the bullet says is not the case.

**Run the plant before believing an assertion is doing work.** A test whose
negation cannot be reached is decoration, and the two failure modes look
identical from a green suite.

### Where §14's remaining claims actually were

The audit's two gaps were a closed one and an unreachable one, so the section
had to be worked sentence by sentence (`harness/hj section-text`). Five real
mechanisms fell out, none of them listed as anything:

1. "Each `allow` carries a comment saying why" — nothing read it, because
   §15's comparison drops trailing comments on *both* sides, which it has to.
2. "Each vendored crate carries its own `[lints]` table" — the non-inheritance
   was asserted and the table that records it as deliberate was not. An empty
   table changes nothing about what is linted, so it deletes silently.
3. §14's file tree is a **licence table**, one section away from §5's, and
   nothing compared it to the manifests.
4. The toolchain pin is written three times; two are edited together because a
   build fails otherwise, and the third is the document.
5. The upstream revision is written five times, twice in full and three times
   abbreviated. The table is what a re-sync updates.

Items 3–5 are one pattern and it is worth naming: **a value written down in
two places where only one of them breaks when it is wrong.** Every one of them
was a document copy of something the build or another test already held. That
is the shape to search a section for once the gap list is exhausted — not
"what is unchecked" but "what is written twice".

A related one, from the `allow` scan: **requiring a comment to *name* its
subject is what separates an argument from a banner.** A check that only wants
"some comment nearby" is satisfied by `# -- misc ---`. It cost one vendored
comment a reword, which is the right price.

### Not taken, and why

- **§0's `tempfile` minor.** The conflict is between `deps.md` §0 rejecting
  `tempfile` and `clippy.toml:38` naming `tempfile::tempdir` as the sanctioned
  replacement for a banned method. `clippy.toml` is denied to every loop, so
  the resolution is not a loop's to make; and fixing §0's row instead would be
  moving the document to match a file I cannot read the intent of.
- **§9's and §13's minors.** Both in `measure_core`, which is a different
  campaign's reading — `measure_core.rs`'s subscriber and `corpus.rs`'s
  `verify_checkout`.
- **A "no `lang_*` crate constructs `Outcome::Committed`" scan**, which my last
  journal entry proposed and which I now hold the right file for. It belongs to
  `core.md#the-trait`, a section another worker was assigned, and closing a
  minor of somebody else's section is how two campaigns collide in one file.
  It is still the right shape and still worth doing by whoever holds §1.
