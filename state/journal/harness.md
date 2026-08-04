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

## 8564e2f1 — the numbers hj computes about the loops

Target: nine gaps across §4, §6, §7 (all five), §11, §17 and §19, plus the
audit-cadence pair as an escalation. Ten commits, no reverts, no experiment
abandoned — which is itself worth reading as a warning rather than a boast:
see below.

The hypothesis was that all nine were one defect wearing different clothes —
a number the design names, computed approximately or computed and then never
read — and that held. Nothing here needed new machinery. `spec_drift`,
`provisional_decisions`, `loc_per_crate` and `check-metrics` all ran on every
campaign already; three of them were scoped so that the harness loop's own
work was invisible to them, and the fourth read the metrics file only to ask
whether a line existed in it.

### The method that found them, and it is the same one as last time

Take the section's literal nouns and grep for each. Every one of these was
findable that way in under a minute: `tokei` appears in `loc_per_crate`'s
docstring and nowhere in its body; `:!harness` appears twice, in the two
functions §6 calls a health metric; `crates/`, `vendor/` is an allow-list
where §19 needs a complement. Judging whether the code "looks right" finds
none of them, because it looks right.

The sharpest tell so far is still a docstring that describes behaviour the
function does not have. Two of this campaign's nine were exactly that. A
comment quoting the spec is where a previous campaign put the claim it
intended to satisfy, so it is the highest-yield place to check whether it did.

### Approaches considered and not taken

* **Do not put the ratchet in `harness/gate`.** §4 scopes the gate's
  "ratchets" step to §11's *size* ratchets, phase 3 only, so a test-count
  ratchet there needs a spec edit to justify it. It goes in `check-metrics`,
  which the gate already calls at step 7, and §7's wording — "the gate checks
  metric direction itself" — is satisfied by the metrics step reading the
  metric. This also left `harness/gate` untouched, which mattered an hour
  later when it became a denied path.
* **Do not have `hj record` write the ratchet baseline.** The first design
  had `harness/ratchets.toml` bumped automatically on every record. Three
  core workers each bumping one scalar in one file conflict on rebase, and
  `merge_back` reads a conflict as "two loops wrote the same file" and stops
  the round. The baseline is the highest value in the append-only metrics
  history instead, with the file as a hand-set floor under it. **Any new
  per-campaign write to a shared single-valued file has this problem.**
* **Do not detect test deletions by diffing a stored inventory.** Storing
  test names per row bloats the metrics file by an order of magnitude, and
  the case that matters — one test deleted, another added, count flat — is
  visible in the diff directly. `git diff -U0` and a two-state scanner does
  it with no storage at all.
