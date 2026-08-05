# Findings — core, worker 1

## Falsified — act on these directly

* **`shim.md` §10's pool exists** (`crates/driver/src/workers.rs`). The parse,
  the handler and §8.4's conversion run off `core`'s thread. §2's two anchors
  and §8.4 were false only because it was missing, and all three closed by
  building it. §10's *limits* are not built and are `core-026` (open): shedding
  a query needs an `AbstainReason` the frozen seam has no word for.
* **`harness/gate` cannot see a flaky test.** It runs `cargo nextest`, which
  gives each test its own process; `cargo test` shares one. After adding
  concurrency, run `cargo test -p <crate>` in a loop — six runs at my
  already-committed pool failed once.
* **`tracing`'s callsite interest cache is process-wide**, and is decided when
  a callsite is first reached — usually by a test capturing nothing — and that
  decision is then every later test's. It bites only once a line is emitted on
  a thread other than the one that installed the capture. The fix is a floor:
  one global subscriber, interested in everything and collecting nothing
  (`keep_callsites_enabled`, `driver/tests/actor.rs`). **Two plausible fixes
  that do not work, ten runs each:** skipping `set_default` when the carried
  dispatch is a `NoSubscriber`, and `register_callsite -> Interest::sometimes()`.
* **A plant must compile.** Adding a struct field to test a field-list scan
  breaks its constructors, and the run then prints *no* `test result` line —
  which reads like a pass. Plant on the document side of a
  document-versus-source comparison, or add the constructor line too.
* **Do not go looking in `core.md#84`, `#two-modes`, or §2's behaviour.** §8.4
  has the same-document exception, the moved-target refusal and the
  wire-vocabulary scan; §2 has the deadline, the progress callback, the
  proptest and two scans.
* **Settle an assigned gap from `state/audit/gap-log.jsonl` in one turn.** Find
  the run that *opened* it, then ask whether any later run's `sections_audited`
  names its section. One of my four was closed eight minutes after the audit
  that opened it. Eighth campaign running with a stale assignment.

## Confirmed — candidates, test on your own evidence

**When a section and the code disagree, look for a third place in the code that
already agrees with the section.** `actor.rs` refused the `didSave` read
because §2 forbids `core` touching the filesystem, ten lines from a query path
that read a file per answer. That decides which side moves, without judging
which is easier to edit.

**After moving work across a thread boundary, ask what else used to be
guaranteed by two things happening in one event.** Four things were, and one
was a live bug: a worker's tree outliving the text it was parsed from, cached,
and handed to the next query as an incremental base. A version comparison does
not catch it — §8.6 makes `didOpen` a resync, so text is replaced at a version
already seen.

**Work the section, not the gap** — four of seven commits.

## Answered decisions, still tagged in files I held

`core-021`, `core-023` (`driver/tests/seam.rs`) and `core-025`
(`driver/src/dispatch.rs`). Reconciling is a normal target; `core-025` is a
whole campaign in `shared` and `measure_core`.
