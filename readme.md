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

## Status

Design only. No code has been written yet - the repository is the four
directories below minus the ones that hold source, and everything in
`design/` is a plan rather than a description.

## Planned layout

```
readme.md            this file
CLAUDE.md            coding rules - the fixed constraints every session works under
clippy.toml
design/              the design documents, described below
harness/             loop runner, gates, ratchets, dashboard. Owned by no
                     agent session; a loop cannot weaken its own gate
spec/                ledger.toml - design claims extracted into a checkable
                     work queue, one entry per claim
state/               loop state: metrics, journals, campaigns, decisions,
                     interventions, cost. Partitioned by owner
vendor/
  rope/              copied from Zed, GPL-3.0-or-later
  sum_tree/          copied from Zed, Apache-2.0
crates/
  shared/            the handler seam, vocabulary newtypes, ProjectView,
                     hand-written LSP wire types, the one error enum
  similarity/        ported from the prior implementation, frozen until phase 3
  lang_rust/         one crate per language, no shared resolution code
  lang_python/
  driver/            the LSP proxy - routing, health, document state, dispatch
  heuristic_jump/    the shipped binary, and the only place languages are listed
  measure_core/      corpus collection and replay; an LSP client, not a proxy
  measure_rust/      four lines each - the binary that measures one language
  measure_python/
```

Two dependency rules shape all of it. **`driver` depends on no language
crate**, so adding a language touches nothing else. **`measure_core`
depends on no language either**, taking its handler as `&dyn`, so one
language can be measured without any other language building - which is
what lets per-language work happen in parallel.

The corpus lives outside the repository, in two roots so that held-out
data is separated by a path rather than by an honour system:

```
../heuristic-jump-corpus/    repos, sampled positions, ground truth
../heuristic-jump-heldout/   the select and final splits
```

## The design documents

Read `design/high-level.md` first; it is the only one that stands alone.

| Document | What it covers |
|---|---|
| `high-level.md` | What the tool is and how it is judged: the retry protocol, divergence reporting, the success metrics, and the per-stratum table those metrics are reported in. Start here |
| `implementation-phases.md` | The phase ordering, and short enough to read in a minute. It is the authority the other documents defer to |
| `core.md` | The driver. Process and thread model, message routing, server health, document state, deadlines, the `LanguageHandler` seam, the measurement record, standalone mode, and the hand-written protocol types. By far the largest |
| `resolution.md` | What a language handler actually does with a reference - the pipeline stages, candidate verification, ranking, strata - worked out longhand against Rust |
| `data-collection.md` | Choosing repositories and collecting ground truth from real language servers. Phases 1b through 1.5 |
| `implementation-loop.md` | How the autonomous coding loops run: the spec ledger, the gate, campaigns, isolation between languages, the Pareto frontier and phase gates, cost accounting, and the operator dashboard |
| `deps.md` | Every dependency chosen and every one rejected, with the reasoning. Consult before adding anything |
| `rope-modifications.md` | What changes in the vendored Zed rope, and what deliberately does not |
| `open-questions.md` | Numbered, and referenced by number from everywhere else |

Two conventions worth knowing before editing any of them. Decisions are
recorded with the reasoning that produced them, including the arguments
against - a decision whose alternatives are not written down gets
relitigated. And questions that are genuinely undecided live in
`open-questions.md` under a stable number rather than being resolved
prematurely in prose.

## Licensing

Our crates are MIT. The shipped binary is GPL-3.0-or-later, because it
links the vendored `rope`. That is the only GPL input, which is
deliberate: replacing it would make the whole workspace permissively
licensable without relicensing anything we wrote. `design/deps.md` §5
has the per-crate table.
