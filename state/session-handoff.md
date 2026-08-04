# Session handoff — 2026-08-04

Written at the end of a long session for whoever picks this up. It is not loop
state; no loop writes or reads it. `state/handoff.md` is a different thing —
that one means a loop stalled and asked a question.

## Where things stand

Both loops are **paused**, `main` is **green** (257 tests, all 7 gate steps),
the working tree is clean, and the intervention queue is **empty**.

| | clean | gaps | campaigns |
|---|---|---|---|
| `core` (core.md, deps.md, rope-modifications.md) | **60/71** | 12 open of 96 found | 39 |
| `harness` (loops.md) | 13/75 | 16 open | 3 |

42 campaigns closed, 87 interventions logged, ~$800 of API-equivalent spend.

**Do not restart the fleet until the two branches below land**, or they keep
diverging from a moving `main`.

## The two things blocking a restart

### 1. `loop/core-1` (41 commits) and `loop/core-2` (31) are unmerged

Both are real, gated-green work that failed to merge. They are intact; nothing
is lost. `archive/loop-core-3` is a third, deliberately dropped (see below).

I tried to land `core-1` by hand and **got it wrong**: I "unioned" a conflict
in `crates/measure_core/tests/pipeline.rs` assuming it sat between two whole
test functions. It cut through a function body and spliced the tail of one test
into the middle of another. It did not compile. I backed the merge out.

The lesson, which I had already written down and then ignored: **for a
conflict where two campaigns wrote different versions of the same thing,
splicing produces a third thing neither wrote.** Take one side whole, or leave
it. Only append-only files (`golden-traffic.jsonl`, the JSONL logs) are safe to
union.

What is known about the conflicts, from having worked through them once:

* `crates/driver/tests/seam.rs` — a human inline `matches!(member.as_str(), …)`
  fix against the campaign's named `INSTALLS_THE_SUBSCRIBER` constant. **Take
  the campaign's**; it is the better factoring.
* `crates/measure_core/tests/pipeline.rs` — two independent tests at the same
  insertion point. Whole functions only. This is the one I broke.
* `crates/shared/src/proto.rs`, `crates/shared/tests/differential.rs` —
  competing doc comments for identical code. Either is defensible; take one
  whole.
* `state/spec-changelog/core.md` — two campaigns both wrote
  `CHANGE-core-005` for different questions. Keep both, renumber one.
* `state/decisions/core-018.md` — the branch has a stale copy; `main`'s carries
  the human answer.
* `state/audit/**` → take `main`'s. `state/findings/*` → take the worker's.
  These two are mechanical and `merge_back` now does them automatically.

**Recommendation**: merge (not rebase) each branch into `main` one at a time,
resolve the seven files above, run `harness/gate core` in the worktree, and only
then fast-forward. `merge_back` now picks merge over rebase automatically above
8 commits, so a fresh attempt will not repeat the rebase thrash.

### 2. `crates/driver/tests/seam.rs` is a contended file

2,300 lines, 71 functions. Almost every `deps.md` or `core.md` section is closed
by appending a manifest assertion to it, so three workers append to one file and
two lose. **It has blocked a merge in every round so far.**

The planner has been told to treat it as a resource one worker may hold
(`harness/prompts/planner.md`), which is a mitigation. The fix is to split it by
topic — manifests, licensing, lints, layout, vocabulary — and that is a
campaign's work, not a paragraph. It is the highest-value single target
available.

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
* **The client half of §8.5's golden corpus.** `harness/capture-editor-traffic
  --install` prints a Zed setting; type in a Rust file for a minute; `--finish`
  folds it in. Two minutes of a human's time, deliberately left as tooling
  rather than a note (`state/decisions/core-018.md`).
* **`archive/loop-core-3`** — dropped, not deleted. Its rope work was redundant
  with what `core-2` landed; its driver work conflicted semantically and its
  gaps remain open, so the planner will retarget them cleanly.

## Operating it

```
harness/hj status                     # everything, per loop
harness/workers core                  # start the fleet (rounds)
harness/loop harness                  # the harness loop (single worker)
harness/loop core --worker 1 --once   # one worker, foreground, no planner
harness/gate <loop>                   # the 7 steps
harness/hj selftest                   # 23 hermetic checks, ~0.5s
harness/dashboard/serve               # http://127.0.0.1:8787
```

Pause by setting `status = "paused"` in `state/phase.toml` **in the integration
checkout** — workers read it from there now. They stop at the top of the next
iteration; running campaigns finish and merge.

Interventions: `state/decisions/*.md` with `status: open`, unreviewed entries in
`state/spec-changelog/`, and campaigns flagged `spec_drift`. All three surface in
`harness/hj status` and on the dashboard. `harness-request` decisions route to
the harness loop automatically and do not need a human.
