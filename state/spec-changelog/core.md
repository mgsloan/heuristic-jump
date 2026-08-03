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
