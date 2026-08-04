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

## CHANGE-harness-005 — loops.md#8-sequencing-and-gates — phase 1b's gate is the tuning / held-out split, and the final carve-out is deferred

**Contradiction:** §8's phase 1b gate required

> repositories checked out at pinned commits, **and the tune / select /
> final split decided and physically separated** ([section 12](#12-held-out-integrity))

while §12, the section it cites for that requirement, argues the opposite
about the third of those three:

> The remedy, if it starts to matter, is to stop selecting on part of it:
> carve a final set out of the five, evaluate it once at the end, and never
> let it choose anything. [...] **the split can be made finer later and never
> coarser.** Deciding it now would mean guessing how much leakage ten gates
> actually cause, which the first few gates will say.

`harness/corpus` implements two splits, `training` and `test`, and
`harness/corpus-selection.toml` records five and five per language — which is
what §12 specifies and what §8's gate, read literally, fails.

**Resolution:** the gate now requires the **tuning / held-out** split decided
and physically separated, and says in a following sentence that carving a
*final* set out of the held-out half is deliberately not part of it, with
§12's reason.

This trades nothing off, and the reason is structural rather than a
preference between two sections. §12's argument for making the split at 1b —
"once a repository has been in the tuning corpus, moving it to held-out does
not un-teach it" — applies to the tuning boundary and only to it. The final
set is carved *out of held-out*, which has never been tuned on, so deferring
it costs nothing that can later be recovered, while deciding it now costs a
guess about leakage that the first few gates will measure. The irreversible
half still lands at 1b's gate; only the half that stays available is moved
out of it. §8's own sentence "the split has to be made here, not later" is
kept and now attaches to the split it is an argument about.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-006 — loops.md#8-sequencing-and-gates — the handler registry is phase 1a's; only parallel dispatch is 2b's

**Contradiction:** §8's phase 1a paragraph says, in one sentence,

> [`core.md`](core.md) in its entirety, which is the document's whole scope

and in the next,

> Explicitly **not** the router, the health model, the actor, dispatch,
> standalone, or divergence reporting

but `core.md` §1 requires the thing "dispatch" excludes:

> `LanguageId` and `FileExtension` are interned, not strings. A handler
> declares its ids as consts; **the driver resolves an incoming LSP
> `languageId` against the registry** and gets `Option<LanguageId>`.

and §1 elsewhere makes `grammar()` "what keeps `driver` language-free",
which is a property of the registry and of nothing else. `shim.md` §13's
module layout is where the ambiguity comes from: it puts two unrelated things
under one directory name —

```
  dispatch/
    pool.rs         bounded worker pool, deadline enforcement
    registry.rs     languageId / extension -> handler, grammar lookup
```

— so "dispatch" on an exclusion list excludes both.

**Resolution:** the list now says **parallel dispatch**, and a following
clause states that the handler registry is `core.md` §1's and therefore phase
1a's, while what 2b holds is `shim.md` §10's bounded pool and its fan-out to
several servers.

This trades nothing off: it resolves a name collision in favour of the
reading that the same paragraph already requires. "core.md in its entirety"
and "not the registry" cannot both hold, and no phase-1a work is possible
without the registry, since `measure_core` and `driver` both need a
`languageId` to reach a handler at all.

**Not resolved here, deliberately:** the other name on that list with the
same problem is "the actor", and it does not have a tradeoff-free answer. It
is escalated as `state/decisions/harness-007.md` and the exclusion list is
left standing, so `loops.md#8-sequencing-and-gates` keeps an open gap it
could have been made to lose. §8 now states the overlap and names the three
files that cross it instead of being widened to fit them.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-007 — loops.md#5-the-auditor-and-the-conformance-loops-number, #levers-by-which-resource-they-move, #mechanics-isolation-in-four-layers — reconciling two answered decisions

Not a contradiction found this campaign. These are the edits two *answered*
Class B records left for this loop, and they are recorded here because the
change is to the spec either way and a reader of this file should not have to
join it to `state/decisions/` to see what moved.

### `harness-003` — the audit cadence

**Contradiction, as the record framed it:** §5 said "At every campaign close,
a **separate session with no memory of writing the code**..." and §15 said
"The auditor is a fixed cost of one session per conformance campaign, and it
is not a knob". §13's "The audit does not parallelise" says the opposite:
"**at most one audit runs at a time, and it runs against `main` rather than
any worker's branch.** A worker whose close makes the audit due runs it; the
others skip and continue." `state/phase.toml` carries `audit_every`, and a
knob set to 1 is still a knob.

**Resolution — answered `accepted: Option B`, 2026-08-04:** §5 now says "at
every **round** close" and defines a round as one campaign for a loop that
runs one at a time and N for a loop running N workers, with §13's argument
for why (three workers each judge a tree nobody ships) and the cost stated
rather than elided (a campaign can close against a verdict older than
itself). §15 now says "one session per conformance **round**" and "not a knob
**a loop may turn**", naming `state/phase.toml` as where `audit_every` lives
and why that is the whole of the protection.

The trade is the record's, not this campaign's: Option A — audit every
campaign, delete the knob — was costed at roughly 40% on top of `core`'s
campaign bill and rejected at this phase.

### `harness-002` — layer 3 of §13's isolation

**Contradiction:** §13's layer 3 said "`allowWrite` is the owned crate
directory, `state/shared-proposals/`, `target/`, and the git directory",
which is narrower than the ownership table four paragraphs below it in the
same section, and the layer was unconfigured besides.

**Resolution — answered `accepted: Option C`, the coarse list containing the
worktree:** the paragraph now describes what is in `.claude/settings.json` —
every worktree, the integration checkout's git directory and `state/`, the
transcript and log roots, `~/.cargo` — and states the limit that choice
accepts: a campaign can write another loop's files inside its own checkout,
and layer 4 catches it at commit time. `failIfUnavailable` is recorded,
because a sandbox that silently degrades is not the layer this section claims.

The per-crate list did not merely under-describe the deployment; it does not
survive it. `harness/workers` runs in the integration checkout and
fast-forwards each worker's worktree after an audit, so a list holding only
the session's own project root breaks the round runner. That is in the
record's ruling as something it did not predict, and it is the sentence worth
carrying into the spec.

### On the shape of this entry

Both edits move the spec toward a deployment that already exists, which is
the shape §19 warns about. What makes them not that: a human ruled on each
before it was written, the ruling is in `state/interventions.jsonl` rather
than in a record this loop could have edited, and each record names the
option that was *rejected* and what it would have cost. The changelog entry
is the place a reviewer can check that claim against the records.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-008 — loops.md#6-spec-changes-what-the-loop-may-decide-alone — the batch trigger counts records waiting, not tagged sites

**Contradiction:** §6 says a batch is triggered by

> the outstanding `DECISION-` count crossing a threshold, or a phase gate

and defines that count two paragraphs earlier as the grep:

> `grep -r DECISION-` is the outstanding-provisional-choice report, and its
> count is a health metric

Those are not the same set, and the difference is not a corner case. A record
can be waiting on a human with **no taggable site at all** — the choice is
about a file the raising loop may not write, so there is nowhere to put
`// DECISION-<id>: provisional` and no work the loop could do meanwhile.
Three of this loop's records say exactly that in their own "Provisional choice
in force" section: `harness-002` (`.claude/settings.json`), `harness-003`
(`state/phase.toml`), `harness-007` (another loop's crate). A trigger driven
by the grep count would never fire for any of them, which inverts the
mechanism — those are the escalations that most need a human, because they are
the ones the loop cannot route around.

**Resolution:** the trigger counts records waiting on a human and crosses
`escalation_batch` in `state/phase.toml`; the grep count keeps the job §6
already gives it, which is the health metric. §6 now says both, and says why
the earlier reading fails.

This trades nothing off. The two counts are different questions — "is the loop
running ahead of its decisions" and "is there enough for a human to sit down
with" — and §6 already asks both; the edit stops one number being asked to
answer both. Nothing is loosened: the health metric is unchanged and still
recorded in every metrics row.

**What was built, and the one thing it deliberately does not do.**
`hj escalations` prints the queue, each record's age *in campaigns closed
since it was raised* — §6's "reconciliation gets more expensive the longer it
waits", measured in the unit the expense is actually paid in, since a quiesced
fleet costs nothing to wait — and whether a batch is due. It exits 1 when due.
`harness/loop` does not consult it and the docstring says it must not: §6 says
the loop "never idles waiting for an answer", so unlike `hj budget`, a
non-zero exit here is a message to the operator rather than a condition the
harness acts on. The trigger also appears on `hj status` and in the note on
the dashboard panel a batch is answered from.

`escalation_batch` is **absent** from `state/phase.toml`, which is denied to
every loop. It defaults to absent rather than to a guessed number, for the
reason `Budget` gives for the same choice: an invented threshold either cries
wolf or never fires, and §6 names no number. With it unset the phase gate is
the only trigger — a degenerate cadence, not a broken one, and setting it is
one line for whoever decides what the number should be.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-009 — loops.md#branches-exist-for-one-commit-at-a-time — the merge is per campaign, and `master` is `main`

**Contradiction:** the subsection said

> So each loop has a branch, and **merges after every green iteration** [...]
> The branch exists for the duration of one commit. There is no merge queue
> and no integration loop; merging is a step in the iteration contract
> ([section 4](#4-the-iteration-contract)), not an agent.

§4's iteration contract has six steps — read, pick, do, gate, commit, exit —
and no merge step. So the cross-reference does not resolve to anything: the
section it names as the home of the merge step does not contain one.

**Resolution:** the merge happens when a campaign closes, and is a step in the
campaign contract rather than the iteration contract.

The reason it cannot be per commit is not a preference. The merge rebases the
branch onto `main`, and rebasing mid-campaign swaps the working tree under a
session that is reasoning from it — including `state/audit/`, which holds the
gap list that campaign was given and is the only oracle it has. That is
recorded evidence rather than a hypothesis: `harness-004` was raised by a
campaign that reached this conclusion the hard way, and its note reads "the
audit that produced this campaign's gap list was computed in *this* worktree,
and rebasing swaps it for a different one halfway through."

Nothing is traded away, because the property the subsection is *for* survives
whole: a campaign is hours rather than weeks, so no long-lived branch
accumulates and `main` still receives a campaign's work as soon as it is
judged. What changes is the unit, from commit to campaign, which is the same
unit §4's "campaigns are the unit of fresh context" and §15's accounting
already use.

Two further sentences were added because they change what `main` looks like
and were only in the code: the fast-forward is serialised by a lock, so a
worker that read `main` a moment before another moved it waits and rebases
onto what landed rather than losing a race; and linear history is traded for a
merge commit once a branch is far enough ahead that a rebase would re-resolve
one conflict on every commit it replays. "Conflict-free by construction" is
now stated as a claim about *loops*, with the pointer to §13's Workers, which
already says it does not hold between workers of one loop.

`master` became `main` in three places. The branch is `main`; nothing in the
harness has ever referred to `master`.

**Left undone deliberately: the heading.** "Branches exist for one commit at a
time" now contradicts its own body. Renaming it changes the anchor, which
appears in `state/audit/loops.toml`, in this document's own cross-references,
and in `harness/section-baseline.toml` — the last of which is denied to every
loop, so a rename would show as permanent baseline drift and one missing
section that no campaign could clear. It is a one-line change for whoever
retakes the baseline next, and the body's first correcting sentence carries
the meaning until then.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b

## CHANGE-harness-010 — loops.md#7-progress-stall-and-the-ways-it-is-faked — what ralph's two heuristics mean in this harness

**Contradiction:** §7 adopts two heuristics from `frankbria/ralph-claude-code`
"which ships them as tuned constants" —

> **flag when ≥30% of recent iterations are test-only changes**, and **cap
> consecutive "done" signals** so a loop that has decided it is finished
> cannot keep saying so

— and neither term has a referent here. This harness has no "done" signal: a
loop's `status` is set by a human in `state/phase.toml`, which no loop may
write, so the thing being capped does not exist under that name. And
"test-only" has two readings that differ by an order of magnitude on real
data, which is not a detail an implementation should be left to guess.

**Resolution:** §7 now says what each means here, and says that both report
and neither stops.

*Test-only* is a commit that touched Rust tests and **nothing else**. The
looser reading — touched no source — measured **71%** against the `core` loop
over its last twenty commits, at a time when that loop was closing gaps and
its commits were a test carrying a claim plus the `design/` edit settling it.
A flag that fires on campaigns doing their job is one that gets ignored by the
third time it fires. Under the strict reading the same window reads 7%.
Commits touching no Rust are outside the denominator rather than counted as
healthy, because the harness loop's own tests are `selftest_cases` *inside*
`harness/hj` and no path can distinguish them from the code they check —
reporting 0% would assert health about a loop this measure cannot see.

*A "done" signal* is a campaign closing `confirmed` or `partial` — a claim of
movement — when the repository shows none. This adds **no** stop condition:
such a campaign already increments `trailing_without_progress` and stops the
loop at N exactly like an honest `no-movement` close. What was missing is the
distinction, and it is the one that matters — the two shapes stop the loop
identically and mean opposite things. A run of `no-movement` is a loop that
hit something the spec did not anticipate; a run of `confirmed` with nothing
behind it is a loop that has decided it is finished, which is the failure the
heuristic is named for.

Nothing is traded off. The document named two mechanisms and this says which
observable each is, in a harness that had neither; no threshold was moved, no
check weakened, and the one number that was ambiguous was resolved toward the
reading that does not fire on correct work.

**Campaign:** 3e637dcd-7552-460c-8eb4-fb41941ef14b
