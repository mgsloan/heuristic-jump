# Journal — core, worker 3

Written for a session that will not remember this one. `state/journal/core.md`
and `core-2.md` are the other workers'; none of them is a summary of the diff.

## 2c129b10 — `core.md` §8.4 and §8.5, the wire types against real input

Assigned both. Closed the §8.5 half by capturing traffic, found the §8.4 half
already closed, and picked up two audit minors and a third section on the way.

### The thing worth knowing before anything else: the gap list goes stale

**Three of the gaps I touched this campaign were already closed when I got
them.** Not partly — closed, with the test that carries the claim sitting in
the repo.

- `#84[f2b9c0b7e5]` says "`Location::line()` is never read by the driver".
  `dispatch::encode` has read it since `fe50a83`, and
  `driver/tests/wire_locations.rs` asserts the disagreement path. Its `claim`
  is a sentence §8.4 does not contain any more — it was retracted under
  `conformance-005`.
- `#vocabulary-types[fbe658c158]` says `shared` re-exports four of `rope`'s
  seven text newtypes. `shared.rs:54` re-exports all seven and
  `seam.rs:24` names them all in one `use`, which is the exact mechanical
  check the audit entry asks for. I spent a claim on this.
- `#adding-a-language[0858868078]` says `heuristic_jump` does not depend on
  `lang_rust`. Its manifest does, with a comment citing §9.

The mechanism is in the audit itself and is visible in `state/audit/core.toml`:
every section carries `last_audited`, the auditor re-judges about seven
sections per run, and **the gap list handed to a worker is not re-derived
against head.** So a gap opened at 06:54 and closed at 20:10 is still in the
list at 04:18 the next morning. The old cohort at the time of writing:

| audit run | gaps still listed | verdict |
|---|---|---|
| 06:54 | `ce5dfefab5`, `fbe658c158` | `fbe658c158` closed; `ce5dfefab5` partly — `lang_rust`, `measure_core`, `measure_rust` now exist and are members, the four still missing belong to later phases |
| 07:56 | `0858868078`, `6e601d5bd1` | `0858868078` closed; `6e601d5bd1` at least partly — `driver::run` exists |
| 09:16 | `d2a209c7a8`, `f2b9c0b7e5` | `f2b9c0b7e5` closed |
| 19:21 onward | the rest | fresh; treat as real |

**Check `last_audited` before you claim.** It costs one `python3` read of
`state/audit/core.toml` and it is the difference between a campaign and a
verification exercise. A gap whose section was last audited before the
campaigns that targeted it is a suspect, not a target.

What I did *not* do, and think is right: I did not edit the audit. It is
denied, and a loop that could quiet its own instrument has no instrument.

### What actually worked: capture, do not compose

The whole §8.5 gap came down to whether real servers could be obtained. They
could, and cheaply — this environment has network:

    go install golang.org/x/tools/gopls@latest        # ~/go/bin/gopls
    npm install --prefix /tmp/... pyright@1.1.411     # pyright-langserver --stdio

Both then drive over stdio with about a hundred lines of Python. The procedure
is the one already written in the corpus header, and the only thing it got
wrong is that **pyright emits no `$/progress` at all** on a small package, so
waiting for the indexing end never returns — a fixed sleep is what that one
needs. gopls does emit begin/end and the wait works.

Four things the real messages said that the hand-authored half had guessed
wrong, and they are the argument for §8.5's mitigation in miniature:

- Asked about a place with no definition, **neither server answers `[]`**.
  Both answer `null`. `[]` is the residual ambiguity §8.5 names and nobody
  here has seen a server send it.
- **pyright 1.1.411 sends no `serverInfo`.** The hand-authored line labelled
  "pyright" invents one.
- **gopls' `serverInfo.version` is a three-kilobyte JSON build document**
  embedded in the string — module versions, sums, build settings.
- pyright answers a call to `print` with **two locations 2075 lines into
  typeshed**, which broke the differential's 96-row `GRID` exactly as that
  fixture's comment predicted it would. Every hand-authored position in the
  corpus is inside the first hundred lines, because somebody writing one picks
  a small number. Real answers point into standard libraries.

