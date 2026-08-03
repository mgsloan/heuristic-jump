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
| `dashboard/serve` | the operator view, and the place escalations are answered |
| `prompts/` | one prompt per variety of phase. Not one template with a swapped middle |
| `trailer-format.md`, `decision-template.md` | the commit trailer convention and the decision-record shape, spliced into the prompts at launch so there is one copy of each |

Run `harness/hj --help` for the subcommands.

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

## What is not built

The supervisor, the frontier tool, held-out selection, cost accounting,
worktree parallelism, and the tuning and optimisation prompts.
`design/loops.md` section 18 is the argument: they exist to serve tuning
loops, and there are none until phase 2a. With one loop, one bash loop is not
a fleet.

The cost panel is therefore empty and says so, rather than showing a zero.
It fills in when `state/cost/<loop>.jsonl` exists — the join is `ccusage`
against the session ids the harness already records, after the fact, with
nothing inside the model instrumented.

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
