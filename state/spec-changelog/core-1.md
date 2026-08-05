# Spec changes — core, worker 1

## CHANGE-core-031 — resolution.md#3-the-projectview-seam — `ProjectView` gains `classified`, so an expiry can carry the prior out

**Contradiction:** not one, and this entry says so plainly because the shape is
the one the loop prompt says to watch for — a design document and the code it
describes edited in the same campaign.

This is an **accepted Class B ruling being implemented**, not a document moved
toward the code. `state/decisions/core-025.md` is `accepted: C plus B`, and C
reads:

> C first, and C in its first form only: `ProjectView`'s expiry carries the
> strata the handler had, as a change to `Error`. Not the second form.

The strata have to reach the `Error` from somewhere. `ProjectView` is
instantiated per query and is what raises the expiry, so it is where the
handler's prior is published — and §3's printed `impl ProjectView` block is the
authoritative signature list, so a method that exists in the code and not there
would be the drift this changelog is for.

**Resolution:** `design/resolution.md` §3's block gains

```rust
pub fn classified(&self, prior: Stratum);
```

with the note that nothing reads it back — its only consumer is
`HandlerError::DeadlineExpired`, which now carries `classified: Option<Stratum>`
— and that it is optional and unchecked, because a handler that abstains before
classifying anything never calls it and that residue is a real absence rather
than a missing call. The residue is what `core-025`'s option B is for and is not
in this change.

Why this reading trades nothing off: it is the one form of C the ruling
authorises. The alternative the ruling names and rejects — requiring handlers to
return `Ok(Abstain { reason: Deadline, .. })` rather than `?` — is refused
because it undoes `core.md` §1's `?`-propagation argument, and nothing here
touches how a handler propagates. `core.md` §1 already defers to §3 for the
signature list ("`read`, `parse`, and `scan` are all on it; `resolution.md` §3
has the full signature list"), so `core.md` needs no edit and did not get one.

**Campaign:** 32a9eaee-72c3-4048-84a9-b89fb1d6967f
