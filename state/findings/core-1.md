# Findings — core, worker 1

## Confirmed — candidates, test them on your own evidence

**A spec-vs-code gap is only editable toward the code when somebody else
already ruled.** Five corrections this campaign, each decided by a document or
a record I did not write: `conformance-013` (accepted) for §1's seam block,
`conformance-005` (accepted) for `deps.md` §8's disk cache,
`data-collection.md` §2 for `#two-modes`' subcommands, §8.2's third list for
§8.3's derive. The test is one grep of `state/decisions/` for the type name. No
ruling, no Class A edit — and naming the ruling in the changelog is what makes
the edit checkable rather than plausible.

**A printed block is prose, and prose is where a stale count survives.**
`measure_core/tests/pipeline.rs` has pinned `#the-command-line` against `clap`
for months, with a doc comment saying the document must be the fixture because
editing it is how progress is faked. It stops at the section boundary — which
is exactly how the section *directly above* it said "two subcommands" while
three were pinned below. **A fact pinned in one section is unpinned in the
section next door that restates it.** Three tests now read the document: the
mode split (`pipeline.rs`), §1's seam block (`shared/tests/handler.rs`, new),
§8.2's third table (`proto.rs`).

**Pin by names and arity, never by transcription.** A test demanding the block
match the source would make the block unwritable — it elides bodies, derives
and doc comments on purpose — and would be repaired by weakening it.

## Falsified — act on these directly

* **`core.md#83[f9ad1766b7]` was stale**, closed by CHANGE-core-007 thirteen
  minutes after the audit stamped the section. Sixth campaign running with a
  stale assignment. It still had a real defect one sentence further along, so
  **take a stale assignment and re-read the section**: that produced three of
  this campaign's five corrections.
* **You cannot assert that a `BOTH` type carries both serde derives.** Written,
  passing, removed in the same commit: dropping `Serialize` from any of the
  five — derived, or the hand-written `impl Serialize for
  TextDocumentSyncKind` — fails the *build*, because each is embedded in a type
  on the other list. The pair is compiler-held; only the document was at risk.
* **`deps.md#10-errors[d50e2285d0]` is held by another worker** — claimed,
  refused, one turn. §1's deadline bullet is its normative source.
* **`resolution.md` §3 carries the parse-cache expectation from the other
  side**, and conformance-005's answer names that correction explicitly. Out of
  this phase's audit scope, so it buys no number.

## Still true from earlier campaigns

* `harness/measure` (core-001), the capture tooling's home (core-020),
  `clippy.toml` (core-003), `deny.toml` (core-021/023) — all need a human.
* The transport is what `driver` is missing, and it lives in `shim.md`, which
  this phase does not audit. Say so in the hypothesis rather than discovering
  it.
* `hard_cap` is not the only place a late answer dies: `encode` reads the
  target file and `ProjectView` refuses an expired read, so a **cross-file**
  late answer never reaches the cap. A gap that names one site is a hypothesis
  about how many there are.
* Free functions in a `tests/` file need the file-level
  `#![expect(clippy::expect_used, clippy::panic, reason = ...)]` —
  `clippy.toml`'s allowance reaches `#[test]` bodies only.
* The machine has `rust-analyzer`, `gopls`, `pyright` and `emacs` 30.2 with
  eglot (the only headless LSP *client* here). Check `which` before calling a
  gap blocked.
