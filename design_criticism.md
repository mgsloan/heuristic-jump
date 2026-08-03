# Criticism of `design/` and `readme.md`

Written against the tree at `5aa4a85`, covering `readme.md` and all ten
documents in `design/` (~10,600 lines), and revised after the first round of
fixes — findings that have since been acted on are removed rather than kept
with a note, so what remains is what is still open.

Nothing here is a style objection. Every item is either a claim I believe is
false, an arithmetic result that contradicts a stated target, a decision I
think is wrong, or a gap that has to be filled before code can be written
against the document.

Ordered by consequence, not by document. Section 9 is the short version.

**Method and standing assumptions.** I take the product goal as given: an
unindexed heuristic go-to-definition that runs in front of, or instead of, a
real language server. I take `CLAUDE.md`'s hard constraints as given. I do not
relitigate settled trade-offs (no async, vendored rope over `ropey`,
hand-written protocol types) — those are argued well and I have nothing to add.
What I attack is where the documents disagree with each other, with arithmetic,
or with their own stated method.

---

## 0. What holds up

Stating this first so the rest reads as calibration rather than as a verdict on
the whole thing.

* **The measurement architecture is the strongest part.** Freezing the LSP's
  answers into `truth.jsonl` once per (repo commit, server version) and
  replaying handlers against it (`core.md` §7) is the decision that makes
  iteration possible at all, and the reasoning for it — the oracle is a fact
  about the corpus, not about our code — is exactly right.
* **`measure_core` + four-line `measure_<lang>`** (`core.md` §7) is a genuinely
  good structural call. The three reasons given for it are all correct and the
  third (isolation that survives into the build graph) is the one most designs
  get wrong.
* **`ProjectPath` unforgeable, `WirePosition` inert** (`core.md` §8.3, §1).
  Making the scope rule and the encoding rule not-compile rather than
  not-recommended is the right instinct, applied in the two places it pays most.
* **Splitting `agreement` into `top1` and `contained`** (`core.md` §6) closes a
  real hole, and noticing that the naive set predicate reintroduces the flaw
  that killed plain match rate is a good catch.
* **`#[serde(untagged)]` on `contentChanges`** (`core.md` §8.5) is a real bug,
  correctly diagnosed, with the right fix. That section justifies its own length.
* **The five failure modes in `loops.md` §19** are honestly stated, which is
  rarer than it should be.

The criticisms below do not touch any of these.

---

## 1. Two ways the plan optimises the wrong number

### 1.1 Latency has no owner

Follow the thread:

* **Phase 2a** optimises "precision and recall, and nothing else. Cost metrics
  are *recorded* and never *gated*" (`loops.md` §10).
* **Replay has no deadline at all** (`core.md` §7), so phase 2a's coverage
  number is explicitly an *upper bound* on what the shim delivers.
* **Phase 3** may not change any answer — "any difference fails the gate"
  (`loops.md` §10, `phases.md`) — and its objectives are latency, binary size,
  and line count.
* **The deadline is now the only bound on a single query's work**
  (`shim.md` §10, `resolution.md` §1.3), so blowing it is a whole-query
  abstention.

Put together: phase 2a is free to build a handler that only wins on an
unbounded clock; nothing measures the gap between replay coverage and delivered
coverage until a phase gate; and phase 3 is then handed the job of closing that
gap *without being allowed to change a single answer*. If the gap turns out to
be structural — a search that is exhaustive by construction and simply cannot
finish in 750 ms on a 200k-line repository — phase 3's only legal move is to
escalate, and the human's only options are to weaken the equality constraint or
give back coverage.

