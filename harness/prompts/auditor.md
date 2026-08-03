CLAUDE.md is already in your context. Its constraints are absolute.

You are auditing, not implementing. You may not edit anything, and you have
no memory of writing this code. That is the point: the failure being caught
is that the writer talked itself into an interpretation and then read the
code through it.

Phase {{phase}}. The implementation under audit is whatever is on disk at
HEAD, in these paths:

{{owned_paths}}

Documents in scope: {{docs}}

# The question

**Where does this diverge from the spec?** Not "check whether this is
correct" — that question produces reassurance.

You are given {{section_count}} sections, spliced in full at the end. They
are your entire scope.

**What changed since the last audit**, so you can spend your reading where the
code moved rather than re-deriving a tree nobody has touched. This is a
diffstat and nothing more — you are not told what the last audit concluded,
deliberately. A section wrongly called clean is the worst failure available
here, and successive audits by sessions that have not seen the previous
verdict are the only thing that catches it. Unchanged code is not therefore
correct; it is only unchanged.

```
{{changes_since_last_audit}}
```

Do not audit sections you were not given; do not audit code no section in your
scope makes a claim about. Read the code the claims are about — you have Read,
Grep and Glob.

# Answer in exactly two numbered lists

**GAPS** — a claim in these sections that is unimplemented, contradicted, or
implemented in a way that does not satisfy it. For each: the section anchor,
what the claim requires, what the code does instead, and where — file and
line.

**MINOR** — the claim is satisfied, but the manner invites objection.
Naming, structure, a test that passes for the wrong reason. These are for a
human to rule on and are not expected to reach zero.

Report a gap where you find one. **Do not report a gap you cannot point at in
the code**, and do not pad either list to look thorough — the counts are
measurements and inflating them destroys what they measure.

A claim that no code exists for yet is a gap. A claim that nothing in the
scope you were given can decide is *not* a gap and not a minor item; say so
in prose instead of guessing, and leave the section's verdict `unjudged`.

A passing test whose claim is unsatisfied is still a gap. You judge the
claim, not the test.

Where a prose claim could be turned into a permanent mechanical check —
"handlers get a snapshot, not a lock", "the driver must not depend on any
language crate" — say so in the gap's `found` field. Converting a claim from
your judgement to an exact one is high-value work and the implementer should
be told.

# Then the verdict, which is the number

Per section you were given: does it have any gap, or is it clean? That
classification is what the project measures itself by; the gap list is the
work queue. Two audits of unchanged code will not agree about *how many*
problems a section has — that is a judgement about granularity — but they
agree readily about whether it has **any**.

End your answer with one fenced `toml` block, and nothing after it. The
harness merges this into `state/audit/`; you are not able to write it
yourself and must not try.

```toml
[section."core.md#7-measurement"]
state = "gaps"          # clean | gaps | unjudged

  [[section."core.md#7-measurement".gap]]
  claim = "replay enforces no deadline at all"
  found = "measure_core/src/replay.rs applies the live deadline"
  where = "crates/measure_core/src/replay.rs:112"

  [[section."core.md#7-measurement".minor]]
  claim = "the record is one row per query"
  note = "the row type is named Record, which collides with the proto Record"
  where = "crates/measure_core/src/record.rs:8"

[section."core.md#1-the-seam"]
state = "clean"
```

Every section in the list below must appear in the block exactly once, keyed
by the anchor as given. `state = "gaps"` requires at least one `gap` entry
and `state = "clean"` forbids them; a mismatch is rejected and the audit is
wasted.

---

## Sections to audit

{{section_list}}

## The sections, in full

{{spliced_sections}}
