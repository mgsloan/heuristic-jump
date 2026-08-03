# Commit trailers

The single source of this convention. It is spliced into the loop prompts at
launch (`design/loops.md` section 14), so there is one copy and it is this
one.

Trailers are parseable with stock `git interpret-trailers`, which is what
makes `git log` the journal and lets stall detection work without separate
bookkeeping.

```
[shim-3.2] route swallow decision through writer:editor

audit: shim.md#3-message-routing gaps -> clean
tests: +1 double_response_assertion
loc: driver +38
binary: +412B
decision: none
loop: conformance
campaign: 4f6a2c18-...
```

Every trailer is `name: value`, one per line, in a block at the end of the
message with a blank line before it. Omit a trailer whose value would be
nothing rather than writing `n/a` — except `decision:`, where `none` is
meaningful and an omission is ambiguous.

| Trailer | Value |
|---|---|
| `audit` | the section anchor this commit targets, and the state change it intends: `gaps -> clean`, `unjudged -> gaps`. It is a claim about intent; the auditor decides whether it happened |
| `tests` | net change in test count, and the name of the test that carries the claim |
| `loc` | per crate, signed |
| `binary` | stripped size delta, when the commit is in a phase that measures it |
| `decision` | `none`, or the ids of the decision records this commit acts under (`conformance-007`) |
| `loop` | the owner, so the gate can tell a loop's commit from a human's |
| `campaign` | the campaign id, which is also the session id, so cost and transcript join to the commit after the fact |

`loop` and `campaign` are not in `design/loops.md` section 4's example. They
are here because two mechanisms need them and neither can work by inference:
the gate's metrics-row check has to distinguish a loop commit from a
hand-authored one, and section 15's cost accounting joins `ccusage` output to
commits on the session id.
