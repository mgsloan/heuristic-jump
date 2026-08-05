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

## CHANGE-core-034 — core.md#7-observability-and-the-corpus-scan, shim.md#10-parallel-dispatch-and-resource-limits — a shed query is a fourth `decision`, and §10 stops calling it an abstention

**Contradiction:** this one *is* a contradiction, and it was created by a human
ruling landing on a document that had not been updated for it.

`shim.md` §10:

> **Max in-flight heuristic queries** (start at 4). Beyond that, new queries
> **abstain immediately** rather than queueing.

`state/decisions/core-026.md`, answered:

> `AbstainReason` is the **handler's** vocabulary [...] A shed query is not the
> handler's event at all. [...] **accepted: D — a shed query is a disposition,
> not an abstention reason.**

So §10 instructs the implementation to do the one thing the ruling forbids.
`core.md` §7 was stale in the matching way: "**`decision` has three values, not
two**", written before there was a fourth.

**Resolution:**

* `core.md` §7 now says four values and adds the paragraphs for `shed`: why it
  is not an `AbstainReason` (a sixth variant no handler could return, on a
  frozen seam), why it is not `failed` (the column would call a working shim
  broken), what it buys (`high-level.md` requires coverage lost to load to be
  visible *as such*, which needs a rate of its own), where the two limits are
  told apart (`stages`, as `shed:in_flight` / `shed:core_behind`), and that a
  shed query's stratum columns are `null` under CHANGE-core-033's rule because
  nothing ran.
* `shim.md` §10's first limit now reads "are **shed** immediately", with a
  sentence naming `core-026` as why the word changed.
* `shim.md` §10's second limit gains what it never had: a note that "backed up"
  is not "non-empty". This is the one place the change is driven by evidence
  from the code rather than from the ruling, and it is worth being exact about.
  The literal reading was implemented first, and
  `the_loop_drains_its_channel_and_ends_when_the_wire_closes` failed on it at a
  depth of *one* — an editor sends a `didOpen` and its request together, so the
  literal rule sheds ordinary sessions. §10 names no threshold, so one had to
  be chosen; it is 4, which is the number §10 itself gives for its other limit
  and in the same "start at" spirit. §10 now says that, and says what makes it
  a starting point rather than a guess: the shed rate is a column now, so the
  cost is measurable.

This is the trade being made rather than avoided, and it is stated so a human
can rule on it: **a threshold that is not in the design has been chosen by this
campaign.** It is not a metric target — nothing in `high-level.md` is computed
from it — but it does decide how much coverage is given up under load, and 4 is
a guess with an argument rather than a measurement. It was not escalated
because `core-026`'s own "What is left" names building these two limits as the
core loop's ordinary work, and a limit cannot be built without a number.

**Campaign:** 32a9eaee-72c3-4048-84a9-b89fb1d6967f

## CHANGE-core-037 — core.md#86-modelling-errors-must-fail-closed — the third self-check is not `core`-side and not O(1), and its own bullet says so

**Contradiction:** the lead-in claims all three self-checks are cheap in the
same way —

> Three cheap self-checks turn drift into a detectable event rather than a
> permanent one, and all three are `core`-side O(1)

— and the third bullet, four lines below it, says the opposite of both halves
for itself:

> **`didSave` is a free end-to-end checksum.** ... It costs a read, so it
> belongs in a worker, off the critical path

A read is not `core`-side: `shim.md` §2 forbids `core` the filesystem outright,
which is the whole reason the bullet sends it to a worker. And the comparison
`core` is left with is not O(1) either — the section asks that "our rope's
length — or a hash of it — must match the file", and a hash is linear in the
document. Only a length comparison would be constant, and that is the one
reading the section does not take, because two texts of the same length are
exactly the drift the check exists to find.

**Resolution:** the lead-in now says the first two are `core`-side and O(1) —
each compares a number the message carries against one we hold — and that the
third is neither, for the reason its own bullet gives. What makes it cheap is
stated instead of asserted: it is on the notification path rather than the
query path, so nothing waits for it and no budget is spent on it.

This trades nothing off because it is the more specific claim winning over the
summary of it. The bullet is argued — it says why the read must leave `core`
and what a mismatch does — where the lead-in is a one-line characterisation
written of all three at once. Nothing downstream depends on the third check
being O(1); what depends on anything is that it not be on the query path, and
that is now what the sentence says.

**This campaign also wrote the code the section describes**, which is the shape
the loop rules say to declare rather than leave to be noticed: `9ff22ac`,
`2e8c082` and `9aabbb6` build the `didSave` read as a second kind of pool job,
and this edit is in the same section. It is not the code's claim being written
back into the document — the contradiction is between two sentences of the
section and would be there against an empty repository — but the edit was found
by implementing the bullet, and a reader should weigh it knowing that.

**Campaign:** 7fda63d7-fc75-4469-9bc5-ac456b8a0143
