# Findings — core, worker 3

## Check `last_audited` before you claim. This is the finding.

Three of the gaps I touched were already closed, with their tests in the repo.
The audit re-judges ~7 sections per run and **the gap list is not re-derived
against head**, so a gap opened at 06:54 and closed at 20:10 is still listed
the next morning. `state/audit/core.toml` carries `last_audited` per section;
one `python3` read of it separates a target from a verification exercise.

Confirmed closed but still listed: `#84[f2b9c0b7e5]` (`dispatch::encode` reads
the carried row; `driver/tests/wire_locations.rs` asserts it),
`#vocabulary-types[fbe658c158]` (`shared.rs:54` re-exports all seven,
`seam.rs:24` names them), `#adding-a-language[0858868078]`
(`heuristic_jump` depends on `lang_rust`). Partly closed:
`#9-workspace-layout[ce5dfefab5]` — the four crates still missing belong to
later phases; `#both-sides-are-sets[6e601d5bd1]` — `driver::run` exists.
Gaps from the 19:21 run onward are fresh; treat those as real.

A gap whose `claim` quotes a sentence the document no longer contains is the
loudest tell — `f2b9c0b7e5` quotes a claim retracted under `conformance-005`.

## §8.5 is closed on the server side, by capture

**This environment has network.** `go install golang.org/x/tools/gopls@latest`
and `npm install pyright` both work; each drives over stdio in ~100 lines of
Python. The corpus header's procedure is right except that **pyright emits no
`$/progress`**, so waiting for the indexing end hangs — use a fixed sleep.

What the real messages corrected, which is §8.5's argument in miniature:
neither server answers `[]` for "no definition" (both send `null`); pyright
1.1.411 sends no `serverInfo` where the hand-authored "pyright" line invents
one; gopls' `serverInfo.version` is 3KB of JSON build metadata inside the
string; pyright answers `print` with two locations 2075 lines into typeshed,
which broke the differential's 96-row `GRID`. Nothing in `proto.rs` moved —
both readings agreed on all nine new messages.

## Do not try to capture editor traffic

`initialize` params and document traffic are composed by an editor; there is
nothing to elicit them from. Zed is installed and `DISPLAY=:0` is the user's
real desktop — starting it opens a window on a desktop somebody is using. That
is `core-018`, waiting on a human. The route for them: Zed's
`lsp.<server>.binary.path` can point at a script that tees stdin. Do not
hand-author lines and label them CAPTURED.

## The recurring defect in this crate: docs naming the wrong mechanism

Two audit minors, same shape. `proto.rs` claimed handlers cannot build a
`WireLocation` for a type-level reason — `PositionEncoding`'s variants are
public, so the real mechanism is `seam.rs`'s source scan. `vocabulary.rs`
claimed `LanguageId` compares by pointer — it is content equality, and must
be, or the registry cannot match a `"rust"` written in another crate. **A doc
naming the wrong mechanism is worse than one naming none:** the next reader
deletes the real mechanism as redundant.

Ruled out as work: §8.5's negative tests are complete (all five unions, both
directions, `tests/proto.rs`). Do not re-derive them.