`loops.md` §19 names the adjacent failure ("the quality phase climbs to an
incompressible peak") but prices it in *binary size*, not latency, and the
remedy it offers — selecting an earlier point off the metrics history — does not
help, because the metrics history records work counters, not the delivered
coverage that the deadline actually determines.

**The missing measurement is cheap and should be in every metrics row**:
replay already computes per-query work counters, so record the *predicted*
deadline-abstention rate — the fraction of replayed queries whose work counters
exceed a calibrated threshold. `resolution.md` open question 15 asks for exactly
this ("report it per stratum and per repository size from the first corpus run")
and `loops.md` §10 does not carry it into the metrics row. That is the one line
that would make the trade visible during phase 2a instead of at the phase 3
gate.

### 1.2 The optimisation target may be the one the design says does not matter

`high-level.md`'s "Value weighting" section says it plainly:

> A correct answer to a query that rust-analyzer would have served in 150ms is
> worth approximately nothing — the tool's value is concentrated entirely on the
> slow tail. This may show that the genuinely useful slice is much narrower than
> the raw identifier count suggests.

`core.md` §7 records `lsp_latency_us` specifically to enable this. And then:
nothing consumes it. It is not on the frontier (`loops.md` §10 is top-1 ×
coverage), not in the metrics row, not in the tuning prompt (`loops.md` §14),
not in a gate. `data-collection.md` §3 samples positions **uniformly** and says
so deliberately, which is correct for an unbiased denominator and guarantees the
corpus is dominated by the queries the document just said are worth nothing.

So the plan is: spend the dominant token budget of the project (`loops.md` §15
marks phase 2a "dominant") hill-climbing unweighted coverage over a uniform
sample, having written down in advance that unweighted coverage over a uniform
sample may be nearly uncorrelated with value.

That is defensible only as a starting point, and only if the check happens
early. `resolution.md` §12 says so — "also worth measuring early... what fraction
of these queries the proper LSP would have answered in under 200ms anyway. If
the slow tail is concentrated in `ExternalDependency` and
`TypeInferenceRequired` — the two classes this design serves worst — the
per-stratum table will look healthy while the tool delivers little" — and then
no phase gate, no loop, and no dashboard panel requires it. It should be a
**gate condition on entering phase 2a**: compute the latency-weighted stratum
distribution from the first truth files, and if the useful slice is concentrated
in the strata scoped out, stop and rethink the metric before spending the budget.

---

## 2. Metric and measurement defects

### 2.1 The ±3-line tolerance is too generous, and the argument for it only covers the easy case

`core.md` §6 counts an answer as matching if it is within three lines of the
LSP's, on the grounds that "the definition is on screen and the user is already
reading it." It then declines to compare columns, reasoning that a stricter test
nested inside a looser one is inconsistent, and concedes exactly one cost: two
definitions on the same line (`int x, y;`).

The same-line case is the *rare* one. The common one is **consecutive
declarations**, which are everywhere in the languages in scope:

```rust
enum Token { Ident, Number, String, Comment }   // four targets, four lines
struct Config { host: String, port: u16, tls: bool }
```

and in C headers, TypeScript interfaces, Go struct tags, Python dataclass
fields, and every `impl` block of one-line accessors. In all of them, a
heuristic that lands on a *neighbouring* declaration of a similar name scores as
a match. That is not a marginal overstatement: adjacent-declaration confusion is
precisely the error mode a name-based heuristic produces, so the tolerance is
widest exactly where the tool is most likely to be wrong.

It also interacts with containment. `contained` is "any of ours within 3 lines
of any of theirs" — with six candidates returned, in a file with a dense
declaration block, containment can be satisfied by a candidate the user would
recognise instantly as wrong.

I am not arguing for exact range equality; the original objection to that is
correct. I am arguing that (a) the cost is much larger than the one case §6
admits, (b) it should be *measured* — the corpus can compute what fraction of
"matches" are non-exact, per stratum, at zero cost — and (c) if that fraction is
material, the tolerance should be same-declaration rather than same-±3-lines,
which the handler can determine because it already has the enclosing node from
`classify`.

### 2.2 `resolution.md` §12's headline top-1 does not follow from its own table

The table gives per-stratum share, coverage, and top-1. Weighting them:

| | over answered queries | over all queries |
|---|---|---|
| coverage | — | **88.1%** (stated: ~87%, fine) |
| top-1 | **86.7%** | 76.4% |
| containment | **96.6%** (stated: ~96%, fine) |

The stated "roughly 82% top-1" matches neither reading. Coverage and containment
both check out, so this is a slip in one cell rather than a different definition.

It matters because the number that slipped is the one the section leans on:

> Weighted, roughly 87% coverage at roughly 82% top-1 and 96% containment. The
> gap between those last two is the point of the whole decision.

Under the table's own rows that gap is **10 points, not 14**, and
`resolution.md` §12 then lists "the top-1/containment gap, predicted at ~14
points" first among "the predictions most worth being wrong about", with the
explicit reading that a much smaller gap means "the ranked list is machinery for
a case that rarely arises and the single-answer design was right."

Recompute the cell. If the intended number really is 82%, some row is wrong and
should be found; if it is 87%, the ranked-list decision is resting on a
substantially thinner predicted margin than the prose claims.

### 2.3 The corpus is an unreconstructable single point of failure, and irreversible decisions are welded to it

`data-collection.md` is explicit and then moves on:

> the checkout is the artifact, not the URL. Since nothing ever re-clones, a
> repository that is force-pushed, renamed, or deleted upstream costs nothing —
> but losing the corpus directory loses the corpus, and it cannot be
> reconstructed from the manifest. It belongs in whatever gets backed up.

That is the entire durability plan for the artifact that every number in the
project is defined against, costing ~100 machine-hours and gating phases 2
through 7. No checksums, no archive format, no offsite copy, no verification
that a restored copy is the one that was collected.

Worse, several cheap-looking decisions become 100-machine-hour decisions because
positions are frozen alongside truth:

* **The identifier-shape rule** (`data-collection.md` §2) — "a named leaf node
  whose entire text is identifier-shaped" — decides the denominator of every
  metric. It is argued in about fifteen lines and listed under "Decided" with
  the cost described as "a slightly noisier denominator rather than a wrong
  one." Changing it later re-enumerates every position and invalidates every
  truth file.
* **Grammar revisions.** Positions are enumerated with our tree-sitter grammar
  and joined by byte offset. `CLAUDE.md` forbids bumping a grammar, and the
  truth header records the grammar revision — good — but nothing anywhere says
  what happens when Zed moves a pin and a re-sync brings a grammar change. The
  honest answer is "recollect", and that answer should be written down next to
  the rule, because it turns a routine dependency update into a phase-length
  event.
* **`20,000` positions per repository**, listed under "Decided. Kept." with no
  power calculation, while `loops.md` "Decided 8" separately records that the
  corpus is *assumed* large enough to distinguish improvement from sampling
  noise and carries the assumption as a risk. Those two should be one decision
  with one piece of arithmetic behind it.

Add three lines to `data-collection.md`: a manifest checksum per truth file,
a stated backup target, and a "what invalidates the corpus" list (repo commit,
grammar revision, server version, identifier-shape rule, sampling seed) so the
irreversibility is visible at the point where each of those is chosen.

### 2.4 Sampling, thin strata, and the one number nobody will trust

`data-collection.md` §3 records the thin-strata problem and explicitly declines
to solve it, on the correct grounds that oversampling a stratum would require
classifying positions with the code under measurement. It then hands the problem
to "the phase that tunes against it", and `loops.md` receives it as a risk in
§19 and an assumption in "Decided 8". Nobody solves it, and the two documents
each believe the other is holding it.

The arithmetic is more favourable than either document assumes — 100k positions
per language at a 4% stratum is 4,000 samples, whose Wilson half-width at 90%
agreement is about ±1 point, which is fine — so this is probably a non-problem.
But it is currently carried as an unresolved fear in two documents, and one
paragraph of arithmetic would retire it. Doing that also produces the number
needed for §2.3's power calculation, so it is the same work.

The `MacroGenerated` row at ~2% is the one that genuinely stays thin, and it is
also the row the design cares least about. Say so and stop worrying.

---

## 3. Gaps that must be filled before code

These are not disagreements. They are places where an implementer following the
document arrives at a question it does not answer.

### 3.1 Nobody owns the byte-offset → wire conversion for the *target* file

`core.md` §8.4 establishes two types: `Location` (byte offsets, what a handler
returns) and `WireLocation` (line/character, what goes on the wire), and says
"the driver converts one to the other on the way out, in the same one place that
owns `PositionEncoding`."

But that conversion needs **the target file's text** — §8.4 says so directly:
"only that one line's text is needed, and only to resolve the UTF-16 column."
The target file is frequently not open, so this is a disk read.

Now cross-reference `shim.md` §2: `core` "never parses, never searches, never
touches the filesystem" and performs "only O(1) state transitions". And the
response is emitted by `writer:editor`, which owns a pipe and nothing else.

So the read has no home. The plausible fix is for the *worker* to build the
`WireLocation` before handing the outcome back — it already holds a
`ProjectView` and is inside the deadline — but that hands `PositionEncoding` to
the dispatch layer, and `core.md` §3 and §8.3 are emphatic that encoding lives in
exactly one place. Whichever way it is resolved, it is a real edit to §8.4,
`shim.md` §2's actor contract, or both, and it should be resolved on paper
because it touches the highest-risk correctness surface in the design.

The same question applies, less urgently, to the `didSave` checksum
(`core.md` §8.6), which §8.6 does correctly assign to a worker — showing the
authors know the constraint and simply missed this instance.

### 3.2 The constants that decide behaviour are absent

The documents specify 1 KiB, 3 lines, 512 words, 4 in-flight, 750 ms, 2000 ms,
20,000 positions. They do not specify:

* the `Unresponsive` threshold — "requests pending beyond a threshold with no
  frames of any kind arriving" (`shim.md` §6);
* the parse LRU's entry and byte ceilings (`shim.md` §5, `deps.md` §8), which
  decide whether a replay reuses trees across queries or reparses the same
  repository thousands of times;
* the rescan debounce window (`core.md` §4);
* the divergence report window (`shim.md` §2 mentions a timer driving it);
* the repeated-panic count that disables a handler (`shim.md` §11);
* the candidate list cap — `open-questions.md` question 12, correctly left open,
  but with no provisional value, so the first implementation has to invent one;
* the stall thresholds' units — `loops.md` "Decided 2" fixes N at 3 and 5
  campaigns, but the per-campaign token ceiling that bounds a campaign
  (`loops.md` §15) has no number at all, and it is the thing that bounds context
  growth.

Most of these want measurement, which is fine. What is not fine is that a
document set this specific leaves them implicit, because the first implementer
will pick values, they will not be recorded as decisions, and `loops.md` §6's
Class A/B machinery will never see them.

---

## 4. Machinery built for a future that is out of scope

`CLAUDE.md` says "avoid premature optimization — implement the slow simple
version first" and "avoid creative additions unless asked". `phases.md` scopes
the initial implementation to **phases 1a through 1.5, then stop**, with the
exit criterion "`measure_rust replay` prints a per-stratum table over real truth
data, twice, identically." One language. No shim. No tuning loop.

Against that scope, the following are built now and justified by needs that the
scope does not contain.

### 4.1 `CommitPolicy`, `Confidence`, `ServerProfile`, `Stratum::Unimplemented`

* `CommitPolicy` (`core.md` §1, `resolution.md` §7.4) is explicitly inert: "in
  v1 that returns `Committed` for every input... the funnel is inert and buys
  nothing today." The case for it is that retrofitting means "auditing every
  commit site in every `lang_*` crate at the moment when there are the most of
  them." In the scoped work there is **one** `lang_*` crate, written by hand,
  as a template. The migration cost the argument is buying down does not exist
  yet.
* `Confidence` is recorded and never read. The argument (a floor can only be
  derived from data collected while nothing was gated) is correct for the
  *feature* columns — `margin` and `considered` on the trace record — and those
  are cheap. It is much weaker for the collapsed `Confidence` value, which
  `resolution.md` §7.1 concedes "on day one the number is not a probability of
  anything" and `resolution.md` open question 8 then asks whether it should
  exist at all rather than being a per-stratum constant. Open question 8 should
  be answered *before* the type is threaded through the seam, not after.
* `ServerProfile` is an empty struct with one `Option<ServerId>` field, and both
  `core.md` §1 and `resolution.md` §1.4 forbid handlers from reading that field.
  A struct whose only member may not be read is not a seam element; it is a
  comment. Add it when the first profile field is justified by corpus evidence,
  which is the rule both documents already state.
* `Stratum::Unimplemented` is defended as "a gate check rather than something
  anybody has to notice" — but no gate anywhere checks it. `loops.md` §4's gate
  list does not include it.

Roughly 250 lines of `core.md` and `resolution.md` defend machinery that, by
the project's own phase plan, will sit unused for the entire scoped work. The
individual arguments are good; the aggregate is the thing `CLAUDE.md` warns
against. I would keep the trace-record columns (`margin`, `considered`,
`stratum_prior`/`stratum_final`) — those are genuinely unrecoverable later — and
defer the rest to the phase that needs them.

### 4.2 The rope newtype sweep is priced honestly and still looks like the wrong first move

`rope-modifications.md` is a well-argued document that reaches a conclusion I
would not take *yet*. What it commits to:

* 51 public function signatures converted, plus all 17 of `ChunkSlice`'s;
* all 54 `Point::new` / `PointUtf16::new` call sites edited;
* `TextSummary` converted, described as "the largest scope increase";
* body edits throughout, with the safety argument downgraded from "a mechanical
  diff proves it" to "review checks the shape, tests check the behaviour" (§3);
* the clean re-sync property explicitly conceded (§6);
* a CI check asserting no bare `usize`/`u32` in `pub fn` signatures, plus an
  allowlist file, to catch the one failure the diff cannot see;
* `vendor/util` folded in, which §4 notes is *contingent on this work* and
  should be reversed if the newtype sweep is ever dropped.

And it lands in **phase 1a**, alongside the seam, `ProjectView`, `measure_core`,
and the corpus plumbing — the phase `loops.md` §8 marks "Hand-driven or heavily
supervised. The seam is decided here, and getting it wrong is expensive
downstream."

The value it buys is real: `ByteColumn` vs `Utf16Column` being distinct is the
single mitigation for the bug class `core.md` §3 calls the highest-risk in the
driver. But `core.md` §8.3's `WirePosition` already makes that bug
unrepresentable *at the boundary*, which is where LSP positions enter, and
`resolution.md` §1.1 states that UTF-16 never reaches a handler at all. So the
marginal risk this closes is a confusion between two column units **inside code
we wrote, below the boundary type that already prevents it**, at the cost of the
largest single patch to a vendored crate in the project, taken before there is
any code to protect.

I would do the boundary types (`ByteOffset`, `ByteLen`, `ByteRange`,
`LineIndex`) — those are needed by the seam and are a much smaller sweep — and
defer `ByteColumn`/`Utf16Column`/`CharCount`/`TextSummary` until either the
encoding property tests show a real failure or phase 3 has spare capacity. That
also keeps the clean re-sync property for longer, and keeps `vendor/util` as an
independent decision rather than a contingent one.

If the sweep is done anyway, note the interaction with `deps.md` §14's
workspace-wide lint config (§6.6 below): `rope`'s upstream tests are what verify
the sweep, and they are kept verbatim precisely because they are unedited.

### 4.3 The retry protocol's machinery costs more than its expected yield

`Spot`, `is_repeat_of` with its four-arm asymmetric comparison, edit-anchoring
of pending spots on every `didChange`, the widening-only-when-current rule, and
the pending-query scan (`shim.md` §7) exist to serve one case: a repeat press at
the same spot while the first request is still outstanding.

By the design's own analysis, that case is close to empty:

* When the server is `Warming` or `Unresponsive`, the policy is **eager**
  (`shim.md` §6), so the first press is answered and there is no retry to detect.
* When the server is `Ready`, it has already answered a definition request, and
  `open-questions.md` question 4 records that a slow-but-alive server is
  deliberately **not** pre-empted. So the retry serves only "a `Ready` server
  that is slow on this particular query."
* `high-level.md` says as much directly: "In a live session against a healthy
  server the retry rule means the shim answers almost nothing."
* `open-questions.md` question 2 records that the whole protocol assumes the
  editor sends a second request rather than cancelling or deduping, verified for
  Zed only, with no trace artifact and no VS Code answer.
* `open-questions.md` question 3 records that under load the retry — the press
  the protocol exists for — may be the one dropped by the in-flight cap.

So: intricate, unverified for half the target clients, and serving a case the
document says is nearly empty, while the case that actually delivers coverage
(eager during `Warming`) needs none of it. The swallow machinery is needed
regardless and is well-designed; `Spot` and anchoring are not.

The cheapest experiment in the entire project is recording a VS Code trace
against a deliberately slow server. It is a prerequisite for `shim.md` §7 and it
has not been done. Do it before building any of this, and if VS Code cancels,
delete `Spot` and the anchoring and rely on eager alone.

---

## 5. The loop design

`loops.md` is 2,500 lines proposing autonomous loops for a project with no code,
and it is candid that "nothing here has been tried on this repository." The
candour is genuine and the failure list in §19 is good. What follows is where I
think the mechanism does not do what it claims.

### 5.1 The conformance loop's number is soft in both numerator and denominator

`loops.md` §3 and §5 make **sections clean / sections total** the conformance
loop's only metric, on the argument that the denominator is "fixed and
mechanical" — parsed from headings in `core.md` and `shim.md` — so "splitting a
gap in two moves nothing."

But the ownership table in §13 grants the conformance loop write access to
`design/`, because Class A spec fixes edit the design documents. So the loop
owns its own denominator. It cannot inflate the number by splitting *gaps*, but
it can by restructuring *sections*: merging two dirty sections into one clean
one, or splitting a clean section into three. Neither looks like gaming in a
diff; both look like the "internal contradiction / section reference that does
not resolve" tidying that Class A explicitly authorises.

The numerator is soft for the reason §19 already admits — "a section can be
audited and wrongly called clean, and nothing downstream disagrees" — and the
auditor is the same model that wrote the code, in a fresh context. §5 argues
persuasively that a fresh context is enough because the failure being caught is
attention-based rather than capability-based. I agree with that for *gaps*. It
does not extend to the verdict, because the verdict is a summary judgement with
no artifact behind it, and it is the number.

Two cheap fixes, neither of which is in the document: snapshot the section list
as a `harness/`-owned file that the loop cannot write, so the denominator is
genuinely mechanical; and require every `clean` verdict to cite the code
satisfying each claim in that section, so a wrong verdict leaves evidence a
later audit can contradict.

### 5.2 `loops.md` contradicts itself on who runs phase 1a, and on whether the auditor writes

* §8: "**Phase 1a** ... *Hand-driven or heavily supervised.* The seam is decided
  here, and getting it wrong is expensive downstream in a way no loop will
  notice."
* §18: "**Phase 1a runs as a conformance loop**, so the machinery that loop
  depends on is in scope and the rest is not."

These are the two possible plans for the only phase currently in scope, and the
document asserts both. §8's reasoning is the better one and §18's scoping
depends on the opposite. This needs resolving before anything is built, because
it decides whether `harness/gate`, the auditor, and the prompts are on the
critical path at all.

Separately: §3 says `state/audit/<doc>.toml` is "written only by the auditor",
while §5 and the auditor prompt in §14 both say the auditor "may not edit
anything" / "cannot edit anything." Presumably the harness captures the
auditor's output and writes the file, but as written the document says the
read-only session writes a file.

### 5.3 Per-server loops have almost no legal moves

§2's table gives the metric loop's concurrency as "parallel, one per (language,
server), in phase 2a", and §13's ownership table splits `lang_python` into a
language loop (everything except `profile/`) and a `python-pyright` loop owning
exactly `crates/lang_python/src/profile/pyright.rs`.

