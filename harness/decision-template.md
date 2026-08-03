# Decision records

The single source of this shape. It is spliced into the loop prompts at
launch (`design/loops.md` section 14), so there is one copy and it is this
one.

MADR, with frontmatter the dashboard parses. The frontmatter is not
decoration: a record whose `status` cannot be read does not appear in the
panel a human answers from, so it waits forever.

Write it to `state/decisions/<owner>-NNN.md`, where `NNN` is the next unused
number for *your* owner. The owner prefix is why two loops cannot claim the
same number four seconds apart.

```markdown
---
id: conformance-007
status: open
opened: 2026-08-02T14:03:00+00:00
campaign: 4f6a2c18-...
kind: class-b
---

# One line naming the question, ending in a question mark

## Context

What you were doing, what you hit, and why it cannot be settled without
trading something off. Quote the spec where the spec is the reason.

## Options

Each with what it costs. Two is usually enough; if there is only one, this
was not a decision.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Which option you took meanwhile, why it is the most reversible one, and the
sites you tagged `// DECISION-conformance-007: provisional`. **This section
is the one that matters**: it is what lets you keep going, and it is what a
human is really ruling on when the answer arrives.

## Consequences

What changes if the answer goes the other way, and how much of the tagged
work has to be redone.
```

`status` is `open` while it waits. A human answers it — from the dashboard,
which appends the answer, the rationale and the resulting status here and to
`state/interventions.jsonl` in one action. Do not answer your own record; a
loop that rules on its own escalation has escalated nothing.

`kind` is `class-b` for a spec change that trades something off, or
`harness-request` when what you need is a number the harness will not give
you or a gate that rejects something it should allow. The second kind exists
so that recurring requests are visible: five campaigns asking for the same
number means the harness is wrong, not that the campaigns are demanding.

Once answered, reconciling the tagged sites is a normal campaign target —
pick it like any other item, not as an interrupt.
