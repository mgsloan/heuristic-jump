---
id: conformance-006
status: open
opened: 2026-08-03T05:34:00+00:00
campaign: dc1c9639-0a25-4eeb-aa61-b0cfaee75485
kind: class-b
---

# Does a wire position outside the document fail, or clamp the way LSP says?

## Context

`core.md` §3 makes position conversion "the highest-risk correctness detail in
the whole driver" and §8.3 makes `WirePosition::resolve` the single door out.
What neither says is what `resolve` does with a position that does not name a
place in the document.

LSP 3.17 does say, for `Position.character`:

> If the character value is greater than the line length it defaults back to
> the line length.

The vendored rope agrees with LSP by construction: `point_to_offset`,
`point_utf16_to_offset` and the `clip_*` family all move an out-of-range or
mid-surrogate position to the nearest valid one and return it with no signal
that they moved it.

The trade is real in both directions:

* Clamping is what a conforming client is entitled to expect, and an editor
  legitimately sends `character` past the end of a line — Vim's virtual edit
  and a client that computed a column against a stale version both do it. A
  strict `resolve` turns those into abstentions.
* Clamping is also what makes an encoding bug invisible, which is the failure
  §3 is written around. A UTF-16 column read as UTF-8 bytes is *in range* on
  almost every line, so clamping does not catch that case either — but a
  position past the end of the document is one of the few signals that the two
  ends disagree about the text at all, and it is the same signal §8.6's
  "range outside our rope" self-check is built on.

It cannot be settled without trading one against the other, and the choice is
observable in the metric: it moves queries between "answered" and "abstained".

## Options

* **Reject.** `resolve` returns `EncodingError` for anything that does not
  survive a round trip through `encode`. Costs coverage on positions a
  conforming client may legally send, and every such loss is visible in the
  corpus record as a failure rather than as a wrong answer.
* **Clamp, per LSP.** Answer about the nearest valid position. Costs the
  detection: a document that has drifted out of sync produces confident
  answers about the wrong place, and nothing downstream can tell that from a
  correct one.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Reject**, at `crates/shared/src/proto.rs`'s `WirePosition::resolve` and
`WirePosition::encode`, both tagged `// DECISION-conformance-006: provisional`.

It is the more reversible of the two. Relaxing to clamping later is deleting
the round-trip comparison in one function, and the positions it currently
refuses are exactly the ones clamping would have accepted — no caller has to
change, because `resolve` already returns `Result` for the line-out-of-range
case either way. Going the other direction is not symmetric: every wrong
answer clamping produced in the meantime was recorded as an answer, so the
corpus cannot say afterwards which rows were affected.

It is also the direction the loop prompt's own posture points: an abstention
costs coverage, a clamped answer costs correctness.

## Consequences

If the answer is "clamp", the change is confined to `resolve`: drop the
round-trip comparison and the `CharacterOutOfRange` variant becomes
unreachable for the past-end-of-line case (it is still reachable for
mid-surrogate, unless that clamps too — which is the sub-question, and the
answer should say). `EncodingError::LineOutOfRange` stays either way; LSP
specifies nothing about a line past the end of the document.

The property tests in `crates/shared/tests/position_encoding.rs` assert the
strict behaviour in two of five properties (`rejects_positions_outside_the_document`
and the exactness half of `resolve_matches_the_reference`); those two would be
rewritten rather than deleted. Nothing else in the workspace calls `resolve`
yet, so the reconciliation cost today is one module and one test file.
