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

## CHANGE-conformance-005 — core.md#6-the-agreement-predicate — normalisation stops at a row, because the predicate may not read

**Contradiction:** §6 says the two sides normalise into byte space —

> "All shapes collapse to a set of `Location` — `(DocumentUri, ByteRange)` —
> taking `targetSelectionRange` for links."

— and three paragraphs later says the predicate may not read the documents it
is comparing:

> "So it **reads nothing** — which matters, because divergence is classified
> when the child responds, seconds after the answer, when the per-query read
> cache is long gone and the target document may never have been open."

Both cannot hold. The child's side arrives as `WireRange`, whose `character`
is in the negotiated position encoding (§8.3); turning that into the
`ByteRange` a `Location` carries requires the target document's text, and §3's
whole argument is that guessing it instead is the highest-risk correctness
failure in the shim. `Location` is also unconstructible without a
`tree_sitter::Node` since the ruling on `conformance-004`, and the classifier
has no node — it has a JSON response.

**Resolution:** both sides normalise to a set of `DefinitionSite`, which is
`(DocumentUri, LineIndex)` — exactly the pair the very next paragraph says the
predicate compares, and nothing else. This trades nothing off because the
range was never an input: §6 already says so in as many words —

> "`Location.range` is unaffected and still earns its place — it is the jump
> target on the wire. It simply is not an input to agreement."

— and the section's own argument against comparing columns applies with more
force to a conversion that would have to read a file to produce a column it
then discards. The wire's `line` needs no encoding to interpret, because every
encoding LSP offers counts *columns*; that is why the row survives where the
range does not.

**Not a spec-toward-code edit:** the code did not exist when this was written.
The contradiction is between two claims in §6, both older than any of it, and
the resolution is decided by which of the two is load-bearing elsewhere —
"reads nothing" is, since §7 classifies agreement when the child's response
arrives rather than while the query is open.

**Campaign:** 5314b0c3-326e-415a-9eb6-1d9e7fad4378

## CHANGE-conformance-006 — core.md#6-the-agreement-predicate — the table's missing row

**Contradiction:** §6's pairwise table has a row for one side being empty but
not the other:

> "| Child answered null or empty, shim committed | differs | `unrelated` |"

while "Both sides are sets" says where a severity comes from:

> "`severity` is classified from the shim's **top-ranked** location whenever
> `agreement` is `mismatch`, since that is where a user who trusts the
> ordering looks first, and is undefined otherwise."

The reverse of the first row — the shim committed no location and the child
answered — is reachable (an `Outcome::Committed` carries a `Vec<Location>`
that nothing constrains to be non-empty) and is a `mismatch`, but it has no
top-ranked location, so the second quote does not tell you what `severity` to
write. The table has no row for it either.

**Resolution:** the row is added and takes `unrelated`. Two reasons it trades
nothing off: it is the pessimistic class, so it cannot overstate precision the
way a milder default would, and it is symmetric with the row directly above,
which is the identical situation with the sides exchanged. A fourth class was
the alternative and was rejected — `high-level.md` attaches a budget to each
class, and adding one that means "the shim sent the user nowhere" would put a
non-jump into a table of wrong jumps.

**This campaign edited §6 and wrote the code it describes, and says so here
because that is the shape being watched for.** The check is the direction of
travel: this row's answer is asserted in
`crates/shared/tests/agreement.rs::both_empty_is_a_match_and_one_sided_emptiness_is_not`,
and it was written from the resolution above rather than the resolution being
written from it. A human reading this should still treat the `unrelated`
choice as provisional; it is the one line here with no prior claim behind it.

**Campaign:** 5314b0c3-326e-415a-9eb6-1d9e7fad4378

## CHANGE-conformance-007 — core.md#the-command-line — `servers.toml` is at the code repository's root, not the corpus's

**Contradiction:** `core.md` §7's command line says

> "**`collect`** drives the server named in the corpus root's `servers.toml`,
> which carries its command and pinned version."

while `data-collection.md` §0 puts the same file somewhere else, and says why:

> "`servers.toml` … lives at the root of the *code* repository rather than
> here. What the corpus holds is the several hundred megabytes of installed
> binaries it points at; which servers were chosen is a decision, and it is
> versioned beside the code that is scored against them."

`external-dependencies.md` §1 agrees with `data-collection.md`, prints the
file at the repository root in its layout, and adds the reason that settles
it: "`servers.toml` is not in any loop's write list … this manifest names the
oracle a language loop is measured against, and a loop that could edit it
could choose its own examiner." A file in the corpus root is outside every
loop's write list too, so that alone does not decide it — but the corpus root
is also outside the repository's history, and a decision that is not in the
history is not reviewable.

**Resolution:** §7 now says the file is at the root of the code repository and
names the two documents that own the placement. Two against one, the two carry
the argument, and the third had no argument attached — so this trades nothing
off. `measure_core` resolves it from its own manifest directory rather than
from `--corpus`, which is what makes the placement observable in code.

**Campaign:** ff3e1a40-5639-4c57-ac81-66ea1144762f

## CHANGE-conformance-008 — core.md#82-what-replaces-it-and-why-it-is-smaller-than-it-sounds — the Construct list gains `measure_core`'s outgoing half

**Contradiction:** not a contradiction — an omission that becomes one the
moment `measure_core` exists. §8.2 says

> "**Only a small set is ever constructed.** Definition responses, error
> responses, `window/showMessage`, `window/showMessageRequest`,
> `window/showDocument`, and — in standalone — one `InitializeResult`."

and §7 says the corpus scan is a "plain LSP client, no editor" that "spawns a
fresh language server per repository, opens documents … asks both sides". A
plain LSP client constructs `initialize`, `didOpen`, `didClose` and definition
requests. Under §8.2 as written those are Read projections, and §8.2's own
rule — "a field we did not model cannot be lost, because nothing writes it
back" — forbids giving a Read projection a `Serialize`.

**Resolution:** two rows are added to the Construct table for `measure_core`'s
request and notification envelopes and its four params types, together with
the reading that makes both halves true at once: the shim reads these because
it sits between an editor and a server, and `measure_core` writes them because
it *is* the client. They are separate types with `Serialize` only, which is
the split `StandaloneInitializeResult` already makes against the read
`InitializeResult`, so the Read and Construct lists stay disjoint and the
no-round-trip property is untouched. `shared::proto` rather than
`measure_core` follows from §8.7.

**This campaign edited §8.2 and wrote the code it describes, and says so here
because that is the shape being watched for.** What limits it: the edit adds
rows to an inventory and changes no rule, the disjointness rule it is
constrained by is the one already enforced mechanically by
`crates/shared/tests/proto.rs::read_projections_are_never_serialized`, and
that test was not touched except to list the new names. The alternative
considered and rejected was keeping the outgoing types in `measure_core`,
which needs no spec edit and violates §8.7 instead.

**Campaign:** ff3e1a40-5639-4c57-ac81-66ea1144762f
