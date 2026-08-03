---
id: conformance-002
status: open
opened: 2026-08-03T04:10:00+00:00
campaign: b59733c6-ebff-47a4-bccf-232abc532a07
kind: harness-request
---

# Who owns the root files `deps.md` §14 requires — `rust-toolchain.toml` and the `LICENSE-*` texts?

## Context

`deps.md` §14 specifies the workspace root as:

```
Cargo.toml
rust-toolchain.toml     pin 1.95.0, so grammar/rope behaviour is reproducible
LICENSE-MIT             covers crates/*
LICENSE-GPL             covers the combined binary, via vendor/rope
LICENSE-APACHE          covers vendor/sum_tree
                        -- all three symlinked into each crate, see above
```

and is emphatic about the symlinks: "Zed does this without exception — 245
symlinks and not one regular copy", because N copies of a license text drift
and a stale one is a licensing problem rather than a formatting one.

The conformance loop's `write` list grants `Cargo.toml`, `Cargo.lock`,
`rustfmt.toml`, `.gitignore`, `readme.md`, `vendor/**`, `crates/*/**`,
`design/**` and its own `state/` paths. It grants no other root file, so
`rust-toolchain.toml`, `LICENSE-MIT`, `LICENSE-GPL` and `LICENSE-APACHE` are
all outside it and `check-scope` rejects the commit that creates them.

This is not obviously an oversight. A toolchain pin changes what every
campaign's measurements mean, and licensing is named in the prompt as a
standing Class B trigger ("licensing or `vendor/`"), so there is a reading
under which both are deliberately a human's to place. The problem is that
under that reading nobody has placed them, and `vendor/rope` cannot be
vendored without `LICENSE-GPL` existing for its symlink to resolve — the
crate arrives from Zed already carrying `LICENSE-GPL -> ../../LICENSE-GPL`,
and `deps.md` §14 notes the symlink resolves after the copy *provided the
copy preserves it*, which makes the root file a precondition of the vendoring
campaign rather than a nicety.

## Options

1. **Add the four paths to the loop's `write` list.** Cheapest, and the
   toolchain pin is then a value the loop can move — which is the objection,
   since a silent edition or version bump would change every latency number
   without an intervention row to join it to.
2. **Grant `LICENSE-*` only, and leave `rust-toolchain.toml` to a human.**
   Keeps the pin an intervention, and unblocks vendoring. The license texts
   are verbatim upstream documents; a loop that alters one is doing something
   a diff makes immediately obvious, and `cargo-deny` (also §14) is the
   check that would catch a license *fact* changing.
3. **A human places all four before the vendoring campaign.** Nothing changes
   in the ownership table, and the loop waits — but "never idle waiting for
   an answer" is the standing instruction, so in practice this means the
   vendoring campaign fails at step 4 and reverts.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

Neither file was created. The toolchain pin is carried instead by
`rust-version = "1.95.0"` in `[workspace.package]`, which is inside
`Cargo.toml` and therefore owned, and which cargo enforces as a floor — it
refuses to build with an older toolchain. It is genuinely weaker than
`rust-toolchain.toml`: it sets a minimum rather than selecting a toolchain,
so a newer rustc still builds and reproducibility is not what it buys.

The per-crate `license = "MIT"` fields are written, so the manifests are
already correct; only the texts they point at are missing.

Tagged site: `Cargo.toml`, `[workspace.package]`, `// DECISION-conformance-002:
provisional` as a TOML comment beside `rust-version`.

This is the most reversible option because it leaves no file to delete. If
the answer is option 1 or 2, a later campaign adds the four paths and the
symlinks in one commit, and the only edit to existing work is removing the
comment.

## Consequences

`core.md#vendoring-the-zed-crates` cannot be taken as a target until this is
answered or the licence texts appear: the copy either arrives with a dangling
symlink or has to be de-symlinked, and de-symlinking is the exact failure
mode `deps.md` §14 spends a paragraph warning about (`cp -r` instead of
`cp -a`). A campaign that hits this without reading here will most likely
reach for the plain copy, which passes every check and silently loses the
property on the first re-sync.
