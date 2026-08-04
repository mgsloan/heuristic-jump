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

## CHANGE-core-015 — rope-modifications.md#6-consequences-for-re-syncing — the allowlist is empty, because §4 already gave its one entry a unit

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

## CHANGE-core-009 — core.md#two-modes-collect-and-replay — `stage_us` is a second field a replay does not reproduce

**Contradiction:** the two modes say of the replayed side that
`heuristic_latency_us` "is therefore **the one field** in the record that a
replay does not reproduce exactly, and the one that needs a quiet machine to
mean anything" (§7, two modes). §7's own record, four hundred lines earlier,
tables `stage_us` as "wall clock per pipeline stage, handler-supplied | both"
and then says of it: "it is an *observation*, so it does not have to be
reproducible the way the rest of the record does."

**Resolution:** the two-modes bullet now names both fields and points at the
record's own sentence about them. This trades nothing off: the record section
is the one that defines the field, it already says exactly this, and no
consumer is affected — nothing branches on either field, and the *table*,
which is the artifact §7's command line requires to be byte-identical, holds
neither. The reading being corrected is only the count "one", which was true
of the record as §7 first described it and stopped being true when `stage_us`
was added beside it.

The code was not moved toward the document or the document toward the code:
`Trace::timed` takes a measured `Micros` from the handler, which is what the
record section describes, and the only code change in the same commit is a
test's mask — `tests/pipeline.rs` masked the one field the sentence named, so
a handler that reported a real stage timing would have failed a determinism
assertion that §7 never made about it.

**Campaign:** 18835da5-abcf-4eed-bc64-f52405edd53f

## CHANGE-core-006 — core.md#two-modes-collect-and-replay — a truth row is its own shape, because §8.2 forbids the other one

**Contradiction:** the two modes say "`collect` writes rows with the `lsp_*`
fields populated and the heuristic side null", i.e. that a truth row is §7's
record half-filled. §8.2 says of the wire types that they are read projections
and gives them no `Serialize`: "a projection written back out" is the thing it
exists to forbid, and `DefinitionResult` cannot be re-serialized at all. A
truth row holding `lsp_locations` as §7 spells them — a list of `uri:line`
labels — could not give replay the bytes the server sent, and replay reading
the oracle's answer with the same code the shim reads a live one with is the
property §6's predicate depends on.

**Resolution:** the section now says the two modes supply the record's two
halves and that only `replay` writes the record; a truth row is its own
smaller shape, and what survives the join is the content of the `lsp_*`
columns rather than their spelling in the intermediate file. This trades
nothing off: §8.2 already decided it, the byte-comparability claim is about a
*completed replay row* and is untouched, and the alternative reading is not
implementable without reversing §8.2.

No code changed in this commit — the implementation has always been this, and
the audit records it as a minor against `truth.rs` on the strength of the
sentence being amended here.

**Campaign:** 18835da5-abcf-4eed-bc64-f52405edd53f

## CHANGE-core-007 — core.md#83-the-wire-position-type-is-inert — `line` is readable, and §6 is why

**Contradiction:** §8.3 said "`WirePosition` has private fields and **no
accessors**". §6 says "**The predicate compares `(uri, line)`, and nothing
else.** Both sides carry a line: the shim's answer because `Location` does,
and the child's because that is what came off the wire. So it **reads
nothing**". The child's line arrives only inside a `WirePosition`, so under
§8.3-as-written the only route to it is `resolve`, which takes the target
document's text — and §6 is explicit that the classifier may not read, because
"divergence is classified when the child responds, seconds after the answer,
when the per-query read cache is long gone and the target document may never
have been open". The two sentences cannot both be honoured.

