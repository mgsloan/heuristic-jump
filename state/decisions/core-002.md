---
id: core-002
status: accepted
opened: 2026-08-04T06:10:00+00:00
campaign: a9937015-4ddb-46e6-a1aa-f85ab25f09ef
kind: class-b
---

# Does `measure_core` install a log subscriber, or does every `measure_<lang>` main?

## Context

`harness/gate core` was **already red at `ac207d4`**, before this campaign
touched anything, and no work of any loop can be committed until it is green.
Two loops raced:

* `12e0d06` (deps loop, 18:10) added
  `our_log_lines_are_distinguishable_and_the_subscriber_is_installed_once` to
  `crates/driver/tests/seam.rs`, which asserts that no `crates/*` source except
  `heuristic_jump`'s names `tracing_subscriber` and that
  `tracing-subscriber` appears in exactly one manifest. It reads `deps.md` §0's
  table row — "`tracing-subscriber` | heuristic_jump | chosen" — and §9's
  reason: "a library with an opinion about where logs go is one that fights
  whoever links it".
* `087fa45` (core loop, 18:58, campaign `e797a506`) put `install_logging` in
  `measure_core::run`, closing `core.md#two-modes-collect-and-replay`. Its
  argument is `core.md` §7's: "a `measure_<lang>` is four lines, and the
  seventh copy of a log setup is the seventh chance for one binary to be quiet
  where the others are not." Without it every `tracing::info!` in the crate —
  including §7's replay wall clock, which `loops.md` §9 records from the first
  run — went to a facade with no dispatcher.

Each was green alone. Together the rule and the code contradict, and neither
side is obviously wrong: §9's reason for the rule is the *shim's* stderr
interleaving with the forwarded child's in an editor panel, which a `measure`
run has neither of.

It cannot be settled without a trade, so it is not a Class A repair: one option
costs the four-line binary template that `core.md` §7 and
`core.md#adding-a-language` both claim, the other costs the "libraries have no
opinion about logging" rule a whole test was written to hold.

## Options

**A — the subscriber stays in `measure_core`.** `deps.md` §0 gains
`measure_core` in the `tracing-subscriber` row and §9 says the subscriber
belongs to whoever owns the process, `measure_core` being the whole of a
`measure_<lang>`. Costs: a library that installs a global subscriber. The
mitigation is real but narrow — the only crates that link `measure_core` are
the four-line `measure_<lang>` binaries, and `try_init` is used, so a test's
scoped subscriber still wins.

**B — the subscriber moves to each `measure_<lang>` main.** The rule survives
exactly as written for libraries. Costs: `crates/measure_rust/src/measure_rust.rs`
is currently eight lines including the doc comment and is the copy-paste
template for every future language; it grows a ~20-line `install_logging` that
must not drift between languages, which is the failure `core.md` §7 gives as
the reason `clap` lives in `measure_core` at all. `deps.md` §0's row changes
either way, to `measure_rust` rather than `measure_core`.

## Decision

**accepted: Option A — the subscriber stays in measure_core**, answered
2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

§9's stated reason is the shim's stderr interleaving with a forwarded child's
in an editor panel, and a measure run has neither: it is a batch program with
one process and no panel. measure_core is a binary body under a library's name
— only the four-line measure_<lang> binaries link it — so the rule it is
excused from, that a library a handler links must have no opinion about where
logs go, is not the rule being broken. try_init means a test's scoped
subscriber still wins. Option B would put a ~20-line install_logging in every
language's main and require it not to drift, which is the exact failure
core.md §7 gives as the reason clap lives in measure_core at all. Reconciling
the tagged sites in measure_core.rs and deps.md §0/§9 is a core campaign's
work, not this ruling's.

### What is left

The two sites tagged `DECISION-core-002: provisional` — `measure_core.rs` and
`deps.md` §9 — plus §0's table row are a `core` campaign's to reconcile. The
test already asserts this answer.

## Provisional choice in force

**Option A**, because it is the one that changes no code: `087fa45` already
wrote it, and reverting would reopen `core.md#two-modes-collect-and-replay`.
Undoing A later is a file move plus two manifest lines; undoing B later would
mean deleting the same block from every language crate that had been added
meanwhile, so A is also the choice that gets more expensive more slowly.

Tagged sites:

* `design/deps.md` §0's `tracing-subscriber` row and §9's closing paragraph.
* `crates/driver/tests/seam.rs`, the two assertions in
  `our_log_lines_are_distinguishable_and_the_subscriber_is_installed_once` that
  name the permitted crates.
* `crates/measure_core/src/measure_core.rs`, at `install_logging`.

The test is widened rather than deleted: it still holds that `driver` and
`shared` — the crates the shim links — have no opinion about logging, which is
the half of §9's reason that survives either answer. What it no longer holds is
that the permitted crate is exactly one.

## Consequences

If the answer is B, the move is `install_logging` and `stderr_for_logging` out
of `crates/measure_core/src/measure_core.rs` into
`crates/measure_rust/src/measure_rust.rs`, the `tracing-subscriber` dependency
with them, and the tagged sites back to naming one crate — except that the
permitted crate becomes a *set* anyway, since every future `measure_<lang>` is
in it. That is worth noticing before ruling: B does not restore the "exactly
one" the test was written around; it only moves the exception from a library to
a growing list of binaries.

Also worth a human's attention beyond this question: **two loops raced and left
`main` red.** `087fa45`'s gate ran before `12e0d06` landed, so both campaigns
saw green and the tree did not. That is a harness property, not a design one,
and this record is not the place it gets fixed — but nothing else records it.