But `ServerProfile` starts empty, and both `core.md` §7 and `resolution.md` §1.4
rule that a field appears "only when the corpus shows a systematic divergence
that a field would fix" and that a handler must never branch on server identity.
So the per-server loop owns one file, is forbidden to grow its type without
what amounts to escalation-grade evidence, and is forbidden the one
implementation shape (`if server.id == PYRIGHT`) that would let it do anything
locally. §13 hedges — "whether per-server loops are worth spawning at all is a
volume question" — but §2 presents them as the concurrency unit.

Pick one. I would drop the per-server loop entirely from the design and keep
per-server *metrics*, which is where the value is, adding a profile loop only
if and when a profile grows enough fields to be worth an optimiser.

### 5.4 Worktree cost is priced in RAM only

§13 says each parallel loop gets its own `git worktree`, and that "a worktree is
its own workspace root, so it gets its own `target/`", with the price described
as "disk and rebuilding shared dependencies per worktree" and "small" because
builds are scoped to owned crates.

Scoped builds still compile `shared`, `similarity`, `rope`, `sum_tree`,
`tree-sitter`, and one grammar per worktree, plus `measure_core` and its LSP
client and JSON stack. That is multiple GB of `target/` per loop, times a dozen
loops in phase 2a and thirty-plus in phase 6, and `CLAUDE.md` opens by warning
against expanding the build matrix and `target/`. §14 caps concurrency on RAM
and does not mention disk. It should, and it should say what happens when
`target/` across worktrees exceeds the machine — because the failure mode is a
loop that fails its gate for reasons invisible in its own diff, which is the
exact failure §13 designed the isolation to prevent.

