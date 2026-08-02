# Implementation loop design

Initial ideas for building this project with autonomous Claude Code
loops rather than by hand — a "Ralph Wiggum" loop per work area, each
running a fixed prompt against a durable on-disk state, iterating until
it either matches the spec or stops making progress.

This is a design sketch, not a commitment. Nothing here has been tried
on this repository, and the parts most likely to be wrong are called
out in [section 19](#19-how-this-goes-wrong) and the open questions.

## 0. The shape of the idea

A Ralph loop is: one fixed prompt, run repeatedly in a fresh context,
where the *filesystem is the memory*. The model does not remember the
previous iteration; it reads the repository, picks the highest-value
open item, does it, commits, and exits. Continuity lives in tracked
files, not in a conversation.

That constraint is what makes the loop work, and it dictates most of
this document. Everything the loop needs to know between iterations has
to be written down: what is done, what is left, what was decided, what
was tried and failed, and what the numbers were last time. A loop whose
state lives only in context is a loop that redoes its own work.

**This design is not pure Ralph, and the deviation is deliberate.** A
fresh context per *iteration* is right when iterations are independent —
one spec item, one implementation, no relationship to the next. Tuning
is not like that: it is hypothesis-driven refinement on the same few
hundred lines, where the most valuable thing in context is the four
variants that were just tried and what each did to the number. Serialising
that to a journal and re-reading it next iteration is both expensive and
lossy, and the loss is concentrated in exactly the part — *why* a variant
failed — that is hardest to write down and most costly to rediscover.

So the unit of fresh context is the **campaign**, not the iteration
([section 4](#campaigns-are-the-unit-of-fresh-context)). Ralph's real
benefits — bounded context growth, and a hard break on a theory the
model has talked itself into — are preserved by resetting at campaign
boundaries rather than at every step.

Three things make this project unusually suited to it:

* **The oracle is mechanical for the part that matters.** Resolution
  quality is a number produced by the corpus scan, per stratum. A loop
  tuning a language handler is not grading its own homework; it is
  reading a scoreboard it cannot see the answers to.
* **The spec is already written and it is enormous.** ~6000 lines
  across four documents, largely decided, with the undecided parts
  already enumerated as numbered open questions. That is a work queue
  that has mostly been built already.
* **The seam is narrow and stated.** `LanguageHandler` plus the
  vocabulary types is the entire contract between the driver and every
  language. Parallel loops need a boundary they cannot accidentally
  cross; this project has one on purpose already.

The part that is *not* suited to it: the driver's correctness
properties — the prime invariant, byte-identical forwarding, the
double-response assertion — have no gradient. They are pass/fail, and
a loop cannot hill-climb toward them. That is a different loop with a
different oracle, which is [section 2](#2-two-loops-two-oracles).

## 1. Current state, and what it forces

There is no code. There are four design documents, a `clippy.toml`, and
a `CLAUDE.md`. Every loop described here is blocked on a bootstrap that
is not itself loopable in any interesting sense, because there is
nothing to measure yet.

More consequentially: there is no shared resolution crate, and per
`resolution-design.md` §9 there must not be one until working handlers
exist to extract it from. Carried to its conclusion — see
[section 13](#13-shared-code-and-when-it-may-exist) — that removes the
shared-library coordination problem from phase 2 entirely, rather than
solving it. The only shared code during tuning is the seam and a frozen
`similarity` crate ported from the prior implementation.

## 2. Two loops, two oracles

|  | **Conformance loop** | **Metric loop** |
|---|---|---|
| Scope | `vendor/`, `shared`, `driver`, `heuristic_jump`, `measure_*` | one `lang_*` crate |
| Oracle | spec ledger + test suite + adversarial verifier | corpus numbers per stratum |
| Progress | ledger items reaching `verified` | movement on the frontier ([section 10](#10-objectives-phases-and-the-frontier)) |
| Done | all P0 items verified, verifier finds nothing for K rounds | frontier stops advancing, or budget exhausted |
| Failure mode | spec drift; the loop edits the spec to match the code | overfitting to the tuning corpus |
| Concurrency | one writer | parallel, one per (language, server), in phase 2a ([section 13](#parallel-loops-and-what-they-share)) |

Conflating these is the first mistake available. A conformance loop
with no number to chase will invent one; a metric loop with a spec
checklist will spend its iterations on checklist bookkeeping instead of
on the thing that moves the number.

## 3. The spec ledger

6000 lines of prose is not a work queue. The first loop task — and the
one artifact worth reviewing by hand before anything else runs — is
extracting the design documents into a machine-readable ledger.

`spec/ledger.toml`, one entry per checkable claim:

```toml
[[item]]
id      = "core-3.2"
title   = "Swallow decision belongs to writer:editor"
doc     = "core-implementation-design.md#32-the-swallow-decision-belongs-to-writereditor"
owner   = "driver"
phase   = 1
priority = "p0"
verify  = "test:double_response_assertion"
status  = "todo"     # todo | implemented | verified | deferred | blocked
```

Rules that make it load-bearing rather than decorative:

* **`status` moves forward only.** `verified` requires `verify` to name
  a test that exists and passes. A loop cannot mark something verified
  by asserting that it is.
* **`verify` is a test name, a metric threshold, or the literal
  `manual`.** `manual` items are the ones a human has to look at, and
  their count is a number the loop reports rather than reduces.
* **`deferred` requires a reason and a decision id.** Deferral is a
  legitimate outcome — much of these documents defers deliberately —
  but an undocumented deferral is indistinguishable from giving up.
* **Extraction is reviewed once, by hand.** Everything downstream is
  driven from this file. An hour spent reading it is the highest
  leverage hour in the plan; a ledger that omits the prime invariant
  produces a loop that never implements it and reports success.

Worth stealing for the extraction pass: **EARS** (Easy Approach to
Requirements Syntax — five sentence patterns, from Rolls-Royce
requirements practice). Its whole purpose is turning prose into claims
that are individually testable, which is exactly the transformation
being asked for here. It is a notation to write ledger titles in, not a
framework to adopt.

The ledger is also how the loop bounds its own context. Each item
carries a document anchor, so an iteration reads the one section it
needs rather than 138KB of core design.

## 4. The iteration contract

One iteration is:

1. Read `spec/ledger.toml`, `state/decisions/`, `state/journal.md`, and
   the last N commit messages.
2. Pick exactly one item — highest priority, lowest phase, unblocked.
3. Do it. Read only the spec sections the item names.
4. Run the gate (below). If the gate fails and cannot be fixed within
   the iteration, `git revert` to green and write what was learned into
   the journal.
5. Commit with structured trailers.
6. Exit.

**The gate**, run in this order, all mandatory:

* `cargo fmt --check`
* `cargo clippy -p <owned crates> --all-targets -- -D warnings`
* `cargo nextest run -p <owned crates>`
* **diff scope**: the commit touches only paths this loop owns
  ([section 13](#mechanics-isolation-in-four-layers)). This is the enforcement,
  not the hook.
* ratchets, in phase 3 only ([section 11](#11-size-and-loc-as-objectives))
* ledger consistency: every `verified` item's test exists and ran
* metrics row appended ([section 10](#10-objectives-phases-and-the-frontier))

**The gate is scoped to the crates the loop owns**, per `CLAUDE.md`'s rule
against routinely building the workspace. A Rust tuning iteration builds
`lang_rust` and `measure_rust` and nothing else — no other grammar, no
driver. Core §11's split of `scan` into `measure_core` plus a four-line
`measure_<lang>` exists to make that possible: without it, measuring one
language means compiling all of them, and the confinement is
decorative. The full-workspace gate runs once per phase gate, not once
per iteration.

**Green-or-revert is not negotiable.** A broken tree costs the *next*
iteration its whole context budget on diagnosis, and the next iteration
does not know that the breakage was deliberate. The cheapest possible
handoff between two amnesiac sessions is a repository that builds.

Commit messages carry machine-readable trailers, parseable with stock
`git interpret-trailers`, so `git log` is the journal and stall
detection needs no separate bookkeeping:

```
[core-3.2] route swallow decision through writer:editor

ledger: core-3.2 todo -> verified
tests:  +1 double_response_assertion
loc:    driver +38
binary: +412B
decision: none
```

`state/journal/<owner>.md` holds what the trailers cannot: approaches
tried and abandoned, and why. This is the single most valuable file for
preventing the loop from rediscovering the same dead end every third
campaign.

### Campaigns are the unit of fresh context

A session spans a **campaign**: one target, one hypothesis, however many
experiments it takes to confirm or kill it. Context persists across the
experiments inside a campaign and is discarded at its boundary.

A campaign:

1. **Opens** by picking a target and writing it down in
   `state/campaigns/<owner>/<id>.md` — for a tuning loop, the
   (stratum, language, server) with the largest share × gap, plus the
   hypothesis about why it is losing coverage; for a conformance loop, a
   ledger item or a cluster of them from one document section. The
   session id and its resume command are recorded here at open
   ([section 16](#16-the-operator-view)).
2. **Runs experiments.** Each is the iteration contract above: change,
   gate, commit or revert. Reverts are normal and informative inside a
   campaign — a falsified variant is a result.
3. **Closes** when the hypothesis is confirmed and committed, or
   falsified, or N experiments produce no movement, or the campaign's
   token ceiling is hit.
4. **Writes up** on close: the journal entry, the campaign record's
   outcome, and — if the hypothesis died — why, in enough detail that
   the next campaign does not retry it.

The write-up happening at *close* rather than per iteration is not a
detail. A journal entry written after each experiment is written at the
moment of least understanding; one written at the end of a campaign is
written when the shape of the result is actually known.

Which unit each loop uses:

| Loop | Campaign is |
|---|---|
| conformance (1a, 2b) | one ledger item, or a cluster from one doc section |
| tuning (2a) | one hypothesis about one stratum |
| phase 3 | one refactor target |

The conformance loops sit close to pure Ralph, because ledger items
genuinely are independent and the spec is the memory. Tuning sits
furthest from it, for the reasons in [section 0](#0-the-shape-of-the-idea).

**What the reset still buys**, and why campaigns are bounded rather than
open-ended: context growth is capped at one campaign rather than one
phase, and a wrong theory cannot survive past the campaign that produced
it. An unbounded tuning session is exactly the failure Ralph exists to
prevent — thirty iterations spent on a premise formed in the first
three, with no mechanism that questions it.

## 5. The verifier

"Loops until it decides it matches the spec" has an obvious weak point:
the entity deciding is the entity that wrote the code, in the same
context, having already convinced itself.

So the decision is taken away from it. Every iteration — or every K
iterations, if cost matters — a **separate session with no memory of
writing the code** is given the spec section and the implementation and
asked one question: *where does this diverge?* It cannot edit anything.
Its findings become new ledger items.

Two properties worth keeping:

* **Adversarial framing.** "Find the divergence" produces findings;
  "check whether this is correct" produces reassurance.
* **Findings are items, not instructions.** They enter the queue and
  get prioritised like anything else. A verifier that directly drives
  the next iteration turns into a second, unaccountable planner.

**A fresh session of the same model is enough to start.** The failure
being caught is attention-based — the writer talked itself into an
interpretation and then read the code through it — not capability-based,
and a fresh context already destroys the shared premise. A second model
adds different blind spots rather than more capability, and is worth
adding only if the same-model verifier is observed rubber-stamping.
That is a cheap experiment when the time comes: the same prompt, a
different CLI.

The transparency golden tests, the double-response assertion, and the
zero-inspection assertion (core §15) are the parts of the spec where
the verifier matters least, because they are already mechanical. It
matters most on the prose-shaped claims — "handlers get a snapshot, not
a lock", "the driver must not depend on any language crate" — several
of which are also mechanically checkable if someone writes the check.
Converting verifier findings into permanent mechanical checks is itself
a high-value ledger item, and the loop should be told so.

## 6. Spec changes: what the loop may decide alone

The loop will find the spec wrong. It is 6000 lines written before a
line of code, so this is certain, and a loop that must escalate every
inconsistency will escalate constantly and stall.

**Class A — the loop fixes it, records it, continues.** Internal
contradiction; a section reference that does not resolve; a type name
that changed; a claim about a dependency's API that is factually false;
an example that does not compile. The test is: *is there a defensible
answer that does not trade anything off?* Fix, and append to
`spec/changelog.md` with the contradiction quoted and the resolution
stated. Class A edits are reviewed in batch, after the fact.

**Class B — escalate, and keep going anyway.** Anything that trades
something off. Specifically, always escalate when the change touches:

* a metric target or budget (the latency numbers, the 97% floor, the
  error severity budgets)
* the `LanguageHandler` seam or any vocabulary type
* the dependency set, or anything in `dependency-plan.md` §13
* licensing, or `vendor/`
* one of the numbered open questions in any document

The escalation is a file, `state/decisions/NNN.md`, in **MADR** format
(context / options / decision / consequences — the usual architecture
decision record template, which already has the immutability discipline
this wants: a record's status changes, its text does not). It states
the question, the options, the evidence available, a recommendation,
and — critically — **what the loop is doing meanwhile.** The loop picks
the most reversible option, tags every affected site with
`// DECISION-017: provisional`, and continues. It never idles waiting
for an answer.

When the answer arrives, reconciling the tagged sites is a normal
ledger item. `grep -r DECISION-` is the outstanding-provisional-choice
report, and its count is a health metric: rising steadily means the
loop is running ahead of its decisions and the work is getting
speculative.

**Seed the queue from what already exists.** The readme, the resolution
design, and the vendored-rope design each end in numbered open
questions, and those are already Class B items in everything but format.
Converting them to decision files before the first loop runs means the
loop starts with its uncertainties enumerated rather than discovering
them one stall at a time — and several have recommendations attached
already, which the loop can adopt as its provisional choice with no
further reasoning.

Enumerate them by reading the documents, not from a count recorded here.
The lists grow, questions get resolved in place, and a number written
down in a second document is wrong shortly afterwards without anything
failing to notice. Some are already marked resolved with their decision;
those are seeded as settled rather than skipped, since the reasoning is
what stops them being reopened.

## 7. Progress, stall, and the ways it is faked

**Progress** is any of: a ledger status advancing; a test count
increasing; a frontier point being added that is not dominated by an
existing one; a decision item being resolved. All four are computable
from the repository, which is the point — the loop does not get to
assess its own progress in prose.

**Stall is judged per campaign, not per experiment.** An experiment that
reverts is a result — a falsified variant is what a campaign is for — so
the per-iteration rule that made sense under pure Ralph would fire
constantly here. What counts is a *campaign* closing with none of the
four forms of progress, and N of those in a row (start with 3).

**An experiment that produces no commit at all is still a tick**, and it
is the one per-iteration signal worth keeping. Not a neutral event, not
"it thought about it and found nothing to do": the official
`ralph-wiggum` plugin shipped a bug that produced 636 consecutive empty
iterations, and the only thing standing between a loop and that outcome
is treating emptiness as a signal rather than as rest. Three empty
experiments end the campaign early. On stall the
loop stops and writes `state/handoff.md`: what it was trying, what it
tried, what it believes is blocking, and the specific question that
would unblock it. Then it notifies and exits.

Two further heuristics worth taking from `frankbria/ralph-claude-code`,
which ships them as tuned constants: **flag when ≥30% of recent
iterations are test-only changes**, and **cap consecutive "done"
signals** so a loop that has decided it is finished cannot keep saying
so. The first is the better one — a run that is mostly test churn is
the characteristic shape of a loop that has run out of real work but
not out of budget.

The failure this is guarding against is not idleness — it is the loop
generating plausible activity indefinitely. Which brings up the ways
the metrics can be satisfied without work being done:

| Gaming route | Countermeasure |
|---|---|
| Mark ledger items `verified` without tests | `verify` must name a test that exists and passed in the gate run |
| Delete or weaken tests | Test count is a ratchet; test *deletions* are flagged for review regardless of count |
| Rewrite the gate script | `harness/` is owned by nobody and denied to every loop; changes to it are Class B |
| Rewrite the spec to match the code | Class A/B split, plus `spec/changelog.md` review; any spec edit in the same commit as code touching the same item is flagged |
| Tune to the corpus | Held-out repos are in a corpus root the loop is never given ([section 12](#12-held-out-integrity)) |
| `cargo insta accept` a metric regression | The gate checks metric *direction* itself; insta pins the table's shape, not its values |
| Split one item into ten to show motion | Ledger additions by the loop are marked `origin = "loop"` and reported separately from the reviewed baseline |

None of these is airtight against a determined optimiser. They are
airtight enough against an *undirected* one, which is the actual risk:
the loop is not adversarial, it is just weakly grounded, and weak
grounding drifts toward whatever is easiest to satisfy.

## 8. Sequencing and gates

The phase structure is [`implementation-phases.md`](implementation-phases.md).
It is short and it is the authority; this section says what each phase
means for the loops and what its gate is.

**Phase 1a — core needed for measurement.** Only the parts of core that
`measure_core` and `measure_<lang>` require: `vendor/rope`, `sum_tree`,
cut-down `util`, the newtype work in `vendored-rope-design.md`, `shared`
(seam, vocabulary, `ProjectView`, the client-side subset of `proto`), the
framing codec, and `measure_core` itself. Explicitly **not** the router,
the health model, the actor, dispatch, standalone, or divergence
reporting.
Gate: workspace builds, upstream rope tests pass unchanged,
position-encoding property tests pass, `measure_core` drives a real
server end to end on one repository.
*Hand-driven or heavily supervised.* The seam is decided here, and
getting it wrong is expensive downstream in a way no loop will notice.

**Phase 1b — repo collection**, concurrently. Needs no code at all, so
it starts on day one. C, C++, Go, JavaScript, TypeScript/TSX, Rust,
Python; medium-sized, popular, trustworthy, spread across domains and
styles.
Gate: repositories checked out at pinned commits, **and the tune /
select / final split decided and physically separated**
([section 12](#12-held-out-integrity)). The split has to be made here,
not later: once a repository has been in the tuning corpus, moving it to
held-out does not un-teach it.

**Phase 1.5 — ground truth collection.** Every language server on every
repository. Depends on 1a (for `measure collect`) and 1b (for the
repositories), which is what makes it a distinct phase rather than a
task inside either.
Gate: a `truth.jsonl` per (repository, server) with a valid provenance
header, and `measure replay` reproducing the recorded positions.

This is the plan's long pole and its highest-uncertainty item — roughly
a hundred machine-hours, seven languages, more servers than languages,
and no useful fallback if the data turns out unusable. It costs almost
no tokens and almost no model time, which is exactly why it is easy to
under-scope: it is invisible to every other kind of accounting
([section 15](#15-cost-and-timing)).

**Phase 2a — per-language quality loops**, one per (language, server),
in parallel. Precision and recall only; cost metrics recorded, never
gated.
Gate: the frontier stops advancing, candidates are re-measured on the
held-out corpus, and **a human picks the point**
([section 10](#selecting-a-version-at-a-phase-gate)).

**Phase 2b — the LSP shim**, concurrently, on `master`. Everything phase
1a deferred: sections 1–15 of the core design.
Gate: transparency golden tests, server-originated round-trips, protocol
race tests, double-response assertion, codec fuzz.
*Conformance loop.* The largest single body of work, and it has no
gradient, so it lives or dies on the ledger being good.

**Phase 3 — whole-repository optimisation.** Latency, binary size, line
count, cross-language, serial, single writer. This is where extraction
of shared resolution code finally happens
([section 13](#13-shared-code-and-when-it-may-exist)), and it runs under
an equality constraint
([section 10](#phase-3-is-a-refactor-under-an-exact-oracle)).

### Why 2a and 2b in parallel is safe

They are disjoint crates, and a language loop needs no part of the
driver to measure itself — that is what the `measure_core` split buys.
The one coupling is `shared`, which 2b churns and 2a depends on.

That is handled by ownership at file granularity inside the crate: **the
seam is frozen at the end of phase 1a** — `LanguageHandler`, `Query`,
`Outcome`, `ProjectView`, the vocabulary newtypes — while `proto` and
the driver-only `Error` variants stay owned by the conformance loop and
churn freely. Language loops depend only on the frozen half. A seam
change during phase 2 is a Class B escalation, which is the correct
price given it would interrupt every language loop at once.

The residual risk is that the seam was designed in phase 1a with only
one consumer built, and 2b discovers it needs something else. Core §12
specifies both sides already, so this is a review-discipline problem at
the 1a gate rather than an ordering problem — but it is the thing to be
most careful about there.

## 9. The inner loop must be fast

A metric loop whose iteration takes four hours is not a loop. The
corpus scan drives a real language server over ten repositories; that
is hours, and it must happen approximately never.

So ground truth is a **frozen artifact**. `measure collect` runs once per
(repo commit, server version) and writes `truth.jsonl`: every
identifier position, the LSP's answer, the LSP's latency. Tuning
iterations run `measure replay`, which launches no language server at all
— it replays handlers against the frozen positions and compares to the
frozen answers. Core §11 specifies both modes.

Target: full replay over one language's tuning corpus in under a
minute. If it is slower than the model's thinking time, the loop is I/O
bound on its own feedback and iteration count collapses.

That target turns out to be load-bearing for a second reason, which
[section 10](#10-objectives-phases-and-the-frontier) needs: a fast
deterministic replay means the *entire metric history is
recomputable*. Any past commit can be re-measured on demand, so a
change to how a metric is defined does not silently invalidate
everything recorded before it — it triggers a sweep. Without that, the
first metric redefinition throws away the frontier.

Corollary: `truth.jsonl` is versioned and pinned, and regenerated
rather than edited. A metric comparison across two corpus versions is
meaningless, and a partially refreshed corpus is the worst case,
because it looks like a regression.

### Determinism is a precondition, not a description

Everything above — the recomputable history, the frontier, and the
argument in [section 17](#17-tooling-adopt-steal-reject) for rejecting
statistical regression detection — rests on replay producing the same
numbers every time. That is not automatic. A wall-clock deadline makes
the handler abstain more on a loaded machine, which moves *coverage*,
not merely latency.

With loops running in parallel, the machine is always loaded, and by a
varying amount. So this stops being a subtlety and becomes the thing
that decides whether parallel tuning works at all: under wall-clock
deadlines, five concurrent loops would each be measuring a number that
depends on what the other four were doing.

Core §11 now requires replay to enforce budgets deterministically —
bytes read and files parsed, not elapsed time. Given that, quality-phase
iterations need no coordination with each other whatsoever, which is
what makes [section 13](#parallel-loops-and-what-they-share) cheap.

The metrics that *cannot* be made deterministic or local are handled
separately, in [section 10](#what-cannot-be-measured-in-isolation).

## 10. Objectives, phases, and the frontier

The loop needs to know what it is maximising, and the honest answer
changes over the life of the project. Trying to express it as one
weighted score from the start requires an exchange rate between a
coverage point and a kilobyte that nobody can currently justify.

So there are **two objective regimes**, and they are phases 2a and 3:

* **Phase 2a, quality.** Precision and recall, and nothing else. Cost
  metrics are *recorded* and never *gated*. Per (language, server),
  parallel.
* **Phase 3, cost.** Latency, binary size, line count. Cross-language,
  serial, single writer, and — the part that makes it tractable —
  **output-preserving**.

Splitting them this way is the loop-level expression of a rule the
project already has: `CLAUDE.md` says implement the slow simple version
first and only optimise once the idea is validated. A size ratchet
running during exploration suppresses exactly the expensive experiments
that establish what is achievable — the brute-force scan that shows
what coverage is *available* before anything is spent making it cheap.

### Phase 3 is a refactor under an exact oracle

The rule from [`implementation-phases.md`](implementation-phases.md):
**deterministic responses do not change at all, and an optimisation that
requires changing them is escalated for human review rather than
taken.**

This is a much stronger constraint than "trade along the frontier," and
it is stronger in the direction that matters for an autonomous loop.
Trading requires judgment about whether a particular exchange of
coverage for bytes is worth it, exercised unsupervised, hundreds of
times, with no feedback until a gate. An equality constraint requires no
judgment at all: replay the corpus before and after, compare outcomes
byte for byte, and the answer is yes or no.

It is implementable precisely because of two earlier decisions.
`resolution-design.md` §11 requires the handler to be deterministic, and
[section 9](#determinism-is-a-precondition-not-a-description) requires
replay to enforce budgets by work rather than wall clock. Without the
second, "the outputs did not change" would be a statistical claim about
a machine's load; with it, it is an equality check.

**The one legitimate difference is truncation.** Reducing per-query work
is the main latency optimisation, and a query that previously exhausted
its budget and now completes will produce a different — better — answer.
So the gate is:

* any difference on a **non-truncated** query fails the gate, full stop;
* differences confined to **truncated** queries are surfaced in the gate
  report for approval rather than blocking.

The record already carries a `truncated` flag
(`resolution-design.md` §11), so this is mechanical rather than a
judgment call about which differences are the good kind.

**What this does to the frontier.** It mostly retires it. If quality
cannot move, phase 3 has one axis — size — and it is a plain
minimisation with a ratchet, not a trade-off surface. The frontier
returns only for the changes you approve by escalation, where it does
its actual job: showing what a proposed exchange costs, at the point
where a human is deciding.

That also means the exchange rate never has to be guessed. Phase 2a's
frontier says what quality was achievable and at what cost; phase 3
proposes a specific trade; you look at both. Nobody has to write down
what a coverage point is worth in kilobytes in the abstract.

### The metrics history

Every iteration appends one row to `state/metrics/<language>.jsonl`, in
both regimes:

```
commit, phase, per-stratum {coverage, precision, n},
work counters (bytes read, files parsed, nodes visited),
measure_<lang> stripped size, lang_<lang> crate contribution,
LOC per crate, test count
```

Append-only, in git, one row per commit, **one file per owner** so that
concurrent loops never write the same file. It is a *cache*, not a
source of truth: replay is deterministic, so any row can be recomputed
from its commit. That is what makes a metric redefinition survivable —
the history is swept and rebuilt rather than lost.

### What cannot be measured in isolation

Two metrics resist the per-iteration row, for different reasons, and
both are handled the same way: **measured at phase gates, not at
iterations.**

* **The shipped binary's size** needs every language to build. The
  authoritative number is the per-language link delta against
  `heuristic_jump`, which means building it with and without each
  language. A quality-phase iteration cannot produce it even in
  principle — that is the coupling the isolation rules exist to remove.
  A close proxy *can* be produced in isolation, though, which is the
  next subsection.
* **Latency needs a quiet machine.** With loops running in parallel,
  every iteration happens on a machine under varying load from the
  other loops, so a wall-clock measurement taken during one is
  meaningless. Serialising the loops to fix this would give up the
  parallelism to measure a metric that
  [section 10](#the-frontier-and-why-it-stays-two-dimensional) treats as
  a constraint rather than an objective — a bad trade.

At a gate, the loops are already quiesced and the frontier candidates
are few. Building a stripped release binary and running a timing pass
for a handful of commits is cheap there and prohibitive per iteration.

**Work counters cover the latency gap in between.** Bytes read, files
parsed, nodes visited — deterministic, machine-independent, local, and
strongly correlated with the thing that cannot be measured. They go in
every row. If a cost-phase iteration triples the bytes read per query,
that shows up immediately rather than at the next gate, and the
wall-clock run at the gate confirms it. They are also what the replay
deadline is enforced against
([section 9](#determinism-is-a-precondition-not-a-description)), so they
are already being computed.

### `measure_<lang>` size covers the size gap

The size proxy is the stripped release size of the loop's own
`measure_<lang>` binary. It is built in isolation, by construction —
that is what the crate split in core §11 is for — so it goes in every
row.

As an *absolute* number it is not the language's bill: it carries a
large constant that never ships (`measure_core`'s LSP client, its JSON
handling, its CLI) alongside `shared`, `rope`, and the tree-sitter
runtime. But **a ratchet only reads deltas**, and across iterations the
constant is constant, so the movement is exactly the handler's own —
which is the number a tuning loop needs and the link delta cannot give
it per iteration.

Three things to get right about it:

* **The constant moves when the conformance loop changes `shared` or
  `measure_core`.** A language loop would see its size metric jump for
  a reason outside its control and outside its diff. The harness knows
  when those crates last changed, so the ratchet re-baselines at those
  commits rather than blaming whoever iterated next.
* **The grammar dominates and must be reported separately.** Hundreds
  of KB of generated parser sits in that binary, so a 2% move in the
  total can be a 30% move in handler code, and the signal drowns.
  `cargo bloat --crates` on `measure_<lang>` attributes per crate, which
  gives `lang_<lang>`'s own contribution directly — approximate, since
  symbol attribution blurs across inlining and generics, but the right
  granularity. Both numbers go in the row: whole-binary size is exact
  and coarse, crate contribution is attributable and fuzzy.
* **It is a proxy, not the metric.** The link delta against the shipped
  binary stays authoritative and stays at gates. The two can disagree
  when a handler pulls in something the driver already links, or
  something `measure_core` already links — and the gate is what settles
  it.

This also restores a property the phase split needs. The cost phase can
only select an earlier, cheaper point from the trajectory if the
trajectory recorded cost, and with binary size deferred entirely to
gates it did not — the quality-phase record would have been work
counters and nothing else. Now every iteration carries a size number,
so [the incompressible-peak failure](#19-how-this-goes-wrong) has a
curve to select from rather than a handful of gate samples.

### The frontier, and why it stays two-dimensional

A commit is on the frontier if no other commit beats it on every axis.
Over a run of a few hundred iterations that is a real, computable
object, and it is strictly more informative than the current numbers:
it is the record of what was *achievable*, including by versions that
were later abandoned.

The discipline that makes it useful is **keeping it to two axes**. With
four or more objectives, non-domination becomes nearly free — almost
every point is on the frontier and the concept stops selecting anything.

**The frontier is a phase 2a object: precision × recall.** The classic
pair, and the right one — the readme is explicit that coverage alone can
be improved by guessing more, so recall without precision on the other
axis is a metric with a trivial exploit.

Phase 3 has no frontier of its own, because it may not move quality at
all ([above](#phase-3-is-a-refactor-under-an-exact-oracle)). It has one
axis, size, and a ratchet. The frontier reappears there only inside an
escalation, to show what a proposed exception would cost.

Two things stay off the axes deliberately:

* **Latency is a constraint, not an objective.** The design already
  converts it: past the hard cap a query abstains, so blown latency
  *spends itself as lost recall* and is already visible on the quality
  axis. Putting it on a third axis double-counts it. Gate it against
  the readme's budgets, report the percentiles, and leave it there.
* **LOC is not on the frontier.** It correlates with binary size, it is
  gameable by formatting, and what it is really a proxy for —
  maintainability — is not something a frontier can see. Report it;
  let review use it.

### Multiple servers do not multiply the frontier

A language with two usable servers has two oracles and two metric
tables (core §11), which threatens to make the frontier
four-dimensional and therefore useless. It does not, because the two
surfaces being optimised are different code:

* **Shared handler logic** is evaluated on the positions where *every*
  server for that language agrees. One 2D frontier per language, and
  the axes mean the same thing regardless of deployment.
* **A `ServerProfile`** is evaluated on the positions where servers
  differ, against that server alone. One 2D frontier per profile, and
  changing one profile provably cannot move another server's numbers,
  because no other server's queries are in its evaluation set.

So the count of frontiers grows with servers but their dimensionality
does not, and the pieces stay independent. That independence is the
reason for the decomposition — a joint objective over all servers would
have to weight them by expected deployment share, which is a number
nobody has.

It also assigns each surface the data that actually determines it.
Tuning shared logic against a single server's full corpus would bake
that server's conventions into code that runs behind all of them; the
agreement subset is exactly the part where "correct" is not a matter of
opinion.

For a language with one usable server — Rust, Go — every position is
trivially unanimous, there is no profile, and none of this machinery
does anything. That is the intended behaviour, not a special case.

### Selecting a version at a phase gate

This is the part that changes how phases end. **HEAD is not
automatically what proceeds.** At a gate:

1. Compute the frontier over the phase's commits. Usually a handful of
   points survive.
2. Evaluate *those points only* on the held-out corpus.
3. Pick one. Continue from it.

Two consequences worth stating.

**This is where held-out belongs.** [Section 12](#12-held-out-integrity)
says held-out is evaluated rarely and shown as a verdict; the reason it
can be rare is that its real job is *selection*, and selection happens
at gates, not at iterations. A handful of candidate commits re-measured
at a gate is affordable in a way that per-iteration held-out evaluation
is not — and per-iteration evaluation would leak the set anyway.

**Selecting on the tuning corpus alone would be model selection on the
training set**, and the frontier makes that worse rather than better,
because it explicitly searches the whole history for the best-looking
point. Searching over hundreds of commits for the best tuning-corpus
number will find noise reliably. The held-out evaluation at step 2 is
not a nicety here; it is what makes step 3 mean anything.

### Going back is not a reset

Choosing a historical version means carrying that commit's *crate
source* forward — not resetting the branch. The ledger, the journal,
the decision records, and possibly the corpus have all advanced, and
all of them are worth keeping: the abandoned attempts are exactly what
the next phase needs so it does not repeat them.

So a selection is a new commit whose tree for the affected crates
matches the chosen sha, with a trailer naming it and a decision record
saying what was given up and why. The history in between stays
readable.

### What this retires

The exchange rate does not have to be guessed after all. The **local
slope of the cost-phase frontier is the empirical rate** — it says what
a coverage point actually cost in bytes, at this operating point, on
this corpus. Deriving it beats declaring it, and it can be re-derived
whenever the operating point moves.

## 11. Size and LOC as objectives

The concern is real: an unsupervised loop adds code. Every iteration
has an incentive to add and none to remove, and after two hundred
iterations the handler is a pile of special cases that each bought a
tenth of a point of coverage.

The phase split from [section 10](#10-objectives-phases-and-the-frontier)
is most of the answer — *phase 3 exists to delete* — and it is a
better answer than a standing ratchet, which would have applied the
pressure during exactly the iterations that should be free to be
wasteful.

* **Binary size.** Two numbers. The stripped release size of
  `measure_<lang>` every iteration, as the proxy a loop can compute in
  isolation; the per-language link delta against `heuristic_jump` at
  phase gates, as the authoritative one
  ([section 10](#measure_lang-size-covers-the-size-gap)). Gated only in
  phase 3, where the ratchet is hard: neither may increase at all, since
  phase 3 may not buy size with quality in the first place. An increase
  requires an approved escalation, not a self-written justification.
* **LOC.** Non-test Rust only, via `tokei`, per crate. Reported always,
  gated never. It is an input to review and a target during cost
  phases, and it is too easy to satisfy dishonestly to be a gate.

**Per-language billing.** Binary size is measured as a delta: build
with and without each `lang_*` linked. Each language then has its own
line item rather than a workspace aggregate nobody can influence. This
matters more than it sounds — tree-sitter grammars are large, on the
order of hundreds of KB of generated parser each, so most of a
language's bill is a fixed cost it did not choose. Report the grammar
and the handler separately, or the number becomes one that gets
ignored.

## 12. Held-out integrity

The readme's development plan holds out 2-3 repositories per language
and calls the tuned/held-out gap the overfitting signal. Under
autonomous loops this needs teeth, because "learning a particular
repo's conventions is the default outcome rather than a risk" is
already the stated expectation for human-driven sessions, and a loop
runs a hundred times more iterations.

* **Held-out repositories and their `truth.jsonl` live in their own
  corpus root** — `../heuristic-jump-heldout/`, a sibling of the tuning
  corpus rather than a subdirectory of it (core §11). Not a convention:
  the loop is given `--corpus` pointing at one root and never the
  other, and a rule it is never told about is a rule it cannot weigh
  against making the number go up.
* **The separation must be physical, not rule-based.** Claude Code's
  `denyRead` rules block the built-in file tools but do not stop `cat`
  in a bash subprocess, so a deny rule is defence in depth and never
  the boundary itself. The boundary is that the data is outside the
  checkout entirely and its path is never passed in.
* **Held-out evaluation runs at phase gates, not at iterations**
  ([section 10](#selecting-a-version-at-a-phase-gate)). A number
  reported every iteration is a number that gets optimised against,
  whatever it is labelled.
* **The loop is shown a verdict, not the numbers.** "Held-out gap
  within threshold" / "gap widened on `ExplicitImport`". Enough to know
  something went wrong and where; not enough to hill-climb.
* **A widening gap stops the loop** and escalates. It is the one signal
  that means the last several iterations were probably net negative,
  and it needs a human to look at the diff.

**A three-way split is probably needed.** Once held-out is used to
*select* a version at every phase gate, it stops being untouched — it
is being optimised against, just at a coarser cadence and by a human.
Over ten gates that is real leakage. The standard remedy applies:
tune / select / final, with the final set evaluated once, at the end,
and never used to choose anything. With ~10 repositories per language
that is roughly 6-7 / 2 / 1-2. This follows directly from making the
frontier selectable and is easy to miss, because the second split was
introduced for a different purpose.

## 13. Shared code, and when it may exist

The question the whole parallel-language plan turns on, and the answer
is more restrictive than an earlier draft of this document had it:
**during phase 2 there is no shared resolution code at all.**

### Three tiers

**`shared` is spec.** `LanguageHandler`, the vocabulary newtypes,
`ProjectView`, `proto`, `Error`. Core §12 is a design commitment and
every crate depends on it. Language loops may not edit it, in any phase.
The seam half is frozen at the end of phase 1a; a change to it is a
Class B escalation, deliberately expensive, because a cheap seam change
is a seam that erodes.

**`similarity` is ported and frozen.** Only what comes across from the
prior implementation — `Occurrences`, `IdentifierParts`, path–namespace
scoring (`resolution-design.md` §5). It can be shared during phase 2
precisely because it is *not* being written during phase 2: it is a
known-good body of code that predates every language crate, so it
generates no churn and no cross-language coupling. Nothing is added to
it.

**Everything else is per-language, and duplication is left standing.**
Two languages that need the same helper each write their own. No
promotion, no shared utility crate, no proposals acted on.

### Why no shared resolution code during phase 2

`resolution-design.md` §9 already argues that a shared-utility layer
designed before any language exists is "a framework wearing a different
hat," and that sharing should be derived from working handlers. Running
the language loops concurrently sharpens that from a design preference
into a hard constraint, for a reason §9 did not have to consider:

* **A shared crate is a surface two writers contend on.** Everything
  else in this design is partitioned so that no two loops ever write the
  same thing. A live `resolve` would be the single exception, and it
  would be the most consequential one.
* **Worse than contention: silent cross-language regression.** A loop
  editing a shared function changes another language's metrics with no
  cause in the affected loop's own diff. That loop then spends
  iterations chasing a regression it did not create — the most expensive
  possible failure for an amnesiac process whose entire context is the
  diff and the journal.
* **Duplication is cheap here and the phase structure pays for it.**
  Phase 3 exists to remove it, is measured on removing it, and removes
  it under an equality constraint. Deferring costs some redundant code
  for the duration of phase 2 and buys complete independence.

So a language loop that wants something shared just writes it locally,
and records the observation in `state/shared-proposals/<language>-NNN.md`
— what it needed, the call site, what it wrote. Nothing consumes those
files until phase 3. They are notes to the extractor, not requests to a
service.

### Extraction is phase 3 work

One writer, nothing running alongside, and the same equality constraint
as everything else in phase 3: a promotion must leave every affected
language's replay outputs byte-identical
([section 10](#phase-3-is-a-refactor-under-an-exact-oracle)). Duplication
removed is binary size and line count removed, which is phase 3's
objective — so extraction is not tidiness, it is how the number moves.

This restores the strict gate an earlier revision had loosened. Loosening
it was a consequence of letting the cost phase trade quality for size;
with trading gone, "metrics unchanged" is simply correct again, and it
has the same virtue phase 3 has generally — the check is exact, so the
loop needs no judgment.

Two rules survive from the earlier design and matter more now:

* **Duplication is the signal, and it is mechanically detectable.**
  Nobody predicts what is shareable; two independent implementations are
  the evidence. Phase 3 has the whole corpus of duplicates to work from
  rather than a guess made before the first handler existed.
* **§9's explicit non-goals become enforceable.** No pipeline driver, no
  query-file loader, no per-language config struct. A loop cannot build
  a framework in a crate that does not exist yet.

### `lang_*` is free

Its own crate, its own fixtures, its own metrics. This is where quality
phase iterations should overwhelmingly land.

### Mechanics: isolation in four layers

Isolation is not one mechanism. Four of them, weakest to strongest, and
they protect different things:

**1. Build isolation — the important one.** A loop's gate names the
crates it owns and builds only those. Core §11's `measure_core` +
`measure_<lang>` split is what makes a language measurable on its own, and
without it the other three layers are theatre: a session confined to
`crates/lang_rust/` that still needs every grammar to compile before it
can see a number is coupled to every other language in the way that
actually costs — build time on every iteration, and a hard stop
whenever any of them is broken.

**2. `PreToolUse` hook.** Rejects `Edit`/`Write` outside owned paths.
Fails fast and saves wasted work, but it is **not a boundary**: deny
rules cover Claude's file tools and not bash subprocesses, so anything
the hook blocks is reachable through `sh -c`.

**3. OS sandbox.** `/sandbox` uses bubblewrap on Linux and takes an
`allowWrite` list. This *is* a real boundary — it covers subprocesses —
and it is the layer that answers "prevent them from writing outside
their dir" literally. `allowWrite` is the owned crate directory,
`state/shared-proposals/`, `target/`, and the git directory; everything
else in the checkout is read-only to the session.

**4. Gate diff scope.** The commit touches only owned paths, checked
after the fact by the gate. Authoritative, because it inspects the
result rather than trusting the actor:

  | Loop | May write |
  |---|---|
  | conformance | `vendor/`, `crates/{shared,driver,heuristic_jump,measure_*}/`, `spec/` |
  | lang-rust | `crates/lang_rust/`, `state/{metrics,journal,decisions}/rust*` |
  | lang-python | `crates/lang_python/` *except* `profile/`, `state/…/python*` |
  | python-pyright | `crates/lang_python/src/profile/pyright.rs`, `state/…/python-pyright*` |
  | phase 3 | everything, one writer, nothing running alongside |
  | *nobody* | `harness/` |

Ownership is by path, not by crate, which is what lets a per-server
profile loop coexist with the language loop that owns the rest of the
same crate. A profile is one file; the language logic is the rest. The
two are evaluated on disjoint sets of corpus positions
([section 10](#multiple-servers-do-not-multiply-the-frontier)), so they
are genuinely independent work rather than two writers sharing a
surface.

Whether per-server loops are worth spawning at all is a volume
question — start with the language loop, and split a profile out only
when the disagreement set is large enough to be worth an optimiser of
its own.

Two rows carry most of the weight. **`measure_core` and
`measure_<lang>` belong to the conformance loop**, never to a language
loop: a loop must not own the code that scores it, and the four-line
binary is written once when the language is added and never again.
**`harness/` is owned by nobody** — the gate script, the ratchet
baselines, the frontier tool, and the held-out runner live there, and
every loop is denied writes to it. Changes are Class B, made by a
human.

That last row is what replaces the separate pinned checkout an earlier
draft called for. The property actually needed is *the loop cannot
weaken its own gate*, and path ownership already delivers it, through
the same three layers as everything else. A second checkout was solving
a problem that a directory boundary solves.

What genuinely does need its own working tree is **evaluating
historical commits at a phase gate.** Measuring a past commit means
having its files on disk, and the alternatives are worse: checking out
an old sha in a loop's tree destroys whatever it was doing, and a
stashed tree is a state machine nobody wants to own.

So the harness keeps **one evaluation worktree** — `git worktree add`
once, then `git checkout <sha>` to each frontier candidate in turn.
Gate evaluation is serial anyway (the loops are quiesced so the timing
pass has a quiet machine), so per-candidate isolation buys nothing, and
one tree means one `target/` and therefore one warm build cache across
candidates rather than a cold full release build each time.

It is a second working tree, not a second clone: `git worktree` shares
the object store, so it costs a checkout rather than a copy of the
repository's history. And it is not *pinned* — it moves to whatever sha
is being measured, which is the whole job. The pinning idea belonged to
the gate-integrity property, and that is `harness/` ownership now.

### Parallel loops, and what they share

Loops run simultaneously, one per language, each in its own
`git worktree` on its own branch.

Cargo cost is lower than it first appears: **a worktree is its own
workspace root, so it gets its own `target/`.** There is no shared lock
to contend on and no serialisation. The price is disk and rebuilding
shared dependencies per worktree, and with builds scoped to the owned
crates — `lang_rust` and `measure_rust`, not the workspace — that price
is small.

Language crates are disjoint on disk, so the code never collides. What
would collide is `state/`, and the fix is the same principle applied to
data: **partition shared state by owner.**

* `state/metrics/<language>.jsonl` — one file per loop, never shared.
* `state/journal/<language>.md` — likewise.
* `spec/ledger/<language>.toml` for language items; the core ledger is
  written only by the conformance loop.
* `state/decisions/<owner>-NNN.md` — owner-prefixed, so two loops
  cannot claim the same number at the same moment. A bare incrementing
  id is exactly the kind of thing that looks fine until two sessions
  allocate `007` four seconds apart.

With code and state both partitioned, nothing two loops write ever
overlaps.

### Branches exist for one commit at a time

The goal is that everything lands on `master` and no long-lived
branches accumulate. That is right — divergent branches are where
integration debt comes from, and none of the reasons to keep one apply
here.

It cannot be done by pointing every loop at `master` directly, for a
mechanical reason: **git refuses to check out the same branch in two
worktrees.** And sharing a single working tree between concurrent loops
is worse than the branch it avoids — loop B would compile loop A's
half-written files, so A's transient breakage becomes B's gate failure,
and green-or-revert stops meaning anything.

So each loop has a branch, and **merges after every green iteration**
rather than at a phase gate:

```
gate passes -> rebase onto master -> fast-forward master -> continue
```

Since code and state are partitioned by owner, the rebase is
conflict-free by construction, and `master` gets a linear history with
one commit per iteration from whichever loop finished first. The branch
exists for the duration of one commit. There is no merge queue and no
integration loop; merging is a step in the iteration contract
([section 4](#4-the-iteration-contract)), not an agent.

The one case that is not automatic: the conformance loop changing
`shared` can break a language crate that rebases onto it. A language
loop does not own `shared` and must not fix it, so a rebase that turns
the gate red is escalated as a blocked item rather than repaired
locally. This is rare by construction — seam changes are Class B and
land in cost phases — and it is the same failure the seam-freeze rule
exists to make rare.

The full-workspace gate still runs at phase gates, because every crate
building *together* and the cross-language checks are the things no
individual loop's scoped gate covers.

**Latency is the one measurement parallelism breaks**, since every loop
is load on the machine the others are timing against. It is handled by
not measuring it during iterations at all: work counters per iteration,
wall clock at gates with the loops quiesced
([section 10](#what-cannot-be-measured-in-isolation)). Coverage and
precision are unaffected because replay's budgets are deterministic
rather than wall-clock
([section 9](#determinism-is-a-precondition-not-a-description)) — without
that property, parallel tuning would not be measuring anything stable.

## 14. What runs the loops

Deliberately boring: a bash loop around headless `claude -p` with a
fixed prompt per loop type. The Ralph pattern is a commodity now and
several harnesses exist, but they are worth reading rather than
depending on — the value is in the gates and the state files, and a
hand-written loop keeps exit codes, gate ordering, and commit trailers
under direct control. The official `ralph-wiggum` plugin specifically
re-feeds the prompt within one session, which defeats the fresh-context
premise the whole design rests on.

Each loop type is a prompt file plus a path-ownership entry plus a gate
command. Adding a language is a table row, which is the same property
the workspace layout already has for adding a language crate.

The fixed part of every prompt — the inviolable rules — is `CLAUDE.md`,
which already reads as one: hard constraints on async, locks,
dependencies, `vendor/`, grammars, and error handling. That file is the
constitution; the loop prompt should point at it rather than restate
it.

## 15. Cost and timing

Tracked with the same discipline as the metrics, and for the same
reason: an unmeasured cost is one that gets discovered after it is
spent. Accounting is **per phase and per language**, because those are
the two dimensions along which the answer actually differs and along
which work can be stopped independently.

### Three resources, not one

They have different drivers and different levers, and conflating them
means optimising the wrong one.

| Resource | What consumes it | Dominant phase |
|---|---|---|
| **Tokens** — the money | model reading and writing | 2a, 2b |
| **Model wall-clock** | iterations × thinking time | 2a, 2b |
| **Machine wall-clock** | truth collection, builds, replay | 1b→2 boundary, 3 |

The distinction matters immediately. Truth collection is ~100 machine
hours and approximately zero tokens; a slow replay inflates model
wall-clock without costing a token; a loop that re-reads 138KB of design
doc per iteration costs tokens without costing wall clock. The
under-a-minute replay target in [section 9](#9-the-inner-loop-must-be-fast)
is a wall-clock lever and not a cost lever, and the ledger's document
anchors in [section 3](#3-the-spec-ledger) are a cost lever and not a
wall-clock one.

### The unit of accounting is the campaign

Every campaign emits one row to `state/cost/<loop>.jsonl`, partitioned
by owner for the same reason the metrics are, with per-experiment
detail nested inside it:

```
campaign id, session id, loop, language, server, phase, target,
commits produced, experiments (committed / reverted / empty),
input tokens (cached / uncached), output tokens,
model seconds, gate seconds, outcome
```

`language` and `server` are null for the phases that are not per-language
(1a, 1b, 2b, 3), which is what lets the same file answer both "what did
Python cost" and "what did the driver cost".

No instrumentation inside the model is needed for this. Each campaign is
one session, the harness records which loop and target it launched, and
`ccusage` reads Claude Code's local JSONL — so the join is on session id
and happens after the fact.

The campaign is the right unit because it is the unit of *decision*: a
campaign that spent its budget and falsified its hypothesis is a
legitimate outcome, and per-iteration accounting would report it as a
run of failures. The experiment counts stay in the row because the
committed / reverted / empty mix is a health signal — a campaign that is
mostly empty experiments is stalling regardless of what it spent
([section 7](#7-progress-stall-and-the-ways-it-is-faked)).

### Cost per unit of progress

The derived number, and the one that should drive stopping decisions.
[Section 7](#7-progress-stall-and-the-ways-it-is-faked) already defines
progress mechanically — ledger advance, test count, frontier movement,
decision resolved — so cost per progress event is computable without a
new definition.

Per phase, the useful ratios are different:

* **1a, 2b (conformance):** tokens per ledger item reaching `verified`.
* **2a (per language, per server):** tokens per coverage point, per
  stratum. This is the number that says when to stop, and it should
  *rise* over a run as the easy wins are exhausted. A rising
  cost-per-point curve is the economic form of the frontier flattening,
  and it usually turns up before the frontier visibly stalls.
* **3 (cost phase):** tokens per kilobyte removed.

### Budgets at three scopes

Each stops and reports rather than continuing quietly:

* **Per campaign** — a token ceiling that closes it. Since context now
  accumulates within a campaign, this is also what bounds context growth,
  so it does double duty and should be set with both jobs in mind.
* **Per phase, per language** — the one that matters. Python's tuning
  budget is exhausted independently of Rust's, and hitting it produces a
  handoff, not a stop-everything.
* **Global** — the backstop.

### Estimates, and replacing them with measurements

The table below is a set of guesses whose main job is to be wrong in a
way that gets noticed. Orders of magnitude only; token counts rather
than currency, since pricing is a multiplication that changes
independently of anything here.

| Phase | Tokens | Model wall-clock | Machine wall-clock | Sensitive to |
|---|---|---|---|---|
| 1a core for measurement | low–moderate | days | small | how good the ledger is |
| 1b repo collection | ~none | ~none | days (mostly human) | repo count |
| truth collection (gate into 2) | ~none | ~none | ~100 machine-hours | repos × servers × index time |
| 2a per language × server | **dominant** | weeks, parallel | small per iteration | iterations to plateau |
| 2b LSP shim | high | weeks, parallel | small | driver test surface |
| 3 whole-repository optimisation | moderate | days, serial | high (release builds) | how much duplication accumulated |

**Calibration is the first ten iterations of each loop.**
[Section 18](#18-what-to-build-first) already calls for watching ten
iterations before trusting the machinery; those ten are also the cost
measurement, and every estimate in the table above gets rewritten from
them before the phase is allowed to run to completion. An estimate that
is never compared against an actual is decoration.

### Levers, by which resource they move

**Tokens.** Bound the context: the ledger's document anchors mean an
iteration reads one section rather than the whole design. Keep the
prompt's fixed prefix byte-identical across iterations and order it
stable-to-volatile — constitution, then prompt, then ledger, then the
journal tail and recent commits — so the cacheable prefix is as long as
possible. Choose the model tier per loop rather than globally: phase 3
is mechanical work under an exact oracle
([`implementation-phases.md`](implementation-phases.md)), which is the
best candidate for a cheaper tier, whereas phase 2a resolution logic is
the hardest reasoning in the project. Tune verifier cadence — every
iteration versus every K — as a cost knob rather than a fixed choice.

**Model wall-clock.** Parallelism across languages, and the replay speed
target. Neither costs tokens.

**Machine wall-clock.** Parallelism across repositories and servers
during truth collection; a warm shared `target/` in the evaluation
worktree; scoped builds during iterations.

## 16. The operator view

Everything above produces files. This section is about the one human who
has to understand them, and it is not a nicety: several places in this
design *stop* until a person decides something — Class B escalations,
phase-3 behaviour-change requests, frontier point selection, stall
handoffs. If those are slow to reach you, the loops idle on provisional
choices and the whole schedule is bounded by how often you go reading
JSON.

So the requirement is not "a status page." It is: **everything a
decision needs, on the page, next to the decision.**

### A single generated HTML file, auto-refreshing

`harness/dashboard/` generates it; output is not committed, since it is
derived from state that is. It is a static page, regenerated whenever a
state file changes.

Auto-refresh needs a trivial local server rather than `file://` — a
page opened from disk cannot poll. So the harness runs a static server
over the dashboard directory plus a filesystem watch; the page polls a
version token every couple of seconds and reloads when it moves. That is
tens of lines and no framework, which is the right size for this. A
`<meta refresh>` would be simpler still and is rejected only because it
resets scroll position on a page you will be reading while it updates.

### What is on it

Five panels, in descending order of how often they should change your
behaviour:

* **Decisions waiting.** The top of the page, always. Each escalation
  rendered with its evidence rather than a link to it — see below.
* **Loop status.** Per loop: phase, current campaign and its hypothesis,
  experiments so far, last commit, and state (running / stalled /
  blocked / budget exhausted). A stalled loop with an unanswered
  question is the thing most likely to be silently costing you a day.
* **Metrics.** The per (language, server) frontier chart, the
  per-stratum table, current versus the recorded baseline, and the
  held-out verdict. This is the chart
  [`implementation-phases.md`](implementation-phases.md) asks for at the
  phase 2a gate; it wants to exist before then, because a frontier is
  more useful watched than reviewed once.
* **Cost.** Per phase and per language, spend against budget, and the
  cost-per-progress trend from
  [section 15](#cost-per-unit-of-progress) — the curve that turns up
  before the frontier visibly flattens.
* **Sessions.** Below.

### Decisions carry their evidence

Each escalation type needs different evidence, and a generic "here is a
markdown file" panel would make you go find it yourself:

| Escalation | What the panel must show |
|---|---|
| Class B decision | options, recommendation, the provisional choice in force, and the sites tagged `DECISION-<n>` |
| Phase 3 behaviour change | the exact replay diff: which queries changed, in which stratum, from what location to what location |
| Frontier selection | the chart, candidates with their held-out numbers, per-stratum breakdown of what each point trades |
| Stall handoff | what was tried, the campaign record, the journal excerpt |
| Held-out gap widened | which stratum, and the commits since the last clean gate |

**Answering should be possible from the page.** A read-only dashboard
means every decision still costs a context switch into an editor, which
is exactly the friction that leaves loops idling. A `POST` to the local
server that writes the answer into `state/decisions/<id>.md` is a few
lines, and it is safe enough on localhost. Worth doing, and worth doing
early — this is the single highest-leverage part of the dashboard,
because it is on the critical path of every phase.

### Every intervention is logged

Everything the loops do is in git, with trailers, attributable to a
campaign. Everything *you* do is currently in nothing at all — a
decision answered by editing a file leaves the answer but not the
reasoning, and killing a loop leaves no trace whatsoever. That is the
missing half of the audit trail, and it is the half with the higher
information density.

`state/interventions.jsonl`, append-only:

```
timestamp, kind, target, answer, rationale, resulting commit
```

`kind` covers the full set: decision answered, phase-3 behaviour change
approved or refused, frontier point selected, stall unblocked, budget
raised, ratchet exception granted, spec edited by hand, loop killed or
restarted, ledger corrected.

**The log is the mechanism, not a record of it.** Answering a decision
*means* appending to this file; the harness derives the decision's
status from the log rather than from someone remembering to write both.
Same trick as the metrics history — make the record the path rather than
a side effect, and it cannot drift from what happened. The dashboard's
POST endpoint is what makes this free: every answer given through the
page is logged by construction. Out-of-band interventions cannot all be
caught, but the two common ones can — a git hook flags hand-authored
commits, and the harness logs its own kills.

**`rationale` is the field that matters and the one that will get
skipped**, so the page should refuse to submit without it. A decision
with no recorded reason is one you will re-litigate in three weeks, and
the loop cannot use it: half the point of a decision record is that a
later campaign reads *why* and applies it to a case nobody anticipated.

Three things fall out of having it:

* **It measures how autonomous this actually is.** Interventions per
  100 experiments, per phase, is the honest version of "close to
  one-shot." Nothing else in the design reports it.
* **Recurring kinds are a design backlog.** Answering fifteen
  variations of "is this trade worth it" in phase 3 means the exchange
  rate should have been written down; answering the same ambiguity about
  a stratum repeatedly means a rule is missing from the prompt. The log
  is where that pattern becomes visible instead of just feeling tedious.
* **Time-to-answer is an operational metric.** The gap between an
  escalation appearing and being answered is time a loop spent idling on
  a provisional choice, and it is the number that says whether the
  dashboard is doing its job.

### Sessions: assign the id, own the transcript

Claude Code already stores every session as JSONL under
`~/.claude/projects/<escaped-cwd>/<session-id>.jsonl` — verified on this
machine, roughly 2 MB per long session. It is tempting to point the
dashboard straight at that. Do not.

**Assign the session id rather than discovering it.** `claude` takes
`--session-id <uuid>`, so the harness generates the campaign id and
passes it in. The campaign id *is* the session id: no correlation step,
no parsing an id out of output, and the transcript path is known before
the session starts. `claude --resume <id>` then works directly, and
`--fork-session` covers resuming a campaign down a different path
without clobbering the original record.

**Tee the stream; do not scrape the store.** Run with
`--output-format stream-json` and write the stream to
`state/sessions/<owner>/<campaign-id>.jsonl` as it goes. That artifact
is the harness's, in a shape it chose. The reason not to depend on
Claude Code's own file is that the format is plainly internal: alongside
the messages it carries `bridge-session`, `ai-title`, `queue-operation`,
and `file-history-snapshot` records, which are TUI bookkeeping and will
drift between versions. A dashboard built on it breaks on upgrade, at a
moment when the thing you want is a working dashboard.

Two structural facts to know if you ever *do* read that file — for
forensics, or because a session predates the harness:

* **It is a tree, not a list.** Records carry `parentUuid`, and
  resume-forks mean more than one leaf. Rendering it as a flat sequence
  in file order is wrong.
* **`isSidechain` marks subagent turns**, interleaved with the main
  thread rather than separated.

The per-campaign row in `state/sessions.jsonl` stays as the index:

```
campaign id (= session id), loop, phase, language, server, target,
started, ended, commits produced, tokens, outcome, transcript path
```

### Reading a transcript

The dashboard links each campaign to a rendered view of its teed stream,
served by the same local server. Raw JSONL is not a view — at 2 MB with
every file read inlined, dumping it into a browser tab is worse than
useless.

What a renderer has to do, in the order it matters for the question you
will actually be asking ("why did it do that?"):

* Turn structure — prompt, thinking, tool calls, results, text — with
  **tool calls collapsed by default**, showing the command or path, and
  expandable.
* Diffs rendered as diffs.
* Gate output and metric deltas pulled out and highlighted, since those
  are the decision points the rest of the turn is reasoning about.
* Lazy loading, and truncation of large tool results with
  expand-on-demand. A campaign that read fifty files will otherwise
  freeze the tab.

That is a few hundred lines of JavaScript and it is the one genuinely
new piece of UI work here. A reasonable first version is a plain link to
the raw file, served rather than rendered, upgraded once campaigns are
long enough that reading them by eye stops working — which will be
almost immediately.

**Storage adds up.** Two MB per campaign, hundreds of campaigns across
seven languages, is gigabyte scale. Transcripts are not committed; they
live beside the corpus, outside the worktree, and old ones for closed
campaigns whose hypothesis was confirmed can be dropped. The ones worth
keeping are the failures, which is the opposite of what a naive
retention rule would do.

## 17. Tooling: adopt, steal, reject

Most of this harness is reinventable from off-the-shelf parts, and most
of those parts are not worth taking. The rule applied below is the same
one `dependency-plan.md` applies to crates: a tool has to solve a
problem we actually have, not a problem adjacent to it.

| Need | Verdict |
|---|---|
| Loop runner | **Build** — ~20 lines of bash. Read `rxdt/loopgate_harness` and `mikehostetler/wreckit` first as reference designs |
| Stall detection | **Steal constants** from `frankbria/ralph-claude-code`; logic is ours |
| Edit-scope hook | **Adopt** `PreToolUse` — as fast feedback, not as a boundary |
| Held-out isolation | **Build** — physical separation; `denyRead` as defence in depth only |
| Worktrees | **Adopt** plain `git worktree` — one per parallel loop, plus ephemeral ones for gate-time evaluation |
| Worktree orchestrators (Conductor, Crystal, claude-squad) | **Reject** — GUI- or macOS-first, and the merge discipline is ours |
| Spec ledger | **Build** — see below |
| Numeric ratchets | **Build** — see below |
| Metric table regression | **Adopt** `insta` for shape; gate checks direction |
| Test runner | **Adopt** `cargo-nextest` (machine-readable output for the test-count ratchet) |
| Decision records | **Adopt** MADR template; `DECISION-` code tags stay ours |
| Journal | **Adopt** git trailers + `git interpret-trailers` |
| Cost monitoring | **Adopt** `ccusage` |
| Verifier | **Build the prompt**; a read-only session is the whole mechanism |
| Ledger extraction notation | **Steal** EARS |
| SDD frameworks (Spec Kit, Kiro, OpenSpec, BMAD) | **Reject** |
| Bencher (metric tracking service) | **Reject** — reasoning below |
| `beads` (agent-facing issue graph) | **Reject** — reasoning below |

Three rejections worth recording, because they are the ones that will
be proposed again:

**Bencher.** It tracks arbitrary metrics with statistical thresholds
and fails CI on regression, which sounds exactly like the ratchets in
[section 11](#11-size-and-loc-as-objectives). The mismatch is that its
value is *statistical* regression detection — seven threshold models
deriving variance from a metric's history — and **our measurements are
deterministic**. `measure replay` runs a deterministic handler
(`resolution-design.md` §11 requires the property) against frozen
answers; binary size is deterministic given a toolchain. A number that
moves has a cause in the diff. The only threshold model that fits is
the static one, which is `if new > baseline { fail }` with a server, a
database, and a token attached.

A baseline file in git is also *better* here, not merely cheaper: a
ratchet change shows up in the commit diff, which is exactly where the
question "did the loop justify this regression?" gets asked. A service
records it somewhere review does not look.

Two places the argument does not reach, noted so the rejection can be
revisited honestly: latency percentiles are genuinely noisy (report
them, gate only gross regressions, do not ratchet them on a dev
machine), and per-stratum *sampling* intervals are real — but that is a
Wilson interval over a query count, computed once, not variance
inferred from history.

**`beads`.** A git-backed issue graph designed for coding agents, with
dependency-aware "what is ready" queries. The ledger's value is that it
is reviewed by hand, once, in a single sitting — 6000 lines of prose
compressed into something a person can audit for omissions. A greppable
TOML file serves that; a Dolt-backed graph database does not, and
`bd ready` is a `phase` field and fifteen lines.

**Spec-driven frameworks.** They solve "the agent has no structure and
hallucinates APIs." That problem is already solved here by a mechanical
oracle and a written spec, and none of them enforce the one invariant
this design turns on — status advancing only when a named test passes.
The two things worth taking are EARS notation and the "constitution"
idea, and the constitution already exists as `CLAUDE.md`.

## 18. What to build first

Minimum viable version, in order:

1. Ledger extraction for **phase 0 and 1 only**, reviewed by hand.
2. The gate script, in `harness/`, denied to every loop.
3. The commit-trailer convention and stall detection.
4. One loop, conformance, on phase 0. Watch it for ten iterations.

Everything else — verifier sessions, held-out isolation, the frontier,
the proposal protocol, per-language billing — is phase 3+ machinery.
Building it now is the premature optimisation `CLAUDE.md` opens by
warning about, and it would be built against a guess at how the loop
behaves rather than an observation. The metrics history is the one
thing worth starting early even though nothing consumes it yet, and
even that is recoverable later by a replay sweep
([section 9](#9-the-inner-loop-must-be-fast)).

The thing to watch in those first ten iterations: does it pick sensible
items, does it leave the tree green, and does the journal accumulate
anything a human would have wanted written down. If the answer to the
third is no, the state file design is wrong and nothing downstream will
save it.

## 19. How this goes wrong

Stated plainly, because each of these has a countermeasure above and
the countermeasures are the weakest part of this document.

* **The ledger is wrong and the loop faithfully implements the wrong
  thing.** Highest-consequence failure. Only defence is the one-time
  hand review, and it is a defence against errors of omission that are
  by nature hard to see.
* **The loop rewrites the spec toward what it built.** Class A/B is a
  judgement call made by the entity with the incentive. The changelog
  makes it auditable after the fact, not preventable.
* **Overfitting.** Expected, per the readme. Held-out isolation catches
  the gross version; a loop that finds a genuinely general improvement
  and a repo-specific hack in the same iteration will ship both.
* **The quality phase climbs to an incompressible peak.** Coverage
  reachable only via thirty special cases, so the cost phase's frontier
  offers nothing but "give back coverage or ship the megabyte." The
  metrics history is what makes this survivable — the cost phase can
  select an earlier, cheaper point — but it cannot manufacture a good
  option that was never on the curve.
* **Intervention that is never recorded.** A decision made in your head
  and applied by hand is invisible to every mechanism here: the loop
  cannot read the reasoning, the next campaign re-raises the question,
  and the autonomy numbers are wrong in the flattering direction. The
  log in [section 16](#every-intervention-is-logged) only helps to the
  extent answering *through* it is easier than answering around it.
* **Plausible motion.** A hundred iterations of refactoring, journal
  entries, and ledger churn with no metric movement. Stall detection is
  the answer and it is tuned by guessing at N.
* **Phase 2 under-scoped.** Corpus collection is real infrastructure
  work. If it slips, every language loop is blocked and the temptation
  is to start them anyway against fixtures, which measures nothing.
* **Selection leaks the held-out set.** Choosing a version at every
  gate is optimisation against it, slowly. The three-way split in
  [section 12](#12-held-out-integrity) is the remedy and it costs
  corpus size, which is the scarce thing.

## Open questions

1. **Should the conformance loop be one loop or several?** The driver
   splits cleanly along core design sections — routing, health,
   documents, dispatch — and those touch mostly disjoint modules. But
   they share `driver`'s own internal types, which is exactly the
   coordination problem [section 13](#13-shared-code-and-when-it-may-exist)
   solves for languages by having the shared layer be derived rather
   than designed. Inside one crate that trick is unavailable.

2. **What is N for stall detection, and is it the same for both loop
   types?** A metric loop legitimately spends several iterations on a
   restructuring that pays off at the end. A conformance loop probably
   should not. Guessing 3 and 5 respectively; no basis.

3. **Is phase 3 tractable as one serial pass?** This is the live risk
   created by forbidding shared resolution code during phase 2. Seven
   languages tuned independently for hundreds of iterations each will
   have accumulated a lot of parallel implementations, and phase 3 has
   to harvest all of it with one writer, under an equality constraint
   that must hold for every language simultaneously. If it turns out
   not to be tractable, the remedy is an interleaved extraction pass
   between tuning rounds — which reintroduces exactly the coordination
   the current rule removes, so it should be a measured decision rather
   than a drift.

4. **Does anything come after phase 3?** `implementation-phases.md` is a
   single pass. But phase 3 refactoring changes the code the phase 2a
   frontier was measured on, and extraction may reveal that one
   language's approach is simply better than another's — which is new
   tuning information, not an optimisation. A second 2a is plausible;
   whether it is worth its cost is not knowable yet.

5. **Does the quality phase need any cost guardrail at all?** The case
   against is the whole argument for the split. The case for is the
   incompressible-peak failure above — a cheap standing limit (an order
   of magnitude, not a ratchet) might prevent the pathological version
   without suppressing legitimate experiments. Unclear whether the
   pathology is real or imagined; the first quality phase will say.

6. **How are ties between languages broken during extraction?** Two
   languages implement the same idea with different signatures.
   Promoting one and migrating the other rewrites a working handler the
   promoting writer did not author. The equality constraint in
   [section 13](#extraction-is-phase-3-work) bounds the damage — outputs
   must not move — but says nothing about which signature wins, and with
   seven languages the number of such ties could be large enough that
   the answer needs to be a rule rather than a case-by-case judgment.

7. **Should decision escalations batch or interrupt?** Interrupting per
   decision is unusable at loop cadence. Batching means the loop runs
   further on provisional choices, and reconciliation gets more
   expensive the longer it waits. A rising `DECISION-` count is the
   signal; the threshold that should trigger a batch review is unknown.

8. **Is the corpus large enough to distinguish a real improvement from
   noise?** Not run-to-run noise — replay is deterministic — but
   sampling noise: some strata will have few enough queries that a
   frontier point is indistinguishable from the one it dominates.
   Per-stratum Wilson intervals, and a rule that ignores movement
   inside them, are needed before the first frontier is selected from.

9. **Does the binary-size ratchet fight the vendored-rope work?**
   `vendored-rope-design.md` adds newtypes throughout `rope`; that is
   monomorphisation-neutral in principle and probably free, but "in
   principle" and a hard gate are different things. The baseline is
   taken after phase 0, not before.

10. **Should the loop be allowed to add languages on its own?** Adding
    `lang_go` is "a table row" by design in three separate places now.
    That makes it exactly the kind of thing an under-constrained loop
    does to show progress, at a cost of a megabyte of grammar and a new
    permanent maintenance surface. Probably explicitly forbidden.
