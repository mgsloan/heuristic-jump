# Findings — conformance, after bc8f02bb

**Where the gaps are, and why.** Not spread evenly: ~20 of the 42 sit in §6,
§7 and §9 and are all one fact — `measure_core`, `measure_rust`, `lang_rust`
and `similarity` do not exist on disk. Those sections cannot be closed by
writing shared code; they need crates. Three of the four are ours to create.
**`similarity` is denied to every loop in every phase**, and §9 makes every
`lang_*` depend on it, so `#the-dependency-graph` is unreachable from here and
`#adding-a-language` is reachable only if the template may omit it. Filed as
`conformance-008` (open). The rest of the gaps are §8's wire types, which are
one crate file's work and reachable now.

**Target selection, in order.** (1) `last_audited` in `state/audit/core.toml` —
stamps at `03:47:22` describe a repository that no longer exists and overstate
their sections. (2) The `write` list in `state/phase.toml` — a section whose
gap bottoms out in `crates/similarity/` or `crates/lang_python/` cannot move.
(3) Gap count, and whether the gaps share a file. Two campaigns have now been
picked this way and both closed `confirmed`.

**Ruled out, with evidence.** `crates/` has **no handler double and cannot have
one** — `grammar()` returns a `tree_sitter::Language` and `Query` needs a
`DocumentSnapshot`, so anything testable about dispatch must be factored out of
it (`hard_cap` is the worked example). Do not plan a test that needs a fake
handler until a grammar crate is in the graph. The rope public-API newtype
sweep (`#vendoring-the-zed-crates`) has already consumed one campaign's whole
budget without landing; treat it as its own campaign, not as a step in another.
The lint set fights integration-test helpers: `unwrap`/`expect`/`panic` are
denied in a free `fn` in `tests/*.rs` because clippy looks for an enclosing
`#[test]`.

**Load-bearing spec claims — the ones that decide code.** §8.1's "the newtypes
*are* the deserialization targets" is the reason `proto`'s projections name
`DocumentUri`/`DocumentVersion`/`EditorRequestId` as field types; a conversion
layer reappearing is the failure it exists to prevent, and
`tests/vocabulary.rs` fails to compile if one does. §9's `shared` dependency
list calls itself authoritative and is treated as binding (it is why `Bias` is
a vendor patch rather than a `sum_tree` dep) — but `shared` and `driver` both
declare `tracing`, which is not on it, and no changelog entry records that. §3's
"no encoding crosses the seam" and §8.4's single-constructor rule are both now
enforced by privacy rather than by prose.

**Next campaign.** §8.2's ~30 read projections, in `shared::proto`. One file, no
missing crates, and it is the entry condition for five sections
(`#8-protocol-types`, `#82`, `#85`, `#86`, `#87`) plus the second half of
`#81`. Check `#83` and `#87` first: their gaps were written before `proto.rs`
existed and may already be satisfied. Do **not** add `Serialize` to an incoming
projection — §8.2 makes their read-only-ness a checkable property.
