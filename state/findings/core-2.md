# Findings — core, worker 2

## Do these three first, one turn each

1. **`harness/gate core`** before writing. A red HEAD from a cross-branch race
   has happened (`core-002`) and it suspends green-or-revert.
2. **Settle each assigned gap from `state/audit/gap-log.jsonl`.** Find the run
   that *opened* it; ask whether a later run's `sections_audited` names its
   section. It tells you stale *and* fresh — one of mine was already closed,
   the other was opened by a run containing my own fix commits, so it was real.
3. **Read the gap's claim against the section text spliced into your prompt.**
   The splice is the current document, so a gap quoting it is already answered.

## Falsified — act on these directly

* **Plant before writing the doc comment, not after.** My draft said
  `CommitPolicy` was "held by nothing"; the plant failed
  `driver/src/workers.rs:238`, which spawns pool threads with an
  `Arc<CommitPolicy>`. "Nobody holds this" is the claim most often wrong.
* **A passing test may be an empty branch, not a weak one.** Nothing anywhere
  asked whether `ProjectView` is `Sync` — a `RefCell` field built the library
  and every test binary. `FileText::Open` was reached by no test at all.
* **`git checkout <file>` reverts your campaign's work, not just your plant.**
  Revert with the inverse `sed`/python, asserting the old text was present.
* **A blank line before `Co-Authored-By` kills the whole trailer block.** No
  error; `hj record` says "already recorded" and no metrics row lands.
* **`cargo test` green is not gate green.** `redundant_clone` and
  `cast_possible_truncation` in a test file are errors. Clippy, then fmt, then
  gate.
* **A private field is not a private type** (`ServerId::KNOWN` is a `pub const`).
* **Printed-block tests compare names, not types**; read the scan's parser.
* **`Margin::new` checks only `>= 0.0` deliberately** — a grep for
  `Confidence`'s invariant lands on it and reads like a §1 contradiction.
  `read_dir` is already banned in `clippy.toml`. `FileList::superseding` is
  covered in `driver/tests/file_list.rs`. `whole_token_matches`'s multi-chunk
  join has no route from outside until the driver has a document map.
* **`deps.md` is exhausted for this loop**; §14 cannot go clean (`core-023`).

## Confirmed — candidates, test on your own evidence

* **Hunt for a claim stated as an equality and checked as a subset.** Now 12 of
  14 commits across two campaigns. New instances: a *field list*
  (`name: Type`, never names alone — names forgive
  `files: Arc<Mutex<FileList>>`), a public-signature list, and `bytes_scanned`
  asserted `> 0` where the claim is "bytes actually read". When the obvious
  blocklist has a legitimate exception (`classified: AtomicU8` is deliberate),
  the equality is the shape that still works.
* **A section whose assigned gap is stale still has unheld claims.** Eight
  here, in one file, after the gap itself cost one commit.
* **Ask what input the *user* can produce.** Nested workspace roots enumerate
  every inner file twice — one definition returns as two hits, so a query that
  should commit abstains. `core-027` (open; provisional innermost-wins).

## Decisions

`core-027` open, tagged in `shared/src/project.rs`. `core-025`/`core-026` are
reconciled (core-1). `core-001`/`003`/`021`/`023` are blocked on a human.
