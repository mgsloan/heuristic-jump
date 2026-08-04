# Spec changelog — harness

Class A spec fixes made by the harness loop, in the shape
`design/loops.md` section 6 asks for. Anything that trades something off is
Class B and belongs in `state/decisions/`, not here.

**An entry is provisional until a human reads it.**

## CHANGE-harness-002 — loops.md#sessions-assign-the-id-own-the-transcript — the teed transcript lives outside the worktree, not under `state/`

**Contradiction:** section 16 gives the transcript two homes, four
paragraphs apart. `#sessions-assign-the-id-own-the-transcript` says "write
the stream to `state/sessions/<owner>/<campaign-id>.jsonl` as it goes".
`#reading-a-transcript` says "Transcripts are not committed; they live
beside the corpus, outside the worktree, and old ones for closed campaigns
whose hypothesis was confirmed can be dropped."

`state/` is committed in its entirety, so the first location makes the
second impossible: nothing under it can be uncommitted, and dropping a file
that is in git does not reclaim the disk the paragraph is about.

**Resolution:** the section now names the transcript root by reference to
`#reading-a-transcript` rather than as a path under `state/`, and says
explicitly that the index row stays in `state/sessions.jsonl` while the
stream never enters git.

This is the reading that trades nothing off, because the other direction is
refuted by the same section: "Two MB per campaign, hundreds of campaigns
across seven languages, is gigabyte scale", and a retention rule that
deletes old transcripts cannot operate on committed paths without rewriting
history. There is no version of "under `state/`" that survives its own
paragraph. The claim being dropped is a path; the claims being kept are
uncommitted, outside the worktree, and droppable — three, against one.

**Declared, per the loop prompt:** the code already writes transcripts
outside the worktree (`harness/adapter transcript-path`, rooted at
`HJ_TRANSCRIPTS` or `../heuristic-jump-transcripts`), so this edit moves the
document toward the implementation, which is the shape that cannot be caught
by the audit. Said plainly here: I did not check which came first, and the
argument above does not depend on it — a reader who disagrees should look at
the two quoted sentences and not at the code.

**Campaign:** bb1e501a-8f20-4466-9bb5-391bae86785a

## CHANGE-harness-001 — loops.md#4-the-iteration-contract — the tests step names `hj selftest` as well as `cargo nextest`

**Contradiction:** section 4 says the gate's steps are "all mandatory" and
lists `cargo nextest run -p <owned crates>` as the test step. Section 13 and
section 18 together create a loop that owns no crate — this one, whose
`crates` list in `state/phase.toml` is empty and whose deliverable is
`harness/`. For that loop the first three steps all print `skipped`, so a
gate that is "all mandatory" executes not one line of the code being changed.

The two claims are consistent only if the harness is not code, and it is:
`hj` computes `sections_clean`, the gap ledger, the cost rows and the scope
check. Section 19 lists "the loop rewrites the spec toward what it built" as
the failure with the thinnest defence; a loop that can silently break the
instrument computing its own number is the same failure with no defence at
all.

**Resolution:** the test step is now `cargo nextest run -p <owned crates>`
*and* `hj selftest`, the latter unconditionally. This trades nothing off:
the check is additive, it is hermetic (in-memory fixtures, no repository
state, no network), it costs about half a second, and no existing check is
weakened or narrowed. The alternative readings both cost something — leaving
it out keeps a loop unchecked, and making it conditional on owning no crate
would mean the check that guards the shared tool does not run on the gate of
the loop most likely to be affected by them breaking.

`hj selftest` is 19 checks over the parsing and arithmetic this campaign
added: cost-row merging, the experiment mix, audit-interval attribution,
spend attribution across phases and languages, budget scopes, and the
adapter's reading of the stream. Each was verified to fail under a mutation
of the code it covers, rather than only to pass.

**Declared, per the loop prompt:** this campaign edited a design document and
the code that document describes, in the same run. The document edit is one
bullet in section 4's gate list and it *widens* what the gate demands; the
campaign's three other commits are in `harness/` and are described by
section 15, which was not edited. `hj campaign-close` will flag the run
regardless, and it should — the declaration is here so the flag has an
answer next to it rather than an archaeology exercise.

