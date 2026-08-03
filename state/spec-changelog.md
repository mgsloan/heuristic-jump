# Spec changelog

Class A spec fixes: an internal contradiction, an unresolvable section
reference, a type name that changed, a false claim about a dependency's API,
an example that does not compile. The loop fixes these itself and records
them here; they are reviewed in batch, after the fact
(`design/loops.md` section 6).

Anything that trades something off is Class B and belongs in
`state/decisions/`, not here.

**An entry is provisional until a human reads it** (`design/loops.md` §6).
Applied immediately so the loop never idles, and surfaced on the dashboard
for the next batch alongside the escalations. That scheduling is not
ceremony: rewriting the spec toward the code is the one gaming route on §7's
list that the audit cannot catch by construction, because moving the spec
removes the gap from the instrument that would have reported it.

Each entry quotes the contradiction and states the resolution, under a
heading the harness can find:

```
## CHANGE-<owner>-NNN — <section anchor> — <one line>
```

Newest last.

---

## CHANGE-bootstrap-001 — loops.md#6-spec-changes-what-the-loop-may-decide-alone — decision-record paths

Section 6 says an escalation is `state/decisions/NNN.md`; section 13 says
`state/decisions/<owner>-NNN.md` and argues for the prefix explicitly: "a
bare incrementing id is exactly the kind of thing that looks fine until two
sessions allocate `007` four seconds apart."

Resolved toward section 13, which is the one with the reasoning attached, and
section 6 updated to match. The code tag follows the file name and is now
`// DECISION-<owner>-NNN: provisional`. `grep -r DECISION-` still works as
the outstanding-provisional-choice report.

Recorded here rather than as a decision because there is nothing to trade: the
two sections cannot both be right, and only one of them says why.

## CHANGE-conformance-001 — core.md#9-workspace-layout — cargo does not hyphenate a binary target's name

**Contradiction:** `core.md` §9 said the binary crate is named `heuristic_jump`
"so the produced binary is `heuristic-jump` without a `[[bin]]` rename — the
same relationship Zed has between its `zed` crate and its `zed` binary", and
`deps.md` §14 restated it as "the artifact is `heuristic-jump` with no
`[[bin]]` rename". Cargo derives a binary target's name from the package name
verbatim; the hyphen-to-underscore mapping runs the other way and applies to
the *library* name, which is a Rust identifier. Checked rather than reasoned
about: a package named `bin_name` builds `target/debug/bin_name`. The Zed
analogy is what hides it — `zed`'s crate and binary are both `zed`, so nothing
about that case distinguishes "cargo hyphenates" from "cargo copies".

The artifact name itself is settled elsewhere and is not in question:
`deps.md` §11 pins `#[command(name = "heuristic-jump")]`, and every invocation
in these documents spells it with the hyphen.

**Resolution:** both sections now say the rename is required and why. The
package stays `heuristic_jump` — `state/phase.toml`'s crate row, the ownership
table, and `core.md` §9's own layout all name it that way, and a package
rename would move all three for no gain — and `crates/heuristic_jump/Cargo.toml`
carries a two-line `[[bin]]` block naming the artifact. This trades nothing
off: the alternative readings are a binary called `heuristic_jump`, which
contradicts `deps.md` §11, or a package called `heuristic-jump`, which
contradicts the ownership table. Only the rename satisfies both.

**Stated plainly, because it is the shape being watched for:** this campaign
edited a design document and wrote the code it describes. What makes it Class A
rather than a rewrite toward the code is that the document's claim is falsified
by cargo and not by anything in this repository, the falsifying experiment is
one command in an empty directory, and the corrected claim is *harder* to
satisfy than the original — it costs a manifest block that the false version
said was unnecessary. `crates/heuristic_jump/tests/binary_name.rs` asserts the
artifact's name, so the claim now fails a test rather than fading out of a
document if the rename is ever dropped.

**Campaign:** b59733c6-ebff-47a4-bccf-232abc532a07

## CHANGE-conformance-002 — rope-modifications.md#folding-vendorutil-in — the kept benchmark is a sixth `util` import site

**Contradiction:** §4 says "**Five import sites change**, across two files:
`chunk.rs:6`, `:76`, `:192`, `:825`, and `rope.rs:1733`", and its table puts
`RandomCharIter` in "a `#[cfg(test)]` module at the crate root". §7 and
`deps.md` §5 patch 4 both keep `benches/rope_benchmark.rs`, whose line 10 is
`use util::RandomCharIter;`. A bench is compiled as its own crate: it can see
neither `util` (not vendored) nor a `#[cfg(test)]` module of the library. With
five sites patched the benchmark does not build — checked, not reasoned about:
`cargo build -p rope --all-targets` fails with `unresolved import util`.

