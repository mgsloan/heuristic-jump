---
id: conformance-013
status: accepted
opened: 2026-08-03T10:05:00+00:00
campaign: 7a30ee1a-41d0-48c8-a372-843cd25e8431
kind: class-b
---

# How does the handler-reported half of §7's record cross the frozen seam?

## Context

`core.md` §7 says of the per-query record: "Everything from `stratum_prior`
through `files_parsed` is reported *by the handler*, since only it knows which
resolution path produced the answer and what it cost". That is nine values —
`stratum_prior`, `stratum_final`, `confidence`, `margin`, `considered`,
`stages`, `bytes_scanned`, `files_parsed`, and `stage_us` from the latency
table below it.

`Outcome` carries three: `locations`, `confidence`, and **one** `Stratum`. So
neither consumer of a handler can obtain the rest, and the two that exist write
them at their empty values — `crates/measure_core/src/record.rs`'s
`HandlerReport`, whose doc comment says so and defers the fix to "its own
campaign".

`Outcome`, `Query` and the vocabulary newtypes are inside the seam
`state/phase.toml` freezes at the phase gate, so widening `Outcome` trades
something off and is Class B by the loop prompt's rule, whatever its merits.
The trade is real in one direction: every `lang_*` handler pays for the wider
seam at every `Outcome` construction site, and there are currently two
handlers and eventually seven.

§7 also constrains the *shape* of two of these, not only their presence:

* "The stratum is two fields, not one … Coverage is reported on
  `stratum_prior` so the denominator is fixed by the reference and does not
  move when the implementation changes; precision is reported on
  `stratum_final` so an answer is judged against the class it turned out to
  be. One field cannot do both."
* `stages` is "**bounded** — a small fixed maximum number of short labels,
  truncated rather than grown", "**stable across runs**", and "**nothing
  branches on it**, ever".

## Options

**A. Widen both arms of `Outcome` with `strata: Strata` and `trace: Trace`,
replacing `stratum`.** The reporting channel is the value the handler already
returns, so there is exactly one moment at which a handler's account of itself
is complete and exactly one place it can be read. Cost: every construction site
in every language crate gains two fields, and `CommitPolicy::decide` grows from
three parameters to four.

**B. An out-parameter: `goto_definition(&self, query: &Query<'_>, trace: &mut
Trace)`.** Leaves `Outcome` alone except for `Strata`, and lets a handler
report what it did on the path that returns `Err` — which A cannot, because
there is no `Outcome` on that path. Cost: it changes the trait's own signature,
which is a larger piece of the frozen seam than the enum is; a handler may
forget to write anything and nothing says so; and the trace becomes reachable
for reading *during* the query, which is precisely the shape §7's "nothing
branches on it" rules out.

**C. Interior mutability on `Query` — a cell the handler appends to.** Ruled
out before it is weighed: `CLAUDE.md` forbids locks, and §2 spends a page
arguing that `DocumentSnapshot` contains no synchronisation primitive and no
interior mutability at all, on a `Query` that crosses threads under fan-out.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**A**, and it is the most reversible of the three because it is the only one
that changes no signature a language crate implements. A `lang_*` crate spells
`fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error>` either
way; what changes is the shape of the value it builds, which is a mechanical
edit the compiler drives, at sites the compiler enumerates. B's version of the
same reversal is a trait-signature change in every language crate at once, and
it is the one that cannot be done quietly later.

Two sub-choices inside A, both narrower than the decision itself:

* `Strata` is a struct with private fields and a two-variant `Refinement`
  enum, rather than two bare `Stratum` fields. `Strata::from_reference(s)`
  makes the two agree; `.refine(Refinement)` is the only way to make them
  differ, and `Refinement` has only `AmbiguousName` and `ExternalDependency`,
  so refining to a stratum that is knowable before the search does not
  compile. This is §7's rule made structural rather than remembered.
* `Trace` is write-only from the handler's side. Its fields are private, its
  builders take newtypes, and the only reader is `into_parts`, which consumes
  it — so a handler that read its own stage log back would have to give up the
  thing it still has to return. That is the strongest available form of
  "nothing branches on it" short of a linear type.

Tagged `// DECISION-conformance-013: provisional` at:

* `crates/shared/src/handler.rs` — `Strata`, `Refinement`, `Trace` and the two
  widened `Outcome` arms.

Not tagged at the call sites: they are a mechanical consequence, the compiler
lists them, and tagging seven crates would make the marker noise rather than a
signal.

## Consequences

If the answer is B, `Strata` and `Trace` survive unchanged and only their
delivery moves: the two `Outcome` arms lose `trace`, the trait gains a
parameter, and each handler gains one line. That is roughly an hour's
mechanical work per language crate, and there are two.

If the answer is "record less than §7 asks for", the removal is by field and
the record columns go back to their empty values — the record's *shape* was
already §7's before this campaign and stays so either way.

One thing does not survive a reversal cheaply: `Table` now counts coverage on
the prior stratum and precision on the settled one. If `Strata` collapses back
to one field, that split collapses with it and `high-level.md`'s central table
stops being comparable across versions, which §7 says is the one property it
needs.

## Answer — 2026-08-03T19:00:32+00:00

**Ruling:** accepted

Option A.

**Rationale:** Option A: widen both arms of Outcome with strata and trace.

The deciding argument is section 7's own: B makes the trace reachable for reading *during* the query, which is the shape section 7 rules out when it says nothing branches on it. That is a property, not an inconvenience -- a trace that can be read mid-query is one a handler can condition on, and then the record stops describing the run and starts shaping it. B also changes the trait signature, which is a larger piece of the frozen seam than the enum, and lets a handler silently write nothing. C was correctly ruled out by the record before being weighed: CLAUDE.md forbids locks and section 2 argues Query carries no interior mutability under fan-out.

A's cost is real and accepted: two fields at every construction site in every language crate, and CommitPolicy::decide grows to four parameters.

Reconciling the sites tagged `// DECISION-conformance-013: provisional` is a
normal campaign target, not an interrupt.
