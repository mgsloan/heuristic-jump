# Findings — conformance, after de2706af

**Check the code before believing a gap.** The audit predates the last campaign
or two. `measure_core`, `measure_rust`, `lang_rust`, `shared::identifier_at`,
`shared::proto`'s client half, `Deadline::none`, `driver::run` and a
`heuristic_jump` that builds a `Registry` all exist. Any gap whose `found:` says
"does not exist on disk" for one of those is a re-judge, not work.

**Target selection, in order.** (1) The stale ones above. (2) The `write` list
in `state/phase.toml`. (3) Gaps per *section* — the number moves per section, so
a section with one gap left beats a gap in a section with three.

**What is left, best first.**

* `#7-observability[c4505d900b]` — the handler-reported half of the record.
  `Outcome` carries `locations`, `confidence` and *one* `Stratum`; §7 needs
  `margin`, `considered`, `stages`, `stage_us`, `bytes_scanned`, `files_parsed`,
  and `stratum_prior` distinct from `stratum_final`. Class B on the frozen seam,
  so a decision record first. `record::HandlerReport` is where it lands and the
  record's shape is already §7's, so this changes values, not columns.
* `#the-trait[93f2f340e6]` — `ProjectView` has no `candidates`/`parse`/`scan`.
  Also seam, also Class B. Ask first whether it belongs on a non-seam neighbour
  (`FileList::paths`, not `ProjectView::candidates`).
* `#86-modelling-errors-must-fail-closed`, `#4-project-file-enumeration`,
  `#text-and-tree-can-never-disagree`, `#both-sides-are-sets`,
  `#10-testing[ddadbddae0]` — all wait on `driver`'s document map, channels and
  transport. `driver::run` is now the entry point they hang off; its body is a
  config report plus `Ok(())`, an honest stub the run-loop campaign replaces
  without moving the language list. One of them alone is a campaign.
* The rope public-API newtype sweep (`#vendoring[d7bbef9371]`) is real, is its
  own campaign, and is never a step inside another.

**Blocked, not open: `conformance-011`.** §9 says `similarity` depends on
`shared`; its manifest does not, and `crates/similarity/**` is denied to every
loop. Deleting the edge from §9 would be a spec edit toward the code on a
layering claim, so *neither* option is in force and the record is the tag.
`#the-dependency-graph` cannot go past this without a human.

**Rules that keep paying off.**

* A claim stays clean only with a test that fails at *compile* time, or reads
  the source, or reads a manifest. `driver/tests/seam.rs` is the pattern and now
  carries six such assertions.
* Manifest assertions are **subsets, not equalities**: `deps.md` §14 has each
  dependency arrive with its first user, so a spec-listed crate not yet declared
  is the intended state. An equality gets "fixed" by adding an unused crate.
* In `shared::proto`, a new direction of travel is a new type, never a derive.
* `vec![Arc::new(handler)]` does not coerce to `Vec<Arc<dyn LanguageHandler>>`
  from the parameter type — write the `as Arc<dyn …>`.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* `fn`s in
`tests/*.rs` — a file-level `#![expect(..., reason = "…")]` is the way through.
`unreachable_pub` means `pub(crate)` from the start. `integer_division` fires on
`/`; `div_ceil` passes. `serde_json::Value` is a disallowed type. `Instant::now`,
`read_dir`, `Command::output` and `io::stdout` are disallowed methods.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention, not a
campaign. `#9-workspace-layout` can never go clean: `lang_python` and
`lang_typescript` are outside every owned path by design.

**Gate.** It inspects untracked and unstaged paths, so a concurrent human edit
to `harness/**` makes the no-argument form un-greenable. Commit only your own
paths, then `harness/gate conformance --rev <sha>`.
