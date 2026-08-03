# Findings — conformance, after ff3e1a40

**Check the code before believing a gap.** Four campaigns running. The audit
you are given predates the last campaign or two, and `measure_core`,
`measure_rust`, `lang_rust`, `shared::identifier`, `shared::proto`'s client
half and `Deadline::none` all now exist. Anything in the gap list whose
`found:` says "does not exist on disk" for one of those is a re-judge, not
work.

**Target selection, in order.** (1) The stale ones above. (2) The `write` list
in `state/phase.toml`. (3) Gaps per *section* — the number moves per section,
so a section with one gap left beats a gap in a section with three.

**What is actually left, best first.**

* `#the-dependency-graph` — three gaps, all cheap now. `heuristic_jump::main`
  is `fn main() {}` and §9 prints the registry plus `driver::run(registry,
  Cli::parse())` it should be; `driver` exports no `run`. Closing that also
  closes `#adding-a-language`'s "one line in `heuristic_jump`", which is the
  only thing left in that section. The third gap is `tracing` missing from
  §9's self-described *authoritative* `shared` dependency list, which
  `shared`, `driver` and `heuristic_jump` all declare — a one-line Class A fix
  plus a manifest test, and the journal already treats that list as binding.
* `#7-observability[c4505d900b]` — the handler-reported half of the record.
  `Outcome` carries `locations`, `confidence` and *one* `Stratum`; §7 needs
  `margin`, `considered`, `stages`, `stage_us`, `bytes_scanned`,
  `files_parsed`, and `stratum_prior` distinct from `stratum_final`. Class B on
  the frozen seam, so it is a decision record first. `record::HandlerReport` is
  where it lands, and the record's shape is already §7's, so this changes
  values rather than columns.
* `#the-trait[93f2f340e6]` — `ProjectView` has no `candidates`/`parse`/`scan`.
  Also seam, also Class B.
* `#86-modelling-errors-must-fail-closed`, `#4-project-file-enumeration`,
  `#text-and-tree-can-never-disagree` — all need `driver`'s document map and
  channels, which do not exist. Bigger than they look.

**Rules that keep paying off.**

* A claim that stays clean has a test that fails at *compile* time, or reads
  the source or a manifest. `driver/tests/seam.rs` is the pattern: it asserts
  about `shared` because `driver` may not name `rope`, and now about the
  measurement crates' `[dependencies]` because an extra edge only shows up as
  a slow build.
* In `shared::proto`, **a new direction of travel is a new type, never a new
  derive.** `read_projections_are_never_serialized` enforces it and the
  inventory lists make it a decision somebody writes down.
* When a seam type blocks you, ask whether you need the *type* or a projection
  (`DefinitionSite`), or whether the thing belongs on a non-seam neighbour
  (`FileList::paths`, not `ProjectView::candidates`).

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* `fn`s in
`tests/*.rs` — a file-level `#![expect(..., reason = "...")]` is the way
through. `unreachable_pub` means writing `pub(crate)` from the start in a crate
with private modules. `integer_division` fires on `/`; `div_ceil` passes.
`serde_json::Value` is a disallowed type. `Instant::now`, `read_dir`,
`Command::output` and `io::stdout` are disallowed methods — `measure_core`
carries a documented `#[expect]` for the last three, since §7's table gives
`measure` no deadline and no wire on stdout.

**Ruled out, with evidence.** The rope public-API newtype sweep is its own
campaign, never a step inside another. §8.5's golden corpus is captured
editor/server traffic, closer to an intervention. `#9-workspace-layout` can
never go clean here — `lang_python`/`lang_typescript` are outside every owned
path by design.

**Gate.** It inspects untracked and unstaged paths, so a concurrent human edit
to `harness/**` makes the no-argument form un-greenable. Commit only your own
paths, then `harness/gate conformance --rev <sha>`.
