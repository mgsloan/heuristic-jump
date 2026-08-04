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
