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
outright: the server answers, the answers are wrong in a
systematically biased way (unresolved imports look like missing
definitions), and nothing in the pipeline flags it. So **build
resolution is part of selection, not a step after it** — a candidate
that cannot be brought to a working index is rejected and replaced, and
the manifest records what it took.

### The split is decided here

Tune / select / final, roughly 6–7 / 2 / 1–2 per language, assigned at
selection time and placed in the correct root immediately. Once a
repository has been in the tuning corpus it cannot be moved to held-out
— nothing un-teaches it. The third split exists because frontier
selection at phase gates consumes the second one
(`implementation-loop.md` §12).

### `manifest.toml`

Per repository: URL, pinned commit, language, split, line count,
domain, what was needed to make dependencies resolve, and the date.
This is what makes selection bias auditable later, when a metric looks
suspiciously good and the question is whether the corpus earned it.

## 2. Positions are enumerated once per repository

Not once per server. `positions/<name>.jsonl` is written first, and
every server run consumes the same file.

This is what makes cross-server comparison possible at all. If each
server run enumerated its own positions, two servers' answers could not
be aligned, and the agreement / divergence split that
`core.md` §11 builds the whole per-server design
on would have nothing to join on.

A position record is `(file, byte offset, identifier text, node kind)`.
Enumeration walks the tree-sitter parse and takes identifier nodes —
grammar-level only, no resolution logic, so nothing here depends on code
that is still being written.

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

## 4. Ground truth collection

### The server matrix

Installed and pinned in phase 1c, documented in
`external-dependencies.md`, and recorded by version in each truth file's
provenance header. Where a language has one usable server the
multi-server machinery is inert.

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

## Open questions

1. **Is 20k positions per repository the right cap?** Chosen so the
   matrix fits in ~100 machine-hours. The right number depends on how
   thin the interesting strata turn out to be, which the exhaustive
   repositories will say.

2. **Do the sampled positions need refreshing when a repository is
   re-pinned?** Bumping a commit invalidates byte offsets. Simplest is
   to treat a re-pin as a new corpus version and recollect, which is
   correct and expensive. Whether anything cheaper is worth building
   depends on how often repositories actually need bumping — plausibly
   never, since nothing forces the corpus forward.

3. **Should positions include non-identifier locations?** Users press
   go-to-definition on keywords, string literals, and whitespace, and
   `NotAnIdentifier` is a real abstention path the corpus currently
   cannot measure at all. Adding them is cheap; the question is whether
   they belong in the same denominator as real queries.

4. **How are servers kept pinned in practice?** Version drift silently
   invalidates truth files. Containers per server version is the honest
   answer and the heavier one; a recorded version plus a check at
   collection time is the cheap one that catches drift after the fact
   rather than preventing it.