**Campaign:** 11b9c019-6714-4563-a97b-fd9a00c5819f

## CHANGE-harness-003 — loops.md#1-current-state-and-what-it-forces — the section stops carrying an inventory of what exists

**Contradiction:** §1 opened "There is no code. There are the design
documents, a `clippy.toml`, and a `CLAUDE.md`. Every loop described here is
blocked on a bootstrap that is not itself loopable in any interesting sense,
because there is nothing to measure yet."

Both halves are contradicted from inside this document. §18 ("Scope: phases 1
and 1.5 first") runs the conformance loop *against* phase 1a — "this is the
phase the loop machinery is built for and first run against" (§8) — so not
every loop is blocked on the bootstrap, and the bootstrap is loopable, because
it is being looped. And §2's whole point is that the two loops have different
oracles: the conformance loop's is "the audit, plus the test suite", which
needs no code at all to exist. The inventory half is stale as a matter of
fact: seven workspace members exist.

**Resolution:** §1 now says the document was written before there was any
code, states that current state is deliberately *not* recorded here, and names
where it is — `state/phase.toml`'s `phase`, `Cargo.toml`'s members,
`state/audit/`. It then states the split the state of the code actually
forces: the metric loop is blocked on the 1a–1.5 bootstrap, the conformance
loop is not, because its oracle exists as soon as the documents do.

This reading trades nothing off because it is the only one that is stable.
Restating the inventory correctly — "seven crates exist" — buys one audit and
is wrong again by the next campaign, and §8 already made exactly this call
for exactly this reason: "'what is in phase 1a' is now a question with a file
for an answer rather than a list to maintain here." The forcing content of the
section, which §13 and §18 reason from, is unchanged: there is still no shared
resolution crate, and there must not be one until working handlers exist to
extract it from.

**No code was changed in this campaign.** That cuts both ways and is worth
saying plainly: nothing here was moved toward an implementation, and equally,
nothing here can be checked by a test. The section's claims after this edit
are checkable against `state/phase.toml`, `Cargo.toml` and `crates/` — none
of which this loop may write except `design/loops.md` itself.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-004 — loops.md#2-two-loops-two-oracles — the conformance loop's concurrency is N workers, not one writer

**Contradiction:** §2's table said the conformance loop's concurrency is

> one writer

while §13's Workers subsection says

> A loop may also be parallelised *within* itself: **N workers, each running
> one campaign at a time, in its own worktree on its own branch, all against
> the same document set and the same gap list.**

and goes on to specify the planner, `hj claim`, the per-worker journal and
findings files, and the rule that "at most one audit runs at a time". It also
states the consequence the one-word cell was hiding: "Section 13's
'conflict-free by construction' is a claim about *loops* and does not hold
between workers; between workers, conflict is a rare event with a handler
rather than an impossibility."

`state/phase.toml` — human-owned and denied to every loop — sets
`workers = 3` for `loop.core`, so the desired state agrees with §13 and not
with §2.

**Resolution:** the cell now reads "N workers, a campaign each, one gap list,
conflict handled rather than excluded", linking to
`#workers-one-loop-several-campaigns-at-once` rather than to
`#parallel-loops-and-what-they-share`, which is the *across*-loops case and
was never what this cell meant.

This trades nothing off. §13 is the later and vastly more detailed passage —
it argues for N, names its costs (spend linear in N, throughput sublinear,
disk linear), and says how to pick it; §2's cell is a one-word index entry
into a section that had moved. Every other cell in that row and column is an
index into a section that carries the argument, so making this one match the
section it points at is what the table is for. The clause "conflict handled
rather than excluded" is carried across deliberately, because it is the part
of the old cell that was load-bearing: a reader who took "one writer" away
from §2 had learned something true about *why*, and dropping N in without it
would lose that.

Not corrected here, because no loop may write it: the comment above
`[loop.core]` in `state/phase.toml` still opens "Phase 1a is one writer" and
then sets `workers = 3` twelve lines later. That is the same stale sentence
in a file this loop is denied.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b
