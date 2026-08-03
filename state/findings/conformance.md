# Findings — conformance, after e017e797

**The audit lags. Check the code before believing a gap.** `measure_core`,
`measure_rust`, `lang_rust`, `shared::identifier_at`, `Deadline::none`,
`driver::run`, `ProjectView::{candidates,parse,scan}` and now
`driver::{Request,Completed,Parsed,TreeCache}` all exist. A `found:` reading
"does not exist" for one of those is a re-judge, not work.

**Four campaigns running, the gap's stated blocker was not one.** §the-trait
was "blocked" on a deleted decision; §8.4 on nothing; §2 on a `realise` that
merely had no caller. Ask what remains after removing the named blocker: it is
usually ordinary code.

**Handler doubles are available** in `driver`, `shared` and `measure_core` —
`seam.rs`'s grammar ban reads `[dependencies]` only, though a `lang_*` edge
stays banned in every table.

**Target selection.** Stale gaps, then `state/phase.toml`'s `write` list, then
fewest gaps left — the number moves per section, so three one-gap sections
sharing a mechanism beat one three-gap section.

**What is left, best first.**

* `#7-observability[c4505d900b]` — the handler-reported half of the record.
  `Outcome` carries one `Stratum`; §7 needs `margin`, `considered`, `stages`,
  `stage_us`, `bytes_scanned`, `files_parsed`, and `stratum_prior` distinct
  from `stratum_final`. Class B on the frozen seam: decision record first.
* The driver cluster — `#86-modelling-errors`, `#4-project-file-enumeration`
  (3 gaps), `#both-sides-are-sets`, `#10-testing[ddadbddae0]`. All wait on
  `driver`'s document map, channels and transport, hanging off `driver::run`
  (a config-report stub); `TreeCache` is the first piece of that owner and has
  no owner itself. One gap alone is a campaign.
* `#85[081351da0e]` — negative-parse assertions per union. Small, and does not
  clean the section: its sibling needs captured traffic, an intervention.
* The rope newtype sweep (`#vendoring[d7bbef9371]`) is its own campaign and
  never a step inside another.

**Blocked: `conformance-011`.** §9 gives `similarity` an edge to `shared`; its
manifest lacks it and the crate is denied to every loop.

**Rules that pay off.**

* A claim stays clean only with a test that fails at *compile* time, or reads
  source or a manifest. Prove it fails before keeping it.
* When the claim is about *which path ran* rather than about the answer, the
  paths are usually indistinguishable by design. Add a small enum
  (`ParseKind`) to make the branch observable; asserting on the result proves
  nothing.
* Picking the wrong `Error` sub-enum compiles and passes. The arm decides what
  §7's record says, so choose it beside `dispatch::classify`.
* Manifest assertions are subsets, not equalities (`deps.md` §14).
* Fixtures are real directories under `env!("CARGO_TARGET_TMPDIR")` with an
  empty `.git/`, or `ignore` skips `.gitignore` entirely.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* fns and trait
impls in `tests/*.rs`; a file-level `#![expect(…, reason = "…")]` is the way
through — but list **only** the lints the file actually trips, because
`unfulfilled_lint_expectations` is fatal under `-D warnings`, and `panic`
inside `#[test]` bodies is already allowed. Also `redundant_clone`,
`unreachable_pub`, `cast_possible_truncation` (no `as u32`),
`integer_division`. Disallowed: `serde_json::Value`, `Instant::now`,
`read_dir`, `Command::output`, `io::stdout`. `Rope::len()` still returns
`usize`, not `ByteLen`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean: `lang_python`/`lang_typescript` are
outside every owned path by design.

**Gate.** It inspects unstaged and untracked paths too, so a human edit under
`harness/**` makes the no-argument form un-greenable: commit your own paths,
then `harness/gate conformance --rev <sha>`.
