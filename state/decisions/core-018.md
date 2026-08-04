---
id: core-018
status: open
opened: 2026-08-04T05:24:00+00:00
campaign: 44773a93-738f-4dd6-8ca1-fa951465ac44
kind: harness-request
---

# Where does the golden-corpus capture tooling live, when it is not Rust and not the harness?

## Context

`core.md` §8.5 makes a captured corpus one of the two conditions on which
dropping `lsp-types` is acceptable: "real `initialize` / `InitializeResult`
pairs and document traffic captured from Zed and VS Code against
rust-analyzer, pyright, and gopls, checked in, and asserted against". Growing
that corpus is not a one-off. This campaign added seventeen lines and the
corpus is still missing whole servers, whole message kinds a server sends
unsolicited, and every editor other than the one that runs headless.

Capturing needs two programs that are not part of the build: a stdio LSP
client for the server half, and a recording proxy plus an editor driver for
the client half. Neither is Rust — the client half in particular is Emacs Lisp
driving eglot, because eglot is the LSP client that runs under `--batch`.

They currently live in `/tmp`, and the cost of that is measurable. This
campaign found the previous campaign's scripts still in `/tmp/hj-capture`,
unlanded, from a session that captured gopls and pyright and never committed
the lines. It saved perhaps ten turns and it was luck; a reboot would have
cost them. Writing the *recipe* into the corpus header is what survived, and
a recipe is strictly worse than a script — this campaign lost one run to
`eglot-ensure` deferring to `post-command-hook` (which never fires under
`--batch`) and another to eglot coalescing three buffer edits into one
notification, both of which a checked-in script would have carried.

The loop cannot resolve this itself: `harness/` is denied to every loop, which
is where a non-build program would obviously go.

## Options

* **`harness/capture/`.** Where it belongs by kind: it is tooling that
  produces an input to the tests, like the corpus scan's own scaffolding.
  Costs a human to create it, and puts a Python and an Emacs Lisp file in a
  tree that is otherwise the loop's scoring machinery — which is exactly the
  tree loops may not touch, so every future capture needs a human too.
* **`crates/shared/tests/capture/`.** In the loop's write list already, and
  beside the corpus it produces. Costs a directory of non-Rust files under
  `tests/`, which `cargo` will ignore and a reader will not expect, and it
  makes `shared`'s test directory the home of a thing that spawns language
  servers.
* **Recipe only, in the corpus header.** What is in force. Costs the
  rediscovery above, every time.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

The recipe, in the header of `crates/shared/tests/golden-traffic.jsonl`. It is
the most reversible: it adds no files, it is where somebody growing the corpus
is already looking, and both other options are a `git mv` away from it. It now
names the two traps that cost this campaign a run each, so the next attempt
pays for them once rather than again.

No source site is tagged, because there is no source site — the thing at issue
is a file that does not exist. The tag is this record and the header paragraph
that references the same problem.

## Consequences

If the answer is `harness/capture/`, a human writes two files once and every
later capture is a loop running an existing script instead of reconstructing
one. If it is `crates/shared/tests/capture/`, the loop can do it in a normal
campaign and this record is closed by that commit. If it stays a recipe,
nothing has to change and the cost is paid per campaign — which is defensible
if the corpus is nearly finished, and is not if §8.5's Zed and VS Code are
still wanted.
