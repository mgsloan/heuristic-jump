# Findings — conformance, after 0faab934

**Check the code before believing a gap; check the decision records too.** The
audit lags a campaign or two: `measure_core`, `measure_rust`, `lang_rust`,
`shared::identifier_at`, `Deadline::none`, `driver::run`, a `Registry`-building
`heuristic_jump` and now `ProjectView::{candidates, parse, scan}` all exist, so
a `found:` reading "does not exist" for one of those is a re-judge. Stronger,
from this campaign: **a gap naming an absent subsystem as its blocker may be a
fossil of a doc comment.** `#the-trait` was blocked on "the parse LRU and the
bounded pool", both of which `conformance-005`'s ruling had already deleted —
`CLAUDE.md` line 112 forbids caching, indexing and optimisation until a corpus
and a benchmark say otherwise. Ask what is left after removing them. Here it
was ordinary code.

**Target selection.** Stale gaps, then `state/phase.toml`'s `write` list, then
whichever section has fewest gaps left — the number moves per section.

**What is left, best first.**

* `#7-observability[c4505d900b]` — the handler-reported half of the record.
  `Outcome` carries `locations`, `confidence` and *one* `Stratum`; §7 needs
  `margin`, `considered`, `stages`, `stage_us`, `bytes_scanned`,
  `files_parsed`, and `stratum_prior` distinct from `stratum_final`.
  `ScanOutcome` now supplies two of those counters. Class B on the frozen
  seam: decision record first. Lands in `record::HandlerReport`, so it changes
  values, not columns.
* The driver cluster — `#86-modelling-errors-must-fail-closed`,
  `#4-project-file-enumeration` (3 gaps), `#text-and-tree-can-never-disagree`,
  `#both-sides-are-sets`, `#10-testing[ddadbddae0]`. All wait on `driver`'s
  document map, channels and transport, which hang off `driver::run` (today an
  honest config-report stub). One of them alone is a campaign.
* The rope newtype sweep (`#vendoring[d7bbef9371]`) is its own campaign and
  never a step inside another.

**Blocked: `conformance-011`.** §9 says `similarity` depends on `shared`; its
manifest does not, and `crates/similarity/**` is denied to every loop, so
neither option is in force. `#the-dependency-graph` needs a human.

**Rules that pay off.**

* A claim stays clean only with a test that fails at *compile* time, or reads
  the source or a manifest. `driver/tests/seam.rs` is the pattern.
* Manifest assertions are **subsets, not equalities** — `deps.md` §14 has each
  dependency arrive with its first user. Missing `rayon` is not a gap.
* Fixtures are real directories under `env!("CARGO_TARGET_TMPDIR")`. `ignore`
  applies `.gitignore` only inside a repository, so a fixture needs an empty
  `.git/` or the exclusion silently does not apply.
* `Arc::new(x)` does not coerce from the parameter type: write `as Arc<dyn …>`.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in *free* `fn`s in
`tests/*.rs`; a file-level `#![expect(…, reason = "…")]` is the way through.
`redundant_clone` is `warn` in the table but the gate builds `-D warnings`, so
it is fatal there. `unreachable_pub` means `pub(crate)` from the start.
`cast_possible_truncation` bans `as u32`. `integer_division` fires on `/`.
`serde_json::Value` is a disallowed type; so are `Instant::now`, `read_dir`,
`Command::output`, `io::stdout`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention, not
a campaign. `#9-workspace-layout` can never go clean: `lang_python` and
`lang_typescript` are outside every owned path by design.

**Gate.** It inspects untracked and unstaged paths, so a concurrent human edit
to `harness/**` makes the no-argument form un-greenable: commit your own paths,
then `harness/gate conformance --rev <sha>`.
