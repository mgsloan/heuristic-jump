---
id: conformance-010
status: accepted
opened: 2026-08-03T09:05:00+00:00
campaign: ff3e1a40-5639-4c57-ac81-66ea1144762f
kind: class-b
---

# Does `measure collect` get a TOML parser, or keep hand-reading `servers.toml`?

## Context

`core.md` §7's command line requires `--server <name>` to be "resolved through
`servers.toml`, which carries its command and pinned version", and gives the
reason: "Naming a server rather than passing a command line is what lets the
provenance header record what was actually run without trusting the invocation
to be repeated correctly."

`servers.toml` is TOML, and there is no TOML parser in the workspace.
`deps.md` §0's table does not list one, in either the chosen or the rejected
column — the file postdates the dependency review. `CLAUDE.md` makes the
dependency set a standing escalation ("Do not add dependencies unprompted…
If a new crate seems necessary, ask"), and `deps.md` §13 is a standing Class B
trigger, so this cannot be settled inside a campaign.

The trade is real in both directions, which is what makes it Class B rather
than Class A. A parser is one more crate in a graph whose count `deps.md` §11
treats as a cost worth measuring; hand-reading is code we own and can get
wrong, on a file that names the oracle every language is scored against.

## Options

**Take `toml` (or `toml_edit`'s read half).** Correct by construction on any
valid TOML, including the shapes `servers.toml` does not use today and might
tomorrow — inline tables, multi-line strings, dotted keys. Costs `toml`,
`toml_datetime`, `toml_write` and `serde_spanned` on the current release; none
is heavy and none is a build-script crate. It also means `servers.toml` can be
`Deserialize`d into a struct, which is where §8.1's argument — the newtypes are
what deserialization produces — would apply if it applied here.

**Hand-read the two shapes the manifest is documented to have.** Top-level
`key = "value"` and a `[[server]]` array of tables, with `#` comments and
`${servers}` expansion; anything else is refused by line number rather than
guessed at. No new crate. The cost is that a manifest a human writes in valid
TOML that this does not accept fails a hundred-hour collection run at minute
zero with a line number — which is loud, but is a failure the parser would not
have had. It also cannot be `Deserialize`d, so the shape is checked in code.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

The hand-read reader, because it is the reversible one: it is confined to a
private `manifest` module inside `crates/measure_core/src/corpus.rs` behind
`resolve_server`, so answering this the other way is deleting that module and
deriving `Deserialize` on two structs, with no call site changing.

The reverse is not reversible in the same sense — a dependency taken and then
removed still moves `Cargo.lock`, and `deps.md`'s value is that every entry is
a choice somebody made rather than one that arrived.

Tagged at:

- `crates/measure_core/src/corpus.rs`, on the `manifest` module
  (`DECISION-conformance-010: provisional`).

## Consequences

If the answer is `toml`: delete the `manifest` module (~90 lines), add the
crate to `[workspace.dependencies]` and to `deps.md` §0's table with its
reason, and derive `Deserialize` for the two shapes. `resolve_server`'s
signature does not change, and no test does either — the reader has no test of
its own precisely because `servers.toml` is outside every loop's write list
and there is no fixture to point one at.

If the answer is to keep hand-reading: the module wants a fixture the day
`servers.toml` exists, since today nothing exercises it, and that is a
follow-up campaign rather than part of this one.

## Answer — 2026-08-03T19:00:32+00:00

**Ruling:** accepted

Option A.

**Rationale:** The hand-reader is already broken against the manifest that exists: it accepts only `[[server]]` and servers.toml uses `[server.rust-analyzer]`. Neither party did anything unreasonable, which is the point -- the two shapes are both valid TOML and they will keep diverging as the manifest grows. servers.toml names the oracle every language is scored against, and its failure mode is refusing a valid file at minute zero of a hundred-hour collection run. Four crates on a 170-crate lock, none heavy, none with a build script, is a small price against that. It also puts servers.toml on the Deserialize path core.md 8.1 already argues for, where the newtypes are what deserialization produces.

Reconciling the sites tagged `// DECISION-conformance-010: provisional` is a
normal campaign target, not an interrupt.
