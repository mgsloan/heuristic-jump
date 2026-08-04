---
id: harness-001
status: accepted
opened: 2026-08-04T00:12:00+00:00
campaign: 11b9c019-6714-4563-a97b-fd9a00c5819f
kind: class-b
---

# Should the campaign id move out of the prompt's instructional body, at the cost of a prompt revision?

## Context

`design/loops.md` section 15's first token lever:

> Keep the prompt's fixed prefix byte-identical across iterations and order
> it stable-to-volatile — constitution, then prompt, then audit state, then
> the journal tail and recent commits — so the cacheable prefix is as long as
> possible.

The ordering half holds: `harness/prompts/conformance.md` puts the whole
instructional body first and the audit state, campaign list, findings and
spliced sections last. The byte-identical half does not, and `hj
prompt-prefix` (added this campaign) measures by how much:

```
prompt-prefix: core     (conformance.md, 40,606 chars rendered)
  cacheable prefix   4,834 chars (11.9%)   ended by {{campaign_id}}
  state block at     18,362 chars (45.2%)
prompt-prefix: harness  (conformance.md, 26,709 chars rendered)
  cacheable prefix   7,800 chars (29.2%)   ended by {{campaign_id}}
  state block at     21,264 chars (79.6%)
```

`{{campaign_record}}` and `{{campaign_id}}` appear four times inside the
instructional body — steps 1, 4 and 5 name the record's path, and the
changelog template names the id. Each contains the campaign uuid, which
differs every campaign, and prompt caching is prefix-based: the first
differing byte ends the saving for everything after it. So 13.5KB of body
that is identical in every campaign is re-sent as uncached input each time,
on both loops.

The size of the prize, from the campaigns measured so far: cache reads run
about 141 to 1 against output tokens, and the core loop has read 47M cached
tokens across 27 campaigns. Extending the prefix from 11.9% to 45.2% is not a
rounding error on that.

This is not a defect I can fix. Section 16 singles out a prompt revision as
**the one intervention that cannot be replayed** — it alters the generator of
campaigns, past campaigns cannot be regenerated under the new prompt, and
metrics either side of it are not strictly comparable with nothing downstream
able to detect that. `CLAUDE.md`'s rules-hygiene section says the same of its
own file. Section 18's table also lists `harness/prompts/` as denied to this
loop while it builds the followup, on the argument that those files judge it
now.

## Options

**A. Move the four id references to the volatile tail.** The body would say
"your campaign record — its path is at the end of this prompt", and the
existing state block would carry the path and id once. Cost: a prompt
revision, so a discontinuity in the metrics series, plus the risk that a
campaign is worse at finding its own record when the path is not next to the
instruction that uses it. Benefit: the measured 13.5KB per campaign on every
loop, growing with the number of loops.

**B. Leave it, and treat the number as documentation.** Cost: the token
lever section 15 names first is not being pulled, permanently, and it gets
worse as phase 2a adds six more loops. Benefit: no discontinuity, and the
prompt keeps the property that every instruction is self-contained where it
is read.

A third framing worth naming, since it changes the trade: if the revision is
going to happen at all, doing it **before** phase 2a is much cheaper than
after — the 2a series has not started, so there is no comparability to break
on the loops that will do the expensive work.

## Decision

**Option A — move the four references to the volatile tail**, answered
2026-08-04. Applied together with the prompt's opening correction, as one
revision.

Two things this record undercounts, both in A's favour. The core loop now runs
**three workers that start within seconds of each other against one template**,
so a long shared prefix is paid for once and hit twice — the saving scales with
worker count, and the record reasons as though campaigns were serial. And the
prompt's opening is being rewritten regardless: it claims "Nothing else you do
counts as progress" about sections clean, which is false, and under workers
`tests up` is the only term that can fire.

So the discontinuity §16 warns about is being spent anyway. Spending it once on
both edits is strictly better than twice, and the moment is the cheapest there
will be: before phase 2a, and after several revisions already made today.

`hj prompt-prefix` stays, as this record proposes under B — it is what would
notice a future edit shortening the prefix again, and it is how the change is
verified rather than assumed.

## Provisional choice in force

**B, plus the measurement.** No prompt file was touched. `hj prompt-prefix
[<loop>]` is the whole of what this campaign did here: it renders a loop's
prompt for two probe campaign ids, reports the first divergence as a
fraction, names the template value that caused it, and says how much of the
instructional body is being re-sent. It reports and never fails, so it cannot
turn into a gate that forces the revision by the back door.

This is the reversible option in the strict sense: a measurement can be
deleted and nothing is lost that cannot be re-derived, whereas a prompt
revision cannot be un-run — the campaigns generated under it are the
artifact.

No sites are tagged `// DECISION-harness-001: provisional`, because nothing
was changed to tag. The tag would belong on the four references in
`harness/prompts/conformance.md`, which this loop may not write.

## Consequences

If the answer is A, the work is four edits to one prompt file and one
addition to the state block — under an hour — plus an intervention log entry
of kind `prompt revised`, and a note on the metrics series saying campaigns
either side are not strictly comparable. `hj prompt-prefix` then reports the
new number, which is how the change is verified rather than assumed. Nothing
this campaign built has to be redone either way; the measurement is what
either answer is made from.

If the answer is B, `hj prompt-prefix` is still worth keeping as a
regression check: it is what would notice a future prompt edit that moves a
per-campaign value further forward and shortens the prefix again.
