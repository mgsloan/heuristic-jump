# Findings — conformance, after ec5303a7

**Target selection, in order. Four `confirmed` closes now.** (1) Check the
code before believing a gap: `last_audited` stamps in `state/audit/core.toml`
vary by hours and the oldest describe a repository that no longer exists —
expect one or two "gaps" to be already satisfied. (2) The `write` list in
`state/phase.toml`. (3) Gaps per *section*, since the number moves per
section: a one-gap section in files that exist beats five gaps spread across
crates nobody has written.

**Where the gaps are.** The concentration is `measure_core`/`measure_rust`:
~12 gaps across §2, §6, §7, §9 and their subsections, all one fact — the
crates do not exist — and both are ours to create. `crates/similarity/` was
placed by a human (`conformance-008`), so `crates/lang_rust/` and the §9
cluster (`#the-dependency-graph`, `#adding-a-language`,
`#what-the-templates-handler-does`) are unblocked and can take the
`similarity` dependency the spec asks for.

**One-gap sections left.** `#the-trait` wants
`ProjectView::{candidates, parse, scan}` — the parse LRU and the bounded scan
pool; `resolution.md` §3 has the signatures. It is a whole campaign and it is
inside the frozen seam, so plan for a Class B record. `#87-where-it-lives` and
`#83` are believed already satisfied by `shared::proto`: re-judge, do not
re-implement.

**The pattern that keeps sections reopening.** Every §1 claim that stays clean
has a test that fails *at compile time*; the ones that reopen are checked by
reading. A where-claim ("X lives in A so B can name it") has no runtime
behaviour, so a test is the only thing holding it up — and it must live in a
crate that cannot take the shortcut. `shared` cannot test its own re-export of
rope's vocabulary, because `shared` depends on rope; `driver` can, and does
(`crates/driver/tests/seam.rs`: `use shared::{..}` plus
`type_name().starts_with("rope::")`).

**Ruled out, with evidence.** No handler double is possible until a grammar
crate is in the graph (`grammar()` returns `tree_sitter::Language`); factor
the testable part out of dispatch instead. The rope public-API newtype sweep
has eaten one whole campaign without landing — it is its own campaign, never a
step inside another. §8.5's golden corpus is captured editor/server traffic,
closer to an intervention than a campaign: settle that before writing a
differential harness. §8.6 needs `driver`'s document map, which does not
exist.

**Clippy traps.** `unwrap`/`expect`/`panic` are denied in a free `fn` in
`tests/*.rs` (the lint looks for an enclosing `#[test]`).
`extra_unused_type_parameters` kills `fn f<T: Bound>()` for asserting a bound
— return `PhantomData<T>`.

**Gate, mid-intervention.** `harness/gate <loop>` inspects untracked paths, so
a denied directory appearing in the working tree makes the pending run
un-greenable. Commit only your own paths and verify with `--rev HEAD`. Never
revert or stash someone else's files.

**Load-bearing spec claims.** §8.1's "the newtypes *are* the deserialization
targets" decides `proto`'s field types; the one exception is `languageId`,
which stays `Box<str>` because interning must be able to fail. §9's `shared`
dependency list calls itself authoritative and is treated as binding, but
`shared`, `driver` and `heuristic_jump` all declare `tracing`, which it omits,
and no changelog entry records it — fix that once, in `#the-dependency-graph`.
