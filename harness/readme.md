# `harness/`

The loop runner, the gate, the auditor, and the tools that turn `state/` into
a record rather than a memory. `design/loops.md` is the design; this file is
how to run it.

**Owned by nobody.** Every loop is denied write access here, and the deny
list is a constant in `harness/hj` rather than a row in the ownership table,
so it cannot be widened by editing configuration. The reason is one line: *a
loop must not be able to weaken the thing that judges it.* Changes here are a
Class B decision, made by a human, and a change that touches measurement is a
metric redefinition — it invalidates comparability across itself and triggers
a recompute sweep (`design/loops.md` sections 10 and 13).

A loop that needs something the harness will not give it files an ordinary
decision record and keeps going on a workaround. **A silent workaround is the
failure this prevents**: a campaign that computes a metric its own way has
forked the measurement and nothing downstream can tell. Five campaigns asking
for the same number means the harness is wrong, not that the campaigns are
demanding.

## Running one

```sh
harness/hj baseline-take              # once, at the start of a phase
harness/loop conformance              # campaigns until stalled or stopped
harness/dashboard/serve               # the operator view, at localhost:8787
harness/hj status                     # the same thing in a terminal
harness/loop conformance --once       # one campaign
```

`state/phase.toml` is the desired state — the phase, and each loop's status.
Setting a loop to anything but `running` stops it at the next campaign
boundary, which is safe because every experiment commits or reverts.

## The pieces

| | |
|---|---|
| `loop` | one campaign per `claude -p` session, with the session id assigned rather than discovered so it is also the campaign id |
| `gate` | `fmt`, `clippy`, `nextest`, diff scope, audit consistency, metrics row — in that order, all mandatory, scoped to the crates the loop owns |
| `audit` | a fresh read-only session judging spec against code, and the merge of its verdict into `state/audit/` |
| `hj` | everything mechanical: section lists, audit merges, scope checks, prompt rendering, campaign records, metrics rows |
| `adapter` | every vendor-specific invocation, in one file. See below |
| `dashboard/serve` | the operator view, and the place escalations are answered |
| `prompts/` | one prompt per variety of phase. Not one template with a swapped middle |
| `trailer-format.md`, `decision-template.md` | the commit trailer convention and the decision-record shape, spliced into the prompts at launch so there is one copy of each |
| `section-baseline.toml` | the denominator, frozen for the phase. See below |
| `corpus`, `corpus-selection.toml`, `corpus-lock.toml` | phase 1b: which repositories the corpus is made of, and rebuilding it. See below |
| `verify-servers`, `server-fixtures/` | phase 1c: every server in `../servers.toml` starts, reports the pinned version, and answers a definition. Here because a loop must not be able to edit the oracle it is scored against |

Run `harness/hj --help` for the subcommands.

## The corpus lives here; the corpus directory is derived

Two files are the corpus:

* **`corpus-selection.toml`** — the phase 1b decision. Seventy repositories,
  five tuning and five held out per language, with the domain each was chosen
  for and the reason every candidate that lost was passed over. Hand-edited.
* **`corpus-lock.toml`** — the pins. One SHA per repository plus what it
  measured when it was pinned. Written by `clone`, never by hand.

`harness/corpus clone` turns those into checkouts and a generated
`manifest.toml` per (split, language) under `../heuristic-jump-corpus/`.

```sh
harness/corpus clone            # rebuild anything missing, at its pin
harness/corpus verify           # every checkout at its pin, every tree clean
harness/corpus status           # what is pinned, what is on disk, what is out of band
```

Four things about it are worth knowing before using it.

**Losing the corpus directory costs bandwidth, not the corpus.** Everything
under it regenerates from the two files above, and a rebuilt checkout is
verifiably the same code because a git SHA is a content hash — which is also
why there are no checksums here. `clone` re-measures a rebuilt tree and refuses
it if the counts disagree with the lock. The one loss no pin protects against
is upstream: a force-push or a deleted repository takes its history with it.
`design/data-collection.md` §1 says the checkout is the artifact and cannot be
reconstructed; that remains true for exactly that case and no longer for the
rest.

**`verify` is the part that outlives phase 1b.** `collect` and `replay` are
required to run it before touching a repository, and the clean-tree half is the
half that matters: a modified or untracked file changes byte offsets and *does
not change `HEAD`*, so a corpus that drifts that way produces truth files that
are wrong with nothing saying so.

