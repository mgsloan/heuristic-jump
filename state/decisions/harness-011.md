---
id: harness-011
status: accepted
opened: 2026-08-04T21:45:00+00:00
campaign: 2e588730-78d0-4235-ad89-afebe7ddcdea
kind: class-b
---

# May a selftest assert a property of a file the loop it fails is denied?

## Context

`hj selftest` runs twice in gate step 3: the reviewed copy pinned on the
integration branch, then the tree's own when they differ. Both resolve
`HARNESS` through `HJ_REPO`, so a check that reads `HARNESS / "gate"` reads
**the worktree being judged** while the assertion itself comes from the
**pinned** copy. The two are different checkouts.

Two such checks were added to the integration checkout during this campaign:

    gate_step_5_calls_the_command_that_exists   reads HARNESS / "gate"
    the_size_ratchet_has_one_route              reads HARNESS / "hj"

They encode the answer to `harness-008`. They fail every branch that predates
that answer — which is every branch, because the paired `harness/gate` change
is uncommitted in the integration checkout and is on no branch at all. The
first is unfixable by any loop by construction: `harness/gate` is in
`DENIED_ALWAYS`. The second directs a loop to remove the `check-metrics` route,
which is answering `harness-008` — and in this tree that record still reads
`status: open`, so acting on it would be a loop ruling on its own escalation.

This is the second campaign to lose time to the shape. `78bbbbc4`'s journal
records the same failure from `c047b4c`, and `harness/readme.md` already warns
that "a check that reaches through `HJ_REPO` therefore tests *that* tree's
copy". The warning has not been enough twice.

The cost is not the diagnosis, which is ten minutes. It is that
green-or-revert stops meaning anything: the gate was red at HEAD with this
campaign's work stashed, so "revert to green" had no green to revert to, and a
campaign that follows the rule literally destroys verified work and still
fails.

## Options

**A — such a check reads the reviewed copy's own siblings**
(`Path(__file__).parent`) rather than `HARNESS`. Always self-consistent, never
fails a branch for what another checkout contains. Cost: it stops catching the
bug it is for. The `merge-blocked` defect was exactly "the shell changed and
`hj` did not", and in that case the tree's own `hj` is identical to the pinned
one, so the tree's copy is never run and the check never sees the change.

**B — keep reading `HARNESS`, and require that a check of this shape assert
only what every live branch already satisfies.** A check that encodes a change
lands *after* that change is merged, never with it. Cost: it is a convention,
and conventions are what failed twice. It also cannot be mechanised easily —
"would this fail a branch that predates it" is not decidable from the check.

**C — gate step 3 fails soft when the pinned copy's failure is not
reproducible in the tree's own copy**, reporting it as a harness defect rather
than a campaign failure. Cost: a real regression that happens to differ across
the two copies stops failing the gate, which is a hole in the one step that
runs everywhere.

## Decision

**accepted: A for a check that encodes a change, and option B's convention made
runnable rather than trusted**, answered 2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this line.

Two halves, because the record's three options each answer a different half.

**A, for the checks that caused this.** A case asserting how two harness files
agree with each other is asserting about the *reviewed* pair, and reads
`PINNED_HARNESS` — the directory the running file is actually in. `harness/gate`
is denied to every loop, so a branch carrying an older copy is behind rather
than broken, and failing it for that is failing a campaign for something it is
forbidden to fix.

**Not A everywhere**, which is where the record's stated cost bites. Three
selftest cases read `HARNESS` deliberately and should keep doing so:
`tuple_returning_misuse`, the prompt-shape check, and the tag-fixture region
check all assert invariants that every live branch already satisfies, so reading
the candidate tree is exactly where their value is — they catch a campaign
introducing the defect. Blanket A would silently retire them.

**B's rule is right and B's mechanism is not.** "Assert only what every live
branch already satisfies" is the correct boundary; relying on an author to
remember it is what failed twice, and the record says so itself. So it is now a
command: `hj selftest --across-worktrees` runs the reviewed checks against every
checkout `git worktree list` knows about and names the ones that fail. "Would
this fail a branch that predates it" stops being undecidable and becomes
something to run before committing a case that reads `HARNESS`.

C is rejected: a real regression that happens to differ across the two copies
would stop failing the gate, and step 3 is the one step that runs everywhere.

### Verified rather than asserted

With the fix in place, all five checkouts pass. With one check reverted to
`HARNESS` in a scratch copy, `--across-worktrees` fails, names
`heuristic-jump-core-2`, and prints the assertion — which is the failure this
record was raised about, reproduced on demand.

### What is left

The harness loop's: `design/loops.md` §13 and `harness/readme.md` both warn that
a check reaching through `HJ_REPO` tests the candidate tree, and neither says
what to do about it. They should name `PINNED_HARNESS`, the three deliberate
exceptions, and the command.

## Provisional choice in force

**None available.** Every route runs through files this loop is denied:
`harness/gate` for A and C, and the offending checks themselves are in an
uncommitted working tree that no branch carries. Nothing is tagged, because
there is no site this loop may edit.

What this campaign did instead is stated in the commit that hit it
(`[loops-7]`): it established that HEAD is red with the campaign's work stashed
— so the commit does not change the gate's colour — verified the work against
the tree's own `hj selftest`, and said so in the commit body rather than
letting a future reader infer a green gate.

## Consequences

Under A the two new checks keep their value only for paired changes and lose it
for exactly the drift they were written for; `shell_intervention_kinds` and
`shell_hj_calls`, added this campaign, read `HARNESS` for that reason and would
have to change with them. Under B nothing changes mechanically and the next
occurrence is a matter of whoever writes the next check remembering. Under C
step 3's contract changes for every loop, which is the widest blast radius of
the three and wants a human rather than a campaign.
