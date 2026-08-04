---
id: harness-009
status: open
opened: 2026-08-04T21:35:00+00:00
campaign: 78bbbbc4-9003-447e-9139-61389562ceb5
kind: class-b
---

# May a loop edit the prompt that generates it?

## Context

`design/loops.md` §18, "The conformance loop builds the followup", says what
this loop may write when it turns around and builds the phase-2 machinery:

> At that point `harness/gate*`, `harness/prompts/` and the auditor stay
> denied to it (they judge it *now*) while `harness/supervisor/` and
> `harness/dashboard/` become writable (they judge phase 2a, later).

The code says something else. `DENIED_ALWAYS` names four files —
`harness/gate`, `harness/prompts/auditor.md`, `harness/section-baseline.toml`,
`harness/ratchets.toml` — and `writes_harness` grants the rest of `harness/**`.
So `harness/prompts/conformance.md` is writable by the harness loop, and that
file is both this loop's own instructions and the core loop's.

It has been used. `0c17d8b` closed `loops.md#campaigns-compare-notes-asymmetrically`
and `d94f76a` closed part of §4 by adding paragraphs to `conformance.md`, and
both are honest gap closures: the claims are about what a loop is told, so the
prompt is where they live. §18 also gives this loop the job of writing the
tuning and optimisation prompts, which are files under `harness/prompts/`.

Against that, §16: a prompt revision is the one intervention that cannot be
replayed, because it changes the *generator* of campaigns rather than a
campaign — so metrics either side of it are not comparable, and it is supposed
to be logged as an intervention. Nothing was logging it. And a loop editing the
text that instructs it is the strongest available form of the failure
`harness/readme.md` exists to prevent; that the two uses so far were honest is
evidence about two campaigns, not about the mechanism.

## Options

**Deny the loop its own prompt** — `harness/prompts/<config.prompt>` added to
the denied set for a `writes_harness` loop, leaving the rest of
`harness/prompts/` writable so the tuning prompts can still be written. Costs
the route by which two gaps were legitimately closed: every prompt-shaped claim
in `loops.md` becomes a human's edit, and the loop can only file a request. It
is also asymmetric in an awkward way — the harness loop could still edit
`conformance.md`'s *fragments*, which are spliced into the same prompt, unless
the denial covers those too.

**Deny all of `harness/prompts/`**, as §18 literally says. Costs the tuning and
optimisation prompts, which §18 assigns to this same loop in the same
paragraph. The two sentences cannot both be satisfied, which is part of why
this is a decision rather than a fix.

**Flag it and log it, as `spec_drift` does.** Costs nothing in reach and buys
no prevention: it makes the edit visible to a human at campaign close and on
the dashboard, and turns an unrecorded prompt revision into a recorded one.
`harness/readme.md` already says of the Class A hole that "the honest version
is that it is made *visible* rather than impossible", and this is the same
shape.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

The third. `prompt_drift` in `harness/hj` records any campaign whose commits
touch `harness/prompts/`, marks separately whether the file edited is the
template that campaign's own prompt was rendered from, writes it to the session
row as `prompt_drift`, and prints it at close.

It is the most reversible of the three by a wide margin: it removes no access,
so answering it either way is a change to `DENIED_ALWAYS` and nothing else, and
until then the record exists to answer *from* — how often this happens, and
whether the edits look like claims being satisfied or like instructions being
softened.

No site carries a `// DECISION-harness-009: provisional` tag: the provisional
choice is an absence of denial, which has no line to tag. The docstring on
`prompt_drift` names the record instead.

## Consequences

If a denial is chosen, the sites are `DENIED_ALWAYS` and `denied_paths`, and
`prompt_drift` becomes redundant for the loop's own prompt while staying useful
for the rest of the directory. Nothing already committed has to be undone —
both past edits are in the history either way, and a denial is not retroactive.

If flagging is chosen, this record should say so explicitly rather than being
closed silently, because the next campaign to read §18 will find the same
discrepancy and re-raise it.

The one thing that should not happen is §18 being edited to match the grant.
This loop is the beneficiary of that grant, and a spec edit that widens its own
reach is the exact shape `harness/readme.md` says the audit cannot catch.
