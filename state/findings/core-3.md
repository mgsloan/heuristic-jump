# Findings — core, worker 3

## Confirmed (candidate): an answered decision leaves a straggler in the spec

`conformance-005` was applied to the code — `project.rs`'s module doc and
`trees.rs`'s `ParseKey` doc both say the disk-file cache does not exist — and
never to `deps.md` §8, which went on claiming its key in the present tense.
That was this round's whole gap. **When reconciling an answered decision, grep
the design corpus for the refused sentence, not only the code.** Likely
remaining instance: `resolution.md` §3's "each file is read at most once" and
"from the parse LRU when possible", which conformance-005's ruling explicitly
calls wrong and which no campaign has fixed — `resolution.md` is out of audit
scope this phase, so it will never appear as a gap.

## Falsified: a granted claim is not evidence a gap is live

`hj claim` granted `deps.md#fxhashmap[e83fd58b7a]`, closed since campaign
5cc94daa — `shared::Map`/`Set` at `shared.rs:85`, and
`the_default_map_and_set_are_the_aliases_shared_exports` in
`driver/tests/seam.rs` scans *every* workspace member for `rustc_hash`,
`FxHashMap`, `FxHashSet`, with vacuity guards on the member list and each
source. The ledger knows who is working on what, not what is true.

## Still true: the gap list over-reports. Work the *section*, not the gap

The one-turn check stands — for each gap, `last_audited` against
`git log -1 <its where-file>` — and is necessary, not sufficient, in both
directions. When a gap *is* stale, re-read its section anyway with the claim in
hand: the defect is usually one step further along the same sentence. That
produced every commit two campaigns running.

## Verified closed, do not re-take

`#fxhashmap`; `deps.md#8-parse-cache` (this round, CHANGE-core-017);
`deps.md#12-testing`; `#14`'s profile gap;
`rope-modifications.md#the-signatures`; the `high-level.md` half of licensing.

## Where the live gaps are

`crates/driver/src/actor.rs` (`#2-channels`: nothing reads `Receiver::len()`)
and `crates/driver/src/dispatch.rs` (`#10`: the deadline→abstention conversion
is silent). Both were REFUSED to me this round, which is the fourth independent
confirmation that the remaining `deps.md` work is those two files. One campaign
or none.

`deps.md#14` cannot go clean this phase: `deny.toml` is outside every loop's
owned paths (measured — the gate refuses it) and `cargo deny` is not
installed. That is `core-023`.

## Method, in order of what it bought

1. **Plant the wrong version, one assertion at a time.** A `thread_local` map
   plants a cache with no lock and no signature change — three lines, and
   `git checkout` reverts it. Planting each half *separately* matters when the
   test's first assertion is a precondition check: otherwise the live
   assertion is whichever one you happened to write.
2. **Make the fixture edit discriminate against the specific claim.** The
   rewrite is a different text of the *same length*, the one edit
   `(path, mtime, len)` cannot see. A shorter rewrite would have tested
   caching in general and proved nothing about the key the spec named.
3. **A Class A edit near a numbered open question must leave it standing.**
   Deleting §8's disk key would have answered `open-questions.md` question 5
   by removing its subject. Keeping the key and marking it cache-less is what
   kept this out of Class B.
4. **Run nextest three times before believing you broke it** — a fixture-name
   collision is a race under nextest and invisible under `cargo test`.

## Do not spend time on

* `core.md`'s three gaps — worker 1's document, and the planner forbids it.
* Building any cache in `shared`: `conformance-005` (accepted) and `CLAUDE.md`
  both refuse it, and that refusal is now `deps.md` §8's stated position.
* `harness/measure` (`core-001`), `clippy.toml` thresholds (`core-003`),
  `deny.toml` (`core-021`, `core-023`) — all need a human.

## My open escalations

`core-023` and `core-025`. **`core-025` is the one that matters**: a handler
that classified, then hit `ProjectView`'s expiry on a read and returned `Err`
via `?` — which `core.md` §1 expects — has no way to carry a stratum out of an
`Err`. All three answers are Class B.
