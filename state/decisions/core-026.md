---
id: core-026
status: accepted
opened: 2026-08-05T00:30:00+00:00
campaign: 2f7fcfdd-a930-4241-909d-0e8a4f86631e
kind: class-b
---

# What does a query say when the shim refuses to run it?

## Context

`shim.md` §10 gives the dispatch pool two limits beyond its size, and both are
refusals rather than answers:

> **Max in-flight heuristic queries** (start at 4). Beyond that, new queries
> **abstain immediately** rather than queueing. Queueing cannot help under a
> wall-clock deadline; it only guarantees the queued queries blow it.

> **No heuristic work while `core` is behind.** If the event queue is backed
> up, forwarding and state transitions take priority.

This campaign built the pool (`crates/driver/src/workers.rs`) because three of
`core.md`'s claims are about which thread parses, calls the handler and reads
the target file for §8.4's conversion. It did **not** build either limit, and
the reason is not effort: there is nothing for the refusal to say.

`core.md` §1's `AbstainReason` has five variants — `NotAnIdentifier`,
`UnsupportedRole`, `NoCandidates`, `Deadline`, `External` — and §1 describes
four of them as facts about the code and the fifth, `Deadline`, as "the one
latency-shaped abstention `high-level.md` allows, and the only reason here that
is not a fact about the code". A shed-load abstention is a second one, and it
is a fact about neither the code nor the query: it is a fact about how busy the
process was.

That matters to the number rather than only to the vocabulary. §7 groups
coverage by stratum and reports abstentions with their reason, and
`resolution.md` §8's whole argument for the reasons is that they separate "this
class is hard" from "this handler is broken". A shed query recorded under any
existing reason adds a third thing to that column that moves with machine load
— and `high-level.md`'s stated posture is that blowing the budget must cost
coverage, never correctness, which is only auditable if the coverage lost to
load is *visible as such*.

`AbstainReason` is on the frozen seam, so widening it is not this loop's to
decide in phase 1a.

## Options

**A — add `AbstainReason::Overloaded` (or `Shed`).** The refusal is nameable,
§7 can report the shed rate as its own column, and the in-flight cap and the
shed-load rule can both be built. Cost: a sixth variant on a frozen seam type
that no *handler* can ever return — it is the driver's word, not the language's
— so every `lang_*` author reads a variant they must not use, and every match
on `AbstainReason` in every language crate grows an arm for a case that cannot
reach it. `#[non_exhaustive]` softens the second half and not the first.

**B — record a shed query as a failure rather than an abstention.** No seam
change: `Error` is the driver's own vocabulary and shedding is the driver's
doing. Cost: §7's `decision` column then says `failed` for a shim that is
working exactly as designed, which is the merge §1 spends a paragraph refusing
— "a stratum with no coverage because resolution is hard and a stratum with no
coverage because the handler is panicking are the same row".

**C — do not shed; let the work channel queue.** No seam change, and no new
vocabulary. Cost: §10's stated reason for the cap. A queued query still holds
its deadline, so it does not blow anything a shed one would not; what it costs
is CPU spent on queries whose budget has already gone, which is precisely the
competition with the proper LSP's startup that §10 says the no-index decision
was bought with.

## Decision

**accepted: D — a shed query is a disposition, not an abstention reason**,
answered 2026-08-05 and logged as a `decision-answered` intervention, which is
what makes it answered — `design/loops.md` §16 derives the status from the log
rather than from this line.

None of the three options as posed, and the reason is that all three answer
"what does the query *say*" when the honest answer is that it says nothing: it
was never attempted. So it is recorded at the level where that is true.

`AbstainReason` is the **handler's** vocabulary — what the language said when it
declined — which is why §1 can describe four of its variants as facts about the
code and single out `Deadline` as the exception. A shed query is not the
handler's event at all. Putting it there costs what option A prices honestly: a
sixth variant on a frozen seam that no handler can ever return, so every
`lang_*` match grows an arm for an unreachable case. `CLAUDE.md` asks for enums
that enforce an invariant rather than comments describing one, and "this variant
exists but you must never return it" is exactly the comment.

B is rejected for the reason the record gives: §7's `decision` column would say
`failed` for a shim working as designed, which is the merge §1 spends a
paragraph refusing.

C is rejected as a resting place, though it was the right thing to be running
in the meantime and the record is right that it was strictly better than what it
replaced.

**What makes D affordable now rather than later.** Its cost is §7's record
shape, and §7's record is *already* being changed: `core-025` was accepted with
option B — `stratum_prior` becomes nullable — and has not been implemented yet.
The same campaign is already in `shared::record`, both producers,
`Table::observe` and `pipeline.rs`'s field-order fixture. Paid once for two
changes rather than twice, and the corpus is still small, which was the argument
for B in `core-025` and holds here for the same reason.

**What it buys that A does not.** `high-level.md`'s posture is that blowing the
budget must cost coverage and never correctness, and that is only auditable if
the coverage lost to load is visible *as such*. A separate disposition makes the
shed rate its own number instead of a reason competing with `Deadline` in a
column `resolution.md` §8 built to separate "this class is hard" from "this
handler is broken". A third meaning in that column is what the whole reason
vocabulary exists to prevent.

### What is left

The core loop's, and it should be one campaign with `core-025` rather than two:
both change §7's record and nothing else shares that reading. Then §10's two
limits become buildable — the in-flight cap as a refusal in front of
`Workers::dispatch`, and the `core`-inbox check in `Actor::requested` — which is
the work this record was raised to unblock.

Reconcile the `DECISION-core-026: provisional` tag at the head of
`crates/driver/src/workers.rs` when it lands.

## Provisional choice in force

**C.** The pool is bounded and its work channel queues; neither limit is built.
It is the most reversible of the three because it adds no vocabulary to remove:
A and B both put a word into a record that `measure_core` reads and a table
groups by, and a column that has meant two things is not un-meant by deleting
the variant.

It is also strictly better than what it replaces rather than merely acceptable.
Before this campaign a slow query blocked `core` itself — no forwarding, no
state transitions, nothing — which is the prime invariant §10's shed-load rule
exists to protect. Queueing on a pool violates a limit; dispatching in line
violated the thing the limit is for.

Tagged at the head of `crates/driver/src/workers.rs`, which is where the
absence is explained.

## Consequences

If the answer is A, the work is a variant in `shared`, an in-flight counter and
a `core`-inbox check in `Actor::requested`, a column in §7's record, and a row
in `measure_core`'s table. None of it is undone work: the pool, the job, the
channels and the drain are the same either way, and the cap is a refusal in
front of `Workers::dispatch`.

If the answer is B, the same minus the seam change, and §7's `failure` column
grows a class that is not a failure.

If the answer is C, this record is what says the limits were declined rather
than forgotten — which is the whole reason it exists, because a limit nobody
wrote and a limit nobody remembered look identical in the code.
