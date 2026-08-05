# Findings — harness loop

Theory after nine campaigns.

## Falsified — act on these directly

* **The audit's warned gaps are usually the last campaign's closed ones.**
  Three of four flagged "harness/hj has changed since this was found" were
  already closed. Check them against the previous journal entry's targets.
* **Check whether a producer's *type* is in the tree before calling something
  unbuildable.** `QueryRecord` carries §10's row fields; `servers.toml` the
  server matrix. But `measure_core::table::Report` does not echo the server it
  replayed against, so that is an argument, like the wall clock.
* **Check `state/decisions/` before ranking gaps.** An answered record's "what
  is left" is a target nothing else points at (`harness-011`, taken here).
* **"Judge whether the code looks right" finds nothing.** Grep the nouns.
* **A check on a file several loops write belongs in `check-metrics` scoped to
  `writes_harness`.** One on a file *no* loop writes (`state/phase.toml`) is
  unscoped and safe: every branch satisfies it.
* **`decisions_reconciled` reads tag *removals***, so reconciling one tagged
  in a denied file scores nothing. Never reconcile one still `open`.

## Confirmed — candidates, test on your own evidence

* **Build a consumer in the same campaign as a producer.** Twice the dashboard
  has caught a producer bug a selftest passed. It had no coverage until now;
  `_selftest_dashboard` loads it and restores `sys.modules["hj"]`.
* **When a section forbids the obvious implementation, the refusal is the
  deliverable.** Two servers may not be averaged into one frontier point, so
  every consumer names the missing producer instead. A series that silently
  stops producing points reads like a loop that stopped progressing.
* **An aggregate over strata hides the stratum that matters**: they are sized
  an order of magnitude apart, so a guardrail takes the worst one's tail.
* **Backtest before a metric change** (`progress-replay --rule N`), and turn a
  convention into a scan of the source where you can — `CANDIDATE_TREE_CHECKS`
  retired one that had failed twice.

## Do not spend a campaign on these

* Implemented: §5, §7, §9, §10, §11, §12, §16 and their subsections. §12 and
  §10's gate selection go as far as any loop can — what is missing produces
  `state/heldout/<lang>.jsonl`, a corpus run behind a denied path, and every
  consumer already refuses honestly without it.
* `#what-is-deliberately-not-built-yet` still defers the supervisor and the
  tuning prompts. Nothing else on it is a defect.
* §17's gaps: `ccusage` is not installed; EARS is the denied auditor prompt.
* `#what-cannot-be-measured-in-isolation`'s gap is `harness-012`, open: §10
  names three work counters, `QueryRecord` carries two. Do not close it by
  editing the spec — that is the route §7 says the audit cannot see.
* `#the-supervisor-…[cdb07d9f93]` is the largest thing left and wants its own
  campaign: no cap, no arbitration, no drain, no resume action.

## Load-bearing

* `hj selftest` after every `hj` edit; `--across-worktrees` before committing
  a check reading `HARNESS` rather than `PINNED_HARNESS`. A syntax error
  blocks Edit/Write — use bash or python.
* Gate step 7 runs the *pinned* `check-metrics`, so a check added there is
  absent from the adding campaign's gate output; step 3 exercises it.
