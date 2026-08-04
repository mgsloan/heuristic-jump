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
