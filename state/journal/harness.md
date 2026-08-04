# Journal — harness loop

What was tried, what failed, and why. Written for a session that will not
remember this one. The diff says what worked; this says what did not, and
what nearly did.

## bb1e501a — section 16, the operator view, read noun by noun

Target: all seven anchors of section 16, plus
`#mechanics-isolation-in-four-layers` as an extension. Eleven commits, no
reverts.

### The method that worked, and is the point of this entry

**Read the section's nouns, not its shape.** The previous campaign's finding
said the dashboard was not where the gaps were, on the evidence that all
eleven panels render. That was true and it was the wrong test. Section 16
names six fields for `state/interventions.jsonl`, a vocabulary of `kind`s,
three derived numbers, twelve fields for the session row, and four
properties of the transcript renderer. Every one of those is a noun you can
grep for. Seven of them were missing, and the panel containing each looked
fine.

The tell, in both this campaign and the last: **a section that names a
number nothing else reports.** Section 15 named tokens per gap closed;
section 16 names interventions per 100 experiments, recurring kinds, and
time to answer. Both times the prose was quoted in the code as a comment or
a panel note and the arithmetic was absent. Grep the design for a sentence
of the form "nothing else in the design reports it" and you have found the
next campaign's target.

### The thing worth not rediscovering

**Deriving a decision's status from the log is not a two-line change**, and
the reason arrived mid-campaign: a human answered `harness-001` by editing
the record, with nothing in `state/interventions.jsonl`. So the log says
open and the record says accepted.

Both simple answers are wrong. Log-authoritative puts a settled question
back in the queue and tells the loop to re-litigate a ruling a human made.
Record-authoritative is the harness trusting someone to have written both,
which is the thing section 16 says will not happen — "`rationale` is the
field that matters and the one that will get skipped".

The resolution is a third state, not a fourth ruling: the ruling stands, and
both the page and the prompt say the reasoning was never logged. If a future
campaign is tempted to simplify this into one rule, this is why it is two.

It also exposed a hole nobody had noticed: **an answered decision with no
tagged site drops off the prompt's decision list entirely.** The list shows
open records, and answered ones only while something in the tree still says
`DECISION-<id>: provisional`. `harness-001`'s answer requires four edits and
no tag, so the loop that raised it would never have learned the answer
arrived. Now surfaced through the unlogged state — which is a fix that
depends on the human having skipped the log, so it is narrower than the hole
it happens to cover. If a future campaign finds an answered decision it was
never told about, this is the shape.

### Approaches considered and not taken

* **A gate check that the design's ownership table matches `hj`'s deny
  constants.** This is the highest-value shape available — converting a
  judgement into an exact check — and I did not build it, for the reason the
  last campaign gave for `hj estimates`: it parses a prose table out of a
  document the core loop can write, and a gate that goes red because someone
  reformatted a table gets deleted by the third campaign it annoys. Worse
  here than there: `hj estimates` reports, but this would have to fail to be
  worth anything, and a failing gate stops *both* loops. If someone builds
  it, key it on path tokens in backticks and not on table structure, and
  think hard about what it does when the table is legitimately edited.

* **Installing the post-commit hook.** The hook is written and works;
  `hj install-hooks` puts it in place. I did not run it: it changes what a
  human's own `git commit` prints, in a checkout they share, and the
  detector half (`hand_authored_commits`) gives the same signal on the
  dashboard without touching their setup. One command, a human's call.

* **Fixing `gate_runs` to count only real gate invocations.** It counts any
  bash command containing `harness/gate`, so a campaign that greps for the
  string inflates its own `empty` experiment count — mine reported 7 gate
  runs against 4 real ones early on. Left alone deliberately: `cost` and
  `progress` share the definition, `experiment_mix` is built on it, and
  changing it is a metric redefinition that invalidates comparability across
  the change for a number whose only job is a health signal. If it is ever
  changed, it is Class B and it needs a sweep.

* **Enabling the OS sandbox** (§13 layer 3). Escalated as `harness-002`
  instead. Two independent reasons: `.claude/**` is denied to every loop,
  and the `allowWrite` list as specified is narrower than the ownership
  table beside it — a campaign runs `hj record` itself, which writes
  `state/metrics/`, and `hj cost` now writes `state/sessions.jsonl` in the
  *integration* checkout. Switching it on as written fails the first
  `hj record` and then every gate after it. Do not "just enable it".

### Things worth knowing about the machinery

* **The gate never ran this campaign's `hj`.** `harness/gate` runs
  `$here/hj selftest`, where `$here` is the reviewed harness — so for the
  one loop allowed to write `harness/`, the file being changed was the one
  file the gate did not execute. Fixed here (the tree's copy is checked too
  when it differs), but the general lesson stands: **the pinned gate checks
  the pinned harness.** Anything you change under `harness/` is on you to
  run. The cross-worktree loop from the last entry is still the right habit,
  and it now has a second reason.
* `assistant` events repeat a `message.id` as a message streams, but the
  `tool_use` blocks inside them do not repeat — 150 blocks, 150 distinct
  ids on campaign 11b9c019. So counting tool calls per event is safe, even
  though summing usage per event is not.
