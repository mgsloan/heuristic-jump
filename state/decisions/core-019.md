---
id: core-019
status: open
opened: 2026-08-04T05:20:00+00:00
campaign: ede3701b-ff0b-4c95-b4f9-6d12c6bb0c84
kind: harness-request
---

# Should a gap handed to a campaign say when it was found, so a closed one stops reading as open?

## Context

The audit re-judges a handful of sections per run, and `state/audit/core.toml`
records `last_audited` per section. What the prompt renders is the union of
every section still in `state = "gaps"`, with no indication of how old each
one is — so a gap opened at 20:56 and closed by a commit at 21:22 is handed to
the next three campaigns as a target.

This is not a hypothesis. Measured over the seventeen open gaps this campaign
was shown, comparing each gap's `last_audited` against the newest commit
touching the file its `where:` names:

| gap | audited | `where:` file last moved |
|---|---|---|
| `#5-deadlines-and-abstention[f0a42a21e1]` | 20:56 | 01:08 (+4h) |
| `#7-observability[bd3003d0fb]` | 00:27 | 01:08 (+41m) |
| `#two-modes-collect-and-replay[f2e74dce26, eb424449b6]` | 20:56 | 21:18 (+22m) |
| `#the-oracle-is-the-server-being-proxied[eb6f4618da]` | 19:21 | 19:30 (+9m) |
| `#what-the-templates-handler-does[9adb0be268]` | 20:56 | 21:22 (+26m) |

Seven of the nine `core.md` gaps. Three were verified closed by reading the
code — `#what-the-templates-handler-does[9adb0be268]` this campaign
(`Table::template()` and its test landed 26 minutes after the audit),
`#the-oracle-is-the-server-being-proxied[eb6f4618da]` (`ServerId::from_command`
is called from `driver/src/config.rs:172` and `driver/tests/oracle.rs` pins it
against `servers.toml`), and `#7-observability[bd3003d0fb]`'s "nothing in
`crates/driver` emits one" (`driver/src/trace.rs` does, from `actor.rs`). The
previous campaign on this worker found three more the same way and wrote
"check `last_audited` before you claim" into its findings as the single most
useful thing it had learned.

The cost is not only wasted campaigns. The planner divides a round using this
list, so a stale gap consumes an assignment slot; and the two gaps this
campaign found that *were* fresh were both already claimed by other workers,
leaving nothing takeable outside its own assignment. A list that is mostly
stale makes the freshest items scarce in exactly the way that looks like
contention.

## Options

**Re-derive the gap list against head before rendering a prompt.** The most
correct and the most expensive: deciding whether a gap is closed is judging
the section, which is the audit's whole job and costs what an audit costs.
This is not really an option — it is asking for an audit on every campaign
open.

**Stamp each gap with the commit it was found at, and render that.** Purely
mechanical, no judgement: `audit-merge` already knows `HEAD` when it writes a
verdict, so each gap gains a `found_at` sha. The prompt then renders
`found at abc1234, 19 commits ago` and — cheaper still, and the signal that
actually discriminates — marks a gap whose `where:` file has changed since
`found_at` as **possibly closed, verify first**. That is `git log -1 -- <path>`
per gap, which is what the table above is, and it needs nobody to judge
anything. It cannot say a gap *is* closed; it can say which ones are worth one
turn of checking, which is the whole difference between a target and a
verification exercise.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Every campaign reads `state/audit/core.toml` itself before claiming, and
compares `last_audited` against `git log` for the files its candidate gaps
name. One `python3` over the file covers all of them, so it costs one turn and
it is the first turn of the campaign.

No sites are tagged: nothing in `crates/` is provisional and the choice is
entirely about what a prompt renders. It is the most reversible option because
it is what a campaign already has to do — the previous campaign on this worker
arrived at it independently, which is the recurrence the `harness-request`
kind exists to make visible.

## Consequences

If the answer is "stamp them", the work is in `harness/hj audit-merge` and the
prompt template, and nothing in `crates/` or `design/` changes. Campaigns stop
spending their first turn on it, and the findings files stop carrying a
paragraph about it — which is a real cost of the status quo, since that
paragraph is spending a 512-word cap on a harness defect rather than on a
theory of the implementation.

If the answer is "leave it", the thing to watch is a campaign that closes
`no-movement` after verifying three stale gaps. That is not the campaign
failing; it is the list, and the metrics row will not say so.