### 5.5 Smaller points on `loops.md`

* **The ownership table contradicts itself on `crates/similarity/`**: the
  `phase 3` row grants "everything, including `crates/similarity/`" and the
  `*nobody*` row lists `crates/similarity/`. The prose resolves it (frozen
  during phase 2, writable in phase 3) but the table is the enforcement
  mechanism and it says both.
* **Class A/B is judged by the party with the incentive**, as §19 admits. The
  countermeasure — "any spec edit in the same commit as code touching the same
  item is flagged" — is defeated by making it two commits, which the iteration
  contract encourages anyway (one target per iteration).
* **The five-panel dashboard, POST endpoint, transcript renderer, supervisor,
  and cost join** are ~600 lines of specification for tooling that §18 correctly
  says is not built yet, and that §18 then proposes the conformance loop should
  build during the phase-1.5 window. That is a reasonable use of idle wall clock
  and a large amount of design written against an unvalidated premise.
* **`ccusage`, `--session-id`, `--fork-session`, `denyRead`, `PreToolUse`, and
  bubblewrap `allowWrite`** are all external product surfaces of one vendor's
  CLI. §17 adopts them without noting that the harness's core mechanisms depend
  on them staying stable, which is a larger dependency risk than any crate in
  `deps.md` — a document that spends 966 lines being careful about exactly this.