* `campaigns_of` merges `state/sessions.jsonl` by id, which is what makes
  appending a partial row a legal backfill. Anything appending a partial row
  must check that, and must append only when the field is actually missing —
  otherwise the file grows by a row per campaign per run.
* This worktree runs ~20 commits behind `main` for most of a campaign, so
  `state/` here is not what the dashboard shows. `harness-001` reads as open
  here and is answered on `main`. Check `main` before concluding something
  is unanswered.

## 11b9c019 — section 15's cost accounting

Target: `#the-unit-of-accounting-is-the-campaign`,
`#budgets-at-three-scopes`, `#cost-per-unit-of-progress`. Closed all three
plus `#4-the-iteration-contract`,
`#estimates-and-replacing-them-with-measurements` and
`#levers-by-which-resource-they-move`. Seven commits, no reverts.

### The thing that nearly broke both loops

`hj selftest` (commit 2bf6b4a) ran its adapter check against
`HARNESS / "adapter"`. `HARNESS` is `REPO / "harness"` and `REPO` comes from
`HJ_REPO` — **the tree being checked, not the harness doing the checking.**
The gate runs from the reviewed harness with `HJ_REPO` pointed at each loop's
worktree, so the check was testing whichever adapter that loop happened to
have. It passed in this worktree, where the new adapter lives, and would have
failed all three core workers on their next gate run with
`KeyError: 'gate_runs'`.

It was caught only because I ran the selftest against every worktree before
closing, on a hunch. **Do this. Every time you touch `harness/`:**

```sh
for t in $(git worktree list --porcelain | awk '/^worktree /{print $2}'); do
  HJ_REPO="$t" harness/hj selftest
done
```

The general shape, which will recur: anything in `hj` that resolves a
*harness* file must resolve it relative to `__file__`, not relative to
`REPO`. `REPO` is for the tree under inspection. The two are the same only in
the worktree you happen to be sitting in, which is why this class of bug is
invisible from here and loud everywhere else.

### Approaches considered and not taken

* **Editing `harness/prompts/conformance.md`** to move the campaign id out of
  the instructional body. This is the fix for section 15's first token lever
  and it is worth real money — the cacheable prefix is 11.9% on core, and
  13.5KB of never-changing body is re-sent uncached every campaign. Do not
  just do it. Section 16 makes a prompt revision the one intervention that
  cannot be replayed, and section 18 denies this loop `harness/prompts/`.
  Escalated as `harness-001` with the numbers; the measurement (`hj
  prompt-prefix`) is the reversible half. **If a future campaign finds
  harness-001 answered `A`, the work is four edits and an intervention log
  entry — do not redo the measurement.**

* **Rewriting section 15's estimate table from the measured actuals.** The
  section literally says the estimates "get rewritten from them", so this
  looked like sanctioned Class A work. I did not, and would advise against
  it: rewriting the spec toward what was measured is indistinguishable, from
  the audit's side, from rewriting it toward what was built, and section 19
  calls that the failure with the thinnest defence. Building `hj estimates` —
  which reads the table out of the document and prints it beside the actuals
  — gives the human at the phase gate everything they need to rewrite it
  themselves, and moves nothing.

* **A per-phase budget scope separate from per-language.** Section 15 says
  "Per phase, per language", which reads as two scopes and is one: the
  ceiling is per (phase, language), and `state/phase.toml` already describes
  exactly one phase. Implemented as a language ceiling applied to
  current-phase spend. If a later reading wants a phase-total ceiling
  independent of language, it is a new key, not a redefinition.

* **Making `hj estimates` a gate step.** It parses a markdown table out of
  `design/loops.md`, and the core loop can write `design/`. A gate that goes
  red because someone reformatted a table is a check that will get deleted by
  the third campaign it annoys. It reports, and returns 1 only when run
  directly.

* **`cost --refresh` re-measuring everything on every call.** Rejected in
  favour of refreshing only rows missing a field named in `COST_FIELDS`. The
  file is append-only, so a blanket refresh grows it by 30 rows per
  invocation; with `merge_cost_rows` that is correct but wasteful, and the
  waste is the kind nobody notices until the file is a megabyte.

### Things worth knowing about the data

* Campaign rows in `state/cost/` had no timestamp of their own. The join to
  time goes through `state/sessions.jsonl`'s `ended`. Audit rows have `ts`.
* Audit rows carried no `phase` until this campaign; older ones fall to the
  current phase, which is right until the first phase change and wrong after.
  If phase 2a starts, the seven 1a audit rows written before `12c70e4` will
  read as 2a spend. Worth a one-off fixup then, not now.
* `campaigns_of` merges `state/sessions.jsonl` by campaign id; cost rows now
  merge the same way. Anything appending a second row for an existing entity
  must check that its readers merge, or it double-counts silently.
* The three "campaigns" with $0.00 and no gate runs are sessions that died
  before doing anything. They are in the denominator of anything that counts
  campaigns. Do not read a 27-campaign average as 27 real campaigns.
