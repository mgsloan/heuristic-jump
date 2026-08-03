---
id: conformance-014
status: open
opened: 2026-08-03T22:34:00+00:00
campaign: 51628b98-b5ea-48b1-bb77-696ecc51face
kind: class-b
---

# Is `measure_<lang>` MIT, as `deps.md` §5 concludes, given that §5's reason for it is false?

## Context

Mechanizing `deps.md` §5's per-crate licence table found the document and the
code disagreeing, and then found the document's *reason* to be false while its
*conclusion* held. Both halves matter, which is why this is a record rather
than a one-line fix.

`deps.md` §5's prose, naming `measure_<lang>` explicitly:

> `shared`, `driver`, `measure_core` and `measure_<lang>` stay MIT: none of
> them depends on `similarity`, which is a `lang_*` dependency and not a driver
> one (`core.md` §9's graph).

Against that, `crates/measure_rust/Cargo.toml` as it stood:

> `license = "GPL-3.0-or-later"` — GPL through `lang_rust`, which is GPL
> through `similarity` (`deps.md` §5).

The manifest is right about the graph and wrong about the rule. `measure_rust`
**does** depend on `similarity`: `core.md` §9 and its own manifest give it
edges to `measure_core` *and* `lang_rust`, and `lang_rust` depends on
`similarity`. So §5's stated reason — "none of them depends on `similarity`" —
is true of the first three names in its sentence and false of the fourth.

What settles the conclusion anyway is a case §5's prose does not discuss and
§14's layout listing does:

> ```
> crates/
>   heuristic_jump/   MIT -- binary crate; the artifact it builds is GPL
> ```

`heuristic_jump` depends on every `lang_*` — that is the one thing the crate
exists to do (`core.md` §9: "the single place where the language list is
enumerated") — and therefore on `similarity`, and §5 marks it MIT. So
"depends on `similarity`" is *not* the rule the document implements. The rule
is that the `license` field describes copyright in the crate's own text: GPL
marks `similarity`, which is ported from Zed's `edit_prediction_context`, and
`lang_*`, which §5 groups with it as the handler layer. Everything else we
write is MIT and the artifact is GPL regardless, through `vendor/rope`.

`measure_rust` is `heuristic_jump`'s case exactly: a binary crate, four lines
of `main`, whose artifact is GPL. Under the rule §14's listing states, it is
MIT.

This is escalated rather than treated as a Class A correction because the
change edits a `license` field, and because §5 is explicit that the per-crate
MIT marking is a *choice* with something on the other side of it rather than a
legal necessity:

> What that buys, concretely: the portable and valuable part of this project is
> `similarity` and the `lang_*` handlers … That option costs nothing today and
> is awkward to recover later, since relicensing needs every contributor's
> agreement.

A human should confirm the reading before it becomes the rule six more
`measure_<lang>` crates are copied under.

## Options

**A — MIT.** What §5's table, §5's prose and §14's layout all conclude, and
what `heuristic_jump` demonstrates the rule to be. Costs the accuracy of
reading a `license` field as "what you may do with the built artifact" — but
that reading is already given up for `heuristic_jump`, `shared` and `driver`,
so it costs nothing that was not already spent.

**B — GPL-3.0-or-later.** What the manifest said. Defensible on the grounds
that a crate whose every use links GPL might as well say so, and it is the
conservative marking. Costs consistency: `heuristic_jump` would have to move
too, and §14's listing of it would become wrong, which turns a one-crate
question into a rewrite of §5's table.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A.** `crates/measure_rust/Cargo.toml` now declares `license = "MIT"`
and `crates/measure_rust/LICENSE-MIT` symlinks the root text in place of the
`LICENSE-GPL` it carried.

It is the more reversible choice for a reason specific to this pair. B is not
a one-crate edit: taking B means `heuristic_jump` moves with it, since the two
crates have the same shape and the same graph, and §14's layout listing then
states something false — so B costs a spec change and A does not. Nothing has
been distributed under either marking, and we are the sole copyright holder,
so the field itself is one line in either direction.

Tagged sites, found by `grep -rn 'DECISION-conformance-014'`: the `license`
field comment in `crates/measure_rust/Cargo.toml`, and `expected_licence` in
`crates/driver/tests/seam.rs`, which is the scan that now holds the table.

## Consequences

If the answer is B: set `crates/measure_rust/Cargo.toml` back to
`GPL-3.0-or-later`, repoint the symlink, and add a `measure_` branch to
`expected_licence` beside the `lang_` one. Then the larger part — decide
`heuristic_jump` the same way, and amend §14's layout listing and §5's table,
since the document currently states the opposite in two places.

Either way `deps.md` §5's sentence needs an edit, because the reason it gives
("none of them depends on `similarity`") is false about `measure_<lang>`
whichever conclusion survives. That edit is deliberately **not** made in this
campaign: the sentence is where the licensing question lives, and moving the
spec to match code the same campaign wrote is the one form of progress the
audit cannot catch.
