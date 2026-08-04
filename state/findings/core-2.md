# Findings — core, worker 2

## Do these two first, one turn each

1. **`harness/gate core`** before writing. A red HEAD from a cross-branch race
   has happened (`core-002`) and it suspends green-or-revert.
2. **`grep -rl '<your section>' state/decisions/`.** Settling a gap from
   `gap-log.jsonl` is necessary, not sufficient — it came back "real" for both
   of my gaps and both were dead. An **answered** record blocks a gap harder
   than an open one: nothing is left to escalate.

## Falsified — act on these directly

* **`deps.md#14` cannot go clean this phase. Do not take it.** `core-023` is
  answered *accepted A — adopt cargo-deny* and gives the work to a human.
  Re-measured twice: `deny.toml: outside core's owned paths`;
  `cargo deny: no such command`.
* **`deps.md` is exhausted for this loop.** §14's only gap is the above; §2 and
  §10 closed three campaigns ago; §5 and §6's minors closed this round. What
  remains sits in `measure_core/src/corpus.rs` (§13) and
  `measure_core.rs` (§9), plus §0's `tempfile` row, whose resolution is in
  `clippy.toml` and denied to every loop.
* **Do not follow an answered record's *Consequences* literally.** `core-021`
  said "if A, the seam test is deleted". A's replacement cannot be built by a
  loop, so deleting it trades a real check for a file nobody wrote.
* **A gap the audit really saw can already be closed.** `7d21b547b7` was fixed
  by `c9e5423` four hours after the run that opened it. The ritual answers "did
  the audit see this file", not "has anyone fixed it since" — so also
  `git log --oneline -8 -- <where-file>`, and read the subjects.
* **An assertion whose negation fails the *build* is decoration.** I wrote one
  (that `shared` re-exports `Language`/`Tree`/`InputEdit`) and removed it in
  the same experiment: `driver/src/trees.rs` imports all three. Second hit
  here. Plant before believing a test works.

## Confirmed — candidates, test on your own evidence

* **Once the gap list is exhausted, hunt for a value written twice where only
  one copy breaks.** Three of eight commits: §14's file tree is a licence table
  beside §5's; the toolchain pin sits in two files the build couples plus a
  third it does not; the upstream sha appears twice in full, three times short.
* **A comment must *name* its subject to count as an argument** — "some comment
  nearby" is satisfied by `# -- misc ---`.
* **Reconcile a wrong premise by appending, never by editing the Decision.**
  `core-023` argues the resolved graph is what "no test can reach";
  `cargo metadata --offline` reaches it. The conclusion survives, so I appended
  a Reconciliation section. A loop that rewrites the ruling it was answered
  with has un-answered itself.

## Traps that cost a red gate

* **Text scans read comments** — never quote a banned identifier in one, and
  skip comment lines in any scan you write yourself.
* **`driver` may not name `tracing_subscriber`, tests included.** Reuse
  `tests/actor.rs`'s `Capturing`.
* ENOSPC mid-campaign: the worktrees share a disk.
  `rm -rf target/debug/incremental` freed 4.7G and cost nothing.

## Decisions

`core-021`/`core-023` reconciled, untagged. `core-022` still provisional.
`core-001`/`core-003`/`harness-008`/`harness-009` need a human.
