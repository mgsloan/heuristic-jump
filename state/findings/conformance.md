# Findings — conformance, after 2fdda442

**The audit lags; verify a gap before working it.** Already satisfied:
`#vocabulary-types[fbe658c158]`, `#the-trait`, `#two-modes[90c8d7bd21]`,
`#the-dependency-graph`, `#adding-a-language`, `#what-the-templates-handler-does`,
`#the-command-line`, and every gap whose `found:` says `measure_core`/
`measure_rust`/`lang_rust` "do not exist" — all three do.

**Genuinely open, verified:** `#86-modelling-errors` (no trust state anywhere),
`#both-sides-are-sets`, `#10-testing[ddadbddae0]` (no frame codec) — all three
wait on `driver`'s actor, off `driver::run`, still a config report. One is a
campaign. `#85[081351da0e]` (negative-parse assertions) is small but cannot
clean its section alone. `#vendoring[148fd8d277]` is its own campaign.

**New, and the audit has not seen it:** `measure_core`'s `manifest::parse`
(`corpus.rs`) cannot read the real `servers.toml` — it expects `[[server]]`
with `name`/`language` and one `key = value` per line; the file has
`[server.<name>]`, `languages`, and multi-line `command` arrays. So
`resolve_server` fails on the first table header and `measure collect` cannot
resolve any server. No test reads the real file (the pipeline fixture invents
`"oracle"`). Belongs to `#where-the-corpus-lives`/`#two-modes`.

**A `found:` naming a missing subsystem is the default false blocker.** Ask
what remains after removing it; usually ordinary code. 2fdda442's real
obstruction was an unmade decision — where the canonical server list lives —
not the missing actor. It was already in the repo (`servers.toml`).

**When the canonical list is a file outside your write list, copy it and test
the copy.** Two copies is what `core.md` normally refuses; justified only when
the test turns disagreement into a build failure. `driver/tests/oracle.rs`
scans `servers.toml` itself — a test sharing a parser with the code it checks
shares its bugs.

**Making a claim mechanical is the whole job.** Strongest first: (1) remove
what the claim forbids; (2) private fields + named constructors, so the state
the gap describes stops being expressible (`ServerProfile`, `Location`,
`Strata` — this is the seam's idiom, and narrowing this way is *not* a Class B
escalation); (3) an exhaustive match; (4) a test, mutation-checked before
committing. Always assert an artifact is non-empty before comparing or looping
over it — an empty scan passes everything vacuously.

**Case-splitting on a `#[non_exhaustive]` seam enum wants a method on it.**

**Traps.**

* **Never `git checkout <path>` to undo a mutation** — it restores HEAD and
  eats the campaign's uncommitted work. `cp` a backup; read the output, since
  a compile failure looks like a clean run.
* `measure_core::run` writes to a raw `stdout()` cargo cannot capture; return
  the artifact (`replay_table`) instead.
* Widening a seam type trips `result_large_err` at 128 bytes. Box inside.
* `python3`/`perl` bulk edits skip the format hook — `cargo fmt -p <crate>`.
* `FileList::enumerate` never returns `Err`. Time must *move* → `DrivenClock`.
* Manifest assertions are subsets (`deps.md` §14). Fixtures are real dirs.

**Clippy.** `unwrap`/`expect`/`panic` denied in *free* fns in `tests/*.rs`;
file-level `#![expect(…, reason)]` listing **only** lints tripped. Prefer
`.expect()` over `panic!` there — one suppression covers both. Also
`redundant_clone`, `unreachable_pub`, `cast_possible_truncation`. Disallowed:
`serde_json::Value`, `Instant::now`, `read_dir`, `Command::output`,
`io::stdout`, `thread::spawn`, `unbounded`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`#9-workspace-layout` can never go clean (`lang_python`/`lang_typescript` are
outside every owned path). `conformance-011`: `similarity` is denied.

**Gate.** It inspects unstaged and untracked paths, so a human edit under
`harness/**` un-greens the no-argument form: commit yours, then
`harness/gate conformance --rev <sha>`.
