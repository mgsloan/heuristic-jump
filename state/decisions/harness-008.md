---
id: harness-008
status: open
opened: 2026-08-04T20:41:00+00:00
campaign: 78bbbbc4-9003-447e-9139-61389562ceb5
kind: harness-request
---

# Should `harness/gate` step 5 call `hj check-ratchets` instead of hard-failing in a cost phase?

## Context

`design/loops.md` §11 gates binary size "only in phase 3, where the ratchet is
hard: neither may increase at all". `harness/gate` reserves step 5 for it and,
in phase 3 or 7, does this:

```sh
case "$phase" in
3 | 7) fail "the phase $phase ratchets are not implemented; design/loops.md section 18 defers them past phase 1.5" ;;
*) echo "skipped: ratchets are a cost-phase gate, and this is phase $phase" ;;
esac
```

That was correct when nothing measured binary size. It is no longer: this
campaign implements the measurement (`hj size`), the re-baselining §10 asks
for, and the ratchet itself (`hj check-ratchets`). The step as written now
fails *every* gate run in a cost phase, including the ones that pass the
ratchet — a check that rejects something correct.

`harness/gate` is in `DENIED_ALWAYS`, so no loop can make the change, and the
deny list's own comment names this case: "A gate step, an auditor's prompt or
a denominator that needs changing is a decision record."

## Options

**Swap the `3 | 7` arm for `"$hj" check-ratchets "$loop" || fail "ratchets"`.**
Costs one line. It puts the check where the gate's step numbering says it
lives, and step 7's metrics check stops carrying something that is not a
metrics row. The risk is that it makes a red gate reachable in a phase nobody
has run yet, on a code path whose only exercise so far is the selftest — but
that is equally true of the workaround.

**Leave step 5 as it is and keep the ratchet inside `check-metrics`.** Costs
nothing now and is where the check actually runs today. It leaves the gate
with a step that fails unconditionally in the phase it exists for, so the
first phase-3 campaign has to discover that its gate cannot go green for a
reason unrelated to its work, and the `fail` message will tell it something
false — that the ratchets are not implemented.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

The second, because it is the only one available: the file is denied. The
ratchet runs from `cmd_check_metrics`, which `harness/gate` calls at step 7,
and is a no-op outside `SIZE_RATCHET_PHASES` — so today's gate behaviour is
byte-identical and the workaround costs nothing until a cost phase starts.

`hj check-ratchets` exists and is written to be step 5's body verbatim, so
answering this is a one-line edit rather than an implementation.

No site carries a `// DECISION-harness-008: provisional` tag, because the
provisional choice is *not doing* something in a file this loop may not open.
The comment above the call in `cmd_check_metrics` names the record instead.

## Consequences

If the answer is the first option, delete the `or check_size_ratchet(...)`
from `cmd_check_metrics` and change one line of `harness/gate`. If it is the
second, `cmd_check_ratchets` is dead code and should be deleted rather than
left as a second route to the same check.

Either way this has to be settled before phase 3 opens, not during it: a gate
that cannot go green stops the loop, and the first campaign of a cost phase is
the worst moment to discover it.
