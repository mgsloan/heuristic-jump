# Findings — conformance, after 571b1bb0

**The audit lags; check the code before believing a gap.** `measure_core`,
`measure_rust`, `lang_rust`, `shared::identifier_at`, `Deadline::none`,
`driver::run`, `ProjectView::{candidates,parse,scan}`, `driver::TreeCache`,
`measure_core::QueryRecord`, `replay::write_records` and now
`driver::FileListCache` all exist. A `found:` reading "does not exist" for one
of those is a re-judge, not work.

**Six campaigns running, the stated blocker was not one.** §the-trait, §8.4,
§2, §7 and now §4 were each "blocked" on a subsystem that turned out to be
irrelevant. Treat a `found:` naming a missing subsystem as the *default* false
blocker: ask what remains after removing it. Usually ordinary code.

**Making a claim mechanical is the whole job.** Three shapes, strongest first:
(1) remove the thing the claim forbids — `watched_files_changed()` takes no
payload, so "never reads the payload" cannot regress; (2) an exhaustive match,
so a new variant fails to compile; (3) a test, *mutation-checked before
committing* — break the code deliberately and watch it fail, or it proves
nothing. When the claim is about *which path ran*, add a small enum
(`ParseKind`, `Refinement`, `Refresh`) to make the branch observable.

**A consumer that wants to case-split on a `#[non_exhaustive]` seam enum is
asking for a method on it, not a `match`** — the wildcard arm `CLAUDE.md` bans
is the only alternative, and it silently misclassifies the next variant.
`AbstainReason::file_list_evidence` is the worked example.

**Handler doubles are available** in `driver`, `shared` and `measure_core` —
`seam.rs`'s grammar ban reads `[dependencies]` only, though a `lang_*` edge
stays banned in every table.

**Target selection.** Stale gaps, then `state/phase.toml`'s `write` list, then
fewest gaps left — the number moves per section.

**What is left, best first.**

* The driver cluster — `#86-modelling-errors`, `#both-sides-are-sets`,
  `#10-testing[ddadbddae0]`, `#the-oracle[8e6807da19]`. All wait on `driver`'s
  document map, channels and transport, hanging off `driver::run` (still a
  config report). `TreeCache` and `FileListCache` are two owned pieces of that
  actor with no actor; a third is not obviously worth more than starting the
  loop. One gap alone is a campaign.
* `#85[081351da0e]` — negative-parse assertions per union. Small, and does not
  clean the section: its sibling needs captured traffic, an intervention.
* The rope newtype sweep (`#vendoring[148fd8d277]`) is its own campaign and
  never a step inside another.

**Blocked: `conformance-011`.** §9 gives `similarity` an edge to `shared`; its
manifest lacks it and the crate is denied to every loop.

**Traps that cost real time.**

* **Widening a seam type trips `result_large_err` at 128 bytes**, firing at
  `Result<_, Dispatched>` signatures the diff never touched. Box *inside* the
  new type. `Dispatched` is near the line again.
* Bulk edits through `python3` skip the formatting hook — `cargo fmt -p
  <crate>` right after, or the gate fails at step 1.
* `FileList::enumerate` never returns `Err`: unreadable entries are skipped and
  a missing root walks to an empty list. Do not build a test on its error path.
* A test that needs time to *move* wants `DrivenClock`
  (`driver/tests/file_list.rs`): a base `Instant` plus an `AtomicU64`, not a
  cell and not a lock. `FrozenClock` cannot advance.
* `measure_core`'s table cannot be asserted on in-process: `Table` is
  `pub(crate)` and `report` writes past cargo's test capture. Making `replay`
  return its table is a real target.
* Picking the wrong `Error` sub-enum compiles and passes. Choose it beside
  `dispatch::classify`.
* Manifest assertions are subsets, not equalities (`deps.md` §14).
* Fixtures are real directories under `env!("CARGO_TARGET_TMPDIR")` with an
  empty `.git/`, or `ignore` skips `.gitignore` entirely.

**Clippy.** `unwrap`/`expect`/`panic` denied in *free* fns in `tests/*.rs`;
file-level `#![expect(…, reason = "…")]` listing **only** the lints the file
trips (`unfulfilled_lint_expectations` is fatal). Also `redundant_clone`,
`unreachable_pub`, `cast_possible_truncation`, `integer_division`. Disallowed:
`serde_json::Value`, `Instant::now`, `read_dir`, `Command::output`,
`io::stdout`, `thread::spawn` (use `Builder::new().name(…)`),
`crossbeam_channel::unbounded` and blocking `Receiver::recv`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean: `lang_python`/`lang_typescript` are
outside every owned path by design.

**Gate.** It inspects unstaged and untracked paths too, so a human edit under
`harness/**` makes the no-argument form un-greenable: commit your own paths,
then `harness/gate conformance --rev <sha>`.
