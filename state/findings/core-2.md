# Findings — core, worker 2

## Open with these two, they cost one turn each

1. **`harness/gate core`** before writing anything. A red HEAD from a
   cross-branch race has happened (`core-002`) and suspends green-or-revert.
2. **Check each assigned gap against the code**: `git log -1` on its `where:`
   file versus `last_audited` in `state/audit/core.toml` (UTC vs local -06:00),
   plus a `grep` of `gap-log.jsonl` for the id. This round the assignment was
   real and the check said so in one turn — the clean answer is what lets you
   stop thinking about it.

## The listed gaps that are stale. Do not spend a campaign rediscovering these

- `deps.md#8-parse-cache[ffcd948852]` — `trees.rs` is an `LruCache` with both
  bounds and byte-ceiling eviction; `lru` is in `Cargo.toml` and `Cargo.lock`.
- `core.md#7-observability[bd3003d0fb]` — all three particulars false since
  `actor.rs`: it emits the record, produces `queued_us`, writes proxy rows.
- `core.md#two-modes[6bd547104d]` — `4c50a45` appends every row, so the partial
  file is a prefix and `done = rows.len()` is a sound position index.
- `e83fd58b7a` (Map/Set aliases) — `seam.rs` has
  `the_default_map_and_set_are_the_aliases_shared_exports`.

Four of twelve. **The list over-reports; the code is the oracle.**

## Where the real work is

**The transport.** `driver::run` builds two channels and an `Actor` nothing
sends to. `shim.md` §2's codec, §3's router and the child spawn are the missing
piece behind every remaining "the driver owns X" gap — and `shim.md` is not
audited this phase, so it buys almost no number and is a large campaign. Say
that in the hypothesis rather than discovering it.

## Traps that cost a red gate

- **Never quote a banned identifier in a comment.** `seam.rs`'s scans read
  source *text*: a comment naming `std::sync::mpsc` fails the async-shape scan.
  Say "the standard library's channel".
- **`std::fs::read_dir` is disallowed** (gitignore semantics), and
  `ignore::WalkBuilder` is unreachable from a test crate. Enumerate our sources
  the way `seam.rs` does — read `crates/<n>/src/<n>.rs` and follow its `mod`
  lines. It also reads what the crate compiles, so a file orphaned by a rename
  cannot change the result.
- **`driver` may not name `tracing_subscriber` in any file, tests included.** To
  read a log line, hand-write a `tracing::Subscriber` (~35 lines; only
  `record_debug` is required on the visitor) sending over a
  `crossbeam_channel::Sender` — `Subscriber` records through `&self`, which is
  exactly where a `Mutex` sneaks in.

## Method that keeps paying

**Plant the negation and watch it fail.** Six plants, six failures this
campaign, and one of them changed the design: planting `or_classified_by` proved
the hard-cap test and the conversion-expiry test were *not* covering each
other's path — which is how I found that the gap named one discard site when
there were two. `encode` reads the target file and `ProjectView` refuses a read
past the deadline, so a late answer whose definition is in another file never
reaches the hard cap at all. Generalise it: **a gap that names one site is a
hypothesis about how many there are.**

The fixture detail that made it discriminating: `src/lib.rs` is the queried
document, so the conversion short-circuits its read; `src/target.rs` is not.

## Decisions affecting you

- **core-022** (mine, open): a query no handler classified still lands in
  `unimplemented`, because the a-priori rule is per-language and obtaining it
  without the handler is a seam method. Provisional tagged in `dispatch.rs`, and
  `the_template_check_reads_an_abstention_no_handler_classified` in
  `pipeline.rs` **asserts the provisional and is meant to fail when this is
  answered** — the message says so.
- core-001, core-002, core-003, core-004: open, all need a human, `harness/` and
  `clippy.toml` are denied. Do not take them.
