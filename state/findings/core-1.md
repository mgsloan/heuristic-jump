# Findings — core, worker 1

## Check the gap against the code before working it

Fourth campaign running into this. `state/audit/core.toml` carries
`last_audited` per section in UTC; `git log --date=iso-strict` on the file the
gap names is local (-06:00). One comparison, one turn. This round
`core.md#vocabulary-types[fbe658c158]` turned out to be closed by `9e581f7`
**four minutes after** its stamp, and §8.3's "nothing settles a
`PositionEncoding`" minor was closed by `measure_core::client::settled_encoding`
after it. The gap text is a statement about the repository at a timestamp. The
*section* is still the right target; what a re-audit finds instead is usually
real and is never in the list.

## The machine has the language servers on it. Check `which` before calling a gap blocked

§8.5's captured-corpus gap read as aspirational for many campaigns and was
not. `rust-analyzer` (`~/.cargo/bin`), `gopls` (`go install`, the network
works), `pyright` (`npx --yes pyright@latest` — the npm package is `pyright`,
the binary is `pyright-langserver --stdio`), and `emacs` 30.2 with built-in
eglot, which is the only headless LSP *client* here. `zed` exists but
`DISPLAY=:0` is the user's real Xorg session and there is no Xvfb, so driving
it would put a window on somebody's screen. Generalise the habit, not the
list.

## Where the gaps are concentrated

**In `driver`, and they are one campaign rather than five.** §5's deadline,
§7's emission, `both-sides-are-sets`' pending-query record and `deps.md §11`'s
`--trace` all say the same thing: `driver::run` logs its config and returns.
Everything downstream of a request arriving is missing. Do not take them one
at a time. This has been true for several campaigns and nobody has taken it.

`measure_core`'s one hole is that **nothing can drive `collect`**:
`Collection::run` spawns a server and the suite has none, so `--restart`, the
probe loop and the resume arithmetic are held by reading. Note the tension
with the finding above — a *real* server is now known to be available, so the
fixture server may not be the answer.

**§8 is done.** Both halves of the corpus are captured (48 messages, 23 of
them captured, every kind in both directions), the unions are exercised by
traffic nobody here composed, and §8.2/8.3/8.6's documents now say what the
code does. Nothing cheap is left in `proto.rs`.

## Load-bearing claims, confirmed by using them

* **§8.2 gives the wire types no `Serialize`** — it decides the truth row's
  shape (CHANGE-core-006) and vetoes any write-out-and-read-back design.
* **§6 compares `(uri, line)` and reads nothing.** This is what forced
  CHANGE-core-007: it makes `WirePosition::line()` mandatory against §8.3's
  "no accessors". When §8.3 and §6 disagree, §6 wins — it is the measurement.
* **§7's record field order is the declaration order**, asserted against the
  document. Adding a field is a seam change, not a convenience.

## Do not spend time on

* `harness/measure` (`core-001`) and where the capture tooling lives
  (`core-020`) — both open, both need a human, `harness/` is denied.
* A `PositionEncoding::settle` in `shared`: `measure_core` has one already.
* Making "handlers cannot build a `WireLocation`" type-level: the variants are
  public unit variants, so it needs a newtype only the driver can build —
  Class B, for no gain. `driver/tests/seam.rs` already holds the property.
* `positions/<repo>.jsonl` carries the token text, so §7's failure-digest
  sample is a join and not a second definition of "identifier".
