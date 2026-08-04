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
itself and serves nothing but heuristic go-to-definition. There is no
proper LSP to compare against, so precision cannot be measured there
and no divergence is ever reported. `shim.md` section 14 has the
mechanics.

Four reasons it exists, in descending order of how much they should
influence the design:

* **Languages with no usable server.** Plenty of languages have no
  language server, or one heavy enough that a user will not run it for
  a quick read through a codebase. There the heuristic is not a stopgap
  for something better - it is the only thing on offer, and the
  comparison it has to win is against no navigation at all.

    This is the reason ranked first and the one nothing below can
    measure: every metric here is defined against a language server's
    answer, and this case has none. `open-questions.md` question 15 asks
    how such a language is scored at all - an LLM oracle is the likely
    answer - and until it is settled, the plan serves this case on the
    unmeasured bet that quality tuned against languages that do have
    servers transfers to languages that do not.

* **Developing and debugging the heuristic.** Running resolution with
  no server in the picture removes every source of variability that is
  not the handler, which is exactly what you want when a jump lands in
  the wrong place and you are trying to find out why.

* **Editors that support several servers per language.** Zed and VS
  Code both do, and both merge definition results. Running standalone
  alongside rust-analyzer is a lower-effort deployment than proxying.
  It is a genuinely worse one - the editor shows a picker instead of
  jumping, and divergence goes undetected, so precision loses its only
  ground truth - but it is the fallback when proxying is not practical,
  and it should work rather than be blocked.

* **Servers with weak navigation.** A server that declares no
  `definitionProvider` is a case the proxy defers. Standalone answers
  most of it without needing a mixed mode: a user in that position can
  run standalone as a second server.

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

When a go-to-definition request is received it is forwarded to the
proper LSP immediately, and whether the heuristic also answers depends
on the server's health. While the server is still starting up, or has
stopped answering, the heuristic answers straight away - those are the
windows the tool exists for. While the server is healthy the shim stays
silent and the real answer arrives, which costs the user nothing they
were not already paying.

An earlier version instead waited for the user to ask a second time at
the same spot before answering. That is gone; `shim.md` section 7 says
why, and the short version is that it was the most intricate state in
the driver serving the one case that mattered least.

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
section 7.

Where a language has more than one usable server - Python and
TypeScript both do - each gets its own truth file, and each is a
separate thing to optimize. This is not redundancy. The shim stands in
for one specific server and reports divergence against that server, so
that server's answer is what counts as correct - and two servers
genuinely disagree on cases like re-exports, where both answers are
defensible rather than one being wrong. So the tool's behaviour varies
with the server behind it, metrics are reported per (language, server),
and they are never averaged across servers.

Of the 10 repos per language, 5 are held out and never seen by
tuning sessions. Since the plan is Claude code sessions iterating
against the corpus, learning a particular repo's local conventions is
the default outcome rather than a risk. Both numbers get reported, and
a gap between tuned and held-out repos is the overfitting signal.

Half the corpus going to validation is a lot, and it is deliberate: the
tuned/held-out gap is the only thing separating a real improvement from
a loop that has learned five repositories, and a thin held-out set makes
that signal noisy exactly when it matters. Note that choosing a version
at a phase gate is itself optimisation against whatever it is chosen on,
so the held-out set is a *selection* set rather than an untouched one -
`data-collection.md` §1 and `loops.md` §12 have what to do if that
starts to cost.

The held-out repos and their truth files live in a **separate corpus
split**, outside the workspace: `../heuristic-jump-corpus/test/`,
alongside but distinct from `training/`. Keeping them in a sibling
directory - rather than a subdirectory of the tuning corpus that
everyone agrees not to look at - is what makes the separation something
other than an honour system: a session is given one path and never the
other. The layout is in `core.md` section 7.

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
of magnitude. In a live session against a healthy server the shim stays
silent and answers nothing at all, so what a session measures is mostly
how often the server was unhealthy - a fact about the user's machine and
project size rather than about resolution.

**Delivered coverage** - the fraction of live queries where a heuristic
answer actually reached the user - is the second number, and it is kept
because it is the only thing that judges the health model. That is a real
design choice, and now the *only* thing deciding whether the shim ever
speaks, so without this number it has no feedback at all. It is reconstructible after the fact from the trace
records, which carry `server_health` and `decision` per query, so it
needs no separate instrumentation.

Only handler coverage is being optimized. Delivered coverage is a
diagnostic for a different part of the system, and treating it as the
target would make a change to the health model look like a resolution
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
`open-questions.md` question 12.

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
an error, and columns are not compared at all. `core.md` section 6 owns
that predicate and argues it; what matters here is only that the tiers
below are measured against it.

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

The shipped binary is GPL-3.0-or-later. That is a project-level
commitment rather than a consequence to be engineered around, and
`LICENSE-GPL` ships alongside it.

**There are two GPL inputs, not one.** `vendor/rope` is Zed's, unchanged,
and everything reaches it through `DocumentSnapshot`. `crates/similarity`
is ported from the prior implementation (`../heuristic_jump_old`), whose
`text_similarity` came out of Zed's `edit_prediction_context`, and every
`crates/lang_*` depends on it — so the handler layer is GPL too.
`vendor/sum_tree` is Apache-2.0, which is one-way compatible into GPL-3.0
and is not the constraint here.

An earlier version of this section said `rope` was the only GPL input and
treated replacing it as an exit: relicense nothing, and the whole workspace
could go permissive. `similarity` closes that exit for the handler layer,
and it was taken deliberately rather than discovered — the binary links
`rope` and is GPL-3.0-or-later either way, so nothing about what ships
changed. What changed is that going permissive would now mean replacing two
things instead of one, and the second is the piece that is genuinely hard to
rewrite well.

The crates that depend on neither are MIT: `shared`, `driver`,
`heuristic_jump`, `measure_core` and each `measure_<lang>`. Vendoring GPL
code does not transfer copyright in code we wrote and MIT is
GPL-3.0-compatible, so an MIT crate combines into a GPL binary with no extra
grant needed — and the permissive surface is then the seam and the
measurement program, which is the part a third party would want anyway. See
`deps.md` section 5.
