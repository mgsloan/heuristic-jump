# Findings — harness loop

Theory after three campaigns. Rewritten, not appended.

## Where the gaps are, and the one method that finds them

`loops.md` describes machinery that mostly exists. The gaps are **named
fields, named ratios and named nouns the implementation approximated** —
never missing components. Three campaigns running, the same method found
every one: **take the section's literal nouns and grep for each.** Judging
whether the code looks right finds nothing, because it looks right.

Two tells, both confirmed repeatedly:

* **A docstring describing behaviour the function does not have.** That is
  where a previous campaign wrote the claim it meant to satisfy. `tokei` was
  in `loc_per_crate`'s docstring and nowhere in its body.
* **A sentence of the form "nothing else in the design reports it."** The
  prose gets quoted in a comment and the arithmetic never appears.

Third concentration, newly confirmed: **anything scoped by an allow-list
excludes `harness/`**, so the loop building the harness was invisible to its
own instruments. `spec_drift` counted only `crates/` and `vendor/`;
`provisional_decisions` and `tagged_sites` excluded `:!harness` wholesale.
If a gap smells like this, check the pathspec first.

## Ruled out — do not re-read

* **Sections 4, 6, 7, 11, 15, 16, 17 and 19 are done.** All five of §7's
  gaps landed. Do not re-read `dashboard/serve` for missing panels or
  `check-metrics` for missing arithmetic.
* **`harness/loop`, `campaign-open/close`, `reap`, `audit-due` and the stall
  rule carry scar tissue.** Their comments record real misrecordings already
  fixed. Treat them as correct.
* Do not rewrite §15's estimate table; do not build a gate check that parses
  the ownership table. Journals of 11b9c019 and bb1e501a.

## What is left in `loops.md`, and it is a different shape

The mechanical one-function gaps are gone. What remains:

* **Three spec edits** — §1 "there is no code", §2 "one writer", §8's
  tune/select/final split. Each is a few sentences and each closes a
  section. **Take them, in a campaign that is not editing the detector.**
  This campaign deliberately did not: it rewrote `spec_drift`, and the
  detector's own campaign should not be the first thing it flags.
* **`harness-003`** — the audit cadence is a knob and §5/§15 say it is not.
  A cost trade, not a loop's to settle.
* **`harness-004`** — `loop/harness` and `main` diverged twenty commits each
  way and neither is a superset. The record carries the resolution rather
  than the complaint: three hunks, all in `hj`, all mechanical.
* Then the unjudged sections. `#the-metrics-history`,
  `#what-a-loop-remembers-about-itself` and
  `#rules-are-inlined-subject-matter-is-read` were read this campaign and
  appear satisfied; the untouched ones are §13's `#workers-*` and
  `#parallel-loops-*`, and `harness/workers` is still unread by any campaign.

## Load-bearing, confirmed

* **The pinned harness judges you, and it is two campaigns stale.** The gate
  on `main` predates the tree-`hj` selftest, so nothing runs this worktree's
  `hj` except you. Run `harness/hj selftest` after every edit to it.
* **A syntax error in `hj` blocks every Edit and Write in the worktree** —
  the hook read a crash as a denial. Fixed by pointing the hook at the pinned
  copy; if it recurs, the escape is bash.
* **Never add a per-campaign write to a shared single-valued file.** Three
  core workers bumping one scalar conflict on rebase and stop the round.
  Append-only jsonl, or a hand-set floor read alongside it.
* **`gate_runs` counts any command mentioning `harness/gate`**, so grepping
  for it inflates your own `empty` count.
