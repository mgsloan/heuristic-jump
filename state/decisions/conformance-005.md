---
id: conformance-005
status: accepted
opened: 2026-08-03T05:20:00+00:00
campaign: e3b8dbf4-56aa-48fc-9a4d-4018d7464f4d
kind: class-b
---

# How does `ProjectView` cache reads per query without a lock?

## Context

`resolution.md` §3 requires it: "The view is instantiated per query. Within it:
each file is read at most once, so stages 3, 5, and 6 touching the same file
cost one read." The counter beside it — `bytes_scanned`, typed `ByteLen` — has
the same shape: it accumulates across a query and reaches the trace record.

`core.md` §1 requires something that appears to contradict it: handlers "fan
out across candidate files ([`shim.md` §10]), so `&Query` — and therefore
`&DocumentSnapshot` — crosses threads and must be `Sync`". `ProjectView` is
behind the same `&Query`. So the read cache is shared mutable state reached
through `&self` from several threads at once, which is the definition of the
thing `CLAUDE.md` says does not exist here: "There is no `Mutex`, `RwLock`,
`parking_lot`, or `dashmap` anywhere in the design; state is owned by one
thread and moves over channels. Reaching for a lock means something is
architecturally wrong — say so rather than adding one."

`core.md` §2 already met this once and resolved it by *removing* the need
rather than by allowing the primitive: an earlier revision memoised the parse
in a `OnceLock`, and the fix was to parse eagerly so there was nothing to
memoise. That is the precedent this decision is trying to follow, and it does
not obviously transfer — a read cache cannot be filled eagerly, because which
files a query reads is what the query is for.

Saying so rather than adding a lock is why this is a decision record and not
an implementation.

## Options

**A. No read cache (in force).** `read` goes to the filesystem every time,
`ProjectView` stays a plain `Sync` struct with no interior mutability, and the
OS page cache absorbs the repeat. Cost: `bytes_scanned` over-counts relative to
what `resolution.md` §3 means by it, so the number that "attributes a latency
regression to a diff" measures re-reads too; and stages 3, 5 and 6 each pay a
syscall and a UTF-8 validation for a file the query has already seen. Nothing
measures how often that happens, because there is no corpus yet.

**B. The cache is per fan-out worker, not per query.** Each parallel unit of
work owns a `&mut` cache, so there is no sharing and no primitive. Cost: it
only deduplicates within a worker, so a file read by two workers is read
twice, and it changes the signature of everything the fan-out touches — the
cache has to be threaded through `scan` rather than living on the view.

**C. The cache is filled by the driver, not the view.** Reads become messages
to the thread that owns the parse LRU (`shim.md` §5), which is already a
single-owner-plus-channel arrangement. This is the shape `CLAUDE.md` describes
as correct. Cost: a channel round-trip per read on the hot path, and it makes
`measure_core` — which has no such thread — either build one or take a
different path from the shim, which is exactly the divergence `core.md` §7
says the shared `ProjectView` exists to prevent.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A.** No sites are tagged, because the absence of a cache is not
something there is a line to tag; it is recorded in the module doc of
`crates/shared/src/project.rs` and in `read`'s doc comment, and it is the
reason `bytes_scanned` does not exist yet.

It is the most reversible because it is the only one of the three that does
not commit a signature. B changes every fan-out signature and C changes what a
read *is*; both are large enough that doing them speculatively and reversing
would cost more than the re-reads.

## Consequences

If the answer is B or C, `ProjectView::read`'s callers change, which today is
nobody: no handler exists, so the tagged work to redo is zero. That is
precisely why it is worth answering while it is still cheap — the cost of this
decision is monotonically increasing in the number of `lang_*` crates.

If the answer is A stands, `resolution.md` §3's "each file is read at most
once" is wrong as written and should say what it means instead, since
`bytes_scanned`'s definition depends on it.

## Answer — 2026-08-03T05:13:34+00:00

**Ruling:** accepted

Option A stands: no read cache. resolution.md section 3 is what is wrong — "each file is read at most once" is not implementable behind a Sync &Query without a primitive this project does not have, and it should say what it means instead. That correction is a Class A edit and a normal campaign target. bytes_scanned is defined as bytes actually read.

**Rationale:** CLAUDE.md line 112 decides it: no new caching or indexing until the corpus harness shows the change is worth it and there is a benchmark, and ask before adding caching. There is no corpus, so nothing could justify B or C yet, and both commit a signature — B changes every fan-out signature, C changes what a read is and forces measure_core to diverge from the shim, which is the divergence the shared ProjectView exists to prevent. One correction to the record, in A`s favour: A does not make bytes_scanned over-count, it makes it honest. That counter`s job is to be a deterministic machine-independent proxy for latency between gates; a re-read costs latency, so counting bytes actually read is what correlates, and a deduplicated count would systematically under-predict. Writing a decision record instead of reaching for a lock was exactly the right move.

Reconciling the sites tagged `// DECISION-conformance-005: provisional` is a
normal campaign target, not an interrupt.