**A repository is never bumped.** The pin is written the first time a
repository is cloned; after that `clone` fetches *that commit* rather than
whatever the default branch has moved on to, and refuses a checkout that has
drifted off it. A newer commit is a different corpus and invalidates every
position and truth file that references it. Replacing one means recording the
old entry under `[[considered]]` — or `retired = true` if it was ever really in
the corpus — and adding the new one.

**Out of band is a decision, not a tolerance.** A repository outside
20k–200k lines is reported until either a reserve is promoted or a
`band_exception` with a reason is written into the selection file. The same
applies to what the band does not catch: the first clone found four
repositories that were majority vendored third-party source and one that
shipped a byte-identical copy of itself in `dist/`, none of which the size
criterion would have flagged on its own.

## Two things about the gate

**It is scoped to the crates the loop owns**, per CLAUDE.md's rule against
routinely building the workspace. A Rust tuning iteration builds `lang_rust`
and `measure_rust` and nothing else. The full-workspace gate runs once per
phase gate. Crates in the ownership table that do not exist yet cost nothing —
`hj crates` intersects the table with the actual workspace members — so the
gate is usable from the first commit, when there is no `Cargo.toml` at all.

**Diff scope is the enforcement, not the `PreToolUse` hook.** A deny rule
covers Claude's file tools and not bash subprocesses, so anything the hook
blocks is reachable through `sh -c`. The gate inspects the result instead of
trusting the actor.

## One adapter, because the CLI is the largest dependency here

`deps.md` is careful about every crate; nothing was that careful about the
fact that this harness is built on one vendor's CLI surface. `--session-id`,
headless `-p`, `--output-format stream-json`, `--tools`, the `PreToolUse`
payload shape and the transcript layout are product surfaces. They are not
versioned like a crate, cannot be pinned in a lockfile, cannot be vendored,
and change under you on upgrade — so the mitigation has to be a different
shape: **isolate rather than pin** (`design/loops.md` §17).

`harness/adapter` is the only file that invokes `claude` or knows the shape
of what it emits. `loop`, `audit`, `hooks/` and `dashboard/` call its
subcommands — `campaign`, `audit`, `summary`, `parse-stream`,
`transcript-path`, `hook-path`, `version` — and `parse-stream` normalises the
stream into our own `{kind, text, name, summary, input, diff}` records, so
nothing downstream reads a vendor record. Replacing the runner means
rewriting one file. `$HJ_RUNNER` swaps the binary.

**Each campaign records the CLI version** beside the prompt sha, for the same
reason: a prompt revision changes the generator of campaigns and nothing
downstream can detect it, and a CLI upgrade does the same thing without
anybody deciding to change anything.

## The denominator is frozen, not re-derived

`sections clean / sections total` is parsed from headings in documents the
conformance loop is allowed to edit — so re-deriving it each audit would let
the loop merge two dirty sections into one clean one and move its own score
without touching a line of code (`design/loops.md` §7).

`harness/hj baseline-take` freezes the section list for the phase into
`harness/section-baseline.toml`, which lives here because everything a loop
must not move lives here. After that: a baseline section that no longer
exists is still counted and is never clean; a section added since is audited,
because it is real work, but does not move the score this phase. Both show up
in `hj baseline-drift` and on the dashboard.

## Class A spec edits are provisional too

Rewriting the spec toward the code is the one gaming route on §7's list that
the audit cannot catch **by construction** — the auditor compares the code
against the spec as it currently reads, so moving the spec deletes the gap
from the instrument that would have reported it, and the section goes clean.

Two mechanisms, both in the dashboard's top panel. Every `state/spec-changelog.md`
entry waits there until a human marks it read, which appends to the
intervention log. And `hj campaign-close` flags any campaign whose commits
include both a `design/` edit and code — scoped to the campaign rather than
the commit, because the iteration contract produces one commit per experiment
and "same commit" is a test the loop's own working style defeats without
anyone intending it.

Neither prevents anything. This is the failure with the thinnest defence in
the whole design, and the honest version is that it is made *visible* rather
than impossible.

## The auditor never writes

It runs with `--tools "Read,Grep,Glob"` and answers in prose plus one fenced
`toml` block. `harness/audit` validates that block — every section key must
exist in the document, `gaps` must carry a gap, `clean` must not — and writes
`state/audit/` itself. A hallucinated anchor is rejected rather than recorded,
because it would otherwise become a permanent work item nobody can close.

