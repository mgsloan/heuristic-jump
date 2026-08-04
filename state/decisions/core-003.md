---
id: core-003
status: accepted
opened: 2026-08-04T06:55:00+00:00
campaign: a9937015-4ddb-46e6-a1aa-f85ab25f09ef
kind: harness-request
---

# Who writes the two `clippy.toml` thresholds `deps.md` §15 says are tuned there?

## Context

`deps.md` §15's clippy block carries this comment above two denies:

> `Error` nests nine sub-enums carrying PathBuf/DocumentUri/Offset, and sits
> in the Err of every Result on the hot path. **Threshold tuned in
> clippy.toml.**
> ```
> result_large_err = "deny"
> large_enum_variant = "deny"
> ```

`clippy.toml` sets `msrv`, `avoid-breaking-exported-api` and six
`allow-*-in-tests` keys, and no threshold at all — neither
`large-error-threshold` nor `enum-variant-size-threshold`. Both lints run at
clippy's defaults (128 and 200 bytes), so the sentence describes a tuning that
was never done, and `state/audit/deps.toml` holds it open as
`deps.md#15-clippy-in-workspace-toml[dff6d582b4]`.

**`clippy.toml` is in this loop's denied list, and in every loop's.** So the
gap cannot be closed by any campaign: the only two moves available to a loop
are to edit `clippy.toml`, which is denied, or to delete the sentence from
`deps.md` §15, which is moving the spec toward the code in the one way the
audit cannot catch. That is what makes this a `harness-request` rather than
work somebody should pick up.

The measurement is done, so what is left is one human edit. `shared::Error` is
**112 bytes** today, and so is `Result<(), shared::Error>`.

## Options

**A — `large-error-threshold = 113`, `enum-variant-size-threshold = 112`.**
Zero headroom: the lint fires the moment `Error` grows past what it is now,
which is the tuning the sentence implies for a type that "sits in the Err of
every Result on the hot path". Verified: with these two keys,
`cargo clippy -p shared -p driver -p measure_core -p heuristic_jump
-p lang_rust -p measure_rust -p similarity --all-targets -- -D warnings` is
clean, run against a copy of `HEAD` in `/tmp` so that nothing denied was
edited. The threshold is inclusive — 112 fires on the current type, 113 does
not, which is worth knowing before someone rounds it.

Costs: a variant that legitimately grows `Error` fails the build until someone
raises the number. That is the point, but it is a tax, and it is paid by
whoever adds the variant rather than by whoever set the threshold.

**B — `large-error-threshold = 128`, `enum-variant-size-threshold = 200`.**
Clippy's defaults, written down. Changes no behaviour and makes the sentence
true in the weakest sense: the threshold is stated rather than inherited, so a
clippy release that changes its default does not silently change ours. Costs:
`Error` can grow by 16 bytes with nothing noticing, which is most of a
`PathBuf`.

## Decision

**accepted: Option A — large-error-threshold = 113,
enum-variant-size-threshold = 112**, answered 2026-08-04 and logged as a
`decision-answered` intervention, which is what makes it answered —
`design/loops.md` §16 derives the status from the log rather than from this
line.

deps.md §15 says the thresholds are tuned and they never were; the sentence
has to become true or go, and it should become true. Zero headroom is what
'tuned' means for a type that sits in the Err of every hot-path Result:
shared::Error is 112 bytes and the lint should fire the moment it is not. The
campaign verified clippy clean across all seven crates with exactly these two
keys. The tax — a variant that grows Error fails the build until someone
raises the number — is the mechanism working, and the number is in git where
the raise is visible in a diff. Option B writes down a default that lets Error
grow most of a PathBuf unnoticed, which is the state we are already in.
clippy.toml is denied to every loop, so this edit is made by hand and logged
here.

### What is left

Done in the same commit as this ruling: `clippy.toml` carries both keys, since
it is denied to every loop and there is nobody else to write them.

## Provisional choice in force

**Neither, and nothing is tagged**, because the file the change goes in cannot
be written by a loop. The repository is unchanged by this record; `deps.md`
§15's sentence stands as written and the gap stays open, which is the honest
state rather than a hidden one.

What a campaign should *not* do meanwhile — and this is the part worth
carrying — is close this by editing §15. The sentence is not wrong about what
it wants; the configuration is missing.

## Consequences

Whichever option is taken, the same edit should carry the check the auditor
asked for, since the two belong together and neither is useful alone:

> Extending `crates/driver/tests/seam.rs`'s §15 comparison to assert that every
> `clippy.toml` key a §15 comment names actually exists would close this.

That check cannot be added before the keys are, because it would fail on the
day it landed. So it is written here rather than committed: a scan of §15's
prose for `` `key` `` spans naming a `clippy.toml` key, asserted present in
`clippy.toml`. Today it would find exactly the two thresholds above, and the
existing §15 test — which compares lint names and levels only — passes without
noticing either.

If neither option is taken, say so and the sentence should go, but that is a
human deleting a claim rather than a loop deleting evidence of its own gap.
