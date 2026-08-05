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

## CHANGE-core-033 — core.md#7-observability-and-the-corpus-scan — both stratum columns become nullable

**Contradiction:** again not one, and again this is a design document and the
code it describes edited in the same campaign — said plainly for the same
reason. `state/decisions/core-025.md` is `accepted: C plus B`, and B is:

> Then B rather than A for the residue. [...] `null` says the true thing in the
> place the absence actually lives, and it forces each consumer to decide what
> to do with it rather than letting it be grouped away silently.

§7 printed `"stratum_prior": "explicitly_imported"` with no nullable case, so a
record whose column is `null` had nothing in the document to be right against.

**Resolution:** §7 gains a paragraph after the coverage/precision one saying
both stratum fields are nullable and that `null` is not a tenth stratum, naming
the two routes into it (a parse abandoned before any handler ran; a handler
that returned `Err`, which has no `Outcome` for a stratum to be on), and saying
what the value used to be and why that was wrong — `Stratum::Unimplemented` is
the *template's* stratum and §9 makes it self-identifying, so a slow or broken
handler counterfeited an unreplaced language crate.

Two places where this goes beyond the ruling's literal words, both stated here
rather than assumed:

* **Both columns, where the record names only `stratum_prior`.** The ruling
  names the prior because it is the coverage denominator, but the absence it
  describes — "nothing ever looked at this reference" — is an absence of
  classification, and a `stratum_final` of `unimplemented` beside a null prior
  is the same guess in the column precision is computed on. Nothing is traded:
  a query with no prior committed nothing, so no verdict is judged under a
  settled stratum either way.
* **Where the bucket goes.** The ruling says consumers "gain an empty bucket"
  and does not say where. §7 now says beside a per-stratum table rather than in
  it, because a tenth row would read as a kind of reference, which is the exact
  ground option A was rejected on — and that it must be split by `decision`,
  because one counter would merge "the parse ran out of time" with "the handler
  is broken", the merge §7 already spends a paragraph refusing.

The report's beside-the-table lines are not otherwise described in `core.md`
(it documents the per-query record, not `replay`'s rendering), so nothing else
in the document moved.

**Campaign:** 32a9eaee-72c3-4048-84a9-b89fb1d6967f
