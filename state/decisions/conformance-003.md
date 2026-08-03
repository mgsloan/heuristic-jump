---
id: conformance-003
status: open
opened: 2026-08-03T05:30:00+00:00
campaign: 4ba19af5-b041-4f2f-9d85-e5553eb14c57
kind: harness-request
---

# Should the gate run `vendor/`'s tests, given that it must not lint them?

## Context

`vendor/rope` and `vendor/sum_tree` now exist and bring 29 upstream tests with
them. Keeping those tests is not incidental — it is the stated reason the
vendoring was done this way. `rope-modifications.md` §7: they "are the only
independent check that a 51-function signature sweep and the body edits that
follow from it did not change behaviour, and several are randomised
differential tests against a `String` oracle, which is precisely the kind of
test nobody would write from scratch."

The gate does not run them. `harness/hj crates` intersects
`state/phase.toml`'s `crates` list — `shared`, `driver`, `heuristic_jump`,
`measure_core`, `measure_rust`, `lang_rust` — with the workspace members, and
`rope`/`sum_tree` are in neither. So every step of `harness/gate conformance`
is green on a tree in which the newtype sweep has broken the rope, and the
campaign that performs that sweep is exactly the one that will believe the
gate.

`state/phase.toml` is denied to every loop, which is right — it is the
ownership table — so this cannot be fixed from inside a campaign.

**The obvious fix is wrong**, which is why this is a decision rather than a
request to add two names to a list. Adding `rope` and `sum_tree` to `crates`
also puts them through gate step 2,
`cargo clippy -p <crate> --all-targets -- -D warnings`, and they do not pass:
`deps.md` §14 deliberately withholds `[lints] workspace = true` from `vendor/*`
("bending them to `unwrap_used`, `panic`, and the `cast_*` family would be a
large amount of work that buys no correctness, and every line of it would
widen the re-sync diff"), while the root `clippy.toml` still applies and
`-D warnings` promotes clippy's default-level lints. Measured, not assumed:
five errors today, in unedited upstream code — `should_implement_trait` on
`Iterator::next`, `from_over_into`, and `disallowed_macros` on the `eprintln!`
that prints a failing seed.

So "build and test but do not lint" is a distinction the gate does not
currently have, and `vendor/` is the first thing that needs it.

## Options

1. **A separate crate list for `vendor/`** — a `vendor_crates` row in
   `state/phase.toml`, run through gate steps 1 and 3 (fmt, nextest) and
   skipped in step 2 (clippy). Costs a second list to keep in step with the
   first, and a gate that now has two notions of "the crates this loop owns".
   Note step 1 would need care too: `cargo fmt -p rope --check` fails on
   upstream text that rustfmt would reflow, and reformatting it is the
   whole-crate diff `CLAUDE.md` warns about.
2. **One list, with clippy scoped to `crates/*`** — keep a single `crates`
   row, and have step 2 filter out members whose manifest path is under
   `vendor/`. Cheaper, and it encodes the rule where the rule actually lives
   (`deps.md` §14 is about `vendor/*` as a directory, not about a particular
   crate name). Costs the ability to ever lint a vendored crate deliberately.
3. **Leave it, and require the campaign to run them by hand.** Costs exactly
   what a gate exists to prevent: it works until the session that forgets, and
   the session that forgets is the one that broke something.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Option 3, because 1 and 2 are both edits to `harness/` or `state/phase.toml`
and no loop may make either. This campaign ran
`cargo nextest run -p rope -p sum_tree` itself — 29 passed — and recorded that
it did so in `vendor/README.md` and in the journal, so the next campaign
inherits the instruction rather than the assumption.

It is the most reversible in the sense that matters here: nothing on disk
depends on the answer, and no code site is tagged, because the choice is about
which commands run and not about anything they inspect.

## Consequences

If the answer is 1 or 2, nothing already written has to change; the gate
starts covering 29 tests it currently does not, and the next campaign to touch
`vendor/` stops needing to remember.

If the answer is 3 — that hand-running is enough — then the newtype sweep in
`rope-modifications.md` should say so in its own section, because the
instruction currently lives only in `vendor/README.md` and in this record, and
the sweep is the one piece of work whose safety depends on it.
