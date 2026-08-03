# Findings — conformance, after 5314b0c3

**Target selection, in order. Five `confirmed` closes now.** (1) Check the
code before believing a gap — this has paid off three campaigns running. Right
now `#vocabulary-types`, `#87-where-it-lives`, `#84` and `#85` are all stale:
`shared` re-exports all seven rope newtypes and `shared::proto` is 800+ lines
with `WireLocation`, `WireRange`, `WireLocationLink` and the untagged
`DefinitionResult`. Those four are a re-judge, not work. (2) The `write` list
in `state/phase.toml`. (3) Gaps per *section*, since the number moves per
section.

**Where the gaps are.** ~12 across §2, §6.1, §7, §9 are one fact:
`measure_core`/`measure_rust` do not exist. Both are ours. Four one-gap
sections (`#one-measurement-library`, `#where-the-corpus-lives`,
`#the-command-line`, `#the-table-is-not-enough`) go clean together and only
together, which makes "create `measure_core`" the highest-yield campaign left
— and it is large. `crates/lang_rust/` is unblocked (`similarity` was placed
by a human, `conformance-008`) and would also close §9's `#adding-a-language`
/ `#what-the-templates-handler-does` cluster.

**The wall that keeps costing campaigns: no test can construct a `Location`
or a `DocumentSnapshot`.** `Location::at_node` and `SnapshotSeed::fresh` both
need a `tree_sitter::Language`, and no grammar crate is in the graph. Three
campaigns have hit it. Do not solve it with a `tree-sitter-*` dev-dependency —
that is Class B twice over (dependency set, and grammars pinned to Zed's
revisions) and puts a grammar on the one crate §9 keeps language-free. Two
things that do work: ask whether the function needs the *type* or only a
projection of it (§6 needed `(uri, line)`, never a range — hence
`DefinitionSite`), or factor the testable part out of the caller. The real fix
is `lang_rust`, which brings a grammar legitimately.

**The pattern that keeps sections reopening.** Claims that stay clean have a
test that fails *at compile time*; the ones that reopen were checked by
reading. A where-claim has no runtime behaviour, so a test is the only thing
holding it up, and it must live in a crate that cannot take the shortcut —
`driver/tests/seam.rs` tests `shared`'s re-exports because `driver` may not
name `rope`.

**Spec method, learned in §6.** Where two claims in one section conflict,
check whether the section already says which is load-bearing before inventing
a resolution — §6 said "`Location.range` … is simply not an input to
agreement" four paragraphs after contradicting itself. That makes the fix
Class A rather than a judgement call.

**Ruled out, with evidence.** The rope public-API newtype sweep has eaten a
whole campaign without landing; it is its own campaign, never a step inside
another. §8.5's golden corpus is captured editor/server traffic, closer to an
intervention than a campaign. §8.6 needs `driver`'s document map, which does
not exist. `#9-workspace-layout` can never go clean under this loop —
`lang_python`/`lang_typescript` are outside every owned path by design.

**Clippy traps.** `serde_json::Value` is a `disallowed_types` entry, so a test
helper that *returns* one fails the gate though inline `json!(…)` is fine; use
a `macro_rules!` helper. `let _ = (a, b);` fails `let_underscore_drop`.
`unwrap`/`expect`/`panic` are denied in a free `fn` in `tests/*.rs` (the lint
wants an enclosing `#[test]`). `extra_unused_type_parameters` kills
`fn f<T: Bound>()` — return `PhantomData<T>`.

**Gate, mid-intervention.** It inspects untracked paths, so a denied directory
appearing in the working tree makes the run un-greenable. Commit only your own
paths; never revert or stash someone else's files.

**Load-bearing spec claims.** §8.1's "the newtypes *are* the deserialization
targets" decides `proto`'s field types, except `languageId`, which stays
`Box<str>` because interning must fail. §9's `shared` dependency list calls
itself authoritative and is treated as binding, but `shared`, `driver` and
`heuristic_jump` all declare `tracing`, which it omits, with no changelog
entry — fix that once, inside `#the-dependency-graph`.
