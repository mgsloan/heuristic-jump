---
id: conformance-008
status: open
opened: 2026-08-03T06:30:00+00:00
campaign: bc8f02bb-1cb1-48d7-8814-a22f8a2b8481
kind: harness-request
---

# Who creates `crates/similarity/`, given that it is denied to every loop?

## Context

Raised from target selection rather than from an implementation, which is why
it is worth writing down: it silently removes sections from the reachable set,
and the next several campaigns will each rediscover it.

`crates/similarity/**` is on the loop prompt's *denied to every loop, in every
phase* list. `core.md` §9's dependency graph names it as a node — "similarity
(frozen, depends on `shared`)" — and has every `lang_*` crate depend on it:

> `lang_*` (depend on `shared` and `similarity` plus their grammar crate)

and `#adding-a-language` makes phase 1a responsible for the template that has
that dependency:

> Phase 1a builds the language-crate template as an instantiable template:
> `crates/lang_<x>/{Cargo.toml, src/lang_<x>.rs}` depending on `shared` +
> `similarity` + its grammar.

`crates/lang_rust/**` *is* in phase 1a's write list, and is named rather than
globbed on purpose (`state/phase.toml`). So the loop is asked to build a crate
whose manifest, as specified, names a package that does not exist and that the
loop may not create. `#the-dependency-graph` cannot go clean from this loop at
all, and `#adding-a-language` can only go clean if the template's manifest is
allowed to omit `similarity`.

This has a precedent with an answer: `conformance-002` asked the same question
about `rust-toolchain.toml` and the `LICENSE-*` texts, which are also absent
from every write list, and it was answered by a human placing the four files.

## Options

**A. A human places `crates/similarity/` the way the licences and the
toolchain pin were placed** — at minimum a manifest and a library root, even
if it is empty until phase 2. Costs one human action, and it is the option
that makes §9's graph checkable as written. It also settles what "frozen"
means: a crate no loop may edit is a reasonable thing to want, and a crate no
loop may *create* is probably not what was intended.

**B. The template omits `similarity` until something needs it**, and §9's
graph is edited to say a `lang_*` crate depends on it *when it uses it*. Costs
nothing now and defers the question, but it edits a dependency-graph claim to
match what the loop is permitted to build, which is the shape the loop prompt
says to be suspicious of — and it would be this loop making the edit, in the
document under audit. It also does not help `#the-dependency-graph`, whose gap
is that the node is missing on disk.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Neither, yet — and nothing is tagged, because nothing has been written against
either reading. This campaign targeted `#vocabulary-types` and did not touch
`lang_rust`; the escalation is filed at the point of *discovery* rather than
at the point of need, so that the campaign which does target
`#adding-a-language` finds an answer already here instead of spending itself
on this.

If a campaign reaches the template before this is answered, the reversible
move is **B applied to the manifest only** — write
`crates/lang_rust/Cargo.toml` without `similarity`, tag it
`// DECISION-conformance-008: provisional`, and leave `core.md` §9 alone. That
keeps the divergence visible to the auditor as a gap rather than hiding it in
a document edit, which is the direction that can be undone.

## Consequences

If the answer is A, the template's manifest gains one line and
`#the-dependency-graph` becomes reachable at all. If it is B, one sentence in
§9 changes and `#the-dependency-graph` stays permanently un-cleanable by this
loop, which is worth knowing explicitly rather than as an unexplained
plateau — the loop's number is sections clean over sections total, and a
section that cannot move is a denominator the loop is being scored against
without a way to affect it.
