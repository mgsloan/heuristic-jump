# Findings — core, after e797a506

**Check the gap list against `git log` before believing it.** All four gaps in
this round's assignment were already closed, with tests, by `ff3e1a40`'s
follow-on commits — which landed hours *after* the audit stamped those
sections. Two commands: per-section `last_audited` in `state/audit/core.toml`
(UTC) against `git log -- crates/<crate>/` (−06:00).

**When the gaps are stale, do not close — read the sections instead of the
list.** Verifying produces nothing; the sections go clean at the next audit
either way. Reading §7's five measure sections found ten claims with no test
and one with no implementation.

**A claim can be satisfied at the call site and still be false end to end.**
§7's "`measure replay` reports its own wall clock" had the `tracing::info!`
right there — and no subscriber behind it, because a `measure_<lang>` main is
four lines and `heuristic_jump::main` is where the subscriber lived. Every
`info!`/`warn!` in `measure_core` went nowhere. When a section says the tool
*reports* something, follow the value to a file descriptor.

**Hold an absence as a closed set.** "There is no `--held-out` flag, and there
must not be" is unassertable by naming it — any *other* new flag passes. §7's
usage block parses out of `design/core.md` and compares to `clap`'s flag set
per subcommand, so editing either side fails. Parsing the design document as
the fixture is still the only shape where faking progress by moving the spec
fails.

**Run the control, and check it fails for the right reason.** One of mine first
failed on a log file that was never created rather than on the assertion that
says what is missing. Another could not be controlled from the implementation
at all — the corpus's identifier rule and the handler's *are* one function, so
breaking it moves both sides; the control has to be in the fixture.

**Where a handler is `&self`, its `Trace` is the observation channel** for what
it was handed. No interior mutability, no lock, and the labels come back in the
record.

**Still true.** `rope-modifications.md` is finished — do not re-read it. Two
documents were satisfied but unheld; assume the third is too. The gate runs
`rope` and `sum_tree` (`conformance-003`). Stage explicit paths, never
`git add -A`.

**Known open, descending value:**

* **`collect` is undrivable from a test.** `Collection` is `pub(crate)`; the
  only way in is `resolve_server`, reading the real `servers.toml`, which is
  outside this loop's write list. Everything past the header — checkpointing,
  resume, `wait_until_useful`, the four outcomes — is unheld. Needs a
  write-list change or a public seam taking a `ServerEntry`.
* **`harness/measure` does not exist** (`state/decisions/core-001.md`, open).
  The digest half of §7 is denied to every loop, and the audit cannot see it —
  every `where:` points into `crates/`.
* **The instantiated template is held by a double.** `lang_rust` declares "no
  tests, deliberately"; the only home is `crates/measure_rust/tests/`.
* **The driver request path** (`#5-deadlines`, `#both-sides-are-sets`,
  `#7-observability`'s emission) is phase 2b. Escalate the phase question
  before building it.
* **`#9-workspace-layout` can never close** — it names `lang_python` and
  `lang_typescript`, outside every owned path.
