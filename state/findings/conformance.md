# Findings — conformance, after 25be160b

**Target selection, in order. This has now picked three `confirmed` closes.**
(1) `last_audited` in `state/audit/core.toml`: the stamps vary by hours and the
oldest ones describe a repository that no longer exists — this campaign's §8
gaps claimed `proto` did not exist when it had for two campaigns, so **check
the code before believing a gap**, and expect one or two sections to be clean
already. (2) The `write` list in `state/phase.toml`. (3) Gaps per *section*,
since the number moves per section: a one-gap section in one existing file is
worth more than five gaps spread across crates that do not exist.

**Where the gaps are now.** `crates/similarity/` was placed by a human during
this campaign (`conformance-008`, accepted), so the §9 cluster —
`#the-dependency-graph`, `#adding-a-language`,
`#what-the-templates-handler-does` — is **no longer blocked**, and
`crates/lang_rust/` can be written with the `similarity` dependency the spec
asks for rather than the tagged omission the record allowed as a holding
position. That is the biggest change to the reachable set since this loop
started, and it is the obvious next target. The remaining concentration is
`measure_core`/`measure_rust`: ~12 gaps across §6, §7 and their subsections,
all one fact — the crates do not exist — and both are ours to create.

**§8 after this campaign.** `shared::proto` holds the whole §8.2 table.
`#83` and `#87` should be re-judged rather than re-implemented. The two real
remainders: §8.5 wants a *golden corpus* of captured editor/server traffic
plus an `lsp-types` dev-dependency oracle — captured traffic is closer to an
intervention than a campaign, so settle that before writing a differential
harness — and §8.6's untrusted-document state needs `driver`'s document map,
which does not exist.

**Ruled out, with evidence.** No handler double is possible in `crates/` until
a grammar crate is in the graph (`grammar()` returns a
`tree_sitter::Language`); factor the testable part out of dispatch instead.
The rope public-API newtype sweep has already eaten one whole campaign without
landing — it is its own campaign, never a step inside another. Clippy denies
`unwrap`/`expect`/`panic` in a free `fn` in `tests/*.rs` (it looks for an
enclosing `#[test]`), and `extra_unused_type_parameters` kills the
`fn f<T: Bound>()` trick for asserting a trait bound — return `PhantomData<T>`.

**The gate, when a human is mid-intervention.** `harness/gate <loop>` inspects
untracked paths too, so a denied directory appearing in the working tree makes
the pending-tree run un-greenable from inside the loop. Commit only your own
paths and verify with `--rev HEAD`, which scopes the check to the commit. Do
not revert or stash someone else's files.

**Load-bearing spec claims.** §8.1's "the newtypes *are* the deserialization
targets" decides `proto`'s field types; the one deliberate exception is
`languageId`, which stays a `Box<str>` because interning must be able to fail.
§8.2's read-only rule is now a source-scanning test (`tests/proto.rs`) with
three lists — read, constructed, and the five value types that travel both
ways; that third list is where an exception would hide, so it is asserted
exactly. §9's `shared` dependency list calls itself authoritative and is
treated as binding, but `shared`, `driver` and `heuristic_jump` all declare
`tracing`, which it omits, and no changelog entry records it — fix that once,
in `#the-dependency-graph`, not three times.
