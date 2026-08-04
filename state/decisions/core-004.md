---
id: core-004
status: accepted
opened: 2026-08-04T05:20:00+00:00
campaign: c601eeec-b30f-479c-8a7d-49e19e4c166d
kind: harness-request
---

# Can a worker's assignment be computed from an audit of the branch it will work on?

## Context

This campaign's assignment named four gaps by id. **Three of them do not
exist in this checkout.** `grep` over `state/audit/gap-log.jsonl` finds
`3723863fd7` and not `68be1693b1`, `cda6a3e9e2` or `4e5b9e1cfa`. The audit
this branch carries was recorded at commit `3dba8fae`, which is not an
ancestor of `loop/core-2`, so the gap list describes a sibling worker's tree.

What the branch's own audit records for the same three sections was already
closed here before this campaign opened:

* `deps.md#14-workspace-cargotoml-shape[f288bd5296]` — closed by `a9937015`.
* `core.md#adding-a-language[0858868078]` — closed by `b59733c6`.

So the assignment's ids were unreadable and its section list was, in the
branch's own terms, already clean. The campaign proceeded by taking the
*sections* and re-deriving every claim in them from the documents, which cost
roughly ten turns before any work started and produced four commits that may
or may not close the gaps the planner meant.

This is not the same problem as `core-002`, which is about a red gate from a
cross-branch race. This one is silent: nothing fails, and a worker who trusts
the assignment plans around gaps that are not there. The previous campaign's
findings digest already warned about it, which is the recurrence signal — the
warning survives only because a human-readable file happened to carry it, and
a fresh session that skipped the digest would have spent the same ten turns
again.

## Options

1. **Audit each worker branch before dividing the round.** The planner's input
   is then the tree the worker will actually see, and an id it quotes is one
   `harness/hj section-text` and `state/audit/` can resolve. Costs one audit
   pass per worker per round, which is the expensive half of the loop.
2. **Have the planner pass claims rather than ids.** The assignment carries
   the claim and `found` text, which a worker can verify against its own tree
   in one grep, and the id becomes a hint rather than a key. Cheap, and it
   does not make the assignment correct — it makes it checkable, which is what
   the ten turns were spent on.
3. **Have `hj claim` refuse an id the branch's own audit does not carry**, so
   the mismatch is reported at the moment it matters rather than discovered.
   Cheapest, and the narrowest: it catches the case where a worker takes an
   assigned id at face value.

## Decision

**accepted: Options 2 and 3 together — claims not ids, and hj claim refuses an
unknown id**, answered 2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

The two are complementary and neither is the expensive one. Option 2 makes the
assignment checkable — a worker verifies a claim against its own tree in one
grep instead of spending ten turns re-deriving a section — and option 3 makes
the mismatch loud at the moment a worker acts on it rather than silent. Option
1 is the right answer to a different question: auditing every worker branch
before dividing a round pays the expensive half of the loop per worker per
round to fix a problem two cheap changes make visible. The id stays as a hint,
which is all it can honestly be across branches.

### What is left

Both halves are the harness loop's work: the planner's assignment format, and
`hj claim` refusing an id the branch's own audit does not carry.

## Provisional choice in force

Option 2, by hand and on the worker's side rather than the planner's: this
campaign treated the four ids as hints, took the three named sections, and
re-derived their claims from `design/deps.md` §5/§14 and `design/core.md` §9.
That is the most reversible choice because it changes nothing outside this
campaign's own record — no harness path is touched, and every commit is
scoped to a section anchor, which is what the auditor reads anyway.

No site is tagged `// DECISION-core-004: provisional`: there is no source line
this is about. The record is the deliverable.

## Consequences

If the answer is option 1 or 3, the ten turns are recovered for every worker
in every round, and a `partial` close stops being ambiguous about whether the
worker took the wrong target or the assignment named one that did not exist.
If it is option 2, the same information arrives in the prompt and the worker's
first grep is against a claim rather than an id.

Nothing already committed has to be redone under any of the three. The cost
of being wrong here is turns, not work.
