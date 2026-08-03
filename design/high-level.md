# A heuristic go-to-definition LSP shim

Ever stuck waiting for the LSP, wanting to do a "go-to-definition"?
This is a tool that provides imprecise results when the proper LSP
isn't ready.

It can simply be run like this:

```sh
$ heuristic-jump -- rust-analyzer
```

It also runs with no language server behind it at all - just
leave off the `-- rust-analyzer` part:

```sh
$ heuristic-jump
```

In that mode it is the whole language server - it answers `initialize`
itself and serves nothing but heuristic go-to-definition. That is
for languages with no usable server, for debugging the heuristic
without a server's variability in the picture, and for editors that
can run several servers per language. There is no proper LSP to
compare against, so precision cannot be measured there and no
divergence is ever reported. See `core.md`
section 17.

Each language implements its own resolution logic. Dispatch is simple -
the go-to-definition call is handed directly to the language's
implementation. Sharing and dedupe between languages happens through
shared utilities, not through a common framework or config format that
each language has to be expressed in.

## Implementation

This will store the current state of the project files, as much as
needed by the LSP protocol.

For recently queried and/or recently edited files, it will have an
in-memory tree-sitter parse (that is incrementally updated)

The grammars are the exact same tree-sitter language definitions Zed
uses - the same crates at the same pinned revisions, taken from Zed's
workspace Cargo.toml.

There is no index. Lookups are done on demand with dumb-jump style
search. This keeps the tool lightweight, and avoids competing with the
proper LSP for CPU during its startup - which is exactly the window
where the heuristic is most needed. The latency budgets below are what
keep this honest: a search that can't finish in time abstains rather
than blocking.

Search scope is the project's own tracked source. External dependency
sources and gitignored files are out of scope.

Availability of the proper LSP is modeled as server state, inferred
from whatever information is on hand - progress notifications,
responsiveness, and process liveness - rather than only tracking
whether a particular request is still outstanding. Unavailability is
not just a startup condition: the server also restarts (e.g. on a
Cargo.toml change), hangs, and crashes, and those windows are when the
heuristic has the most to offer.

When a go-to-definition request is received, run the proper LSP. If a
query is done again on the same spot and it still hasn't responded,
give the heuristic one. Also complete the proper LSP request with the
heuristic one.

If the proper LSP returns after a heuristic result is used, but its
result is different, notify the user.

Steps:

* Collect the id and namespace under the cursor, and information about
  the usage context (Class vs function vs type vs type variable, etc)?

* Walk up doing proper local binding resolution.

* Info from the current file's imports is used - if it's explicitly
  imported, then the corresponding file may be able to be found
  directly.

    - If found, a technique like
    https://github.com/jacktasia/dumb-jump is used to find candidate
    definitions there.

    - A tree-sitter parse of the found candidates is done, and it
      checks if the candidate parses (not in a block comment,
      etc). For languages that have Some.Object.Nested, it's analyzed
      whether it would be been possible for this to be the referenced
      thing.

    - If not found, fall back to whole project search.

* If it may be part of wildcard imports, then dumb-jump style search
  is used on all of the wildcard-imported modules.

* Otherwise, just search the whole project.

## Future work

* Possibly serve find-references too. It would stay unindexed, same as
  go-to-definition.

* External dependency sources are out of scope for now. Jumping into a
  dependency is a common go-to-definition though, so this is worth
  revisiting - the cost is that dependency sources are huge, and
  on-demand search over them is a much harder latency problem.

* Gitignored files are out of scope for now. Checked-out generated
  code falls in this bucket.

* Distinguish heuristic results from confident ones in the response,
  including a probability estimate. This interacts with the precision
  floor below: if the user can see up front that an answer is a guess,
  a wrong jump costs much less trust, and the floor can be looser for
  results that are clearly marked.

### Coverage at a precision floor

Not v1. v1 answers whenever it has a guess and measures what that
costs; this is the shape the metric would take once there is enough
data to set thresholds that mean something.

