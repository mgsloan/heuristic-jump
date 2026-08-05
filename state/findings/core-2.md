# Findings — core, worker 2

## Do these three first, one turn each

1. **`harness/gate core`** before writing. A red HEAD from a cross-branch race
   has happened (`core-002`) and it suspends green-or-revert.
2. **Read your gap's claim against the section text spliced into your prompt.**
   The splice is the *current* document, so a gap that quotes it is already
   answered there. This caught one of my two gaps in zero turns.
3. **`grep -rn '<gap-id>' state/` and `grep -rl '<section>' state/decisions/`.**
   Absent id → the gap does not exist (my other one: section `clean`, no gap,
   no minor). An **answered** decision blocks harder than an open one.

## Falsified — act on these directly

* **The audit's section state does not locate the work.** `#two-modes` was
  `gaps` for a sentence already fixed; `#both-sides-are-sets` was **clean**
  with two normative sentences nothing implemented. Clean ≠ checked.
* **`deps.md#14` cannot go clean this phase.** `core-023` is answered
  *accepted A*; `deny.toml` is outside every loop's paths and `cargo deny` is
  not installed. Re-measured twice.
* **`deps.md` is exhausted for this loop.** §2/§10 closed three campaigns ago;
  §5/§6 minors closed; §0's `tempfile` row resolves in `clippy.toml`, denied.
* **Do not follow an answered record's *Consequences* literally.** `core-021`
  said to delete the seam test; its replacement cannot be built by a loop.
* **A gap the audit really saw can already be fixed** — check
  `git log --oneline -8 -- <where-file>` and read the subjects.
* **An assertion whose negation fails the *build* is decoration.** Third hit
  across workers. Plant before believing a test works.

## Confirmed — candidates, test on your own evidence

* **Hunt for a metric the document *names* against the column printed under
  that name.** The table printed the disjoint `MatchContained` counter under a
  header reading `contained`; the section defines containment as *any* match
  and says `match_top1` implies `match_contained`. 42.9% against 21.4% on one
  fixture. Same shape as "a value written twice", with a header string as the
  second copy.
* **Plant each half separately; a plant can rewrite the design.** Planting
  "one per answer" for a result count showed the fixture oracle returns one
  location per row — so a count taken from the child's side reads 1.0 for a
  handler returning two, hiding exactly the gaming the column exists to expose.
* **Reconcile a wrong premise by appending, never by editing the Decision.**

## Verified covered — do not re-walk

`measure_core`: both my sections, sentence by sentence. Byte-identical tables
across runs, frozen `lsp_latency_us`, the three subcommands and their flags,
the provenance header, replay's absent deadline, the records/table
reconciliation (now including the result count). The driver half of
`#both-sides-are-sets` — mismatch-only divergence, rank preservation — is
covered in `driver/tests/pending.rs`.

## Open question for the next campaign

`--format json` carries counters and no ratios, so a consumer computing
containment as `match_contained / judged` repeats the misreading I just fixed
in the text table. Coverage and precision are text-only too, so changing this
is a decision about §7's report surface — closer to Class B than to a fix.

## Traps that cost a red gate

Text scans read comments — skip comment lines. `driver` may not name
`tracing_subscriber`, tests included. ENOSPC: `rm -rf target/debug/incremental`.

## Decisions

`core-021`/`core-023` reconciled. `core-022` still provisional.
`core-001`/`core-003`/`harness-008`/`harness-009` need a human.
