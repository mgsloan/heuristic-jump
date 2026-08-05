---
id: core-027
status: open
opened: 2026-08-05T02:05:00+00:00
campaign: b5c7bae2-e62f-478a-8faf-182713552bd6
kind: class-b
---

# When one workspace root contains another, which root owns the files inside both?

## Context

`FileList::enumerate` walks each root the editor sent. Nothing stops the roots
overlapping, and an editor is entitled to send folders that do: opening a
monorepo and one of its packages is an ordinary thing to do in VS Code, and
`workspace/didChangeWorkspaceFolders` can produce it at any time.

Today both walks find the inner root's files, and each mints its own
`ProjectPath` — `(outer, "src/lib.rs")` and `(inner, "lib.rs")` are different
values with different hashes, so `Set<ProjectPath>` does not deduplicate them.
Observed on a two-root fixture: three of the five files appeared twice.

The consequence is not the extra entry. `core.md` §1 has the scan read every
candidate exhaustively, and `resolution.md` §1.3 makes that exhaustiveness the
source of the uniqueness signal:

> a partial scan cannot tell "the only definition of this name in the project"
> from "the first of eleven", and global uniqueness is the main confidence
> signal the later stages rank on

A duplicated candidate is the same defect from the other side: one definition
comes back as two hits in what look like two files, so the query that should
have committed *the only definition* sees an ambiguity and abstains. The trace
record is wrong the same way — `bytes_scanned` and `files_scanned` double-count,
and `core.md` §7 makes `bytes_scanned` the deterministic machine-independent
proxy for latency that gates are compared on.

It cannot be settled without trading something off because deduplicating means
picking an owner, and the owner decides which of `resolution.md` §4's tiers the
file lands in for a document elsewhere in the outer root — tier 3 (same root,
far away) or tier 4 (another root). That is search-scope ordering across roots,
which is `open-questions.md` question 8:

> How should multi-root workspaces order search scope? The folder containing
> the requesting document first is the obvious default, but a monorepo with
> many roots may want the pagerank-style ranking from question 1 instead.

Question 8 is about *preference* between roots and does not reach this: it
assumes a file belongs to one root and asks which root to prefer. But an answer
to it would be written against whatever this decides, so deciding it silently
would constrain question 8 from underneath.

## Options

**A. The innermost root keeps it.** Consistent with `ProjectView::root_of`,
which already resolves a document to the longest matching root prefix — so a
handler's `lookup(root_of(uri), rel)` finds the entry that exists. Costs: a
file in the inner root becomes tier 4 for a document in a sibling directory of
the outer root, where today it is tier 3, so it is searched later than a
sibling-of-a-sibling. On the layout that motivates nested roots — the user
opened the inner folder because that is what they are working in — this is
arguably right rather than merely acceptable.

**B. The outermost root keeps it.** Maximises tier-3 relationships, since the
outer root contains more. Costs: it contradicts `root_of`. For a document at
`outer/src/lib.rs`, `root_of` returns `inner`, the list holds the entry under
`outer`, and `lookup` returns `None` for a file that is plainly in the project
— a handler resolving an import in its own directory gets nothing. Fixing that
means `root_of` changing too, which is a second rule and a wider change.

**C. Refuse nested roots at enumeration**, warn, and walk only the outermost.
Cheapest to reason about and loses the inner root's identity entirely, which
matters if the roots are later given per-root configuration; and a warning is
not something a user of an editor will ever see.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**A**, because it is the only one that leaves the existing public API
self-consistent: `root_of` already implements longest-prefix-wins, so A adds no
rule, where B adds a contradiction and C adds a policy. It is also the most
reversible — the whole of it is one `continue` in the walk, and reverting it
restores today's behaviour exactly, where B and C would have to be unpicked from
`root_of` and from the root list respectively.

Tagged `// DECISION-core-027: provisional` at
`crates/shared/src/project.rs`, inside `FileList::enumerate`'s walk — the single
site. `crates/shared/tests/project.rs::a_file_inside_two_roots_is_one_candidate`
holds it: it asserts no absolute path is enumerated twice, that the outer root
does not own a file the inner root contains, and that the scan reports three
hits rather than five for a fixture where one file is inside both.

## Consequences

If the answer is B or C, the tagged `continue` is replaced and the test's second
and third assertions change — perhaps twenty lines, one file each side, no
caller affected, since nothing outside `enumerate` knows how the list was built.
If the answer is B, `ProjectView::root_of` changes with it and `lookup`'s
contract is restated; that is the expensive one, and it is why A is what is in
force.

If the answer is "leave the duplicates", the deliverable is not the revert but a
statement of what a handler is supposed to do with two `ProjectPath`s for one
file — because `resolution.md` §1.3's uniqueness signal has no reading under
which they are two definitions.