There are three outcomes per query: correct, wrong, or abstain.
Abstaining is cheap - the user just waits for the proper LSP, which is
the status quo. So precision is fixed first, and coverage maximized
underneath it:

> Of queries where the tool commits to an answer, >=97% must match the
> LSP. Subject to that, maximize the fraction of queries it commits on.

That committed fraction becomes the headline number. Plain "match rate"
is not used, because it can be improved by guessing more.

Every heuristic then needs a confidence notion and an explicit "not
confident enough" path. 97% is a floor and not a target - a stratum
that can't clear it should abstain rather than drag precision down for
everything else.

The reasoning: the user's alternative is waiting a few seconds. A wrong
jump costs the jump, the realization, and the trip back - but mostly it
costs trust. Once the tool is wrong often enough to warrant checking
every result, that verification cost is paid on the correct answers
too, and the tool is net negative.

Error severity gets budgets at the same time - roughly <= 1% for a
wrong file in the same module tree, <= 0.5% for an unrelated one, with
same-file misses absorbing the remainder.

The prerequisite is the per-stratum table with real numbers in it,
which is exactly what v1's measure-everything posture produces.

## Development plan

The plan is to have ~10 opensource repos per language, and
incrementally collect authoritative go-to-definition information from
the LSP. Identifier positions are enumerated with tree-sitter and then
*sampled* - uniformly, capped per repository - because an exhaustive
scan is thousands of machine-hours across the full matrix. For each
sampled position the scan records both the LSP's resolved location and
how long the LSP took to answer. `data-collection.md` has the
arithmetic and the sampling rules.

That file - `truth.jsonl` - is collected once per (repo commit, server
version) and then frozen. The LSP's answer is a fact about the corpus,
not about our code, so tuning never re-runs a language server: it
replays the handler against the recorded positions. The `measure`
binaries have both modes for this reason, and the fast one is what a
tuning session actually runs. See `core.md`
section 11.

Where a language has more than one usable server - Python and
TypeScript both do - each gets its own truth file, and each is a
separate thing to optimize. This is not redundancy. The shim stands in
for one specific server and reports divergence against that server, so
that server's answer is what counts as correct - and two servers
genuinely disagree on cases like re-exports, where both answers are
defensible rather than one being wrong. So the tool's behaviour varies
with the server behind it, metrics are reported per (language, server),
and they are never averaged across servers.

Of the ~10 repos per language, 3-4 are held out and never seen by
tuning sessions. Since the plan is Claude code sessions iterating
against the corpus, learning a particular repo's local conventions is
the default outcome rather than a risk. Both numbers get reported, and
a gap between tuned and held-out repos is the overfitting signal.

The held-out share is split again, into *select* and *final*: choosing
a version at a phase gate is itself optimisation against whatever it is
chosen on, so the set used for that is not the set the last number is
reported from. Roughly 6-7 / 2 / 1-2 - `data-collection.md` §1 and
`implementation-loop.md` §12.

The held-out repos and their truth files live in a **separate corpus
root**, outside the workspace, alongside but distinct from the tuning
one. Keeping them in a sibling directory of the tuning corpus - rather
than a subdirectory of it that everyone agrees not to look at - is what
makes the separation something other than an honour system: a session
is given one path and never the other. The layout is in
`core.md` section 11.

Claude code sessions will then be used on each language to improve the
metrics below.

## Success metrics

### Coverage

> Maximize **handler coverage**: of all the go-to-definition queries in
> the corpus, the fraction the resolution logic answers at all.

That fraction is the headline number, and for now it is the only one
with a target attached. **Precision is measured, not enforced.** If the
heuristic has a guess, it returns the guess; the cost of a wrong answer
is not something this version tries to manage.

Handler coverage is measured by the corpus scan, not by watching a live
session, and the distinction matters because the two differ by an order
of magnitude. In a live session against a healthy server the retry rule
means the shim answers almost nothing - the user has to press twice - so
what a session measures is mostly how often people double-press, which
is a fact about habits rather than about resolution.

