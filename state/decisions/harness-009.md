---
id: harness-009
status: accepted
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

**accepted: the third option, with the log made part of it — flag it, and
record the flag as a `prompt-revised` intervention**, answered 2026-08-04 and
logged as a `decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this line.

The record is right that the two §18 sentences cannot both be satisfied, and
right that denying the loop its own prompt is the reading that honours §18's
principle. What decides against it is that the defect §16 actually names is not
*that* the prompt was edited — two gaps were legitimately closed that way, and
the claims genuinely live in the prompt — but that the edit **was not recorded**.
A prompt revision is the one intervention that cannot be replayed, so what an
analyst needs is a row in the log marking where the generator changed. Denial
would have delivered that as a side effect; logging delivers it directly, and
keeps the route §18 assigns to this same loop in the same paragraph.

This is the answer `harness/readme.md` already gives for the Class A hole, in
its own words — the honest version is that it is made *visible* rather than
impossible — with the gap that the visibility stopped at the dashboard now
closed.

**What tips it, and what would tip it back.** The evidence is two edits, both
honest, which the record correctly says is evidence about two campaigns rather
than about the mechanism. That is why the log matters more than the flag: it is
what makes the next twenty edits countable, and `prompt-revised` rows
accumulating faster than gaps close is the signal that would justify revisiting
this and denying the template outright. The record exists to be answered from
evidence, and now there will be some.

### Done in the same commit as this ruling

`log_prompt_revision` appends a `prompt-revised` row at campaign close, with the
campaign id, the files, whether the template was the campaign's own, and a
rationale saying the harness wrote it rather than a person — nobody is present
at a close to give one, and a row that pretends otherwise is worse than one that
says so. A human can still add their own row with reasoning.

Only *live* templates, via `live_prompt_templates`: §18 gives this loop the
tuning and optimisation prompts, and editing a prompt for a phase that has not
run changes no generator and makes no measured metric incomparable. The commit
is folded into the close, so a campaign that revised a prompt and one that did
not leave the same number of commits behind.

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
