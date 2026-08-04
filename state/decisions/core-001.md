---
id: core-001
status: accepted
opened: 2026-08-04T00:48:00+00:00
campaign: e797a506-29e9-4cb6-99b2-714f5a785c1a
kind: harness-request
---

# Who writes `harness/measure`, which `core.md` §7 names and no loop may create?

## Context

`core.md`'s "the table is not enough: a replay has to show its failures" splits
one job across two owners, and names both:

> **Digesting those into something readable is the harness's job**, not
> `measure_core`'s — the same split that keeps `measure_core` ignorant of
> `state/`. `harness/measure` runs the replay, prints the table, and writes a
> failure digest beside it.

`measure_core`'s half is done and held. `replay --records <path>` writes the
unfiltered per-query JSONL, with no `--records` it writes nothing, and both
digest keys the section specifies are now asserted to fall out of an exact
string group-by: `(stratum_prior, reason, stages)` for coverage loss and
`(stratum_final, agreement, severity, stages)` for precision loss
(`crates/measure_core/tests/pipeline.rs`).

The harness's half does not exist. There is no `harness/measure`, and nothing
under `harness/` reads a records file. This is not something a loop can fix:
`harness/**` is denied to every loop, and correctly so — "a loop must not be
able to weaken the thing that scores it".

It is worth being precise about why this matters more than a missing
convenience script. The section's argument is that a tuning campaign handed a
summary statistic can form no hypothesis, and the digest is what turns a
thousand failures into a finding. Until it exists, the first language campaign
either reads raw JSONL by hand — which is the "random twenty of them is an
anecdote" failure the section rejects by name — or writes its own grouping,
which forks the measurement in the way the loop prompt says nothing downstream
can detect.

## Options

**Write `harness/measure` now, before any handler exists.** The shape is fully
specified — group, count, share of stratum, then a small seeded sample — and
the record it reads is frozen and tested. Cost: it is written against a corpus
that produces one abstention reason and nothing else, so nothing exercises the
grouping until a real handler lands, and it is harness code nobody can iterate
on from inside a loop.

**Leave it until the first language campaign needs it.** Cost: that campaign
discovers the absence at the point it is trying to tune, and the cheapest thing
in front of it is to read rows by hand or to build a private digest — the two
outcomes the section is written to prevent. The absence is also invisible from
inside the loops: the audit's `where:` fields all point into `crates/`, so a
section can go clean on the measurement's half alone.

## Decision

**Now, and the harness loop builds it**, answered 2026-08-04.

The record's framing needs one correction: "this is not something a loop can
fix" was true when it was written and is not now. `harness/**` is denied to
every loop *except* one declaring `writes_harness`, which the harness loop
does — and this record is already spliced into that loop's prompt under
"Harness requests waiting on you". So the question was never who, only when.

**When is now**, for the reason `loops.md` §18 gives for that loop existing:
phase 1.5 is roughly a hundred machine-hours with no model in it, the harness
loop has nothing else to build in that window, and `harness/measure` is
exactly the phase-2 machinery §18 points it at.

The objection to building early — that nothing exercises the grouping while
the corpus yields one abstention reason — is already answered by the half that
is done. `crates/measure_core/tests/pipeline.rs` asserts both digest keys fall
out of an *exact* string group-by, so the harness half can be built and tested
against synthetic records today and meet real ones later without its shape
being a guess.

The alternative's cost is the one the section names by itself: a tuning
campaign that finds the digest missing reads raw JSONL by hand — "random
twenty of them is an anecdote" — or writes its own grouping, forking the
measurement in the way the loop prompt says nothing downstream can detect.
That cost lands on a campaign with no way to escalate out of it mid-tuning,
which is the worst place to put it.

## Provisional choice in force

The second, because it is what doing nothing already amounts to and because
this campaign cannot take the first. No sites are tagged: nothing in
`crates/measure_core` is provisional, and the choice is entirely about a file
in a directory this loop may not write.

What is in force meanwhile is that `--records` is the whole interface. Anything
that wants a digest reads that JSONL and groups it on the two keys above, which
the tests now pin, so a digest written later against those keys cannot disagree
with one written by hand today.

## Consequences

If the answer is "write it", the work is entirely inside `harness/` and nothing
in `crates/` changes — the record shape, the field order and the two keys are
already what the section specifies and are already tested. If the answer is
"not yet", the thing to watch is the first language campaign: if it produces
its own grouping rather than asking again, the measurement has forked and the
tuned/held-out gap (`loops.md` §12) is the only remaining detector.
