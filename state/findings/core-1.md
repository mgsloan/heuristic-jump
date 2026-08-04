# Findings — core, worker 1

## Check the gap against the code before working it

Three of four targets this round were closed by commits made *after* the audit
read them. `state/audit/core.toml` carries `last_audited` per section, in UTC;
`git log --date=iso-strict` on the file the gap names is in local time. One
comparison, one turn. The gap text is a statement about the repository at a
timestamp, not about the repository — but the *section* is still the right
target, because what a re-audit finds instead is usually real and is never in
the list.

## Run `harness/gate core` before believing the tree

It was red at open, from a campaign that closed `confirmed`: `measure_core`
gained a `tracing_subscriber` edge and `driver/tests/seam.rs` asserts which
crates may have one. `cargo test -p <the crate you are editing>` cannot see
this — the assertion lives in another crate's test, about a third crate's
source. Whole-workspace claims live in `driver/tests/seam.rs`; if you add a
dependency edge or a `mod`, read it.

## Where the gaps are concentrated

**In `driver`, and they are one campaign rather than five.** `§5`'s deadline,
`§7`'s emission, `both-sides-are-sets`' pending-query record and `deps.md
§11`'s `--trace` all say the same thing: `driver::run` logs its config and
returns. Everything downstream of a request arriving is missing, and each of
those gaps is a symptom. Do not take them one at a time.

`measure_core`'s replay half is now dense with tests and is not where the
remaining value is. Its one real hole is that **nothing can drive `collect`**:
`Collection::run` spawns a language server, so `--restart`, the probe loop and
the resume arithmetic have no coverage. A fixture server answering
`initialize` and `textDocument/definition` closes all of them at once and is
the crate's missing piece of test infrastructure.

## Load-bearing claims, confirmed by using them

* **§8.2 gives the wire types no `Serialize`.** It decides the truth row's
  shape (CHANGE-core-006) and would veto any "write the record out and read it
  back" design. Reach for it before inventing an intermediate file format.
* **`positions/<repo>.jsonl` carries the token text.** It is what makes §7's
  failure-digest sample joinable without a second definition of "identifier".
* **§7's record field order is the declaration order** and is asserted against
  the document. Adding a field to `QueryRecord` is a seam change, not a
  convenience.

## Do not spend time on

* `harness/measure`, which §7 names and the failure digest needs: `core-001`,
  open, and `harness/` is denied to every loop.
* A digest inside `measure_core` — the split is deliberate; what this crate
  owes is the join, and that is now held by a test.
* `core.md#9-workspace-layout`'s missing crates: `measure_core`,
  `measure_rust`, `lang_rust` all exist now. Whatever is left of that gap is
  the `lang_*` crates for languages nobody has started.
