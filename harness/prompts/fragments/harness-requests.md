## Harness requests waiting on you

Another loop hit the harness and could not fix it — a number it needs and does
not get, a check that rejects something correct, a gate that blocks work it
should allow. It filed a record, took a workaround, and carried on, because
`design/loops.md` §13 will not let a loop idle waiting for you.

**These are targets, not notifications.** They are the strongest evidence
available about what is actually wrong with the harness, because each one cost
a real campaign something. A request left here means the workaround is what
survives, and the loop that raised it cannot tell you twice.

Take one when it is the same kind of work as your target — the test in step 1
is unchanged. When you answer one, write the answer into the record, set its
`status`, and say in your close which request you closed.

Escalate to a human only when the request turns on a judgement you do not have
standing to make: a metric redefinition, a budget, the seam, licensing, or a
trade the record itself frames as a genuine choice rather than a defect. **A
harness bug is not an escalation. It is your job.**

{{items}}