**Delivered coverage** - the fraction of live queries where a heuristic
answer actually reached the user - is the second number, and it is kept
because it is the only thing that judges the health model and the retry
protocol. Those are real design choices and without it they have no
feedback at all. It is reconstructible after the fact from the trace
records, which carry `server_health` and `decision` per query, so it
needs no separate instrumentation.

Only handler coverage is being optimized. Delivered coverage is a
diagnostic for a different part of the system, and treating it as the
target would make a change to the retry rule look like a resolution
regression.

That is a deliberate starting point rather than an oversight, and it
reverses the obvious ordering. A confidence model cannot be calibrated
without data, and a stratum that always declines to answer produces no
data about itself - so starting strict means the per-stratum table
below is mostly empty rows, and the thresholds that would fill it have
to be guessed. Starting permissive fills every row from the first
corpus run, and a floor can then be set from measurements instead of
from intuition. See "Coverage at a precision floor" under Future work
for what that would look like.

Two things cause the tool to decline. Neither is about confidence:

* There is no candidate at all - the cursor is on a keyword, or nothing
  matched.
* The latency budget ran out. See below.

Several candidates that nothing distinguishes is not a third case. LSP's
definition response is already a list, so the tool returns all of them,
ranked, and the editor shows a picker. That is decided - see "Several
candidates" below.

What keeps this honest in the meantime is divergence reporting: when
the proper LSP disagrees with an answer the user was already shown,
they are told, every time and without rate limiting. That is the whole
safety mechanism right now, so it matters more in this version than it
would under a floor.

It is also a *proxy-mode* property. Standalone has no second answer to
disagree with, so it never reports anything - which is right, because a
standalone user was told at startup that the tool is heuristic-only and
has no reason to expect otherwise.

### Several candidates

When the ranking cannot separate the candidates, all of them are
returned, ranked best-first. `textDocument/definition` answers with a
list, and both Zed and VS Code render a multi-result answer as a picker
rather than a jump, so this needs no protocol extension and no new
interaction - it is the one the editor already has for this exact
situation.

It is also the honest answer. Eleven files define `new`; the tool knows
which eleven and does not know which one. Returning one of them is a
coin flip presented as knowledge, and returning all eleven is a
correct statement of what was found.

This decides the question the deferred precision floor left open, and it
decides it without an abstention: ambiguity is a *shape of answer*, not a
reason to decline. It also improves the case the tool is worst at -
`x.foo()` needing type inference - from a coin flip to a short list.

**A list has to be short to be an answer.** Past some size a picker is
worse than nothing, so the ranked list is capped and the cap is reported.
What should happen at the cap - truncate to the best N, or treat "too
many to be useful" as an abstention - is not settled; see
`open-questions.md` question 13.

#### What this does to the metrics

Precision was defined against a single answer, and a set breaks it in a
specific way worth naming: **"the correct answer is somewhere in the
list" always improves by returning more.** That is exactly the flaw that
made plain match rate unusable above, arriving in a new place. So it is
never the headline, and never reported alone.

Three numbers, together:

* **Top-1 agreement** - the first location matches the LSP. Directly
  comparable to the single-answer number it replaces, and **returning
  more candidates cannot improve it**. This is the one to optimize.

* **Containment** - the LSP's answer is somewhere in the list. This is
  what the user can actually get to, so it is the ceiling on how useful
  the answer was. Always reported beside the size distribution, because
  alone it is gameable.

* **Result count** - the distribution of how many locations were
  returned, per stratum. The gap between containment and top-1 is the
  cost the picker imposes, and this is what prices it.

A change that raises containment while raising result counts has bought
nothing. A change that raises top-1 at constant counts is a real
improvement. Stating both is what keeps that distinction visible.

**Divergence is reported on containment, not on top-1.** If the proper
LSP's answer was in the list, the user was shown it and nothing was
hidden from them - telling them they were misled would be false. This
also makes the report meaningfully rarer and therefore more worth
reading.

