# Findings — core, worker 3

## Falsified — act on these directly

**Two `git log -S` runs settle a spec/code disagreement before you weigh it.**
`CharCount` was `usize` in `rope-modifications.md` and `u32` in the code. That
reads like a judgement call and was not: the document was moved by **`1b9dd51`,
a human commit, "Design change: CharCount is usize"**, two days *after* the code
was written. Check who moved which side, and when. The code moves.

**A wrong record is worse than no record.** `vendor/README.md` called that
narrowing deliberate and attributed it to §4 — which asks for the opposite —
citing an argument §2 rebuts in its very next sentence. Three campaigns took the
section and none saw the repr, because the file that would have told them said
it was settled. Records get read *instead of* what they describe.

**The gate cannot see a stale cross-reference.** It checks that an anchor
*resolves*, not that the cited document still makes the claim. Four this
campaign: a vendor record, a citation, a summary sentence, and an allowlist
three documents call empty and none bounded.

**A prose scan must strip per-line markdown, not just collapse whitespace.**
`rope-modifications.md` states its headline claim in a **blockquote**; `>`
survives `split_whitespace`, and my first scan failed against the document it
was quoting *from*. `newtype_api.rs`'s `unwrapped` handles it — use it.

**Plants mask each other.** Three in one test: the first fired, the second
assertion was never reached. Revert and re-run one at a time.

**A grep that disagrees with a document is a claim about the grep first.**
`fn [a-z_]*_raw` misses `offset_utf16_to_offset_raw`: the class excludes digits.

**`Trace`-not-allocated cannot be tested here.** It needs a counting
`#[global_allocator]`; `GlobalAlloc` requires `unsafe`, which `CLAUDE.md` bans.
The width assertion holds *boxed*; the rest is review. Do not rebuild this.

**Do not re-take:** `core.md#the-trait` — assigned stale three campaigns running
(CHANGE-core-018 closed it nine minutes after the audit opened it); the block
agrees with the seam and its behaviour is now covered. `Strata::refine` is held
end-to-end at `pipeline.rs:543`. `rope-modifications.md` should be clean.

## Confirmed — candidates, test on your own evidence

* **A mechanising test can encode the side the document argued *against*.** §4's
  `MAX` assertion transcribed §2's losing argument — "the bound `Point.row`
  already imposed" — into an assertion, and looked like coverage. Read a test's
  *message* against the section, not only its subject.
* **The compiler holds a repr; it does not hold the accumulation the repr is
  for.** Sum two summaries instead of building a 4G rope — the same `AddAssign`
  the sum tree runs at every internal node.
* **A file of printed-block scans leaves behaviour uncovered.**
  `shared/tests/handler.rs` compared names and arity, so `MAX_STAGES` was never
  reached by any test in the repository.
* Six plants, six correct failures. Still what pays.

## Blocked on a human

`deny.toml` (`core-021`/`core-023`), `harness/measure` (`core-001`),
`clippy.toml` thresholds (`core-003`). **`core-025` is accepted and still
unstarted** — a `shared` + `measure_core` campaign, tagged at `dispatch.rs`'s
`Classified::strata`. It rules **C then B**: that arm stops returning a
`Stratum` rather than getting a better one.