**Resolution:** §8.3 now says the fields are private, `character` has no
accessor, and `line` does. This trades nothing off, because inertness is a
claim about *offsets*: `character` is the number in the negotiated encoding
and remains unreachable without naming that encoding and the text, which is
the failure §3 exists to prevent. A row is in no encoding at all — every
encoding LSP 3.17 offers counts columns
(`reference/lsp-3.17/shim-relevant.md`, the position-encoding section), so a
row cannot be misread as an offset the way a column can. The alternative
reading — drop `line()` — is not implementable: it makes §6 read the target
document, which §6 forbids in the same paragraph that requires the
comparison.

**Written toward existing code, and said plainly:** `WirePosition::line` was
already there, with a doc comment making this argument. What this campaign
changed is the document and a test. The test is the reason the edit is not
simply moving the goalposts: `the_wire_position_has_exactly_one_door_per_unit`
in `crates/shared/tests/proto.rs` scans `impl WirePosition` and asserts its
public surface is exactly `line`, `resolve` and `encode`, so a `character()`
accessor added later fails rather than quietly widening the section again.

**Campaign:** 44773a93-738f-4dd6-8ca1-fa951465ac44

## CHANGE-core-008 — core.md#82-what-replaces-it-and-why-it-is-smaller-than-it-sounds — the third list, which §8.3 requires and §8.2 did not name

**Contradiction:** §8.2 gives two lists and says of them "**Nothing is ever
round-tripped**" and "the two lists have to stay disjoint". §8.3 requires
`WirePosition::encode(Offset, enc, &Rope)` as the outbound constructor of
the same type §8.2's Read table lists as arriving in `definition params`. So
`WirePosition` must carry both derives, and with it `WireRange`,
`WireLocation`, `PositionEncoding` and `TextDocumentSyncKind`. Two lists
cannot hold five types that are in both.

**Resolution:** §8.2 now names the third list and says what bounds it. This
trades nothing off because the two claims were never about the same thing:
"nothing is round-tripped" is about *messages* — a projection deserialized and
written back, losing the fields it did not model — and these are *values*. A
`WirePosition` that arrives is resolved to a `Offset` and dropped; one that
leaves was built by `encode` from an offset this system produced. No instance
makes the trip, so the forward-compatibility property the first bullet
protects is untouched. The alternative reading — that §8.2 forbids a value
type in both directions — makes §8.3 unimplementable.

**Written toward existing code, and said plainly:** `crates/shared/tests/proto.rs`
already asserted a `BOTH` list of exactly these five, and the audit records it
as a minor: "the third category is the code's invention asserted by a test as
though it were the section's". It is the section's now, with the same five and
the same bound, and no code changed.

**Campaign:** 44773a93-738f-4dd6-8ca1-fa951465ac44

## CHANGE-core-010 — deps.md#14-workspace-cargotoml-shape — the cargo-machete table is `[package.metadata]`, which is what the bullet's own precedent cites

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

## CHANGE-core-011 — deps.md#licensing-our-crates-are-mit-the-binary-is-gpl — high-level.md carried the position §5 records as superseded

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

## CHANGE-core-012 — core.md#adding-a-language — the template manifests omit two dependencies each, and neither is a choice

**Contradiction:** the template prints

> ```
> crates/lang_<x>/
>   Cargo.toml          shared, similarity, tree-sitter-<x>. Nothing else
> crates/measure_<x>/
>   Cargo.toml          measure_core, lang_<x>
> ```

and neither manifest compiles as printed. §1's trait is
`fn grammar(&self) -> tree_sitter::Language`, so a `lang_<x>` has to name the
tree-sitter *runtime* — the grammar crate supplies a `LANGUAGE` constant and
not that type. §7's four lines are
`measure_core::run(&Handler::new(), Cli::parse())` inside
`fn main() -> Result<(), shared::Error>`, so a `measure_<x>` has to name
`clap`, whose trait must be in scope for `parse()`, and `shared`, which is the
only route to the error type `main` returns.

**Resolution:** the block now lists all four on each side, and the paragraph
below it says which signature forces each. This trades nothing off — the two
existing manifests already declared exactly these, with a comment beside each
explaining why, and what was wrong was the document they claimed to be
verbatim copies of. It is the same failure `CHANGE-conformance-009` recorded
in §9's printed `main`, which omitted `shared` for the identical reason: a
manifest derived from a code block by reading it rather than compiling it.

