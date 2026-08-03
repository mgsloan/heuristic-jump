# Findings — conformance, after a482daad

**The audit lags; verify a gap before working it.** Already satisfied, do not
rework: `#vocabulary-types[fbe658c158]`, `#the-trait`, `#two-modes[90c8d7bd21]`,
`#the-dependency-graph`, `#adding-a-language[0858868078]` (heuristic_jump does
register `lang_rust::Handler::new()` and `seam.rs` asserts it),
`#what-the-templates-handler-does`, `#the-command-line`,
`#2-document-snapshots[d1cd7205ef]` (both consumers go through
`SnapshotSeed::realise`; `DocumentSnapshot::tree` is private so there is no
second route), and both `#7-observability` gaps — every gap whose `found:` says
`measure_core`/`measure_rust`/`lang_rust` "do not exist", or that `Outcome`
carries one `stratum`. All are dated `06:54`.

**Genuinely open, verified:** `#86-modelling-errors` (no trust state, no
document map, `DidChange`/`DidSave` params unread), `#10-testing[ddadbddae0]`
(no frame codec), `#85[081351da0e]` (negative-parse assertions; small, but
cannot clean its section alone), `#vendoring[148fd8d277]` (its own campaign).
`#9-workspace-layout` can never go clean — `lang_python`/`lang_typescript` are
outside every owned path.

**Also open and the audit has not seen it:** `measure_core`'s `manifest::parse`
(`corpus.rs`) cannot read the real `servers.toml` — it expects `[[server]]`
with one `key = value` per line; the file has `[server.<name>]`, `languages`,
and multi-line `command` arrays, so `resolve_server` fails on the first table
header. No test reads the real file. Belongs to `#where-the-corpus-lives`.

**A `found:` naming a missing subsystem is the default false blocker.** Delete
the clause and ask what remains; it is usually ordinary code that also tests
fine without the subsystem. a482daad's "there is no run loop" hid a struct, a
map and a comparison. `#86` and `#10-testing[ddadbddae0]` are the last two gaps
hiding behind `driver`'s actor, and they are *separate* campaigns — checked,
and they share no file with the pending-query work.

**Making a claim mechanical is the whole job.** Strongest first: (1) remove
what the claim forbids; (2) private fields + named constructors, so the state
the gap describes stops being expressible (`ServerProfile`, `Location`,
`Strata`, `Divergence` — the seam's idiom, and narrowing this way is *not* a
Class B escalation); (3) an exhaustive match; (4) a test, mutation-checked
before committing. Two shapes that keep earning their place: make the setter
take a type the caller cannot have tampered with (`&Answer`, not
`Vec<Location>`), and derive an `Option` from an existing partial function
(`Agreement::severity()?`) rather than writing a `match` that returns `None`
twice — a rule the next caller can decline to follow is not mechanical.

**Mutation-test each property separately.** Two mutations that look like one
claim often are not: reversing `resolve`'s classification input and reversing
what the report reads fail *different* tests, because they reach the stored
list by different routes. Always assert an artifact is non-empty before
comparing or looping — an empty scan passes everything vacuously.

**Fixture trap in §6's neighbourhood.** `same_module_tree` is "same containing
directory", so two files in `src/` are `NearModule` and never `Unrelated`; and
any fixture wanting a mismatch must keep *every* shim location more than three
lines from *every* child location, or it silently becomes `match_contained`.

**Traps.**

* **Never `git checkout <path>` to undo a mutation** — restores HEAD, eats the
  campaign's uncommitted work. `cp` a backup; read the output, since a compile
  failure looks like a clean run. Better: commit green first, then mutate.
* A `perl -0pi -e` substitution that silently fails to match reads exactly like
  a mutation the tests survived. `grep` the result before believing it.
* `measure_core::run` writes to a raw `stdout()` cargo cannot capture.
* Widening a seam type trips `result_large_err` at 128 bytes. Box inside.
* `python3`/`perl` bulk edits skip the format hook — `cargo fmt -p <crate>`.
* `FileList::enumerate` never returns `Err`. Time must *move* → `DrivenClock`.
* Manifest assertions are subsets (`deps.md` §14). Fixtures are real dirs.
* `ShowMessageParams.message` is `Box<str>`, not `String`.

**Clippy.** `unwrap`/`expect`/`panic` denied in *free* fns in `tests/*.rs`;
file-level `#![expect(…, reason)]` listing **only** lints tripped. Disallowed:
`serde_json::Value`, `Instant::now` (use `SystemClock.now()`), `read_dir`,
`Command::output`, `io::stdout`, `thread::spawn`, `unbounded`.

**Ruled out.** §8.5's golden corpus is captured traffic — an intervention.
`conformance-011`: `similarity` is denied. `servers.toml` is unwritable, so
`ServerId::KNOWN` copies it and `driver/tests/oracle.rs` makes disagreement a
build failure — the only justification for two copies.

**Gate.** It inspects unstaged and untracked paths, so a human edit under
`harness/**` un-greens the no-argument form: commit yours, then
`harness/gate conformance --rev <sha>`, then `harness/hj record conformance`,
then re-gate.
