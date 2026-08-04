---
id: harness-006
status: accepted
opened: 2026-08-04T07:10:00+00:00
campaign: (none — raised by hand, enabling layer 3 for harness-002)
kind: class-b
---

# `git add -A` fails inside the sandbox. Do the masked paths get gitignored, and at what cost?

## Context

Layer 3 is on (`state/decisions/harness-002.md`, option C). The sandbox hides
dotfiles it does not want a session reading — `~/.bashrc`, `.gitconfig`,
`.mcp.json`, `.claude/skills/` and nineteen others — by bind-mounting
`/dev/null` over each one, and inside the sandbox those mounts are visible at
the project root.

Two consequences. The first is fixed: `hj working_tree_paths` counted them as
pending paths, so step 4 of the gate told every campaign to revert twenty-two
files that exist in no checkout. It now drops character devices.

The second is not, and is the reason for this record:

```
$ git add -A
error: .bash_profile: can only add regular files, symbolic links or git-directories
fatal: adding files failed
```

`git add -A` and `git add .` both fail. `git commit -a` is unaffected, since
it stages tracked modifications only, and so is any path-limited `git add`.
Nothing in `harness/` runs the failing form — `commit_harness_state` is
path-limited — and no prompt names it. But a campaign is a Claude session
writing its own commits, and `git add -A` is what a session reaches for. The
failure is loud and its message does not name the sandbox, so the likely
outcome is a campaign that spends turns on it, or one that fails its gate and
costs its whole budget for nothing.

This was not visible before layer 3 and will be hit by the first campaign that
runs after it.

## Options

* **Gitignore the masked paths.** `git add -A` skips ignored files, so this
  fixes it in every checkout with one committed file. Costs the meaning of
  three of those names: `.gitmodules` is a real git file, and `.claude/skills/`
  and `.claude/commands/` are plausible things this project adds later — all
  three would then be silently unaddable, which is the same class of quiet
  failure this record is about, moved somewhere less obvious.
* **`.git/info/exclude` per checkout.** Same effect on `git add -A`, without
  committing anything or changing what the repository means. Costs five
  hand-installed files that nothing records, that a new worktree does not get,
  and that the next person will not know exist — the untracked-shared-state
  problem the fleet has already been bitten by four times.
* **Say it in the campaign prompt.** One sentence: stage paths explicitly,
  `git add -A` fails under the sandbox. Costs prompt tokens on every campaign
  and relies on the session reading it, but it is the only option that leaves
  git's own semantics alone and the only one that explains *why* rather than
  hiding the symptom.
* **Narrow what the sandbox masks.** The masks exist to keep a session out of
  the operator's shell configuration, which is a property worth having, and
  `heuristic_jump` has no `.bashrc` of its own — the masked names are the
  sandbox's list, not this project's. Whether that list is configurable was
  not established.

## Decision

**accepted: say it in the campaign prompt**, answered 2026-08-04 and logged as
a `decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

It is the only option that leaves git's own semantics alone, and the only one
that explains *why* rather than hiding the symptom. Gitignoring is the obvious
move and the one worth being careful about: it would make `.gitmodules`,
`.claude/skills/` and `.claude/commands/` silently unaddable if this project
adds them later, which is this same class of quiet failure moved somewhere
less obvious — and this failure's being loud is the reason it was found at all.
`.git/info/exclude` is the untracked-shared-state problem the fleet has already
been bitten by four times. Narrowing the masks was never established to be
possible, and the property they buy — a session that cannot read the operator's
shell configuration — is worth keeping.

### What is left, and what is done

Done in the same commit as this ruling: `harness/trailer-format.md` gains a
**Staging** section naming the error text, since that is what a session will
search for. That file is the single source spliced into every loop prompt as
`{{trailer_format}}`, so one edit reaches both loops and every worker, and it
lands under the heading a session is already reading when it goes to commit.
It is in `harness/`, which is denied to every loop, so this is a human edit
like the sandbox itself.

Still the harness loop's: `design/loops.md` §13's isolation section should say
that layer 3 is on and what it costs, which it does not — the same reconciling
`harness-002` left behind.

## Provisional choice in force

None, and nothing is tagged. The failure is loud rather than silent, which is
why enabling layer 3 was still the right move: a campaign that hits this stops
and says so. What is not acceptable is leaving it undocumented, which is what
this record fixes.

## Consequences

If the answer is the prompt sentence, it belongs beside the commit instructions
and should name the error text, since that is what a session will search for.
If it is gitignoring, the three names above want an explicit note saying they
are masked rather than unwanted, or a later submodule fails in a way nobody
connects to this. Either way `design/loops.md` §13's isolation section should
say that layer 3 is on and what it costs, which it currently does not.
