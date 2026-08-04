---
id: core-018
status: accepted
opened: 2026-08-04T04:30:00+00:00
campaign: 2c129b10-41f7-4292-a1f5-4e31ed08b7ea
kind: class-b
---

# Is §8.5's golden-corpus condition met by captured *server* traffic alone, when the client half needs an editor no loop can run?

## Context

`core.md` §8.5 makes a golden corpus one of the two conditions on which
dropping `lsp-types` is acceptable, and it names both ends of the wire:

> **A golden corpus.** Real `initialize` / `InitializeResult` pairs and
> document traffic captured from Zed and VS Code against rust-analyzer,
> pyright, and gopls, checked in, and asserted against.

The server half of that is now real. `crates/shared/tests/golden-traffic.jsonl`
holds captured `InitializeResult`, definition answers and `$/progress` from
rust-analyzer 1.95.0, gopls v0.23.0 and pyright 1.1.411, and
`the_captured_half_covers_every_server_the_section_names` fails if any of the
three goes missing. Capturing them needed nothing but a stdio pipe.

The client half cannot be obtained the same way, and the reason is structural
rather than a matter of effort. `initialize` params, `didOpen`, `didChange`,
`didSave` and `didClose` are *composed by the editor*. There is no way to
elicit one from a server, so capturing them means running Zed or VS Code
against a recording binary and driving it by hand. Zed is installed on this
machine, but only as a GUI application on the user's own display: starting it
opens a window on a desktop somebody is using, and producing `didChange`
traffic means typing into it.

This matters more than it sounds, because the untagged union §8.5 spends its
longest passage on is `contentChanges` — the one whose failure "replaces the
entire document with the few characters the user just typed". Its real traffic
is exactly the half that cannot be captured here. Every `didChange` in the
corpus is hand-authored, which is the population §8.6 says is not the long
tail: it holds the fields somebody thought of.

The gap between "captured from three servers" and "captured from two editors
against three servers" is therefore not a rounding error, and closing the
section over it is a judgement about how much of the mitigation the dependency
decision in `deps.md` §3 actually rested on.

## Options

**(a) The server half is the condition; the editor half is a standing want.**
§8.5 is judged against what a loop can produce, the corpus header records what
is still missing, and `contentChanges` stays covered by hand-authored messages
plus the negative tests §8.5 also asks for. Costs: the union with the worst
failure mode in the design keeps a hand-authored corpus indefinitely, and the
"real traffic" claim is quietly weaker than the section's words.

**(b) The section stays open until a human captures editor traffic.** One
session at a desktop with a recording wrapper in front of rust-analyzer —
Zed's `lsp.<server>.binary.path` setting points at a script that tees stdin —
produces the whole client half in a few minutes, including `contentChanges`
from real typing. Costs: `core.md` §8.5 cannot go clean until somebody does
it, and the number that scores this loop is blocked on work outside it.

## Decision

**(a) now, (b) when convenient**, answered 2026-08-04. §8.5 is judged on the
server half, which a loop can produce; the corpus header records what is
missing; `contentChanges` stays covered by hand-authored messages and the
negative tests §8.5 also asks for.

The honest cost is stated rather than softened: the union with the worst
failure mode in the design keeps a hand-authored corpus until someone captures
the other half, and `TextDocumentContentChangeEvent` — full-replace against
incremental, distinguished by a field being *absent* — is exactly the shape
§8.5 exists to catch on traffic nobody imagined.

**"When convenient" is where an item like this dies, so it is not left as an
intention.** `harness/capture-editor-traffic` is written: `--install` prints
the Zed setting to paste, the script tees both directions of the real server,
and `--finish` folds the capture into `golden-traffic.jsonl`, keeping payloads
verbatim and de-duplicating by *shape* so a minute of typing contributes one
row per message shape rather than hundreds. The remaining step is two minutes
at a desktop, not a project.

Nothing in the loops will raise this again, which is the point of leaving the
tooling rather than a note.

## Provisional choice in force

**(a)**, because it is the reversible one: a captured line is appended to
`golden-traffic.jsonl` and nothing in `differential.rs` knows where a line came
from, so option (b) can be satisfied later without retracting anything done
under (a). The reverse is not true — holding the section open produces no
artifact at all.

Tagged at:

- `crates/shared/tests/golden-traffic.jsonl`, in the header, which names this
  record as what is still wanted.
- `crates/shared/tests/differential.rs`, on
  `the_corpus_holds_traffic_nobody_here_composed`, whose three required kinds
  are the server-to-client ones precisely because the client half has no
  producer here.

No production code is tagged. Nothing in `shared::proto` changes either way —
what is at issue is the evidence behind it, not its shape.

## Consequences

If the answer is (b), the work is a capture session and then appending lines;
no code is redone and no test changes except the required-kinds list in
`the_corpus_holds_traffic_nobody_here_composed` growing to include `didChange`
and `initializeParams`. If it is (a), the one thing worth adding is a note in
`deps.md` §3 that the `lsp-types` decision rests on a corpus whose client half
is hand-authored, so a future re-reading of that decision is not misled about
what was checked.
