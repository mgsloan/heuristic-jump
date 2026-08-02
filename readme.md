# A heuristic go-to-definition LSP shim

Ever stuck waiting for the LSP, wanting to do a "go-to-definition"?
This is a tool that provides imprecise results when the proper LSP
isn't ready.

It can simply be run like this:

```sh
$ heuristic-jump rust-analyzer
```

It also runs with no language server behind it at all:

```sh
$ heuristic-jump --hj-standalone
```

In that mode it is the whole language server - it answers `initialize`
itself and serves nothing but heuristic go-to-definition. That is
for languages with no usable server, for debugging the heuristic
without a server's variability in the picture, and for editors that
can run several servers per language. It has no ground truth, so it
borrows its calibration from the proxy mode that does. See
`core-implementation-design.md` section 17.

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

Landing within 3 lines of the proper LSP's answer counts as a match, not
an error. At that distance the right definition is on screen and the
user is already looking at it, so calling it wrong would measure
something nobody experiences as wrong.

* Same file, further off than 3 lines. Recoverable - the user is at
  least in the right place - but they have to hunt. Absorbs whatever
  remains of the error budget.

* Wrong file, same module tree. Moderate cost. Budget <= 1%.

* Wrong file, unrelated module or crate. This is the trust-destroying
  one. Budget <= 0.5%.

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

## License

The crates in `crates/` are MIT.

The shipped binary is GPL-3.0-or-later, because it links a vendored
copy of Zed's `rope` (`vendor/rope`), which is GPL-3.0-or-later.
`vendor/sum_tree` and `vendor/util` are Apache-2.0.

Keeping our own crates MIT is deliberate: `rope` is the only GPL
input, so replacing it would make the whole workspace permissively
licensable without relicensing anything. See `dependency-plan.md`
section 5.

## Future questions / work

1. When it's whole project search, how to choose the better module when
   it's heuristic? Maybe something like repomap's pagerank?

2. Does the editor actually send a second go-to-definition request while the
   first is still pending? It might instead cancel the first and send a new one,
   or dedupe and send nothing at all. The entire retry-triggered design assumes
   the first of these. This needs a trace from Zed and from VS Code, pressing
   go-to-definition twice against a deliberately slow server, before much is
   built on top of it.

   - *Zed* does send two requests

3. The shim caps concurrent heuristic queries and abstains past the cap. But a
   retry is itself a new query, so under load the second press - the one the
   whole retry protocol exists to serve - could be the one that gets dropped.
   Should retries of an already-pending spot bypass the cap, or hold reserved
   slots?

4. Should a slow-but-alive proper LSP be pre-empted, the way a warming one is?
   Right now it isn't - a server that has answered one definition request is
   treated as ready no matter how slowly it answers afterward. Doing otherwise
   reintroduces a "slow" health state, detected against the server's own rolling
   baseline rather than an absolute threshold. Whether that can be done without
   false positives needs measurements that don't exist yet, so the conservative
   version ships first.

5. Disk-file parse caches key on (path, mtime, len). Second-granularity mtime on
   some filesystems means a same-second rewrite of the same length serves a
   stale tree. Is a content hash worth the read, or is this rare enough to
   accept?

6. What should the shim do when the editor misbehaves - didOpen for a document
   already open, didChange for one never opened, didClose for one that isn't?
   Ignoring is probably right, but silently ignoring hides editor bugs the user
   would want reported.

7. Should the shim supervise and restart the proper LSP when it dies,
   instead of just exiting and letting the editor deal with it? The shim
   already holds authoritative text for every open document, so replaying
   state into a fresh server is nearly free, and it could serve heuristics
   through the whole gap. rust-analyzer restarts on Cargo.toml edits often
   enough that this might be the most noticeable feature. The cost is
   owning restart policy and backoff, which is real machinery.

8. How should multi-root workspaces order search scope? The folder containing
   the requesting document first is the obvious default, but a monorepo with
   many roots may want the pagerank-style ranking from question 1 instead.

9. Does the parse cache need a memory ceiling separate from its entry ceiling?
   Probably - one generated file can be enormous - but the right number depends
   on measurements that don't exist yet.

10. **Does standalone want the watcher that proxy mode defers?**
    `dependency-plan.md` section 7 defers `notify` because a stale file list
    costs a miss the proper LSP covers. In standalone it costs a permanent miss,
    so the case for watching is stronger here — possibly strong enough to make
    standalone the reason the watcher gets built.

11. **Error or `null` for abstention?** `core-implementation-design.md`
    section 17.5 picks the error on section 9's reasoning, but that reasoning was written
    about a transiently unresponsive server, where the failure really is
    transient. In standalone an abstention is permanent for that spot, and a
    permanent failure reported as a transient one is its own small lie. Needs a
    look at what Zed and VS Code actually render for each.

12. **Revisit precision floor** - actually seems useful to return questionable
results.

13. **Should the precision floor differ by mode?**
    `core-implementation-design.md` section 17.6 says no, for v1, on trust
    grounds. It is the change most likely to be worth
    making later and the one most likely to be made for bad reasons, so it
    should require a measurement rather than an argument.
