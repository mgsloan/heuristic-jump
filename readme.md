# A heuristic go-to-definition LSP shim

Ever stuck waiting for the LSP, wanting to do a "go-to-definition"?
This is a tool that provides imprecise results when the proper LSP
isn't ready.

It can simply be run like this:

```sh
$ heuristic-jump rust-analyzer
```

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

## Development plan

The plan is to have ~10 opensource repos per language, and
incrementally collect authoritative go-to-definition information from
the LSP. There will be a complete scan of all identifiers, and it will
write all of it to a file. For each identifier the scan records both
the LSP's resolved location and how long the LSP took to answer.

Of the ~10 repos per language, 2-3 are held out and never seen by
tuning sessions. Since the plan is Claude code sessions iterating
against the corpus, learning a particular repo's local conventions is
the default outcome rather than a risk. Both numbers get reported, and
a gap between tuned and held-out repos is the overfitting signal.

Claude code sessions will then be used on each language to improve the
metrics below.

## Success metrics

### Coverage at a precision floor

There are three outcomes per query: correct, wrong, or abstain.
Abstaining is cheap - the user just waits for the proper LSP, which is
the status quo. So precision is fixed first, and coverage maximized
underneath it:

> Of queries where the tool commits to an answer, >=97% must match the
> LSP. Subject to that, maximize the fraction of queries it commits on.

That committed fraction is the headline number. Plain "match rate" is
not used, because it can be improved by guessing more.

Every heuristic therefore needs a confidence notion and an explicit
"not confident enough" path. 97% is a floor and not a target - a
stratum that can't clear it should abstain rather than drag precision
down for everything else.

The reasoning: the user's alternative is waiting a few seconds. A wrong
jump costs the jump, the realization, and the trip back - but mostly it
costs trust. Once the tool is wrong often enough to warrant checking
every result, that verification cost is paid on the correct answers
too, and the tool is net negative.

### Error severity

Wrong answers are not interchangeable. Tracked separately, as a
fraction of committed answers:

* Near miss - right file, wrong line or symbol. Cheap to recover from,
  the user is already looking at the right place. Absorbs whatever
  remains of the error budget.

* Wrong file, same module tree. Moderate cost. Budget <= 1%.

* Wrong file, unrelated module or crate. This is the trust-destroying
  one. Budget <= 0.5%.

### Latency

The heuristic answer has to land inside the LSP request without the
editor noticing. Budgets, measured cold on the largest repo in the
corpus:

* p50 <= 20ms
* p99 <= 150ms
* hard cap of 250ms - past that, abstain

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
may turn out to be a permanent abstain class. Keeping it as its own row
rather than dissolving it into an average is most of what makes these
numbers honest.

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

## Future questions

1. When it's whole project search, how to choose the better module when
  it's heuristic ? Maybe something like repomap's pagerank?

2. Should the shim supervise and restart the proper LSP when it dies,
  instead of just exiting and letting the editor deal with it? The shim
  already holds authoritative text for every open document, so replaying
  state into a fresh server is nearly free, and it could serve heuristics
  through the whole gap. rust-analyzer restarts on Cargo.toml edits often
  enough that this might be the most noticeable feature. The cost is
  owning restart policy and backoff, which is real machinery.

3. **Should eager answering extend to `Slow`?** The health model can
   distinguish "slow" from "warming," but whether a slow-but-alive server
   should be pre-empted depends on how well `Slow` can be detected without
   false positives. Starting conservative.

4. **How should multi-root workspaces order search scope?** Requesting
   folder first is the obvious default, but a monorepo with many roots may
   need the pagerank-style ranking already noted in the readme's future
   questions.

5. **Does the parse LRU need a memory ceiling separate from its entry
   ceiling?** Probably, but the right number depends on measurements that do
   not exist yet.