Held by `adding_a_language_costs_the_template_and_one_line` in
`crates/driver/tests/seam.rs`, which compares both manifests against the
template in both directions, so a missing entry and an extra one each fail.

**Campaign:** c601eeec-b30f-479c-8a7d-49e19e4c166d

## CHANGE-core-013 — core.md#adding-a-language — "nothing else in the workspace changes" omits the four manifest lines cargo requires

**Contradiction:** "then one line in `heuristic_jump`. Nothing else in the
workspace changes. That is the whole cost" — against §14 of `deps.md`, which
requires that every member be listed in `[workspace] members` and that "every
dependency version lives in `[workspace.dependencies]`. Member crates never
name a version." Adding a language therefore changes the workspace manifest in
four places: two member entries, one `[workspace.dependencies]` entry for
`lang_<x>`, and `heuristic_jump`'s dependency line — before the one line of
Rust the section prices it at.

**Resolution:** the claim now reads "**No crate other than `heuristic_jump`
changes**", which is what it was actually asserting — the graph stays flat, no
existing crate learns a language's name — followed by the manifest bookkeeping
named explicitly and marked as bookkeeping. `measure_<x>` needs no
`[workspace.dependencies]` entry, since nothing depends on a binary.

This trades nothing off: the strong reading is unaffected and is now
mechanized, where the literal reading was false and unmechanizable. The test
asserts the sharper claim — that no member other than `heuristic_jump` and
`measure_<x>` names a `lang_*` in its manifest — which is stronger than the
sentence it replaces, since "nothing changes" says nothing about a workspace
where a second crate already knew.

**Campaign:** c601eeec-b30f-479c-8a7d-49e19e4c166d

## CHANGE-core-014 — core.md#9-workspace-layout — the four phase-2 crates are marked, because phase 1a is forbidden to create them

**Contradiction:** §9's tree prints eleven `crates/` entries as the workspace
layout, and four of them — `lang_python/`, `lang_typescript/`,
`measure_python/`, `measure_typescript/` — cannot exist in this phase.
`loops.md`'s decided question 10: "**The loop may never add a language.**
Enforced rather than discouraged: a new `crates/lang_*` is outside every
loop's owned paths, so the gate rejects the commit." `state/phase.toml` says
the same from the other side: "`crates/lang_rust/` is named here and not
globbed, deliberately … so `crates/lang_python/` stays outside every owned
path and the gate rejects it." So the tree, read as a phase-1a requirement,
asks for four commits the harness is built to refuse.

**Resolution:** the four carry `phase 2` in the tree's own note column, and a
paragraph below names the two rules that make them unbuildable here. Nothing
is removed and no requirement is weakened: the four are still the layout, and
`#adding-a-language` still prices them.

This trades nothing off, and the marking is what makes the section
mechanizable at all — `the_workspace_is_the_layout_section_9_prints` in
`crates/driver/tests/seam.rs` now compares the tree against `[workspace]
members` in both directions, which without the marking would have demanded
exactly the commit the gate rejects.

**Campaign:** c601eeec-b30f-479c-8a7d-49e19e4c166d

## CHANGE-core-016 — deps.md#licensing-our-crates-are-mit-the-binary-is-gpl — the "what that buys" paragraph still argued the retracted one-GPL-input position

**Contradiction:** §5's licensing subsection said, four paragraphs before its
own table, that "the portable and valuable part of this project is
`similarity` and the `lang_*` handlers … Marking those MIT means anyone who
supplies a different text layer can lift them, and it means that if `ropey`
ever wins the argument above, the whole workspace becomes permissively
licensable **without relicensing a line**."