The section list is parsed out of the design documents' `##` headings, so it
follows the documents rather than drifting from them, and coverage comes from
rotation: the sections the last campaign touched, plus the least recently
audited slice of the rest.

## The dashboard answers, it does not only display

`harness/dashboard/serve` is a local server over a page regenerated per
request. Five panels, in descending order of how often they should change
what you do: decisions waiting, loop status, metrics, cost, sessions — plus
the intervention log.

The part that earns it is that **you answer from the page**. A read-only
dashboard means every decision costs a context switch into an editor, which
is exactly the friction that leaves loops idling on provisional choices. A
`POST` writes the ruling into `state/decisions/<id>.md` — appending the
answer and flipping the status, never editing the record's text, which is
MADR's discipline — and appends to `state/interventions.jsonl` in the same
action. The log is the mechanism, not a record of it, so an answer given
through the page is logged by construction. **The rationale field is
required**, because a decision with no recorded reason is one you
re-litigate in three weeks and one the loop cannot use.

Two smaller things it does on purpose. It swaps the panels in place rather
than reloading, so the scroll position survives an update you were reading
through — and it skips the swap entirely while a rationale is half-written.
And the transcript view renders the teed stream: tool calls collapsed to
their command or path, diffs as diffs, gate verdicts and metric rows pulled
out of the fold, large results truncated with the raw JSONL one link away.

Nothing it produces is committed; it is all derived from state that is.

## Cost, and the three budget scopes

One row per campaign in `state/cost/<loop>.jsonl`, joined on the session id
after the fact — the harness recorded which loop and target it launched and
the teed stream carries the numbers, so nothing inside the model is
instrumented. `hj cost <loop>` writes the rows the loop runner has not
written yet; `hj cost <loop> --refresh` re-measures rows that predate a field
the row spec has since grown, which is how a backfill happens.

Two of the row's fields are measured off tool calls rather than off the
session's `result` event, and both live in `harness/adapter` because a tool
call is a vendor shape. **Gate seconds** is the `tool_use` → `tool_result`
timestamp delta, there being no duration field on either. The **experiment
mix** — committed / reverted / empty — takes one gate run as one experiment,
since section 4's contract makes the gate the boundary: `committed` is the
campaign's commits from git, `reverted` is revert commands seen in the
stream, and `empty` is the residual. A campaign that is mostly empty is
thrashing regardless of what it spent, which is the signal the mix is for.

`hj estimates` puts section 15's guess table beside what has actually been
spent, per phase, in the three resources the section keeps apart — tokens,
model wall-clock, machine wall-clock. The table is read out of the document
rather than copied here, so it cannot drift; the comparison is left to a
human because the guesses read "low–moderate" and "days". The section's own
argument is why the command exists: an estimate that is never compared
against an actual is decoration, and calibration is the first ten campaigns
of each loop.

Budgets are three scopes, per `design/loops.md` section 15, and each stops
and reports rather than continuing quietly:

```toml
[budget]
global_usd = 900.0        # the backstop, across every phase and loop

[budget.language]
rust = 120.0              # this phase, this language, independent of the rest
```

Both are optional and both default to absent. The per-campaign ceiling is
`budget_usd` on the loop's own table, and it is the runner's to enforce
(`--max-budget-usd`) because only the runner can stop a session mid-turn; a
campaign it stops closes with outcome `budget`. The outer two are the
harness's: `hj budget [<loop>]` reports spend against each and exits 1 when
one is reached, and `harness/loop` consults it before opening a campaign, so
a stop lands between campaigns where the tree is committed.

## What is not built

The supervisor, the frontier tool, held-out selection, worktree parallelism,
and the tuning and optimisation prompts. `design/loops.md` section 18 is the
argument: they exist to serve tuning loops, and there are none until phase
2a. With one loop, one bash loop is not a fleet.

`design/loops.md` section 18 also says who builds them — this same
conformance loop, pointed at `loops.md`, during the ~100 machine-hours of
phase 1.5 when it has nothing else to do. At that point `harness/gate*`,
`harness/prompts/` and the auditor stay denied to it (they judge it *now*)
while `harness/supervisor/` and `harness/dashboard/` become writable (they
judge phase 2a, later).

## Requirements

`python3` 3.11+ (for `tomllib`), `git`, `claude`, and — once the first crate
exists — `cargo` and `cargo-nextest`. The gate names the install command when
nextest is missing rather than silently falling back to `cargo test`, whose
output the test-count ratchet cannot read.
