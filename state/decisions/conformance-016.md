---
id: conformance-016
status: open
opened: 2026-08-03T22:50:00+00:00
campaign: 51628b98-b5ea-48b1-bb77-696ecc51face
kind: harness-request
---

# `clippy.toml` denies the `unbounded()` that `deps.md` §2 and `shim.md` §2 require — which one gives?

## Context

Two design documents require unbounded channels. The lint configuration denies
them by name. No loop can edit the lint configuration.

`deps.md` §2, in full:

> `crossbeam-channel`, `unbounded()` everywhere, per `shim.md` §2.
>
> One thing to get right: `shim.md` §2 says unbounded because a bounded channel
> could stall a reader. That is correct but it means memory is bounded only by
> the shed-load rule in `shim.md` §10, so the `core` inbox length is a number we
> should log and watch, not just assert about.

`design/shim.md:173`:

> All channels are unbounded. A bounded channel would eventually make a reader
> [block]…

`clippy.toml:53`, in `disallowed_methods`, which `[workspace.lints.clippy]`
sets to `deny`:

> `{ path = "crossbeam_channel::unbounded", reason = "Unbounded channels hide
> backpressure and are usually where a recv_timeout/select was intended. Use
> bounded unless sender and receiver share a thread.", replacement =
> "crossbeam_channel::bounded" }`

These are not reconcilable by reading them more carefully. They give opposite
defaults and opposite reasons: §2 says a bound stalls a reader, `clippy.toml`
says the absence of one hides backpressure. Following §2 under the current
lint configuration means an `#[expect(clippy::disallowed_methods, reason =
"…")]` at every channel construction in the transport — which is `deps.md`
§15's own stated failure mode for a lint, quoted there about
`indexing_slicing`: "those files become a solid wall of `#[expect]`, which
turns a lint into decoration in the one place it would have mattered most."

**This is not yet blocking and that is why it is worth filing now.** The
transport channels are `shim.md`'s and belong to phase 2b. The only channels
that exist today are the two in `crates/driver/src/files.rs`, and they are
`bounded(1)` — with a comment that reasons the lint's way rather than §2's:

> Both are `bounded(1)` because at most one walk is ever outstanding —
> `Refresh::InFlight` is what guarantees it — and an unbounded channel here
> would hide the day that stops being true.

So the conflict will be discovered by whichever loop first builds the `core`
actor, mid-build, at the moment it is most expensive to stop and escalate.

Filed as `harness-request` rather than `class-b` because of who can act. The
substance is a design question — which default is right — but if the answer is
"§2 is right", the change is to `clippy.toml`, which `state/phase.toml` denies
to every loop in every phase. A loop cannot implement that answer even after
someone decides it.

## Options

**A — `clippy.toml` wins; amend §2 and `shim.md` §2 to require a bound with a
stated reason per channel.** Matches the one construction site that exists.
Backpressure stays visible, and `crossbeam`'s `Receiver::len()` — which
`deps.md` §1 keeps crossbeam partly *for*, so `shim.md` §10's "no heuristic
work while `core` is behind" rule can read it — becomes a bound to compare
against rather than an unbounded number to watch. Costs the property §2 names:
a full channel blocks its sender, and in the transport the senders are pipe
threads that must never block on anything but their own fd. Getting a capacity
wrong there is a deadlock, not a slowdown.

**B — §2 wins; remove or narrow the `clippy.toml` entry.** Matches the two
design documents. The reader-stall hazard is real and structural, and §2 already
names the mitigation — `shim.md` §10's shed-load rule, plus logging the `core`
inbox depth. Costs the lint on every *other* channel, including ones where the
`bounded(1)` reasoning in `files.rs` is exactly right. Narrowing rather than
removing (allow it in the transport only) is not expressible in
`clippy.toml`, which is path-based and not location-based — it would have to be
a per-crate `[lints]` override, and `driver` holds both kinds of channel.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A**, by doing nothing: `files.rs` already uses `bounded(1)` with a
per-channel reason, and this campaign builds no channels.

It is the more reversible because it is the one that compiles. Under B the
tree stays green either way — a `bounded` call is not denied by anything — so
choosing A now costs at most the two capacities in `files.rs`, which have a
stated justification that survives B anyway. Choosing B now would mean writing
`#[expect]` at each site and deleting them again if the answer goes the other
way.

Nothing is tagged, because nothing provisional exists in the code yet: the
channels the conflict is about are not written. That is the point of filing
early.

## Consequences

If the answer is B: one `clippy.toml` edit, which no loop can make, and an
amendment to §2 recording that the entry was narrowed and why. No code
changes — `files.rs`'s `bounded(1)` remains correct under B, since B permits
unbounded rather than requiring it.

If the answer is A: `deps.md` §2 and `shim.md`'s "All channels are unbounded"
both need rewriting, and `shim.md` §10's shed-load rule needs re-reading
against a bound it can now exceed — the rule currently assumes the inbox grows
rather than blocks, so a capacity is a second place the shim can wedge.

The cost of *not* deciding is paid by the phase-2b loop, which finds it while
building the `core` actor rather than before.
