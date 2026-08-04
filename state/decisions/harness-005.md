---
id: harness-005
status: accepted
opened: 2026-08-04T06:20:00+00:00
campaign: (none — raised by hand, landing the three worker branches)
kind: harness-request
---

# Two campaigns can take one id and the merge will not say so. Should the gate check?

## Context

`hj allocate-id` exists because two workers wrote `core-001` and two wrote
`CHANGE-core-005` within an hour of the fleet starting. It reserves a number
`O_EXCL` against the integration checkout, and it works: nothing allocated
through it has collided since.

What it does not cover is the branches that were already diverged when it
landed. Merging the three of them back turned up four collisions, and the
fourth is the one worth a record:

* `state/decisions/core-018` — two different questions. **Conflicted**, so a
  human saw it.
* `CHANGE-core-005`, twice over, and `CHANGE-core-006` … `010` — five entries
  under numbers another worker had used. **Conflicted**, so a human saw them.
* `CHANGE-core-005` a third time — `loop/core-2`'s rope-side entry against
  `main`'s `core.md` one. The two entries sit in **different parts of the
  file**, so git merged them cleanly into one document that defines the same
  id twice. Nothing conflicted. Nothing failed. `harness/gate core` passed all
  seven steps over it.

It was found by `grep -o '^## CHANGE-core-[0-9]*' | sort | uniq -d`, run on a
hunch after the third collision, not by any check the project has.

The failure mode is quiet and permanent. A changelog id is what a code comment
cites when it explains why a line says what it says — `wire_locations.rs` and
`vendor/rope/src/chunk.rs` both cite `CHANGE-core-005`, and after a silent
collision they cite different entries under one name. A human reading either
one is sent to a plausible, wrong paragraph. Renumbering later means moving
every reference again, and by then the id is in commit messages and in the
intervention log, which do not get to be rewritten.

## Options

* **A gate step.** `sort | uniq -d` over the `## CHANGE-<owner>-NNN` headings
  and over `state/decisions/*.md` ids, failing the gate on a duplicate. Costs
  a step in the seven and catches it at the commit that creates it, which is
  the only moment the fix is one file.
* **Fold it into `check-audit`,** which already runs at step 6 and already
  reads this class of file. Costs nothing structurally; hides a distinct check
  inside a step named for something else.
* **Leave it to `allocate-id`.** Defensible now that every live campaign is
  handed a reserved number: the collisions above are all from branches that
  predate it. Costs the assumption that no future campaign ever writes an id
  by hand — and the prompt still shows examples with literal ids in them.

## Decision

**accepted: Option B — fold the duplicate-id check into gate step 6**,
answered 2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

Step 6 already bundles check-audit and check-links under 'audit consistency,
and the design's own cross-references', so a third command there is consistent
with what that step already is rather than a distinct check hidden inside it —
which is the objection the record raises against B and which applies less than
it looks. It should cover both id spaces at once, decisions and changelog
entries, since the same two campaigns collided in both. Option C, leaving it
to allocate-id, assumes no campaign ever writes an id by hand while the prompt
still shows literal ids in its examples.

### What is left

The harness loop's work, in `harness/gate` step 6 and a new `hj` check.

## Provisional choice in force

None. The four collisions found are resolved in the merge commits
(`ed91c10`, `77c3c72`), renumbered to one past the highest as `allocate-id`
would have done, with every reference moved. No check is installed, so a fifth
would be as quiet as the third was.

## Consequences

If a check lands, it is worth pointing at both id spaces at once — decisions
and changelog entries — since the same two campaigns collided in both. If it
does not, the thing to write down instead is that a merge of a long-diverged
branch needs a duplicate-id pass by hand, because the conflicts it does raise
are not the whole story.
