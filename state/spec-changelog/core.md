# Spec changes — core

Class A repairs, in the shape `harness/prompts` fixes. The flat
`state/spec-changelog.md` beside this directory is the legacy file, holding
`CHANGE-conformance-001` through `-016` from before the loop was renamed.

## CHANGE-core-001 — rope-modifications.md#all-of-upstreams-tests-are-kept — nine `#[gpui::test]`, of which eight carry `iterations`

**Contradiction:** §7 says "**`#[gpui::test(iterations = N)]` on nine
functions**" and, four paragraphs later, "Two changed lines per test, nine
tests, bodies verbatim". `vendor/README.md` patch 3 repeats it: "`#[gpui::test
(iterations = N)]` on nine functions → `#[test]` calling `seeded(N,
<name>_inner)`". The vendored crate has **eight** `seeded` call sites and
eight `_inner` bodies (`src/chunk.rs:893,927,944,983,1005`,
`src/rope.rs:2009,2568,2630`).

Settled against upstream rather than by choosing, since the pinned revision is
fetchable: `zed@90d024b8`'s `crates/rope/src/{rope,chunk}.rs` carry **nine**
`#[gpui::test]` attributes, of which eight are `#[gpui::test(iterations = N)]`
and one — `test_point_utf16_to_offset_clips_to_correct_absolute_offset`,
`chunk.rs:1235` — is bare and takes no `rng`. It is present in the vendored
copy as a plain `#[test]` (`vendor/rope/src/chunk.rs:1332`), and both sides
have 24 test functions.

So the section's load-bearing claim — that every upstream test is preserved —
is **true**, and only the arithmetic describing how was wrong: nine attributes,
eight `seeded` conversions.

**Resolution:** §7 now says "`#[gpui::test]` on nine functions, eight of them
with `iterations = N`", says "eight tests" where it said nine, and names the
ninth and why it needs no `seeded` — one changed line rather than two.
`vendor/README.md` patch 3 says the same. This trades nothing off: the two
counts are both facts about upstream, and the previous text was not a decision
that could have gone the other way.

