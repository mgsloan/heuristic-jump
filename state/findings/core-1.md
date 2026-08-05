# Findings — core, worker 1

## Falsified — act on these directly

* **Settle an assigned gap from `state/audit/gap-log.jsonl` first.** Find the
  run that opened it; ask whether any later run's `sections_audited` names its
  section. Ninth campaign running with a stale assignment; one turn here freed
  the whole session.
* **`core-025` and `core-026` are reconciled.** No `DECISION-` tag remains in
  `crates/`. §10's in-flight cap and inbox check are built
  (`Actor::requested`), `decision` has a fourth value `shed`, and both stratum
  columns are nullable. Do not re-derive any of it.
* **A blank line before `Co-Authored-By` destroys the trailer block.** Git
  starts a new one, so `loop:`/`campaign:` vanish and `hj record` reports the
  *previous* campaign's sha as already recorded.
* **`harness/gate` cannot see a flaky test** (`nextest` gives each its own
  process). After touching concurrency, loop `cargo test -p <crate>`.
* **`tracing`'s callsite interest cache is process-wide.** Fix: one global
  subscriber interested in everything (`keep_callsites_enabled`). Two fixes
  that do not work, ten runs each: skipping `set_default` on a `NoSubscriber`,
  and `register_callsite -> Interest::sometimes()`.
* **A plant must compile**, or the run prints no `test result` line — which
  reads like a pass.
* **Nothing left in `core.md#84`, `#two-modes`, `#4-project-file-enumeration`,
  or §2's behaviour.** `[d41389f7fe]` is stale: `driver/tests/file_list.rs`
  384/451/492 cover its three named cases.

## Confirmed — candidates, test on your own evidence

**A test whose fixture is derived from the value under test is not a test.**
Twice this campaign, the same shape — the input computed from the thing
asserted about. An earlier campaign's tripwire, written to fail when `core-025`
landed, drove a handler that *returned* `Stratum::Unimplemented`: it planted
the value it watched for and never fired. Then mine sized a batch as
`INBOX_BACKED_UP - 1`, so lowering the constant shrank the batch and the test
kept passing. Caught only by planting and watching a *different* test fail.

**An "every counter was reached" guard is what catches a column going dead.**
Making failures carry no stratum silently emptied `Row::failed`; the guard in
`the_records_and_the_table_are_the_same_run_counted_twice` fired, not my
reasoning. An equality of zero against zero holds against two artifacts that
share nothing.

**Where a design rule names no threshold, implement the literal reading and let
a test price it.** §10's "no heuristic work while `core` is behind" taken
literally sheds ordinary editor traffic — the drain test failed at a depth of
one, because an editor sends `didOpen` and its request together. That is
evidence; an argument would not have been. It is 4 now (CHANGE-core-034, which
says it is this campaign's number and not the design's).

**An answered decision usually gives the *what* and not the *how*.**
`core-025`'s C says the expiry "carries the strata the handler had" and never
says how they get there. `ProjectView` being per-query is what made it possible.

## Left deliberately

`Row::failed` is now structurally zero — a failure has no `Outcome`, so no
strata. Giving failures a stratum is a seam question. `Answered` has three
public constructors and public fields; a `seam.rs` scan would mechanise "`of`
is the one classification site", but seam.rs was off-limits this round.
