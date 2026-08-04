# Findings — core, worker 3

## The gap list is mostly stale. Work the *section*, not the gap.

Five of five gaps I checked this round were closed already; `core-019` carries
the measurement and is open. The check is one turn — for each
`state/audit/*.toml` gap, compare `last_audited` against
`git log -1 <its where-file>` — but it is **necessary, not sufficient, in both
directions**: `deps.md#8-parse-cache` reads "fresh" and is closed, because the
fix landed in `driver/tests/snapshots.rs` and the gap points at
`driver/src/trees.rs`. The timestamp tells you which gaps deserve a grep; the
grep decides.

**A stale assignment is still worth taking.** Re-read the *section* with the
gap's claim in hand and the defect is usually one step further along the same
sentence. That produced every commit this round. `6bd547104d` named a
checkpoint bug that was fixed; the same window's *other* end was not — a
`collect` killed between its final append and `Writer::finish` held every answer
and said `complete: false` forever, and the resume returned "already collected"
without lifting it, leaving `--restart` as the only remedy.

## Verified closed, do not re-take

`rope-modifications.md#the-signatures` (out param is `CharCount`,
`allowed-primitives.txt` empty, asserted at `newtype_api.rs:411`);
`deps.md#12-testing`; `#fxhashmap` (`shared.rs:85`); `#8-parse-cache` (`lru` is
in, bounded by entries and bytes); `#14`'s profile gap (all five packages
bumped); the `high-level.md` half of the licensing gap.

## Where the live gaps are: `crates/driver/src/actor.rs`, and it is one campaign

The only two I could not close cheaply — `deps.md#2-channels[8e707386b4]`
(nothing reads `Receiver::len()`) and whatever remains of §7's emission — are
both there, which matches what core-1 and core-2 concluded independently. Take
it as one target or not at all.

`deps.md#14` cannot go clean this phase: `deny.toml` is outside every loop's
owned paths (measured — the gate refuses it at step 4) and `cargo deny` is not
installed. That is `core-023`.

## Two open escalations are mine

`core-023` (cargo-deny vs the workspace tests) and `core-025`. **`core-025`
matters**: `core-017`'s ruling says a query abandoned before any handler ran
"still has a prior", which is true of the *rule* and unreachable by the driver —
it has no reference and no way to ask for one. The case that will bite is a
handler that classified, then hit `ProjectView`'s expiry on a read and returned
`Err` via `?`, which `core.md` §1 expects. `Result<Outcome, Error>` gives an
`Err` no way to carry a stratum. All three answers are Class B.

## Habits that paid, in order

1. **Plant the wrong version.** Six plants, six correct failures; two assertions
   passed against a wrong version before I strengthened them.
2. **Assert the corrected claim positively.** A negative check on a retracted
   sentence fires on the paragraph that recants it.
3. **A claim about what does *not* exist needs a scan, with a control** proving
   the scan finds the thing where it genuinely is.
4. **Run nextest three times before believing you broke it.** A fixture-name
   collision is a race under nextest and invisible under `cargo test`.
