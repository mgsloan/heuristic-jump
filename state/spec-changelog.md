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

## CHANGE-conformance-009 — core.md#the-dependency-graph — the printed `main` names a type that does not exist and passes one `driver` may not see

**Contradiction:** §9 prints the binary's `main` as

> ```rust
> let registry = HandlerRegistry::new(vec![ … ]);
> driver::run(registry, Cli::parse())
> ```

against two things it says elsewhere. The registry type is `Registry`
(`core.md` §1: "the registry resolves a `languageId` or a file extension to a
handler"; there is no `HandlerRegistry` in any document but this snippet). And
`Cli` is `clap`'s output, where the same bullet says "argument parsing and log
setup live here rather than in `driver`", `deps.md` §11 puts `clap` in
`heuristic_jump` alone, and `shim.md` §13 annotates `driver`'s `config.rs` with
"(clap lives in `heuristic_jump`)" — so `driver::run` cannot name the type its
own printed signature takes.

**Resolution:** the snippet builds a `Registry`, resolves `cli` into
`driver::Config` in `heuristic_jump`, and calls `driver::run(registry,
config)`. The bullet's edge list gains `shared`, which its own printed
`-> Result<(), shared::Error>` has always required and which `measure_rust`
already carries for the identical reason.

Nothing is traded: `Config` is the type §9's sibling section and `shim.md` §13
already designate for exactly this crossing ("what the binary resolved from its
argv, in the vocabulary `driver` thinks in"), and every other reading either
puts `clap` in `driver` — contradicting three sections — or leaves `main`
uncompilable, which is the state that produced the gap.

**This campaign edited §9 and wrote the code it describes, and says so here
because that is the shape being watched for.** What limits it: the claim the
gap is about — `heuristic_jump` depends on every `lang_*` and is the single
place the language list is enumerated — is not touched by the edit and is
*harder* to satisfy after it, since
`crates/driver/tests/seam.rs::the_language_list_is_enumerated_in_heuristic_jump`
now fails if a `crates/lang_*` member is missing from either the manifest or
the registry literal. The edit corrects a name and an argument in a snippet;
the loosening reading — dropping the snippet, or dropping the claim that
`heuristic_jump` is the single enumeration point — was available and was not
taken.

**Campaign:** de2706af-51e1-4f63-828c-7cd3cfcc5195

## CHANGE-conformance-010 — core.md#the-dependency-graph — `tracing` joins the authoritative dependency list it was always in the graph of

**Contradiction:** §9 lists `shared`'s dependencies and says

> "This list is the authoritative one; §8.7 refers back to it rather than
> restating it."

The list has nine crates and does not have `tracing`, which `crates/shared/Cargo.toml`
declares and `ProjectView` calls at `project.rs:136` and `:144`. `deps.md` §9
says of the same crate

> "`tracing` is not really a choice — `rope` and `sum_tree` depend on it, so it
> is in the graph regardless, and having two logging facades would be silly"

and `shared` depends on `rope`. So one document has it as unavoidable and the
other, in the sentence claiming to be authoritative, omits it. The graph's
`driver` annotation reads `crossbeam-channel, rayon` against a manifest of
`rustc-hash, shared, tracing`, which is the same omission twice.

**Resolution:** `tracing` joins §9's `shared` list, with `deps.md` §9's reason
attached, and the graph's `driver` annotation gains `rustc-hash` and `tracing`.

This trades nothing because it selects no dependency: `tracing` was chosen in
`deps.md` §9, arrives transitively through `rope` whatever §9 says, and is
declared in two manifests already. The opposite reading — that the
authoritative list forbids it, so `shared` must drop `tracing` — costs the
`ProjectView` logging and leaves `rope`'s copy in the graph anyway, gaining
nothing. This is *not* a widening of the dependency set and does not need
escalating as one; a crate not already sanctioned by `deps.md` would be Class B.

**This campaign edited §9 and touched the code it describes**, though only the
manifests it *reads*: no dependency was added or removed anywhere.
`shared_declares_only_the_dependencies_section_9_lists` now fails on any
declared dependency outside the list, so the next omission is a red build
rather than four campaigns of nobody rereading the section. It asserts a subset
and not an equality, because §9 lists `rayon` for `ProjectView::scan` and
`deps.md` §14 has each dependency arrive with its first user — a listed crate
not yet declared is the intended state.

**Campaign:** de2706af-51e1-4f63-828c-7cd3cfcc5195

**Campaign:** ff3e1a40-5639-4c57-ac81-66ea1144762f

## CHANGE-conformance-011 — resolution.md#3-projectview — `scan` cannot return a bare `ScanOutcome`

**Contradiction:** §3 prints

> `pub fn scan(&self, req: &ScanRequest) -> ScanOutcome;`

while three claims in the same two sections make that return type
unwritable. §3 itself, four paragraphs later: "Every read still checks the
deadline first and **fails with the deadline variant of `shared::Error`**
rather than starting I/O that cannot be used." §1.3: "The only thing that can
stop a search early is the deadline, and when it does the query **abstains
entirely** rather than committing from a partial view." §4, on `ScanOutcome`:
"It does not stop on a byte budget, a file count, or a parse count, and
**there is no partial-scan outcome to report**."

A scan reads. So a scan can fail the way a read fails, and `ScanOutcome` is
specified to have nowhere to put that: no partial flag, no truncation marker,
no count that could be read as "and then it stopped".

**Resolution:** `scan` returns `Result<ScanOutcome, Error>`. The deadline
expiry propagates out of `read` with `?` and the caller never sees a
`ScanOutcome` that is not the whole scan.

This reading is the one that trades nothing off because the alternatives each
give up something the documents insist on: a `ScanOutcome` with a `partial`
field is the truncation flag §4 spent most of a section removing, and one
without it would report a clipped scan as a complete one, which is the
overclaimed-uniqueness failure §4 says is worst on exactly the large
repositories the corpus is least able to catch. Nothing else in the shape
changes — `ScanOutcome`'s three fields are §4's, verbatim.

The document was wrong rather than the code being made to fit: this is a
return type that could not have been implemented as printed, and the campaign
that found it implemented `scan` for the first time.

**Campaign:** 0faab934-4ecd-4a55-b992-c112e0bfcb4d

## CHANGE-conformance-012 — core.md#84-location-is-byte-based-and-this-fixes-a-real-inconsistency — `measure_core` does put a position on a wire; what it never puts there is an answer

**Contradiction:** §8.4 closes its second consequence with

> `measure_core` puts nothing on a wire and does none of this, which is why
> the conversion lives in `driver` rather than in `shared`.

`measure_core` is an LSP client. It settles a `PositionEncoding` from the
oracle's `InitializeResult` (`collect.rs:141`) and builds the wire position it
sends with the very constructor this section is about:

> ```rust
> let wire = WirePosition::encode(
>     ByteOffset(position.offset),
>     encoding,
>     &Rope::from(text.as_str()),
> )?;
> ```
> — `crates/measure_core/src/collect.rs:271`

§8.2's own Construct list already records the same thing from the other side
(CHANGE-conformance-008, "the outgoing half"), so the document contradicts
itself rather than merely the code.

**Resolution:** the premise is narrowed to the conversion the section is
actually about, and the conclusion is left where it was. `measure_core`
encodes the position it *asks about*; what it never does is put an *answer* on
a wire, because it asks and reads the reply rather than serving one. The
handler's `Location`s stay in byte space all the way into the record —
`replay.rs:180` maps them straight to `DefinitionSite`, §7's position field is
a byte offset, and §6's predicate compares `(uri, line)` — so
`Location -> WireLocation` has exactly one caller in the system, and that is
the reason it lives with its caller instead of in `shared`.

This trades nothing off: the sentence was load-bearing only for the
conclusion, and the conclusion survives on a stronger premise than the one it
had. Nothing about where the conversion lives changes.

**Same-campaign disclosure.** This campaign also wrote the conversion
(`847624c`), so it edited a design document and the code that document
describes in one sitting. What moved in the document is the *justification*
for a placement the code already had and still has: `Location -> WireLocation`
was in `driver` before this campaign and is in `driver` after it, and the code
side of the campaign adds a caller rather than moving one. The gap the
auditor recorded against this sentence was that the sentence is false about
`measure_core`, not that the code disagreed with it.

**Campaign:** b62bf25e-f5da-47f8-8c6e-00f19d0ab13c

## CHANGE-conformance-013 — core.md#snapshots-are-o1-to-take-and-are-parsed-before-a-handler-sees-one — `realise` cannot parse inside a deadline it is not given

**Contradiction:** the section's prose puts the parse under the deadline

> the split is what keeps `core` doing O(1) work while the parse still
> happens inside the worker and **inside the deadline**

and the signature printed nine lines below it takes no deadline and has no
other route to one:

> ```rust
> impl SnapshotSeed {
>     pub fn realise(self) -> Result<DocumentSnapshot, Error>;
> }
> ```

`SnapshotSeed`'s fields are `uri`, `text`, `version`, `language_id`, `base`
and `grammar`, so the printed type cannot observe a clock, a cancellation flag
or anything else that would let the parse stop early. "Inside the deadline"
was true only of the *thread* the parse runs on, which is what the same
sentence's first clause already says.

**Resolution:** the signature becomes
`realise(self, deadline: &Deadline) -> Result<DocumentSnapshot, Error>`, and
the section gains two paragraphs: an abandoned parse reports
`HandlerError::DeadlineExpired` rather than `Error::Parse`, and the
abandonment is best-effort at tree-sitter's own granularity.

This is the reading that trades nothing off. The alternative — deleting "and
inside the deadline" — would have made the document consistent by giving up
the property, and the property is one the design needs: a query cancelled by
`$/cancelRequest` would otherwise keep a worker parsing at full speed while
the proper LSP waits behind it, which is the exact failure `deadline.rs`'s own
module comment says cooperative cancellation exists to avoid.

The error arm matters as much as the parameter. A parse abandoned on time is a
*decision* and belongs with every other expiry (§1's one error class mapped
back to an abstention); a parse that fails is a *failure*. Merging them would
put "this file is too large to parse in 40ms" and "this grammar is broken" in
one row of §7's table, which is the distinction the record exists to make.

The granularity paragraph is new information rather than a repair: the
callback fires once per 100 parser operations
(`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `tree-sitter/src/parser.c:81`), so
small parses are uninterruptible. Measured, not assumed —
`crates/shared/tests/document.rs` asserts both directions, including that a
60-byte document parses to completion under a cancelled deadline.

**Same-campaign disclosure.** This campaign also wrote the code the changed
signature describes. What moved in the document is a signature that could not
express a claim the same paragraph made; the claim itself is unchanged, and
the edit makes the section *harder* to satisfy rather than easier — before it,
a `realise` that ignored deadlines conformed.

**Campaign:** e017e797-a44c-4aae-8906-3ce8a4004a7d

## CHANGE-conformance-014 — core.md#84-location-is-byte-based-and-this-fixes-a-real-inconsistency — the conversion's price rests on a read cache a human already ruled away

**Contradiction:** §8.4 prices the `Location -> WireLocation` conversion
against a cache that does not exist:

> The reason it must be there rather than anywhere later is the one this
> document already used to make the agreement predicate read nothing: **the
> per-query read cache is only alive inside the query.** [...] at the moment
> the handler returns, every target file's text is already in the view's cache
> and the conversion is nearly free.

against `state/decisions/conformance-005.md`, answered `accepted`
2026-08-03T05:13:34+00:00:

> Option A stands: no read cache. resolution.md section 3 is what is wrong —
> "each file is read at most once" is not implementable behind a Sync &Query
> without a primitive this project does not have, and it should say what it
> means instead. That correction is a Class A edit and a normal campaign
> target.

The same section's second claim is wrong for an unrelated reason — a false
claim about a dependency's API. §8.4 said that without a carried row the
driver must build "a whole-file line index", and that with one "only that one
line's text is needed". Neither holds against the vendored `rope`:
`Rope::offset_to_point` is a sum-tree seek on the `Point` dimension
(`vendor/rope/src/rope.rs:423`), so a row costs `O(log n)` and no index is
built — and a caller *cannot* read one line without exactly the index the
claim was avoiding, since finding a line's bytes is the same lookup. The
claim was repeated at `core.md`'s `Location` doc comment and in
`resolution.md` §6.

**Resolution:** §8.4 now says the conversion re-reads the target file, once
per location, and gives the reason it still belongs on the worker thread
immediately after the handler returns: proximity to the read the handler just
did, which is where the page cache is warmest and the bytes are likeliest to
still be the bytes the offsets were taken against. The "why `Location` carries
a line" bullet that claimed a saved index now states what the redundancy
actually buys, and `resolution.md` §6's version — which is about
divergence-classification time, where a *read* genuinely is the alternative —
keeps its argument and loses its reference to a cache.

This reading trades nothing off. The conclusion §8.4 drew (convert in the
dispatch wrapper) survives with a better premise; what changes is the price
quoted for it, in the direction of the accepted decision.

**A code change landed in the same campaign, and this is that flagged
plainly.** Rewriting the spec toward the code is the one way of faking
progress the audit cannot catch, so: the correction above *created* a finding
rather than absorbing one. With no cache, the handler's read and the
conversion's read are two reads of the same path, so a file edited between
them yields offsets that are stale and still in range — and
`WirePosition::encode` would encode them without complaint. The carried row is
the only witness, so the conversion now compares it against the text it read
and refuses on disagreement (`EncodingError::LineDisagreesWithRange`,
`Location::line_in`, and
`a_target_file_that_moved_under_the_query_is_refused_rather_than_encoded`).
The document gained a hazard it did not previously describe; it did not lose
one it could not meet.

**Campaign:** acb37d9b-56ff-4568-8b74-a5ac0bc66a55

## CHANGE-conformance-015 — deps.md#12-testing — `tempfile` is rejected, per the ruling on conformance-015

**Contradiction:** §0's summary table read `| `tempfile` | chosen |` and §12's
table placed it exactly — "Fixture repositories for `ProjectView` scope
tests". That suite exists. `crates/shared/tests/project.rs` builds its fixture
repositories under `Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)` and
declares no such dependency.

Under §14's "each arrives with its first user", a chosen-but-undeclared crate
is the intended state — five of §0's rows read that way today. `tempfile` was
not one of them: its named user had arrived and chosen otherwise, which is
indistinguishable, from the table alone, from a crate that is merely early.

**Resolution:** §0's row now reads **rejected**, and §12's row names
`CARGO_TARGET_TMPDIR` instead. A "Deliberately not adding" entry gives the
reasoning: the safety property `tempfile` was chosen for — a stale fixture
cannot mask a failure — is already held, because `fixture()` calls
`remove_dir_all` on entry and rebuilds from scratch, its call sites use
distinct names, and `CARGO_TARGET_TMPDIR` is per test target. What the
directory buys over a dropped `TempDir` is that it survives a failure, which
for a suite whose fixtures encode `.gitignore` semantics is worth more than
automatic cleanup. The entry says plainly that the general argument for
`tempfile` is the better one and would win on a suite whose helper did not
already clear.

**This is a document moved toward the code, and that is flagged rather than
buried.** It is not this campaign's judgement: `state/decisions/conformance-015.md`
was answered `accepted` with the edit named as the point of the ruling — "a
dependency marked chosen whose named user does not use it is indistinguishable
from one that is merely early, which is the confusion this record was opened
about". The campaign changed no code here; `fixture()` is untouched and
predates it.

**Campaign:** 51628b98-b5ea-48b1-bb77-696ecc51face

## CHANGE-conformance-016 — deps.md#2-channels — the removed `unbounded` lint is recorded where it used to fire

**Contradiction:** §2 said "`crossbeam-channel`, `unbounded()` everywhere, per
`shim.md` §2", and `shim.md:173` said "All channels are unbounded." Against
both, `clippy.toml:53` denied `crossbeam_channel::unbounded` by name —
"Unbounded channels hide backpressure and are usually where a
recv_timeout/select was intended. Use bounded unless sender and receiver share
a thread." — under a `disallowed_methods = "deny"` that `[workspace.lints.clippy]`
sets. Following §2 would have meant an `#[expect]` at every transport channel,
which is §15's own stated failure mode for a lint.

**Resolution:** ruled on in `state/decisions/conformance-016.md`, answered
`accepted` for option B: the design documents win and the `clippy.toml` entry
is gone. §2 now records that the entry existed and why it was removed, because
a removed lint leaves no trace where it used to fire and its reasoning was good
enough to be re-added by someone who had not read the section.

The narrower point the ruling turns on is now in §2 as well: in the transport a
full channel does not apply backpressure, it deadlocks — the sender is a
pipe-reader thread, so blocking it stops the fd being drained, which blocks the
child's write. A lint cannot be right about that by default, because the answer
differs per channel; `driver/src/files.rs`'s `bounded(1)` stays correct. What
replaces the lint is not a lint: the `core` inbox depth is logged and watched,
with `shim.md` §10's shed-load rule bounding memory, which is the mitigation §2
already named.

**This document was moved and no code moved with it.** The campaign built no
channels and changed none; `files.rs` predates it. The escalation was filed
before the conflict was blocking precisely so that it would not be discovered
mid-build by the phase-2b loop.

**Campaign:** 51628b98-b5ea-48b1-bb77-696ecc51face
