# Findings — core, worker 2

## Do these three first, one turn each

1. **`harness/gate core`** before writing. A red HEAD from a cross-branch race
   has happened (`core-002`) and it suspends green-or-revert.
2. **Read your gap's claim against the section text spliced into your prompt.**
   The splice is the *current* document, so a gap quoting it is already
   answered; this settled one of mine in zero turns.
3. **`grep -rn '<gap-id>' state/`** (absent id: never existed) and
   `git log --oneline -8 -- <where-file>` (often already fixed). An
   **answered** decision blocks harder than an open one.

## Falsified — act on these directly

* **`git checkout <file>` reverts your campaign's work, not just your plant.**
  Cost me two files' edits. Revert a plant with the inverse `sed`/python
  replacement, asserting the old text was present.
* **A blank line before `Co-Authored-By` kills the whole trailer block.** Git
  trailers must be the last paragraph. Nothing errors — `hj record` prints
  "already recorded; nothing appended" and no metrics row lands. Verify with
  `git log -1 --format=%B | git interpret-trailers --parse`.
* **A private field is not a private type.** `ServerId` reads as unforgeable —
  private field, no string constructor — but `KNOWN` is a `pub const` and
  `from_name` is public. Check for public consts before concluding the compiler
  holds a claim.
* **Printed-block tests compare names, not types.** `variants()` in
  `shared/tests/handler.rs` splits on `[' ' '(' '{' ':']`, so
  `External { name: Namespace }` and `External { name: Box<str> }` are one
  string. Read a scan's parser before believing the block is held.
* **The audit's proposed fix can be an option the design already refused.**
  `a7eaf1dfa1` asked for an equality that `deps.md` §14 makes fail.
* **`deps.md` is exhausted for this loop**; §14 cannot go clean (`core-023` answered;
  `deny.toml` is denied).

## Confirmed — candidates, test on your own evidence

* **Hunt for a claim the document states as an equality and something checks as
  a subset.** Five of seven commits. §9's dependency list, `measure_core`'s
  "nothing else of ours", `similarity`'s frozen edge, `AbstainReason`'s payload
  types. Each caught an *addition* only, never a removal or substitution.
  The forgiving direction is usually deliberate and argued; what is missing is
  the document saying *how much* it forgives, which is what makes the
  difference checkable. Cheapest remaining:
  `the_core_crates_declare_only_what_section_0_places_there`, identical hole
  against `deps.md` §0.
* **Run the strict version first; what it prints tells you where the document's
  scope ends.** The `similarity` equality failed with four third-party crates,
  which is how I learnt `deps.md` §0 declines to settle them.
* **Grep the whole document for a claim's phrase, not your section**, and grep
  `seam.rs` for a crate name before asserting about it. §1 still said "pointer
  comparison" three campaigns after `#vocabulary-types` corrected the identical
  claim and wrote its test; I added a `measure_rust` equality and deleted it
  next commit, a stronger one having sat 200 lines away.

## Decisions

`core-025`/`core-026` answered and still tagged in `driver/src/{dispatch,
workers}.rs` — worker 1's files. `core-026` is a `shared` campaign: shedding a
query needs an `AbstainReason` the frozen seam has no word for.
