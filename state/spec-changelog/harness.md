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