### Error severity

Wrong answers are not interchangeable, so they are measured
separately - reported, with no budget attached yet.

With a list, the failure being classified is **the correct answer not
being in it**, and the tier is taken from the top-ranked location, since
that is where a user who trusts the ordering looks first:

Landing within 3 lines of the proper LSP's answer counts as a match, not
an error. At that distance the right definition is on screen and the
user is already looking at it, so calling it wrong would measure
something nobody experiences as wrong.

* Same file, further off than 3 lines. Recoverable - the user is at
  least in the right place - but they have to hunt.

* Wrong file, same module tree. Moderate cost.

* Wrong file, unrelated module or crate. This is the trust-destroying
  one.

The point of tracking these now, with nothing enforced, is that the
budgets in Future work have to come from somewhere. These three numbers
are what would set them.

### Latency

The heuristic answer has to land inside the LSP request without the
editor noticing. Budgets, measured cold on the largest repo in the
corpus:

* p50 <= 50ms
* p99 <= 400ms
* hard cap of 750ms - past that, abstain

These are looser than they first look, and deliberately so. With no index,
whole-project search is an on-demand walk plus content scan, which lands
in the low hundreds of milliseconds on a mid-size repo. A tighter budget
would not make that faster; it would just convert every whole-project
query into an abstention and quietly delete a whole stratum from
coverage. Meanwhile the thing being raced is a language server that is
seconds to minutes from answering, so several hundred milliseconds is
still a large win. Revisit once there are real measurements.

The hard cap ties the metrics together: blowing the latency budget
degrades to an abstention, so it costs coverage but never precision.

Latency is reported per stratum too. Local resolution should be
sub-millisecond; whole-project search is where the tail lives.

### Stratification

A complete identifier scan is dominated by locals and same-file
references, which proper scope resolution already nails. A single
aggregate would report a flattering number that is mostly cases which
never needed a heuristic in the first place. Precision, coverage, and
latency are all reported per class:

* local binding (same scope)
* same-file, module level
* explicitly imported, unambiguous
* wildcard / glob imported
* ambiguous name - many definitions project-wide
* cross-crate / external dependency
* macro-generated or derived
* method call requiring type inference (`x.foo()`)

The per-class table is the artifact that drives decisions, not any
single rolled-up number.

That last class is the one a heuristic fundamentally cannot compete on,
and in Rust it's a large share of real go-to-definition invocations. It
is no longer a candidate for a permanent abstain class, though: returning
every method of that name, ranked, is a genuinely useful answer where
picking one would have been a guess. Expect this row to show low top-1
agreement, high containment, and large result counts - which is a fair
description of the case rather than a failure to handle it. Keeping it as
its own row rather than dissolving it into an average is most of what
makes these numbers honest.

### Value weighting

Since the scan records LSP latency per query, results can also be
weighted by it. A correct answer to a query that rust-analyzer would
have served in 150ms is worth approximately nothing - the tool's value
is concentrated entirely on the slow tail. This may show that the
genuinely useful slice is much narrower than the raw identifier count
suggests.

## Prior version

I have an old version of this in ../heuristic_jump_old, and may want to
use the text similarity stuff, maybe other things.  However, it was
based on the idea of being integrated directly into Zed, where
language configs are more traditionally based on treesitter queries.
I no longer want to stick with that limitation.  Instead, each
language will have its own analysis implementation in rust.

Generally this should reference the zed code (../zed) and how it does
LSP. It's conceivable this could be integrated back into Zed or
possibly as an extension if Zed's extension API is greatly expanded.

## License

The crates in `crates/` are MIT.

The shipped binary is GPL-3.0-or-later, because it links a vendored
copy of Zed's `rope` (`vendor/rope`), which is GPL-3.0-or-later.
`vendor/sum_tree` is Apache-2.0.

Keeping our own crates MIT is deliberate: `rope` is the only GPL
input, so replacing it would make the whole workspace permissively
licensable without relicensing anything. See `deps.md`
section 5.
