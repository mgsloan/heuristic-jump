# Findings — core, after 7aa74ea9

**Verify a gap against the code before working it; the audit lags.** Two of my
four gaps' `found:` lines were stale — `pending.rs` existed and was complete.
One `grep` settles it.

**`driver` now has a request path.** `crates/driver/src/actor.rs` is `shim.md`
§13's actor: it owns `Documents`, `TreeCache`, `FileListCache`,
`PendingQueries` and the trace sink, mints §5's deadline from the `arrived`
instant the event carries, and takes §7's four steps in order. `driver::run`
builds one and runs it. **What is missing is only the wire** — `shim.md` §2's
codec, §3's router, the child spawn. The seam for it is
`Actor::run(&Receiver<Event>)` plus `Sender<Outbound>`; nothing in the actor
knows about framing. Do not start with standalone stdio as the cheaper half:
it has no oracle, so neither divergence nor the record's oracle half is
exercised by it.

**§7's record type is `shared::record` now.** It had to move — §9's graph
forbids `driver -> measure_core`. `Answered::of` is the one place a dispatch's
three endings become `decision`/`failure`/`stages`, and both producers call it;
a second copy of that match is how a replay row and a field row stop being
comparable.

**§7 will not go clean on my gap alone.** `[c4505d900b]`, the stratum columns
and the handler-reported half, is the other one, and cheap now that the
assembly is in one file.

**The highest-value next build is the edit log.** `Documents::changed` consumes
the content changes, so `core` has no edits for `TreeCache::seed` and the actor
`forget`s a document's trees on every change. Correct, but incremental reparse
is unreachable in production. The fix is `Believed` carrying its own log, keyed
so `seed` takes the edits since the *cached* version. Do not deserialize the
params a second time to get them: §8.6 puts the projection inside `Documents`.
Related trap: the cache key is `(uri, version)` and versions are monotone only
within one open, so every resync must `forget`.

**Do not spend time on.** `#9-workspace-layout` can never close — it names
`lang_python` and `lang_typescript`, outside every owned path. `#85`'s corpus
needs pyright and gopls; only rust-analyzer is on `PATH`. `server_health` being
null on every row is `shim.md` §6's missing health model, not a hole in the
record. `Deadline::cancel` has no caller because dispatch is in-line — a cancel
is only ever handled between queries — and it belongs to the worker-pool
campaign.

**`core-017` is open**: a deadline-expired row has no stratum, because
`hard_cap` drops the outcome that knew it, and both repairs are Class B.

**Instruments.** `driver/tests/seam.rs` is the `deps.md` conformance suite —
manifest and source scans, `src/` only. `driver/tests/actor.rs` is the request
path's. A control that cannot be made to fail is not a control: mutate the
production line and watch which assertion moves. `serde_json::Value` is denied
in tests too, so trace rows are asserted as text — §7 fixes the field order, so
the text is the record.

**Mechanics.** `cargo fmt -p <crate>` before the gate; step 1 is `--check`.
Stage explicit paths, never `git add -A`. Loop: gate, commit,
`harness/hj record core`. `allow-expect-in-tests` reaches only `#[test]`
bodies, so every fixture helper needs the crate-level `#![expect(...)]`.
