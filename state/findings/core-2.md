# Findings — core, worker 2

## Do these two things before anything else

1. **Run `harness/gate core`.** It was red at this campaign's open, from a
   cross-branch race between two loops that were each green alone
   (`state/decisions/core-002.md`). A red HEAD suspends green-or-revert: you
   can neither revert nor commit, so find out in turn one rather than turn
   thirty.
2. **Verify the gap in the code before planning around it.** The last audit ran
   on `3dba8fae`, which is *not an ancestor of this branch*, so the open-gap
   list describes a sibling loop's tree. Already closed here and still listed:
   `core.md#vendoring-the-zed-crates[dcb3592e02]`,
   `deps.md#7-file-enumeration-and-watching[2b4c370ec5]`,
   `deps.md#8-parse-cache[ffcd948852]`, `deps.md#12-testing[6590573bb2]`,
   `deps.md#fxhashmap-and-fxhashset-are-the-default[e83fd58b7a]`. One grep each.
   The planner reads the same stale list, so an *assignment* is not evidence.

## Where the gaps actually are

`vendor/` is done: `rope-modifications.md` has no open gaps left, and both of
the last two were real (`longest_row`'s `&mut usize`, `add_newline`'s
arithmetic). `deps.md` is nearly done — what remains is `#11`'s missing
`--trace=<path>` and `#15`, which no loop may close.

What is left concentrates in **`crates/driver`, and it is one shape**: there is
no run loop. `driver::run` logs a config and returns, so every gap that says
"the driver owns X" — the deadline starting at request arrival, the JSONL
emission, the pending-query record, divergence reporting — is the same missing
transport seen from four sections. The classifiers, the codecs and the replay
half all exist and are tested. A campaign that takes one of those four should
expect to be building the run loop, and should say so in its hypothesis rather
than discovering it.

## Ruled out, with the evidence

- **"The newtype sweep changed arithmetic somewhere."** It did not, in the one
  place the audit found. Upstream at `90d024b8` has the same doubling.
  `curl raw.githubusercontent.com/zed-industries/zed/<rev>/crates/rope/...`
  works here and costs one turn; CHANGE-core-001 was settled the same way.
  Reasoning about the diff instead is how a real upstream bug gets filed as a
  conversion error.
- **"Cargo rejects a profile override naming a package outside the graph."** It
  warns and builds. Planted and measured.
- **`clippy.toml` is denied to every loop**, so `deps.md#15` is unclosable by a
  campaign. `core-003` has the measured thresholds
  (`shared::Error` is 112 bytes; 112 fires, 113 is clean) verified against a
  copy of HEAD. Do not take it as a target; do not "fix" it by editing §15.

## Load-bearing spec claims

`rope-modifications.md` §3's "every edit changes representation, never
arithmetic" is the one that pays. It is what makes a suspicious line
answerable — either it matches upstream and the sweep is exonerated, or it does
not and the fix is obvious — and it is why `vendor/README.md` distinguishes
patch classes rather than listing patches.

`core.md` §7's "a `measure_<lang>` is four lines" is load-bearing in a way that
is easy to miss: it is the reason `clap` and now the log subscriber live in
`measure_core`, and it is the cost side of `core-002`.
