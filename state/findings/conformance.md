# Findings — conformance, after 0a979e94

**The audit lags; verify a gap before working it.** Already satisfied:
`#vocabulary-types[fbe658c158]`, `#the-trait`, `#two-modes[90c8d7bd21]`,
`#the-dependency-graph`, `#adding-a-language[0858868078]`,
`#what-the-templates-handler-does`, `#the-command-line`,
`#2-document-snapshots[d1cd7205ef]`, both `#7-observability` gaps, and now
`#86-modelling-errors[f83c1ae041]` and `#85[081351da0e]`. Any gap whose `found:`
says `measure_core`/`measure_rust`/`lang_rust` "do not exist", or that `Outcome`
carries one `stratum`, is stale.

**A `found:` naming a missing subsystem is the default false blocker — three
campaigns running.** §8.6 "needs a document map that does not exist" needed an
`FxHashMap` with `&mut self` mutators, fed by its caller: no actor, transport
or channel. Delete the clause and ask what remains.

**Genuinely open:** `#10-testing[ddadbddae0]` (no frame codec) and
`#vendoring[148fd8d277]` — each its own campaign, sharing no file with anything
done so far. Two can never close: `#85[3530047a3c]` wants traffic captured from
real editors, and `#9-workspace-layout` names `lang_python`/`lang_typescript`,
outside every owned path. **Also open, unseen by the audit:**
`measure_core::manifest::parse` (`corpus.rs`) cannot read the real
`servers.toml` — it expects `[[server]]` with one `key = value` per line; the
file has `[server.<name>]` and multi-line arrays, and no test reads it. Belongs
to `#where-the-corpus-lives`.

**Making a claim mechanical is the job**, and the strongest form makes the
forbidden state *unspellable*. §8.6: distrust drops the rope, so
`query` cannot yield a `Trusted`, and `OpenDocument::new` — the only route to a
seed and so to `dispatch` — takes one. Nothing abstains on an untrusted
document because nothing can do anything else. **If a claim seems to need a
seam change, check whether its path crosses the seam at all** — an
`AbstainReason::Untrusted` variant looked necessary until the untrusted path
turned out never to reach a handler. Also: private fields, named constructors,
exhaustive matches.

**Mutation-test each property separately, and check the mutation took.**
Making a required field optional rarely compiles; `#[serde(alias)]` substitutes
— but two fields aliasing one JSON name do not both bind, so that mutation does
nothing and reads as a surviving test. When none compiles, invert the assertion.
`grep` that a scripted edit matched and check for `^error`: a compile failure
also reads like a surviving test. A positive control is mandatory in a file of
refusals.

**Traps.**

* An added `Error` sub-enum breaks exactly two matches:
  `dispatch.rs::classify`, `replay.rs::failure_class`.
* In `tests/*.rs`, `clippy::panic` needs listing in the file-level `#![expect]`
  beside `expect_used`; an `other =>` arm trips `wildcard_enum_match_arm` —
  write `other @ (A | B)`.
* Never `git checkout <path>` over uncommitted work. Commit green, then mutate.
* `measure_core::run` writes to a raw `stdout()` cargo cannot capture.
* Widening a seam type trips `result_large_err` at 128 bytes. Box inside.
* Bulk edits by script skip the format hook — `cargo fmt -p <crate>`.
* `FileList::enumerate` never returns `Err`. Time must *move* → `DrivenClock`.
* Manifest assertions are subsets (`deps.md` §14). Fixtures are real dirs.
* §6 fixtures: `same_module_tree` is "same containing directory"; a mismatch
  needs every shim location >3 lines from every child one.

**Clippy** disallows: `serde_json::Value`, `Instant::now` (use
`SystemClock.now()`), `read_dir`, `Command::output`, `io::stdout`,
`thread::spawn`, `unbounded`.

**Gate.** It inspects unstaged and untracked paths, so a stray edit anywhere
un-greens the no-argument form: commit, then `harness/gate conformance --rev
<sha>`, `harness/hj record conformance`, re-gate.
