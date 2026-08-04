# Findings — core, worker 3

## The stale-gap tax is now a decision record. Stop re-deriving it.

Three campaigns in a row opened by verifying gaps that were already closed.
`core-019` (`harness-request`, open) carries the measurement — seven of nine
`core.md` gaps have a `where:` file that moved after the audit that opened
them — and the provisional rule: **before claiming, compare `last_audited` in
`state/audit/core.toml` against `git log -1 -- <where-file>`.** One `python3`
over the file covers every gap and costs one turn. Verified closed and still
listed: `#what-the-templates-handler-does[9adb0be268]`,
`#the-oracle-is-the-server-being-proxied[eb6f4618da]`,
`#7-observability[bd3003d0fb]`'s "nothing in `crates/driver` emits one"
(`driver/src/trace.rs` does). Do not spend a campaign on these.

The freshest gaps are the real ones and they get claimed first. When
`hj claim` refuses twice, the list is telling you the round is over — that is
not contention, it is a stale list making two live items look scarce.

## `#the-table-is-not-enough` is closed as far as a loop can close it

`--records`, both digest keys, unfiltered rows, held-out isolation, the
records↔table reconciliation and the six sample fields are all asserted in
`crates/measure_core/tests/pipeline.rs`. What is left is `harness/measure`
itself: `core-001`, open, `harness/**` denied. **Do not write the digest in
`measure_core`** — the section says digesting is the harness's job, "the same
split that keeps `measure_core` ignorant of `state/`", so a digest there would
satisfy an auditor and destroy the thing the sentence protects.

## Reconciling two artifacts is how you find a wrong verdict

The bug this found: `replay` classified `agreement` for rows the handler never
answered, so an abstention the oracle answered was `mismatch` in the records
and nothing in the table — a precision loss where §7 counts coverage loss. The
rule was already written, in `ChildAnswer`'s doc comment, and unexecutable.

Generalise it: **`shared` is full of doc comments stating rules that no test
runs.** Where one names a rule, the cheap high-yield move is to find the two
places that must agree about it and assert they do. The tell is a comment that
says "which is a different fact from" or "must not be" — that phrasing is a
rule nobody could check. Same defect family as last campaign's "docs naming
the wrong mechanism", one level worse: a rule stated and not enforced, rather
than a mechanism named and not real.

## Fixtures: one file hides joins, one handler hides denominators

`pipeline.rs`'s fixtures were all a single file and mostly non-refining
handlers, and both hid assertions. A join on `(file, offset)` is
indistinguishable from one on `offset` alone with one file (`OTHER_SOURCE` now
puts a different identifier at the same byte). `Row::precision`'s denominator —
the three agreement counters, not `committed` — is invisible unless a handler
*refines*, because otherwise the two coincide; `ReportingHandler` against a
`null` oracle separates them, 100% against 0%. **Plant the wrong version and
watch it fail** before believing an assertion of this kind; both of the above
passed first try and one would have passed against the bug.

## Ruled out, do not re-derive

§8.5's negative tests are complete (`crates/shared/tests/proto.rs`). Editor
traffic cannot be captured by a loop (`core-018`). The §7 minor about replay's
`mode: "proxy"` with `server_health: null` is a real doc gap and a *minor* —
it moves no number, and taking it means editing §7 in a campaign that edits
`replay.rs`, which is the shape the spec-drift rule watches for. The journal
entry for `ede3701b` has the resolution written out if someone takes it
deliberately.
