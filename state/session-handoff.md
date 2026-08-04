# Session handoff — 2026-08-04

Written at the end of a long session for whoever picks this up. It is not loop
state; no loop writes or reads it. `state/handoff.md` is a different thing —
that one means a loop stalled and asked a question.

## Where things stand

**Both loops are running again** as of 2026-08-04T19:41Z — `core` at three
workers in rounds (`harness/workers core`), `harness` at one
(`harness/loop harness`). `main` was green when they started (271 tests, all 7
gate steps for both loops, `hj selftest` 57 checks) and **every branch was
merged**; all four worktrees were fast-forwarded to `main` first, which cost
nothing because each was already an ancestor.

To stop them: set `status = "paused"` in `state/phase.toml` and they finish the
round they are in. That is the lever, not a kill.

| | clean | gaps | campaigns |
|---|---|---|---|
| `core` (core.md, deps.md, rope-modifications.md) | **60/71** | 12 open of 96 found | 39 |
| `harness` (loops.md) | 13/75 | 16 open | 3 |

**Nothing is waiting on a human**, which is what made the restart the right
call: 30 decisions all answered, 34 spec changes all read, 20 drift flags all
reviewed. Two provisional `DECISION-` tags remain and both are ordinary
campaign reconciliation, not questions —
`crates/driver/src/actor.rs:579` (`core-017`) and
`crates/measure_core/src/measure_core.rs:173` (`core-002`).

### Two status lines were lying, and both are fixed (`0742dd5`)

Worth knowing because the *shape* of the bug recurs: a line that counts a
standing total while its wording promises a queue.

* `provisional DECISION- tags` read **6** when the truth was **2**. Widening
  `DECISION_TAG_NOT_A_SITE` from `:!harness` was right — excluding all of
  `harness/` makes the count structurally zero for the loop whose
  implementation *is* `harness/` — but it reached one file too far and started
  counting `hj`'s own selftest fixtures, which are diffs carrying a tag because
  what they test is the reader of that tag. Now excluded by region rather than
  by path, read off the source, failing closed.
* `campaigns that edited design/ and code together: 20 — read them` counted
  every campaign ever flagged, so reading never emptied it. It now subtracts
  the reviewed ones — and immediately found one genuinely unread
  (`44773a93`, arrived on `loop/core-1` after the last review batch, since
  reviewed).

### `harness-006` is answered: stage explicit paths

`git add -A` fails under the sandbox because the masked dotfiles are
`/dev/null` bind mounts. The ruling is the prompt sentence, not a gitignore —
ignoring `.gitmodules` and `.claude/skills/` would make *that* failure quiet
where this one is loud. It lives in `harness/trailer-format.md`, which is
spliced into every loop prompt as `{{trailer_format}}`, so one edit reaches
both loops and every worker.

## What the merges cost, and what they say

The three branches the previous handoff left — `loop/core-1` (41 commits),
`loop/core-2` (31) and `loop/harness` (58, which it did not mention) — are
merged, in that order, each as a merge commit with its resolution reasoning in
the message (`ed91c10`, `77c3c72`, `abf2099`). Read those before re-deriving
any of it.

The rule that mattered is the one the previous attempt broke: **take one side
whole**. It held everywhere except two places, and both are worth knowing:

* A **dict literal** and a **corpus file** are not things there can be two
  versions of. Two campaigns adding different keys, or different captured
  messages, at one insertion point is an append both sides made — union is the
  resolution and picking a side loses work.
* `measure_core/tests/pipeline.rs` **was not a conflict at all.** Both sides
  added tests whose bodies share a prefix, and the diff aligned one against
  the other; taking either side would have dropped three real tests. It was
  rebuilt from the three merge stages (`git show :1: :2: :3:`) by diffing base
  against each side and applying the additions separately. Histogram alignment
  does not help — it produces the same hunks. If a conflict region starts or
  ends mid-function, stop and do this instead.

### `crates/driver/tests/seam.rs` is still the contended file

