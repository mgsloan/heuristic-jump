# Findings — core, worker 3

## Falsified — act on these directly

**`deps.md` §2 and §10 are finished.** §2's inbox depth, its single bounded
channel, §10's nine arms, `main`'s return type, the foreign-error rule, and now
the expiry conversion on *all three* of `classify`'s callers are all mechanised.
§2's one remaining sentence — that `clippy.toml` no longer denies
`crossbeam_channel::unbounded` — is **compiler-held**: `Cargo.toml:182` sets
`disallowed_methods = "deny"` and `driver/src/driver.rs:66` calls `unbounded()`,
so re-adding the entry fails the build. A test there would be strictly weaker.
Measured, not assumed.

**Stale assignments are now measured in minutes, not days.** `d50e2285d0` was
closed eight minutes after the audit that opened it. The `git log -1` staleness
check is still the right first turn; read the *time*, not the date.

**A five-line fixture cannot test a parse abandoned on the deadline.**
`SnapshotSeed::realise` polls the deadline from tree-sitter's progress callback,
which fires once per 100 parser operations, so `DOCUMENT` finishes inside one
interval and observes no deadline. Planted: it fails on the hard cap instead.
Any test of that route needs its own large document.

**`#![expect(clippy::panic)]` does not cover `panic_in_result_fn`.** A handler
double's vacuity guard must return an `Err`, not panic.

**`Co-Authored-By` in its own paragraph unmakes every trailer.**
`git interpret-trailers` reads only the last paragraph, so `hj record` walks
past your commit and says "already recorded". Keep it inside the block.

## The most valuable thing here: `core-025` is accepted and unstarted

It rules **C then B**: `ProjectView`'s expiry carries the strata out as a change
to `shared::Error`, which empties the second route into `Classified::Nothing`;
then `stratum_prior` becomes nullable in `shared::record`, `measure_core`'s
`Table::row` and its replay. That arm does not get a better `Stratum` — it stops
returning one. The record says `ExpiredStrata::Assigned`/`Unclassified`; the code
says `Classified::By`/`Nothing`, renamed since.

`core-022` and `core-024` are closed as its duplicates. Until this campaign the
only driver tag named `core-022` and none named `core-025`, so a grep for the
open work found nothing. Now tagged at `dispatch.rs`'s `Classified::strata`.
**This is a whole campaign** — `shared` and `measure_core`, not `driver`.

## Confirmed — candidates, test on your own evidence

* **Work the section, not the gap.** Both assigned gaps were closed; the defect
  was one sentence further along, in a *comment* claiming coverage nothing
  checked. Third campaign running that this produced the commit.
* **A negative fixture is the test.** Three routes asserting one conversion line
  each are worth little without the exact **zero** on `hard_cap`'s line beside
  them.
* Plant every assertion. Four plants, four correct failures.

## Verified closed — do not re-take

`deps.md` §2, §10, §8, §12, #fxhashmap, #14's profile;
`rope-modifications.md#the-signatures`; licensing's `high-level.md` half.

## Blocked on a human

`deny.toml` (`core-021`/`core-023`), `harness/measure` (`core-001`),
`clippy.toml` thresholds (`core-003`). `core.md#4[d41389f7fe]` was REFUSED to me
— another worker holds it.
