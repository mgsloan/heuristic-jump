# Findings — harness loop

Theory after six campaigns. Rewritten, not appended.

## Falsified — act on these directly

* **"Judge whether the code looks right" finds nothing.** Grep the section's
  literal nouns. `tokei`, `master`, `merge-blocked` fell in a minute that way.
* **A docstring describing behaviour the function does not have is where the
  bug is.** Fourth campaign running.
* **A glob over `state/findings/` matches `core-1.md`..`core-3.md`.** Anything
  meaning "per loop" reads `phase.loops`. Same trap in `state/journal/`.
* **Do not rename a section heading.** The anchor is in
  `harness/section-baseline.toml`, denied to every loop — permanent drift.
* **The audit list lags; verify with one grep.** But its *one-line* list is the
  best target-picker there is: sections one gap from clean, twice running.
* **A closed gap's section cannot be derived from its id.** `gap_id` is
  `sha256(section|claim)[:10]` and a closed gap is gone from the audit. It must
  be recorded when it closes — that is what `closed_gaps` is for.
* **A selftest reading `HARNESS / x` is not hermetic.** `HARNESS` resolves
  through `HJ_REPO`, so a pinned assertion judges *your* tree. This has now cost
  two campaigns. **Before reverting, measure:** stash your work, run the gate at
  HEAD, unstash. If HEAD is red the breakage is not yours and "revert to green"
  has no green — `main` may not carry the fix either, because the counterpart
  can be sitting uncommitted in the integration checkout. `harness-011`.

## Confirmed — candidates, test on your own evidence

* **What is left is deferred mechanisms, not wrong words.** Six sections now:
  each had a cheap mechanical half deferred along with an expensive one.
* **Where the code and `loops.md` disagree, it favours this loop.**
  `harness-008`, `harness-009` still open. **Do not close them by editing the
  document** — this loop is the beneficiary.
* **Never reconcile a decision whose `status:` is still `open` in your tree**,
  even when a gate check demands it. That is ruling on your own escalation.
* **Backtest before believing a metric change.** `hj progress-replay --rule N`.
  My first rule-3 draft was *looser* than rule 2, and its headline number was an
  artifact of the replay modelling only half of what the live code ORs together.
* **`decisions_reconciled` still reads tag *removals***, so reconciling a
  decision whose site is in a file the loop may not write scores nothing.
  Counting a `decision:` trailer naming an already-*answered* record fixes it.
  Class B.

## Load-bearing

* `harness/hj selftest` after every edit to `hj` (92 → 101 here).
* A syntax error in `hj` blocks every Edit and Write. Use bash.
* The shell/`hj` seam now has two scans (`--kind` literals, `"$hj" <sub>`).
  Both read the worktree deliberately — see the falsified note above.
* Never add a per-campaign write to a shared single-valued file.

## For the core loop

`crates/lang_rust` already depends on `tree-sitter-rust`, so the workspace
`Cargo.toml` comment "no grammar crate is in the graph yet" is stale — that is
`deps.md`, which is yours.
