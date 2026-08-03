# Criticism of `design/` and `readme.md`

Written against the tree at `5aa4a85`, covering `readme.md` and all ten
documents in `design/`, and revised after each round of fixes — findings that
have been acted on are removed rather than kept with a note, so what remains
is what is still open. It has shrunk by more than half.

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

**The measurement half of this has been addressed and the structural half has
not.** `core.md` §7 now records latency at every point it can be — queue time,
per-stage, whole-handler, and the real server's — and `loops.md` §10 carries
per-stratum percentiles and the deadline-abstention rate in every metrics row.
So the trade is now *visible* during phase 2a.

What remains is that nothing *acts* on it. Phase 2a still gates on quality
alone and phase 3 still may not change an answer, so a handler that only wins
on an unbounded clock can be built, watched, recorded, and then handed to the
one phase forbidden from fixing it. That is a deliberate choice rather than an
oversight, and it may well be the right one — recording first and deciding
later is the same posture the replay-time target took. It is listed here
because the decision point is real and unscheduled: someone has to look at the
deadline-abstention rate after the first corpus run and say whether phase 3's
equality constraint survives it.

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

### 2.3 Irreversible decisions are welded to the corpus without saying so

The corpus costs ~100 machine-hours, gates phases 2 through 7, and
`data-collection.md` says plainly that losing the directory loses it — it
cannot be reconstructed from the manifest. Durability has since been ruled on
deliberately (a clean checkout at a pinned SHA, nothing more), so that is not
the complaint.

The complaint is that several cheap-looking decisions become 100-machine-hour decisions because
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
Durability itself has been ruled on and is deliberately thin: a clean checkout
at a pinned SHA, no checksums and no archive. That is a defensible trade — it
catches the failures that actually happen and needs nothing maintained — and it
leaves the *irreversibility* unaddressed, which is the part still worth a
paragraph.

**What is missing is a stated "what invalidates the corpus" list**: repository
commit, grammar revision, server version, the identifier-shape rule, and the
sampling seed. Each of those is chosen somewhere in `data-collection.md` as a
small local decision, and each silently costs ~100 machine-hours to revisit.
Writing them down in one place is what makes the cost visible at the moment of
choosing rather than at the moment of discovering.

---

## 3. The constants that decide behaviour are absent

Not a disagreement — a place where an implementer following the document
arrives at a question it does not answer.

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

### 5.2 Worktree cost is priced in RAM only

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

### 5.3 Smaller points on `loops.md`

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

### 6.1 Handler-side branching on server identity is forbidden by a comment, not by a type

**Two different things are keyed by server identity, and only one of them is
allowed to branch on it.** `core.md` §1 says so directly — `ServerAdapter`
lives in `driver`, is matched against `serverInfo.name`, and exists precisely
to do per-server things (`peek_mode`, `definition_indicates_ready`).
`ServerProfile` lives in `shared`, reaches handlers, and must *not*:

> handlers must not dispatch on server *identity* — `if server.id == PYRIGHT`
> scattered through a handler is the per-language configuration format
> `resolution.md` §1.2 rules out, wearing yet another hat. A handler reads a
> field describing a behaviour; it does not ask which server it is talking to.

`resolution.md` §1.4 repeats it. So identity-branching **is** part of the
design, in the driver; it is banned in handlers, where a per-server `if` is
the rejected config format arriving by another route.

The criticism is not that the rule is wrong — it is right, and the two-surface
split it protects is one of the better ideas in the design. It is that the
rule is enforced differently from every comparable rule here. `ProjectPath` is
unforgeable, `WirePosition` is inert, handlers cannot construct
`Outcome::Committed`. Here the guard is a doc comment, on a struct whose only
member is the thing it forbids reading and which is also sufficient to detect
standalone mode — something `resolution.md` §1.1 separately says a handler
must never learn.

**That the distinction is easy to misread is itself the evidence.** "The
tool's behaviour varies by server" and "a handler may ask which server it is"
are one sentence apart in the prose and opposite in the design.

Two fixes, either sufficient: keep `ServerProfile` out of `Query` until it has
a real behaviour field (§4.1 argues for that anyway, since it is empty), or
make `ServerId` opaque to handlers — no `PartialEq`, no exported constants —
so the comparison does not compile. The second costs nothing and turns a rule
someone must remember into one they cannot break.

### 6.2 Assorted smaller ones

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
* `high-level.md` scopes out `ExternalDependency` (predicted 4% of queries,
  0% coverage) while its own Future Work says "jumping into a dependency is a
  common go-to-definition." Those two sentences are about the same stratum and
  they disagree about how common it is — and §1.2's value weighting suggests it
  is also where the slow-LSP tail lives. The corpus now records the per-position
  server latency that would settle which of the two sentences is right.

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

One thing left, and it is deliberate rather than an oversight: **the first
eighteen lines are byte-identical to `high-level.md`'s**, while the readme
tells the reader `high-level.md` is the one document that stands alone. Kept
on purpose — the pitch is short and a reader arriving at either file should
get it — so this is a note rather than a finding. If those two ever say
different things about what the tool is, that is when it matters.

## 9. The short version

If I could change two things:

1. **Make the latency-weighted stratum distribution a gate condition on
   entering phase 2a** (§1.2). The design says the headline metric may be
   nearly uncorrelated with value, and now records the per-position server
   latency that would settle it — but nothing looks at it before the dominant
   budget is spent.
2. **Defer the inert machinery out of phase 1a** (§4.1). The seam freezes at
   that gate, and `CommitPolicy`, `Confidence`, and `ServerProfile` are all in
   `Query` or `Outcome`. Keep the trace-record columns; they are the only part
   that is genuinely unrecoverable later.

Everything else here is a smaller correction or a note about volume.

The design's core judgement — measure everything, freeze the oracle, abstain
rather than block, keep languages independent — is right, and the parts of it
that will be built first are the parts that are best argued. The risk was
never that it is wrong. It is that it was *finished* before a single
measurement existed to check any of it against; the last several rounds of
edits have been that finish coming off, which is the right direction.
