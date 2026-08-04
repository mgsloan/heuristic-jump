---
id: core-024
status: duplicate
opened: 2026-08-04T20:45:00+00:00
campaign: 5cc94daa-a1bb-476a-9255-e10177487c15
kind: class-b
---

# What does §7's record carry when the deadline expired before anything classified the reference?

## Context

This is the residue of `core-017`, and it is narrower than that record was.

`core-017` asked what stratum a query carries when the hard cap dropped the
outcome that knew it, and was answered: the prior is a fact about the
*reference*, not about the answer, so "a query whose outcome is discarded after
the fact — by §5's hard cap, say — has not thereby lost its prior. The prior
was never the outcome's to carry away." That is now implemented.
`Dispatched::DeadlineExpired` carries a `LateStrata`, and both paths where a
handler classified the reference before running out of time keep it:

* the hard cap, which drops an answer that came back late; and
* `encode`, which reads the target file to convert the answer and is refused by
  `ProjectView` when the deadline has expired — the more common of the two,
  since it fires for every cross-file definition.

What is left is the paths where **nothing classified anything**, which
`LateStrata::Unclassified` names:

* `realise` abandoning the parse, so no handler ran at all; and
* a handler propagating a refused `ProjectView` read with `?` before it had
  classified the reference.

`core-017`'s answer covers these in one sentence — "a query abandoned before
any handler ran still has a prior, because the reference and the query are all
its rule needs" — and that is true of the *rule* while being unavailable to the
*driver*. `LanguageHandler` has one method and it is `goto_definition`; there is
no way to ask a handler for a prior without asking it to resolve. So the driver
has nothing to write, and today it writes `Strata::from_reference(Stratum::Unimplemented)`
— which is what `core-017` ruled against, for the reason `measure_core`'s
`Table::template` reads an *abstention* under `unimplemented` as "the template
has not been replaced".

The blast radius is much smaller than it was. Before this campaign every capped
query wrote that row; now only a query whose deadline expired before its
handler classified anything does, which on a warm parse cache means a query
that was already over budget when it was dispatched.

## Options

**A — a prior-only entry point on the seam.** `LanguageHandler` gains something
like `fn stratum(&self, q: &Query<'_>) -> Stratum`, evaluating `resolution.md`
§8's a-priori rule and nothing else. It is what `core-017`'s answer describes
most directly, and it makes the prior genuinely independent of whether the
search finished. It costs a method on the frozen seam that every `lang_*` must
implement, a second place the classification rule lives (with the obligation
that the two agree), and it is unreachable in the parse-abandoned case anyway —
`Query` needs a `DocumentSnapshot`, which is exactly what `realise` failed to
produce.

**B — `stratum_prior` becomes nullable, for this case only.** Honest: nothing
classified the reference, so there is no prior, and `null` says that. It costs
§7's record shape, which `measure_core`'s replay writes too and every consumer
groups on — and a nullable denominator key means every table has to decide what
to do with the rows that have none. `core-017` priced this as Class B and did
not choose it.

## Decision

**Closed as a duplicate of `core-025`**, which carries the answer. Logged as a
`decision-answered` intervention on 2026-08-04 so §16's status derives from the
log rather than from this line.

This record and `core-025` are the same question, raised independently by
different workers of round 1 because the claim system was granting every request
— `campaign_is_alive` asked the process table, which the OS sandbox had made
private, so from inside any campaign every live sibling read as dead and no
claim was ever refused (fixed in `c047b4c`). Campaign `5cc94daa` wrote this
one.

Nothing here is wrong and nothing is discarded: the framing differs and the
reasoning is worth keeping, which is why this is closed rather than deleted. The
ruling, its argument, and the work it leaves are in `core-025`.


## Provisional choice in force

**`Stratum::Unimplemented`, still**, at the one remaining site:
`Actor::answer`'s `LateStrata::Unclassified` arm in
`crates/driver/src/actor.rs`, tagged `// DECISION-core-024: provisional`.

It is the most reversible because it is a value at a single site rather than a
type anything else names — the same argument `core-017` made for it, now
holding over a much smaller set of queries. Both alternatives change something
a crate this loop does not own has to read.

Worth being explicit about what this does *not* leave broken: a real handler
that misses its deadline no longer writes an `unimplemented` abstention, so
`Table::template`'s gate check is no longer tripped by slowness. What can still
trip it is a query dispatched with its budget already spent, which is a
different and much rarer thing.

## Consequences

If the answer is A, the work is a seam method plus an implementation in the
template and in `lang_rust`, and the parse-abandoned case still has no answer —
so A probably has to be paired with B or with a ruling that the abandoned case
records nothing at all.

If the answer is B, the work is `QueryContext`/`Answered`'s `strata`, the
`StratumName` serialization, `Table::add`'s row lookup and whatever `measure`
does with a row that has no denominator. `crates/driver/tests/actor.rs`'s
`a_capped_answer_keeps_the_stratum_the_handler_assigned` is unaffected either
way: it asserts the classified paths, which are settled.

If the answer is "leave it", nothing is redone, and the thing to watch is a
corpus run reporting `TemplateState::Unreplaced` for a language whose handler
is real. That would mean queries are arriving already over budget, which is a
latency finding worth having on its own.
