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
