---
id: harness-004
status: open
opened: 2026-08-04T02:25:00+00:00
campaign: 8564e2f1-4e5b-4e5d-bfbd-76e363b98d6b
kind: class-b
---

# `harness/` has two writers — this loop and a human on `main` — so where does §13's "conflict-free by construction" leave the rebase?

## Context

`loop/harness` and `main` have diverged. Twenty commits each way, sharing a
merge base at `c0b9d74`, and neither is a superset of the other:

* `main` has hand-authored work this branch has never seen — `hj
  progress-replay` and the reworked progress rule, per-worker findings, the
  assignment plumbing, `interventions.jsonl` moved to the integration
  checkout.
* `loop/harness` has two campaigns of work `main` has never seen — the
  transcript view, the intervention log's kinds and hand-authored-commit
  flagging, `harness-002`, the gate's tree-`hj` selftest, and everything this
  campaign committed.

This is not the ordinary state of affairs and nothing announced it. It is
visible only by running `git log main..HEAD` by hand, which is not in any
loop's procedure, and its first consequence is that **the gate that grades
this loop is `main`'s copy** — so the tree-`hj` selftest step that the last
campaign added to `harness/gate` has never once run, because the pinned gate
predates it.

`design/loops.md` §13 states the property that is failing:

> Code and state are partitioned by owner, so the rebase is conflict-free by
> construction and the fast-forward touches nothing the other loop has open.

The partition holds between *loops*. It does not hold for `harness/`,
because the second writer is not a loop: `harness/` is "owned by nobody",
which in practice means it is owned by a human on `main` and by this loop on
its branch at the same time. `merge_back`'s failure message — "a conflict
here is a real finding: two loops wrote the same file" — names the right
finding and the wrong parties.

**The conflict itself is small, and this is the part a human should not have
to rediscover.** `git merge-tree main HEAD` produces exactly three hunks, all
in `harness/hj`, and none of them is a judgement:

1. `prompt_values` — `main` adds `findings_path`, this branch adds
   `journal_tail` and `recent_commits`, adjacent lines in the same dict.
   Both sides are kept.
2. `cmd_intervene`'s `append_jsonl` target — `main`'s `CONTROL / "state" /
   "interventions.jsonl"` supersedes this branch's `INTERVENTIONS`, which is
   the same fix `sessions_path()` already got: the file belongs to the fleet
   and lives in the integration checkout. `main` wins outright.
3. The two lines after it, for the same reason — with this branch's
   `return 0`, which the hunk boundary cuts across and `main`'s side does not
   contain.

Everything else — `design/loops.md`, `harness/loop`, `harness/dashboard/serve`
— merges without a conflict.

## Options

**A. Status quo: resolve at `merge_back`, by hand, when it blocks.** The
mechanism already exists and already fired for `core-3` at 00:54 today, and
its rationale is right that "a machine choosing a side here would hide the
evidence the planner needs". Cost: work sits on the branch for an unbounded
time — two campaigns so far — and during that time the reviewed harness is
stale, which is exactly the copy that grades every loop. The staleness is
silent: no campaign is told its branch is behind.

**B. The loop rebases onto `main` at the start of each campaign, before it
picks a target.** `harness/loop` already runs `campaign-reap` at that point,
which is the same kind of "make the tree sane before starting" step. Cost: a
campaign can find itself resolving another writer's conflicts as its first
act, with no context for either side, and a rebase changes the commit shas
that `state/metrics/` rows key on — a cost the design already pays at
`merge_back`, but paid at open it is paid before any work exists to justify
it. Buys: divergence stays one campaign wide, so it is always small enough
to be mechanical.

**C. Fail loudly instead of quietly.** Leave the merge policy alone, and
have `harness/loop` refuse to open a campaign on a branch more than N
commits behind `main`, logging it the way a stalled loop is logged. Cost: it
stops the loop rather than fixing anything, and needs a human either way.
Buys: the failure is at the top of a session instead of at the bottom of a
merge nobody reads.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**A, unchanged**: this campaign did not rebase. Two reasons, and the second
is the one that matters.

Rebasing mid-campaign replaces the tree the audit judged with a different
one — the gap list in this prompt was computed in *this* worktree, and the
sections it names are the sections here. A campaign that rebases into a
different `state/audit/` halfway through is scored against a picture nobody
gave it.

And resolving hunk 2 means deciding that `main`'s hand-authored fix
supersedes this branch's — which is correct, and is still a human choosing
between a human's work and a loop's. Doing it inside the loop is how the
evidence gets buried. So the resolution is written down above instead, in
enough detail to be applied in a couple of minutes.

No site is tagged: the choice is a branch's shape, not a line of code.

## Consequences

If A stands, expect this record to be re-raised — the next harness campaign
inherits a wider divergence than this one did, and the ratio of merge work to
campaign work grows with it.

If B: `harness/loop` grows a rebase at campaign open, and the metrics rows
written before it keep pointing at commits that no longer exist. `hj record`
is idempotent per commit, so the effect is a duplicate row rather than a
hole, but `check-metrics` looks up the *last* loop commit and would fail
until the campaign's first `record` — worth handling in the same change
rather than discovering at a red gate.

If C: one constant, one message, and the loop stops until somebody merges —
which is the honest version of what A does silently.
