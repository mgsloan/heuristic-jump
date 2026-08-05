# Findings — harness loop

Theory after seven campaigns. Rewritten, not appended.

## Falsified — act on these directly

* **Check `state/decisions/` before ranking gaps.** Two targets this campaign
  were records already `accepted` whose ruling nobody had applied: a gap reads
  as unsolved when the answer is written down.
* **An audit's proposed fix can be an option a decision already refused** —
  `[bfb721b74c]`'s was `harness-009` option one.
* **"Judge whether the code looks right" finds nothing.** Grep the section's
  literal nouns — `tokei`, `merge-blocked`, `ccusage` fell in a minute.
* **A docstring describing behaviour the function lacks is where the bug is.**
  Fifth campaign running.
* **The audit lags ~1.5 campaigns** — six of ten listed gaps were already
  closed — but its *one-line* list is still the best target-picker there is.
* **A closed gap's section cannot be derived from its id** (`sha256`); record
  it when it closes. **Do not rename a section heading** — the anchor is in
  the denied baseline.
* **A check on a file more than one loop writes belongs in `check-metrics`
  scoped to `config.writes_harness`, never `selftest`.** `selftest` runs for
  every loop, from the pinned `hj`, against `HARNESS` — which resolves through
  `HJ_REPO` to *that* loop's worktree, so it fails in the tree of whoever has
  not merged. `harness-011`, third campaign. Fixtures for the logic.

## Confirmed — candidates, test on your own evidence

* **What is left is deferred mechanisms, not wrong words.** Seven sections.
* **An instructed number that nothing computes drifts.** The 512-word digest
  cap went unmeasured since campaign one; three of five were over. Find others.
* **Where the code and `loops.md` disagreed it favoured this loop** —
  `harness-008`, `harness-009`, both now answered *for* the code. That call is
  a human's: edit the document only under a ruling, and say so in the
  changelog.
* **Never reconcile a decision still `open` in your tree** — that is ruling on
  your own escalation.
* **Backtest a metric change** (`hj progress-replay --rule N`) before it.
* **`decisions_reconciled` still reads tag *removals***, so reconciling a
  decision tagged in a file the loop may not write scores nothing. Class B.

## Do not spend a campaign on these

* §12 held-out, §11 tokei and binary size, §5, §7, `#branches-…`: implemented.
* `#sessions-assign-the-id-own-the-transcript`, `#reading-a-transcript`: the
  adapter assigns `--session-id` and tees `stream-json`, and the dashboard
  renders transcripts. Expect clean verdicts.
* §17's two gaps are what genuinely remains, neither cheap. `ccusage` is
  **not installed here**, so adopting it means unrunnable parsing code — the
  `cargo bloat` dead end. EARS is a notation change to the denied auditor
  prompt.
* `#rules-are-inlined…`'s "Both go in the volatile tail" is ambiguous and the
  code is arguably right. Let the auditor judge it.

## Load-bearing

* `hj selftest` after every `hj` edit (104 → 115 here). A syntax error there
  blocks Edit/Write — use bash or python.
* **`campaign-open`'s stdout is the campaign id**; `harness/loop` captures it.
* `prompt-prefix` percentages are not comparable across campaigns (the tail
  grows); use absolute offsets.
* Never add a per-campaign write to a shared single-valued file.
