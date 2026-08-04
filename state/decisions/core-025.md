---
id: core-025
status: open
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

Undecided — waiting on a human.

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
