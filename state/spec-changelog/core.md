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

## CHANGE-core-005 — core.md#84-location-is-byte-based-and-this-fixes-a-real-inconsistency — the conversion does not read the file the query came from

**Contradiction:** §8.4 states without exception that "the conversion
**re-reads the target file**, once per location, and the honest price is a
syscall and a UTF-8 validation", and builds the paragraph after it on that:
"the handler's read and the conversion's read are two reads of the same path,
so a file edited between them yields offsets that are stale and *still in
range*". The same section says a page earlier that the conversion is placed in
the worker because "the target is frequently a file the editor never opened" —
which concedes that frequently it is not, and for that case there is no second
read to be stale against. A definition in the file the cursor is in is the
most ordinary answer this tool gives, and it is the case the universal
statement gets wrong.

**Resolution:** the section now names the exception and says why it is not the
thing `conformance-005` refused: when the target is the query's own document
the conversion encodes against the `DocumentSnapshot` it was already handed,
which is not a cache — nothing is stored, nothing is keyed, and nothing
outlives the query — but the query declining to go and find text it is
holding. The stale-offsets paragraph is scoped to "a target the editor does
not have open", which is where the hazard it describes actually lives.

This trades nothing off because it removes no claim: the re-read, its price,
the carried row and `EncodingError::LineDisagreesWithRange` all stand exactly
as they were for the case they were written about. What changes is that the
sentence no longer says something about the open-document case that is not
true of it.

**The code this describes was not touched in this campaign.** The short
circuit is `dispatch::target_text`'s first branch and predates it. What this
campaign added beside the edit is
`a_target_in_the_query_s_own_document_is_encoded_without_reading_it`, which
deletes the document's file from disk and asserts the conversion still
succeeds — so the sentence is now a checked claim rather than prose, and the
next revision cannot quietly reintroduce a read.

**Campaign:** 2c129b10-41f7-4292-a1f5-4e31ed08b7ea
