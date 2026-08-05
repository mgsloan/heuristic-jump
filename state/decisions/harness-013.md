---
id: harness-013
status: open
opened: 2026-08-05T02:10:00+00:00
campaign: 2953c426-61d6-4c26-ab02-4de263107557
kind: class-b
---

# May a loop's prompt state §7's five forms of progress, or is that telling it how to score itself?

## Context

`design/loops.md` §14 says the prompts "do not describe the gate's
internals, since a loop that knows how it is scored is a loop that can
optimise the scoring". The live conformance prompt opens with the
opposite:

> `design/loops.md` §7 counts five, all computed from the repository: a
> section going clean, the audit's gap count falling, the test count
> rising, a frontier point added that nothing dominates, a decision item
> resolved. […] That leaves the test count and reconciling an answered
> decision as the two you can move today, and it is why a campaign that
> closes a gap and writes no test can still register nothing.

Two of §7's own gaming routes are "delete or weaken tests" and "split one
item into ten to show motion", and the paragraph above tells a campaign
in as many words which lever it can pull today. That is the shape §14 is
warning about.

The other side is why the paragraph exists. §7 judges stall on a campaign
closing with *none* of the five, and the first two — a section going
clean, the gap count falling — are read out of an audit that runs after
the campaign exits. So a campaign that closes a real gap and writes no
test genuinely registers nothing at the moment it closes, and campaigns
were closing that way. A loop that does not know this cannot tell an
honest flat campaign from a wasted one, and §7 asks a *stalled* loop to
write down "what it believes is blocking it", which is not answerable
without knowing what progress means.

This is not the same question as the one this campaign closed. Naming
`check-metrics` in the prompt is a pure leak with nothing on the other
side, and it is now checked mechanically. This one has something real on
both sides, which is why it is here rather than settled in a commit
message.

## Options

1. **Keep the paragraph.** The loop knows what counts and can aim a
   campaign at a test that carries a claim. Cost: the two cheapest
   progress terms are stated as levers, and §7 has no second mechanism
   behind the test-count row of its table beyond the audit judging the
   claim.
2. **Cut it to the objective alone** — sections clean, and nothing about
   the other four. Cost: an honest campaign that closes a gap and writes
   no test looks flat to itself and to the stall detector, and the loop
   cannot say why in its handoff. This is the state that produced the
   paragraph.
3. **Keep it and remove the incentive instead**: make the audit-derived
   terms attributable to the campaign that earned them, so a campaign
   closing a gap scores when the next audit confirms it rather than
   needing a test on the day. Strictly more work than either of the
   others and it changes what a metrics row means, which is a metric
   redefinition on its own.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Option 1, unchanged. It is the reversible one in the exact sense that
matters here: it is the status quo, so taking it changes no campaign's
behaviour, and reversing it later is a prompt edit whose effect is
visible in the next campaign rather than a data migration. Option 2 is
equally cheap to apply but not to undo — the campaigns run under it close
differently, and metrics either side of a prompt revision are not
comparable (§16), so trying it and changing back costs the comparability
of everything between.

Tagged nowhere in code: the site is prose in
`harness/prompts/conformance.md`, above the `# What you may write`
heading, and a `// DECISION-…` marker in a prompt would be spliced into
every campaign's context. `design/loops.md` §14 carries the pointer
instead, in the paragraph this record is named from.

## Consequences

If the answer is option 2, the opening paragraph of the conformance
prompt is cut back to the number and the sentence about the audit running
after the loop exits, and §14's new paragraph drops its last clause. No
code changes and nothing has to be redone; what changes is that campaigns
after the edit are not comparable to campaigns before it, and the
intervention log has to say so.

If it is option 3, the work is in `cmd_campaign_close` and the audit
merge, and it is a metric redefinition — a rule number and a backtest,
like `PROGRESS_RULE` 3 → 4.
