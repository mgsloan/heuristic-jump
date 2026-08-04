---
id: core-021
status: duplicate
opened: 2026-08-04T20:20:00+00:00
campaign: 5cc94daa-a1bb-476a-9255-e10177487c15
kind: harness-request
---

# `deps.md` §14 asks for a `cargo-deny` config, and no loop can write one or run one — who provides the licence check?

## Context

`deps.md` §14 closes with the only claim in the section that is about the
resolved dependency graph rather than about a file in this repository:

> A `cargo-deny` config asserting that `GPL` reaches the graph only through
> `vendor/rope` and `crates/similarity` is worth having from the start. Those
> two are the ruled-on inputs (§5); the check is what notices a *third*
> arriving without anyone deciding, which is how a licence surface grows — not
> by a decision but by a dependency.

The claim is right and the gap it names is real. Every licensing check in
`crates/driver/tests/seam.rs` — `every_member_declares_the_licence_section_5_assigns_it`,
`the_gpl_inputs_are_the_two_the_documents_name`,
`the_licence_text_is_symlinked_into_every_member` — reads manifests under
`crates/*` and `vendor/*`. None of them can see a third-party crate, because a
third-party crate's manifest is in the registry cache and not in this
repository. The surface that grows "by a dependency" is precisely the one
nothing here reads.

Two separate obstacles stop a loop implementing it as written, and they are
independent:

* **`deny.toml` is outside every loop's owned paths.** Measured, not assumed:

  ```
  $ printf '# probe\n' > deny.toml && harness/hj check-scope core
  scope: deny.toml: outside core's owned paths
  ```

  The obvious workaround — `[workspace.metadata.deny]` in `Cargo.toml`, which
  *is* owned — is a guess about cargo-deny's config resolution that I cannot
  check offline, and `CLAUDE.md` treats a false claim about a dependency's API
  as a Class A defect. `cargo-deny` reads `deny.toml` from the workspace root
  or a path given with `--config`; there is no evidence it reads workspace
  metadata, and writing a config in a shape the tool ignores produces a file
  that satisfies an auditor and checks nothing.

* **`cargo-deny` is not installed** (`~/.cargo/bin` has no `cargo-deny`), and
  nothing in `harness/gate` invokes it. So even with the file in place, the
  check would not run — which is worse than no check, because the section
  reads as satisfied while a third input arrives unnoticed. That is the exact
  failure the paragraph is about, one level up.

This is filed as `harness-request` rather than `class-b` because nothing is
traded off. The property §14 wants is not in dispute; what is in dispute is
who owns the file that asserts it and what runs it.

## Options

**Grant `deny.toml` to one loop and install `cargo-deny` in the gate.**
Faithful to §14, and the tool is better at this than anything written here:
it understands SPDX expressions properly, distinguishes `license-file`
crates, and has an exception list with a reason field per entry. The costs are
a binary on the gate's critical path (`cargo deny check licenses` is a few
seconds), a fifth thing the gate can be red for, and a config file whose
ownership has to be decided — it is not obviously `core`'s, since a licence
surface is a project-level fact and `conformance` and the language loops add
dependencies too.

**Assert the property from the seam suite instead, and leave §14's sentence
describing an aspiration.** `cargo metadata --format-version 1 --offline`
carries `license` for every package in the graph, takes about 0.1s, and needs
no network. The check then runs on every gate for free, in a file `core`
already owns. What it costs is that the SPDX handling is ours and is crude —
it splits on `OR` and looks for `GPL`, so it is right about
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` (which `r-efi` really carries in this
graph) and would be wrong about an expression with nested parentheses that
cargo-deny would parse exactly. It errs toward flagging, which is the safe
direction for this question but means a false alarm is possible, and a false
alarm on a licensing check is the kind somebody silences.

## Decision

**Closed as a duplicate of `core-023`**, which carries the answer. Logged as a
`decision-answered` intervention on 2026-08-04 so §16's status derives from the
log rather than from this line.

This record and `core-023` are the same question, raised independently by
different workers of round 1 because the claim system was granting every request
— `campaign_is_alive` asked the process table, which the OS sandbox had made
private, so from inside any campaign every live sibling read as dead and no
claim was ever refused (fixed in `c047b4c`). Campaign `5cc94daa` wrote this
one.

Nothing here is wrong and nothing is discarded: the framing differs and the
reasoning is worth keeping, which is why this is closed rather than deleted. The
ruling, its argument, and the work it leaves are in `core-023`.


## Provisional choice in force

**Option two**, implemented as `no_third_copyleft_input_reaches_the_dependency_graph`
in `crates/driver/tests/seam.rs`, tagged `// DECISION-core-021: provisional`.

It is the more reversible one by a wide margin: it adds no dependency, no
binary, and no gate step, and if the answer is "install cargo-deny" the test
is deleted in one commit and the property is unchanged in the meantime. The
opposite order is not available — writing the config first means the property
goes unasserted until somebody installs the tool.

The assertion is deliberately *derived* rather than listed. The copyleft
packages in the resolved graph must be exactly the copyleft workspace members,
and which members those are is pinned to §5's table by
`the_gpl_inputs_are_the_two_the_documents_name`. So the two tests do not carry
two copies of the same list, and together they say the graph's copyleft
surface is `vendor/rope`, `crates/similarity`, and what §5's dependency rule
marks downstream. A control run confirms the third-party branch fires: with
the predicate flipped to `Zlib`, it reports `["foldhash = Zlib"]`.

`design/deps.md` is **not** edited. The section still asks for a cargo-deny
config, which is the honest state — the code does not yet do what it says, and
moving the sentence toward the code would remove the gap from the instrument
rather than close it.

## Consequences

If the answer is "install cargo-deny and grant the path", the seam test is
deleted, `deny.toml` is written by whoever owns it, and the gate grows a step.
About 120 lines of test come out; nothing in `crates/` or `design/` changes
shape, and no other work is redone.

If the answer is "leave it", the thing to watch is the crude SPDX predicate.
The first false alarm will be a permissively-licensed crate that ships only a
`license-file`, which this test flags as unreadable on purpose. If that
happens twice, the answer has changed and this record is the evidence.
