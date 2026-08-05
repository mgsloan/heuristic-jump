# Findings — core, worker 3

## Falsified — act on these directly

**Reading a test tells you what it looks at; only a plant tells you what it
would notice.** `mark_stale` refuses to restart the debounce window. I assumed
the existing debounce test held that. It does not — three triggers 50ms apart
then a whole window advanced leaves the last one 500ms behind the tick under
*both* behaviours. Planting `since: now` left all fourteen tests in the file
passing. **When you believe a test covers a claim, plant against it and run the
whole file.**

**A stale assignment costs one turn to prove** (core-1's recipe): find the `gap-log.jsonl` row that *opened* the id, then check
whether any later row's `sections_audited` names its section. Both of mine were
closed 10–11 minutes after the audit that opened them.

**`/tmp` is read-only here.** `cp x /tmp/x.bak` fails, so a plant you think is
backed up is not. Revert plants with `git checkout <path>`, and `git status`
after.

**A plant that does not apply is indistinguishable from one that does not
fire.** Mine anchored on a line the file lacked; the script wrote nothing and
the test passed. Put `assert old in s` in every plant script.

**A plant must compile.** A bare `Command::new` fails the *build*, so nothing
runs. Use a fully-qualified `std::process::Command::new`.

**`cargo test` green does not mean the gate is green.** `redundant_clone` in a
test file is a clippy error. Run `cargo clippy -p <crate> --all-targets --
-D warnings` *before* `harness/gate`.

**The gate cannot see a stale cross-reference** — it checks an anchor
*resolves*, not that the cited document still makes the claim.

**A wrong record is worse than no record.** `vendor/README.md` called the
`CharCount` narrowing deliberate; three campaigns read it *instead of* the
repr. When a document and the code disagree, two `git log -S` runs settle which
side moved, before you weigh either.

**`Trace`-not-allocated cannot be tested here** — it needs a counting
`#[global_allocator]`, and `GlobalAlloc` requires `unsafe`. Do not rebuild it.

**Do not re-take:** `core.md#the-trait` (stale four rounds running).
`core.md#4-project-file-enumeration` and
`rope-modifications.md#textsummary-is-converted-too` are now swept
sentence-by-sentence — 10 tests, every claim plant-verified. Expect clean.

## Confirmed — candidates, test on your own evidence

* **A section that carried a live gap for two audit rounds has more unheld
  sentences than the gap named.** Six commits this campaign, none of them the
  assigned gap. Nobody sweeps a section; they each close the gap in front of
  them.
* **Scope a "nothing does X" scan by asking who will legitimately break it
  first.** If the answer is "they relax the assertion", the scope is wrong —
  `driver` spawns the proxied child, so the in-process scan is the *query
  path*, not the crate.
* **A printed block a test transcribes asserts what its author believed.** Read
  it out of the document instead; `newtype_api.rs`'s `unwrapped` strips
  per-line markdown, which `split_whitespace` alone does not.

## Blocked on a human

`deny.toml` (`core-021`/`core-023`), `harness/measure` (`core-001`),
`clippy.toml` thresholds (`core-003`). **`core-025` is accepted and still
unstarted** — a `shared` + `measure_core` campaign, tagged at `dispatch.rs`'s
`Classified::strata`.
