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
* **The spec is already written and it is enormous.** Ten thousand-odd
  lines across `design/`, largely decided, with the undecided parts
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

This document was written before there was any code, and the plan it
describes is shaped by that. **Where the project has actually got to is
deliberately not recorded here**: it is `state/phase.toml`'s `phase`, the
workspace members in `Cargo.toml`, and the section ledger in
`state/audit/`. A prose inventory of what exists would be stale within a
week and would have to be re-verified at every audit — the cost
[section 8](#8-sequencing-and-gates) already avoids by letting
`phases.md` answer "what is in phase 1a" rather than a list maintained
in this document.

What the state of the code forces is a split, and it is the split
[section 2](#2-two-loops-two-oracles) is about. **The metric loop is
blocked on a bootstrap**: it has nothing to measure until there is a
corpus, ground truth, and a handler that answers something, which is
phases 1a through 1.5, and that bootstrap is not itself loopable in any
interesting sense. **The conformance loop is not blocked.** Its oracle
is the audit, which exists as soon as the documents do, so it can run
from day one and against phase 1a itself — which is what
[section 18](#18-scope-phases-1-and-15-first) has it do.

**So most of this document is not yet in scope.** The initial
implementation covers phases 1a through 1.5 and stops
([section 18](#18-scope-phases-1-and-15-first)); everything from 2a
onward is a followup. What follows is specified now because specifying
it is what makes the followup cheap, and because several phase-1
decisions — the corpus split, the shape of the measurement record —
are only correct if you know what will consume them.

More consequentially: there is no shared resolution crate, and per
`resolution.md` §9 there must not be one until working handlers
exist to extract it from. Carried to its conclusion — see
[section 13](#13-shared-code-and-when-it-may-exist) — that removes the
shared-library coordination problem from phase 2 entirely, rather than
solving it. The only shared code during tuning is the seam and a frozen
`similarity` crate ported from the prior implementation.

## 2. Two loops, two oracles

|  | **Conformance loop** | **Metric loop** |
|---|---|---|
| Scope | `vendor/`, `shared`, `driver`, `heuristic_jump`, `measure_*` | one `lang_*` crate |
| Oracle | the audit, plus the test suite | corpus numbers per stratum |
| Progress | a section going clean ([section 5](#5-the-auditor-and-the-conformance-loops-number)) | movement on the frontier ([section 10](#10-objectives-phases-and-the-frontier)) |
| Done | every section clean, and a human has ruled on the minor list | frontier stops advancing, or budget exhausted |
| Failure mode | spec drift; the loop edits the spec to match the code | overfitting to the tuning corpus |
| Concurrency | **N workers**, a campaign each, one gap list, conflict handled rather than excluded ([section 13](#workers-one-loop-several-campaigns-at-once)) | parallel, **one per language**, in phase 2a ([section 13](#parallel-loops-and-what-they-share)) |

Conflating these is the first mistake available. A conformance loop
with no number to chase will invent one; a metric loop with a spec
checklist will spend its iterations on checklist bookkeeping instead of
on the thing that moves the number.

## 3. Where the work comes from

Nine thousand lines of prose is not a work queue — but it is already
*structured*. The design documents are numbered, sectioned, anchored,
and written by hand, which means the enumeration a work queue needs
already exists and has already been reviewed by the person who wrote it.

An earlier revision extracted that structure into `spec/ledger.toml`:
one entry per checkable claim, several hundred of them, hand-reviewed
once before anything ran. **That is removed.** It duplicated a structure
the documents already have, it was the single largest artifact standing
between here and the first line of code, it would have gone stale on
every Class A spec edit, and — by [section 19](#19-how-this-goes-wrong)'s
own ranking — a ledger with a hole in it was the highest-consequence
failure available, because the loop would faithfully implement the wrong
thing and every downstream number would agree that all was well.

### The audit produces the queue

[Section 5](#5-the-auditor-and-the-conformance-loops-number) already
reads the spec and the code and reports gaps. Those gaps *are* the work
queue. `state/audit/<doc>.toml` holds the result, and **the harness writes
it from the auditor's output** — the auditor session itself edits nothing,
which is what keeps it a judgement rather than a participant
([section 5](#5-the-auditor-and-the-conformance-loops-number)). Read by
everyone:

```toml
[section."shim.md#3-message-routing"]
state       = "gaps"        # clean | gaps | unjudged
last_audited = "..."

  [[section."shim.md#3-message-routing".gap]]
  claim = "the swallow decision belongs to writer:editor"
  found = "router.rs drops the frame before the writer sees it"
  where = "crates/driver/src/router.rs:88"
```

Nobody authors this file and nobody edits it by hand. The section list
comes from parsing headings out of `core.md` and `shim.md`, so it is
mechanical, it costs nothing, and it updates itself when a document is
edited rather than drifting from it.

### Sections clean is the number

A gap count is a poor metric on its own: two audits of unchanged code
disagree about *how many* problems a section has, because that is a
judgement about granularity. They agree far more readily about whether
a section has **any**.

So the headline is **sections clean over sections total**. The
denominator is fixed and mechanical, the numerator moves only when a
section that had gaps stops having them, and neither depends on the
auditor's mood about whether two related problems are one gap or two.
The gap list stays, as the work queue and as a secondary "how much is
left" figure, but it is not what progress is measured in.

Coverage comes from **rotation, not from a checklist**: every audit
covers the sections the last campaign touched plus a rotating slice of
the rest, so every section is revisited within a bounded number of
campaigns. That replaces the property the ledger was providing — that
nothing is silently unexamined — with a schedule instead of an
inventory. A section nobody has reached yet is `unjudged`, which is
visible, and different from `clean`.

### Who uses it

**Only the conformance loops**, and it is worth saying so because the
document could easily read as though this were universal. Tuning loops
target the stratum with the largest share × gap and are judged by the
corpus; phase 3 targets duplication reports and is judged by an equality
check. Neither reads any of it.

Which documents get audited follows from the same test. `core.md` and
`shim.md` do. **So does this one**, once the conformance loop turns
around and builds the phase-2 machinery from it
([section 18](#the-conformance-loop-builds-the-followup)) — the claims
here are checkable in the same way, and nothing about the mechanism
changes.

The asymmetry is not an oversight, it is the difference between the
documents. **`core.md` and `shim.md` make checkable claims** — the prime
invariant holds, `writer:editor` owns the swallow decision,
`ProjectView` yields nothing gitignored — and a claim either holds or it
does not. **`resolution.md` describes an approach whose success is
measured**, and no amount of conforming to it makes a handler good. So
one is audited and the other is not, and a tuning campaign that tried to
"conform to `resolution.md`" would be optimising the wrong thing.

### What the ledger was carrying, and where it went

* **A named test per claim.** Gone as a mechanical link. The audit
  judges the claim directly and may cite a test as evidence; a passing
  test with an unsatisfied claim is still a gap, which is the stronger
  reading anyway.
* **Priority.** The auditor ranks its own gaps, and phase decides which
  documents are in scope at all.
* **Items needing a human.** These are the audit's minor list and its
  `unjudged` sections, both of which already escalate.
* **Context bounding.** Unchanged, and now direct: a gap names a
  document anchor, so a campaign reads that section rather than 90KB of
  shim design.

## 4. The iteration contract

One iteration is:

1. Read `state/audit/`, `state/decisions/`, `state/journal/<owner>.md`,
   and the last N commit messages.
2. Pick exactly one target — an open gap, or an `unjudged` section.
3. Do it. Read only the spec sections the item names.
4. Run the gate (below). If the gate fails and cannot be fixed within
   the iteration, `git revert` to green and write what was learned into
   the journal.
5. Commit with structured trailers.
6. Exit.

**The gate**, run in this order, all mandatory:

* `cargo fmt -p <owned crates> --check` — **never bare `cargo fmt`**,
  which would reformat `vendor/` and destroy the re-sync property
  (`CLAUDE.md`)
* `cargo clippy -p <owned crates> --all-targets -- -D warnings`
* `cargo nextest run -p <owned crates>`, and `hj selftest` — the harness's
  own parsing and arithmetic — on every run rather than only on the loops
  that own crates. A loop whose deliverable is not a crate otherwise reaches
  step 4 having executed nothing it wrote, and `hj` is what computes every
  loop's number, so it breaking stops all of them at once
* **diff scope**: the commit touches only paths this loop owns
  ([section 13](#mechanics-isolation-in-four-layers)). This is the enforcement,
  not the hook.
* ratchets, in phase 3 only ([section 11](#11-size-and-loc-as-objectives))
* audit consistency: every open gap names a document anchor that exists
* metrics row appended ([section 10](#10-objectives-phases-and-the-frontier))

**The gate is scoped to the crates the loop owns**, per `CLAUDE.md`'s rule
against routinely building the workspace. A Rust tuning iteration builds
`lang_rust` and `measure_rust` and nothing else — no other grammar, no
driver. `core.md` §7's split of the measurement program into `measure_core`
plus a four-line `measure_<lang>` exists to make that possible: without
it, measuring one language means compiling all of them, and the
confinement is decorative. The full-workspace gate runs once per phase
gate, not once per iteration.

**Green-or-revert is not negotiable.** A broken tree costs the *next*
iteration its whole context budget on diagnosis, and the next iteration
does not know that the breakage was deliberate. The cheapest possible
handoff between two amnesiac sessions is a repository that builds.

Commit messages carry machine-readable trailers, parseable with stock
`git interpret-trailers`, so `git log` is the journal and stall
detection needs no separate bookkeeping:

```
[shim-3.2] route swallow decision through writer:editor

audit: shim.md#3-message-routing gaps -> clean
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
   (stratum, language) with the largest share × gap, plus the
   hypothesis about why it is losing coverage; for a conformance loop, a
   open gap, or an unjudged section. The
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
| conformance (1a, 2b) | one open gap, or one unjudged section |
| tuning (2a) | one hypothesis about one stratum |
| phase 3 | one refactor target |

The conformance loops sit close to pure Ralph, because gaps genuinely
are independent and the spec is the memory. Tuning sits
furthest from it, for the reasons in [section 0](#0-the-shape-of-the-idea).

### Campaigns compare notes, asymmetrically

Loops are isolated in code and state, and deliberately so. They are
**not** isolated in what they have learned, because the languages differ
in mechanism while their resolution problems rhyme — import resolution,
re-export chains, ambiguity ranking, wildcard imports. A hypothesis
tested in Rust is evidence about Python, and `resolution.md` §9's
"why each resists sharing" is an argument about *code*, not about
findings. Prose creates neither write contention nor silent
cross-language regressions, which are the two things the no-shared-code
rule exists to prevent.

So each loop maintains `state/findings/<owner>.md`: **a digest of at
most 512 words, rewritten at every campaign close**, holding what this
loop has learned that might matter to a different language. Every loop
reads every other loop's digest in its volatile tail.

Rewritten rather than appended, and capped rather than budgeted. An
append-only log grows until the tail cannot carry it and then gets
truncated by recency, which discards on the wrong axis — the finding
that matters may be from campaign three. A hard cap forces the opposite:
to add something, the loop must decide what no longer earns its place,
which is synthesis rather than accumulation. The cap is the mechanism,
so 512 is a limit and not a target.

The digest is not the archive. Full campaign records stay in
`state/campaigns/`, and a loop that finds a digest line interesting can
go read the campaign behind it. That is a legitimate pointer — it is
subject matter, not a rule ([section 14](#rules-are-inlined-subject-matter-is-read)).

**Falsified and confirmed hypotheses are not shared on equal terms**,
and this asymmetry is the whole of the design:

* **A falsified hypothesis is broadcast plainly.** It saves another loop
  the entire cost of retrying, and it *removes* an option rather than
  proposing one — so it cannot anchor anybody toward a particular
  answer.
* **A confirmed hypothesis is published as a candidate, never as a
  result.** Another language may test it among its own hypotheses; it
  may not adopt it on Rust's evidence.

The reason for the asymmetry is that sharing correlates the loops, and
correlation is expensive here. Seven languages tuning independently are
seven independent samples, which is what makes a widening held-out gap
mean something and what makes parallel loops worth more than one loop
run seven times as long. A corpus-specific trick that propagates
compounds the overfitting in every language at once, and a loop that
reads "this worked elsewhere" tends to test that instead of exploring
its own space — which spends the hypothesis diversity the parallel
structure was bought for. Negative results carry none of that: they
narrow the space without pointing anywhere in it.

The risk is also **monitorable rather than merely feared.** Correlated
overfitting has a signature — the held-out gap widening across languages
at the same time, rather than in one — and
[section 12](#12-held-out-integrity) already stops the loops when a gap
widens. If findings are doing harm, that is where it shows.

At phase 2a's scale — a dozen loops, 512 words each — every digest fits
in every tail with room to spare. **Phase 6 is where that stops being
true**: Zed's full language set is thirty-plus loops, and carrying all
of them would cost more tail than the findings are worth. A selection
rule is needed there and deliberately not before, since choosing one now
would mean guessing at which axis matters before a single digest exists.

### What a loop remembers about itself

Distinct from the above, and solving a different problem: a fresh
context does not know what this loop has already tried. Two artifacts in
the tail, because "have I tried this?" and "what do I currently believe?"
are not the same question.

* **Every past campaign as one line** — target, hypothesis, outcome.
  Complete rather than recent: the campaign that matters is as likely to
  be the third as the fortieth, so a recency window discards on the
  wrong axis here too. It grows linearly and slowly enough not to
  matter — two hundred campaigns is a few thousand words.
* **A self-summary, capped like the findings digest** — the current
  theory of this language, what has been ruled out, where the coverage
  is going. Rewritten at each close.

The one-liners are the anti-repetition mechanism, where coverage beats
depth. The summary is the thing a fresh context most lacks and would
otherwise spend a whole campaign rebuilding. Full campaign records stay
on disk and are read when a one-liner suggests it is worth it.

**What the reset still buys**, and why campaigns are bounded rather than
open-ended: context growth is capped at one campaign rather than one
phase, and a wrong theory cannot survive past the campaign that produced
it. An unbounded tuning session is exactly the failure Ralph exists to
prevent — thirty iterations spent on a premise formed in the first
three, with no mechanism that questions it.

## 5. The auditor, and the conformance loop's number

"Loops until it decides it matches the spec" has an obvious weak point:
the entity deciding is the entity that wrote the code, in the same
context, having already convinced itself.

So the decision is taken away from it. At every **round** close, a
**separate session with no memory of writing the code** is given the
spec and the implementation and asked one question: *is this
implemented, and where is it not?* It cannot edit anything. It answers
in two numbered lists — **gaps** and **minor items** — and that shape is
what turns the audit from a safety net into the measurement the
conformance loop otherwise lacks.

A round is one campaign for a loop that runs one at a time, and N for a
loop running N workers ([section 13](#workers-one-loop-several-campaigns-at-once)) —
which is where the distinction comes from, and it is not a softening.
Three workers auditing their own branches would each judge a tree nobody
ships and would write three verdicts for one section with no rule for
which wins, so the audit runs once, against the merged result. The cost
is that a campaign can close against a verdict older than itself, and
the progress it made is then attributed to whichever campaign closes
after the audit that measures it. That is `harness-003`, answered in
favour of the round: attribution is by named gap rather than by count
delta, which pays most of it off, and §7's stall rule already excludes a
campaign that closed with no audit since it opened, so the cadence
cannot stop the loop by itself.

### Sections clean is the metric

[Section 2](#2-two-loops-two-oracles) concedes that the conformance loop
has no gradient: its properties are pass/fail. The audit supplies the
missing number. **Sections clean is to the conformance loop what
coverage is to a tuning loop** — something that moves campaign by
campaign, that a campaign can be aimed at, and that says whether the
last ten campaigns accomplished anything
([section 3](#sections-clean-is-the-number)).

Two lists rather than one, because they terminate differently:

* **Gaps** — a spec claim is unimplemented, contradicted, or implemented
  in a way that does not satisfy it. This count should go to zero.
* **Minor items** — the claim is satisfied, but the manner invites
  objection: naming, structure, a test that passes for the wrong reason.
  This count should *not* be driven to zero.

**When only minor items remain, the loop stops and asks.** Chasing them
is where a model's judgement is least reliable and where the cost of a
fix is highest relative to its value, so it is the wrong thing to spend
unsupervised campaigns on. A human reads the minor list and decides
which are real. That is a terminal condition rather than an asymptote,
which matters because an asymptote is something a loop will happily
grind against forever.

### Making the count mean something

A count produced by a model's judgement is not a measurement, and two
audits of unchanged code will not agree. Left alone, that variance
swamps the signal — a falling count could be a different auditor mood.

So the audit is asked a question with a stable answer. Not "how many
things are wrong?" but, **per section, "does this section have any
gap?"** The section set is fixed and mechanical, so that is
classification rather than discovery and successive audits are
comparable by construction. The individual gaps are still reported —
they are the work queue — but they are not the number.

Comparing successive audits by gap identity rather than by total is what
makes the difference visible even so: a list that is the previous list
minus what was fixed is progress; a different list of the same length,
in sections that were already dirty, is variance and worth noticing.

### Properties worth keeping

* **Adversarial framing.** "Where does this diverge?" produces findings;
  "check whether this is correct" produces reassurance.
* **Findings are items, not instructions.** They enter the queue and get
  prioritised like anything else. An auditor that directly drives the
  next campaign becomes a second, unaccountable planner.
* **The implementer sees the output, never the prompt.** It has to read
  the gap list — that is the point — but the audit prompt itself is
  `harness/`-owned and not in the implementer's context, for the same
  reason [section 14](#one-prompt-per-variety-of-phase) keeps the gate's
  internals out: a loop that knows exactly how it is judged optimises
  for the judgement.
* **A fresh session of the same model is enough to start.** The failure
  being caught is attention-based — the writer talked itself into an
  interpretation and then read the code through it — not
  capability-based, and a fresh context already destroys the shared
  premise. A second model adds different blind spots rather than more
  capability, and is worth adding only if the auditor is observed
  rubber-stamping. That is a cheap experiment: same prompt, different
  CLI.

The transparency golden tests, the double-response assertion, and the
zero-inspection assertion (`shim.md` §12) are the parts of the spec where the
audit matters least, because they are already mechanical. It matters
most on the prose-shaped claims — "handlers get a snapshot, not a lock",
"the driver must not depend on any language crate" — several of which
are also mechanically checkable if someone writes the check. Converting
an audit finding into a permanent mechanical check is itself a
high-value gap to close, because it moves a claim from the audit's judgement to
the exact one, and the loop should be told so.

## 6. Spec changes: what the loop may decide alone

The loop will find the spec wrong. It is ten thousand lines written before a
line of code, so this is certain, and a loop that must escalate every
inconsistency will escalate constantly and stall.

**Class A — the loop fixes it, records it, continues.** Internal
contradiction; a section reference that does not resolve; a type name
that changed; a claim about a dependency's API that is factually false;
an example that does not compile. The test is: *is there a defensible
answer that does not trade anything off?* Fix, and append to
`state/spec-changelog.md` with the contradiction quoted and the resolution
stated.

**Class A edits are provisional until reviewed**, in the same sense a
Class B provisional choice is: applied immediately so the loop never
idles, tagged in the changelog, and surfaced on the dashboard for the
next batch alongside the escalations
([section 16](#decisions-carry-their-evidence)). The difference from
Class B is only that the loop does not have to *wait* for anyone, not
that nobody looks. An edit nobody was scheduled to read is the one that
lets the spec drift toward whatever was built —
[below](#7-progress-stall-and-the-ways-it-is-faked) has why that
particular drift is invisible to every other mechanism here.

**Class B — escalate, and keep going anyway.** Anything that trades
something off. Specifically, always escalate when the change touches:

* a metric target or budget (the latency numbers, the 97% floor, the
  error severity budgets)
* the `LanguageHandler` seam or any vocabulary type
* the dependency set, or anything in `deps.md` §13
* licensing, or `vendor/`
* one of the numbered open questions in any document

The escalation is a file, `state/decisions/<owner>-NNN.md`
([section 13](#parallel-loops-and-what-they-share) says why the owner
prefix), in **MADR** format
(context / options / decision / consequences — the usual architecture
decision record template, which already has the immutability discipline
this wants: a record's status changes, its text does not). It states
the question, the options, the evidence available, a recommendation,
and — critically — **what the loop is doing meanwhile.** The loop picks
the most reversible option, tags every affected site with
`// DECISION-<owner>-017: provisional`, and continues. It never idles waiting
for an answer.

When the answer arrives, reconciling the tagged sites is a normal
campaign target. `grep -r DECISION-` is the outstanding-provisional-choice
report, and its count is a health metric: rising steadily means the
loop is running ahead of its decisions and the work is getting
speculative.

**Escalations are reviewed in batches, never as interrupts.** Answering
each one as it arrives is unusable at campaign cadence and would make
the operator the rate limiter for every loop at once. So they queue, and
a batch is triggered by whichever comes first: the number of records
**waiting on a human** crossing `escalation_batch` in
`state/phase.toml`, or a phase gate — which is already a synchronisation
point where the loops are quiesced and a human is looking anyway.
`hj escalations` computes it and exits 1 when a batch is due; nothing in
the runner consults that exit code, and nothing should, because a due
batch is a message to the operator and never a stop. With no threshold
set, the phase gate is the only trigger, which is a degenerate cadence
and not a broken one.

**Records waiting, not tagged sites.** An earlier revision made the
trigger "the outstanding `DECISION-` count", meaning the `grep -r
DECISION-` report above. But a record can be waiting with no taggable
site at all, and those are systematically the ones that most need a
human: the choice is about a file the raising loop may not write —
`state/phase.toml`, `.claude/`, another loop's crate — so there was
nowhere to put a tag and no work the loop could do meanwhile. Counting
sites would leave exactly those invisible to the trigger. The grep count
keeps the job it already has, which is the health metric; when the loop
is running ahead of its decisions is a different question from when to
hold a review.

The cost of batching is that the loop runs further on provisional
choices and reconciliation gets more expensive the longer it waits.
That is the trade being taken deliberately, and the outstanding count is
what makes it visible rather than silent. **The unit that expense is
paid in is campaigns, not days** — a quiesced fleet costs nothing to
wait — so `hj escalations` reports, per record, how many campaigns have
closed since it was raised. If reconciliation starts dominating, the
threshold is too high.

**The queue starts empty, and is not seeded from `open-questions.md`.** An
earlier revision said the opposite — that document and `resolution.md`'s
own list are numbered Class B items in everything but format, so converting
them to decision files looked like free enumeration of the loop's
uncertainties.

It is not free, and it inverts what a decision record is for. **Those
questions are the author's**, waiting on measurements and product judgement
that no campaign has; a record exists because a campaign hit something and
could not proceed without choosing. Seeding a hundred of them hands the loop
provisional choices it has no evidence to make, in code it has not written
yet, and destroys the one signal the mechanism produces: a rising
outstanding-`DECISION-` count means the loop is running ahead of its
decisions, and it cannot mean that if the count started at a hundred.

The loop still meets those questions — by reading the document a gap points
it at, where they sit in context with the reasoning around them.

## 7. Progress, stall, and the ways it is faked

**Progress** is any of: a section going clean; a test count
increasing; the audit's gap count falling
([section 5](#sections-clean-is-the-metric)); a frontier point being added
that is not dominated by an existing one; a decision item being
resolved. All five are computable
from the repository, which is the point — the loop does not get to
assess its own progress in prose.

**Stall is judged per campaign, not per experiment.** An experiment that
reverts is a result — a falsified variant is what a campaign is for — so
the per-iteration rule that made sense under pure Ralph would fire
constantly here. What counts is a *campaign* closing with none of the
five forms of progress, and N of those in a row. **N is 3 for a
conformance loop and 5 for a tuning loop.** The asymmetry is the point:
a tuning campaign legitimately spends several rounds on a restructuring
that only pays off at the end, so a tight threshold would kill the
campaigns most worth running. A conformance loop making no progress on
one gap three times running has hit something the spec did not
anticipate, and the sooner that reaches a human the better.

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

Both are `hj stall`'s and neither stops anything. What each *means* here
needs saying, because ralph's vocabulary does not map onto this one:

* **Test-only** is per commit, since [section 4](#4-the-iteration-contract)
  makes one commit per experiment, and means a commit that touched Rust
  tests and **nothing else** — not merely no source. A commit that adds a
  test and edits `design/` did other work, and for a conformance loop that
  other work is usually the point: the test carries the claim the spec edit
  settles. The loose reading measured 71% against a loop that was closing
  gaps at the time, and a flag that fires on campaigns doing their job is
  one nobody keeps believing. Commits touching no Rust at all sit outside
  the denominator rather than counting as healthy — a loop whose tests live
  *inside* its implementation, as this harness's do in `hj selftest`, is one
  the measure cannot see, and it says so rather than reporting 0%.
* **A "done" signal** here is a campaign closing `confirmed` or `partial` —
  a claim of movement — when the repository shows none. The cap is not a new
  rule: such a campaign already increments the no-progress count above and
  stops the loop at N like any other. What is added is the *distinction*,
  because the two shapes stop the loop identically and mean opposite things.
  A run of honest `no-movement` closes is a loop that has hit something the
  spec did not anticipate, which is exactly what should reach a human; a run
  of `confirmed` closes with nothing behind them is a loop that has decided
  it is finished, which is the failure this heuristic is named for.

The failure this is guarding against is not idleness — it is the loop
generating plausible activity indefinitely. Which brings up the ways
the metrics can be satisfied without work being done:

| Gaming route | Countermeasure |
|---|---|
| Delete or weaken tests | Test count is a ratchet; test *deletions* are flagged for review regardless of count |
| Rewrite the gate script | `harness/` is owned by nobody and denied to every loop; changes to it are Class B |
| Rewrite the spec to match the code | Class A/B split, plus scheduled review of `state/spec-changelog.md`; **any campaign whose commits include both a `design/` edit and code under the same anchor is flagged**, whether or not they share a commit. See below — this one has no other defence |
| Tune to the corpus | Held-out repos are in a corpus split the loop is never given ([section 12](#12-held-out-integrity)) |
| `cargo insta accept` a metric regression | The gate checks metric *direction* itself; insta pins the table's shape, not its values |
| Split one item into ten to show motion | Progress is sections clean over sections total ([section 3](#sections-clean-is-the-number)); the denominator is mechanical, so splitting a gap in two moves nothing |
| Write a test that passes trivially | The audit judges the claim, not the test; a passing test with an unsatisfied claim is still a gap |
| Add a language to show progress | Forbidden outright. A new `crates/lang_*` is outside every loop's owned paths, so the gate rejects it |

**One of those rows is weaker than the others and it is worth saying
why.** Rewriting the spec toward the code is the only entry on the list
that the audit cannot catch, and it cannot catch it *by construction*:
the auditor compares the implementation against the spec as it currently
reads, so an edit that moves the spec makes the gap disappear from the
one instrument that would have reported it. The section then goes clean
and the number goes up. Every other row here has a second mechanism
behind it; this one has only the changelog, which is why
[section 6](#6-spec-changes-what-the-loop-may-decide-alone) now schedules
a human to read it rather than leaving it to be found.

The detector is also scoped to the **campaign**, not the commit, and that
matters more than it sounds. The iteration contract deliberately produces
one commit per experiment with one target each
([section 4](#4-the-iteration-contract)), so "the spec edit and the code
are in the same commit" was a test the loop's own working style defeats
without anybody intending to. A campaign already records which commits
belong to it, so widening the window costs nothing and closes the gap.

A related consequence worth carrying: the conformance loop's number is
*sections clean over sections total*
([section 3](#sections-clean-is-the-number)), and the section list is
parsed from headings in documents the loop may edit. So the denominator
should be snapshotted in `harness/` at the start of a phase rather than
re-derived each audit — otherwise merging two dirty sections into one
clean one moves the score without touching a line of code.

None of these is airtight against a determined optimiser. They are
airtight enough against an *undirected* one, which is the actual risk:
the loop is not adversarial, it is just weakly grounded, and weak
grounding drifts toward whatever is easiest to satisfy.

## 8. Sequencing and gates

The phase structure is [`phases.md`](phases.md).
It is short and it is the authority; this section says what each phase
means for the loops and what its gate is.

**Phase 1a — core needed for measurement.** [`core.md`](core.md) in its
entirety, which is the document's whole scope: `vendor/rope`, `sum_tree`,
the newtype work in `rope-modifications.md` (which folds Zed's `util`
items into rope rather than vendoring a third crate), `shared` (seam,
vocabulary, `ProjectView`, the client-side subset of `proto`), the
framing codec, and `measure_core` itself. Explicitly **not** the router,
the health model, the actor, parallel dispatch, standalone, or divergence
reporting — all of which are [`shim.md`](shim.md) and phase 2b. Parallel
dispatch and **not** the handler registry: `shim.md` §13 puts both under
`dispatch/`, but the registry is `core.md` §1's — "the driver resolves an
incoming LSP `languageId` against the registry and gets
`Option<LanguageId>`", which is what keeps `driver` free of a build
dependency on every grammar crate — so `core.md` in its entirety includes
it, and what 2b holds is `shim.md` §10's bounded pool and its fan-out to
several servers.

**Where the rest of that line falls is not settled, and the phase 1a tree
already crosses it.** `core.md` §5's deadline, §6's agreement predicate
and §7's per-query record are `core.md`'s claims, and each needs a single
owner of the state it reads — which is the file `shim.md` §13 calls
`actor.rs`. The tree contains that file, and a `Mode::Standalone`, and a
`Divergence`, all three excluded by name above and none of them covered
by a document the phase 1a audit reads. That is
`state/decisions/harness-007.md`; until it is answered this list stands
as written rather than being widened to fit what was built. So "what is
in phase 1a" is a question with a file for an answer *except* at that
seam, which is the one place it is still a list maintained here.
Gate: workspace builds, upstream rope tests pass unchanged,
position-encoding property tests pass, `measure_core` drives a real
server end to end on one repository.
*Conformance loop*, per [section 18](#18-scope-phases-1-and-15-first) —
this is the phase the loop machinery is built for and first run against.

**With one thing carved out: the seam.** `LanguageHandler`, `Query`,
`Outcome`, `ProjectView`, and the vocabulary newtypes are decided here,
they are frozen at this gate, and getting them wrong is expensive
downstream in a way no loop will notice — a language loop cannot
observe that the seam made its job harder, it can only be slow. So a
commit that changes a seam type is a **Class B escalation even during
1a**, when it is otherwise the loop's own crate. The loop proposes and
keeps going on its provisional choice; a human rules at the batch. That
is the narrowest form of the supervision an earlier revision of this
section asked for as a blanket rule, and it puts the review exactly
where the irreversibility is rather than over the whole phase.

**Phase 1b — repo collection**, concurrently. Needs no code at all, so
it starts on day one. C, C++, Go, JavaScript, TypeScript/TSX, Rust,
Python; medium-sized, popular, trustworthy, spread across domains and
styles.
Gate: repositories checked out at pinned commits, **and the tuning /
held-out split decided and physically separated**
([section 12](#12-held-out-integrity)). That split has to be made here,
not later: once a repository has been in the tuning corpus, moving it to
held-out does not un-teach it. Carving a *final* set out of the held-out
half is deliberately **not** part of this gate — §12 leaves it until the
first phase gates say how much leakage selection actually causes, and it
stays available to be made then precisely because a finer split comes out
of held-out and never out of tuning.

**Phase 1c — LSP installation**, concurrently. Every trustworthy server
Zed supports for these languages, installed, pinned, and documented in
`external-dependencies.md`. Human intervention expected; several of
these are not a package-manager one-liner.

**Phase 1.5 — ground truth collection.** Every language server on every
repository. Depends on all three of 1a (for `measure collect`), 1b (for
the repositories), and 1c (for the servers), which is what makes it a
distinct phase rather than a task inside any of them.
[`data-collection.md`](data-collection.md) is the design.
Gate: a `truth.jsonl` per (repository, server) with a valid provenance
header, and `measure replay` reproducing the recorded positions.

This is the plan's long pole and its highest-uncertainty item — roughly
a hundred machine-hours, seven languages, more servers than languages,
and no useful fallback if the data turns out unusable. It costs almost
no tokens and almost no model time, which is exactly why it is easy to
under-scope: it is invisible to every other kind of accounting
([section 15](#15-cost-and-timing)).

**Phase 2a — per-language quality loops**, one per language, in
parallel. **One loop per language, not one per (language, server)** — a
language with two usable servers has two oracles and two metric tables,
and one optimiser that reads both
([section 10](#several-servers-do-not-mean-several-loops)). Each starts by instantiating the language-crate template
from phase 1a — whose default handler resolves nothing, so the first
measurable point is a real zero rather than a build error. Top-1
agreement and coverage only; cost metrics recorded, never gated.
Gate: the frontier stops advancing, candidates are re-measured on the
held-out corpus, and **a human picks the point**
([section 10](#selecting-a-version-at-a-phase-gate)).

**Phase 2b — the LSP shim**, concurrently. Everything phase 1a deferred,
which is [`shim.md`](shim.md) end to end.
Gate: transparency golden tests, server-originated round-trips, protocol
race tests, double-response assertion, codec fuzz.
*Conformance loop.* The largest single body of work, and it has no
gradient, so it lives or dies on the audit being honest.

**Phase 3 — whole-repository optimisation.** Latency, binary size, line
count, cross-language, serial, single writer. This is where extraction
of shared resolution code finally happens
([section 13](#13-shared-code-and-when-it-may-exist)), and it runs under
an equality constraint
([section 10](#phase-3-is-a-refactor-under-an-exact-oracle)).

**Phases 4 through 7 — the same shape, at Zed's full language set.**
Repo collection and LSP installation (4), ground truth (5), per-language
loops (6), whole-repository optimisation (7). Two differences in phase 6
that matter to this document:

* **The shared library now exists and is read-only.** Phase 3 created
  it; phase 6 loops may call into it and may not change it. That
  generalises the rule in [section 13](#13-shared-code-and-when-it-may-exist)
  rather than contradicting it — shared resolution code is *writable*
  only during whole-repository phases, and *readable* always. A phase 6
  language that needs something different still writes it locally and
  leaves the duplication for phase 7.
* **Parallelism has to be bounded.** Seven languages in parallel is one
  thing; Zed's full set is another, and the limit is the machine rather
  than the design. The scheduler runs a fixed pool of concurrent loops
  and rotates languages through it, which changes nothing about
  isolation and everything about wall-clock estimates
  ([section 15](#15-cost-and-timing)).

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
one consumer built, and 2b discovers it needs something else. `core.md` §1
specifies both sides already, so this is a review-discipline problem at
the 1a gate rather than an ordering problem — but it is the thing to be
most careful about there.

## 9. The inner loop is a replay, and its cost is measured

A metric loop whose iteration takes four hours is not a loop. The
corpus scan drives a real language server over ten repositories; that
is hours, and it must happen approximately never.

So ground truth is a **frozen artifact**. `measure collect` runs once per
(repo commit, server version) and writes `truth.jsonl`: every
identifier position, the LSP's answer, the LSP's latency. Tuning
iterations run `measure replay`, which launches no language server at all
— it replays handlers against the frozen positions and compares to the
frozen answers. `core.md` §7 specifies both modes.

### There is no replay-time target, and that is deliberate

An earlier revision set one — a full replay over one language's tuning
corpus in under a minute — and it is removed rather than relaxed.

The number could not have been justified. Replay cost is dominated by how
often a query falls through to whole-project search, by how much of that
survives `DefinitionHints`, and by how much of a repository stays resident
in the parse LRU across queries. All three are properties of a handler and
a corpus that do not exist yet, and the search is exhaustive by
construction (`resolution.md` §1.3), so the cost is not something a
constant can be chosen to fit. A target picked now would either be met
trivially and mean nothing, or be missed on the first real run and demand
a design change to satisfy a number nobody measured.

**So it is measured instead.** `measure replay` reports its own wall clock,
and the harness records it as an ordinary metric alongside the work
counters ([section 10](#the-metrics-history)) — per language, per commit,
from the very first run. Three things it is used for:

* **Calibration, not gating.** The first ten campaigns already exist to
  replace estimates with measurements
  ([section 15](#estimates-and-replacing-them-with-measurements)); replay
  wall clock is one of them.
* **Noticing the trend.** A replay that doubles over a phase is a finding
  about the handler's search behaviour, visible in the same row as the
  coverage it bought. That is more useful than a threshold, because it
  attributes the cost to a diff.
* **Deciding what to do about it, later and with data.** If replay turns
  out to be slow enough to bound iteration, the options are real and
  choosable at that point — replay a sample in the inner loop with the
  full corpus at gates, warm the parse LRU across a run, or reinstate a
  deterministic work budget. Choosing between them now would be choosing
  without the one number that decides it.

The one thing that does **not** depend on how long a replay takes is
correctness: a slow replay is a slow loop, never a wrong number.

That matters for a second reason, which
[section 10](#10-objectives-phases-and-the-frontier) needs: a
deterministic replay means the *entire metric history is recomputable*.
Any past commit can be re-measured on demand, so a change to how a metric
is defined does not silently invalidate everything recorded before it — it
triggers a sweep. Whether that sweep is affordable is exactly the
measurement above; whether it is *sound* is the determinism below. Without
soundness, the first metric redefinition throws away the frontier.

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

`core.md` §7 now requires replay to enforce **no deadline at all**, and
`resolution.md` §1.3 makes a search exhaustive — it reads every candidate
file and stops when it runs out of them. So there is no stopping rule
left for machine load to perturb, and determinism is structural rather
than calibrated. An earlier revision got there by substituting a
reproducible byte budget for the clock, which worked but had to be
calibrated against a wall-clock deadline to mean anything.

Given that, quality-phase iterations need no coordination with each
other whatsoever, which is what makes
[section 13](#parallel-loops-and-what-they-share) cheap.

The price is that replay reports an **upper bound**: the shim has a
deadline and will sometimes abstain where replay answered. That gap is a
latency fact, and this document already measures latency separately and
for exactly this reason — work counters per iteration, wall clock at
gates ([section 10](#what-cannot-be-measured-in-isolation)).

The metrics that *cannot* be made deterministic or local are handled
separately, in [section 10](#what-cannot-be-measured-in-isolation).

## 10. Objectives, phases, and the frontier

The loop needs to know what it is maximising, and the honest answer
changes over the life of the project. Trying to express it as one
weighted score from the start requires an exchange rate between a
coverage point and a kilobyte that nobody can currently justify.

So there are **two objective regimes**, and they are phases 2a and 3:

* **Phase 2a, quality.** Precision and recall, and nothing else. Cost
  metrics are *recorded* and never *gated*. One loop per language,
  parallel; metrics still per (language, server).
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

The rule from [`phases.md`](phases.md):
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
`resolution.md` §11 requires the handler to be deterministic, and
[section 9](#determinism-is-a-precondition-not-a-description) requires
replay to run without a clock. Without the second, "the outputs did not
change" would be a statistical claim about a machine's load; with it, it
is an equality check.

**The gate has no carve-out: any difference fails it.** An earlier
revision exempted one class — a query that had exhausted its per-query
byte budget and now completed, since making the search cheaper
legitimately changed its answer — and administered the exemption with a
`truncated` flag on the record. `resolution.md` §1.3 removes the budget,
so that class no longer exists and neither does the flag. The
constraint is now the strongest form it can take, and needs nothing
recorded to enforce.

That also removes the one place a human had to adjudicate "is this the
good kind of difference?" during a phase whose whole appeal was that it
required no judgment.

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
commit, phase, per-stratum {coverage, top1, contained, result-count
distribution, n},
conformance loops instead: {sections clean/total, open gaps, audit minors},
work counters (bytes read, files parsed, nodes visited),
replay wall clock, per-stratum heuristic latency percentiles and the
per-stage breakdown, deadline-abstention rate,
measure_<lang> stripped size, lang_<lang> crate contribution,
LOC per crate, test count
```

**The wall-clock numbers are in the row despite being machine-dependent**,
which looks like it contradicts the rule below that they are taken at gates. It does not, because it is not measuring the same thing: the
latency numbers below are a claim about the *shipped tool* and have to be
comparable, whereas this is a claim about *how long the loop's own feedback
took* and only ever needs to be right to within a factor. It is noisy under
parallel loops and should be read as a trend, never gated on
([section 9](#there-is-no-replay-time-target-and-that-is-deliberate)).

The same applies to the per-query latency figures `core.md` §7 records —
per-stratum percentiles, the per-stage breakdown, and the rate of
`AbstainReason::Deadline`. They are written down because they can be, and
because a number nobody recorded is a number nobody can go back for; they are
not gated, not on the frontier, and not trustworthy to better than a factor
while the loops run in parallel. The authoritative latency measurement is
still the quiet-machine pass at a gate. **The deadline-abstention rate is the
one worth watching most**, since it is the whole of the gap between what
replay reports and what the shim would deliver
(`resolution.md` open question 15).

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
that is what the crate split in `core.md` §7 is for — so it goes in every
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

**The frontier is a phase 2a object: top-1 agreement × coverage.**

Which of the three quality numbers goes on the axis is decided by the
same criterion that rejected plain match rate: **it must not be
improvable by answering more.** The tool now returns every plausible
candidate as a ranked list (`high-level.md`, "Several candidates"),
which gives three numbers instead of one, and only one of them survives
that test.

* **Top-1 agreement** — the first location matches. Returning more
  candidates cannot improve it. This is the axis.
* **Containment** — the answer is somewhere in the list. Rises
  monotonically with list length, so as an objective it is the old flaw
  wearing new clothes.
* **Result count** — the price of containment.

So containment and result count are *reported beside* the frontier and
never on it, the same treatment latency and LOC get for the same kind of
reason. A point that raises containment by lengthening lists is not a
frontier advance and must not read as one.

The list cap (`open-questions.md` question 12) is a constraint, like the
latency budget: enforced, reported, not optimised.

Phase 3 has no frontier of its own, because it may not move quality at
all ([above](#phase-3-is-a-refactor-under-an-exact-oracle)). It has one
axis, size, and a ratchet. The frontier reappears there only inside an
escalation, to show what a proposed exception would cost.

Two things stay off the axes deliberately:

* **Latency is a constraint, not an objective.** The design already
  converts it: past the hard cap a query abstains, so blown latency
  *spends itself as lost recall* and is already visible on the quality
  axis. Putting it on a third axis double-counts it. Gate it against
  `high-level.md`'s budgets, report the percentiles, and leave it there.
* **LOC is not on the frontier.** It correlates with binary size, it is
  gameable by formatting, and what it is really a proxy for —
  maintainability — is not something a frontier can see. Report it;
  let review use it.

### Several servers do not mean several loops

A language with two usable servers has two oracles and two metric tables
(`core.md` §7), which threatens either a four-dimensional frontier or two
loops contending on one crate. It needs to be neither, and the way out is
to notice that the two oracles disagree about only a small, specific part
of the corpus:

* **Shared handler logic** is evaluated on the positions where *every*
  server for that language agrees. That is the bulk of the corpus, the
  axes mean the same thing regardless of deployment, and it is **the
  frontier** — one per language, still 2D.
* **A `ServerProfile`** is evaluated on the positions where servers
  differ, against that server alone. Changing one profile cannot move
  another server's numbers, because no other server's queries are in its
  evaluation set. Those numbers are *reported beside* the frontier, the
  same treatment containment and result count get.

So the frontier stays one per language and two-dimensional however many
servers there are, and a joint objective — which would have to weight
servers by expected deployment share, a number nobody has — is never
needed.

**One loop owns both surfaces.** An earlier revision split them, giving
each server its own loop owning its own profile file, on the grounds that
disjoint evaluation sets make genuinely independent work. The evaluation
argument was right and the conclusion did not follow. A per-server loop
owns one file it is forbidden to grow except on corpus evidence
(`core.md` §7, `resolution.md` §1.4), and is forbidden the one shape —
`if server.id == PYRIGHT` — that would let it do anything locally, so it
is a loop with almost no legal move. Meanwhile the language loop beside
it does all the work and cannot see the profile, which is where a
divergence it just caused would show up.

So a language is one loop, and it reads every table for its language. The
decomposition survives intact as an *evaluation* rule, which is what it
was actually good for: tune shared logic where the servers agree, tune a
profile where they do not, and never average the two.

For a language with one usable server — Rust, Go — every position is
trivially unanimous, there is no profile, and none of this machinery does
anything. That is the intended behaviour, not a special case.

It also assigns each surface the data that actually determines it.
Tuning shared logic against a single server's full corpus would bake
that server's conventions into code that runs behind all of them; the
agreement subset is exactly the part where "correct" is not a matter of
opinion.

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
source* forward — not resetting the branch. The audit state, the journal,
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

**Most, but not all.** Phase 2a also carries a standing ceiling, set an
order of magnitude above where anything reasonable sits, on
`measure_<lang>` size and on per-query work. It is a guardrail and
emphatically not a ratchet: crossing it does not mean the last change
was wrong, it means the loop has wandered somewhere no legitimate
experiment goes, and it stops and escalates rather than failing the
gate.

The reason to have one despite the argument above is the
incompressible-peak failure in [section 19](#19-how-this-goes-wrong) —
coverage reachable only through a structure phase 3 cannot compress,
discovered when phase 3 is already underway and the alternative is
giving back the coverage. A limit that loose suppresses no real
experiment; it only catches the pathological version, which is exactly
what a guardrail should do. Being an order of magnitude out is also the
kind of thing worth knowing within a campaign rather than at a gate.

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

`high-level.md`'s development plan holds out five of the ten repositories
per language and calls the tuned/held-out gap the overfitting signal. Under
autonomous loops this needs teeth, because "learning a particular
repo's conventions is the default outcome rather than a risk" is
already the stated expectation for human-driven sessions, and a loop
runs a hundred times more iterations.

* **Held-out repositories and their `truth.jsonl` live in their own
  split** — `../heuristic-jump-corpus/test/`, a sibling of `training/`
  rather than a subdirectory of it (`core.md` §7). Not a convention:
  the loop is given `--corpus` pointing at one and never the other, and
  a rule it is never told about is a rule it cannot weigh against
  making the number go up.
* **The separation must be physical, not rule-based.** Claude Code's
  `denyRead` rules block the built-in file tools but do not stop `cat`
  in a bash subprocess, so a deny rule is defence in depth and never
  the boundary itself. The boundary is that the data is outside the
  checkout entirely and the held-out path is never passed in.
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

**The split is five and five** (`data-collection.md` §1), and held-out
is therefore a *selection* set rather than an untouched one. Once it
picks the version that proceeds at every phase gate it is being
optimised against — slowly, at a coarse cadence, and by a human, but
optimised against. Over ten gates that is real leakage.

The remedy, if it starts to matter, is to stop selecting on part of it:
carve a final set out of the five, evaluate it once at the end, and
never let it choose anything. That stays available precisely because
those repositories have never been in the tuning corpus — **the split
can be made finer later and never coarser.** Deciding it now would mean
guessing how much leakage ten gates actually cause, which the first few
gates will say.

## 13. Shared code, and when it may exist

The question the whole parallel-language plan turns on, and the answer
is more restrictive than an earlier draft of this document had it:
**during phase 2 there is no shared resolution code at all.**

### Three tiers

**`shared` is spec.** `LanguageHandler`, the vocabulary newtypes,
`ProjectView`, `proto`, `Error`. `core.md` §1 is a design commitment and
every crate depends on it. Language loops may not edit it, in any phase.
The seam half is frozen at the end of phase 1a; a change to it is a
Class B escalation, deliberately expensive, because a cheap seam change
is a seam that erodes.

**`similarity` is ported, and frozen for the duration of phase 2.**
Only what comes across from the prior implementation —
`Occurrences`, `IdentifierParts`, path–namespace scoring
(`resolution.md` §5). It can be shared during phase 2 precisely because
it is *not* being written during phase 2: a known-good body of code that
predates every language crate generates no churn and no cross-language
coupling.

**Phase 3 may write it**, which follows from the general rule below
rather than being an exception to it — `similarity` is shared resolution
code, and phase 3 is a whole-repository phase. It is also the natural
destination for what extraction produces, so phase 3 grows this crate
rather than spawning a second shared one and inventing a boundary
between them. If what lands there outgrows the name, renaming is part of
the extraction and costs nothing, since phase 3 is output-preserving
anyway.

**Everything else is per-language, and duplication is left standing.**
Two languages that need the same helper each write their own. No
promotion, no shared utility crate, no proposals acted on.

Stated as one rule that covers every phase: **shared resolution code is
writable only during whole-repository phases, and readable always.**
Before phase 3 there is none to read; after it, phase 6 languages call
into it and still may not change it. The writable window is always a
phase with one writer and nothing running alongside.

### Why no shared resolution code during phase 2

`resolution.md` §9 already argues that a shared-utility layer
designed before any language exists is "a framework wearing a different
hat," and that sharing should be derived from working handlers. Running
the language loops concurrently sharpens that from a design preference
into a hard constraint, for a reason §9 did not have to consider:

* **A shared crate is a surface two writers contend on.** Everything
  else in this design is partitioned so that no two loops ever write the
  same thing. A live shared resolution crate would be the single exception,
  and it
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

One writer, nothing running alongside, `crates/similarity/` writable,
and the same equality constraint as everything else in phase 3: a
promotion must leave every affected language's replay outputs
byte-identical
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
crates it owns and builds only those. `core.md` §7's `measure_core` +
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
their dir" literally.

**The list is per checkout, not per crate** (`harness-002`). It is every
worktree, the integration checkout's git directory and `state/`, the
transcript and log roots, and `~/.cargo`; everything outside those —
`$HOME`, `~/.ssh`, and `.claude/` itself, so a loop cannot reach the file
that configures its own sandbox — is read-only to the session. An earlier
revision of this paragraph named the owned crate directory, which is
narrower than the ownership table beside it and does not survive contact
with the deployment: `harness/workers` runs in the integration checkout
and writes into *each* worker's worktree when it fast-forwards them after
an audit, so a list holding only the session's own project root breaks the
round runner rather than a campaign. `~/.cargo` is on it because `cargo`
writes its registry cache and `.package-cache` lock there and the gate's
build step fails without it.

What that concedes is that a campaign can write another loop's files
*inside its own checkout*, `design/` included. Layer 4 catches that at
commit time, which is the division of labour throughout: this layer stops
a session escaping its tree, and the gate decides what may be in it.
`failIfUnavailable` is set, so a machine without bubblewrap stops rather
than running every campaign unsandboxed behind a warning — the same
posture as abstaining rather than guessing, applied to the harness.

**4. Gate diff scope.** The commit touches only owned paths, checked
after the fact by the gate. Authoritative, because it inspects the
result rather than trusting the actor:

  | Loop | May write |
  |---|---|
  | conformance | `vendor/`, `crates/{shared,driver,heuristic_jump,measure_*}/`, `design/` |
  | lang-rust | `crates/lang_rust/`, `state/{metrics,journal,decisions}/rust*` |
  | lang-python | `crates/lang_python/`, `state/…/python*` |
  | phase 3 | everything except `harness/`. One writer, nothing running alongside |
  | *nobody, ever* | `harness/` |

  And one row that is not about a loop at all, because it is the only
  entry whose answer depends on which phase is current:

  | Path | Writable |
  |---|---|
  | `crates/similarity/` | whole-repository phases only — 3 and 7. Denied to every loop that runs alongside another |

`design/` is writable by the conformance loop because Class A spec fixes
edit the design documents themselves
([section 6](#6-spec-changes-what-the-loop-may-decide-alone)). Class B
changes are still escalations — the write access is what makes fixing a
contradiction possible, not permission to decide one.

`harness/` has one bounded exception, and only one: while the
conformance loop is building the phase-2 machinery
([section 18](#the-conformance-loop-builds-the-followup)) it may write
the parts that will judge *later* loops, and never the gate, the prompts,
or the auditor that judge it now.

**A language is one loop, servers and all.** An earlier revision split
`lang_python/` between a language loop and a per-server profile loop, on
the grounds that the two are evaluated on disjoint corpus positions
([section 10](#several-servers-do-not-mean-several-loops)) and so are
independent work. Disjoint evaluation sets did not make it independent
*work*: a profile loop owns one file it may not grow except on corpus
evidence and may not branch on server identity at all, which leaves it
almost no legal move, while the loop beside it changes the shared logic
whose divergences the profile exists to absorb. So the whole crate has
one owner, and it reads every metric table for its language.

Ownership is still by path rather than by crate, because the state
directories partition that way and a language's `state/…/{language}*`
files are what keep two *languages* from colliding.

Two rows carry most of the weight. **`measure_core` and
`measure_<lang>` belong to the conformance loop**, never to a language
loop: a loop must not own the code that scores it, and the four-line
binary is written once when the language is added and never again.
**`harness/` is owned by nobody** — the gate script, the ratchet
baselines, the frontier tool, and the held-out runner live there, and
every loop is denied writes to it. Changes are Class B, made by a
human.

`crates/similarity/` used to share that row, and it was wrong to put it
there: the phase 3 row granted it and the *nobody* row denied it, in
the same table, and the table is the enforcement mechanism rather than
a summary of the prose. It gets its own row above because it is the one
path whose owner is a function of the phase — nobody during 2a, the
optimisation loop during 3 and 7 — which is just the general rule
([above](#three-tiers)) stated where the gate can read it: **shared
resolution code is writable only during whole-repository phases.** The
gate resolves the table against `state/phase.toml`, so there is one
answer at any moment.

But a loop may **request** one, and needs to be able to. The harness is
the loop's entire interface to feedback: if `harness/measure` does not
report the number a campaign needs, or the gate rejects something it
should allow, the loop is blocked in a way it cannot fix and — worse —
can route around. **A silent workaround is the failure being prevented
here.** A campaign that computes a metric its own way because the
harness would not give it one has quietly forked the measurement, and
nothing downstream can tell.

So a harness request is an ordinary Class B escalation with its own
kind: a `state/decisions/` file naming the capability, the campaign that
needed it, and the workaround in force meanwhile — the same
provisional-choice shape as any other decision, so it needs no separate
queue, no separate review, and no new machinery on the dashboard. The
loop continues; a human changes the harness or refuses.

Two consequences. Recurring requests are a signal in the same way
recurring decision kinds are ([section 16](#every-intervention-is-logged)):
five campaigns asking for the same number means the harness is wrong,
not that the campaigns are demanding. And **a harness change that
touches measurement is a metric redefinition** — it invalidates
comparability across the change exactly as
[section 10](#the-metrics-history) describes, and triggers the same
recompute sweep. Changes to gate ergonomics or the dashboard do not.

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
data: **partition shared state by owner** — and, for anything a *second*
worker must read before the first has merged, keep it out of the branches
altogether. `state/phase.toml`, `state/sessions.jsonl`,
`state/assignments/` and the claim files live in the integration checkout
for that reason: desired state that a worker reads from its own branch
cannot be changed by pausing the fleet, and a claim nobody else can see is
not a claim.

* `state/metrics/<language>.jsonl` — one file per loop, never shared.
* `state/journal/<language>.md` — likewise.
* `state/decisions/<owner>-NNN.md` — owner-prefixed, so two loops
  cannot claim the same number at the same moment. A bare incrementing
  id is exactly the kind of thing that looks fine until two sessions
  allocate `007` four seconds apart.

With code and state both partitioned, nothing two loops write ever
overlaps.

### Branches exist for one commit at a time

The goal is that everything lands on `main` and no long-lived
branches accumulate. That is right — divergent branches are where
integration debt comes from, and none of the reasons to keep one apply
here.

It cannot be done by pointing every loop at `main` directly, for a
mechanical reason: **git refuses to check out the same branch in two
worktrees.** And sharing a single working tree between concurrent loops
is worse than the branch it avoids — loop B would compile loop A's
half-written files, so A's transient breakage becomes B's gate failure,
and green-or-revert stops meaning anything.

So each loop has a branch, and **merges when a campaign closes** rather
than at a phase gate:

```
campaign closes -> rebase onto main -> fast-forward main -> continue
```

**Once per campaign, and not once per commit**, which an earlier
revision of this subsection asked for. It cannot be per commit: the
merge rebases the branch onto `main`, and doing that mid-campaign swaps
the working tree under a session that is reasoning from it — including
`state/audit/`, which holds the gap list the campaign was given and is
the only oracle it has. The property this subsection wants survives
intact, because a campaign is hours rather than weeks: nothing
long-lived accumulates, and `main` receives a campaign's work as soon as
it is judged. What follows is that merging is a step in the **campaign**
contract; [section 4](#4-the-iteration-contract)'s iteration contract has
no merge step and should not grow one. There is still no merge queue and
no integration loop.

Since code and state are partitioned by owner, the rebase between
*loops* is conflict-free by construction. **Between workers of one loop
it is not**, and that is where the guarantee stops: they read one gap
list and write one document set, so a conflict there is a rare event
with a handler rather than an impossibility
([below](#workers-one-loop-several-campaigns-at-once)). Two consequences
of that concurrency change what `main` looks like and are worth stating
here rather than leaving to the code: the fast-forward is **serialised
by a lock**, so a worker that read `main` a moment before another moved
it waits and then rebases onto what landed, which is the intended
sequence rather than a retry; and **linear history is traded for a merge
commit** once a branch is far enough ahead that rebasing would
re-resolve the same conflict on every commit it replays. The merge
commit is then the record that the branch diverged, which is worth
having.

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

### Workers: one loop, several campaigns at once

[Above](#parallel-loops-and-what-they-share) parallelises *across* loops —
one per language, disjoint crates. A loop may also be parallelised *within*
itself: **N workers, each running one campaign at a time, in its own
worktree on its own branch, all against the same document set and the same
gap list.**

The reason is throughput against a queue that is deeper than one worker
can serve. A conformance loop over three documents has tens of open gaps
and a campaign closes one to four of them, so the queue is the constraint
rather than the reading — and unlike the across-loops case, there is no
crate boundary that makes the work disjoint for free. Workers are a
throughput lever and nothing else; they do not make any single campaign
better, and they multiply spend by N.

**Naming.** A worker is `<loop>-<n>` — `core-1`, `core-2`, `core-3`. That
is the branch suffix, the worktree suffix and the campaign record's
`worker` field. It is not a separate loop: it shares the loop's prompt,
its documents, its baseline and its number.

#### Disjointness is planned, then claimed

The across-loops case gets disjointness from the filesystem: two language
crates cannot collide. Workers have no such boundary — they read the same
gap list, and the obvious failure is three workers opening the same
highest-value gap within a second of each other and doing it three times.

So a round is **divided before it starts**, by a short read-only session:

* `harness/workers` runs a **planner** per round. It reads the open gaps and
  the code, and writes one assignment per worker — grouped so that each
  worker's targets share their reading and no two workers share theirs. It
  is a session rather than a rule because "which of these are the same
  file" is a judgement about the code, and a first-come rule gets
  disjointness and nothing else.
* **An empty assignment is a real answer.** Three workers and two
  independent targets should be two workers and an idle one; a session
  spent inventing work costs what one spent doing it costs.
* A campaign that wants a target **outside** its assignment asks for it:
  `hj claim <loop> --campaign <id> <target>`, which grants or refuses
  against what live campaigns hold. One file per target created `O_EXCL`,
  so two workers asking at the same instant cannot both win. A claim is
  released at close, and a claim held by a campaign that is no longer alive
  is stale and taken.

**The backstop is the rebase.** If two workers do touch the same file, the
second to finish fails to rebase onto the first, and that is a loud,
local, recoverable failure with a diff attached. Section 13's
"conflict-free by construction" is a claim about *loops* and does not hold
between workers; between workers, conflict is a rare event with a handler
rather than an impossibility.

**Claiming is per gap, not per section.** Two workers in the same section
on different gaps is normal and often good — they share the reading, which
is the expensive part, without doing the same work.

**A conflict that is not a judgement is resolved rather than escalated.**
`state/audit/**` takes the integration checkout's copy, since only the round
runner audits; `state/findings/*` takes the worker's, since it is that
worker's digest of its own campaign. Everything else — `crates/`, `vendor/`,
`design/` — escalates as a blocked item, because there the conflict *is* the
finding: two workers wrote one file, which is a failure of the plan that
divided them, and picking a side destroys the evidence and loses a campaign.

#### What is per worker and what is shared

The rule from [above](#parallel-loops-and-what-they-share) is that shared
mutable state is partitioned by owner. Within a loop the owner is the
*loop*, not the worker, so most state stays shared and the partitioning
happens on the files where concurrent writes actually collide:

| State | Scope | Why |
|---|---|---|
| Worktree, branch | per worker | git refuses one branch in two worktrees |
| `state/campaigns/<loop>/<id>.md` | shared directory | ids are unique; no two workers write one file |
| `state/journal/<loop>-<n>.md` | per worker | append-only prose; a shared file conflicts on every merge |
| `state/findings/<loop>-<n>.md` | per worker | see below |
| `state/metrics/<loop>.jsonl`, `cost/` | shared | append-only, union-merged |
| `state/decisions/<loop>-NNN.md` | shared | the id is allocated by the harness, not the campaign |

**The findings digest is per worker, and each worker reads the others'.**
An earlier revision made it one shared file, on the argument that three
theories of one implementation is the same synthesis done three times, each
missing what the other two learned. The argument holds; the mechanism did
not. Three branches rewriting one capped file conflict on every merge, and
"last write wins" is not something git does — it stops and asks. Splicing
two more capped digests costs less than a conflict per round, and is honest
about what is true, since three workers do have three views.

#### The audit does not parallelise

The audit reads the merged tree and writes `state/audit/`, and running two
concurrently means two verdicts for one section with no rule for which
wins. So **at most one audit runs at a time, and it runs against `main`
rather than any worker's branch.** A worker whose close makes the audit due
runs it; the others skip and continue.

`audit_every` counts campaign closes across all workers, so with N workers
the audit fires N times more often in wall clock at the same setting. That
is usually what is wanted — the queue is being consumed N times faster —
but it means the audit's cost per unit of wall clock rises with N, and the
setting should be re-read when N changes rather than carried over.

#### What N costs, and how to pick it

* **Spend is linear in N**, and campaigns are the dominant cost. Three
  workers is three times the burn rate against the same quota, which is
  the constraint that actually binds.
* **Disk is linear in N**: a worktree is its own workspace root and gets
  its own `target/`.
* **Throughput is sublinear in N**, because the queue is not infinitely
  deep and because claims remove the best targets from the other workers'
  lists. Where it stops paying is an empirical question and the number to
  watch is *gaps closed per campaign*, not campaigns per hour: if that
  falls as N rises, the workers are picking worse targets because the good
  ones are claimed.
* **Latency measurement is unaffected**, for the reason
  [above](#branches-exist-for-one-commit-at-a-time) — it is not measured
  during iterations at all.

Three is a starting point, not a derived number.

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

### The supervisor is a reconciler, not a scheduler

One process above the loops, in `harness/`. Its entire job is to make
**observed state match desired state**, and framing it that way rather
than as a scheduler is what makes it survivable: it holds nothing in
memory that matters, so killing it and starting it again is a no-op, and
a machine reboot costs one tick rather than a recovery procedure.

* **Desired state** is `state/phase.toml` — the current phase, and per
  loop a status of `running`, `paused`, `blocked`, or `done` — plus the
  ownership table. A human advances the phase after a gate; the
  supervisor only reads it. It must be a file rather than supervisor
  memory, or a restart loses which phase the project is in.
* **Observed state** is which campaign processes are alive.
* **Reconcile**: start campaigns for loops that should be running and
  are not, up to the concurrency cap; do nothing about loops that are
  where they should be; clean up after ones that died.

Everything it does not do is as important. It does not decide what work
to do — that is the campaign's job. It does not advance a phase, answer
an escalation, or judge whether a loop is making progress; those are a
human's and the gate's respectively. A supervisor that starts making
judgements becomes a second, unaccountable planner, which is the same
failure [section 5](#5-the-auditor-and-the-conformance-loops-number)
guards against for the auditor.

**Starting a campaign** is mechanical: allocate a UUID, make it both the
campaign id and the session id (`--session-id`, per
[section 16](#sessions-assign-the-id-own-the-transcript)), ensure the
loop's worktree and branch exist, write the campaign record, and exec
`claude -p --output-format stream-json` with that loop's prompt, teeing
the stream to the transcript path. The loop's identity — which paths it
owns, which language and server, which phase — is passed in rather than
inferred, so a prompt is a template and the supervisor is what fills it.

**The concurrency cap** is a number in config, and the binding
constraint is likely to be RAM rather than CPU: every worktree has its
own `target/`, and parallel cargo builds are memory-hungry well before
they are core-hungry. When more loops want to run than there are slots,
least-recently-run wins — simple, fair enough, and a loop that keeps
losing is visible on the dashboard rather than silently starved. Phase 6
is where this stops being theoretical, since Zed's full language set
will not fit.

**Quiescing** is the one place it must coordinate rather than supervise.
Latency measurement and gate-time evaluation need a quiet machine
([section 10](#what-cannot-be-measured-in-isolation)), so there is a
drain state: stop launching campaigns, let running ones finish their
current experiment and close early, then report the machine idle.
Interrupting between experiments is cheap precisely because every
experiment commits or reverts — there is no half-finished work to
protect.

**Crash recovery** happens on every tick, not just at startup. For each
campaign record still marked open whose process is gone: revert its
worktree to `HEAD` (an uncommitted experiment is a failed one, which is
green-or-revert applied by someone else), close the record with outcome
`crashed`, and free the slot. It does **not** try to resume — the
context is gone, and the hypothesis is written down, so a fresh campaign
can pick the thread up. Resuming a crashed campaign from its session id
stays available as a *human* action from the dashboard, which keeps the
line clear: the supervisor keeps things moving, a person investigates.

### Prompts, as starting points

The prompt is the most load-bearing artifact in this design and the
least validated part of it. What follows is a sketch to start from, not
a specification — [the open question](#open-questions) below says what
about it is still unsettled.

Ordered stable-to-volatile so the cacheable prefix is as long as
possible ([section 15](#levers-by-which-resource-they-move)). Everything
above the line is identical across every campaign of that loop type;
everything below changes.

**Conformance loop** (phases 1a, 2b):

```
CLAUDE.md is already in your context. Its constraints are absolute and
override anything here.

You are the {loop} loop, in phase {phase}.
You may write only: {owned paths}. Anything else fails the gate.

One campaign per session:
  1. Pick one target from state/audit/: an open gap, or an unjudged
     section. Prefer gaps.
     Write the target to state/campaigns/{owner}/{campaign_id}.md.
  2. Read only the design sections the item names. Not whole documents.
  3. Implement. Run `harness/gate {loop}` after each change.
     Green: commit using {{trailer-format}}.
     Red and unfixable in this experiment: revert to green, record why.
  4. Close when the item's test passes, or three experiments produce no
     commit, or the budget is spent. The auditor runs on close; its gap
     list is your next campaign's most likely target.
  5. On close: write state/journal/{owner}.md — what you tried, what
     failed, and why. Write it for a session that will not remember this one.

Spec changes: fix contradictions in design/, record them in
state/spec-changelog.md.
Anything that trades something off is a decision — write
state/decisions/{owner}-NNN.md, pick the reversible option, tag the
sites `// DECISION-{owner}-NNN: provisional`, and keep going. Never wait.
---
Open gaps: {from the last audit, with their document anchors}
Your campaigns so far: {one line each — target, outcome}
Decisions affecting you: {unresolved}
```

**The auditor** (paired with the conformance loop, run at every campaign
close). Not a loop — one session, one question, no edits:

```
CLAUDE.md is already in your context. Its constraints are absolute.

You are auditing, not implementing. You may not edit anything.

Spec: {{the design sections this phase covers}}
Prior audit: {{state/audit/ for those sections — verdicts and open gaps}}

Answer in exactly two numbered lists.

GAPS — a claim in these sections that is unimplemented, contradicted, or
implemented in a way that does not satisfy it. For each: the section
anchor, what the claim requires, what the code does instead, and where.

MINOR — the claim is satisfied but the manner invites objection.
Naming, structure, a test that passes for the wrong reason.

Finally, per section you were given: does it have any gap, or is it
clean? That verdict is the number; the gap list is the work queue.

Report a gap where you find one. Do not report a gap you cannot point
at in the code, and do not pad either list to look thorough — the counts
are measurements and inflating them destroys what they measure.
```

**Tuning loop** (phase 2a) differs in what it targets and how it knows
it succeeded:

```
CLAUDE.md is already in your context. Its constraints are absolute and
override anything here.

You are the {language} tuning loop, in phase {phase}.
You may write only: crates/lang_{language}/, state/…/{language}*.
There is no shared resolution code. If you need a utility another
language also needs, write it locally and note it in
state/shared-proposals/{language}-NNN.md. Phase 3 harvests those.

One campaign per session, and a campaign is one hypothesis:
  1. Pick the stratum with the largest share × gap from the table below.
     Then read that stratum's failure groups — they are what a hypothesis
     is formed from; the table only says where to look.
     State a hypothesis about *why* it is losing coverage, in terms of a
     group rather than of a stratum.
     Write both to state/campaigns/{owner}/{campaign_id}.md.
  2. Experiment. After each change run `harness/measure {language}`,
     which replays the frozen corpus against every server your language
     has, prints a per-stratum table per server, and rewrites the failure
     digest.
     Commit what improves top-1 agreement or coverage without regressing
     the other; revert what does not.
     Judge a change by what happened to the *group* you targeted, not only
     by the stratum total — a fix that moved four hundred cases and a fix
     that moved three look similar in a rounded percentage.
  3. Close when the hypothesis is confirmed and committed, or falsified,
     or five experiments move nothing, or the budget is spent.
  4. On close: write up what the hypothesis was and what actually
     happened. A falsified hypothesis is a result — record it in enough
     detail that nobody retries it. Then update two capped digests:
     state/findings/{owner}.md, for other languages — falsified plainly,
     confirmed marked untested elsewhere — and your own summary. Both are
     rewritten to fit, not appended to.

Optimise top-1 agreement, not containment. Returning more candidates
raises containment for free and is not progress.

If your language has more than one server, optimise the shared logic on
the positions where they agree — that is the frontier. A position where
they disagree is a profile question, not a resolution one, and the two
are never averaged.
---
Per-stratum table: {coverage, top1, contained, count, n}
Failure groups: {per stratum, the largest abstention and mismatch groups
  keyed by (reason | severity, stage log) — count, share, and a few
  concrete cases each. `core.md` §7. Prefer a hypothesis that explains a
  large group; a fix aimed at the named examples is worth much less than
  one aimed at the shape they share}
Frontier: {non-dominated points so far}
Your campaigns so far: {one line each — target, hypothesis, outcome}
Your summary: {current theory, what is ruled out}
Other languages: {their digests; candidates, not conclusions — test
  before adopting}
```

**Optimisation loop** (phases 3, 7) has no hypothesis and no spec gap — its target is duplication, and its oracle is exact:

```
CLAUDE.md is already in your context. Its constraints are absolute and
override anything here.

You are the optimisation loop, in phase {phase}.
You may write anywhere. Nothing else is running.

Deterministic responses must not change. That is the gate, not a goal:
`harness/equivalence` replays every language's frozen corpus before and
after, and fails on **any** difference at all. There is no carve-out
([section 10](#phase-3-is-a-refactor-under-an-exact-oracle)).

One campaign per session, and a campaign is one refactor target:
  1. Pick a target: the duplication report below, or an unharvested
     entry in state/shared-proposals/. Prefer whichever removes most
     bytes. Write it to state/campaigns/{owner}/{campaign_id}.md.
  2. Refactor. Run `harness/gate optimisation` after each change —
     equivalence first, size ratchet second.
  3. Commit what shrinks the binary with outputs unchanged. Revert the rest.
  4. If an optimisation would change outputs, do not take it. Write
     state/decisions/{owner}-NNN.md with the replay diff — which queries,
     which stratum, from where to where — and move on to another target.
  5. Close when the target is done, or three experiments produce no
     commit, or the budget is spent.
---
Duplication: {same-shape implementations across crates/lang_*}
Size: {per-language link delta, current against baseline}
Proposals: {unharvested state/shared-proposals/}
```

### One prompt per variety of phase

Not one template with a swapped middle. The varieties are the ones
above — conformance and its auditor, tuning, optimisation — plus phase 6
as its own — it is tuning, but under a rule 2a does
not have (the shared library exists and is read-only), and a rule that
applies to one phase and not another is exactly the thing a
parameterised template hides in substitution logic where nobody reviews
it.

Sharing would halve the surface that drifts, which was the argument for
it. That argument is much weaker now that rules are inlined by
substitution: **the common material has already been factored out — into
`CLAUDE.md`, not into a template.** What remains in each prompt is the
part that genuinely differs, so unifying them would be unifying the
differences.

The deciding consideration is blast radius. Prompts are the artifact
that will be revised most often and with the least evidence, and a
revision aimed at the tuning loop should not be able to change how the
conformance loop behaves. Separate files make that structural instead of
careful.

### Rules are inlined; subject matter is read

The `{{...}}` above is substitution, and it is the rule: **anything the
campaign must obey is in the prompt text, not behind a filename.** A
pointer produces one tool result that is then buried, so a rule read at
experiment one is a fuzzy memory by experiment six — and campaigns were
deliberately made long.

Two things keep that from collapsing into "maintain a second copy of
every rule," which would be worse than pointers:

* **Substitution happens at launch, from the source file.** The template
  holds `{{trailer-format}}` and the like; the supervisor reads the real
  file and splices it in. There is one copy of every rule and it is the
  original, so drift is not merely discouraged, it is unrepresentable.
* **`CLAUDE.md` needs none of this.** Claude Code loads it into every
  session's context already, so it satisfies the inline rule for free and
  splicing it would put it in context twice. The prompts therefore state
  its *precedence* — its constraints override anything the prompt says —
  and nothing else. That precedence line is worth keeping: without it,
  two sets of instructions arrive with no stated ordering.
* **The boundary is rules versus subject matter.** Constraints,
  procedure, ownership, and stop conditions are inlined. The spec
  sections, code, and corpus tables a campaign works *on* are not rules
  and are not all knowable in advance — but the ones that are get
  spliced too: a gap carries a document anchor, so the supervisor can
  inline that section and the campaign starts with it already in
  context. Anything further it follows from there, it
  reads.

Both go in the volatile tail rather than the stable prefix, so the
cacheable region is unaffected.

One consequence for [section 16](#every-intervention-is-logged): the
recorded prompt sha must be **the hash of the rendered prompt**, not of
the template, or a spliced-in file could change behaviour while the
template sha sits unchanged. `CLAUDE.md` is the exception and needs no
help — it is in git, and the campaign records the commit it started
from, so which version was in context is already recoverable. Editing it
is still logged as an intervention, for the same
reason a prompt revision is: it changes every subsequent campaign and
nothing downstream can tell.

Three things these deliberately do *not* do, because each is a lesson
already in this document: they do not tell the loop how to resolve
references, which is `resolution.md`'s job and would fossilise a guess
into the prompt; they do not describe the gate's internals, since a loop
that knows how it is scored is a loop that can optimise the scoring; and
they do not restate a rule that `CLAUDE.md` already states, because the
splice covers it.

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
wall-clock without costing a token; a loop that re-reads 160KB of design
doc per iteration costs tokens without costing wall clock. Replay speed
([section 9](#9-the-inner-loop-is-a-replay-and-its-cost-is-measured))
is a wall-clock lever and not a cost lever, and the document anchors on
audit gaps ([section 3](#3-where-the-work-comes-from)) are a cost lever
and not a wall-clock one. Replay wall clock is one of the numbers these
first campaigns exist to measure.

### The unit of accounting is the campaign

Every campaign emits one row to `state/cost/<loop>.jsonl`, partitioned
by owner for the same reason the metrics are, with per-experiment
detail nested inside it:

```
campaign id, session id, loop, language, phase, target,
commits produced, experiments (committed / reverted / empty),
input tokens (cached / uncached), output tokens,
model seconds, gate seconds, outcome
```

`language` is null for the phases that are not per-language (1a, 1b, 2b,
3), which is what lets the same file answer both "what did Python cost"
and "what did the driver cost". There is no `server` column: a campaign
belongs to a language, and a language's servers are all measured by the
same one.

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
progress mechanically — a section going clean, test count, frontier movement,
decision resolved — so cost per progress event is computable without a
new definition.

Per phase, the useful ratios are different:

* **1a, 2b (conformance):** tokens per gap closed.
* **2a (per language):** tokens per coverage point, per stratum. This is the number that says when to stop, and it should
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
| 1a core for measurement | low–moderate | days | small | how sharp the audit is |
| 1b repo collection | ~none | ~none | days (mostly human) | repo count |
| truth collection (gate into 2) | ~none | ~none | ~100 machine-hours | repos × servers × index time |
| 2a per language × server | **dominant** | weeks, parallel | small per iteration | iterations to plateau |
| 2b LSP shim | high | weeks, parallel | small | driver test surface |
| 3 whole-repository optimisation | moderate | days, serial | high (release builds) | how much duplication accumulated |

**Calibration is the first ten campaigns of each loop**, starting with
phase 1a's conformance loop — which is also the first time any of this
machinery runs against real work. Those ten are the cost measurement,
and every estimate in the table above gets rewritten from them before
the phase is allowed to run to completion. An estimate that is never
compared against an actual is decoration.

The other thing to watch across those ten: does the loop pick sensible
targets, does it leave the tree green, and does the journal accumulate
anything a human would have wanted written down. If the answer to the
third is no, the state file design is wrong and nothing downstream will
save it.

### Levers, by which resource they move

**Tokens.** Bound the context: a gap's document anchor means a campaign
reads one section rather than the whole design. Keep the
prompt's fixed prefix byte-identical across iterations and order it
stable-to-volatile — constitution, then prompt, then audit state, then the
journal tail and recent commits — so the cacheable prefix is as long as
possible. Choose the model tier per loop rather than globally: phase 3
is mechanical work under an exact oracle
([`phases.md`](phases.md)), which is the
best candidate for a cheaper tier, whereas phase 2a resolution logic is
the hardest reasoning in the project. The auditor is a fixed cost of one
session per conformance **round** ([section 5](#5-the-auditor-and-the-conformance-loops-number)),
and it is **not a knob a loop may turn**: `audit_every` lives in
`state/phase.toml`, which is denied to every loop, because it is the only
number that loop has ([section 5](#sections-clean-is-the-metric)) and a
loop that could make its own measurement cheaper would.

**Model wall-clock.** Parallelism across languages, and replay speed —
which is measured rather than targeted
([section 9](#there-is-no-replay-time-target-and-that-is-deliberate)), so
it becomes a lever only once the measurement says it is one. Neither
costs tokens.

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
* **Metrics.** The per-language frontier chart, the per-stratum table —
  one per server where a language has several — current versus the
  recorded baseline, and the held-out verdict. This is the chart
  [`phases.md`](phases.md) asks for at the
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
| Class B decision | options, recommendation, the provisional choice in force, and the sites tagged `DECISION-<owner>-<n>` |
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
restarted, audit overruled, **prompt revised**, **harness changed**.

**The log is the mechanism, not a record of it.** Answering a decision
*means* appending to this file; the harness derives the decision's
status from the log rather than from someone remembering to write both.
Same trick as the metrics history — make the record the path rather than
a side effect, and it cannot drift from what happened. The dashboard's
POST endpoint is what makes this free: every answer given through the
page is logged by construction. Out-of-band interventions cannot all be
caught, but the two common ones can — a git hook flags hand-authored
commits, and the harness logs its own kills.

**A prompt revision is the one intervention that cannot be replayed**,
and it is worth separating from the others for that reason. Every other
change here is recoverable after the fact: code is re-measurable because
replay is deterministic, a metric redefinition triggers a sweep, a
corpus change triggers recollection. A prompt change alters the
*generator* of campaigns, and past campaigns cannot be regenerated under
the new one. Metrics either side of it are not strictly comparable and
nothing downstream can detect that.

So it gets one extra mechanism rather than just an honest log entry:
prompt files live in `harness/prompts/`, in git, owned by nobody like
the rest of `harness/` — and **each campaign records the prompt sha it
ran under**. Segmenting behaviour by prompt version is then a join
rather than an archaeology exercise, and "cost per progress rose after
campaign 40" becomes attributable to a diff instead of a hunch. The
intervention log entry carries the commit and the reasoning; git carries
the diff.

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
`<transcripts>/<owner>/<campaign-id>.jsonl` as it goes, where
`<transcripts>` is the root [below](#reading-a-transcript) puts beside the
corpus rather than in the worktree — the index row stays in
`state/sessions.jsonl`, the stream itself never enters git. That artifact
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
campaign id (= session id), loop, phase, language, target,
prompt sha, started, ended, commits produced, tokens, outcome,
transcript path
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
one `deps.md` applies to crates: a tool has to solve a
problem we actually have, not a problem adjacent to it.

| Need | Verdict |
|---|---|
| Loop runner | **Build** — ~20 lines of bash, with every vendor-specific flag behind one adapter file ([below](#the-largest-dependency-in-this-project-is-not-a-crate)). Read `rxdt/loopgate_harness` and `mikehostetler/wreckit` first as reference designs |
| Stall detection | **Steal constants** from `frankbria/ralph-claude-code`; logic is ours |
| Edit-scope hook | **Adopt** `PreToolUse` — as fast feedback, not as a boundary |
| Held-out isolation | **Build** — physical separation; `denyRead` as defence in depth only |
| Worktrees | **Adopt** plain `git worktree` — one per parallel loop, plus ephemeral ones for gate-time evaluation |
| Worktree orchestrators (Conductor, Crystal, claude-squad) | **Reject** — GUI- or macOS-first, and the merge discipline is ours |
| Spec work queue | **Build** — see below |
| Numeric ratchets | **Build** — see below |
| Metric table regression | **Adopt** `insta` for shape; gate checks direction |
| Test runner | **Adopt** `cargo-nextest` (machine-readable output for the test-count ratchet) |
| Decision records | **Adopt** MADR template; `DECISION-` code tags stay ours |
| Journal | **Adopt** git trailers + `git interpret-trailers` |
| Cost monitoring | **Adopt** `ccusage` — behind the adapter, and replaceable from the teed stream |
| Auditor | **Build the prompt**; a read-only session is the whole mechanism |
| Spec claim notation | **Steal** EARS, for phrasing auditable claims |
| SDD frameworks (Spec Kit, Kiro, OpenSpec, BMAD) | **Reject** |
| Bencher (metric tracking service) | **Reject** — reasoning below |
| `beads` (agent-facing issue graph) | **Reject** — reasoning below |

### The largest dependency in this project is not a crate

`deps.md` spends its whole length being careful about crates — what each
one buys, what it costs, what happens if it goes away. Nothing has been
that careful about the table above, and the table above contains a
**bigger** exposure than anything in `deps.md`: the harness's core
mechanisms are built on one vendor's CLI surface.

`--session-id`, `--fork-session`, `-p`, `--output-format stream-json`,
`denyRead`, `PreToolUse`, `/sandbox`'s `allowWrite`, and `ccusage` are
all product surfaces of Claude Code. They are not versioned the way a
crate is, cannot be pinned in a lockfile, cannot be vendored, and change
under you on upgrade. The usual mitigation — pin it and vendor it if it
disappears — is simply unavailable, so the mitigation has to be a
different shape: **isolate rather than pin.**

Sorted by what actually breaks:

| Surface | If it changes |
|---|---|
| headless `-p` with a fixed prompt | there is no loop. Nothing else here works without it |
| `--session-id` | campaign id and session id stop being the same value, so the cost join, the transcript path, and resume all need a correlation step ([section 16](#sessions-assign-the-id-own-the-transcript)) |
| `--output-format stream-json` | no teed transcript, so the dashboard's campaign view goes dark and forensics falls back to scraping the vendor's own store — which [section 16](#sessions-assign-the-id-own-the-transcript) already refuses to depend on |
| `ccusage` | cost accounting only, and the teed stream already carries the token counts, so this is a parsing job rather than a loss |
| `--fork-session` | one human convenience on the dashboard |
| `PreToolUse`, `denyRead` | nothing that was a boundary. [Section 13](#mechanics-isolation-in-four-layers) already classes both as fast feedback and defence in depth, never the boundary itself |
| `/sandbox` `allowWrite` | the real isolation boundary — but the mechanism underneath is `bubblewrap`, which is an ordinary Linux tool we can invoke ourselves. Losing the wrapper costs a script, not the property |

Two things follow, and both are cheap.

**One adapter, and it is the only place a vendor-specific invocation
appears.** Every `claude` flag, every hook shape, every transcript path
lives in one file in `harness/`; the supervisor, the gate, and the
dashboard talk to that file and never to the CLI. This is the same move
the design makes for `ServerAdapter` and for `shared::proto`, applied to
the tool that runs the loops instead of to the ones it talks to — and
[section 16](#sessions-assign-the-id-own-the-transcript)'s refusal to
scrape the vendor's session store is that instinct already applied once,
in the single place it was most obviously needed. Generalising it is the
whole fix.

**Record the CLI version per campaign**, beside the prompt sha
([section 16](#every-intervention-is-logged)). The argument there is
exact: a prompt revision changes the *generator* of campaigns and nothing
downstream can detect it, so metrics either side are not comparable. A
CLI upgrade does the same thing, arrives without anybody deciding to
change anything, and is otherwise completely invisible. Pin it in
`external-dependencies.md` with the language servers, and treat an
upgrade as the intervention it is.

The honest summary: this is a real risk, it is larger than any crate
choice, and it is accepted because the alternative — building a loop
runner against a stable API that does not exist — is not a better
trade. What is not acceptable is running it unnamed, which is what the
table above was doing.

### Three rejections worth recording

Because they are the ones that will be proposed again:

**Bencher.** It tracks arbitrary metrics with statistical thresholds
and fails CI on regression, which sounds exactly like the ratchets in
[section 11](#11-size-and-loc-as-objectives). The mismatch is that its
value is *statistical* regression detection — seven threshold models
deriving variance from a metric's history — and **our measurements are
deterministic**. `measure replay` runs a deterministic handler
(`resolution.md` §11 requires the property) against frozen
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
dependency-aware "what is ready" queries. There is no hand-authored
queue to put in it: the work list is the audit's gap output
([section 3](#3-where-the-work-comes-from)), regenerated rather than
maintained, and a database for a derived artifact is storage for
something that should be cheap to recompute.

**Spec-driven frameworks.** They solve "the agent has no structure and
hallucinates APIs." That problem is already solved here by a mechanical
oracle and a written spec, and none of them enforce the one invariant
this design turns on — status advancing only when a named test passes.
The two things worth taking are EARS notation and the "constitution"
idea, and the constitution already exists as `CLAUDE.md`.

## 18. Scope: phases 1 and 1.5 first

**The initial implementation stops after phase 1.5.** Everything from
2a onward is a followup, planned once there is a working corpus and a
working measurement to plan against.

That boundary is not arbitrary. Phases 1a through 1.5 produce the
oracle; every loop described in this document is judged by it, and none
of them can be evaluated — or usefully designed — before it exists.
Building the campaign machinery first would mean building it against a
guess at how a tuning loop behaves, and the way to stop guessing is
three phases long.

### What the initial implementation is

* **Phase 1a** — `vendor/rope`, `sum_tree`, `shared`, the language crate
  template, `measure_core`, `measure_rust`.
* **Phase 1b** — the repository corpus, split and pinned
  (`data-collection.md` §1).
* **Phase 1c** — the language servers, installed and documented.
* **Phase 1.5** — ground truth for every (repository, server), frozen.

**Exit criterion, in two parts, because they answer different questions:**

* **The pipeline works** — `measure_rust replay` prints a per-stratum table
  over real truth data, and produces the same table twice. Rust only, and
  deliberately: this is the calibration run
  (`data-collection.md` §6), and it is what says the machinery is sound.
* **The corpus is complete** — every one of the seven languages has ground
  truth for every (repository, server), per
  [`phases.md`](phases.md) and `data-collection.md` §0. This is the ~100
  machine-hours, and it is the part that gates phase 2a.

The first is a milestone worth reaching early and stopping to look at; the
second is the phase. Conflating them is how the long pole gets
under-scoped — the pipeline proving itself on Rust in a week is not phase
1.5 being done.

At that point the tool has an oracle. Nothing before that point has one, and
nothing after it lacks one — which is why it is the place to stop and reassess
rather than a milestone to pass through.

### What that needs from this document

**Phase 1a runs as a conformance loop**, so the machinery that loop
depends on is in scope and the rest is not:

* **The gate** — `fmt`, `clippy`, `nextest`, diff scope
  ([section 4](#4-the-iteration-contract)). In `harness/`, written by a
  human, denied to the loop.
* **The auditor** ([section 5](#5-the-auditor-and-the-conformance-loops-number)),
  and with it `state/audit/`. This is the only feedback a conformance
  phase has, so it is not optional and not deferrable — without it the
  loop has no number and no work queue.
* **Campaign records and the journal**
  ([section 4](#campaigns-are-the-unit-of-fresh-context)), plus the
  commit-trailer convention so `git log` is queryable later.
* **The conformance and auditor prompts**
  ([section 14](#prompts-as-starting-points)), which are the two least
  validated artifacts here — expect to revise them during the first ten
  campaigns and to log each revision
  ([section 16](#every-intervention-is-logged)).

What a single loop does **not** need: the supervisor, since one bash
loop is not a fleet; worktrees and branches, since with one writer
`master` is uncontended; the dashboard, since with one loop the audit
state and the gap list are two files to read. Those arrive with 2a,
when there is more than one of anything.

Phases 1b and 1c are human work with scripts. Phase 1.5 is machine work
with no model in it at all.

### What is deliberately not built yet

The supervisor, the frontier, the evaluation half of held-out selection,
the per-language link delta, and the tuning and optimisation prompts.
Each is specified here because the specification is what makes the
followup cheap — but every one of them exists to serve tuning loops, and
there are none until 2a.

**This list shortens as the followup is built, and it is the list rather
than the argument that moves.** The argument is unchanged: a thing that
only serves tuning loops is not built before there are tuning loops. What
changes is which things are still on the far side of that line, and
[section 18](#the-conformance-loop-builds-the-followup) points the
conformance loop at this document precisely so that they cross it during
phase 1.5. Anything crossed off here has a section that now describes
something real, which is what an audit is for.

Two exceptions worth starting early because retrofitting them is
expensive:

* **The metrics history** ([section 10](#the-metrics-history)), even
  though nothing consumes it during phase 1. It is recoverable later by
  a replay sweep, so this is a preference rather than a requirement.
* **The corpus split** — tuning and held-out decided at 1b and
  physically separated ([section 12](#12-held-out-integrity)), which is
  the same two-way split [section 8](#8-sequencing-and-gates) gates 1b
  on. A third, *final* set is deliberately not decided here:
  [section 12](#12-held-out-integrity) keeps carving one out of the
  held-out five as a remedy held in reserve, on the argument that
  deciding it now would mean guessing how much leakage ten gates cause.
  The split can be made finer later and never coarser, so only the
  coarse decision is irreversible — and it is the one that is *not*
  recoverable: once a repository has been in the tuning corpus, moving
  it to held-out does not un-teach it.

### The conformance loop builds the followup

Once phase 1a is done and the loop has proved itself on it, **point that
same loop at this document and have it build the phase-2 machinery** —
supervisor, dashboard, frontier tool, campaign and findings digests, the
cost join.

The scheduling argument is the strongest one: **phase 1.5 is roughly a
hundred machine-hours with no model in it at all.** The conformance
loop finishes 1a and then has nothing to do while ground truth is
collected. Building the followup in that window costs no wall clock
that was not already being spent.

The rest of the case is that this is the right shape of work for that
loop. `loops.md` makes checkable claims — the supervisor reconciles
desired against observed state, the digest is capped and rewritten,
answers are appended to the intervention log — which is exactly what
[section 3](#who-uses-it) says makes a document auditable. So it joins
`core.md` and `shim.md` as audited material at that point, and the
conformance loop's own machinery works on it unchanged.

**When.** Not a vibe: phase 1a's own gate — every section of `core.md`
clean and a human having ruled on the minor list — plus the qualitative
check [section 15](#estimates-and-replacing-them-with-measurements)
already asks for over the first ten campaigns. If the loop picked
sensible targets, left the tree green, and wrote a journal worth
reading, it is good enough to point at something else. If it did not,
that is worth knowing before it builds the thing that will run six more
loops.

**The one rule that has to bend, carefully.**
[Section 13](#mechanics-isolation-in-four-layers) denies every loop
write access to `harness/`, and this work is writing to `harness/`. The
prohibition exists for one reason — *a loop must not weaken the gate
that judges it* — so the split follows from the reason rather than
suspending it:

| Path | While building the followup |
|---|---|
| `harness/gate*`, `harness/prompts/`, the auditor | **denied** — these judge this loop, now |
| `harness/supervisor/`, `harness/dashboard/`, the frontier tool | writable — these judge phase 2a, later |

That is enforceable by the same diff-scope check as everything else, and
it keeps the invariant intact: nothing a loop writes can change how that
loop is being scored.

**Conformance to this document is necessary and not sufficient.** An
audit can check that the supervisor has the structure described here.
It cannot check that the resulting campaigns are any good, because that
is a claim about behaviour under load, and the first real evidence
arrives when phase 2a runs. So the machinery is reviewed by a human on
the way in — it is a Class B change, like any other harness change — and
the first ten campaigns of 2a are where it is actually validated.

### What the followup will know that this document does not

Phase 1.5's output is what turns several guesses here into
measurements — how long a replay actually takes, how noisy a stratum is,
what a campaign costs, whether the corpus is large enough to distinguish
an improvement from sampling noise (decided question 8). The followup
should revisit sections 9, 10, and 15 against real numbers rather than
implementing them as written.

## 19. How this goes wrong

Stated plainly, because each of these has a countermeasure above and
the countermeasures are the weakest part of this document.

* **The audit never looks at a section, or looks and misses.** This
  replaces the old "the ledger has a hole in it" failure and is milder
  in one respect and worse in another: rotation guarantees every section
  is *reached*, which no hand-built inventory did, but a section can be
  audited and wrongly called clean, and nothing downstream disagrees.
  Successive audits of the same section by fresh sessions are the only
  defence, and it is a statistical one.
* **The loop rewrites the spec toward what it built.** Class A/B is a
  judgement call made by the entity with the incentive, and the audit is
  structurally blind to it — moving the spec removes the gap from the
  instrument that would have reported it
  ([section 7](#7-progress-stall-and-the-ways-it-is-faked)). Scheduled
  changelog review and a campaign-scoped detector make it *visible* after
  the fact. Neither makes it preventable, and this remains the failure
  with the thinnest defence in the whole design.
* **Overfitting.** Expected, per `high-level.md`. Held-out isolation catches
  the gross version; a loop that finds a genuinely general improvement
  and a repo-specific hack in the same iteration will ship both.
* **The quality phase climbs to an incompressible peak.** Coverage
  reachable only via thirty special cases, so the cost phase's frontier
  offers nothing but "give back coverage or ship the megabyte." The
  metrics history is what makes this survivable — the cost phase can
  select an earlier, cheaper point — but it cannot manufacture a good
  option that was never on the curve.
* **The loop builds the machinery that will judge later loops.** The
  audit can confirm the supervisor matches
  [section 14](#the-supervisor-is-a-reconciler-not-a-scheduler)
  structurally; it cannot confirm the campaigns it produces are any
  good, and the first evidence of that arrives a whole phase later.
  Human review on the way in is the only check that happens before the
  cost is sunk.
* **Intervention that is never recorded.** A decision made in your head
  and applied by hand is invisible to every mechanism here: the loop
  cannot read the reasoning, the next campaign re-raises the question,
  and the autonomy numbers are wrong in the flattering direction. The
  log in [section 16](#every-intervention-is-logged) only helps to the
  extent answering *through* it is easier than answering around it.
* **Plausible motion.** A hundred iterations of refactoring, journal
  entries, and audit churn with no metric movement. Stall detection is
  the answer and it is tuned by guessing at N.
* **Phase 2 under-scoped.** Corpus collection is real infrastructure
  work. If it slips, every language loop is blocked and the temptation
  is to start them anyway against fixtures, which measures nothing.
* **Phase 3 is bigger than one serial pass can hold.** Seven languages
  tuned independently for hundreds of campaigns accumulate a lot of
  parallel implementations, and phase 3 harvests all of it with one
  writer under an equality constraint that must hold for every language
  at once. Accepted deliberately (decided question 3); the remedy is
  interleaved extraction, which costs back the independence phase 2a
  depends on.
* **Sampling noise is mistaken for improvement.** The corpus is assumed
  large enough to separate the two (decided question 8). If a thin
  stratum's confidence interval is wider than the movement being chased,
  a campaign can confirm a hypothesis that is not there — and the
  frontier will record the point as real.
* **Selection leaks the held-out set.** Choosing a version at every
  gate is optimisation against it, slowly. With five held-out
  repositories and no final split, nothing currently detects it; the
  remedy in [section 12](#12-held-out-integrity) costs corpus size,
  which is the scarce thing.

## Decided

The questions this document opened with, and what they were settled to.
The reasoning is kept because several of these are close calls that will
look re-openable later.

1. **The conformance loop is one loop**, not one per driver subsystem.
   Splitting it would create exactly the shared-surface problem
   [section 13](#13-shared-code-and-when-it-may-exist) solves for
   languages by having the shared layer be derived — and inside a single
   crate that trick is unavailable, so the split would have to be
   managed rather than designed away.

2. **Stall N is 3 for conformance, 5 for tuning**
   ([section 7](#7-progress-stall-and-the-ways-it-is-faked)).

3. **Phase 3 stays a single serial pass.** The risk that seven
   languages' accumulated duplication is too much for one writer under
   an equality constraint is real and is being taken deliberately; it is
   recorded in [section 19](#19-how-this-goes-wrong) rather than
   pre-solved. Interleaving extraction between tuning rounds is the
   remedy if it fails, and it reintroduces coordination that the current
   rule removes — so it should be a measured retreat, not a drift.

4. **Moot.** `phases.md` answers it: phases 4 through 7
   repeat 2a and 3 for Zed's full language set.

5. **Phase 2a keeps a standing cost guardrail** — an order-of-magnitude
   ceiling, not a ratchet
   ([section 11](#11-size-and-loc-as-objectives)).

6. **Extraction ties are handled case by case**, not by rule. Two
   languages implementing the same idea with different signatures is a
   judgement the phase 3 writer makes with both implementations in
   front of it, and the equality constraint already bounds what a wrong
   choice can cost. A rule written now would be written without a single
   example of the thing it governs.

7. **Escalations batch; they never interrupt**
   ([section 6](#6-spec-changes-what-the-loop-may-decide-alone)).

8. **The corpus is assumed large enough** to distinguish real
   improvements from sampling noise. This is an assumption, not a
   finding, and the exhaustive repositories in `data-collection.md` §3
   are what would falsify it. Carried as a risk in
   [section 19](#19-how-this-goes-wrong).

9. **The binary-size ratchet is not expected to fight the rope newtype
   work.** The baseline is taken after phase 1a, so whatever the
   newtypes cost is inside it rather than charged to whoever iterates
   next.

10. **The loop may never add a language.** Enforced rather than
    discouraged: a new `crates/lang_*` is outside every loop's owned
    paths, so the gate rejects the commit
    ([section 7](#7-progress-stall-and-the-ways-it-is-faked)).

## Open questions
**Do the capped digests actually stay useful?**
[Section 4](#campaigns-compare-notes-asymmetrically) bets that a 512-word
digest, rewritten under a hard cap, is better than a log truncated by
recency — because dropping something requires judging it superseded
rather than merely old. That is an argument, not a finding, and it can
fail in a specific way: a loop that rewrites badly loses a result nobody
notices is missing, and the full campaign records are there but nothing
prompts anyone to look. The first sign would be two campaigns
independently retrying the same falsified hypothesis, which is
detectable from the one-liners and worth checking for early.

The related sizing question — how a digest is selected for the tail once
phase 6 has thirty-plus loops — is deliberately unanswered, since
choosing an axis now means guessing before a single digest exists.
