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
