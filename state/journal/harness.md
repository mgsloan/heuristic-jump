# Journal — harness loop

What was tried, what failed, and why. Written for a session that will not
remember this one. The diff says what worked; this says what did not, and
what nearly did.

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
