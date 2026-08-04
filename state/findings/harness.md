# Findings — harness loop

Theory after five campaigns. Rewritten, not appended.

## Falsified — act on these directly

* **"Judge whether the code looks right" finds nothing.** Take the section's
  literal nouns and grep for each. `tokei`, `master`, `dispatch` fell in a
  minute that way.
* **A docstring describing behaviour the function does not have is where the
  bug is.** Third campaign running.
* **A glob over `state/findings/` matches `core-1.md`..`core-3.md`.** Anything
  meaning "per loop" must read `phase.loops`. Same trap in `state/journal/`.
* **A computed prompt value can be spliced nowhere and nothing complains.**
  Check `grep -o "{{[a-z_]*}}"` on templates against `prompt_values`.
* **Do not rename a section heading.** The anchor is in
  `harness/section-baseline.toml`, denied to every loop — a rename is
  permanent drift no campaign can clear.
* **The audit list is mostly stale.** Of sixteen "open" gaps, two were real;
  §1, §2, §5, §6, §7, §11's tokei and both §13 entries were already fixed.
  Verify with one grep before taking one.
* **A selftest that reads `HARNESS / "hj"` is not hermetic.** `HARNESS`
  resolves through `HJ_REPO`, so it inspects the *worktree's* copy and fails
  every branch that predates it. The gate went red mid-campaign this way, from
  a hand-authored commit on `main`. You cannot fix it in your tree — the
  pinned copy runs first and fails before yours is consulted. Merge `main`:
  `git stash push -u -- <files>`, `git merge --no-edit main`, `git stash pop`.

## Confirmed — candidates, test on your own evidence

* **What is left is deferred mechanisms, not wrong words.** §11 and §12 were
  each one gap from clean and were one thing seen twice: a number computed at
  an iteration versus at a phase gate. Both had been deferred wholesale as
  "phase 2a work", which took their computable-today halves with them.
* **Where the code and `loops.md` disagree, it favours this loop.** Twice this
  campaign: §18 says `harness/prompts/` stays denied and the code denies only
  `auditor.md`; `harness/gate` step 5 hard-fails in the phase it exists for.
  Both are escalations (`harness-008`, `harness-009`) and neither was closed by
  editing the document. **Do not close them that way** — this loop is the
  beneficiary of the grant.
* **`decisions_reconciled` still reads tag *removals*,** so reconciling a
  decision whose choice lived in a file the loop may not write scores nothing.
  Counting a `decision:` trailer that names an already-*answered* record fixes
  it. It is a metric redefinition, so Class B.

## For the core loop specifically

`crates/lang_rust` already depends on `tree-sitter-rust`, so the workspace
`Cargo.toml` comment "no grammar crate is in the graph yet" is stale — that is
`deps.md`, which is yours. And `hj link-delta` exits 1 until `heuristic_jump`
carries one optional dependency per language behind a `lang-<x>` feature;
without it §11's authoritative size number cannot be taken at all.

## Load-bearing

* Run `harness/hj selftest` after every edit to `hj` (68 → 92 here).
* A syntax error in `hj` blocks every Edit and Write. Use bash.
* Never add a per-campaign write to a shared single-valued file.
* `gate_runs` counts any command mentioning `harness/gate`.
