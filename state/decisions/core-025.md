---
id: core-025
status: accepted
opened: 2026-08-04T20:11:57+00:00
campaign: 20bbc1bf-03c5-4d3c-afda-a5c5791d47ce
kind: class-b
---

# What stratum does a query carry when its deadline expired before anything classified it?

## Context

`core-017` is answered and this campaign implemented it: the hard cap drops the
*answer* and keeps the classification, so a handler that merely ran slow no
longer moves a query out of its own coverage denominator. `Dispatched::DeadlineExpired`
now carries `ExpiredStrata::Assigned(strata)` on that path.

`core-017`'s ruling went one step further than the code can:

> The parse-expiry case resolves the same way. A query abandoned before any
> handler ran still has a prior, because the reference and the query are all its
> rule needs.

That is true of the *rule* and not reachable by the *driver*. The rule is
`resolution.md` §8's and it is the handler's to evaluate; the driver has no
reference, no resolution vocabulary, and — by `core.md` §1's design — no way to
ask for one. Two paths arrive with nothing:

* **The parse was abandoned.** `SnapshotSeed::realise` gave up on the deadline,
  so no handler ever saw the document. There is genuinely nothing that could
  have classified it, and this is the case `tests/snapshots.rs` asserts.
* **The handler propagated an expiry with `?`.** `ProjectView` fails a read past
  the deadline (`shared/src/project.rs`), and `core.md` §1 says in as many words
  that the wrapper converts that class back into an abstention *because*
  handlers do ordinary `?` propagation. But the seam's `Result<Outcome, Error>`
  gives an `Err` no way to carry a stratum, so a handler that had already
  classified the reference loses it on the way out — which is `core-017`'s
  defect again, one layer down and behind the seam rather than in front of it.

The second is the one that will matter in the field. `resolution.md` §8 assigns
the prior from the reference *before* the search, and the search is where the
I/O and therefore the expiries are, so the common shape is a handler that knew
the stratum and returned `Err` from a read.

## Options

**A — a `Stratum` for it.** An `Unclassified` variant, so the row says what is
true instead of borrowing `Unimplemented`'s meaning. Costs a variant on the
frozen seam, and every `lang_*` and every metrics consumer gains a case.
`core.md` §1's list is one row per `high-level.md` stratum plus the template
placeholder, so a third kind of member changes what the enum *is*.

**B — `stratum_prior` becomes nullable in §7's record.** The honest shape: there
was no prior, and null says so where any stratum name is a guess. Costs a change
to §7's record — the field order is asserted against the document
(`measure_core/tests/pipeline.rs`), the shim and replay both emit it, and every
consumer that groups on it gains an empty bucket. `core-017` priced this as
Class B and it still is.

**C — the seam lets a handler report a prior with a failure.** Aims at the case
that will actually occur: `ProjectView`'s expiry carries the strata the handler
had, or the handler is expected to return `Ok(Abstain { reason: Deadline, .. })`
rather than `?`. The first is a change to `Error`; the second is a convention
`core.md` §1 explicitly declines to require, because requiring it is how the
`?`-propagation argument that section makes gets undone.

Not exclusive: C plus A-or-B is the complete answer, since C narrows the residue
to the abandoned parse and does not empty it.

## Decision

**accepted: C plus B — the seam carries the prior out with the failure, and
`stratum_prior` becomes nullable for what is left**, answered 2026-08-04 and
logged as a `decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

C first, and C in its first form only: `ProjectView`'s expiry carries the strata
the handler had, as a change to `Error`. Not the second form. Requiring handlers
to return `Ok(Abstain { reason: Deadline, .. })` instead of `?` is the
convention `core.md` §1 declines to require, and it declines for a reason that
this record does not overturn — the `?`-propagation argument that section makes
is undone the moment a handler has to remember not to use it.

C is chosen because this record identifies the common case correctly: §8 assigns
the prior from the reference *before* the search, and the search is where the
I/O is, so the shape that will actually occur in the field is a handler that
knew the stratum and returned `Err` from an expired read. Fixing only the
residue would leave that case losing information at the seam, which is
`core-017`'s defect one layer down.

Then B rather than A for the residue. `Stratum` is meant to be one row per
`high-level.md` stratum plus the template placeholder, and "nothing ever looked
at this reference" is not a kind of reference — it is the absence of a
measurement. Putting it in `Stratum` makes every `lang_*` match a case it can
never produce and creates a bucket that appears in every coverage table as
though references of that kind existed. `null` says the true thing in the place
the absence actually lives, and it forces each consumer to decide what to do
with it rather than letting it be grouped away silently.

B is the largest of the three changes and that is accepted knowingly. It is
paid once, and it is cheapest now: the corpus is small, and every later day adds
producers and consumers of §7's record.

### What is left

The core loop's, as an ordinary campaign. `ExpiredStrata` is already where the
answer lands either way, which is what this campaign got right — the enum with a
named case rather than an `Option<Strata>` with a convenient default is what
makes this a one-arm change rather than a rewrite. Reconcile the
`DECISION-core-025: provisional` tag at `crates/driver/src/actor.rs` when the
arm is implemented.

`core-022` and `core-024` are the same question and are closed as duplicates of
this record.

## Provisional choice in force

**`Stratum::Unimplemented`, unchanged from before this campaign**, at
`crates/driver/src/actor.rs` in the `ExpiredStrata::Unclassified` arm, tagged
`// DECISION-core-025: provisional`.

It is the reversible one for the reason `core-017` gave for the same value: it
is a value at a single site rather than a type anything else names, and all
three options above change something a crate this loop does not own reads.

**What has changed is the size of what it is wrong about**, and that is the
argument for shipping it rather than waiting. Before this campaign every capped
answer took this value; now only a query that nothing classified does. The
residue is a real defect — `measure_core`'s `Table::template` still reads an
abstention under `unimplemented` as an unreplaced language template, so a
corpus with abandoned parses in it can still report one — and it is bounded by
how often a parse misses its deadline, where before it was bounded by how often
a handler ran slow.

## Consequences

If the answer is A, the change is the variant, this arm, and `measure_core`'s
`StratumName` rendering — under ten lines, plus a case in each `lang_*` that
matches exhaustively on `Stratum`, of which there is currently one.

If B, the change is §7's record shape and so `shared::record`, both producers,
`Table::observe`, and `pipeline.rs`'s field-order fixture. The largest of the
three.

If C, `ExpiredStrata::Unclassified` becomes reachable only from the abandoned
parse, and A or B is still needed for that. The work already done does not move
either way: `ExpiredStrata` is where the answer lands whichever it is, which is
why it is an enum with a named case rather than an `Option<Strata>` with a
convenient default.
