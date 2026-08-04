---
id: core-017
status: accepted
opened: 2026-08-04T01:05:00+00:00
campaign: 7aa74ea9-28d5-4745-943b-c2296fe4fa93
kind: class-b
---

# What stratum does a query record carry when the hard cap dropped the outcome that knew it?

## Context

`core.md` §7 makes `stratum_prior` the coverage denominator and says why the
choice matters:

> Coverage is reported on `stratum_prior` so the denominator is fixed by the
> reference and does not move when the implementation changes.

The driver's hard cap is at odds with that for one query in every hundred.
`dispatch::hard_cap` turns a `Dispatched::Decided(answer)` into
`Dispatched::DeadlineExpired` when the handler returned after its deadline —
"a non-cooperative handler produces a correctness-neutral waste of CPU rather
than a late answer" (§5) — and `DeadlineExpired` is a unit variant, so the
`Strata` the handler had already assigned goes with the dropped answer. The
same variant is produced by `SnapshotSeed::realise` abandoning a parse, where
no handler ran and there is genuinely no stratum to have.

So when `Actor::requested` assembles the record for an expired query it has no
stratum, and §7's `stratum_prior` is not nullable: it is a `StratumName`, and
every consumer groups on it.

This is not a defect in something already written; it is a question the record
could not ask until something emitted one. It arrives now because
`crates/driver/src/actor.rs` is the first producer of a field row.

## Options

**A — keep `Stratum::Unimplemented`.** No type changes. Costs two things.
Coverage for the stratum the query really belonged to loses a query from its
denominator, which is exactly the movement §7 chose `stratum_prior` to prevent;
and `Unimplemented` acquires a second meaning, where `core.md`'s templates
section makes it self-identifying — "its presence in a metrics table means the
template has not been replaced".

**B — `Dispatched::DeadlineExpired` carries the strata it discarded.** The
handler's own classification survives the cap, which is the honest row. It
needs an answer for the parse-expiry case that never reached a handler, and
both available answers are Class B: a new `Stratum` variant is a change to the
frozen seam, and a nullable `stratum_prior` is a change to §7's record shape
and so to every consumer of the metric.

## Decision

**Not a seam question — the document was ambiguous**, answered 2026-08-04.

Both options priced a cost that does not exist, because the premise under them
is wrong. `core.md` §7 said two things about `stratum_prior` and never
reconciled them: "everything from `stratum_prior` through `files_parsed` is
reported *by the handler*", and "`resolution.md` §8 assigns a stratum a-priori
*from the reference* … the denominator is fixed by the reference". Read
together they suggest the field is the handler's to produce and therefore the
handler's to lose.

*A-priori* is about the **rule**. The handler evaluates it; what makes the
prior stable is that the rule reads only the query and the reference and never
what the search found. So the prior is knowable without the search finishing,
and **a query whose outcome the hard cap discards has not lost its prior** —
it was never the outcome's to carry away. §7 now says this in as many words.

That makes **A wrong** and **B free**. A loses a query from its true stratum's
denominator, which is the movement `stratum_prior` exists to prevent, and
overloads `Unimplemented`, which `core.md` makes self-identifying. B needs
neither a new `Stratum` variant nor a nullable field: the prior is available
before the outcome is, so `DeadlineExpired` carrying it — or the driver
holding it from before dispatch — costs nothing on the frozen seam.

The parse-expiry case resolves the same way. A query abandoned before any
handler ran still has a prior, because the reference and the query are all its
rule needs.

**Work left, and it is ordinary rather than an escalation:** make the prior
reachable without a completed outcome, and record it for capped and abandoned
queries. That is a normal campaign target.

## Provisional choice in force

**A.** It is the reversible one: it is a value at a single site rather than a
type anything else names, and the only thing that persists is the trace rows of
runs already made. B changes a public enum, a seam type or the record shape,
and each of those is read by code this loop does not own.

Tagged at `crates/driver/src/actor.rs`, in `Actor::answer`'s
`Dispatched::DeadlineExpired` arm — the one place a stratum is invented.

## Consequences

If the answer is B, the change is the enum variant, the two `dispatch.rs` sites
that construct it, `files.rs::observe`'s match, and the tagged arm — under ten
lines, plus whichever of the two sub-questions the answer picks. Trace rows
written under A are not migrated: a deadline-expired row is identifiable by
`stages` containing `abstain:deadline`, so a corpus that needs the distinction
can recover it, and one that does not can drop the rows.
