---
id: conformance-012
status: accepted
opened: 2026-08-03T08:30:00+00:00
campaign: 0faab934-4ecd-4a55-b992-c112e0bfcb4d
kind: class-b
---

# Where does `ProjectView::parse` get its `tree_sitter::Language`?

## Context

`resolution.md` §3 prints the method with no route to a grammar:

> `/// Parsed tree, from the parse LRU when possible.`
> `pub fn parse(&self, path: &ProjectPath, text: &FileText) -> Option<Tree>;`

Neither parameter carries one. `ProjectPath` is a root plus a relative path
and `FileText` is bytes, so as printed the method cannot construct a parser.
The signature was written when the LRU it defers to lived in `driver`, which
holds the registry and therefore every handler's `grammar()`; §3 then moved
`ProjectView` into `shared` — "under `phases.md` the measurement binaries
exist **a whole phase before `driver` does**" — and the grammar did not move
with it.

It cannot be settled without a trade because the two ways to supply it put the
cost in different places, and one of them changes a signature the phase gate
freezes. `core.md` §1 lists `ProjectView` among the seam types decided in
phase 1a, and `state/phase.toml` makes any change to it a Class B escalation
even while `crates/shared/` is this loop's to write.

The related question is *not* open: `core.md` §1 already says why `driver`
takes its grammar as a runtime value rather than a build dependency —
"`grammar()` is what keeps `driver` language-free" — so whatever supplies the
view is a `tree_sitter::Language` obtained from a registered handler, never a
`tree-sitter-<lang>` edge. This decision is only about *when* it is handed
over.

## Options

**A. The grammar is handed to `ProjectView::new` (in force).** The view is
already instantiated per query, and a query is dispatched to exactly one
handler, so "this view's grammar" is well defined. §3's printed `parse`
signature is satisfied exactly. Cost: `ProjectView::new` grows a third
parameter, and the type now asserts one language per view — which is true of
every construction the design has, but is an assertion the printed struct did
not make. A view that had to parse a second language would need a second view.

**B. `parse` takes the grammar as a parameter.** No constructor change, and
the one-language assertion is never made. Cost: it deviates from §3's printed
signature, and it moves the grammar to every call site — a handler calling
`parse` in three stages passes `self.grammar()` three times, which is the kind
of repetition that eventually acquires a helper on the handler side, in every
language crate.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A.** Tagged at `crates/shared/src/project.rs` (the field and
`new`) and at `crates/measure_core/src/replay.rs` (the only production
construction site there is).

It is the more reversible of the two because the work that has to be redone is
proportional to the number of `ProjectView::new` calls, which is one, rather
than to the number of `parse` calls, which is zero today and grows with every
handler written. Reversing A costs a constructor edit; having chosen B and
reversing costs a sweep of every `lang_*` crate's search stages. The
asymmetry runs the same way as `conformance-005`'s and for the same reason:
the cost of this decision is monotonically increasing in the number of
language crates, so the cheap moment to answer it is now.

A also keeps §3's printed signature true, which matters more than it looks —
`parse`'s signature is what a language author reads, and a document that
prints one thing while the code takes another is the failure mode the whole
conformance loop exists to catch.

## Consequences

If the answer is B: `ProjectView::new` loses a parameter, `parse` gains one,
`replay.rs`'s call site changes, and §3 is edited to print the parameter.
That is under twenty lines today. It grows by one call site per resolution
stage per language.

If the answer is A stands, `resolution.md` §3 should say so where it
introduces the struct, since "the pool is handed to it at construction" is
already the pattern and the grammar is the second thing handed over the same
way.

## Answer — 2026-08-03T19:00:32+00:00

**Ruling:** accepted

Option B: parse takes the grammar as a parameter.

**Rationale:** Option B: `parse` takes the grammar as a parameter. Not the provisional choice, so this one has real reconciliation work -- three files are tagged against A.

B is chosen because A makes an assertion the printed struct never made: one language per view. That is true of every construction the design has today, and a type that asserts it forecloses a view that has to parse a second language without anyone deciding to foreclose it. B's cost is repetition at the call site -- a handler passing self.grammar() three times -- which is visible, local, and cheap to fix later with a helper on the handler side. A's cost is a structural claim that is invisible until something needs to violate it.

Reconciling the sites tagged `// DECISION-conformance-012: provisional` is a
normal campaign target, not an interrupt.
