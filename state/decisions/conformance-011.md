---
id: conformance-011
status: accepted
opened: 2026-08-03T08:10:00+00:00
campaign: de2706af-51e1-4f63-828c-7cd3cfcc5195
kind: class-b
---

# Does `similarity` keep §9's edge to `shared`, when no loop may write the crate that would declare it?

## Context

`core.md` §9's graph draws `shared -> similarity` and its bullet says it
outright:

> "**`similarity` depends on `shared`, and is frozen.** It holds only what is
> ported from the prior implementation — `Occurrences`, `IdentifierParts`, and
> path–namespace scoring (`resolution.md` §5)."

`crates/similarity/Cargo.toml` declares `arraydeque`, `hashbrown`, `itertools`
and `smallvec`, and not `shared`. Nothing in the crate names a `shared` type:
the ported code is generic over its own occurrence and identifier types, which
is why it could be ported before any vocabulary existed.

Neither half of the discrepancy is mine to settle. `crates/similarity/**` is on
the deny list for every loop in every phase, so this campaign cannot add the
dependency; and removing the edge from §9 is moving the spec toward the code on
a claim about layering, which is the one class of edit the audit cannot catch.
It also trades something real: the edge is what says a handler's ranking speaks
the same vocabulary the seam does, so `Location` and `ProjectPath` do not have
to be converted at the `similarity` boundary when `resolution.md` §5's scoring
is actually wired to one.

The section is otherwise clean as of this campaign, so this is what stands
between `#the-dependency-graph` and staying clean once somebody looks at the
edge rather than at whether the crate exists.

## Options

1. **`similarity` declares `shared`.** The graph is true as drawn, and the
   ranking API can take `&Location`/`ProjectPath` when phase 2 reaches it.
   Costs: a write to a frozen, denied crate, by whoever owns it; and until a
   type is actually used, the edge is a declared dependency with no user, which
   `deps.md` §14's "each arrives with its first user" argues against.
2. **§9 drops the edge until a `lang_*` needs it.** Costs nothing today and
   matches the manifest, but it weakens a layering claim on the strength of
   code that is frozen precisely so nobody re-argues it — and the dependency
   then arrives during phase 2, in the crate nobody is allowed to touch, at the
   moment a language loop is blocked on it.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Neither. The manifests and §9 are left exactly as they are, which is the most
reversible thing available: option 1 is unavailable to this loop by the write
list, and option 2 is a spec edit toward the code that would erase the question
instead of answering it.

No sites are tagged, because there is nothing this loop may write that the
answer would change — the two candidate sites are
`crates/similarity/Cargo.toml`, which is denied, and `core.md` §9's bullet,
which is the claim under audit. This record is the tag.

## Consequences

If the answer is option 1, one line is added to a manifest by whoever owns
`crates/similarity/`, and nothing else in the workspace changes; the seam test
in `crates/driver/tests/seam.rs` gains an assertion that the edge exists. If it
is option 2, one bullet and one arrow in §9 change and the phase-2 language
loops inherit the conversion at the `similarity` boundary instead. Either way
no code written by this loop is redone.

## Answer — 2026-08-03T19:00:32+00:00

**Ruling:** accepted

Option A.

**Rationale:** The deps.md 14 objection -- each dependency arrives with its first user -- is about third-party crates, where an unused dependency is version risk carried for nothing. `shared` is a path dependency in the same workspace: no pin, no resolution, no risk, so the rule's cost does not apply. Against that, dropping the edge means the dependency arrives during phase 2, in a crate nobody may touch, at the moment a language loop is blocked on it. And `similarity` is frozen precisely so nobody re-argues it; leaving its manifest disagreeing with the graph guarantees exactly that argument. The contradiction was introduced by the human who placed the crate, not by any campaign.

Reconciling the sites tagged `// DECISION-conformance-011: provisional` is a
normal campaign target, not an interrupt.
