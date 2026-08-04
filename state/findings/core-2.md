# Findings — core, worker 2

## Open with these two, one turn each

1. **`harness/gate core`** before writing. A red HEAD from a cross-branch race
   has happened (`core-002`) and suspends green-or-revert.
2. **Settle each assigned gap from `state/audit/gap-log.jsonl`, not from
   timestamps.** Find the run that *opened* it and check whether that run's
   `sections_audited` names the section. My two answered differently this
   round: §10's was opened by the latest run against the current file (real);
   §2's was a stale row carried forward by a *partial* audit that never
   re-judged the section, and had been closed three campaigns earlier.

## Falsified — act on these directly

* **"The list over-reports" is only half of it. It under-reports the same
  sections.** §10's one listed gap closed in ten lines; the section then had
  **four** more claims with no mechanism, none of them listed — a transcribed
  nine-name list, `main`'s return type, one foreign error not `#[source]`d,
  and (§2) the single bounded channel. A gap is what the auditor could *see*,
  so a one-gap section is not a one-problem section. Print the section
  (`harness/hj section-text`) and take its sentences one at a time, asking what
  would fail if each stopped being true. That produced four of my five commits.
* **Stale, verified, do not re-check:** `deps.md#8[ffcd948852]` (`trees.rs` is
  an `LruCache`, `lru` declared), `core.md#7[bd3003d0fb]` (all three
  particulars false), `core.md#two-modes[6bd547104d]` (`4c50a45` appends every
  row), `e83fd58b7a` (aliases at `shared.rs:85`).
* **`deps.md` §10 and §2 are done** — five commits, both clean, nothing left.

## Confirmed — candidates, test on your own evidence

* **Plant the negation.** Nine plants this round, nine correct failures. Two
  generalisations that paid: a gap naming one site is a hypothesis about how
  many there are; and in a two-fixture test the *negative* fixture is usually
  the whole test — assert the line is absent where the mechanism should not
  fire, or you have tested the symptom.
* **Log at the point of conversion, not of construction.** `classify` is the
  only place an `Error` stops being a failure; `Actor::answer` sees three
  origins and can name none of them.
* **The transport is still where the real work is** — `shim.md` §2's codec,
  §3's router, the child spawn. It buys almost no number (`shim.md` is
  unaudited this phase) and is a large campaign. Say so in the hypothesis.

## Traps that cost a red gate

* **Text scans read comments.** Never quote a banned identifier in one — *and
  never write a scan that fires on prose*. Skip comment lines, or you build the
  trap for the next worker (`actor.rs` explains at length why its inbox is not
  bounded).
* `std::fs::read_dir` is disallowed; enumerate with `seam.rs`'s `sources_of`,
  which follows `mod` declarations.
* **`driver` may not name `tracing_subscriber`, tests included.** The
  hand-rolled `Capturing` subscriber in `tests/actor.rs` already exists —
  reuse it rather than rediscovering the 35 lines.

## Decisions

* **core-022** (mine, open): an unclassified query lands in `unimplemented`;
  `pipeline.rs`'s template check asserts the provisional and is *meant* to fail
  when answered.
* core-001–004: all need a human; `harness/` and `clippy.toml` are denied.
