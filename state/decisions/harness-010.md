---
id: harness-010
status: accepted
opened: 2026-08-04T21:24:00+00:00
campaign: 2e588730-78d0-4235-ad89-afebe7ddcdea
kind: class-b
---

# Does tightening gap attribution to the named gap redefine `progress`?

## Context

`design/loops.md` §5 says, of the round audit's cost: "attribution is by named
gap rather than by count delta, which pays most of it off". The audit found
the code doing something weaker (`loops.md#5-...[02f06f3aad]`):

    named = bool(targets & audited_sections) and bool(closed_ids)

That is attribution by *round*: this campaign named a section the audit
happened to look at, and the round closed some gap somewhere. Under workers it
credits three campaigns for one campaign's work, which is exactly the padding
§5 says the named-gap rule exists to make score worse than not padding.

Bringing the code to the spec is ordinary work. What makes this Class B is
that `progress` is the input to `trailing_without_progress`, which is what
stops a loop (§7). Changing how it is computed invalidates comparability
across the change, and the loop making the change is one of the loops the
number judges.

Two things were needed and only one is a judgement:

* **Not a judgement.** A closed gap's section was not recoverable — a gap id
  is `sha256(section|claim)[:10]`, and a gap that has closed is no longer in
  the audit to look up. The gap log now records `closed_gaps` (id and section)
  alongside the existing `closed`, additively, at the one moment it is
  knowable.
* **The judgement.** Whether to take the audit-side term away from campaigns
  that had it under rule 2.

## Options

**A — tighten, bump `PROGRESS_RULE` to 3, backtest.** What is implemented.
Over the 50 closed campaigns, rule 3 moves the attributed *term* on 13 and the
progress *verdict* on none: every campaign it takes the audit-side term from
had already scored `sections_clean` at close. Cost: rows either side of the
change carry different `progress_rule` values and a reader comparing terms
across the boundary is comparing two rules. `hj progress-replay --rule 2`
recomputes the old answer from the same rows, so the boundary is legible
rather than lost.

**B — leave it, and record the divergence in the spec instead.** Cost: §5's
sentence is the one that pays for the round audit's known weakness, so
weakening it in the document removes the reason the round audit is acceptable
at all. It also rewrites the spec toward the code, which is the one gaming
route §7's table says has no second defence — and this loop is the
beneficiary.

## Decision

**accepted: A — tighten attribution to the named gap, `PROGRESS_RULE` 3**,
answered 2026-08-04 and logged as a `decision-answered` intervention, which is
what makes it answered — `design/loops.md` §16 derives the status from the log
rather than from this line.

§5's sentence is what pays for the round audit's known weakness. Weakening it in
the document to match the code would remove the reason a round audit is
acceptable at all, and it is this loop that benefits — the shape §7's table says
has no second defence. So the code moves, not the spec.

The backtest is what makes this cheap rather than a leap: over 50 closed
campaigns the attributed *term* moves on 13 and the progress *verdict* on none,
because every campaign it takes the audit-side term from had already scored
`sections_clean` at close. So no campaign in the history changes status, and the
redefinition is one of precision rather than of outcome.

The comparability cost is accepted and is bounded by being recorded: rows either
side carry different `progress_rule` values, and `hj progress-replay --rule 2`
recomputes the old answer from the same rows. That is the difference between a
metric redefinition that is legible and one that is lost, which is all §11 asks
for.

`closed_gaps` on the gap log was not part of the judgement and is right
regardless: a gap id is `sha256(section|claim)[:10]` and a closed gap is no
longer in the audit to look up, so the section has to be recorded at the one
moment it is knowable. Additive, so old rows keep working.

## Provisional choice in force

**A.** It is the more reversible of the two: the rule is versioned, every
historical row keeps the `progress_rule` it was written under, and
`hj progress-replay --rule 2` reconstructs the previous answer from the
unchanged session rows — so reverting is deleting one arm, not recovering lost
information. B is the irreversible one, because a spec edit that removes the
claim also removes the gap from the instrument that reports it.

Tagged at `harness/hj`, on `PROGRESS_RULE`.

The backtest is the evidence offered for the choice being safe rather than
merely reversible: no historical campaign's progress verdict changes, so no
loop would have stalled earlier under rule 3 than it did.

## Consequences

If the answer is B, `gap_attribution` reverts to the round-wide reading, the
`closed_gaps` field stays (it is additive and useful regardless), and
`PROGRESS_RULE` goes to 4 rather than back to 2 — a rule number is a record of
what was in force and is not reused. Nothing else is tagged, and no campaign's
recorded verdict has to be rewritten either way, because none of them changed.
