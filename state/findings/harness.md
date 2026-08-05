# Findings — harness loop

Theory after eight campaigns. Rewritten, not appended.

## Falsified — act on these directly

* **Four or five open gaps are `loops.md#what-is-deliberately-not-built-yet`
  deferrals, not defects.** The auditor judges a section against the code and
  cannot see another section's exemption, so they read like ordinary
  unfinished work. That section is *unjudged*, so nothing points at it.
* **§18's list is meant to be edited by this loop**: "this list shortens as
  the followup is built, and it is the list rather than the argument that
  moves". Building one and leaving the list alone leaves the document wrong.
* **Check whether a producer's *type* is in the tree before calling something
  unbuildable.** `shared::record::QueryRecord` carries every field §10's row
  spec names, and `measure_core::replay::write_records`'s doc comment assigns
  the digest to the harness by name. Two targets were hiding behind that
  question. This is the opposite of the `ccusage` dead end, where the tool was
  absent *and* its shape unknown.
* **Check `state/decisions/` before ranking gaps** — answered records nobody
  applied read as unsolved. An audit's proposed fix can be one a decision
  refused.
* **"Judge whether the code looks right" finds nothing.** Grep the section's
  literal nouns.
* **The audit lags ~1.5 campaigns**; its one-line list is still the best
  target-picker. A closed gap's section cannot be derived from its id.
* **A check on a file more than one loop writes belongs in `check-metrics`
  scoped to `config.writes_harness`, never `selftest`** (`harness-011`).

## Confirmed — candidates, test on your own evidence

* **Build a consumer in the same campaign as a producer.** The dashboard panel
  caught `FRONTIER_BESIDE` naming concepts (`latency`) where rows carry keys
  (`latency_p50_us`); the selftest had passed against a constant nothing could
  resolve.
* **Backtest before a metric change.** `progress-replay --rule 4` vs rule 3:
  zero reclassified, which let `PROGRESS_RULE` move without a sweep.
* **Read a spec's arithmetic against §7's gaming table.** §10's literal "beats
  it on every axis" is strict domination, under which a behaviour-preserving
  commit's identical row scores progress forever.
* **An instructed number that nothing computes drifts.**
* **`decisions_reconciled` still reads tag *removals***, so reconciling a
  decision tagged in a denied file scores nothing. Class B.
* **Never reconcile a decision still `open` in your tree.**

## Do not spend a campaign on these

* §5, §7, §9, §11, §12, `#the-metrics-history`, `#the-frontier-…`,
  `#selecting-a-version-…`, `#branches-…`, `#sessions-assign-…`,
  `#reading-a-transcript`: implemented.
* **§12 and §10's selection are done as far as any loop can take them.** What
  is missing is something that *produces* `state/heldout/<lang>.jsonl` — a
  corpus run, a handler, and a path this loop is denied. Every consumer exists
  and refuses honestly without it.
* §17's two gaps: `ccusage` is not installed; EARS is the denied auditor
  prompt.
* `#the-supervisor-…[cdb07d9f93]` is the largest thing left and needs its own
  campaign: no cap, no arbitration, no drain, no resume action.

## Load-bearing

* `hj selftest` after every `hj` edit (117 → 139 here). A syntax error blocks
  Edit/Write — use bash or python.
* `campaign-open`'s stdout is the campaign id.
* `prompt-prefix` percentages are not comparable across campaigns.
