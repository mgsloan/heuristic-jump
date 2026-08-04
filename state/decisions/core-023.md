---
id: core-023
status: accepted
opened: 2026-08-04T20:04:16+00:00
campaign: 20bbc1bf-03c5-4d3c-afda-a5c5791d47ce
kind: class-b
---

# Does the licence surface get a `cargo-deny` config, or do the workspace tests hold it?

## Context

`deps.md` §14's last bullet asks for one:

> a cargo-deny config asserting GPL reaches the graph only through
> `vendor/rope` and `crates/similarity` is worth having from the start,
> because it is what notices a third arriving without anyone deciding

It is the one gap left in `deps.md#14-workspace-cargotoml-shape`
(`d822e97954`), and no loop can close it as written. Two separate obstacles,
and they point at different answers:

* **The path is unowned.** `deny.toml` sits at the repository root and is in no
  loop's write list. Measured rather than assumed: writing it fails the gate at
  step 4, `deny.toml: outside core's owned paths`.
* **The tool is not installed.** `cargo deny` is `error: no such command`, so a
  config committed today asserts nothing until someone adds the tool to
  whatever runs checks. A config file for a tool nothing runs is worse than an
  absence, because the bullet it satisfies reads as covered.

What has changed since the bullet was written is that the property it names is
now checked. `crates/driver/tests/seam.rs` holds the licence surface from three
sides: `the_gpl_inputs_are_the_two_the_documents_name` compares
`high-level.md`, §5's table and the manifests;
`every_member_declares_the_licence_section_5_assigns_it` holds every member's
field; and this campaign added
`the_permissive_surface_is_exactly_what_does_not_reach_similarity`, which is
specifically the "third arriving without anyone deciding" case — a fourth
member gaining a `similarity` dependency fails it without any `license` field
having moved.

So the question is not whether the property is worth holding. It is whether
`cargo-deny` buys anything over the tests that now hold it, and that trades
something off in both directions, which is why it is not a Class A edit to the
bullet.

## Options

**A — adopt `cargo-deny`.** It reads the resolved dependency *graph*, where the
tests read manifests. That difference is real and is not cosmetic: a GPL crate
arriving transitively through a permissive direct dependency is invisible to a
manifest scan and is exactly the case the bullet's "without anyone deciding"
describes. It also covers advisories and duplicate versions for free.

Costs: a tool in the check path (`loops.md` §17 territory), a root file no loop
owns, and a second place the licence policy is written — the tests already
encode it, and a policy in two files is a policy that drifts. Someone has to
decide where it runs, since a config nothing invokes is decoration.

**B — the workspace tests are the mechanism, and §14's bullet says so.** Delete
nothing and add nothing; amend the bullet to name the tests that hold the
property, and record the transitive-graph case as the gap that leaves.

Costs: the transitive case stays unchecked. Nothing today reaches it — every
GPL input in the graph is a workspace member, so a manifest scan and a graph
scan currently give the same answer — but "currently" is doing the work in that
sentence, and the day it stops being true is the day the check was for.

## Decision

**accepted: A — adopt `cargo-deny`**, answered 2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

The record makes its own case: a manifest scan cannot see a GPL crate arriving
transitively through a permissive direct dependency, and that is precisely the
case §14's bullet describes as happening "without anyone deciding". Option B's
cost is not that the check is weaker but that it is absent exactly when it
matters. Every GPL input in the graph is a workspace member today and both
scans agree — and "today" is the whole of that argument.

The objection worth answering is the real one: a policy written in two places
drifts. The resolution is that they are not two statements of one policy.
`the_permissive_surface_is_exactly_what_does_not_reach_similarity` is a claim
about *our* crates and their direct manifests, which is a design property the
tests should keep asserting. `deny.toml` is a claim about the resolved graph,
which no test can reach. Keep both, and say in `deny.toml` which half it holds.

### What is left, and who does it

A human writes `deny.toml` and wires it into the check path: it is a root file
no loop owns, and `harness/gate` is denied to every loop, which is why
`core-021` was filed as a `harness-request`. `loops.md` §17's adopt/steal/reject
note should record the adoption and what it bought.

Where it runs matters, since a config nothing invokes is decoration. It belongs
in the gate rather than in a test, because it needs the network on a cold
registry and the tests must not.

Nothing written by campaign `20bbc1bf` has to be undone; the work is additive.
The tag at `crates/driver/tests/seam.rs` comes off when `deny.toml` lands, and
§14's bullet is then satisfied as written rather than amended.

`core-021` is the same question and is closed as a duplicate of this record.

## Provisional choice in force

**B, and only the half that costs nothing to reverse.** No `deny.toml`, no tool
adopted, and **§14's bullet is left exactly as written** — an unamended bullet
is an open gap, which is the honest state, where an amended one would close the
gap by describing the thing that did not happen. That is the reversibility that
matters here: option A remains available at its full original cost, and nothing
downstream has been told the question is settled.

Tagged at the one site that would change if the answer is A:
`crates/driver/tests/seam.rs`, on
`the_permissive_surface_is_exactly_what_does_not_reach_similarity` — the test
that stands in for the config's stated purpose.

Not tagged, deliberately: `design/deps.md` §14. Tagging the bullet would put a
provisional marker on the requirement rather than on the thing standing in for
it, and the requirement is not what is provisional.

## Consequences

If the answer is A, the test stays — it is a manifest-level check and cheap —
and gains a `deny.toml` beside it plus a decision about what runs it. The
tagged comment comes off and §14's bullet is satisfied as written. Nothing
written this campaign has to be undone; the work is additive.

If the answer is B, §14's bullet is amended to name the tests, which is a Class
A edit somebody makes deliberately rather than a campaign making it while
closing its own gap — the shape the spec-drift rule watches for, and the reason
this is a record rather than an edit.

The gap `d822e97954` stays open either way until then, so
`deps.md#14-workspace-cargotoml-shape` cannot go clean this phase. That is a
section the loop's number will not move, and the reason is here rather than in
a campaign that looks stalled.
