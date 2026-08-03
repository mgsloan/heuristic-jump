Headers prefixed with the same number are run in parallel. The letter suffixes
are stable names the other documents refer to — `loops.md` §8
and `data-collection.md` use them throughout.

**Scope of the initial implementation: phases 1a through 1.5, then stop.**
Everything from 2a onward is a followup, planned once there is a working
corpus and a working measurement to plan against. `loops.md` §18 says what
that does and does not require building, and the exit criterion is that
`measure_rust replay` prints a per-stratum table over real truth data,
twice, identically.

# 1a: Core needed for measurement

Build out only the parts of core needed for implementing `measure_core` and
`measure_LANG`

This should also define an instantiable template for the language crates. The default handler should just return definition not found.

This phase also does rope-modifications.md

# 1b: Repo collection

Collect repos for the following languages: C, CPP, Go, Javascript, Typescript/TSX, Rust, Python

These should be medium sized, popular, and trustworthy. Ideally they should also be across a variety of domains and code styles in order to increase coverage.

# 1c: LSP installation

Install LSPs, potentially with human intervention. Let's also document all of
these in a "external-dependencies.md"

Install all trustworthy / popular LSPs for this set of languages that Zed supports.

# 1.5: Ground truth collection

Collect the ground truth for every language server on every repo.

# 2a: Per-language loop refining precision and recall

First, instantiate the template for the language.

Once this is no longer making material progress, it will be my decision of which point along the pareto frontier is chosen. Present a nice automated html file of charts for this.

At this point there is no shared resolution code.

# 2b: Build the LSP shim / main program

This can happen in parallel with the per-language loops. It touches disjoint
crates, so it lands on master through the same merge-after-every-green-iteration
rule as everything else (`loops.md` §13).

# 3: Whole repository loop refining latency, binary size, line count

Instead of having the per-language loops do premature optimization, this is where things are actually optimized. This is also where shared resolution logic is pulled into a shared library.

This should effectively be a refactor. The deterministic responses should not change at all. If this is preventing some optimization, this should be escalated for human review.

# 4: Repo collection for all languages built into Zed

Same as "1: Repo collection" but for all of these languages.

# 4: LSP installation

Need to install lsps for all these languages too.

# 5: Ground truth collection for all languages built into Zed

Same as "1.5: Ground truth collection", but scaled up

# 6: Per-language loop refining precision and recall

Now do this loop.  Differences with prior ones:

* Can use the shared resolution library, but still can't modify it

* Will need to bound parallelism. Doing all the languages in parallel is likely
  too much for my machine.

# 7: Whole repository loop

Now do this again with all these new languages.
