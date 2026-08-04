# Findings — core, worker 1

## Falsified — act on these directly

* **Settle a stale gap from `state/audit/gap-log.jsonl`, in one turn.** Find
  the run that *opened* it, then ask whether any **later** run's
  `sections_audited` names its section. Timestamps decide nothing: a partial
  audit moves the clock without moving the judgement. Two of my three
  assignments were stale that way — opened by row 15, closed by my own
  CHANGE-core-018/019, carried forward by row 16, which audited a different
  section set. Seventh campaign running with one. Re-read the section anyway:
  three of seven commits came from that.
* **`core.md#two-modes-collect-and-replay` is exhaustively mechanised. Do not
  go looking.** Eight tests in `measure_core/tests/pipeline.rs` cover the
  deadline, the record's single writer, the no-server property, provenance
  drift, resume and both wall-clock claims; the header carries every field the
  section lists.
* **A scan over a printed block must delimit by counting braces.** A
  `}`-in-column-one rule swallowed the enum after §1's single-line `Refinement`
  and compared the wrong two lists.
* **`serde_json::Value` is `clippy::disallowed_types`.** Do not add a `serde`
  dev-dependency to read one reference file — copy `seam.rs`'s `#[expect]`.
* **`git checkout <file>` to revert a plant also reverts uncommitted work in
  it.** Plant and revert with the same tool.

## Confirmed — candidates, test on your own evidence

**When spec and code contradict, the direction is decided by which of the
*spec's own* claims survives, not by which side is easier to edit.** §4 said the
watcher catches what the on-demand path "structurally cannot", one paragraph
before "nothing depends on it … the backstop that always works". The code took
the losing side: a deleted candidate failed *every later query*, permanently in
standalone. The other two ways out each cost a standing claim — a load-bearing
watcher, or a non-exhaustive `scan`. Observing the failure cost nothing any
claim depended on, so the **code** moved and the contradicting sentence went.
Say in the changelog that both moved in one campaign.

**Narrow a classifier to what the fix can actually fix.** Only a read that
failed because the file is *gone* is evidence about the walk; a permissions
error is a fact about the file, and the walker returns the same entry next
pass — so marking stale on one is a rescan per query for as long as it lasts.

**Pin against the protocol, not a second copy of our own string.**
`reference/lsp-3.17/metaModel.json` gave the method name and its direction, and
the direction is the half §4's argument rests on.

**A printed block's *input* side drifts too, and so does prose beside code.**
`Query` was unpinned where `Outcome` was; `ServerProfile`'s **source** comment
said "the constructors are the two situations" with three declared under it. §1
also named a `crates/lang_*` source scan as what holds the commit funnel, and
nobody had written it — invisible until a precision floor arrives, which is
when there are the most `lang_*` crates to audit by hand.

## Still true

`harness/measure` (core-001), `clippy.toml` (core-003) and `deny.toml`
(core-021/023) need a human. The transport is what `driver` lacks and lives in
`shim.md`, unaudited this phase — say so in the hypothesis rather than
discovering it.
