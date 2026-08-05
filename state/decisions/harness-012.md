---
id: harness-012
status: open
opened: 2026-08-05T01:05:00+00:00
campaign: fb78b589-0b53-462a-b22d-f65de1c9a78f
kind: class-b
---

# Does §10's third work counter get a field on `QueryRecord`, or does the claim drop to the two that exist?

## Context

`design/loops.md` names the work counters twice, identically, and both times as
three:

* `#the-metrics-history`, in the row spec — "work counters (bytes read, files
  parsed, nodes visited)".
* `#what-cannot-be-measured-in-isolation` — "Bytes read, files parsed, nodes
  visited — deterministic, machine-independent, local, and strongly correlated
  with the thing that cannot be measured. They go in every row."

The argument for them is load-bearing rather than decorative. The same section
gives up wall-clock latency per iteration — "latency needs a quiet machine",
and with loops running in parallel the machine never is one — and the counters
are what covers the gap until a gate. So they are the only per-iteration signal
that a cost-phase change made the handler do more work.

`crates/shared/src/record.rs`'s `QueryRecord` carries `bytes_scanned: usize`
and `files_parsed: u32`. It carries no node count. `considered: Option<u32>` is
the candidate count the resolver weighed, which is a different quantity — it
counts locations, not syntax nodes, and it is already reported for its own
reasons. So `replay_digest` records two counters and can record no third.

This is not something the harness can fix. `crates/shared/` is the `core`
loop's, and `QueryRecord` is the type both producers of a metrics row write
through — `measure_core::replay` and `driver`'s trace sink — so a field on it
is a change to the shape of a record that two crates and one metrics history
already agree on.

It cannot be settled without trading something off, which is why it is here
rather than in the changelog: the two ways out cost different things and
neither is free.

## Options

**A — add a node counter to `QueryRecord`.** The design gets the third counter
it names twice, and the counter it names is the one most directly attributable
to a handler's own work: bytes and files are properties of what the search
*reached*, and nodes are a property of what it *did* with them. Cost: it is a
field on a type near the seam, so it is the `core` loop's to add and it has to
be threaded from wherever the handler walks a tree — which is `LanguageHandler`
territory, and a counter every handler must remember to increment is a counter
that silently reads zero for the handler that forgets. Also, the number is only
meaningful if every handler counts the same thing, and "a node" is not a
vocabulary term today.

**B — the spec drops to two counters.** `#the-metrics-history` and
`#what-cannot-be-measured-in-isolation` both say "bytes read, files parsed",
and the section's argument is unaffected: two deterministic, machine-
independent, local counters cover the latency gap as well as three, and both
are already computed with no handler cooperation. Cost: it gives up the one
counter that measures work rather than reach. A handler that reads the same
bytes and parses the same files but walks each tree three times is invisible
under B, and that is a real cost-phase regression shape — phase 3 is
output-preserving, so it is exactly where a change adds traversal without
adding I/O.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**B, in the code only, and reversibly.** `replay_digest` records the two
counters that exist and does not emit a zero for the third, which is the state
it was already in — a row that claimed a measurement nothing took would be
worse than a row that is honest about carrying two. The site is tagged
`// DECISION-harness-012: provisional` in `harness/hj`.

The spec is **not** edited to match, deliberately. Under A the harness change
is one line in `replay_digest` and one in `#the-metrics-history`'s row spec;
under B it is two words in two sections. Editing the document now would make
the gap disappear from the audit while nothing had been decided, which is the
one gaming route `design/loops.md` §7 admits the audit cannot catch.

## Consequences

If the answer is A, this campaign's tagged line grows a third counter and the
`core` loop grows a field, a `LanguageHandler` obligation, and a vocabulary
term for what a node is. If it is B, two sections lose two words and
`#what-cannot-be-measured-in-isolation`'s gap closes with no code change at
all. Either way the metrics rows recorded before the answer carry two counters
and are comparable across it, since adding a counter is additive and the two
that exist do not change meaning.
