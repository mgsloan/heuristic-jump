# Findings — core, worker 1

## Falsified — act on these directly

* **Settle a stale assigned gap in one turn — then ask what already *holds*
  it.** Find the `gap-log.jsonl` run that opened the id; ask whether a later
  run's `sections_audited` names its section. Tenth campaign running. The
  second question is the more useful one:
  `tests/actor.rs::the_parse_and_the_conversion_never_run_on_the_thread_that_owns_the_state`
  and `seam.rs::only_the_pool_realises_a_seed_or_calls_the_dispatch_wrapper`
  hold §2 and §8.4 from both sides, so those sections need nothing at all.
* **`try_recv().is_err()` does not mean "nothing was dispatched".** A query
  that *was* dispatched takes a worker a parse and a handler call, so an empty
  channel just after `handle` returns is what a *working* dispatch looks like.
  Use `recv_timeout(QUIET)` — `file_list.rs`'s shape. My first such assertion
  survived a plant: a racy assertion that happens to win reads exactly like a
  sound one until you plant a second time.
* **`clippy.toml` denies `std::fs::read_dir`.** A scan that wants a crate's
  modules reads the library root and follows its `mod` lines — the better
  question anyway, since what compiles is what the root names.
* **A blank line before `Co-Authored-By` destroys the trailer block.** Git
  starts a new one, so `loop:`/`campaign:` vanish and `hj record` reports the
  previous campaign's sha.
* **`harness/gate` cannot see a flaky test** (`nextest` gives each its own
  process). After touching concurrency, loop `cargo test -p <crate>`.
* **`tracing`'s callsite interest cache is process-wide.** Fix: one global
  subscriber interested in everything. Two that do not work: skipping
  `set_default` on a `NoSubscriber`, and `Interest::sometimes()`.
* **A plant must compile**, or the run prints no `test result` line — which
  reads like a pass. Mine failed clippy this time.
* **Nothing left in §8.6, §8.4, §2, `#two-modes`, `#4-project-file-enumeration`.**
  Every decision is `accepted` and no `DECISION-` tag remains in `crates/`.

## Confirmed — candidates, test on your own evidence

**When you need a guard, grep the file you are already in for one that argues
the same thing.** §8.6's checksum needed "has this document moved since the
save", and `TreeFate` — four screens above the new code — already argues that a
`DocumentVersion` cannot answer it, because `didOpen` is a resync that replaces
the text at a version we have seen. One read settled the design.

**Where a design rule names no threshold, implement the literal reading and let
a test price it.** §10's "no heuristic work while `core` is behind" taken
literally sheds ordinary editor traffic.

**A lint's remedy can be backwards.** `large_enum_variant` on `Job`/`Finished`
wants a `Box` on the *common* variant — an allocation on every query so the
rare checksum does not sit in a query-sized slot. `#[expect]` with the reason.

## The best target left in these files

**§2's "the parse is usually incremental from a cached base" is false here**,
and it is the load-bearing half of the case for parsing eagerly.
`Actor::notified` forgets the tree on every `didChange`, because
`Documents::changed` returns no `InputEdit`s — so `Actor::edits` is permanently
empty and every parse after a change is full. The code says so; §2 does not.
**Do not close it by editing §2.** It is `documents.rs`/`trees.rs`/`actor.rs`
work and wants a benchmark first (`CLAUDE.md`).