* **Do not edit `design/loops.md` this campaign.** Three of the remaining
  open gaps (§1's "there is no code", §2's "one writer", §8's split) can only
  be closed by editing the document. This is the campaign that rewrote
  `spec_drift` so that a campaign editing `design/` and code together is
  flagged. Making the detector's own campaign the first thing it flags is a
  reviewer's afternoon for no gain; the edits are three sentences each and
  will be cheaper anywhere else. **The next campaign should take them.**
* **Do not rebase onto `main` mid-campaign.** See `harness-004`. The audit
  that produced this campaign's gap list was computed in *this* worktree, and
  rebasing swaps it for a different one halfway through.

### Things worth knowing about the machinery

* **A syntax error in `hj` bricks the worktree's Edit and Write tools.** The
  `PreToolUse` hook shells out to `hj check-path` and reads any non-zero exit
  as "denied", and a Python file that will not parse exits non-zero for every
  call — including the call that would fix it. The escape is bash, which the
  hook does not cover. Fixed by pointing the hook at the pinned copy, but the
  general shape stands: **anything the hook depends on must not be the thing
  the campaign is editing.**
* **`cargo nextest list` costs about 20 seconds cold and runs on every gate
  now** for a loop that owns crates. Warm, after step 3 has already built, it
  is a second or two. If a future campaign sees the gate get slow, this is
  where to look first.
* **The pinned gate on `main` is two campaigns behind this branch**, so the
  tree-`hj` selftest step added last campaign has never actually run. Run
  `harness/hj selftest` by hand after every edit to `hj`. I did, all ten
  times, and it caught nothing — but the one time it does, it will be the
  time three core workers would have gone red.
* **`git merge-tree --write-tree main HEAD` is the cheap way to see how bad a
  merge is** without touching the branch. Three hunks, all mechanical. That
  measurement is what turned "the branches have diverged" from an alarm into
  a two-minute job for whoever reads `harness-004`.

### The warning in "ten commits, no reverts"

An experiment that produces no commit is a signal, and I produced none. That
is either a well-chosen target set or a campaign that only picked work it
already knew how to do. Both are consistent with the evidence. The honest
read is that the gaps in this batch were unusually mechanical — nine
one-function changes — and that the ones left in `loops.md` are not: what
remains is spec edits, a cost trade, and a merge nobody owns. Expect the next
campaign to have a worse ratio, and do not read that as it going badly.

## 3e637dcd — the reconciliation campaign, and one gap deliberately left open

Target: §1, §2 and §8's two gaps — the three spec edits the previous campaign
handed over — which grew to §5, §6, §7, §13's merge cadence and §13's digest
asymmetry, plus a false positive in `hj`. Ten commits, no reverts.

### The method, again, and the one place it did not apply

Take the section's literal nouns and grep. Three of this campaign's gaps were
`master` (the branch is `main`), `tokei`, and `dispatch`. The one that was not
findable that way is the one still open, and the difference is the lesson:
§8's actor gap is not a word that is wrong, it is a **boundary that was
crossed**, and boundaries are found by reading the tree against the list
rather than by grepping the list against the tree.

### The thing I want the next campaign to not undo

**§8 keeps an open gap that two words would have closed.** `crates/driver/`
holds `actor.rs`, a `Mode::Standalone` and a `Divergence`, all three on §8's
phase-1a exclusion list by name, in `shim.md`, which the core loop's `docs`
does not include — so 3,249 lines exist against no oracle. Widening the list
would have made my number go up and made that invisible. It is `harness-007`
with no provisional choice taken, which is unusual and correct: the provisional
choices available were all less reversible than doing nothing loudly.

If a future campaign finds `harness-007` answered, the work is small and the
record says which. Do not close it by editing §8 first.

### Approaches considered and not taken

* **Renaming the heading "Branches exist for one commit at a time"**, which now
  contradicts its own body. The anchor is in `harness/section-baseline.toml`,
  denied to every loop, so a rename is permanent baseline drift and one
  `sections_missing` no campaign can clear. `hj baseline-drift` says "the
  documents match the baseline" today; keep it that way. Fixing the heading is
  a job for whoever retakes the baseline.
* **Adding a fifth progress term for a decision reconciled with no tagged
  site.** `decisions_reconciled` reads tag *removals* from the diff, so
  reconciling `harness-002` and `harness-003` — neither of which had a site,
  because the choice was in `.claude/` and `state/phase.toml` — scored nothing.
  The clean fix is to count a `decision:` trailer naming an already-*answered*
  record, which distinguishes reconciling from acting-under. I did not do it:
  changing how `progress` is computed is a metric redefinition and therefore
  Class B, and doing it in the campaign that benefits is the wrong shape. It
  is a good next escalation.
* **The loose reading of "test-only".** "Touched no Rust source" measures 71%
  against `core` over its last twenty commits, while that loop was closing
  gaps — its shape is a test carrying the claim plus the `design/` edit
  settling it. Strict ("touched Rust tests and nothing else") reads 7%. If a
  future campaign is tempted to loosen it because it never fires, that is the
  measure working; loosening it makes a flag that fires on correct work.
* **Making `hj escalations` a step the runner consults.** §6 says the loop
  "never idles waiting for an answer". It exits 1 when a batch is due so an
  operator's script can act, and `harness/loop` deliberately does not call it.
  That is the difference from `hj budget`, and it is in the docstring so the
  next person does not wire it up.

### Things worth knowing about the machinery

* **`tagged_sites` and `provisional_decisions` had drifted apart while one's
  docstring claimed they had not.** Same tell as `tokei` last campaign: the
  comment is where a previous campaign wrote the claim it meant to satisfy.
  The effect was that five answered decisions sat in loop prompts saying
  "reconcile and remove the tag" against `harness/hj`, a file two of those
  loops may not write. If two functions are documented as sharing a scope,
  make them share the function.
* **`{{other_findings}}` was computed in `prompt_values` and appeared in no
  template**, for the whole of phase 1a. Worth grepping the placeholder list
  against the templates occasionally: `grep -o "{{[a-z_]*}}"` on both sides.
  The reverse — a placeholder in a template with no value — fails loudly; this
  direction is silent.
* **A glob over `state/findings/` matches `core-1.md`, `core-2.md` and
  `core-3.md`.** Anything that means "per loop" must be driven by
  `phase.loops`, not by the directory. The same trap exists for
  `state/journal/`.
* `harness/hj selftest` went 57 → 68 this campaign, and the gate now runs the
  tree's copy as well as the pinned one ("harness/hj differs from the reviewed
  copy; checking this tree's own too"), so the step my last journal entry said
  had never run does now.
