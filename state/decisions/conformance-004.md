---
id: conformance-004
status: accepted
opened: 2026-08-03T05:05:00+00:00
campaign: e3b8dbf4-56aa-48fc-9a4d-4018d7464f4d
kind: class-b
---

# Does `Location` keep public fields, or become private with accessors?

## Context

`core.md` §1 and §8.4 both print the type with public fields:

```rust
pub struct Location {
    pub uri: DocumentUri,
    pub range: ByteRange,
    pub line: LineIndex,
}
```

and §8.4's prose says the opposite about how it is built: "The risk is a `line`
that disagrees with `range`. `Location` is therefore constructed only through
`Location::at_node(uri, node)`, which derives both from the same node, so the
two cannot drift apart by hand." §1's doc comment repeats it — "Constructed
only via `Location::at_node`, so the two cannot disagree."

In Rust those cannot both hold. With three public fields a struct literal is
available to every crate in the workspace, so `at_node` is a convenience and
the invariant is a convention; the failure it exists to prevent — a row that
does not match the range — is then a code-review question in every `lang_*`
crate rather than a compile error, and it is a failure whose symptom is a
confidently wrong jump three lines off.

This is Class B rather than Class A because `Location` is a vocabulary type on
the frozen seam, and because the two readings are not equally free: privacy
costs something real to the code that consumes a `Location`, and that code
does not exist yet, so the cost is being estimated rather than observed.

## Options

**A. Private fields, `at_node` the only constructor, `uri()`/`range()`/`line()`
accessors.** The invariant becomes a property of the type. Cost: every consumer
reads through a call, no struct literal in a test fixture or a corpus replay,
and no destructuring `let Location { uri, range, .. }`. The known consumers are
all reads — the driver's `WireLocation` conversion (§8.4), the agreement
predicate's `(uri, line)` (§6), and the trace record's `heuristic_locations`
(§7) — so nothing today needs the literal. The risk is a consumer that does not
exist yet: if `measure_core`'s replay path ever has to rebuild a `Location`
from a recorded JSONL row, it has no node and therefore no constructor, and a
second constructor taking the fields directly gives the invariant away again.

**B. Public fields as printed, `at_node` as the recommended constructor.**
Costs nothing anywhere and matches both code blocks verbatim. The invariant
holds only as long as everyone uses `at_node`, and the audit cannot check
"everyone did" — it can only check that `at_node` exists.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A**, tagged at `crates/shared/src/vocabulary.rs`'s `Location::at_node`
with `// DECISION-conformance-004: provisional`.

It is the more reversible of the two, which is the whole reason it is the one
in force: going from A to B is deleting three accessors and adding `pub` three
times, and the compiler names every call site. Going from B to A means finding
every struct literal that has accumulated in the meantime, in crates that will
by then include one `lang_*` per language — the same asymmetry `resolution.md`
§7.4 uses to argue that `policy.decide` must be the only commit path from the
start, and for the same reason: half-adopting it later is worse than either
choice.

## Consequences

If the answer is B, the change is mechanical and local: three `pub`s, three
deleted accessors, and the call sites the compiler points at. Nothing else in
the tagged work has to be redone.

If the answer is A, the thing to settle at the same time is the replay path:
`measure_core` reading a recorded location back out of JSONL wants a
constructor that takes the fields, and granting it silently is how A decays
into B. `core.md` §7's record carries `heuristic_locations` for reporting
rather than for reconstruction, so the likely answer is that replay compares
recorded rows and never rebuilds a `Location` at all — but that is a claim
about a program nobody has written yet.

## Answer — 2026-08-03T05:13:34+00:00

**Ruling:** accepted

Option A. Private fields, at_node the only constructor, uri()/range()/line() accessors. core.md sections 1 and 8.4 both print the struct with pub fields and both say in prose that at_node is the only constructor; the code blocks are what is wrong, and removing those three pubs is the Class A follow-up.

**Rationale:** CLAUDE.md line 134 already decides this — prefer enums that enforce an invariant over a comment describing it — and the invariant here is worth the enforcement: a line that disagrees with its range is a confidently wrong jump three lines off, which is this tool value proposition inverted. The record`s one open worry closes on inspection rather than argument: core.md section 7 stores heuristic_locations as strings for reporting and computes agreement at collect time, so replay compares recorded rows and never reconstructs a Location. There is therefore no consumer that needs a field-taking constructor, and the way A decays into B does not arise. Its reversibility argument is also right and is the deciding practical point: B to A later means finding every struct literal accumulated across one lang_* crate per language, which is the same asymmetry resolution.md section 7.4 uses for CommitPolicy.

Reconciling the sites tagged `// DECISION-conformance-004: provisional` is a
normal campaign target, not an interrupt.
