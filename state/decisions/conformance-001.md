---
id: conformance-001
status: accepted
opened: 2026-08-03T04:10:00+00:00
campaign: b59733c6-ebff-47a4-bccf-232abc532a07
kind: harness-request
---

# Can a loop ever pass `harness/gate` before committing, while the harness's own state files are dirty?

## Context

`harness/gate conformance` was red at step 4 before this campaign wrote a
single byte:

```
scope: state/interventions.jsonl: denied to every loop in this phase
scope: state/sessions.jsonl: denied to every loop in this phase
GATE FAILED at step 4: diff scope
```

Both files are written by the harness itself when it opens a campaign, both
are in `DENIED_ALWAYS`, and `check-scope` without `--rev` inspects
`working_tree_paths()` — "staged, unstaged and untracked" — so it sees them
whether or not the loop stages them. Nothing the loop is allowed to do
removes them from that set: writing them is denied, `git checkout` on
`interventions.jsonl` would destroy a human's record, and committing them
would be a loop putting denied paths into a commit that carries its own
`loop:` trailer.

So the iteration contract in the prompt — "after each change run
`harness/gate conformance`; green: commit" — has no green state available
during a live campaign. The gate is not wrong about ownership; it is
answering a question about the *working tree* when the thing being gated is
the *commit*.

## Options

1. **Take the verdict post-hoc, with `--rev`.** Commit only owned paths, then
   `harness/gate conformance --rev <sha>`, where step 4 checks
   `commit_paths(rev)` instead. Costs the pre-commit signal: steps 1–3 still
   run against the working tree and so are honest, but a scope violation is
   caught after the commit rather than before it, and fixing one means a
   rewrite rather than a `git restore`.
2. **`check-scope` ignores denied paths that the loop did not modify**, e.g.
   by intersecting the working-tree path list with paths whose content the
   loop could have written, or simply by excluding harness-authored state
   files from the pending-commit check. Costs the harness a way to tell "the
   loop touched a denied file" from "the harness did", which is currently
   free because any appearance is a violation.
3. **The harness commits its own rows** as it appends them, so the tree is
   clean between campaigns. Costs an extra commit per campaign in a history
   whose commits are otherwise one-per-experiment, and `commits_for_campaign`
   counts commits, so it would need to keep ignoring untrailered ones.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Option 1, because it is the only one available from inside the loop: options
2 and 3 are both edits to `harness/`, which no loop may make. This campaign
committed its owned paths and then ran `harness/gate conformance --rev HEAD`
for the verdict, and the campaign file and journal record that the pre-commit
run was red on those two paths alone.

It is also the most reversible: it changes nothing on disk. If the answer is
option 2 or 3, the only thing that changes is the order in which a future
campaign runs two commands.

No code sites are tagged, because there is no code to tag — the choice is
about how the gate is invoked, not about anything the gate inspects.

## Consequences

If this stays as it is, every campaign in every loop pays the same tax and
each one will rediscover it at its own step 4, which is a recurring cost of
exactly the kind `kind: harness-request` exists to make visible. Nothing has
to be redone if the answer goes the other way.

The sharp edge worth naming: a loop that hits this and does not read the
deny list carefully could conclude the obvious fix is to `git add` the two
files and commit them, which would pass step 4 on the *next* run and would be
a loop quietly committing the audit trail that exists to watch it.

## Answer — 2026-08-03T03:51:31+00:00

**Ruling:** accepted

Option 3, implemented: the harness now commits its own state as it writes it (campaign-open, campaign-close, record, audit-merge, intervene), path-limited and with no loop:/campaign: trailer.

**Rationale:** The report was correct and the diagnosis was exact — the gate was answering a question about the working tree when the thing being gated is the commit. Option 3 over option 2 because it keeps the deny list absolute: a carve-out would put an exception in the one check whose entire value is that any appearance of a denied path is a violation, and that check is what stops a loop committing the audit trail that watches it. Option 1 was the right provisional choice and the wrong permanent one, since a scope error then costs a history rewrite instead of a git restore. The untrailered commits are invisible to commits_for_campaign, check-metrics and the cost join, so the history stays one commit per experiment as far as anything that counts is concerned. Two follow-on bugs fixed with it: check-metrics now walks back past those commits to the last one carrying this loop`s trailer, and the loop`s reset --hard at close can no longer destroy an uncommitted harness row.

Reconciling the sites tagged `// DECISION-conformance-001: provisional` is a
normal campaign target, not an interrupt.
