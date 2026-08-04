# Findings — harness loop

Current theory after one campaign. Rewritten, not appended.

## Where the gaps are

`loops.md` describes machinery that mostly exists. The gaps are not missing
subsystems; they are **named fields and named ratios that the implementation
approximated**. Section 15's row spec listed `experiments` and `gate seconds`
and had neither; it named three budget scopes and had one; it named tokens
per *gap closed* and the dashboard showed tokens per *campaign that
progressed*. All three were computable from data already recorded. Expect
more of this shape: read the section's nouns literally and check each one
against `state/`, rather than judging whether the code has the right shape.

The second concentration is **claims that are true of the design and untrue
of the deployment.** Section 4 says the gate's steps are "all mandatory";
for a loop owning no crate, three of seven printed `skipped` and nothing
executed the code being changed. That gap was invisible from inside the
document.

## Ruled out

* **The dashboard is not where the gaps are.** All eleven panels render, and
  it implements the POST-answers-decisions and rendered-transcript claims
  section 16 calls the highest-leverage parts.
* **`harness/loop`, `campaign-open/close`, `reap`, `audit-due` and the stall
  rule carry scar tissue** — their comments record real misrecordings already
  fixed. Treat them as correct.
* **Section 15's estimate table should not be rewritten by this loop**, even
  though the section asks for it. See the journal.

## Load-bearing, confirmed

* **The pinned-gate split is real and it is the thing to be careful about.**
  `hj` runs from the reviewed harness with `HJ_REPO` pointing at another
  tree. Any harness file resolved through `REPO` resolves to the *checked*
  tree's copy. This bit once already; it passes here and fails everywhere
  else, hours later, in someone else's session. Resolve harness files from
  `__file__`, and run `hj selftest` against every worktree before merging.
* **Cache reads are ~141:1 against output.** Prompt-prefix stability is the
  dominant cost lever and it is currently poor (11.9% on core). `harness-001`
  escalates it; the numbers are in the record.
* **`state/` files are append-only and merged by id on read.** Adding a
  writer without checking the reader merges means silent double-counting.

## What the next campaign should not waste time on

* Re-reading `dashboard/serve` looking for missing panels.
* Section 15 — all six subsections plus `#15-cost-and-timing` are now
  implemented and, I believe, clean. Start elsewhere.
* Answering `harness-001` itself. It is a human's call and it is filed.
* Building a supervisor. Section 18 defers it, and one bash loop with
  workers is what exists.

## Where to go next

Untouched clusters, in the order I would take them: section 16's six
operator-view sections (dashboard, one file, already known good — likely
several clean verdicts for cheap); section 13's isolation and workers
sections (`harness/loop`, `check-scope`, worktrees — all observable right
now, and the workers machinery is the newest and least exercised); then
sections 3 and 5, the audit ledger and the denominator, which compute this
loop's own number and deserve the same literal-noun treatment section 15
got.