---

## 6. Cross-document inconsistencies

These are Class A defects by `loops.md` §6's own definition — defensible answers
that trade nothing off — and they should be fixed before anything reads these
documents as a spec.

### 6.1 The held-out split is stated three different ways

* `high-level.md`: "Of the 10 repos per language, 5 are held out."
* `data-collection.md` §1: "Ten repositories each — five for training, five held
  out."
* `loops.md` §12: "`high-level.md`'s development plan holds out **3-4**
  repositories per language" — and then, twenty lines later in the same section:
  "**The split is five and five** (`data-collection.md` §1)."
* `resolution.md` §11: "Per `high-level.md`'s development plan, **3–4**
  repositories per language never seen by tuning."

Two documents misquote a third, and one contradicts itself internally. This is
also the most consequential single number in the corpus plan — `data-collection.md`
§1 calls it "the one phase-1b decision that cannot be revisited."

### 6.2 The anti-restatement rule is stated and not followed, and the restatements have already drifted

`shim.md`'s preamble sets the rule clearly:

> What this document does **not** own, and must not restate ... a second copy of
> the encoding rule or the agreement predicate is exactly how the shipped metric
> and the measured metric stop being the same number.

Against that:

* **The licensing rationale** ("`rope` is the only GPL input, so replacing it
  would make the whole workspace permissively licensable without relicensing
  anything") appears in `readme.md`, `high-level.md`, `core.md` §9, and
  `deps.md` §5 — four copies, three of them near-verbatim.
* **The corpus layout tree** appears in `readme.md`, `high-level.md`,
  `core.md` §7, and `data-collection.md` §0 — four copies, and they already
  differ (three splits vs two).
* **The ±3-line tolerance and its justification** appear in `high-level.md`
  and `core.md` §6 twice within the same section.
* **The standalone rationale** appears in `high-level.md` and `shim.md` §14.1.
* **`readme.md`'s first eighteen lines are byte-identical to
  `high-level.md`'s**, while `readme.md` simultaneously claims `high-level.md`
  is "the only one that stands alone."
* **The held-out split**, §6.1 above — the one restatement that has already
  produced a contradiction, exactly as the rule predicts.

### 6.3 `phases.md` 1.5's scope is much larger than its gate or the exit criterion

* `phases.md` 1.5: "Collect the ground truth for **every language server on
  every repo**." Seven languages, ten repositories each, more servers than
  languages — `loops.md` §15 prices it at ~100 machine-hours and calls it "the
  plan's long pole and its highest-uncertainty item."
* `data-collection.md` §0 gate: "**for at least one language**, every repository
  has positions, a truth file per server..."
* `loops.md` §18 exit criterion: "`measure_rust replay` prints a per-stratum
  table over real truth data, twice, identically." Rust only.

Two of the three say one language is enough to finish the scoped work; the phase
definition says all seven. The difference is roughly the difference between one
week and two months of machine time, on the item the plan itself identifies as
its long pole. `data-collection.md` §6 even recommends doing Rust first and C++
second as a risk-reduction ordering, which only makes sense under the
one-language reading.

Resolve it in `phases.md`, which the readme calls "the authority the other
documents defer to."

### 6.4 The error/abstention separation leaks

`CLAUDE.md`, `core.md` §1, `resolution.md` §1.1, and `deps.md` §10 all insist,
correctly and at length, that abstention and failure are different things and
must not share a type. Then `AbstainReason` carries `HandlerError` and
`NoParse`, and `resolution.md` §8 says `HandlerError` "feeds the repeated-panic
handler disable in `shim.md` §11."

So a failure *is* an abstention reason, is consumed as a failure signal, and the
one enum that exists to hold no failures holds two. This is defensible — the
driver converts a failure into an abstention at the dispatch boundary, and
`deps.md` §10 even says so ("Some `driver` code will convert an `Error` into an
abstention; that conversion is explicit and logged") — but then the *reason*
recorded should say the conversion happened, not pretend the handler chose to
abstain. As written, the metrics cannot distinguish "this stratum has no
coverage because resolution is hard" from "this handler is panicking", which is
precisely the distinction `resolution.md` §8's last paragraph says the reasons
exist to make.

### 6.5 `ServerProfile.id` is a documented backdoor to something the design forbids

`core.md` §1: `ServerProfile.id` is `Option<ServerId>`, `None` in standalone,
and "a handler that branches on this is doing something wrong — but the absence
has to be representable."

Everywhere else, this design makes rules structural rather than customary —
`ProjectPath` is unforgeable, `WirePosition` is inert, handlers cannot construct
`Outcome::Committed`, and `core.md` §1 says so about each in turn. Here the rule
is a comment, and the field it guards is simultaneously (a) the only member of
the struct, (b) forbidden to read, and (c) sufficient to detect standalone mode,
which `resolution.md` §1.1 separately says a handler must never learn.

Either the field should not be in the struct handed to handlers, or the rule
should be enforced the way every comparable rule in the design is enforced.

### 6.6 Assorted smaller ones

* **`deps.md` §14 puts `[lints] workspace = true` in every member**, and §15
  then denies `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`,
  `string_slice`, and the `cast_*` family. Members include `vendor/rope` and
  `vendor/sum_tree` — 7,400 lines of someone else's text-datastructure code,
  plus the upstream tests `rope-modifications.md` §7 keeps *verbatim* precisely
  because they are unedited. Editing `vendor/` is permitted now, so this is no
  longer a contradiction, but satisfying those lints there is a large amount of
  low-value work that would also enlarge every re-sync diff. `deps.md` §14
  should carry a per-crate `[lints]` override for `vendor/*` as a deliberate
  choice.
* **`indexing_slicing` and `string_slice` are denied workspace-wide**, and
  `shim.md` §3.1's bounded prefix scanner is a hand-written byte scanner that is
  nothing but indexing and slicing. It will be written entirely under
  `#[expect]`, which converts a deny into decoration in the one file where the
  lint would have had the most value.
* **`resolution.md` §6.4 requires a total order over candidate scores.** If
  scores are `f32`, the idiomatic comparator is `partial_cmp().unwrap()`, and
  `unwrap_used` is denied. `f32::total_cmp` solves it; nothing says so, and the
  first implementer will reach for the `unwrap`.
* `loops.md` describes the design corpus as "nine thousand lines" (§3) and "ten
  thousand-odd lines" (§0) and "ten thousand lines" (§6). It is ~10,600.
* `core.md` §9 says the template handler's `Stratum::Unimplemented` "means the
  template has not been replaced — a gate check rather than something anybody has
  to notice." No gate in `loops.md` §4 checks it.
* `shim.md` §14.5's worked abstention message is "ambiguous, 7 candidates", a
  variant v1 does not have. `resolution.md` §8 flags this, `resolution.md` open
  question 6 flags it again, and the message is still there.
* `open-questions.md` question 2's answer — "*Zed* does send two requests" — is
  the load-bearing empirical claim under `shim.md` §7 and is recorded as a bare
  bullet with no trace, no date, and no version, in a document set that
  elsewhere cites `crates/lsp/src/lsp.rs:793` and `src/subtree.c:561`.
* `DocumentSnapshot.parsed` is a `std::sync::OnceLock` (`core.md` §2). Under
  fan-out, two workers sharing a snapshot can contend on it, and one blocks —
  which is a lock on the query path, in a design whose stated rule is "no locks
  anywhere" and "reaching for a lock means something is architecturally wrong."
  It is the right primitive; the rule should acknowledge it rather than being
  stated absolutely and then quietly excepted.
* `high-level.md` scopes out `ExternalDependency` (predicted 4% of queries,
  0% coverage) while its own Future Work says "jumping into a dependency is a
  common go-to-definition." Those two sentences are about the same stratum and
  they disagree about how common it is — and §1.2's value weighting suggests it
  is also where the slow-LSP tail lives.

---

## 7. Volume, archaeology, and who these documents are for

`loops.md` §15 identifies context bounding as the primary *token* lever for the
whole project, and §3 removed the ledger partly because it was "the single
largest artifact standing between here and the first line of code." Both
instincts are right, and neither was applied to the design documents themselves.

Two costs are being paid on every read, by every agent session, forever:

**Superseded positions are carried inline.** `resolution.md` alone contains: an
open-questions section where 8 of 18 entries are "Resolved" or "void"; §4's
explicit history of "the successive positions this document held"; §6.4's
rebuttal of "the argument the previous revision gave"; §7.5's "this reverses an
earlier decision"; §8's list of three deleted variants. `core.md` §7 and §9,
`shim.md` §14.5, `rope-modifications.md` §3 and §4, and `loops.md` §3, §9, §10,
and §13 all do the same. The rationale — `readme.md`'s "a decision whose
alternatives are not written down gets relitigated" — is sound, and the
implementation is wrong: the alternatives should be written down *once*, in a
decisions log, not threaded through the prose that a fresh context has to read
to learn what is currently true. I estimate 15–20% of the corpus is argument
against positions no reader holds.

**The document set is sized for a much larger project than the one scoped.**
`deps.md` is 966 lines pinning exact patch versions of ~25 crates against a
`rustc` version, "as of 2026-08-02", before a line of code exists. Those pins
will be stale before the first commit and the document does not say whether they
are decisions or observations. `loops.md` is 2,500 lines for machinery that §18
says is mostly not built yet, running loops that do not exist, in phases that are
out of scope. `shim.md` is 1,700 lines for phase 2b, which `phases.md` places
after the "then stop."

The scoped work — phases 1a through 1.5 — is served by `core.md`,
`data-collection.md`, `phases.md`, the parts of `resolution.md` that describe the
Rust handler, and about a third of `deps.md`. That is a good, proportionate
design document. The rest is a plan for a project that has not started, written
at the same level of finish, and it will need re-deciding against real numbers
anyway — `loops.md` §18 says so explicitly ("the followup should revisit
sections 9, 10, and 15 against real numbers rather than implementing them as
written").

---

## 8. `readme.md` specifically

Shorter, and mostly fine. Three things:

* It duplicates `high-level.md`'s opening eighteen lines verbatim while telling
  the reader that `high-level.md` is the one document that stands alone. Either
  the readme should be the standalone one and `high-level.md` should start at
  "Four reasons it exists", or the readme should link.
* It duplicates the licensing rationale, which now exists in four places
  (§6.2).
* The "Planned layout" tree is the third copy of the workspace layout
  (`core.md` §9, `deps.md` §14) and the fourth copy of the corpus layout. For a
  file whose job is orientation, the table of documents at the bottom is the
  valuable part and the trees are the part that will drift.

The document table itself is good and should stay. The one improvement worth
making: mark which documents are in scope for the current phase. A reader today
cannot tell from `readme.md` that `shim.md` and most of `loops.md` describe work
that `phases.md` explicitly defers.

---

## 9. The short version

If I could change four things:

1. **Put a predicted deadline-abstention rate in the metrics row** (§1.1), so
   the gap between replay coverage and delivered coverage is visible during
   phase 2a rather than at the phase 3 gate, where the equality constraint
   forbids fixing it.
2. **Make the latency-weighted stratum distribution a gate condition on
   entering phase 2a** (§1.2). The design already says the headline metric may
   be nearly uncorrelated with value; check before spending the dominant budget.
3. **Record a VS Code trace** (§4.3). It is an afternoon, it is a prerequisite
   for `shim.md` §7, and if the answer is "cancels", a large section of the shim
   design deletes itself.
4. **Fix the Class A defects** (§6): the 3–4 vs 5/5 split, the phase-1a
   self-contradiction, the phase-1.5 scope, the `similarity` ownership row, the
   four-way duplication.

Plus one standing recommendation that is not a defect: **defer the inert
machinery** (§4.1) and the column-newtype half of the rope sweep (§4.2) out of
phase 1a. Keep the trace-record columns; they are the only part that is
genuinely unrecoverable later.

The design's core judgement — measure everything, freeze the oracle, abstain
rather than block, keep languages independent — is right, and the parts of it
that will be built first are the parts that are best argued. The risk is not
that it is wrong. It is that it is *finished*: 10,600 lines of interlocking,
cross-cited commitments written before a single measurement exists to check any
of them against.
