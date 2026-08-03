---
id: conformance-007
status: accepted
opened: 2026-08-03T06:07:00+00:00
campaign: f08773ec-615a-4226-be82-7968f4ab0db9
kind: harness-request
---

# Should `hj record` key its row on the loop's own commit rather than on `HEAD`?

## Context

`harness/hj record` builds its row with `"commit": head_sha()` and
`"campaign": trailers_of("HEAD")`, and `hj check-metrics` then looks for a row
whose commit is *the loop's last trailered commit*, walking back past the
harness's own untrailered ones. The two disagree the moment anything commits
between a loop's `git commit` and its `harness/hj record`.

That happened in this campaign. The sequence was:

```
9e4a777 [core-5] close campaign f08773ec        <- the loop's commit
a7e7f8b harness: cost rows for 5 campaign(s)    <- arrived in between
88f8075 harness: metrics row for a7e7f8b9       <- what `record` wrote
```

The row that `record` appended names `a7e7f8b`, which is not a loop commit,
and carries `"campaign": null`, because that commit has no `campaign:`
trailer. `9e4a777` has no row at all and can never get one: `record` has no
`--rev`, so there is no way to ask for a row against a commit that is no
longer `HEAD`. The gate then fails at step 7 for a commit that did everything
it was supposed to, and the only ways out are to make another commit or to
leave the gate red — neither of which is what step 7 is trying to enforce.

The journal already records the same class of interference from the other
direction (`dc1c9639`: another writer's files appearing in the working tree,
so `git add -A` would produce an out-of-scope commit). This is that hazard on
the metrics path rather than the diff-scope path, and unlike that one it
cannot be avoided by being careful — there is no way to make `git commit` and
`hj record` atomic from the loop's side.

## Options

* **`record` keys on the loop's last trailered commit**, using the walk-back
  `check-metrics` already implements, instead of on `HEAD`. Costs nothing:
  the two would agree by construction, and a `campaign` field read from that
  same commit stops being `null`. The risk is that `record` run twice in a
  row would try to write a second row for the same commit, so it needs to be
  idempotent — which it should be anyway.
* **`record` grows a `--rev`**, and the loop passes the sha it just made.
  More explicit, and it leaves `HEAD` as the default for the common case. It
  costs a step in the prompt's commit ritual, which is the part a campaign
  under budget pressure is most likely to get wrong.
* **Do nothing; loops make another commit when they lose the race.** What
  this campaign did. It leaves a permanent hole in the metrics series at
  every commit that lost, and it means the number the dashboard reads is
  keyed to harness commits — `"campaign": null` rows are already in
  `state/metrics/conformance.jsonl`.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

The third option, because it is the only one a loop can take: `harness/` is
denied to every loop, so neither fix is mine to apply. This record is
committed as a loop commit and recorded immediately after, which restores the
gate by giving `check-metrics` a last-loop-commit that has a row. No source
site is tagged: there is nothing in `crates/` this affects, and the affected
file is one I may not edit.

The hole stands at `9e4a777`, and the stray row naming `a7e7f8b` with a null
campaign stands in `state/metrics/conformance.jsonl`. Both are visible rather
than repaired, since repairing them by hand is the other thing a loop must
not do to its own instrument.

## Consequences

If the answer is either fix, the two rows above are the ones to reconcile,
and no campaign work has to be redone. If the answer is "do nothing", then
`campaign: null` rows are an expected part of the series and whatever reads
it should filter them, which is worth saying out loud in `design/loops.md`
§15 — cost accounting joins `ccusage` output to commits on the session id,
and a row keyed to a harness commit has no session to join to.

## Answer — 2026-08-03T06:11:31+00:00

**Ruling:** accepted

Option 1, applied: record now keys on last_loop_commit, the same walk-back check-metrics uses, and is idempotent. The two stray campaign=null rows and the missing row for 9e4a777 are repaired separately once the loop quiesces — the series is a cache and section 10 says any row can be recomputed from its commit, so a human may repair it even though a loop must not.

**Rationale:** The report is right and the diagnosis is exact. It also understates the frequency: there are two campaign=null rows in the series, not one. And the specific incident was mine — a7e7f8b is a manual hj cost run I made at 00:05:02, four seconds after the loop committed at 00:04:58, which is logged separately. Option 1 over 2 because it needs nothing from the campaign, and the ritual step option 2 adds is the part a campaign under pressure drops first. Option 1 over 3 because a permanent hole in a series the dashboard reads is not something to design in. The deciding point is one the record could not know: answering a decision in the dashboard also commits and also moves HEAD, so this race is reachable by a human doing exactly what the dashboard is for. That makes operational discipline no defence and the structural fix the only one.

Reconciling the sites tagged `// DECISION-conformance-007: provisional` is a
normal campaign target, not an interrupt.
