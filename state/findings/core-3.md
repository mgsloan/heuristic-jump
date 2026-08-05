# Findings — core, worker 3

## Falsified — act on these directly

**A `measure_<lang>` crate may not have tests.** `core.md` §9's template says
so and `seam.rs::adding_a_language_costs_the_template_and_one_line` enforces
it. I wrote a binary-level test in `crates/measure_rust/tests/`, all three
plants fired, and it had to be deleted — `CARGO_BIN_EXE_measure-rust` exists
only for that package, so there is nowhere else. Do not rebuild it.

**§9's "the JSONL records are not tracing output" resists a mechanical check.**
Every formulation false-fires on correct code: field scans hit
`queries = records.len()` (which §7 requires); line scans hit
`"a query record would not serialize"`. I spent the turns; do not repeat them.

**A prose scan is satisfied by any occurrence in its window.** My fixed
manifest comment defeated its own new check because a second paragraph
*discussing* the rule contained the crate name the sentence had dropped. Prose
about a rule masks prose stating it.

**A plant that breaks the build reads exactly like one that does not fire** —
no `test result` line at all. `pub fn` in `driver` trips `-D unreachable-pub`;
commenting a vendored dependency breaks its crate. Tail the raw output.

**A whole-file `str.replace` is not a plant.** Replacing `measure_core` across
`Cargo.toml` rewrote the member list and the path dep. Anchor on a phrase
unique to the sentence.

**`tracing_subscriber` written in prose inside `crates/*/src` fails the seam
scan** (plain `contains`). Spell it hyphenated outside code.

**Still true, cheaply:** the gap-log recipe (find the row that *opened* the id;
ask whether a later row's `sections_audited` names its section) costs one turn.
Mine was fresh this time — first in three campaigns. `/tmp` is read-only;
revert plants with the inverse replacement, never a backup copy.

**Do not re-take:** `deps.md#9` is swept sentence by sentence — five commits,
none of them the assigned site except the first. `deps.md#13`'s gix/git2 reason
is corrected and its scope asserted.

## Confirmed — candidates, test on your own evidence

* **A rule stated in several places goes wrong in the copies nothing reads
  back.** §9 lives in the section, the workspace manifest's comment, and each
  installer's doc comment. Four of five defects were in a copy, not at the
  audit's `where`. Hunt for the second copy before the named site.
* **The audit's proposed mechanism can be too weak to hold its own gap.** It
  asked for a name scan; `tracing-subscriber`'s blanket `MakeWriter for
  Fn() -> W` walks straight through one. Read the value back out of the impl.
* **Assert the destination, not just the wrapper.** `PrefixedWriter<Stdout>`
  passed the first version — a log line on the JSON-RPC wire.
* **d9435dad's scoping test keeps working**: `Command::new("git")` scoped to
  `measure_core` survives it; a `std::io::stderr` ban does not, because
  `shim.md` §2's transport will legitimately need a raw handle to forward the
  child's stderr verbatim.

## Blocked on a human

`clippy.toml` (`core-003` accepted; its lru ban still cites 0.18.1 against a
0.16 pin — `deps.md#8` minor, denied path). `deny.toml` (`core-021`/`core-023`),
`harness/measure` (`core-001`).
