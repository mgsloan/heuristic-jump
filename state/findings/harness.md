# Findings — harness loop

Theory after ten campaigns.

## Falsified — act on these directly

* **A new `hj` subcommand and its shell caller cannot land in one commit.**
  The reviewed selftest scans the candidate tree's scripts against the
  *reviewed* `hj`'s parsers, which predate the new one. `sibling_hj_source`
  fixes it from the merge after `2953c426`; `KNOWN_KINDS` still has the same
  latent defect one seam over.
* **The audit's warned gaps are usually the last campaign's closed ones.**
  Check them against the previous journal entry's targets first.
* **Check whether a producer's *type* is in the tree before calling something
  unbuildable.** And check the other design documents before inventing a name:
  `external-dependencies.md` §7 already pinned the CLI *and* named the
  intervention kind for an upgrade.
* **"Judge whether the code looks right" finds nothing.** Grep the nouns.
* **`decisions_reconciled` reads tag *removals***, so reconciling one tagged in
  a denied file scores nothing. Never reconcile one still `open`.
* **A check reading `HARNESS` runs in every loop's worktree** (gate step 3), so
  one that fails on an unmerged tree turns another loop's gate red over a file
  it is denied. Read `PINNED_HARNESS` in selftests; `HARNESS` only in checks
  scoped to `writes_harness`.

## Confirmed — candidates, test on your own evidence

* **When the prompt and the document disagree, the intervention log decides
  which is stale.** A human's `prompt-revised` row plus an argued commit
  message means the document was never updated — that is not the
  spec-toward-code move §7 warns about, and saying so in the changelog is the
  whole cost.
* **Prompt properties are checkable in template coordinates**, above
  `BODY_END` or over the whole template file. Rendered coordinates sweep in
  the journal and the digest, where a loop names checks legitimately.
* **Derive a check's vocabulary from the artifact** (`harness/gate`'s steps,
  the pin in a document) rather than listing it — a list in two files
  eventually disagrees with itself.
* **Build a consumer in the same campaign as a producer.** The dashboard has
  caught two producer bugs a selftest passed.
* **Backtest before a metric change** (`progress-replay --rule N`), and expect
  an aggregate over strata to hide the stratum that matters.

## Do not spend a campaign on these

* Implemented: §5, §7, §9, §10, §11, §12, §14, §16, §17's CLI half.
* `harness/loop`'s two gaps — `[02927bbcaa]`, `[3530d46864]`. Gating the
  rebase needs step 7 not to demand a row for a sha the rebase rewrote: a
  denied file, or a metric redefinition. Both need a scratch-repo fixture; the
  close path cannot be exercised without a real merge.
* `#the-supervisor-…[cdb07d9f93]` is the largest thing left and wants its own
  campaign.
* `harness-012` (§10's third work counter) and `harness-013` (whether the
  prompt may state §7's progress terms) are open. Do not close either by
  editing the spec.

## Load-bearing

* `hj selftest` after every `hj` edit; `--across-worktrees` before committing.
* The previous close commit has no metrics row, so your first gate fails at
  step 7; `harness/hj record <loop>` fixes it. Run `record` after your own
  close commit so the next campaign does not inherit it.
