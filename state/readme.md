# `state/`

The loops' memory. `design/loops.md` section 0: the model does not remember
the previous iteration, so everything it needs between them has to be written
down — what is done, what is left, what was decided, what was tried and
failed, and what the numbers were last time.

Everything here is tracked in git. It is partitioned by owner, so two loops
running at once never write the same file.

| Path | Written by | What it is |
|---|---|---|
| `phase.toml` | a human | desired state: the phase, each loop's status, and the ownership table. Denied to every loop |
| `audit/<doc>.toml` | `harness/audit` | per section: clean, gaps, or unjudged, and the gap list. Nobody authors these by hand |
| `campaigns/<owner>/<id>.md` | the loop | one campaign: target, hypothesis, experiments, outcome. The archive behind the one-line summaries |
| `journal/<owner>.md` | the loop | approaches tried and abandoned, and why. What the trailers cannot carry |
| `decisions/<id>.md` | the loop, answered by a human | MADR records. Class B escalations, and the seeded open questions |
| `metrics/<owner>.jsonl` | `harness/hj record` | one row per commit. A cache, not a source of truth — replay is deterministic, so any row can be recomputed |
| `findings/<owner>.md` | the loop | a digest of at most 512 words, rewritten at every campaign close, for other loops to read |
| `shared-proposals/<lang>-NNN.md` | a tuning loop | "I needed this and wrote it locally." Nothing consumes them until phase 3 |
| `spec-changelog.md` | the loop | Class A spec fixes, with the contradiction quoted |
| `sessions.jsonl` | the harness | the campaign index: id, prompt sha, commits, outcome |
| `interventions.jsonl` | the harness | everything a *human* did. The other half of the audit trail, and the half with the higher information density |
| `handoff.md` | the loop | written on stall: what it was trying, what is blocking, and the one question that would unblock it |

Transcripts are not here. They are 2 MB per campaign and gigabyte scale over
a phase, so they live outside the worktree — `../heuristic-jump-transcripts/`
by default, `$HJ_TRANSCRIPTS` to move them. `sessions.jsonl` is the index into
them.

## Decision ids are owner-prefixed

`state/decisions/<owner>-NNN.md`, because a bare incrementing id is the kind
of thing that looks fine until two loops allocate `007` four seconds apart
(`design/loops.md` section 13). Code tags follow the file name:
`// DECISION-conformance-007: provisional`, and `grep -r DECISION-` is the
outstanding-provisional-choice report. Its count is a health metric — rising
steadily means the loop is running ahead of its decisions.

**The queue is not seeded from `design/open-questions.md`.** That list and
`resolution.md`'s are the author's, not the loops': they are questions to be
answered with measurements and product judgement, and converting them to
escalations would hand a loop a hundred provisional choices it has no
evidence to make and no campaign to make them in. A decision record exists
because a campaign hit something and could not proceed without choosing —
that is what makes the outstanding count mean anything.

## Answering happens through the dashboard

`harness/dashboard/serve`, not an editor. Answering a decision *means*
appending to `interventions.jsonl` — the harness derives a record's status
from what was written, so the log cannot drift from what happened. Editing a
decision file by hand leaves the answer but not the reasoning, which is the
half with the higher information density.

## What is not here yet

The cost rows (`cost/<loop>.jsonl`), the findings protocol in anger, and the
frontier all belong to phase 2a and are specified in `design/loops.md`
without being built. Section 18 says why: they exist to serve tuning loops,
and there are none until there is a corpus to tune against.
