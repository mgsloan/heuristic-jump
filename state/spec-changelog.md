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
