# Data collection design

Phases 1b and 1.5 of [`implementation-phases.md`](implementation-phases.md):
choosing repositories, and collecting ground truth from real language
servers over them. Everything downstream is measured against what this
phase produces, so its failure mode is not "late" but "every number
after it is wrong and nothing says so."

## 0. What it produces, and when it is done

```
../heuristic-jump-corpus/<lang>/
  repos/<name>/                 checkout at a pinned commit
  positions/<name>.jsonl        every query position, enumerated once
  truth/<server>/<name>.jsonl   one per server
  manifest.toml                 what was chosen and why
../heuristic-jump-heldout/      same shape, select + final splits
```

**Gate:** for at least one language, every repository has positions, a
truth file per server with a valid provenance header, and
`measure replay` reproducing the recorded positions exactly. Plus the
oracle-determinism check in [section 5](#5-validate-the-oracle-before-trusting-it).

## 1. Repository selection

Seven languages: C, C++, Go, JavaScript, TypeScript/TSX, Rust, Python.
Ten repositories each, minus the split below.

**Size: 20k–200k lines.** This is a criterion with a reason, not a
vibe. Below ~20k a whole-project search finishes instantly and the
latency budgets in `high-level.md` are never exercised, so the corpus cannot
distinguish a fast heuristic from a slow one. Above ~200k, server
indexing time starts to dominate collection and one repository can eat a
day. The interesting behaviour — where search cost is real but bounded —
lives in between.

**Popular, actively maintained, real production code.** Not tutorials,
not benchmark suites, not one-person experiments. The convention density
of real code is the thing being learned against.

**Varied.** Different organisations, a mix of applications and
libraries, different domains. Ten repositories from the same shop teach
one house style, and the held-out split will not catch that, because the
held-out repositories were chosen the same way.

**Mostly hand-written.** A repository that is 60% generated bindings
produces positions whose "correct" answer is a file nobody reads.

### The requirement that will actually bite

**Dependencies must resolve, or the server's answers are garbage.**
This is the single largest practical risk in the phase and it is
per-language work:

| Language | Needs before the server is useful |
|---|---|
| Rust | `cargo metadata` succeeds; registry populated |
| Go | modules downloaded; `go list ./...` clean |
| Python | a resolvable environment; pyright wants a venv or a config |
| TS/JS | `node_modules` installed; `tsconfig.json` present |
| C / C++ | `compile_commands.json` — usually the hardest of the set |

A repository where this half-works is worse than one where it fails
outright: the server answers, the answers are wrong in a systematically
biased way — unresolved imports look exactly like missing definitions —
and nothing in the pipeline flags it. So **build resolution is part of
selection, not a step after it**, and the bar is *full* resolution
rather than good-enough.

### Resolution is verified, not assumed

"The build works" is a claim, and it is the claim most likely to be
wrong in the direction that quietly poisons a truth file. So it is
checked, with the machinery already being built for something else:
**sample import statements across the repository and require the server
to resolve every one.**

This reuses the readiness probe below rather than adding a mechanism,
and it measures the thing actually at stake — not whether a build system
exited zero, but whether the server can follow a reference — in a way
that is identical across languages. Parsing per-server diagnostics would
be the alternative and would need per-server knowledge of which
diagnostic codes mean "unresolved" versus "type error".

**The gate is every probe resolving.** A miss is investigated, not
absorbed into a threshold: it is either a fixable setup problem, or the
repository is replaced. If one genuinely cannot resolve — a
platform-conditional import, a generated module — it is recorded in the
manifest with its reason, so the exception is a decision somebody made
rather than a tolerance the pipeline grants silently.

Rejecting a repository is cheap at this stage and expensive later.
Nothing downstream can detect a repository that indexed at 90%: it
produces plausible answers, a plausible truth file, and a stratum
distribution slightly wrong in a way no metric names.

### The split is decided here

Tune / select / final, roughly 6–7 / 2 / 1–2 per language, assigned at
selection time and placed in the correct root immediately. Once a
repository has been in the tuning corpus it cannot be moved to held-out
— nothing un-teaches it. The third split exists because frontier
selection at phase gates consumes the second one
(`implementation-loop.md` §12).

### `manifest.toml`, and repositories are never bumped

Per repository: URL, pinned SHA, language, split, line count, domain,
what was needed to make dependencies resolve, and the date. This is what
makes selection bias auditable later, when a metric looks suspiciously
good and the question is whether the corpus earned it.

**Plain checkouts verified against the manifest — not submodules.**
Submodules would drag the corpus into the repository's own history,
which is gigabytes of other people's code, and would couple corpus
version to source version in a way nothing wants. Instead the checkouts
live in the corpus root and collection starts by checking `git rev-parse
HEAD` against the manifest for every repository, refusing to run on a
mismatch. Cheap, and it catches the accidental `git pull` that would
otherwise invalidate every byte offset silently.

**A repository is never bumped.** A newer commit is a different corpus,
and re-pinning invalidates every position and every truth file that
references it. If a repository must be replaced, it is *added* as a new
entry and the old one retires; the manifest grows rather than changes.

One consequence worth stating: **the checkout is the artifact, not the
URL.** Since nothing ever re-clones, a repository that is force-pushed,
renamed, or deleted upstream costs nothing — but losing the corpus
directory loses the corpus, and it cannot be reconstructed from the
manifest. It belongs in whatever gets backed up.

## 2. Positions are enumerated once per repository

Not once per server. `positions/<name>.jsonl` is written first, and
every server run consumes the same file.

This is what makes cross-server comparison possible at all. If each
server run enumerated its own positions, two servers' answers could not
be aligned, and the agreement / divergence split that
`core.md` §11 builds the whole per-server design
on would have nothing to join on.

A position record is `(file, byte offset, text, node kind, class)`.

### Which positions count as identifiers

Tree-sitter, not a regex — the grammar is already on hand, since
`measure_core` gets it from `LanguageHandler::grammar()`, and it knows
what a regex has to guess at.

The selection rule is **language-agnostic on purpose**: a named leaf
node whose entire text is identifier-shaped.

* **Named** excludes keywords and punctuation in most grammars for free,
  because those appear as anonymous tokens — the grammar already drew
  this line and there is no reason to redraw it in a per-language list.
  A list of node kinds per language (`identifier`, `type_identifier`,
  `field_identifier`, `property_identifier`, …) is exactly the
  per-language configuration format the rest of the design keeps
  refusing.
* **Leaf** — no named children — is what makes it a token rather than a
  construct containing one.
* **Identifier-shaped text** catches what the first two miss: grammars
  that make some keywords named (`self`, `true`, `super` are named nodes
  in several), and named leaves that are literals or comments. A string,
  a number, and `// note` all fail the shape test; `self` passes and is
  kept, because go-to-definition on `self` is a real query with a real
  answer.

**The offset is the identifier's start.** A cursor can sit anywhere
inside a token and the handler must behave identically wherever it sits,
but that invariance is better asserted by a property test than paid for
with corpus positions — sampling random intra-token offsets would spend
the budget re-measuring the same query and make every position harder to
reason about.

### Non-identifier positions

**About 100 per language, total — not per repository.** Keywords,
punctuation, string interiors, comments, whitespace: places a user can
press go-to-definition where the honest answer is nothing.

They exist to prove the `NotAnIdentifier` abstention path fires on real
input, not to measure anything, so a hundred is plenty and the sample
does not need to be representative.

**They carry their own denominator and never enter the main one.** On
these positions answering nothing is correct, so folding them into
coverage would mix two different questions and move the headline number
for a reason that has nothing to do with resolution.

## 3. Sampling

**Uniform random, capped per repository. Start at 20k positions.**

Exhaustive enumeration is what `high-level.md` describes, and it does not
survive contact with arithmetic: a 100k-line repository has on the order
of half a million identifier occurrences, and at 20ms per query that is
hours per repository per server, before any server is slower than that.
Across ~70 repositories and more servers than languages it is not a
hundred machine-hours, it is thousands.

Uniform rather than stratified, deliberately. Stratified sampling would
spend the budget where the heuristic is interesting rather than where
identifiers happen to be — but strata are defined by *where the
definition turned out to be*, which is exactly what is not known before
the LSP answers. Pre-classifying with our own logic would make the
corpus's stratum labels come from the code under measurement, which is
circular in the direction that hides errors.

So: sample uniformly, record the sampling rate, and let strata fall
where they fall. The per-stratum counts will be lopsided — dominated by
locals and same-file references, per `high-level.md` — and thin strata get
wide confidence intervals, which is honest rather than convenient.

**One exhaustive repository per language**, chosen small, to measure the
true stratum distribution and validate that the sample resembles it. If
it does not, this decision gets revisited with data instead of the
arithmetic above.

### Known issue: thin strata

Uniform sampling buys unbiased proportional coverage and does not buy
enough `AmbiguousName` or `TypeInferenceRequired` positions to
distinguish two candidate rankings. With 20k positions per repository
and those strata at a few percent, the rarest rows will carry confidence
intervals wide enough to hide a real improvement.

It is recorded here and **not solved here.** Strata are defined by the
resolution logic — they are `resolution.md`'s categories, reported by
the handler, and they do not exist at collection time. A corpus that
oversampled them would have to classify positions before the answers
exist, using the code under measurement, which is the circularity §3
avoids. So the question of whether a stratum has enough data to tune
against belongs to the phase that tunes against it; this document's job
is to sample without bias and say how much of each thing it got.

## 4. Ground truth collection

### The server matrix

Installed and pinned in phase 1c, documented in
`external-dependencies.md`, and recorded by version in each truth file's
provenance header. Where a language has one usable server the
multi-server machinery is inert.

**Version drift is checked, not assumed.** `measure collect` reads the
installed server's version and compares it against the manifest:

* fresh collection with a different version — warn, record what was
  actually used, and continue, since the version in the header is what
  makes the file interpretable;
* resuming or appending to an existing truth file with a different
  version — refuse. Half a file from one version and half from another
  is the one outcome with no honest provenance header.

The set is "every trustworthy server Zed supports for the language",
which is what makes the list below a starting point rather than a
closed set.

| Language | Servers |
|---|---|
| Rust | rust-analyzer |
| Go | gopls |
| Python | pyright, pylsp |
| TS/JS | typescript-language-server, vtsls |
| C / C++ | clangd, ccls |

### Readiness is the correctness crux

**Querying before the index is built returns confidently wrong
answers**, usually empty ones, and nothing distinguishes them from real
"no definition here." A truth file collected during indexing is
poisoned in a way that looks like the heuristic doing well.

Every server signals readiness differently — progress notifications,
custom notifications, or nothing at all. That is the same problem
`core.md` §7 solves for the shim, so the same
per-server adapter knowledge applies, though `measure_core` uses it to
*wait* rather than to race.

Belt and braces: after readiness is signalled, issue a small set of
known-answerable probe queries and require them to resolve before
starting the run. A server that claims ready and answers nothing is a
condition to detect at position zero, not at position 20,000.

### The run

Per (repository, server): start the server, wait for ready, probe, then
walk `positions.jsonl` issuing `textDocument/definition`, recording the
answer and the elapsed time.

The answer is a *list*, and all of it is recorded in the order the server
gave it. `textDocument/definition` may legitimately return several
locations, and `core.md` §10 classifies agreement against the child's
whole set — so a truth file that kept only the first location would make
`contained` uncomputable and silently understate the shim. Checkpoint every few hundred positions.

**Resumable at two granularities**, because a hundred machine-hours will
be interrupted: (repository, server) is the unit of work, and a
checkpoint within a repository means a crash costs minutes rather than
the repository. A partially collected truth file is marked incomplete in
its header and is never consumed by replay.

Concurrency: several requests in flight per server, tuned per server
rather than assumed — some handle pipelining well and some serialise
internally, and the difference is an order of magnitude in wall clock.
Multiple (repository, server) pairs in parallel is the safer axis, and
is bounded by RAM rather than CPU, since a warm index is large.

### Failures are recorded, not dropped

Four outcomes, kept distinct:

| Outcome | Meaning |
|---|---|
| `resolved` | one or more locations, **in the server's order** |
| `none` | server answered, no definition — a real answer |
| `error` | server returned an error |
| `timeout` | no answer inside the cap |

Collapsing `error` or `timeout` into `none` is the mistake that quietly
inflates precision later, because the heuristic gets credit for
abstaining where the oracle merely failed. Only `resolved` and `none`
are ground truth; the other two are excluded from metrics and reported
as a coverage figure for the *collection*, which is itself a quality
signal about the repository's build setup.

## 5. Validate the oracle before trusting it

Two checks, both cheap, both capable of invalidating the premise:

* **Determinism.** Collect one repository twice with the same server
  and version; diff. Nonzero divergence means the oracle is not a
  function of the code, and every downstream comparison inherits that
  noise. Better to learn this on one repository than to infer it from a
  confusing metric six weeks later.
* **Sanity.** Hand-check a few dozen answers per language. This is the
  only step in the pipeline that can catch a systematically wrong setup
  — the wrong `tsconfig`, a stale index, a server resolving into the
  wrong SDK — and it costs an hour.

## 6. Cost

Almost no tokens and almost no model time, which is exactly why this
phase is easy to under-plan: it is invisible to the accounting that
watches everything else (`implementation-loop.md` §15).

Rough shape, to be replaced with measurements after the first language:

| Item | Estimate |
|---|---|
| Repository selection + dependency resolution | days of human work, front-loaded on C/C++ |
| Indexing, per (repo, server) | minutes to an hour |
| Querying, 20k positions | ~10 min to a few hours, server-dependent |
| Full matrix | ~100 machine-hours, parallelisable |
| Storage | repositories dominate; truth files are small |

The first language through is the calibration run. Do Rust first: one
server, the best tooling, and the fewest ways for dependency resolution
to fail — so the pipeline gets debugged where the language is not also
fighting back. Do C++ second rather than last, because if
`compile_commands.json` turns out to be a wall, that should be known
while there is still time to drop the language rather than after
everything else is built around it.

## Decided

The questions this document opened with, settled — recorded because the
reasoning matters more than the answer:

* **20k positions per repository.** Kept.
* **Repositories are never bumped**, so the "refresh positions on
  re-pin" problem does not arise. SHA manifest, verified checkouts, no
  submodules — [above](#manifesttoml-and-repositories-are-never-bumped).
* **Non-identifier positions are included, ~100 per language**, in their
  own denominator — [above](#non-identifier-positions).
* **Server versions are recorded and checked**, warning on a fresh
  collection and refusing on a resume — [above](#the-server-matrix).
  Containers per server version stay available if drift turns out to be
  recurring rather than occasional.
* **The identifier-shape rule stays language-agnostic.** It keeps a few
  things a per-language kind list would drop — `true`, `self`, label and
  macro names — and most of those are legitimate queries. The cost is a
  slightly noisier denominator rather than a wrong one.
* **Dependencies must resolve fully**, verified rather than assumed —
  [above](#resolution-is-verified-not-assumed).

One known issue is carried rather than closed: [thin
strata](#known-issue-thin-strata), which belongs to the tuning phase
rather than to collection.
