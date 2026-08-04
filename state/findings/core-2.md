# Findings — core, worker 2

## Do these two things before anything else

1. **Run `harness/gate core`.** It has been red at a campaign's open before,
   from a cross-branch race between two loops that were each green alone
   (`state/decisions/core-002.md`). A red HEAD suspends green-or-revert.
2. **Verify every assigned gap id against `state/audit/gap-log.jsonl`.** Last
   round, three of four assigned ids were not in this branch's log at all: the
   audit that produced them ran on a commit that is not an ancestor of this
   branch, so the list describes a sibling tree, and what the branch's own
   audit recorded for the same sections was already closed. This is now
   `core-004` (harness-request, open). **An assignment is not evidence.**

## Working a section whose ids you cannot resolve

This is the procedure that produced five commits last round, and it needs no
ids. `harness/hj section-text <anchor>`, then enumerate the section's claims
one at a time and grep for the test that reads each. §14 had fifteen bullets
and six were read by nothing. It is mechanical, and it costs about ten turns.

## Where the gaps actually are

`vendor/` is done. `deps.md` is done except `#11`'s missing `--trace=<path>`
and `#15`, which no loop may close (`clippy.toml` is denied; `core-003` has
the measured thresholds — do not take it, do not "fix" §15 by editing it).
`core.md`'s manifest-shaped sections — §9's layout, `#adding-a-language`, the
licensing subsection — are now scanned in both directions from the documents
themselves.

What is left concentrates in **`crates/driver`, and it is one shape**: there
is no run loop. `driver::run` logs a config and returns, so every gap saying
"the driver owns X" — the deadline starting at request arrival, the JSONL
emission, the pending-query record, divergence reporting — is the same missing
transport seen from four sections. Classifiers, codecs and replay all exist.
A campaign taking one of those four is building the run loop; say so in the
hypothesis rather than discovering it.

## Ruled out, with the evidence

- **"The newtype sweep changed arithmetic."** It did not, where checked;
  upstream at `90d024b8` has the same code. `curl raw.githubusercontent.com`
  costs one turn and settles it.
- **"Cargo rejects a profile override naming a package outside the graph."**
  It warns and builds. Planted and measured.
- **§14's "each `allow` carries a comment saying why" is not an open claim.**
  The §15 test already enforces something stronger for `[workspace.lints.*]` —
  every lint must be printed *and argued* in `deps.md` §15 — and `vendor/*` is
  exempt by §14's next bullet. Every comment-proximity scan that holds for all
  four allow sites is a heuristic that would pass a table with one comment and
  three allows.
- **§9's four phase-2 crates cannot be built by any loop** (`loops.md` decided
  question 10; `phase.toml` names `lang_rust` rather than globbing). If
  `ce5dfefab5` re-opens, the answer is `CHANGE-core-014`, not a campaign.

## Load-bearing spec claims

Documents-as-fixtures is the pattern that pays: `fenced_toml_of` (§15's
lints), `fenced_block_of` (§9's tree), `section_of` (a markdown body). Compare
the document to the tree in **both** directions and a third copy cannot drift.

A negative check on a wrong sentence fires on the paragraph that recants it —
assert the corrected claim positively instead.

## Decisions affecting you

- core-001: Who writes `harness/measure`? [open]
- core-002: Does `measure_core` install a log subscriber? [open]
- core-003: Who writes the two `clippy.toml` thresholds? [open]
- core-004: Can an assignment be computed from an audit of this branch? [open]
