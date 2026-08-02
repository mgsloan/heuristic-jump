# Implementation loop design

Initial ideas for building this project with autonomous Claude Code
loops rather than by hand — a "Ralph Wiggum" loop per work area, each
running a fixed prompt against a durable on-disk state, iterating until
it either matches the spec or stops making progress.

This is a design sketch, not a commitment. Nothing here has been tried
on this repository, and the parts most likely to be wrong are called
out in [section 16](#16-how-this-goes-wrong) and the open questions.

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

More consequentially: `resolve` does not exist and, per
`resolution-design.md` §9, *must not* exist until the Rust handler has
been written longhand and the shared utilities extracted from it. So
the shared-library coordination problem — the hard one — does not bite
until language number two. That is a genuine simplification and the
sequencing in [section 8](#8-sequencing-and-gates) leans on it: build
the coordination machinery lazily, when there is a second writer, not
before.

## 2. Two loops, two oracles

|  | **Conformance loop** | **Metric loop** |
|---|---|---|
| Scope | `vendor/`, `shared`, `driver`, `heuristic_jump`, `scan` | one `lang_*` crate |
| Oracle | spec ledger + test suite + adversarial verifier | corpus numbers per stratum |
| Progress | ledger items reaching `verified` | coverage up, held-out gap not widening |
| Done | all P0 items verified, verifier finds nothing for K rounds | metric plateau or budget exhausted |
| Failure mode | spec drift; the loop edits the spec to match the code | overfitting to the tuning corpus |
| Parallel? | no — one writer | yes — one per language |

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
* `cargo clippy --all-targets -- -D warnings` (per `CLAUDE.md`, clippy
  passes for every commit)
* `cargo test --workspace`
* ratchets: test count, binary size, LOC ([section 10](#10-size-and-loc-as-objectives))
* ledger consistency: every `verified` item's test exists and ran

**Green-or-revert is not negotiable.** A broken tree costs the *next*
iteration its whole context budget on diagnosis, and the next iteration
does not know that the breakage was deliberate. The cheapest possible
handoff between two amnesiac sessions is a repository that builds.

Commit messages carry machine-readable trailers so `git log` is the
journal and stall detection needs no separate bookkeeping:

```
[core-3.2] route swallow decision through writer:editor

ledger: core-3.2 todo -> verified
tests:  +1 double_response_assertion
loc:    driver +38
binary: +412B
decision: none
```

`state/journal.md` holds what the trailers cannot: approaches tried and
abandoned, and why. This is the single most valuable file for
preventing the loop from rediscovering the same dead end every third
iteration.

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

The escalation is a file, `state/decisions/NNN.md`: the question, the
options, the evidence available, a recommendation, and — critically —
**what the loop is doing meanwhile.** The loop picks the most
reversible option, tags every affected site with
`// DECISION-017: provisional`, and continues. It never idles waiting
for an answer.

When the answer arrives, reconciling the tagged sites is a normal
ledger item. `grep -r DECISION-` is the outstanding-provisional-choice
report, and its count is a health metric: rising steadily means the
loop is running ahead of its decisions and the work is getting
speculative.

**Seed the queue from what already exists.** The readme has 13 numbered
future questions, `resolution-design.md` has 13 open questions,
`vendored-rope-design.md` has 8. Those are already Class B items in
everything but format. Converting them to decision files before the
first loop runs means the loop starts with its uncertainties enumerated
rather than discovering them one stall at a time — and several have
recommendations attached already, which the loop can adopt as its
provisional choice with no further reasoning.

## 7. Progress, stall, and the ways it is faked

**Progress** is any of: a ledger status advancing; a test count
increasing; a corpus metric improving beyond noise; a decision item
being resolved. All four are computable from the repository, which is
the point — the loop does not get to assess its own progress in prose.

**Stall** is N consecutive iterations (start with 3) with none of the
four. On stall the loop stops and writes `state/handoff.md`: what it
was trying, what it tried, what it believes is blocking, and the
specific question that would unblock it. Then it notifies and exits.

The failure this is guarding against is not idleness — it is the loop
generating plausible activity indefinitely. Which brings up the ways
the metrics can be satisfied without work being done:

| Gaming route | Countermeasure |
|---|---|
| Mark ledger items `verified` without tests | `verify` must name a test that exists and passed in the gate run |
| Delete or weaken tests | Test count is a ratchet; test *deletions* are flagged for review regardless of count |
| Rewrite the gate script | Gate runs from a pinned checkout outside the worktree; changes to it are Class B |
| Rewrite the spec to match the code | Class A/B split, plus `spec/changelog.md` review; any spec edit in the same commit as code touching the same item is flagged |
| Tune to the corpus | Held-out repos are physically unreadable by the loop ([section 11](#11-held-out-integrity)) |
| Split one item into ten to show motion | Ledger additions by the loop are marked `origin = "loop"` and reported separately from the reviewed baseline |

None of these is airtight against a determined optimiser. They are
airtight enough against an *undirected* one, which is the actual risk:
the loop is not adversarial, it is just weakly grounded, and weak
grounding drifts toward whatever is easiest to satisfy.

## 8. Sequencing and gates

Loops cannot start in parallel because the measurement infrastructure
is a prerequisite for the only real oracle.

**Phase 0 — vendor and vocabulary.** `vendor/rope`, `sum_tree`, cut-down
`util`, the newtype work in `vendored-rope-design.md`, `shared` with the
trait and `proto`. Gate: workspace builds, upstream rope tests pass
unchanged, position-encoding property tests pass.
*Hand-driven or heavily supervised.* This is where the vocabulary types
are decided, and getting them wrong is expensive downstream in a way no
loop will notice.

**Phase 1 — the driver.** Sections 1-15 of the core design. Gate:
transparency golden tests, server-originated round-trips, protocol race
tests, double-response assertion, codec fuzz.
*Conformance loop.* This is the largest single body of work and it has
no gradient, so it lives or dies on the ledger being good.

**Phase 2 — measurement.** `scan`, the trace record schema, corpus
repositories, ground-truth collection. Gate: a `truth.jsonl` exists for
at least one repo and the replay harness reproduces it.
**This is the gate for everything after it, and it is the phase most
likely to be under-scoped.** See [section 9](#9-the-inner-loop-must-be-fast).

**Phase 3 — Rust, longhand.** `lang_rust` written without `resolve`,
per `resolution-design.md` §10. Gate: fixture tests, the corpus
baseline recorded.
*Metric loop, single writer.* No shared-library coordination problem
exists yet.

**Phase 4 — extraction.** `resolve` harvested from `lang_rust`. One
task, one writer, no parallelism. Gate: `lang_rust` metrics unchanged
to the byte after extraction — a pure refactor with a mechanical proof.

**Phase 5 — parallel languages.** Python, TypeScript, and onward. This
is where [section 12](#12-coordinating-the-shared-library) applies and
it is the only phase that needs it.

## 9. The inner loop must be fast

A metric loop whose iteration takes four hours is not a loop. The
corpus scan drives a real language server over ten repositories; that
is hours, and it must happen approximately never.

So ground truth is a **frozen artifact**. `scan collect` runs once per
(repo commit, server version) and writes `truth.jsonl`: every
identifier position, the LSP's answer, the LSP's latency. Tuning
iterations run `scan replay`, which launches no language server at all
— it replays handlers against the frozen positions and compares to the
frozen answers. Core §11 specifies both modes.

Target: full replay over the tuning corpus in under a minute. If it is
slower than the model's thinking time, the loop is I/O bound on its own
feedback and iteration count collapses.

Corollary: `truth.jsonl` is versioned and pinned. A metric comparison
across two different corpus versions is meaningless, and a loop that
regenerates ground truth mid-run has destroyed its own baseline.

## 10. Size and LOC as objectives

The concern is real: an unsupervised loop adds code. Every iteration
has an incentive to add and none to remove, and after two hundred
iterations the handler is a pile of special cases that each bought a
tenth of a point of coverage.

**Binary size is the honest metric.** It is what the user pays, it
cannot be gamed by formatting, and it is directly attributable.

**LOC is a proxy for maintenance cost** and it is gameable — long
lines, dense expressions, cleverness — and pushing on it fights
`CLAUDE.md`'s explicit priority of clarity over concision. So the two
get different treatment:

* **Binary size: a hard ratchet.** Release build, stripped. It may not
  increase without an entry in `state/budget-exceptions.md` naming the
  coverage points bought and the stratum they came from. The gate fails
  otherwise.
* **LOC: reported per crate, soft ratchet.** Non-test Rust only, via
  `tokei`. An increase is allowed; an increase with no ledger item or
  metric movement attached is flagged for review.

**Per-language billing.** Binary size is measured as a delta: build
with and without each `lang_*` linked. Each language loop then sees its
own bill rather than a workspace aggregate it cannot influence. This
matters more than it sounds — tree-sitter grammars are large, on the
order of hundreds of KB of generated parser each, so most of any
language's line item is a fixed cost it did not choose. Report the
grammar and the handler separately, or every language loop will read a
number it cannot move and start ignoring it.

**On a single combined fitness score:** tempting, and probably wrong to
start with. `score = Σ w_s · coverage_s − λ_size · bytes − λ_loc · loc`
requires knowing the exchange rate between a coverage point and a
kilobyte, and there is no basis for that number yet. Ratchets need no
exchange rate: they say "not worse unless justified", which is the
actual intent, and they leave the tradeoff to the justification where
it can be read. Revisit once the per-stratum table has real numbers —
at which point the exchange rate can be *derived* from the value
weighting in the readme (a coverage point on a stratum the LSP serves
in 150ms is worth close to nothing) rather than guessed.

One more incentive worth building in: a **deletion bounty**. A ledger
item type whose completion is measured in code removed with metrics
unchanged. Without it, nothing in the loop's incentive structure ever
points at simplification, and `CLAUDE.md`'s "implement the slow simple
version first" only constrains the beginning.

## 11. Held-out integrity

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
* **Held-out evaluation runs every K iterations, not every one.** A
  number reported every iteration is a number that gets optimised
  against, whatever it is labelled.
* **The loop is shown a verdict, not the numbers.** "Held-out gap
  within threshold" / "gap widened on `ExplicitImport`". Enough to know
  something went wrong and where; not enough to hill-climb.
* **A widening gap stops the loop** and escalates. It is the one signal
  that means the last several iterations were probably net negative,
  and it needs a human to look at the diff.

## 12. Coordinating the shared library

The question the whole parallel-language plan turns on. Three tiers,
with different rules, derived from what each thing actually is.

### `shared` is spec

`LanguageHandler`, the vocabulary newtypes, `proto`, `Error`. Core §12
is a design commitment and every crate in the workspace depends on it.

**Language loops may not edit it.** A needed change to the seam is a
Class B escalation by definition, and it lands the way a stop-the-world
pause lands: all language loops quiesce, one writer makes the change
and migrates every `lang_*` in the same commit, the full gate runs, the
loops resume.

That is deliberately expensive, and the expense is the feature. If a
seam change is cheap, the seam erodes — and the seam is the only reason
these loops can run in parallel at all.

### `resolve` is derived

Per `resolution-design.md` §9, `resolve` is an inventory extracted from
working handlers, and a shared-utility layer designed in advance "is a
framework wearing a different hat". That rule, taken seriously, mostly
dissolves the coordination problem:

**A language loop that needs a shared utility writes it locally, in its
own crate.** Always allowed, never blocks, never conflicts. It also
files `state/shared-proposals/NNN.md`: what it needed, the call site,
what it wrote locally.

**Promotion is a separate job with a single writer.** An extraction
pass — run between rounds, not concurrently with language loops —
scans for the same thing implemented in two or more `lang_*` crates,
promotes it to `resolve`, and updates every caller in one commit. The
gate is the phase-4 gate: every affected language's metrics unchanged.

The consequences are all good ones:

* **Duplication is the signal, and it is mechanically detectable.**
  Nobody has to predict what is shareable; two independent
  implementations are the evidence, which is the rule §9 already states.
* **No inner-loop coordination at all.** No locks, no queue, no waiting.
  The cost is temporary duplication, which is cheap and visible.
* **Existing `resolve` functions are never modified by a language
  loop.** Modifying a shared function is how one loop silently changes
  another language's metrics — the single worst failure mode available
  here, because the affected loop sees a regression with no cause in
  its own diff. Language loops propose changes to existing `resolve`
  code; they do not make them.
* **§9's "explicit non-goals" become enforceable.** No pipeline driver,
  no query-file loader, no per-language config struct. A loop cannot
  build a framework in `resolve` if it cannot write to `resolve`.

### `lang_*` is free

Its own crate, its own fixtures, its own metrics. This is where
iterations should overwhelmingly land.

### Mechanics

* **One git worktree per loop**, one branch each. An integration loop
  merges in a fixed order and runs the full gate, and it is the only
  writer to `main`. Shared-tier changes are near-zero by construction,
  so merges are almost always trivial.
* **Edit scope is enforced by a hook, not by the prompt.** A
  `PreToolUse` hook in `.claude/settings.json` rejects `Edit`/`Write`
  outside the loop's owned paths. Path ownership is a table in the
  harness config:

  | Loop | May write |
  |---|---|
  | conformance | `vendor/`, `crates/{shared,driver,heuristic_jump,scan}/`, `spec/` |
  | lang-rust | `crates/lang_rust/`, `state/shared-proposals/` |
  | lang-python | `crates/lang_python/`, `state/shared-proposals/` |
  | integration | everything, one at a time, between rounds |

  A prompt-level rule about which files to touch is a rule that gets
  violated on iteration forty when it is inconvenient, and nobody
  notices until the merge.
* **The metric harness runs from outside the worktree**, at a pinned
  commit, so a loop cannot alter how it is scored.

## 13. What runs the loops

Deliberately boring: a shell loop around headless `claude -p` with a
fixed prompt per loop type, or the `/loop` skill for supervised runs.
The interesting engineering is in the state files and the gate, not in
the runner. If the runner needs to be clever, the state files are not
carrying enough.

Each loop type is a prompt file plus a path-ownership entry plus a gate
command. Adding a language is a table row, which is the same property
the workspace layout already has for adding a language crate.

## 14. Cost

Worth estimating before committing, because the failure mode is finding
out after $2000 of tokens that the ledger was wrong in phase 1.

Rough shape: the conformance loop is the expensive one — large context
per iteration (design doc sections plus driver code), many iterations,
low per-iteration value. The metric loops are cheaper per iteration and
have a natural stopping point.

Two controls: a per-loop iteration budget that stops and reports rather
than continuing, and mandatory human review at each phase gate. Phase
gates are also the natural place to check that the loop has not been
producing plausible motion — which is much cheaper to detect at 50
iterations than at 500.

## 15. What to build first

Minimum viable version, in order:

1. Ledger extraction for **phase 0 and 1 only**, reviewed by hand.
2. The gate script, outside the worktree.
3. The commit-trailer convention and stall detection.
4. One loop, conformance, on phase 0. Watch it for ten iterations.

Everything else — verifier sessions, held-out isolation, the proposal
protocol, per-language billing — is phase 3+ machinery. Building it now
is the premature optimisation `CLAUDE.md` opens by warning about, and
it would be built against a guess at how the loop behaves rather than
an observation.

The thing to watch in those first ten iterations: does it pick sensible
items, does it leave the tree green, and does the journal accumulate
anything a human would have wanted written down. If the answer to the
third is no, the state file design is wrong and nothing downstream will
save it.

## 16. How this goes wrong

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
  the gross version; a loop that finds a genuinely general
  improvement and a repo-specific hack in the same iteration will ship
  both.
* **Plausible motion.** A hundred iterations of refactoring, journal
  entries, and ledger churn with no metric movement. Stall detection is
  the answer and it is tuned by guessing at N.
* **Phase 2 under-scoped.** Corpus collection is real infrastructure
  work — repository selection, server versions, running ten real LSPs
  to completion, storage. If it slips, every language loop is blocked
  and the temptation is to start them anyway against fixtures, which
  measures nothing.
* **Two loops improve their own metric by making the shared thing
  worse.** The proposal protocol prevents the direct version. It does
  not prevent a `resolve` promotion in phase 4 that quietly suits one
  language better than another.

## Open questions

1. **Should the conformance loop be one loop or several?** The driver
   splits cleanly along core design sections — routing, health,
   documents, dispatch — and those touch mostly disjoint modules. But
   they share `driver`'s own internal types, which is exactly the
   coordination problem [section 12](#12-coordinating-the-shared-library)
   solves for languages by having the shared layer be derived rather
   than designed. Inside one crate that trick is unavailable.

2. **What is N for stall detection, and is it the same for both loop
   types?** A metric loop legitimately spends several iterations on a
   restructuring that pays off at the end. A conformance loop probably
   should not. Guessing 3 and 5 respectively; no basis.

3. **Does the verifier need to be a different model, not just a
   different session?** Same-model verification shares blind spots by
   construction. A cheaper model as verifier is affordable enough to
   run every iteration, which may beat a better one run every fifth.

4. **How are ties between languages broken in the extraction pass?**
   Two languages implement the same idea with different signatures.
   Promoting one and migrating the other is a change to a working
   handler made by a loop that did not write it, and it can move that
   language's metrics. Possibly the extraction gate should require
   metrics unchanged for *every* language, which makes some promotions
   impossible and is arguably correct.

5. **Should decision escalations batch or interrupt?** Interrupting per
   decision is unusable at loop cadence. Batching means the loop runs
   further on provisional choices, and reconciliation gets more
   expensive the longer it waits. A rising `DECISION-` count is the
   signal; the threshold that should trigger a batch review is unknown.

6. **Is the corpus large enough to distinguish a real improvement from
   noise at loop cadence?** Ten repositories per language, minus
   held-out, with per-stratum breakdown — some strata will have few
   enough queries that iteration-to-iteration movement is meaningless.
   Per-stratum confidence intervals, and a rule that ignores movement
   inside them, are probably needed before the first metric loop runs,
   not after.

7. **Does the binary-size ratchet fight the vendored-rope work?**
   `vendored-rope-design.md` adds newtypes throughout `rope`; that is
   monomorphisation-neutral in principle and probably free, but "in
   principle" and a hard gate are different things. The ratchet needs a
   baseline taken after phase 0, not before.

8. **Should the loop be allowed to add languages on its own?** Adding
   `lang_go` is "a table row" by design in three separate places now.
   That makes it exactly the kind of thing an under-constrained loop
   does to show progress, at a cost of a megabyte of grammar and a new
   permanent maintenance surface. Probably explicitly forbidden.
