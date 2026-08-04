---
id: harness-007
status: open
opened: 2026-08-04T20:04:00+00:00
campaign: 3e637dcd-7552-460c-8eb4-fb41941ef14b
kind: class-b
---

# Phase 1a excludes the actor, standalone and divergence reporting, and the tree contains all three — is the exclusion wrong, or is the code?

## Context

`design/loops.md` §8 says phase 1a is

> [`core.md`](core.md) in its entirety [...] Explicitly **not** the router,
> the health model, the actor, dispatch, standalone, or divergence
> reporting — all of which are [`shim.md`](shim.md) and phase 2b.

and the audit reports code on the far side of that line
(`loops.md#8-sequencing-and-gates[59c352d1d4]`, `crates/driver/src/actor.rs:1`).
It is not one file and it is not a naming accident:

* `crates/driver/src/actor.rs:1` — "`shim.md` §13's `actor.rs`", 667 lines,
  with references to `shim.md` §4's negotiation, §7's flow, §8's abstention
  and §9's report.
* `crates/driver/src/driver.rs:1` — "The LSP driver: everything
  `design/shim.md` describes".
* `crates/driver/src/config.rs:131` — `Mode::Standalone`, and `:93`
  `--proxy-only` cited as "`shim.md` §11's". Standalone is on the exclusion
  list by name.
* `crates/driver/src/pending.rs:19,148,163` — `Divergence`, "reported on
  `mismatch` only", `shim.md` §9. Divergence reporting is on the exclusion
  list by name.

**The tension is real in both directions**, which is why this is not a defect
with an obvious side. Against the exclusion list: `core.md` §5 puts the hard
cap on the driver ("enforced by the driver, not trusted to the handler") and
names the standalone deadline; §6 is the agreement predicate, which only the
driver can evaluate because it "is the only component that sees both the
heuristic answer and the server's" (`core.md:844`); §7 is one record per
query. Those claims are phase 1a's by "core.md in its entirety", and every one
of them needs a single owner of mutable state. `shim.md` §13 already gave that
owner a name and a file, so building it under that name is the natural move
and `actor.rs`'s own header argues exactly this, deferring the transport.

In favour of the exclusion list: what is on the far side is **unaudited**.
`state/phase.toml` sets the core loop's `docs = ["core.md",
"rope-modifications.md", "deps.md"]`, and its comment says "`shim.md` joins
this list at phase 2b". So 3,249 lines of `crates/driver/` exist against a
document no audit reads, and the parts of it that are genuinely `shim.md`'s —
the §4 negotiation states, the §8 abstention wire form, the §9 report — have
no oracle at all. §2's named failure mode for this loop is "spec drift; the
loop edits the spec to match the code", and the cheapest way to make this gap
disappear is to widen §8's list, which is that failure exactly.

I am not the loop that wrote this code and I may not write `crates/**` or
`state/phase.toml`, so neither the code fix nor the docs-list fix is available
to me. What is available is the spec edit, and the spec edit is the one move
that should not be made unilaterally.

## Options

**A — widen §8: the state owner is phase 1a's, the proxy is 2b's.** Drop "the
actor" from the exclusion list and replace it with what actually stays behind:
the transport (`shim.md` §2's codec, §3's router, the child spawn), the health
model, standalone as a *mode of operation*, and divergence *reporting to the
editor* as distinct from divergence being computed and recorded. Costs: it
legalises work already done, which is the one shape §19 calls indefensible,
and it leaves the `shim.md`-derived parts of `actor.rs`, `config.rs` and
`pending.rs` with no document auditing them for another whole phase.

**B — add `shim.md` to the core loop's `docs` at 1a.** Keeps §8 as written and
closes the coverage hole instead: whatever crossed the line is at least
audited against the document it came from. Costs: it is `state/phase.toml`, a
human's file; it adds `shim.md`'s sections to the denominator mid-phase, so
`sections_clean / sections_total` is not comparable across the change
(a metric redefinition, §11); and it would put phase 2b's whole gap list in
front of a loop that is meant to be finishing 1a, which is a large and
probably unwanted widening of scope.

**C — narrow the code back to core.md's claims.** `crates/driver/**` is the
core loop's. Costs: it is a real deletion of working, tested code, and the
`shim.md` references in `config.rs` and `pending.rs` are mostly *citations for
values* (`shim.md` §14.6's standalone deadline) rather than implementations of
`shim.md` behaviour, so the narrowing is a judgement call per site and not a
mechanical one.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**None of the three. §8's exclusion list stands as written, and the overlap is
stated in the document rather than removed from it.**

`design/loops.md` §8 now names the seam, names the three files that cross it,
says the crossing code is covered by no document the phase 1a audit reads, and
points here. Nothing was widened.

This is the most reversible option because it is the only one that adds
information without moving a boundary: A and B are both hard to undo — A by
sanctioning code that then accretes, B by changing what the loop's number
counts — and C destroys work. Leaving the list intact also keeps the audit
reporting the gap, which is the property that matters: this campaign could
have closed `loops.md#8-sequencing-and-gates` by deleting two words, and the
section is deliberately left with an open gap instead so that the question
survives until someone rules on it.

**There are no `// DECISION-harness-007: provisional` tags, and there cannot
be.** The affected sites are `crates/driver/src/{actor,config,pending}.rs`,
which this loop may not write, and `design/loops.md`, which `hj`'s
`DECISION_TAG_NOT_A_SITE` classifies as a mention rather than a site. A
Class B raised by one loop about another loop's code has nowhere to put the
tag; the file list above is the substitute. If that recurs, the fix is for
`hj` to read the sites from the record rather than from a grep — but once is
an observation, not a pattern.

## Consequences

* **If A:** §8's exclusion list is rewritten to the transport / health /
  editor-facing report, `loops.md#8-sequencing-and-gates` goes clean on this
  gap, and the second half of the problem — an unaudited `shim.md` surface —
  needs an answer of its own, most likely B at a smaller scope.
* **If B:** `state/phase.toml` gains `shim.md`, this campaign's §8 paragraph
  loses its last two sentences, and `sections_total` jumps. The metrics row at
  the change needs the redefinition recorded (§11), and the section baseline
  needs retaking.
* **If C:** the core loop takes a campaign of deletions and §8 needs no edit
  at all beyond this campaign's registry disambiguation, which stands under
  every option.
* **Either way the registry half stands.** `dispatch/registry.rs` is
  `core.md` §1's by that section's own words and is not part of this question;
  it was fixed as Class A (`CHANGE-harness-006`).
