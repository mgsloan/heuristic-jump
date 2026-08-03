# Findings — conformance, after 7a30ee1a

**The audit lags; check the code before believing a gap.** `measure_core`,
`measure_rust`, `lang_rust`, `shared::identifier_at`, `Deadline::none`,
`driver::run`, `ProjectView::{candidates,parse,scan}`, `driver::TreeCache`,
`measure_core::QueryRecord` and `replay::write_records` all exist. A `found:`
reading "does not exist" for one of those is a re-judge, not work — this
campaign spent a third of its budget establishing that for `#7[10d2239070]`.

**Five campaigns running, the stated blocker was not one.** §the-trait was
"blocked" on a deleted decision; §8.4 on nothing; §2 on a `realise` that merely
had no caller; §7 on a Class B escalation that cost one decision record and one
commit. Ask what remains after removing the named blocker: usually ordinary
code.

**Handler doubles are available** in `driver`, `shared` and `measure_core` —
`seam.rs`'s grammar ban reads `[dependencies]` only, though a `lang_*` edge
stays banned in every table.

**Target selection.** Stale gaps, then `state/phase.toml`'s `write` list, then
fewest gaps left — the number moves per section.

**What is left, best first.**

* The driver cluster — `#86-modelling-errors`, `#4-project-file-enumeration`
  (3 gaps), `#both-sides-are-sets`, `#10-testing[ddadbddae0]`,
  `#the-oracle[8e6807da19]` (`ServerProfile` has no producer). All wait on
  `driver`'s document map, channels and transport, hanging off `driver::run`
  (a config-report stub); `TreeCache` is the first piece of that owner and has
  no owner itself. One gap alone is a campaign.
* `#85[081351da0e]` — negative-parse assertions per union. Small, and does not
  clean the section: its sibling needs captured traffic, an intervention.
* The rope newtype sweep (`#vendoring[148fd8d277]`) is its own campaign and
  never a step inside another.

**Blocked: `conformance-011`.** §9 gives `similarity` an edge to `shared`; its
manifest lacks it and the crate is denied to every loop.

**Rules that pay off.**

* A claim stays clean only with a test that fails at *compile* time, or reads
  source or a manifest. Prove it fails before keeping it.
* When the claim is about *which path ran* rather than about the answer, add a
  small enum (`ParseKind`, `Refinement`) to make the branch observable;
  asserting on the result proves nothing.
* **Widening a seam type trips `result_large_err` at 128 bytes**, and it fires
  at `Result<_, Dispatched>` signatures the diff never touched. Box *inside*
  the new type, never at the use site. `Dispatched` is near the line again.
* Bulk edits through `python3` skip the formatting hook — `cargo fmt -p
  <crate>` right after, or the gate fails at step 1 before compiling anything.
* `measure_core`'s table cannot be asserted on in-process: `Table` is
  `pub(crate)` and `report` writes past cargo's test capture. Making `replay`
  return its table is a real target, and the coverage-on-prior /
  precision-on-settled split currently rests on inspection.
* Picking the wrong `Error` sub-enum compiles and passes. The arm decides what
  §7's record says, so choose it beside `dispatch::classify`.
* Manifest assertions are subsets, not equalities (`deps.md` §14).
* Fixtures are real directories under `env!("CARGO_TARGET_TMPDIR")` with an
  empty `.git/`, or `ignore` skips `.gitignore` entirely.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* fns and trait
impls in `tests/*.rs`; a file-level `#![expect(…, reason = "…")]` is the way
through — list **only** the lints the file trips, because
`unfulfilled_lint_expectations` is fatal under `-D warnings`. Also
`redundant_clone`, `unreachable_pub`, `cast_possible_truncation` (no `as u32`),
`integer_division`. Disallowed: `serde_json::Value`, `Instant::now`,
`read_dir`, `Command::output`, `io::stdout`. `Rope::len()` still returns
`usize`, not `ByteLen`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean: `lang_python`/`lang_typescript` are
outside every owned path by design.

**Gate.** It inspects unstaged and untracked paths too, so a human edit under
`harness/**` makes the no-argument form un-greenable: commit your own paths,
then `harness/gate conformance --rev <sha>`.
