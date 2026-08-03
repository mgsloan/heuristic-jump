---
id: conformance-009
status: open
opened: 2026-08-03T07:40:00+00:00
campaign: 5314b0c3-326e-415a-9eb6-1d9e7fad4378
kind: class-b
---

# What makes two files "the same module tree", for the purpose of `severity`?

## Context

`core.md` §6's severity table splits a wrong-file answer two ways:

> | Different file, same module tree | differs | `near_module` |
> | Different file, unrelated | differs | `unrelated` |

and nothing in any document says what "same module tree" tests. It is not a
presentation detail: `high-level.md` attaches a separate budget to each —

> "wrong file in the same module tree, <= 0.5% for an unrelated one"

(`high-level.md:189`) — and calls the second one the trust-destroying class
(`high-level.md:391`). Where the boundary sits therefore decides what both
budgets are measured over, and moving it later invalidates every corpus row
collected before the move.

Two constraints narrow the answer hard. §6 says the predicate **reads
nothing**, because divergence is classified when the child responds, when the
per-query read cache is gone and the target document may never have been open.
And the predicate lives in `shared` (§9's graph: `measure_core` does not depend
on `driver`, and neither depends on a `lang_*`), so it has no handler and no
language knowledge — it has two URIs.

That rules out the answer the phrase most naturally suggests. Rust's module
tree is *declared*, and `resolution.md` §10.2 makes that the language's main
advantage over path similarity — but reading `mod` declarations means reading
files, and knowing that `mod.rs` and `foo.rs` are siblings in the same module
means knowing Rust.

## Options

**A. Same containing directory.** `src/main.rs` and `src/parser.rs` are near;
`src/main.rs` and `tests/golden/fixture.rs` are not. Costs nothing, reads
nothing, needs no language. It is wrong in one direction that will show up in
the corpus: `src/ast/expr.rs` and `src/ast.rs` are one module tree in Rust and
two directories here, so a real `near_module` error scores `unrelated` and the
trust-destroying budget is measured pessimistically.

**B. Shared path prefix of some depth, or "one is a prefix of the other's
directory".** Catches the `src/ast/expr.rs` / `src/ast.rs` case. Costs a
parameter nobody can set without corpus data — at depth 1 every file in a
repository is near every other, and at the wrong depth the two budgets stop
distinguishing anything. It also silently makes the class depend on how deeply
a project nests, so the same error is `near_module` in a flat crate and
`unrelated` in a deep one.

**C. Ask the handler.** The only option that can be right, and the one the
architecture forbids: the predicate has no handler, by §9's graph, and adding
one would put a language crate on `measure_core`'s dependency edge, which §9
says only `measure_<lang>` may have. It would also make the predicate
per-language, which is precisely the fork §6 exists to prevent.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A**, tagged `// DECISION-conformance-009: provisional` at
`crates/shared/src/agreement.rs` — on `same_module_tree`, and on the
severity-over-a-set fold in `Agreement::classify` (below).

It is the most reversible because it has no parameter: going from A to B is
changing one function whose whole body is a string comparison, with one
caller, and the corpus rows that would need recollecting are the same ones B
would need recollecting anyway. Going the other way — shipping B, discovering
the depth is wrong, and changing it — is the same cost paid twice. A is also
the one whose error direction is known and safe: it can only move an error out
of `near_module` into `unrelated`, so the tighter budget is measured against
the larger population.

**A second reading is folded into the same tag, because it is the same
question.** §6's table is pairwise and the child's side is a set, so a
severity has to be lifted over it. The code takes the **mildest** class across
the child's whole answer. That follows from §6's own rule that matching any of
the child's locations is a match, because "the LSP is itself expressing
ambiguity and picking one of its own candidates is not an error" — charging
the shim for the child's least convenient candidate would contradict it. It is
noted here rather than as a separate record because it moves the same two
budgets and a human ruling on one should see the other.

## Consequences

If the answer is B, the change is one function and one test in
`crates/shared/tests/agreement.rs`; no call site moves. What does not survive
is corpus data: every `severity` already collected under A has to be
recomputed, which is cheap while the record carries `heuristic_locations` and
`lsp_locations` (§7 says it does) and impossible if that ever stops being
true. **That is the thing worth deciding before the first corpus run**, not
the boundary itself — as long as both location lists are in the record, the
classification is recomputable from stored rows and this record is cheap to
answer late.

If the answer is C, §9's dependency graph has to change, which is a much
larger question than this one.