2,300 lines, 71 functions, and it conflicted in **both** `core` merges again,
exactly as predicted. Nothing about that has changed: splitting it by topic —
manifests, licensing, lints, layout, vocabulary — is still the highest-value
single target available, and still a campaign's work rather than a paragraph.

### Four id collisions, and one of them was silent

Two campaigns took `core-018` for different questions; five `CHANGE-core-*`
numbers were used twice; and **one duplicate merged cleanly and passed all
seven gate steps**, because the two entries sat in different parts of the
changelog. `grep -o '^## CHANGE-core-[0-9]*' | sort | uniq -d` found it and
nothing else would have. Raised as `harness-005`; until it is answered, a merge
of any long-diverged branch needs that check run by hand.

Renumbering follows `allocate-id`'s rule — one past the highest, never the
lowest free — and the side that moves is the one **not** already cited from
code or from `state/interventions.jsonl`, which cannot be rewritten.

### One thing the merge settled

`core-018` was answered "(a) the corpus is judged on its server half now, (b)
capture the client half when convenient". `loop/core-1` did (b): the corpus
holds Eglot-through-a-recording-proxy `didOpen`/`didChange`/`didSave`/
`didClose` and an editor's own `initialize`, and `differential.rs` now requires
a captured message of all eight kinds. The `DECISION-core-018: provisional`
tags are gone with it. The deferred item in the old handoff — two minutes at a
desktop with `harness/capture-editor-traffic` — **is no longer needed**.
`core-020` is what is left of it: whether the headless Emacs driver, which
needs no human, joins that tool in `harness/`.


## The one lesson that generalises

Four separate bugs this session had one cause:

> **State the fleet coordinates through cannot live on a branch.**

A campaign's *output* — its record, journal, commits — belongs on its branch and
merges. Anything another worker, the dashboard, or the stall detector must read
*before* that merge does not. The four that had to move to the integration
checkout (`CONTROL`, set by `HJ_CONTROL`):

* `state/phase.toml` — pausing `main` did not stop workers reading their own copy
* `state/sessions.jsonl` — three workers ran and the dashboard showed nothing
* `state/assignments/` — two of three workers never got the plan and did each
  other's work
* `state/claims/`, `state/reserved/` — untracked, shared, ephemeral

If something new needs to be visible across workers, put it under `CONTROL` and
gitignore it if it is ephemeral. The test is *"does someone else need this
before the merge?"* — not *"is it in `state/`?"*

## What changed this session

Roughly in order. All committed, all with reasoning in the commit messages.

**Parallel workers.** `core` runs three, each in its own worktree on its own
branch (`harness/workers core`). A round is: plan → three campaigns → barrier →
merge → audit. `design/loops.md` §13 was rewritten to match (the section
previously described claim-at-open, which did not survive contact).

**A planner divides each round.** A short read-only session reads the gap list
*and the code* and writes one assignment per worker, grouped so each worker's
targets share their reading. `harness/prompts/planner.md`. An empty assignment
is a supported answer and the prompt says so.

**`hj claim`** lets a campaign take a target outside its assignment, refused if
another live campaign holds it. `O_EXCL` per target; a claim younger than
`REAP_GRACE_SECONDS` is held regardless of process liveness (same race the
reaper has).

**The audit does not parallelise.** One per round, after the barrier, in the
integration checkout — the only tree with all three workers' commits.
`audit_every = 1`, `rotation = 18`.

**Progress is settled at the audit, not at close** (`PROGRESS_RULE = 2`).
Under workers a campaign closes before the instrument runs, so `sections_clean`
was frozen and only `tests up` could fire — the loop had no stopping condition
but money. Also: attribution by named gap rather than count delta, and
**consolidation credit** so removing code can score at all.

**`hj progress-replay --rule N`** backtests a progress rule against every closed
campaign before it decides anything live. Rule 1 reproduces history exactly;
that is the gate for trusting any later rule.

