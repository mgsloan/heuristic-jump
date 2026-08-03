# Findings — conformance, after 50f21a18

**Verify a gap against the code before working it; the audit lags by hours.**
Six of the eight gaps I was handed were already closed. Three campaigns the
harness recorded **`crashed`** had committed real work *after* the last audit
ran — a `crashed` campaign is not an empty one. Two-minute check, do it first:
`git log --since` the `last_audited` stamp in `state/audit/core.toml`.

**Known closed, waiting only on a re-audit:** both `#two-modes` gaps
(`Provenance::drift` destructures every field; `corpus::locked_grammar` reads
the lockfile), `#what-the-templates-handler-does` (`Table::template`),
`#the-command-line` determinism (`table.rs` holds no `Duration`),
`#the-oracle` (`ServerProfile::proxying_named`), `#vocabulary-types`
(`seam.rs` names all seven), `#84`, `#7-observability` (both),
`#why-shared-is-separate`, `#adding-a-language`. Do not re-derive these.

**The rope newtype sweep is done** — `rope-modifications.md` in full, which is
twenty of the unjudged sections. Do not reopen it. What to know before
touching `vendor/rope`: a `_raw` suffix means "upstream's body, return type
unwrapped" (seven of them); `vendor/rope/tests/newtype_api.rs` fails the build
if a `pub fn` signature or a named `pub` field names a bare `usize`/`u32`
outside `allowed-primitives.txt`, or if a newtype gains an impl against a bare
integer; `vendor/README.md` patch 7 is the record.

**Genuinely open, in descending value:**

* **`deps.md` just joined the audited docs** (53 → 71 sections, 38 unjudged).
  Nothing has ever been judged against it and much of it is probably already
  satisfied — the cheapest sections-per-campaign on the board.
* **The driver's request path** — `#5-deadlines[f0a42a21e1]`,
  `#both-sides-are-sets[6e601d5bd1]`, and `#4`'s minor are one thing, and that
  thing is `shim.md`'s transport, codec and `core` actor, which
  `design/phases.md` assigns to **phase 2b**. They will keep looking cheap and
  they are not. Escalate the phase question before building it.
* **`#85`'s captured corpus** needs pyright and gopls; only rust-analyzer is on
  the `PATH`. `shared/tests/golden-traffic.jsonl`'s header says how to add
  more.
* **`#9-workspace-layout` can never close** — it names `lang_python` and
  `lang_typescript`, outside every owned path.
* Unnamed by any gap: nothing in the driver builds an `InputEdit`
  (`documents.rs`, `trees.rs`), and `measure_core::manifest::parse` cannot
  read the real `servers.toml` shape.

**Making a claim mechanical is the job, and a scan nobody has seen fail is not
mechanical.** Mutate every property and check the mutation *compiled* (`grep`
for `could not compile`; `error: test failed` matches both). Reverting
`Rope::is_char_boundary` to `usize` compiles, and the signature scan caught
it; the field scan failed on its first run and found `TabPosition`, a public
type the hand-audited spec list predates. Strongest form: make the bad state
unspellable; second: a scan with a negative control beside it.

**Still true.** Never `git checkout` over uncommitted work (copy to `/tmp`).
Loop: commit, `harness/gate conformance --rev <sha>`, `hj record`. Clippy
disallows `serde_json::Value`, `Instant::now`, `read_dir`, `Command::output`,
`io::stdout`, `thread::spawn`, `unbounded` — but **not in `vendor/*`**, which
takes no workspace lints. `grep -r DECISION-` is now empty; keep it that way.
