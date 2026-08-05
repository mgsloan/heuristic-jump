# Spec changelog — core, worker 3

One entry per Class A change, in the shape the loop prompt fixes. Newest last.

## CHANGE-core-027 — rope-modifications.md#the-dimension-impls — `sum_tree` is not a pristine copy, and the claim this cites no longer says it is

**Contradiction:** `#the-dimension-impls` reads

> **`sum_tree` needs no changes at all.** `Dimension` is generic over the
> summary type, so the impls live in rope. That matters: `sum_tree` stays a
> pristine copy, and [section 9](../../design/core.md#vendoring-the-zed-crates)'s
> claim that it needs no patching survives.

`vendor/README.md`'s "Patches to `sum_tree`" lists three: `src/tree_map.rs`
deleted with its `mod` and `pub use` lines; `ztracing::instrument` →
`tracing::instrument`, one line each in `src/sum_tree.rs` and `src/cursor.rs`;
and `#[ctor::ctor] fn init_logger` + `zlog::init_test()` deleted. So the
vendored crate is not a pristine copy and has not been one since it was
vendored.

The cross-reference is the sharper half. `core.md#vendoring-the-zed-crates` does
not make the claim this sentence says survives — it makes the opposite one, in
bold: "**`sum_tree` is patched, minimally, and the newtype work is not why.**"
That section was corrected in an earlier campaign; this one kept citing the
sentence it used to have.

**Resolution:** the paragraph now says `sum_tree` needs no changes *for this*,
states in as many words that it is not a pristine copy, names the three patches
and what they have in common — all dependency-stripping, none of them this
document's — and quotes §9's claim in the form §9 actually makes it.

This is the reading that trades nothing off because the two halves of the
original sentence were never one claim. "The dimension impls cost `sum_tree`
nothing" is true, is this section's own point, and is untouched. "`sum_tree` is
byte-identical to upstream" is a different and false claim that was riding on
it, and deleting it costs the document nothing it was using. The distinction
survives as the one a re-sync actually needs, which §6 already draws for `rope`:
a crate patched only to remove dependencies re-syncs as a replayable diff, and
a crate patched throughout for an API change re-syncs as a merge.

**No code moved with it**, and none could have: the three patches are recorded,
predate this campaign by several, and are required — `ztracing`, `zlog` and
`ctor` are Zed-internal crates that are not in this workspace and cannot be.
Nothing under `crates/` or `vendor/*/src/` is touched by this entry.

The mechanism is `vendor/rope/tests/newtype_api.rs`'s
`the_dimension_impls_cost_sum_tree_nothing_but_sum_tree_is_not_pristine`, which
reads the section and `vendor/README.md` together, so the weaker sentence
cannot come back without the patch list also emptying.

**Campaign:** b106a11d-e4c9-43d7-8aad-dc42cb9f54d5
