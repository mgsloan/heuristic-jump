---
id: harness-002
status: accepted
opened: 2026-08-04T01:20:00+00:00
campaign: bb1e501a-8f20-4466-9bb5-391bae86785a
kind: class-b
---

# Layer 3 of §13's isolation is unconfigured, and its `allowWrite` list is narrower than the ownership table it sits beside — enable it, and against which list?

## Context

`design/loops.md` §13, "Mechanics: isolation in four layers", names four.
Three exist:

* **1, build isolation** — `hj crates <loop>`, and the gate builds only those.
* **2, `PreToolUse`** — `harness/hooks/edit-scope`, wired in
  `.claude/settings.json`, delegating to `hj check-path`.
* **4, gate diff scope** — `hj check-scope`, resolved against
  `state/phase.toml` and `DENIED_ALWAYS`.

Layer 3 does not:

> **3. OS sandbox.** `/sandbox` uses bubblewrap on Linux and takes an
> `allowWrite` list. This *is* a real boundary — it covers subprocesses — and
> it is the layer that answers "prevent them from writing outside their dir"
> literally. `allowWrite` is the owned crate directory,
> `state/shared-proposals/`, `target/`, and the git directory; everything else
> in the checkout is read-only to the session.

`.claude/settings.json` configures the `PreToolUse` hook and nothing else. No
sandbox is enabled for any loop, so the section's own account of why layer 2
is not a boundary — "anything the hook blocks is reachable through `sh -c`" —
currently has no layer behind it. The gate is the only real enforcement, and
it is post-hoc.

Two reasons this is not something a loop can just do.

**It is denied.** `.claude/**` is in `DENIED_ALWAYS` for every loop in every
phase, which is right: a loop that could edit its own sandbox configuration
has no sandbox.

**As written, it would break the loop.** The quoted `allowWrite` list is
narrower than the ownership table four paragraphs below it, which grants the
conformance loop `design/` and grants every loop its `state/` files. A
campaign runs `harness/hj record` and `harness/hj cost` itself — the prompt
requires the first after every commit — and those write
`state/metrics/<owner>.jsonl` and `state/cost/<owner>.jsonl`. Since the
workers landed, `hj cost` also appends to `state/sessions.jsonl` in the
*integration* checkout, which is outside the worktree entirely, and the
transcript root is outside it too. Under the list as quoted, a campaign's
first `hj record` fails and the gate's metrics step then fails every
subsequent commit.

So this is not "switch it on". The list has to be derived from the same
ownership table layer 4 reads, or the two layers enforce different things and
the stricter one wins by accident.

## Options

**A. Leave layer 3 unimplemented; say so in the document.** Cost: the section
keeps four layers and the deployment has three, and the honest version is that
a campaign's bash can write anywhere in the checkout. The gate catches it at
commit time, which is what actually holds today. Cheapest, and loses the
property the section calls "a real boundary".

**B. Enable it, with `allowWrite` derived from `hj`'s own tables** — the
loop's `write` patterns from `state/phase.toml`, plus `target/`, the git
directory, the integration checkout's `state/` and the transcript root.
Cost: a human writes and maintains `.claude/settings.json` per loop, and
every future path the harness writes is a new way for a campaign to die
mid-run in a way that looks like a tool failure rather than a permission
one. It also has to be got right per *worker*, since each has its own
worktree.

**C. Enable it with a coarse list — the whole worktree, plus the git dir,
the integration `state/`, and the transcript root.** Cost: it stops a
campaign escaping its worktree, which is the containment that matters when
several worktrees run at once, and does *not* stop it writing another loop's
files inside its own tree. Layer 4 already catches that, and unlike B it
cannot break the harness's own writes, because they are all named.

## Decision

**accepted: Option C — the coarse list, containing the worktree**, answered
2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

The containment that matters when several worktrees run at once is that a
campaign cannot escape its own, and C buys exactly that. It cannot break the
harness's own writes because every path the harness needs is named — the
worktree, the git directory, the integration checkout's state/, the transcript
root — where B's per-loop, per-worker derivation makes every future harness
write a new way for a campaign to die mid-run looking like a tool failure
rather than a permission one. What C does not stop, a campaign writing another
loop's files inside its own tree, layer 4 already catches at commit time. A
gives up a boundary the section is right to call real. .claude/** is denied to
every loop, correctly, so this is a human edit.

### What is left

Done in the same commit as this ruling: `.claude/settings.json` carries the
sandbox, since `.claude/**` is denied to every loop. `design/loops.md` §13's
`allowWrite` paragraph still describes the narrower list and is the harness
loop's to reconcile.

**Reconciled** by campaign `3e637dcd`, `CHANGE-harness-007`. Layer 3 now
describes the coarse list, names `failIfUnavailable`, states the limit option C
accepts — a campaign can write another loop's files inside its own checkout,
and layer 4 catches it at commit — and carries the finding this record said it
did not predict: the list cannot be the session's own project root alone,
because `harness/workers` fast-forwards every worker's worktree from the
integration checkout.

**What enabling it turned up, which the record did not predict.** The list
cannot be "the worktree" in the singular. `harness/workers` runs in the
integration checkout and writes into *each* worker's worktree — the
`git merge --ff-only main` it does after an audit — so a list holding only the
session's own project root breaks the round runner rather than a campaign. All
five checkouts are named. This was found the way the record warned it would
be: a `touch` that had worked minutes earlier began failing with
`Read-only file system`, which reads as a tool fault and not a permission one.

`~/.cargo` is on the list for the same reason and is worth stating, since it
is the one entry outside the project: `cargo` writes its registry cache and
`.package-cache` lock there, and without it step 2 of the gate fails before
compiling anything.

Verified rather than assumed, in a worker's worktree under the live sandbox:
`harness/gate core` passes all seven steps, 271 tests. Containment bites where
it should — `$HOME`, `~/.ssh` and `.claude/` itself are read-only, so a loop
can no longer reach the file that configures its own sandbox, which is the
property this record wanted. `design/` inside the checkout stays writable,
which is option C's stated limit and layer 4's job.

`failIfUnavailable` is set, so a machine without bubblewrap stops rather than
running every campaign unsandboxed behind a warning. That is the same posture
as abstaining rather than guessing, applied to the harness.

## Provisional choice in force

**A, by default rather than by choice**: `.claude/**` is denied to me, so no
option here is one I can apply. Nothing is tagged, because there is no site
to tag — the absence is the state.

What I did instead is record it. This campaign is the first to read §13's
layer list against the deployment, and an unimplemented layer that nobody has
written down is indistinguishable from one that was considered and rejected.

If the answer is B or C, the work is a `.claude/settings.json` edit and a
`prompt-revised`-adjacent intervention log entry; if it is A, the work is one
paragraph in §13 saying the sandbox is not enabled and why, which is a Class
A edit somebody should make deliberately rather than a loop making it to
close a gap.

## Consequences

If A: §13 needs the edit, and the "four layers" framing becomes three plus a
rejected one. Nothing else moves.

If B or C: a campaign that has been able to write anywhere in its checkout
suddenly cannot, and the failure mode is a bash command failing for a reason
the model will not attribute correctly. Expect one campaign lost to
diagnosing it, which is an argument for doing it at a phase boundary rather
than mid-phase. The gate's diff-scope check stays either way — it is the
layer that inspects the result, and it is what makes the others optional.
