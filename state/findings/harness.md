# Findings — harness loop

Theory after two campaigns. Rewritten, not appended.

## Where the gaps are, and how to find them

`loops.md` describes machinery that mostly exists. The gaps are **named
fields and named ratios the implementation approximated**, and they hide
behind components that look right. Section 15 named `experiments` and `gate
seconds` and had neither. Section 16 named six fields for
`interventions.jsonl`, a vocabulary of `kind`s, twelve session-row fields,
three derived numbers and four renderer properties — seven were missing while
every panel rendered.

**The method: read the section's nouns and grep for each one.** Judging
whether the code has the right shape finds nothing. The sharpest tell is a
sentence of the form *"nothing else in the design reports it"* — both
campaigns found the prose quoted in a comment and the arithmetic absent.

Second concentration: **claims true of the design and untrue of the
deployment.** §4's gate steps were "all mandatory" while three printed
`skipped`. §13 names four isolation layers and three exist. Neither is
visible from inside the document.

## Ruled out

* **Sections 15 and 16 are done.** All six of 15's subsections and all seven
  of 16's anchors are implemented and, I believe, clean. Do not re-read
  `dashboard/serve` looking for missing panels.
* **`harness/loop`, `campaign-open/close`, `reap`, `audit-due` and the stall
  rule carry scar tissue.** Their comments record real misrecordings already
  fixed. Treat them as correct.
* **Do not rewrite §15's estimate table**, even though the section invites
  it. Journal, campaign 11b9c019.
* **Do not build a gate check that parses the design's ownership table.**
  Tempting and wrong; journal, this campaign.

## Load-bearing, confirmed

* **The pinned gate checks the pinned harness.** It runs `$here/hj`, not the
  tree's — so the loop that edits `harness/` was the one whose changes the
  gate never executed. Half-fixed (the tree's `hj selftest` now runs too when
  it differs); the habit still stands: run everything you change yourself, in
  every worktree.
* **This worktree runs ~20 commits behind `main`.** `state/` here is not what
  the dashboard shows. `harness-001` reads open here and is answered on
  `main`. Check `main` before concluding anything about state.
* **`gate_runs` counts any command mentioning `harness/gate`**, so grepping
  for it inflates your own `empty` count. Changing that definition is a
  metric redefinition, Class B, needs a sweep.

## Open, and yours

* **`harness-001` is answered `A` on `main`** — the campaign id moves out of
  the prompt body. The journal from 11b9c019 says the work is four edits and
  an intervention entry, and **do not redo the measurement**. It is a
  separate campaign: different files (`harness/prompts/`), different section.
* **`harness-002`** escalates §13's missing OS sandbox. Do not enable it
  without reading the record — as specified it breaks `hj record`.

## Where to go next

Section 13's `#workers-*` and `#parallel-loops-*` subsections — the newest,
least exercised machinery, and `harness/workers` is still unread by any
campaign — then sections 3 and 5, the audit ledger and the denominator, which
compute this loop's own number and deserve the same literal-noun treatment.