**`hj allocate-id`** reserves decision and changelog numbers, because two
campaigns took `core-001` and two took `CHANGE-core-005`.

**Gaps carry `found_at`** and the prompt flags any whose `where:` file has moved
since — seven of nine `core.md` gaps were stale and being handed out as targets.

**The harness loop** (`writes_harness = true`) may write `harness/`, and is
judged by the reviewed copy on the integration branch rather than its own
(`HJ_PINNED_HARNESS`). Read the comment on `HARNESS_PATHS` in `harness/hj`
before touching either half — they are only safe together.

**Dashboard**: one page with a loop selector, a live row per running campaign,
an audit table, campaign diffs, and a tracked-lines chart.

## Traps that will bite again

* **Do not edit `harness/workers` or `harness/loop` while they are running.**
  Bash parses a loop body once; a running fleet keeps executing the old version.
  Round 1's audit silently never ran because of this. `hj` is re-exec'd per
  invocation and is safe to edit.
* **`pgrep -f` matches your own shell.** It has produced a false "still running"
  at least four times. Match on `/proc/<pid>/cmdline` argv instead.
* **`--ours`/`--theirs` invert between merge and rebase.** In a rebase, `--ours`
  is `main`. In a merge, `--ours` is the branch. `harness/loop` has separate
  functions for this rather than one with a flag.
* **A campaign's gate is scoped to its loop.** Two branches can each be green
  and be red together — that happened, with one campaign adding a `seam.rs`
  assertion and another adding the code it forbids.

## Deferred, with reasons

* **Stage 2 of the concision plan** (`~/.claude/plans/cheerful-gliding-honey.md`)
  — consolidation rounds, where the planner hands one worker a deletion brief
  every K rounds. Waiting on one round closing under rule 2 so the credit is
  observed firing before a scheduler depends on it.
* ~~**The client half of §8.5's golden corpus.**~~ Done by `loop/core-1` and
  landed in the merge — see "One thing the merge settled" above. What is left
  is `core-020`, which is a smaller question about where a script lives.
* **`archive/loop-core-3`** — dropped, not deleted, and it is a **tag** rather
  than a branch, so `git branch` does not show it. Its rope work was redundant
  with what `core-2` landed; its driver work conflicted semantically and its
  gaps remain open, so the planner will retarget them cleanly.

## Operating it

```
harness/hj status                     # everything, per loop
harness/workers core                  # start the fleet (rounds)
harness/loop harness                  # the harness loop (single worker)
harness/loop core --worker 1 --once   # one worker, foreground, no planner
harness/gate <loop>                   # the 7 steps
harness/hj selftest                   # 57 hermetic checks, ~0.5s
harness/dashboard/serve               # http://127.0.0.1:8787
```

**Launch from the integration checkout, always — including the single-worker
`harness/loop harness`.** `loop` takes its repo from `$HJ_REPO` or from its own
`dirname`, so starting it from inside `../heuristic-jump-harness` makes it read
*that worktree's* `state/phase.toml` and its own `harness/prompts/`. Both were
one commit stale on the restart, and the loop exited saying `harness is
'paused'` seconds after `phase.toml` had been set to running. It looks like a
config that did not take. `harness/workers` gets this right on its own — it
`cd`s to the integration checkout and exports `HJ_CONTROL`.

Fast-forward the worktrees before starting, and again after any commit to
`main` that a loop should see:

```
for d in ../heuristic-jump-{core-1,core-2,core-3,harness}; do
    git -C "$d" merge --ff-only main -q
done
```

Pause by setting `status = "paused"` in `state/phase.toml` **in the integration
checkout** — workers read it from there now. They stop at the top of the next
iteration; running campaigns finish and merge.

Interventions: `state/decisions/*.md` with `status: open`, unreviewed entries in
`state/spec-changelog/`, and campaigns flagged `spec_drift`. All three surface in
`harness/hj status` and on the dashboard. `harness-request` decisions route to
the harness loop automatically and do not need a human.
