# Findings — conformance, after acb37d9b

**The audit lags by hours; verify a gap before working it.** Every `found:`
in §10's entry was false when I read it ("there is no frame codec", "no test
constructs a `SnapshotSeed`"). Rule, now four campaigns old: **a `found:`
naming a missing subsystem is a stale audit, not a blocker** — delete that
clause and ask what remains. Known-stale: anything saying `measure_core`,
`measure_rust` or `lang_rust` "do not exist".

**Check for the tool before recording a blocker.** `3530047a3c` (captured
golden corpus) was called unreachable by three campaigns. `rust-analyzer` is
on the `PATH`; sixty lines of throwaway python got real traffic into
`shared/tests/golden-traffic.jsonl`. Its header says how to add more. Still
absent: pyright, gopls, any editor.

**Closed this campaign:** all of `#10-testing`, `#85`'s corpus, `#84`. Also
already satisfied despite the audit: `#vocabulary-types`, `#the-trait`,
`#two-modes[90c8d7bd21]`, `#the-dependency-graph`, `#adding-a-language`,
`#what-the-templates-handler-does`, `#the-command-line`,
`#2-document-snapshots`, both `#7-observability`, `#86`, `#85[081351da0e]`.

**Genuinely open, each its own campaign:** `#vendoring[148fd8d277]`,
`#the-oracle[eb6f4618da]` (no `ServerId` producer),
`#where-the-corpus-lives[5be0acce11]` — and under that last one, unseen by the
audit: `measure_core::manifest::parse` (`corpus.rs`) cannot read the real
`servers.toml`, expecting `[[server]]` with one `key = value` per line where
the file has `[server.<name>]` and multi-line arrays. `#9-workspace-layout`
can never close: it names `lang_python`/`lang_typescript`, outside every owned
path.

**Best unnamed target, reading named:** nothing in the driver builds an
`InputEdit`. `Documents::changed` records none, so `shared::input_edit` (new,
correct, reference-tested) has only test callers. Blocked on truncating the log
at the *cached tree's* version — `TreeCache::version` knows it, `Documents`
does not — which is designing `core`'s actor. `documents.rs`, `trees.rs`.

**Mutation-test every property, and check the mutation compiled.** `error: test
failed` and a compile failure both match `^error`; grep `could not compile`.
Two live traps this found:

* **tree-sitter's `InputEdit` `Point` fields are unobservable.** Comparing a
  reparse against a full parse — kinds, byte *and point* ranges — passes with
  all three wrong, because the read callback is byte-based and positions are
  recomputed. Pin them against a reference, never through a tree.
* **`to_sexp()` encodes no offsets.** Useless as a tree fingerprint.

**Lint shape.** A `_` arm over a *foreign* `#[non_exhaustive]` enum does **not**
trip `wildcard_enum_match_arm` (it covers nothing visible), so no `#[expect]` —
adding one is an unfulfilled-expectation build failure. Conversely
`#![expect(expect_used, panic)]` is required the moment one use sits in a free
function, and unfulfilled if all are in `#[test]` bodies.

**Manifest scans must read declarations, not text** — `contains("lsp-types")`
matched the comment beside the dependency and survived deleting it. Run the
negative control.

**Still true:** making a claim mechanical is the job; the strongest form makes
the bad state unspellable. Never `git checkout` over uncommitted work (copy to
`/tmp`). Gate: commit, `harness/gate conformance --rev <sha>`, `hj record`,
re-gate. Clippy disallows `serde_json::Value`, `Instant::now`, `read_dir`,
`Command::output`, `io::stdout`, `thread::spawn`, `unbounded`.