Upstream only gets away with it because rope dev-depends on
`util = { workspace = true, features = ["test-support"] }`, which patch 1
deletes.

**Resolution:** six sites across three files. `test_support` is a file,
`vendor/rope/src/test_support.rs`, and the bench opens with
`#[path = "../src/test_support.rs"] mod test_support;`. This trades nothing
off: one copy of the source, no new dependency (`rand` is already a
dev-dependency, and benches get dev-dependencies), and nothing added to rope's
public API — which the alternatives both cost. Making the module `pub` and
feature-gating it would put a test helper in the shipped API surface or force
a default feature; copying `RandomCharIter` into the bench would give the
crate two copies of a borrowed Apache-2.0 item to keep in step.

The document's table is updated in the same edit, since "`#[cfg(test)]` module
at the crate root" is what made the file-versus-inline choice look free.

**Campaign:** 4ba19af5-b041-4f2f-9d85-e5553eb14c57

## CHANGE-conformance-003 — core.md#snapshots-are-o1-to-take-and-are-parsed-before-a-handler-sees-one — the snapshot has to carry the document's URI

**Contradiction:** §2 prints `DocumentSnapshot` with the fields `text`,
`version`, `language_id` and a private `tree`, and §1 prints `Query` with
`doc`, `position`, `project`, `deadline`, `server` and `policy`. §1 also has a
handler return `Outcome::Committed { locations: Vec<Location> }`, and §8.4
settles that a `Location` is "constructed only through
`Location::at_node(uri, node)`". Nothing in either struct is a `DocumentUri`,
so a handler that resolves a local binding — the commonest answer there is,
and the whole of `resolution.md` §2's stage 1 — cannot name the file the
answer is in. The same gap shows up a second time in `resolution.md` §3, whose
`ProjectView::root_of(&self, uri: &DocumentUri)` is documented as "the root
containing a document, for scoping searches" and has no argument to be called
with.

**Resolution:** `SnapshotSeed` and `DocumentSnapshot` both carry
`pub uri: DocumentUri`, and §2 says why in a bullet beside the block. This
trades nothing off. The alternatives are all worse in a way that is easy to
check rather than to argue about: putting the URI on `Query` instead splits a
document's identity from the document, so `measure_core` — which builds a
snapshot per recorded position and no `Query` at all — would carry it
separately; giving handlers a second `Location` constructor that takes no URI
would give away §8.4's single-constructor property, which is the thing making
row and range unable to drift apart; and deriving it from `ProjectView` needs
a `ProjectPath`, which a handler only has for files it looked *up*, never for
the one it was handed. It is also not an encoding leak or a scope widening: a
URI is not a position, and the document was already in scope by construction.

**Stated plainly, because it is the shape being watched for:** this campaign
edited a design document and wrote the code it describes, in the same commit.
What makes it Class A is that the correction is forced by the document's own
other claims rather than by anything convenient about the implementation — the
seam as printed cannot return the answer §1 says handlers return — and that
the fix adds an obligation to the driver (it must know the URI at dispatch)
rather than removing one.

**Campaign:** e3b8dbf4-56aa-48fc-9a4d-4018d7464f4d

## CHANGE-conformance-004 — core.md#vocabulary-types — `Location`'s printed fields lose their `pub`, per the ruling on conformance-004

**Contradiction:** §1 and §8.4 both print

```rust
pub struct Location {
    pub uri: DocumentUri,
    pub range: ByteRange,
    pub line: LineIndex,
}
```

while §1's doc comment on the same block says "Constructed only via
`Location::at_node`, so the two cannot disagree", and §8.4's prose says
"`Location` is therefore constructed only through `Location::at_node(uri,
node)`, which derives both from the same node, so the two cannot drift apart
by hand." With three public fields a struct literal is available to every
crate in the workspace, so both cannot hold.

**Resolution:** the three `pub`s are removed from both code blocks and §1's
doc comment names the accessors. This is not a reading this campaign chose:
`state/decisions/conformance-004.md` was answered `accepted` on
2026-08-03T05:13:34+00:00, and the ruling says in as many words that "the code
blocks are what is wrong, and removing those three pubs is the Class A
follow-up". It trades nothing off because the prose it aligns with was already
the operative claim — §8.4's argument for carrying `line` at all depends on
row and range being derived together — and because no consumer needs a
literal: the three known ones (§8.4's `WireLocation` conversion, §6's
`(uri, line)` predicate, §7's `heuristic_locations`) are all reads.

**Not a spec-toward-code edit, and here is the check for it:** the code already
took this reading in campaign e3b8dbf4 and tagged it provisional at
`Location::at_node`; this commit removes that tag and changes nothing about
the type. The document moved, the code did not.

**Campaign:** bc8f02bb-1cb1-48d7-8814-a22f8a2b8481
