# Spec changelog — core, worker 2

One entry per Class A change, in the shape the loop prompt fixes. Newest last.

## CHANGE-core-029 — core.md#the-dependency-graph — §9's edges are reconciled with the manifests, and "not yet declared" becomes a named set rather than a standing excuse

**Contradiction:** three of §9's edge claims disagree with the manifests they
describe, in both directions.

§9's prose list for `shared` reads

> Its own dependencies are `serde`, `serde_json`, `url`, `rope`,
> `tree-sitter`, `ignore` (for `ProjectView`'s walk), `rayon` (for
> `ProjectView::scan`, which executes on the pool it is handed at
> construction — `resolution.md` §3) […] This list is the authoritative one

`crates/shared/Cargo.toml` declares no `rayon`, `ProjectView::new` takes
`(files, deadline, grammar)` and no pool (`crates/shared/src/project.rs:426`),
and `scan` is a sequential `for` loop over candidates (`:584`) whose own
docstring says so.

§9's graph draws

> `driver  <-- crossbeam-channel, rayon, rustc-hash, tracing`

while `crates/driver/Cargo.toml` declares `crossbeam-channel`, `lru`,
`notify`, `serde_json`, `shared` and `tracing` — so the arrow names two crates
the manifest does not have and omits two it does.

§9's prose for `heuristic_jump` gives

> `heuristic_jump` depends on `driver` and every `lang_*`, plus `clap`,
> `tracing-subscriber`, and `shared` for the error type its `main` returns.

while its manifest also declared `tracing`, which no line in the crate used.

The reason none of this was caught is in the one test that reads any of it,
`shared_declares_only_the_dependencies_section_9_lists`, whose own comment
states the rule it was applying: "A subset rather than an equality, because §9
lists `rayon` […] a listed crate not yet declared is the intended state". That
is `deps.md` §14's "each arrives with its first user" and it is right — but as
an unbounded subset it forgives an absence for *any* reason, so `shared` could
have dropped `ignore` and nothing would have failed.

**Resolution:** §9 now names the chosen-but-undeclared set explicitly and
completely — `rayon` in `shared`, `rayon` and `rustc-hash` in `driver` — with
the reason for each, and the `shared` bullet's parenthetical points at that
list instead of asserting a present tense the code does not have. The `driver`
arrow names `lru`, `notify` and `serde_json`. `heuristic_jump`'s unused
`tracing` is deleted rather than added to §9, because §14's rule is that a
dependency arrives with its first user and this one had none.

This reading trades nothing off because it changes no dependency *choice*:
`rayon` stays chosen for both crates and `resolution.md` §3's fan-out onto a
bounded pool stays the settled arrangement. What changes is only the tense —
§9 stops describing that arrangement as though it were built. Parallelising
`scan` is an optimisation, and `CLAUDE.md` withholds those "until the corpus
harness shows the change is worth it *and* there is a benchmark", so building
it to match the document would have traded a rule off; saying which entries
have not arrived trades nothing.

The set being *named* is what makes it mechanical: `seam.rs` parses those
bullets and asserts `declared == named \ deferred` for all three crates, in
both directions. A dependency deleted from a manifest now fails, and a
deferred one that acquires its first user fails until the same commit removes
it from §9's list. Four plants, four correct failures: an entry named and
neither declared nor deferred; `rayon` added to `shared` with §9 untouched;
the bullets reworded so the parse silently returns empty; and
`heuristic_jump`'s `tracing` put back, which is the state the audit found.

Both edits are to the document alone except for the deleted `tracing` line and
a stale comment in `project.rs` — no code changed behaviour under this entry.

**Campaign:** 26e3bb3c-2937-495c-afea-2f1d0ae858f3

## CHANGE-core-030 — core.md#the-dependency-graph — "ours" is used in two senses one paragraph apart, and only one of them is meant

**Contradiction:** §9's first edge bullet reads

> * **`shared` depends on nothing of ours.**

and its own list, three lines later in the same bullet, reads

> Its own dependencies are `serde`, `serde_json`, `url`, `rope`,
> `tree-sitter`, `ignore` […]

`rope` is `vendor/rope`, a workspace member of this repository listed in the
root `Cargo.toml` beside `crates/shared`. So `shared` depends on something of
ours by the reading that counts workspace members, and on nothing of ours by
the reading that counts `crates/` — and the section uses the same word again,
two bullets down, for a claim that has to be checked rather than read:
`measure_core` "depends on `shared` and nothing else of ours".

That second one is where it bit. Mechanising it
(`the_measurement_crates_have_the_edges_section_9_gives_them`) required
choosing a sense of "ours", and the two give different tests: the strict one
fails a `measure_core` that declares `rope` directly, the loose one does not.
Nothing in the section said which.

**Resolution:** the bullet now states the sense §9 uses — "no crate of ours in
`crates/`" — and says in as many words that the vendored text crates are
neither an exception to it nor covered by it. That is the reading the rest of
the document already takes: `deps.md` §14's tree separates `crates/` from
`vendor/` so provenance and licensing stay obvious, and §9's own graph draws
`rope` on the same arrow as `tree-sitter` and `serde`. Nothing is traded,
because no edge changes and no dependency is added or removed — the word stops
meaning two things.

The seam test reads the `measure_core` claim **strictly**, quantifying over
`vendor/` as well, and §9 now says so rather than leaving the test to imply it.
The reason is given there and is specific rather than a preference for
strictness: the text vocabulary reaches `measure_core` through `shared`'s
re-export, which `the_text_vocabulary_is_nameable_through_shared_and_defined_in_rope`
already asserts, so a direct `rope` edge would be a divergence and not another
spelling of the same edge.

**Campaign:** 26e3bb3c-2937-495c-afea-2f1d0ae858f3

## CHANGE-core-032 — core.md#the-trait — the same false claim was corrected in one section and left standing in another

**Contradiction:** §1's `LanguageId` bullet ends

> Unknown languages fail to resolve at the boundary rather than travelling
> inward as a string that matches nothing, and lookup becomes pointer
> comparison.

`crates/shared/src/vocabulary.rs:48` says the opposite, in a comment written
deliberately to say it:

> Comparison is `str` equality on the interned text and deliberately not
> pointer identity: two crates may each write `"rust"` into their own
> `&'static str`, and an id that compared unequal to itself across a crate
> boundary would fail to resolve a handler that had declared it.

The code is right and the reason is decisive rather than a preference. The
registry resolves an incoming `languageId` against ids a `lang_*` crate
declared; those two `"rust"`s are literals in different crates. Under pointer
identity they differ, the handler is not found, and nothing reports an error —
an unresolved id is exactly what an unsupported language looks like.

What makes this worth an entry rather than a one-word fix is where it had
already been fixed. Commit `ffbd71b` ("LanguageId compares by text, not by
address", campaign 2c129b10) corrected this claim under
`core.md#vocabulary-types`, argued it in the same words, and added
`a_language_id_compares_by_text_and_not_by_address` — which leaks a
runtime-built `"rust"` so the compiler merging two equal literals cannot
answer the question for it. It corrected the doc comment in `vocabulary.rs`
and left §1's own sentence untouched, because the two sections are audited
separately and §1 was not the one being worked.

**Resolution:** §1 now states the `str`-equality rule, gives the
different-crates argument for why pointer identity would be wrong rather than
merely unimplemented, and names the test. Nothing is traded: no code changes,
the claim already holds, and the correction is one already made elsewhere in
the same document under a campaign that argued it.

**Campaign:** 26e3bb3c-2937-495c-afea-2f1d0ae858f3
