---
id: core-022
status: open
opened: 2026-08-04T20:12:00+00:00
campaign: 2dca52ce-54ab-4b4b-ac4a-dd26359dc080
kind: class-b
---

# How does the driver obtain a query's a-priori stratum when no handler ever ran?

## Context

`core-017` is answered, and its answer is the reason this question is now the
only one left. It rules that the prior survives a discarded outcome:

> *A-priori* is about the **rule**. The handler evaluates it; what makes the
> prior stable is that the rule reads only the query and the reference and never
> what the search found. So the prior is knowable without the search finishing,
> and **a query whose outcome the hard cap discards has not lost its prior** — it
> was never the outcome's to carry away.

That is implemented. `Dispatched::DeadlineExpired` now carries a `Classified`,
the hard cap fills it from the answer it drops, and an expiry raised downstream
of the handler — inside §8.4's conversion, which reads the target file — picks
the same classification back up.

What is not implemented is the case core-017 disposes of in one sentence:

> The parse-expiry case resolves the same way. A query abandoned before any
> handler ran still has a prior, because the reference and the query are all its
> rule needs.

It does *have* one. Nothing here can *evaluate* it. The rule is
`resolution.md` §8's, which is per-language by construction — `resolution.md`
§1.2 refuses to centralise exactly this kind of judgement — so the only thing
that can assign a stratum to a query is the handler, and in this case the
handler never ran. Two paths reach it: `SnapshotSeed::realise` abandoning the
parse on the deadline, and a `ProjectView` read inside the handler expiring
before it had classified anything.

The cost is the one `core.md` §1 names. The row is an **abstention** under
`unimplemented`, and `measure_core`'s `Table::template` reads an abstention
under that stratum as "the template has not been replaced"
(`crates/measure_core/src/table.rs:158`) — deliberately, because that counter is
the one thing no real handler produces. Under load, a real handler now produces
it.

## Options

**A — a seam method.** `LanguageHandler::stratum_prior(&self, q: &Query) -> Stratum`,
evaluated by the driver before dispatch, so every query has a prior whether or
not anything got as far as answering. It is what core-017's reasoning implies:
if the rule reads only the query and the reference, it can be asked for
separately. Costs a change to the trait `state/phase.toml` freezes at this
phase's gate, and a second entry point every `lang_*` crate must implement
consistently with its own `goto_definition` — two places that can disagree
about the same query, where today there is one.

**B — a nullable `stratum_prior`.** §7's record gains `null` for "nothing
classified this", and the table grows a row for it. Honest, and it costs §7's
record shape: `StratumName` is not an `Option` today, every consumer groups on
it, and `measure_core`'s replay writes the same column. It also spreads the
question — every consumer now decides what a null prior means for its
denominator.

**C — keep the placeholder, and weaken the template check.** `Table::template`
stops reading raw abstentions under `unimplemented` and reads something
narrower. Cheapest, and it pays for the row by giving up the check `core.md` §1
calls "a gate check rather than something anybody has to notice" — a template
that abstains on every query and a handler whose deadlines all expire become the
same table.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**C's status quo half, without C's weakening**: the row keeps
`Strata::from_reference(Stratum::Unimplemented)`, and `Table::template` is left
exactly as it is.

It is the most reversible because it is a value at one site and no type anything
else names — the same reason core-017's provisional was reversible — and because
it leaves both instruments telling the truth as they see it: the record says the
query was never classified, and the template check says an `unimplemented`
abstention was observed. What is wrong is that those are the same sentence for
two different situations, and that is precisely what a human is being asked to
separate.

Tagged at `crates/driver/src/dispatch.rs`, in `Classified::strata`'s
`Classified::Nothing` arm — the one place a stratum is now invented, down from
the two before this campaign.

`crates/measure_core/tests/pipeline.rs`'s
`the_template_check_reads_an_abstention_no_handler_classified` holds the
consequence from the table's side, so whichever way this goes there is an
executable statement of what it costs rather than a paragraph.

## Consequences

**A**: the trait gains a method, `Registry`'s callers gain a call before
dispatch, the two `Classified::Nothing` construction sites take the prior the
driver already holds, and `Classified` collapses back to a plain `Strata`. Every
`lang_*` crate written before the answer needs the method added — today that is
the template and nothing else, which is the cheapest this will ever be.

**B**: `QueryRecord::stratum_prior` becomes `Option<StratumName>`, `Table::row`
grows a case, `measure_core`'s replay and the truth-row join both change, and
the tagged site becomes `None`. Wider than A, and none of it is in `driver`.

**C**: the tagged site is unchanged and `Table::template` changes instead, which
moves the work into `measure_core` and out of the seam entirely.

Rows written under the provisional are identifiable: a deadline-expired row
carries `abstain:deadline` in `stages`, so a corpus that needs the distinction
can recover it, and one that does not can drop the rows. That is core-017's own
disposal of the same problem and it still holds.
