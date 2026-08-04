# Campaign 20bbc1bf-03c5-4d3c-afda-a5c5791d47ce

- loop: core
- phase: 1a
- opened: 2026-08-04T19:43:43+00:00
- commit at open: 692e0987fe1ab924f13f219c523271b8771c1edc
- session: `claude --resume 20bbc1bf-03c5-4d3c-afda-a5c5791d47ce`

## Target

`core.md#two-modes-collect-and-replay`, assigned as
`core.md#two-modes-collect-and-replay[6bd547104d]` together with
`rope-modifications.md#the-signatures[a163ac3aee]`.

### Both assigned ids are stale, for the same mechanical reason

The audits that opened them ran on trees that do not contain the fix:

* `6bd547104d` describes a checkpoint that "appends only the single latest row
  every CHECKPOINT_EVERY positions". `4c50a45` ("a resumed collection asked the
  wrong positions") replaced that with an append per answer and moved the row
  vector inside `truth::Writer`. `git merge-base --is-ancestor 4c50a45
  dd846068` is false — the audit commit predates the fix.
* `a163ac3aee` asks for `ChunkSlice::longest_row`'s out parameter to be a
  `CharCount`. It is one (`vendor/rope/src/chunk.rs:401`),
  `allowed-primitives.txt` is empty by CHANGE-core-015, and
  `newtype_api.rs:411` asserts the declared type *and* the value
  (`total_characters == CharCount(5)`). Same ancestry result against
  `e23d2149`. Nothing to do; the section is clean.

Fourth campaign on this worker to open this way. `core-019` carries the
measurement and is open.

## Hypothesis

The section `6bd547104d` names is still not clean, and the live defect is one
step further along the same claim — §7's "**a truth file is regenerated, never
edited**", and "a partially collected truth file is marked incomplete and is
never consumed by replay".

`collect` appends every answer as it arrives and rewrites the header last, so
the window between the final `append` and `Writer::finish` — which spans
`client.stop` and the server shutdown handshake, plausibly the slowest single
step of the run — leaves a file that holds **every** answer and still says
`complete: false`. `Truth::read` refuses that, so replay cannot use it. And the
resume cannot close it either: `Collection::run` sees `done >= all.len()`, logs
"already collected" and returns without touching the header. The only remedy
left is `--restart`, which discards the machine-hours the rows already paid for
and spends them again — on the one artifact the section says should be
regenerated rarely.

Shared context with the assignment: this is `collect.rs`'s resume arithmetic
and `truth.rs`'s writer, the two files the planner named, on the section the
planner assigned.

## Targets taken

Named, in the order taken. The first is the assignment; the rest were claimed
through `harness/hj claim` and each shared the files or the section already
open.

1. `core.md#two-modes-collect-and-replay` — the sealing defect above, plus two
   claims of the section that no test read.
2. `deps.md#licensing-our-crates-are-mit-the-binary-is-gpl[9d0b19a109]`
   (claim granted).
3. `deps.md#14-workspace-cargotoml-shape[d822e97954]` — escalated, not closed.
4. `core.md#the-trait[5fcb043e7b]` (claim granted) — reconciling `core-017`.
5. `deps.md#9-logging-and-tracing` — reconciling `core-002`'s tagged sites.

## Experiments

1. `truth::resume_collection` folds the drift check, the answered-row count and
   the seal into the one decision `collect` makes before it starts a server;
   `Collection::run`'s early return becomes that call. Public for the reason
   `check_resumable` is — `collect` cannot be driven from a test without a
   language server, and this is the whole of what it does before it starts one.
   Gate green, committed (`559c0b9`). Found and fixed a pre-existing nextest
   race in the same commit: two tests shared `fixture("digest_sample")`, which
   is the corpus root, which `fixture_of` clears.
2. Two scans for §7's "no server, no network, no `didOpen` round trips" and
   "only `replay` writes the record" — both claims about what does not exist,
   which nothing but a scan holds. Green, committed (`5a2c10e`).
3. The licensing gap was stale, but the retraction it names left a second
   straggler inside `deps.md` §5 itself. Class A (CHANGE-core-016), plus the
   section's one edge claim asserted positively. Green, committed (`ab42dcd`).
4. `deny.toml` probed against the gate to establish it is unwritable rather
   than assumed; escalated as `core-023`. Green, committed (`581dc3f`).
5. `Dispatched::DeadlineExpired` carries `ExpiredStrata`, so a capped answer
   keeps the stratum the handler assigned. The residual case escalated as
   `core-025`. Green, committed (`e95246a`).
6. `core-002`'s three tagged sites reconciled. Green, committed (`de3242a`).

## Outcome

**Confirmed.** Six commits, +6 tests, two escalations filed and two answered
decisions reconciled.

Both assigned gap ids were stale — the audits that opened them ran on trees
that do not contain the fix — and so were the three other gaps checked
(`deps.md#12-testing`, `#fxhashmap`, `#8-parse-cache`, `#14`'s profile gap, and
the `high-level.md` half of the licensing gap). What the campaign did instead
was work the assigned *sections*, which is what the stale-gap procedure is for:
the section is still the right target and a re-audit of it finds something the
list does not have.

`rope-modifications.md#the-signatures` needed nothing: the out parameter is a
`CharCount`, `allowed-primitives.txt` is empty by CHANGE-core-015, and
`newtype_api.rs:411` asserts the declared type and the value.

Left deliberately: `deps.md#14` cannot go clean this phase (`core-023`), and
`deps.md#2-channels[8e707386b4]` is live but lands in `crates/driver/src/actor.rs`,
which is one campaign with the rest of the missing run loop.