None of it needed a change in `proto.rs`. Both readings agreed on all nine new
messages, which is the result the differential exists to produce.

### The half that cannot be done here, and why it is not laziness

`initialize` params and document traffic are **composed by an editor**. There
is nothing to elicit them from, so capturing them means running Zed or VS Code
against a recording binary. Zed *is* installed here and `DISPLAY=:0` is a real
X server — the user's own desktop. Starting it opens a window on a desktop
somebody is using, and producing `didChange` traffic means typing into it. I
did not, and a future campaign should not: it is `core-018`, and the answer is
a human's.

The route, for whoever does it: Zed's `lsp.<server>.binary.path` setting can
point at a wrapper script that tees stdin to a file and execs the real server.
That yields the whole client half — `initialize`, `didOpen`, `didChange` from
real typing, `didSave`, `didClose` — in one sitting. Do not hand-author them
and label them CAPTURED; `a_captured_message_agrees_with_the_server_its_label_names`
will not catch you, but the point of the corpus is precisely that it holds
what nobody predicted.

### Two doc comments that claimed mechanisms they do not have

Both were audit minors and both are the same defect, which is why they are
worth naming as a pattern rather than as two fixes:

- `proto.rs` said handlers cannot construct a `WireLocation` "and the reason
  is the encoding rather than the visibility". `PositionEncoding`'s variants
  are public unit variants in a public module, so a handler writes
  `PositionEncoding::Utf16` and the argument collapses. What holds it is a
  source scan in `seam.rs`, whose own doc has said so correctly all along.
- `vocabulary.rs` said `LanguageId` comparison "is then a pointer comparison
  rather than a `str` one". It is content equality, and it has to be: the
  registry resolves an incoming id against ids a `lang_*` crate declared, and
  those `"rust"` literals are in different crates.

**A doc comment that names the wrong mechanism is worse than one that names
none**, because the next reader concludes the real mechanism is redundant and
deletes it. Both fixes are comment-only; the `LanguageId` one gained a test
that leaks a runtime-built `"rust"` so the compiler cannot answer the
comparison by merging two equal literals.

### The Class A edit, and what I was careful about

`CHANGE-core-005`. §8.4 says the conversion "re-reads the target file, once
per location" without exception, where `dispatch::target_text` clones the
snapshot's rope when the target is the query's own document. The section
already concedes the case a page earlier — "the target is frequently a file
the editor never opened" — so the universal statement contradicted its own
setup, and it is the *ordinary* case it got wrong.

I did not touch the code it describes, and said so in the changelog. What I
added instead is the test that makes the sentence checkable:
`a_target_in_the_query_s_own_document_is_encoded_without_reading_it` deletes
the document's file from disk before dispatching. That is the only way to tell
"did not read" from "read and got the same bytes", since the snapshot and the
file hold identical text by construction. Disabling the short circuit fails it
and nothing else.

## Campaign ede3701b — the digest's inputs, and one wrong verdict

Assignment: `core.md#the-table-is-not-enough-a-replay-has-to-show-its-failures`
(unjudged) and `core.md#what-the-templates-handler-does[9adb0be268]`.

### The templates gap was stale, again

`Table::template()` and
`the_unimplemented_stratum_identifies_the_template_and_not_a_broken_handler`
landed in `933a4aa` at 21:22 UTC; the section's `last_audited` is 20:56 UTC,
26 minutes earlier. Nothing to do. This is the third campaign on this worker to
open with a verification exercise instead of a target, so I stopped writing it
into the findings and filed it as `core-019` (`harness-request`) with the
measurement: seven of the nine `core.md` gaps have a `where:` file that moved
after the audit that opened them, and three of those are verifiably closed.

### What the section actually needed, and what I did not try

The section splits across two owners. `harness/measure` is the other one and is
`core-001`, open, and `harness/**` is denied — so the temptation is to write
the digest somewhere I *can* write. I did not, and the next campaign should not
either: the section says in as many words that digesting "is the harness's job,
not `measure_core`'s — the same split that keeps `measure_core` ignorant of
`state/`". A digest in `measure_core` would satisfy an auditor and break the
thing the sentence is protecting.

