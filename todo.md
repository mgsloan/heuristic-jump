* Pin the Claude Code version in `external-dependencies.md` alongside the
  language servers, when that file is written in phase 1c, and treat an
  upgrade as the intervention it is (`design/loops.md` §17). Each campaign
  already records the version it ran under, so the join exists; what is
  missing is the decision to treat a version change as one.

* The dashboard's cost panel is empty until `state/cost/<loop>.jsonl` exists.
  The join is `ccusage` against the session ids already in
  `state/sessions.jsonl`, run after the fact — a small script, not a
  redesign, and the cost-per-progress curve it produces turns up before the
  frontier visibly flattens.

* Prompts are the least validated artifact here. Expect to revise
  `harness/prompts/` during the first ten campaigns, and log each revision
  with `harness/hj intervene --kind prompt-revised` — a prompt change is the
  one intervention that cannot be replayed, so metrics either side of it are
  not strictly comparable and nothing downstream can detect that.

* Calibration is the first ten campaigns (`design/loops.md` §15). Watch three
  things: does the loop pick sensible targets, does it leave the tree green,
  and does `state/journal/conformance.md` accumulate anything a human would
  have wanted written down. If the third is no, the state file design is
  wrong and nothing downstream will save it.