The table immediately below marks both `GPL-3.0-or-later`
(`| crates/similarity | GPL-3.0-or-later (ported, see below) |`,
`| crates/lang_* | GPL-3.0-or-later, because they depend on similarity |`),
and the section retracts the argument in as many words two paragraphs later:
"**There are two GPL inputs, not one.** An earlier revision of this section
said `rope` was the only one, and treated keeping everything else permissive
as an exit … `crates/similarity` closes that exit for the handler layer. …
going permissive would now mean replacing two things instead of one."

So one paragraph of the section describes two crates as MIT and names an exit,
and two later paragraphs mark the same crates GPL and record that exit as
closed.

**Resolution:** the paragraph now states what the MIT marking buys under the
position the section actually holds — the seam and the measurement program are
the permissive surface, which is what the subsection's own last paragraph
already says — and keeps the retracted argument as history, marked as
retracted, pointing at the paragraph that settled it.

This trades nothing off: the two claims cannot both stand, the section itself
names one of them superseded, and CHANGE-core-011 already applied the same
retraction to `high-level.md`'s License section. The straggler was a second
copy of the sentence that revision was about, in the document that wrote it.

**No code moved with it.** The manifests were already correct — `expected_licence`
in `crates/driver/tests/seam.rs` has held them to the table since before this
campaign — and the same commit adds `the_permissive_surface_is_exactly_what_does_not_reach_similarity`,
which asserts the corrected claim positively rather than checking that the
wrong sentence is absent: the members that depend on `similarity` are exactly
`crates/lang_*`, and each of them is GPL. Planted the dependency on
`measure_core` and watched it fail.

**Campaign:** 20bbc1bf-03c5-4d3c-afda-a5c5791d47ce

## CHANGE-core-019 — core.md#two-modes-collect-and-replay — three subcommands, and enumeration is not `collect`'s first half

**Contradiction:** `#two-modes-collect-and-replay` said "`measure` therefore
has two subcommands, and only the first needs a server", and described
`collect` as "spawn the server, drive `didOpen` across the repository,
**enumerate identifiers**, ask the LSP, write `truth.jsonl`".

[`#the-command-line`](../../design/core.md#the-command-line), 270 lines further
down the same document, opens "**Three** subcommands, one per stage of
`data-collection.md`", prints `measure-<lang> enumerate --corpus <dir> ...`
first of the three, and gives it its own bullet: "**`enumerate`** parses each
repository, samples positions, writes `positions/<repo>.jsonl`."

`crates/measure_core/src/cli.rs:19` has the three-way `Command` enum, and its
`Collect` carries no limit and no seed — the two flags enumeration needs — so
the code cannot express the two-subcommand reading either.

**Resolution:** the three-subcommand reading, because it is the only one
`data-collection.md` allows and the disagreement is not really about how many
subcommands there are.

[`data-collection.md` §2](../../design/data-collection.md) is titled "Positions
are enumerated once per repository" and its first line is "**Not once per
server.** `positions/<name>.jsonl` is written first, and every server run
consumes the same file", because "if each server run enumerated its own
positions, two servers' answers could not be aligned, and the agreement /
divergence split that `core.md` §7 builds the whole per-server design on would
have nothing to join on". Enumeration inside `collect` *is* enumeration once
per server: `collect` takes `--server`, so its output would be a function of
which server it was collecting against. So the stale text was not a smaller
version of the same design, it was one that takes away the join
`#7-observability-and-the-corpus-scan` rests on.

This trades nothing off in the other direction either, because the section's
own argument survives the correction untouched: the split it exists to defend
is the *mode* split — a slow server-driven collection frozen once, against a
serverless replay that can be run every iteration — and that is still two, with
`enumerate` on `replay`'s side of it. Which is why the heading, and the "two
modes" framing under it, are unchanged: the section now says the modes are a
two-way split and the subcommands a three-way one, and that they are not the
same partition.

**No code moved with it, and none was read for it beyond confirming the enum
has three arms.** The resolution is decided entirely by two other design
documents, one of which is this one.

**Campaign:** 9110a409-f685-4569-ba82-fbf938928727
