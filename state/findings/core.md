# Findings — core, after 918d4544

**Verify a gap against the code before working it; the audit lags by hours.**
`core.md#vocabulary-types` was already closed and held when this campaign
opened. One `grep` establishes it. A campaign the harness recorded `crashed`
may still have committed real work.

**`deps.md` is now essentially done.** Closed this campaign: `#fxhashmap`, §7,
§8, §9, §10, §12. Closed by 51628b98: §0, §1, §2, §4, §5-licensing, §6, §13,
§14, §15. **Remaining: §3 and §11 only**, and §11 is blocked — see below.

**`crates/driver/tests/seam.rs` is the deps.md conformance suite.** Thirteen of
its twenty-five tests are deps.md sections, and it carries the manifest/source
scan helpers (`manifest_text`, `dependency_entries`, `table_of`, `sources_of`,
`crate_members`). A new deps.md claim belongs there; do not build a second
instrument. Note `sources_of` reads `src/` only — it follows the crate root's
`mod` declarations, so no `tests/` file is ever scanned, and `read_dir` is
banned by `clippy.toml`.

**A control that produces no `test result` line is not a control.** Three
assertions across two campaigns cannot be made to fail because cargo or rustc
rejects the mutation first: `vendor/rope`'s `[lints]`, `notify`'s
`optional = true`, and a bare `Box<dyn Error>` in `shared::Error` (it costs
`Error` its `Send`, which `files.rs`'s scanner thread needs). Keep such
assertions and say *in place* that the compiler's enforcement is incidental.
The `Box<dyn` case has a sharp edge: **`Box<dyn Error + Send + Sync>` compiles
clean**, and that is the form anyone would actually write, so the scan covers
exactly what the compiler misses.

**Do not take `deps.md#11` until the request path exists.** Its gap wants
`--trace=<path>` to write JSONL records; §7 emits one "once both answers are
known", which needs the pending-query path `driver` does not have. The record
type is `measure_core::QueryRecord`, and §9's graph forbids `driver` depending
on `measure_core`, so a driver-side writer needs the type moved to `shared`
first. Adding the flag alone leaves `--trace` silently writing nothing and does
not make the section clean.

**Still blocked on phase 2b:** `core.md#5-deadlines` and
`#both-sides-are-sets`, both of which are the driver run loop (`shim.md`'s
transport and `core` actor). Escalate the phase question before building it.
`#9-workspace-layout` can never close — it names `lang_python` and
`lang_typescript`, outside every owned path. `#85`'s corpus needs pyright and
gopls; only rust-analyzer is on `PATH`.

**Load-bearing spec claims confirmed.** `deps.md`'s subset rule — "each arrives
with its first user" — is what makes §0's table a subset check rather than an
equality; `rayon`, `insta` and `tempfile` are still chosen-and-absent on
purpose. `shim.md` §5 is the cross-reference `deps.md` §7 and §8 both turn on;
read it before either.

**Mechanics.** Never `git checkout` over uncommitted work. Stage explicit
paths, never `git add -A` (`harness/**` is denied and edited concurrently).
`cargo fmt -p <crate>` before the gate — step 1 is `--check`, and it is the
only step this campaign failed. Loop: commit, gate, `hj record core`.
