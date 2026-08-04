# Findings — harness loop

Theory after four campaigns. Rewritten, not appended.

## Falsified — act on these directly, they cost campaigns already

* **"Judge whether the code looks right" finds nothing.** It looks right.
  Four campaigns, one method that works: **take the section's literal nouns
  and grep for each.** `tokei`, `:!harness`, `master`, `dispatch` were all
  found in under a minute that way.
* **A docstring describing behaviour the function does not have is where the
  bug is.** Third campaign running. `tagged_sites` claimed
  "Same scope as `provisional_decisions`" and implemented a different one, so
  five answered decisions sat in loop prompts pointing at a file with no tag
  in it. If two functions are documented as sharing a scope, make them share
  the *function*.
* **A glob over `state/findings/` matches `core-1.md`..`core-3.md`.** Anything
  meaning "per loop" must read `phase.loops`. Same trap in `state/journal/`.
* **A computed prompt value can be spliced nowhere and nothing complains.**
  `{{other_findings}}` was dead for all of phase 1a. Check
  `grep -o "{{[a-z_]*}}"` on the templates against `prompt_values`; the
  reverse direction fails loudly, this one is silent.
* **Do not rename a section heading.** The anchor is in
  `harness/section-baseline.toml`, denied to every loop, so a rename is
  permanent drift and a `sections_missing` no campaign can clear.
  `#branches-exist-for-one-commit-at-a-time` now contradicts its own body for
  exactly this reason; it is a job for whoever retakes the baseline.
* **These audit gaps are stale — verified fixed in the tree:** §11's
  `tokei`, §7's decision-resolution-scores-nothing, §7's test-deletions.
  §4, §5, §6, §7, §11, §15, §16, §17 and §19 are otherwise done.

## Confirmed — candidates, test them, do not adopt on my evidence

* **What is left is boundary crossings, not wrong words.** The nouns method
  found every gap this campaign but one. That one — `harness-007`, phase 1a
  containing `actor.rs`, `Mode::Standalone` and `Divergence` — needed reading
  the tree *against* the list. Expect the rest of `loops.md` to be that shape:
  §11's binary size, §12's held-out evaluation, §17's ccusage.
* **The cheap close is usually the dishonest one.** `harness-007` could have
  been closed by deleting two words from §8. The gap is deliberately left
  open, with no provisional choice taken, because all three options were less
  reversible than doing nothing loudly. Do not close it by editing §8 first.
* **Next escalation, already scoped:** `decisions_reconciled` reads tag
  *removals*, so reconciling a decision whose choice lived in a file the loop
  may not write scores no progress. Counting a `decision:` trailer that names
  an already-*answered* record fixes it. It is a metric redefinition, so
  Class B — and doing it in the campaign that benefits is the wrong shape.

## Load-bearing

* **The pinned harness judges you.** Run `harness/hj selftest` after every
  edit to `hj`; the gate now runs the tree's copy too (57 → 68 checks).
* **A syntax error in `hj` blocks every Edit and Write in the worktree.** The
  hook reads a crash as a denial. The escape is bash.
* **Never add a per-campaign write to a shared single-valued file.** Workers
  conflict on rebase and the round stops. Append-only jsonl.
* **`gate_runs` counts any command mentioning `harness/gate`**, so grepping
  for it inflates your own `empty` count.
