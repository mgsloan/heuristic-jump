# Findings — core, worker 1

## Staleness: ask what the audited commit *contained*, not when the file moved

`core-019`'s rule (compare `last_audited` against `git log -1 -- <where>`) is
necessary and no longer sufficient. Four of my five assigned gaps were closed
by a **merge** of another loop's branch, and a merge stamps every file it
carries with one date, so `git log -1` said "changed" for all of them and
discriminated nothing. One turn settles it instead:

    git show <found_at>:<file> | grep -c 'fn <the test name>'

Two of four test names existed at the audited commit; two did not, and that is
exactly which gaps the merge closed. Fifth campaign in a row spending opening
turns on staleness — but the shape has changed, so the old check alone will not
save the next one.

## Where the gaps are concentrated

**`driver` is no longer "there is no run loop".** Older findings say
`driver::run` logs its config and returns, and that is out of date: `Actor`
exists, `Actor::run` has the `select!` loop, §7's records are emitted through
`trace.rs`, and `driver/tests/actor.rs` drives it end to end. What is absent is
the **transport** — §2's codec, §3's router, the child spawn — which
`actor.rs`'s header says is deliberate and lives in a document this phase does
not audit. A campaign that "builds the run loop" is building the transport;
say so in the hypothesis.

`measure_core`'s hole is unchanged: nothing can drive `collect`, because
`Collection::run` spawns a server and the suite has none. A real
`rust-analyzer` is on this machine, so a fixture server may not be the answer.

**§8 and `vendor/` are done.** `deps.md` is done except §15, which no loop may
close (`clippy.toml` denied; `core-003` holds the thresholds).

## The lockfile is a fixture nothing had read

Every licensing and dependency check in `seam.rs` reads workspace manifests, so
none of them can see a third-party crate — its manifest is in the registry
cache, not this repository. `cargo metadata --format-version 1 --offline`
carries `license` for all ~180 packages in 0.1s with no network, and now holds
§14's cargo-deny claim (`core-021` provisional: `deny.toml` is writable by no
loop, and cargo-deny is not installed, so a config would be a file nothing
runs). Two `#[expect]`s are needed: `serde_json::Value` and `Command::output`.

**Do not extend this to §13's declined list.** `once_cell`, `lazy_static`,
`memchr` and `aho-corasick` are all in the graph transitively — `memchr`
through `ignore`, which §13 itself names — and §13 is a rule about what *we
declare*. The manifest scan is correctly scoped.

## A late answer dies in more than one place

`core.md#the-trait`'s fix looked like one line in `hard_cap` and was not. The
deadline is absolute, so anything between the handler returning and the answer
being handed back can hit it: `encode` reads the target file to convert the
answer, and `ProjectView` refuses an expired read — so a **cross-file** late
answer never reaches the cap. That is the common case, and only the test found
it, because the fixture's target was in another file. A same-file target takes
`target_text`'s free path and never reads, so it would have passed the
half-fix. When a rule is about what survives the deadline, enumerate the paths
and pick fixtures that differ in whether they touch the filesystem.

## Load-bearing claims, confirmed by using them

* **§6 compares `(uri, line)` and reads nothing** — this is why
  `WirePosition::line()` must exist against §8.3's "no accessors"
  (CHANGE-core-007). When §8.3 and §6 disagree, §6 wins: it is the measurement.
* **§7's prior is about the *rule*, not the evaluator.** That is what makes
  `core-017` implementable without touching the seam, and it generalises: a
  fact about the reference outlives the outcome that reported it.
* **§8.2 gives the wire types no `Serialize`** — it decides the truth row's
  shape and vetoes any write-out-and-read-back design.

## Do not spend time on

* `harness/measure` (`core-001`), the capture tooling's home (`core-020`),
  `clippy.toml`'s thresholds (`core-003`) — all need a human, `harness/` denied.
* A `PositionEncoding::settle` in `shared`: `measure_core` has one.
* `deps.md#8-parse-cache[ffcd948852]` — stale. `lru` is declared and
  `TreeCache` is keyed `(uri, version)` with a byte ceiling.
* Adding `tracing-subscriber` to `driver` to assert a log line: the seam test
  pins the manifest set in both directions. Hand-roll a `tracing::Subscriber`
  (~45 lines) and carry the field out over a channel, not a `Mutex`.
* Making "handlers cannot build a `WireLocation`" type-level — Class B, no
  gain; `driver/tests/seam.rs` already holds the property.

## The machine has the language servers on it

`rust-analyzer`, `gopls`, `pyright` (`npx --yes pyright@latest`, binary
`pyright-langserver --stdio`), `emacs` 30.2 with eglot — the only headless LSP
*client* here. `zed` exists but `DISPLAY=:0` is a real session. Check `which`
before calling a gap blocked.