The reason it is worth more than a typo fix is the failure it was hiding in
plain sight. "Nine tests converted, eight conversions present" is precisely
what a *dropped* test looks like, and nothing in the repository could tell the
two apart — which is why `tests/newtype_api.rs` now asserts both counts
(CHANGE-core-004's experiment, same campaign).

**Campaign:** 37a6d098-e7c7-4fb3-af7a-5f1562728e56

## CHANGE-core-005 — rope-modifications.md#6-consequences-for-re-syncing — the allowlist is empty, because §4 already gave its one entry a unit

**Contradiction:** §4's second table says of `ChunkSlice::longest_row` that
"`total_chars` is a `CharCount`", under a paragraph whose whole point is that
these four functions "get the *correct* newtype rather than being left bare —
which is the point of auditing rather than grepping". §6 then says "The
allowlist is `vendor/rope/allowed-primitives.txt` and is short: the
`total_chars: &mut usize` parameter of `ChunkSlice::longest_row` and anything
else [§4] records as a genuine primitive."

Those are the same parameter, and §6 forgives exactly what §4 converts.
`allowed-primitives.txt` followed §6 and carried the single entry `longest_row`,
which meant the signature scanner — the enforcement §6 says the whole change
rests on — was skipping the one function §4 singles out. A `&mut usize` out
parameter is also where a unit is least visible: it is not in the binding at
the call site and not in the return type.

**Resolution:** §4 wins, because it is the section that decides which newtype
each function gets and it names one; §6 only mentions the parameter as an
example of a *genuine* primitive, which §4 says it is not. `longest_row` now
takes `&mut CharCount`, `allowed-primitives.txt` has no entries, and §6 says
the file is empty and explains what it is still for — the re-sync case, an
upstream `pub fn` arriving with a bare primitive that really is one. Nothing is
traded: the allowlist's purpose survives intact, and the conversion is the one
§4 already specified.

CHANGE-core-003 named this entry too, correcting `Chunk::longest_row` to
`ChunkSlice::longest_row`. It fixed which type the function is on and left the
exemption standing; this removes the exemption.

The code and the document move in the same campaign, which is the shape the
loop prompt says is watched for, so plainly: the code moved *to* what §4
already said, and the document changed only where §6 described the allowlist's
contents. No claim about what is converted was weakened to fit the code.

Held by `the_public_surface_speaks_in_the_units_it_measures_in` in
`vendor/rope/tests/newtype_api.rs`, which now binds the out parameter as a
`CharCount` and asserts its value, and by
`no_public_signature_names_a_bare_primitive`, which no longer skips anything.

**Campaign:** a9937015-4ddb-46e6-a1aa-f85ab25f09ef

## CHANGE-core-003 — rope-modifications.md#the-signatures — the four char-unit functions are `ChunkSlice`'s, and there are 27 of them not 17

**Contradiction:** §4's second table names `Chunk::first_line_chars`,
`Chunk::last_line_chars`, `Chunk::longest_row` and
`Chunk::last_line_len_utf16`, and §6 names `Chunk::longest_row` again as the
allowlist's one entry. All four are on **`ChunkSlice`**
(`vendor/rope/src/chunk.rs:374,380,386,400`, inside `impl<'a> ChunkSlice<'a>`
at `:254`); `impl Chunk` at `:55` has none of them. `allowed-primitives.txt`
already carried the discrepancy as a note rather than a fix.

The same table's row above says "`ChunkSlice` | All 17 of its public
functions". It has **27**, and so does upstream at the pinned revision — so
this was wrong when written rather than drift.

**Resolution:** the four are named on `ChunkSlice`, with one line on why
(`Chunk` holds the storage, `ChunkSlice` does the measuring, so every function
answering "how far into a line" is on the slice); the count is 27; and §6 and
`allowed-primitives.txt` say `ChunkSlice::longest_row`. Nothing about which
functions are converted or what unit each is changes — both errors are about
where a function lives and how many there are, and both are countable rather
than arguable.

**Campaign:** 37a6d098-e7c7-4fb3-af7a-5f1562728e56

## CHANGE-core-004 — core.md#vendoring-the-zed-crates — `sum_tree` is patched, and the redirect is three lines not one

**Contradiction:** §9 says of the `ztracing` redirect "That is a single-line
patch to `rope`, recorded as such", and then "**`sum_tree` needs no
patching**" — two sentences before conceding "Its `tree_map.rs` is unused here
and can be dropped", which is a patch.

`vendor/README.md` has held the answer the whole time, under "Patches to
`sum_tree`": the same `ztracing` redirect in `src/sum_tree.rs` and
`src/cursor.rs`, the `#[ctor::ctor]` logger deleted, and `tree_map.rs`
deleted. `deps.md` §5 already names all three redirect sites. So the code is
right, the record is right, and it is this section's count that is wrong.

**Resolution:** §9 now says the redirect is "one line in `rope` and one line
in each of `sum_tree`'s two instrumented files — three in all", and that
`sum_tree` is patched minimally, listing the three fix-ups and deferring to
`vendor/README.md`. What it no longer says is that the crate is unpatched.

The claim that was doing the work is kept and is untouched: `sum_tree`'s
`Dimension` is generic over the summary type, so `Offset`'s impls live in
`rope` and the newtype sweep costs `sum_tree` nothing. That is what the
section needed to be true, and it is — "needs no patching" was a stronger
statement made in passing, and nothing rests on it.

Held by `every_vendored_crate_records_the_patches_it_carries` in
`crates/driver/tests/seam.rs`, which compares the three against each other:
the crates `vendor/` holds, the crates the README records patches for, and the
redirect's actual site count. Both halves fail under a planted change.

**Campaign:** 37a6d098-e7c7-4fb3-af7a-5f1562728e56

## CHANGE-core-002 — rope-modifications.md#the-dimension-impls — four line references that the sweep moved

**Contradiction:** §1 cites the bare-`usize` dimension impls at `rope.rs:1492`
and `rope.rs:1502`, §5 cites the same two at `rope.rs:1492`, `:1502`, and §4
cites `OffsetUtf16`'s pair — the ones `Offset`'s new impls mirror — at
`rope.rs:1516`, `rope.rs:1526`. After the sweep, `rope.rs:1492` is inside
`impl TextDimension for TextSummary` and the four impls are at `:1567`,
`:1577`, `:1624` and `:1634`.

**Resolution:** the four references now name where the impls are. This is
citation drift, not a claim: the audit had already recorded the §5 pair as a
minor with the corrected line, and every one of the four impls is present and
unchanged in kind. Nothing about which impls exist, or what they do, is
touched.

**Campaign:** 37a6d098-e7c7-4fb3-af7a-5f1562728e56

## CHANGE-core-006 — deps.md#14-workspace-cargotoml-shape — the cargo-machete table is `[package.metadata]`, which is what the bullet's own precedent cites

**Contradiction:** the bullet names a workspace-level table and then cites a
package-level one as its precedent, in consecutive sentences:
"**`[workspace.metadata.cargo-machete] ignored`** for deps that are used but
invisible to static analysis. `rope` already needs `tracing` listed this way
upstream, and our patched copy still will." What `rope` carries upstream, and
what `vendor/rope/Cargo.toml:56` and `vendor/sum_tree/Cargo.toml:29` carry
here, is `[package.metadata.cargo-machete] ignored = ["tracing"]`.

**Resolution:** the bullet now names `[package.metadata.cargo-machete]`, "in
the crate that carries the dependency", and states the reason the exemption is
needed at all — the redirect reaches `tracing` only through `#[instrument]`,
which no static scan follows. This trades nothing off: it is the placement the
bullet's second sentence already appealed to, it is the one a re-sync
preserves without a diff, and it keeps the record of a dependency beside the
crate whose dependency it is rather than in a root file that would have to be
edited every time a vendored crate arrived or left.

No code moved to meet the document. Both manifests already carried the
package-level table, unchanged since the vendoring campaign; what changed is
the sentence that described them, and
`the_workspace_manifest_has_the_shape_section_14_states` in
`crates/driver/tests/seam.rs` now derives which crates need the record rather
than listing them.

**Campaign:** c601eeec-b30f-479c-8a7d-49e19e4c166d

## CHANGE-core-007 — deps.md#licensing-our-crates-are-mit-the-binary-is-gpl — high-level.md carried the position §5 records as superseded

**Contradiction:** `deps.md` §5 says "**There are two GPL inputs, not one.** An
earlier revision of this section said `rope` was the only one, and treated
keeping everything else permissive as an exit: replace `rope`, relicense
nothing, and the workspace could go permissive. `crates/similarity` closes
that exit for the handler layer." `design/high-level.md:483` was that earlier
revision, still standing: "Keeping our own crates MIT is deliberate: `rope` is
the only GPL input, so replacing it would make the whole workspace
permissively licensable without relicensing anything." §5 also names
`high-level.md` as the place the commitment "should be stated plainly", so the
two are not independent statements that happen to differ — one is the other's
designated summary.

**Resolution:** `high-level.md`'s licence section now says what §5 says: two
GPL inputs, `vendor/rope` and `crates/similarity`; every `crates/lang_*` GPL
through the second; the binary GPL-3.0-or-later as a project-level commitment;
and the permissive surface named as what it now is — `shared`, `driver`,
`heuristic_jump`, `measure_core` and each `measure_<lang>`, which is the seam
and the measurement program rather than the whole workspace. The superseded
exit is kept as history rather than deleted, because "replace rope and go
permissive" is a conclusion a reader can reach again from the remaining text
if nothing says why it stopped being available.

This trades nothing off: the manifests, `expected_licence` in
`crates/driver/tests/seam.rs`, and §5's table already agreed with each other
and disagreed with `high-level.md` alone.

**This edits a design document.** No code moved to meet it — every `license`
field is untouched and was already what §5's table assigns — and the same
commit adds `the_gpl_inputs_are_the_two_the_documents_name`, which compares
the three sources against each other rather than trusting any of them, and
fails if a third GPL input arrives in any one of them.

**Campaign:** c601eeec-b30f-479c-8a7d-49e19e4c166d
