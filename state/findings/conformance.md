# Findings — conformance, after 576c2c6f

**The audit lags badly; verify a gap before working it.** Directly checked this
session and already satisfied: `#vocabulary-types[fbe658c158]` (all seven
re-exported, `shared.rs:47`), `#the-trait[93f2f340e6]`
(`ProjectView::{candidates,parse,scan}`), `#two-modes[90c8d7bd21]`
(`Deadline::none`), `#the-dependency-graph[7f643e614e,7f3a1bb4ec]` (core.md
lists `tracing`; `seam.rs` asserts the set), `#adding-a-language[0858868078]`
and `#what-the-templates-handler-does` (both, `lang_rust::Handler` +
`heuristic_jump`'s `Registry`), plus every gap saying `measure_core`/
`measure_rust`/`lang_rust` "do not exist". Re-judging is not work.

**Genuinely open, verified:** `#86-modelling-errors` (no trust state anywhere),
`#both-sides-are-sets`, `#10-testing[ddadbddae0]` (no codec),
`#the-oracle[8e6807da19]` (`ServerId::new` has zero callers; every
`ServerProfile` is `id: None`) — all four wait on `driver`'s actor, off
`driver::run`, still a config report. One gap alone is a campaign.
`#85[081351da0e]` (negative-parse assertions — `proto.rs` has none) is small
but cannot clean its section. `#vendoring[148fd8d277]` is its own campaign.

**Seven campaigns running, the stated blocker was not one.** A `found:` naming
a missing subsystem is the *default* false blocker: ask what remains after
removing it. Usually ordinary code.

**When the fix is a deletion, the campaign is the licence, not the edit.**
576c2c6f deleted a `Vec<u64>`; the work was proving it cost nothing. Three
questions, all of which must pass or it is a Class B escalation: does another
section already own the thing being removed; does `grep harness/` show anyone
consuming it; and is the doc that seems to contradict you talking about a
different artifact? Two design claims that look contradictory usually are not —
§7 makes the *record* non-reproducible and the *table* byte-identical, and both
hold at once.

**Making a claim mechanical is the whole job.** Strongest first: (1) remove
what the claim forbids — `Table` holds no `Duration`, so `render` cannot vary;
(2) an exhaustive match; (3) a test, *mutation-checked before committing*. For
"which path ran", add a small enum (`ParseKind`, `Refinement`, `Refresh`).
Assert an artifact is non-empty before comparing two — empty strings are equal.

**Case-splitting on a `#[non_exhaustive]` seam enum wants a method on it, not
a `match`** (`AbstainReason::file_list_evidence`).

**Traps.**

* `measure_core::run` writes to a raw `stdout()` handle cargo does not capture.
  `replay_table` now returns the table; anything else asserting on printed
  output needs the same treatment.
* Widening a seam type trips `result_large_err` at 128 bytes, at `Result<_,
  Dispatched>` signatures the diff never touched. Box *inside* the new type.
* `python3` bulk edits skip the format hook — `cargo fmt -p <crate>` after.
* `FileList::enumerate` never returns `Err`. No test on its error path.
* Time must *move* → `DrivenClock` (`driver/tests/file_list.rs`).
* Wrong `Error` sub-enum compiles and passes. `HandlerError` has one variant.
* Manifest assertions are subsets (`deps.md` §14). Fixtures are real dirs under
  `CARGO_TARGET_TMPDIR` with a real `.git/`.

**Clippy.** `unwrap`/`expect`/`panic` denied in *free* fns in `tests/*.rs`;
file-level `#![expect(…, reason)]` listing **only** lints the file trips
(`unfulfilled_lint_expectations` is fatal). Also `redundant_clone`,
`unreachable_pub`, `cast_possible_truncation`, `integer_division`. Disallowed:
`serde_json::Value`, `Instant::now`, `read_dir`, `Command::output`,
`io::stdout`, `thread::spawn`, `crossbeam_channel::unbounded`, blocking `recv`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean (`lang_python`/`lang_typescript` are
outside every owned path). `conformance-011`: `similarity` is denied.

**Gate.** It inspects unstaged and untracked paths, so a human edit under
`harness/**` un-greens the no-argument form: commit yours, then
`harness/gate conformance --rev <sha>`.
