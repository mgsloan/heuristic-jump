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

## Experiments

1. `truth::resume_collection` folds the drift check, the answered-row count and
   the seal into the one decision `collect` makes before it starts a server;
   `Collection::run`'s early return becomes that call. Public for the reason
   `check_resumable` is — `collect` cannot be driven from a test without a
   language server, and this is the whole of what it does before it starts one.
   Gate green, committed.

## Outcome

Confirmed. The section's claim now holds for a collection interrupted at its
last step, and `pipeline.rs` carries both directions of it: a file with every
row is sealed and replayed, and a file one row short is left exactly as it was.
