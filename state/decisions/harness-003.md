---
id: harness-003
status: open
opened: 2026-08-04T02:10:00+00:00
campaign: 8564e2f1-4e5b-4e5d-bfbd-76e363b98d6b
kind: class-b
---

# The audit cadence is a knob and two sections say it is not one — is the knob the deviation, or the claim?

## Context

Two sections make the same claim in different words.

§5, "The auditor and the conformance loop's number":

> at every campaign close, a separate session with no memory of writing the
> code judges the implementation

§15, "Levers, by which resource they move":

> The auditor is a fixed cost of one session per conformance campaign, and it
> is **not a knob**: it is the only number that loop has.

The deployment has two departures from that, and they are not the same kind
of thing.

**`audit_every` exists.** `state/phase.toml` carries it, `hj audit-due`
reads it, and `harness/loop` runs the auditor only when it returns 0. It is
set to `1` today, so for a loop that runs one campaign at a time the
behaviour matches §5 exactly. What contradicts §15 is that the number is
settable at all — a knob set to 1 is still a knob, and the next person who
wants a cheaper run has an obvious dial with no argument attached to it.

**Workers do not audit, and the round does.** `harness/loop` skips the
auditor for any campaign started with `--worker`; `harness/workers` runs one
audit per round, in the integration checkout, after all three have merged.
So `core`'s real cadence is one audit per three campaigns. This one is not an
oversight — §13's "The audit does not parallelise" argues it directly, and
the code quotes the argument: three workers auditing their own branches would
each judge a tree nobody ships, and would write three verdicts for one
section with no rule for which wins.

So §5 and §13 disagree, in a document whose §5 was written before workers
existed. The measured cost, for whoever weighs it: an audit is $6.52 against
a $20 harness campaign, and for `core`, $3.84 average against $9.22 — call it
40% on top of every campaign if the cadence goes to one-per-campaign
everywhere.

The reason this is not simply a Class A reconciliation is that "audit every
campaign" is what makes the number mean something per campaign. §7's stall
rule already carries the scar: `judged_campaigns` excludes a campaign that
closed with no audit since it opened, because otherwise the audit cadence
could stop the loop — with `audit_every` equal to `stall_n`, two unmeasured
campaigns and one flat one is a stall that never happened. That workaround
exists because the cadence is not one-per-campaign. Making the cadence match
§5 would let it be deleted; keeping the cadence means keeping it.

## Options

**A. The claim is right and the knob is the deviation.** Delete
`audit_every`, audit at every campaign close, and give workers a rule that
does not produce three verdicts — the plausible one being that the round
runner audits once per *worker* campaign, serially, after merge. Cost: the
audit bill rises by roughly 40% of the campaign bill for `core`, and a
round's audits serialise behind its merges, which lengthens the round by
three auditor sessions rather than one. Buys: every campaign is measured,
and the `judged_campaigns` exclusion can go.

**B. The knob is right and §5 is stale.** §5 becomes "at every round close",
with a round being one campaign for a loop with one worker, and §15's "not a
knob" becomes "not a knob a loop may turn" — which is already true, since
`state/phase.toml` is denied to every loop. Cost: a campaign can close
against a verdict up to two rounds old, and the progress it reports is then
attributed to whichever campaign happens to close after the audit that
measures it. That misattribution is invisible in the metrics.

**C. Split the difference: keep the knob, bound it.** `audit_every` may not
exceed 1 for a loop whose `stall_n` it would interact with, enforced in `hj`
rather than in prose, and workers keep the round audit. Cost: it settles the
`core` case by fiat without saying which of A or B is true, and the bound is
the kind of rule that is right for one phase and wrong for the next.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**B, by inertia**: the code is unchanged and `audit_every` stays at 1, which
is the most reversible position — nothing is deleted, and A remains available
at the cost of one commit whenever the answer arrives. No site is tagged,
because the choice lives in `state/phase.toml`, which is denied to every
loop, and in prose I have deliberately not edited.

**Why the spec was not edited to match, which is the part worth reading.**
Under either answer this ends in a `design/loops.md` edit — B's is three
sentences. The campaign that raised it is the one that rewrote `spec_drift`
so that a campaign editing `design/` and `harness/` in the same run is
flagged, which is §19's thinnest defence and §7's only unbacked
countermeasure. Making the first flagged campaign the one that fixed the
detector, in the same run, is exactly the shape a reviewer would have to stop
and check. It is three sentences either way; it can be three sentences in a
campaign that is not holding the detector.

## Consequences

If A: `hj audit-due` and the `audit_every` key go, `harness/workers` audits
per campaign rather than per round, and `judged_campaigns`'s unaudited
exclusion can be removed — which is the one piece of scar tissue that exists
only because of the cadence. Roughly 40% more spend per campaign on `core`.

If B: §5 and §15 need the edit, and the misattribution stays. Worth adding
`audited` to what a campaign is told about itself, so a campaign that closes
unmeasured knows its own number did not move for reasons that have nothing to
do with it.

If C: `hj` grows a bound nobody can state a phase-independent reason for,
which is the option most likely to be re-raised.
