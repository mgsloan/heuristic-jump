## `harness/` is yours, and this is the one way you can destroy this project

Every other loop is denied `harness/` outright, for one reason: **a loop must
not be able to weaken the thing that scores it.** You have write access
anyway, because the phase-2 machinery lives there and building it is your job.
The reason did not go away when the permission arrived.

So read this once, carefully, because it is the only rule here you cannot
recover from breaking.

**You are not judged by the harness in your worktree.** `harness/gate` runs
from the reviewed copy on the integration branch, pointed at your tree through
`HJ_REPO`. Editing the gate in front of you does not change the gate that
grades you. This is not a trick played on you — it is stated here so you never
waste a campaign discovering it, and so you know that the cheap way out is
closed before you are tempted by it at turn ninety with a red tree.

**What counts as cheating.** Not a vibe; these are the specific shapes:

* Deleting, disabling, `#[ignore]`-ing, or narrowing the scope of a check so
  that failing work passes.
* Loosening a ratchet, a threshold, or a baseline so the current number
  clears it.
* Making `check-scope`, `check-audit`, `check-links` or `check-metrics` accept
  what they were built to reject.
* Editing the auditor's prompt, the section baseline, or the gap ledger so the
  score moves without the code moving.
* Anything whose honest one-line description is "the check was in my way."

The test is a question you can always answer: **did the work get better, or
did the judge get quieter?** If it is the second, you are cheating, and the
fact that you can do it is exactly why it is written down.

**What is legitimate, and welcome.** The gate has real bugs and real gaps, and
you are the loop best placed to find them:

* A check that rejects something correct — a false positive — is a bug. Fix
  it, and add a test that would have caught it.
* A claim in a design document that could become a mechanical check is the
  highest-value work available to you. Converting a judgement into an exact
  check is worth more than any number of small features.
* Making the gate faster, clearer, or better at saying *why* it failed costs
  nothing and helps every campaign after you.

The asymmetry is the thing to hold onto: **adding a check is ordinary work;
removing or loosening one is a Class B escalation, always, no exceptions.**
Write the decision record, pick the reversible option, and continue. Never
loosen a check and mention it in a commit message as though it were a
refactor.

**If you break the harness, both loops stop.** The conformance loop runs
against the same tools from the same branch, and it cannot fix them — it is
denied `harness/` and will simply fail, campaign after campaign, until a human
notices. Green-or-revert therefore binds harder for you than for anyone else:
a broken gate does not fail loudly in your session, it fails silently in
someone else's, hours later.

Two specific things to be careful with, because they are shared and load
bearing:

* **`harness/adapter`** is the only file that knows the runner's output
  shapes. Every loop's outcome, cost and transcript flows through it. A change
  here that looks fine and is subtly wrong misrecords campaigns rather than
  crashing.
* **`harness/hj`'s metric code** — `record`, `check-metrics`, `audit-merge`,
  the gap ledger — defines what progress *means*. A change to how a number is
  computed is a metric redefinition: it invalidates comparability across the
  change, so it is Class B and it needs saying plainly in the record.
