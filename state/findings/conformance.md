# Findings — conformance, after b62bf25e

**The audit lags. Check the code, the decision records, and the changelog
before believing a gap.** `measure_core`, `measure_rust`, `lang_rust`,
`shared::identifier_at`, `Deadline::none`, `driver::run`, a `Registry`-building
`heuristic_jump` and `ProjectView::{candidates,parse,scan}` all exist; a
`found:` reading "does not exist" for one of those is a re-judge, not work.

**Three campaigns running, the gap's stated blocker was not one.** §the-trait
was "blocked" on a parse LRU and a bounded pool that `conformance-005` had
already deleted; §8.4 was blocked on nothing at all — every piece existed and
`dispatch` merely had no encoding parameter. Ask what is left after removing
the named blocker. It is usually ordinary code.

**Handler doubles are available now.** `deadline.rs` claimed for four
campaigns that phase 1a has none, because `impl LanguageHandler` must name
`tree_sitter::Language`. False: `seam.rs`'s grammar ban reads
`[dependencies]` only, and `shared`/`measure_core`/`driver` all dev-depend on
`tree-sitter-rust`. A `lang_*` edge stays banned in *every* table. Any gap
justified by "nothing can build a `Query`" is stale.

**Target selection.** Stale gaps, then `state/phase.toml`'s `write` list, then
the section with fewest gaps left — the number moves per section.

**What is left, best first.**

* `#7-observability[c4505d900b]` — the handler-reported half of the record.
  `Outcome` carries one `Stratum`; §7 needs `margin`, `considered`, `stages`,
  `stage_us`, `bytes_scanned`, `files_parsed`, and `stratum_prior` distinct
  from `stratum_final`. Class B on the frozen seam: decision record first.
  Lands in `record::HandlerReport`, so it changes values, not columns.
* The driver cluster — `#86-modelling-errors`, `#4-project-file-enumeration`
  (3 gaps), `#text-and-tree-can-never-disagree`, `#both-sides-are-sets`,
  `#10-testing[ddadbddae0]`. All wait on `driver`'s document map, channels and
  transport, hanging off `driver::run` (an honest config-report stub). One
  alone is a campaign.
* `#85[081351da0e]` — negative-parse assertions per union. Small, and does not
  clean the section: its sibling needs captured traffic, an intervention.
* The rope newtype sweep (`#vendoring[d7bbef9371]`) is its own campaign and
  never a step inside another.

**Blocked: `conformance-011`.** §9 gives `similarity` an edge to `shared`; its
manifest lacks it and `crates/similarity/**` is denied to every loop.

**Rules that pay off.**

* A claim stays clean only with a test that fails at *compile* time, or reads
  source or a manifest. `driver/tests/seam.rs` is the pattern — and prove the
  scan fails before keeping it.
* Manifest assertions are **subsets, not equalities** (`deps.md` §14).
* Fixtures are real directories under `env!("CARGO_TARGET_TMPDIR")` with an
  empty `.git/`, or `ignore` skips `.gitignore` entirely.
* `Arc::new(x)` does not coerce: write `as Arc<dyn …>`.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* fns and trait
impls in `tests/*.rs`; a file-level `#![expect(…, reason = "…")]` is the way
through. `redundant_clone` is `warn` in the table but fatal under the gate's
`-D warnings`. `unreachable_pub`, `cast_possible_truncation` (no `as u32`),
`integer_division`. Disallowed: `serde_json::Value`, `Instant::now`,
`read_dir`, `Command::output`, `io::stdout`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean: `lang_python`/`lang_typescript` are
outside every owned path by design.

**Gate.** It inspects untracked and unstaged paths, so a concurrent human edit
to `harness/**` makes the no-argument form un-greenable: commit your own paths,
then `harness/gate conformance --rev <sha>`.