What was left on the measurement's side was not a feature but two properties of
what a replay already writes:

* the digest's **share of a stratum** takes its numerator from the records file
  and its denominator from the table, and nothing checked the two artifacts
  were two accounts of one run;
* the **sample** — "repository, file, line, the identifier, what we returned,
  what the server said" — is six fields of which none is a column, so the claim
  is a reachability rather than a schema.

### The reconciliation found a real one

Reconciling every counter failed immediately: 14 `mismatch` in the records
against 0 in the table, under a handler that only abstains. `replay::one` called
`Agreement::classify` for every row and passed `Some(agreement)` to
`answered_by`, while `Table::observe` judged only commits. §6 makes agreement
the classification of *the shim's answer* against the child's, and `ChildAnswer`
in `shared::record` already said so in a doc comment nobody had made
executable: "a query the shim never answered has no answer of ours to compare,
which is a different fact from the two sides disagreeing".

Where I put the fix, and why not elsewhere. The obvious repair is to filter in
`Table::observe` by `decision`, which leaves the record still carrying a verdict
— the table and the record would then hold different answers for the same row,
which is the exact failure the reconciliation exists to catch. The next obvious
one is `QueryRecord::answered_by`, which knows `self.decision` and could null
the columns in `shared` for both producers; I did not, because the table does
not go through it, so the rule would live in two places and only one of them
would be the enforcing one. The `Agreement` is now minted at one site, only for
a commit, and both consumers get the same `Option`. `Table::observe` takes
`Option<Agreement>` rather than filtering, so there is nothing to hand over
rather than something to ignore.

The driver was already right — `pending::answered_by_shim` stores `None` on
abstain, so `resolve` has nothing to classify. My first commit message implied
otherwise; the code comment now names where the other producer enforces it.

### The two-file fixture, and why the assertion needed one

Every fixture in `pipeline.rs` is one file, and the sample's identifier is
joined from the positions file. With one file a join on `(file, offset)` and a
join on `offset` alone are indistinguishable, and the wrong one finds a *real*
identifier from another file — which reads as a finding rather than as a bug.
`OTHER_SOURCE` therefore puts `gamma` at byte 7 where `SOURCE` has `alpha`.
Verified by planting the offset-only join: it fails with
"names `src/other.rs:0` and `alpha`, and that line reads `pub fn gamma()`".

### The rendering check, and the case that made it bite

The reconciliation is against `--format json`; a person reads the text table.
Comparing them is not a string comparison — the text prints coverage and
precision as percentages — and recomputing those is what exposes
`Row::precision`'s denominator, which its doc argues at length must be the
three agreement counters and not `committed`. Under a non-refining handler the
two coincide and the assertion is decoration. `ReportingHandler` against a
`null` oracle is the case that separates them: prior `explicitly_imported`,
settled `ambiguous_name`, an empty commit against a `null` answer is §6's
mutual match — so one row is all coverage and no judgement and the other all
judgement and no coverage, and the wrong denominator reads 0% where the right
one reads 100%. Verified by planting `committed` as the denominator.

### Dead ends and things not taken

* Both genuinely-fresh gaps outside the assignment — `#83[f9ad1766b7]` and
  `#adding-a-language[68be1693b1]` — were REFUSED by `hj claim`; other workers
  hold them. Two turns, and the right two: the alternative is a merge somebody
  resolves. Everything else on the list is stale, so there was nothing outside
  the lane to take.
* I considered a Class A edit to §7 for the `#7-observability` minor — replay
  writes `mode: "proxy"` with `server_health: null`, a third case the section
  describes neither as proxy nor as standalone. Defensible, and I left it: it
  is a minor rather than a gap so it moves no number, and editing §7 in the
  same campaign that edited `replay.rs` is the one shape the spec-drift rule
  watches for. If a later campaign takes it, the honest resolution is that
  `mode` says whether a second answer exists — replay's does, frozen — and
  `server_health` is null because there is no child to be healthy, which is a
  fact about the producer and not about the mode. `Mode::Proxy`'s doc comment
  in `shared::record` already argues exactly this.
