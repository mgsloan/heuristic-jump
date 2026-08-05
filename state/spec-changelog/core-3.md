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

## CHANGE-core-028 — core.md#vendoring-the-zed-crates — the sweep replaces bare integers, and `TextSummary`'s are not `u32`

**Contradiction:** `core.md#vendoring-the-zed-crates` opens

> its public API speaks in newtypes — `Offset`, `ByteLen`, and `ByteRange`
> instead of `usize` and `Range<usize>`, and `LineIndex` / `ByteColumn` /
> `Utf16Column` / `CharCount` instead of the bare **`u32`s** in `Point`,
> `PointUtf16`, and `TextSummary`

and names `rope-modifications.md` in the next sentence as the document to read
before touching `vendor/`. That document states the same claim as its opening
blockquote, and states it differently:

> `ByteColumn`, `Utf16Column`, and `CharCount` instead of the bare **integers**
> in `Point`, `PointUtf16`, and `TextSummary`.

`TextSummary` is what separates them. Its `len` and `chars` are `usize`
upstream, not `u32`, so "the bare `u32`s" is false of exactly the type the
sweep changes most — and `rope-modifications.md` §2 spends a paragraph on why
`CharCount` is the one member of the family backed by `usize` rather than
`u32`, an argument `core.md`'s wording quietly contradicts.

**Resolution:** `core.md` now says "the bare integers", which is
`rope-modifications.md`'s own word for the same set. This trades nothing off
because the sentence is a summary of the other document pointing at it: where
the two disagree about a claim only one of them argues, the one that argues it
is the one that stands. Nothing in `core.md` depends on the narrower reading —
the section's three "things stated here because they change this section's own
claims" are about where the types are defined, the re-sync cost, and upstream's
tests, none of which mentions a width.

**Code moved in this campaign and this entry is downstream of it, which is
worth stating plainly rather than leaving to be noticed.** `CharCount` was
`u32` in the code and `usize` in `rope-modifications.md`; commit `831f79f`
widened the code, because a human commit (`1b9dd51`, "Design change: CharCount
is usize") had moved the document two days after the code was written. This
entry then corrects the *third* place the claim is stated. No design sentence
was moved toward the code at any point: the code moved toward the design, and
`core.md` moved toward `rope-modifications.md`.

The mechanism is `vendor/rope/tests/newtype_api.rs`'s
`both_documents_describe_the_newtype_sweep_the_same_way`, which asserts the
shared clause in both documents, so a revert of either fires. Both directions
planted.

**Campaign:** b106a11d-e4c9-43d7-8aad-dc42cb9f54d5
## CHANGE-core-036 — deps.md#9-logging-and-tracing — the prefix rule and the filter default are scoped differently, and only one of them excuses `measure`

**Contradiction:** §9 states the prefix unconditionally —

> "Every line we emit gets a distinguishing prefix, and the default filter is
> `warn` so we are quiet unless asked."

— and then, arguing for `measure_core`'s `info` default, gives a reason that
would excuse it from both:

> "a `measure` run has neither a child nor an editor while §7 requires it to
> report its own wall clock."

The "neither a child" half is false of the code. `measure_core::client::Server::start`
spawns the language server with `.stderr(Stdio::inherit())` — "a server's stderr
is its own", in its own comment — so a `collect` run has exactly the
interleaving the prefix exists for, in the one process that had no prefix:
`install_logging` handed `tracing-subscriber` a bare `std::io::Stderr`.

**Resolution:** §9 now separates the two rules. The prefix applies to every
process of ours that forwards a child's stderr, which is both of them; the
`warn` default is justified by the editor panel alone, which is the half that
is true, and `measure`'s `info` stands on the same reason it always did (§7
requires it to report). Nothing is traded off: the unconditional claim is not
weakened, and the `info` default is not disturbed — what changes is that its
stated reason no longer asserts something the code contradicts.

The mechanical consequence is that `LOG_PREFIX` and `PrefixedWriter` move from
`driver` to `shared`, because `core.md` §9's graph gives `measure_core` no edge
to `driver` and two copies of a prefix are two strings that can drift. No graph
edge is added: `driver` and `measure_core` both already depend on `shared`, and
nothing in `shared` reaches `tracing-subscriber`, which is the line §9 draws
and the one `driver/tests/seam.rs` scans for.

**Spec and code in one campaign, deliberately:** this edit accompanies the fix
to `measure_core::install_logging` and a new assertion in
`driver/tests/seam.rs`. The direction is worth checking rather than taking on
trust — the section's load-bearing claim ("every line we emit gets a
distinguishing prefix") is left exactly as it was and the code was moved to
satisfy it; what the spec lost is a false premise in a subordinate argument.

**Campaign:** b67cc6d7-c0e6-4a24-bb9e-1adfdb5779f4

## CHANGE-core-038 — deps.md#13-explicitly-not-depended-on — gix/git2 are still rejected, but not for the reason given

**Contradiction:** §13's entry reads —

> **`gix` / `git2`** — `ignore` reads `.gitignore` files directly; we never
> need to talk to git.

— while `measure_core::corpus::verify_checkout` runs `git rev-parse HEAD` and
`git status --porcelain` on every repository of every corpus run, and
`measure_core/tests/pipeline.rs` builds its fixture checkouts the same way.
`data-collection.md` §1 requires that verification, so it is not incidental:
the clean-tree half is what stops a modified file shifting the byte offsets a
truth file was frozen against.

**Resolution:** the entry keeps the rejection and states what it actually
rests on. Nothing on the *query path* talks to git — that half is true, and
`ignore` reading `.gitignore` directly is why. What talks to git is corpus
verification, and it needs two commands in a program that already spawns a
language server, with no latency budget to protect; a git implementation
linked in for that buys nothing the `git` on `PATH` does not, and `git2` would
put libgit2 in the graph as well.

Nothing is traded off: no dependency changes, and the rejection is not
weakened. What changes is that the reason is now checkable, which it was not
while it quantified over the whole repository —
`driver/tests/seam.rs::no_member_declares_a_crate_section_13_rejects` asserts
that `Command::new("git")` appears in `measure_core` and in no other member,
so a `git` call reaching `driver` fails rather than quietly becoming the case
for the library §13 has now declined twice.

**Spec and code in one campaign:** the document is edited and no code it
describes moves — the only code change is the assertion above. The direction
worth checking is that the entry says *more* than it did rather than less.

**Campaign:** b67cc6d7-c0e6-4a24-bb9e-1adfdb5779f4
