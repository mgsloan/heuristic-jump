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

## Campaign 20bbc1bf — five stale gaps, and what the sections had underneath

Assignment: `core.md#two-modes-collect-and-replay[6bd547104d]` and
`rope-modifications.md#the-signatures[a163ac3aee]`. Both stale. So were three
more I checked before claiming, and one I claimed and then found stale.

### The staleness check is now mechanical and takes one turn — use it

```
python3: for each state/audit/*.toml section with gaps,
         compare last_audited against `git log -1 <the gap's where-file>`
```

Sixteen gaps, one turn, and it prints `STALE?` per gap. **But it is necessary
and not sufficient, in both directions.** `deps.md#8-parse-cache` reads "fresh"
by timestamp and is closed anyway — the fix landed in `driver/tests/snapshots.rs`
and the gap's `where:` names `driver/src/trees.rs`, so the file the gap points
at never moved. Two of the five I checked were like that. The reliable check is
still the timestamp *plus* one grep for the thing the gap says is missing; the
timestamp just tells you which gaps deserve the grep.

The gaps that turned out to be live were the two nobody could reach cheaply,
both in `crates/driver/src/actor.rs`.

### What a stale assignment is actually worth

Not nothing, and this is the lesson I would want a fresh session to take. The
*section* was still the right target both times, and re-reading it with the
gap's claim in hand found a defect one step further along the same sentence:

`6bd547104d` says the checkpoint appends every N positions and the resume
miscounts. `4c50a45` fixed that. What it did not fix is the *other* end of the
same window — `collect` appends every answer and rewrites the header last, so a
run killed between its final `append` and `Writer::finish` holds every answer
and still says `complete: false`. `Truth::read` refuses that. And the resume
could not lift it either: `done >= all.len()` logged "already collected" and
returned. The only remedy was `--restart`, which re-spends the machine-hours
those rows already paid for — on the artifact §7 says should be regenerated
rarely.

So: read the section, not the gap. The gap is a statement about a repository at
a timestamp; the section is the claim.

### Where the fix went, and the shape I rejected first

I first wrote a `seal(path)` that rewrote the header, with the arithmetic
staying in `collect`. That is testable but tests the wrong thing: the bug was
`collect`'s early return, and a test of `seal` alone would have passed against
it. What is committed instead is `truth::resume_collection(path, wanted,
positions)`, which makes the *whole decision* — header drift, answered count,
seal — one call that `collect` delegates to entirely, and hands the rows back
so there is no second read. Public for the reason `check_resumable` is, and its
doc says so.

The second shape I rejected: `Writer::create` + re-append + `finish` for the
seal. It truncates the file first, and on the sealing path the rows being
written back are the only copy there is — a crash mid-append would lose a
collection to a call whose only job was to flip one field. `rewrite_complete` is
one `fs::write`, and `Writer::finish` now goes through it too.

### A nextest race that `cargo test` hides

Adding two fixtures made an existing flake frequent: `a_digest_group_names_...`
and `the_digests_concrete_sample_...` both built `fixture("digest_sample")`, and
the name is the corpus root, which `fixture_of` clears. Under nextest — process
per test, several at once — they delete each other's checkout mid-run. It fails
as `ProjectError::Read` on a source file that was there a moment ago, in
whichever test lost.

**It reproduces on a clean tree about one run in three, and never under
`cargo test`**, whose threads-in-one-process interleave less. I confirmed that
by stashing my change and running nextest three times before touching it —
worth the two turns, because the alternative was believing I had broken it.

Worse than a name clash: the two wanted *different* repositories (one file
versus two), so the join under test was being asserted against whichever
checkout survived. Renamed, plus `no_two_tests_build_a_fixture_under_the_same_name`,
which is a scan of the file's own `fixture("...")` literals.

### The licensing gap: stale, and the same retraction had two stragglers

`9d0b19a109` says `high-level.md` still claims rope is the only GPL input.
`e1136f3` fixed it an hour before the audit ran on a tree without it. But
`deps.md` §5 — the document that *wrote* the retraction — still had the
retracted argument four paragraphs above its own table: "the portable and
valuable part of this project is `similarity` and the `lang_*` handlers …
Marking those MIT means … the whole workspace becomes permissively licensable
without relicensing a line", against a table marking both GPL-3.0-or-later and
a paragraph two below saying that exit is closed. CHANGE-core-016.

The mechanising half is the one claim in §5 that is about an *edge*: no MIT
member depends on `similarity`. Every existing test holds `license` *fields*,
and a field is what a licence claims — add `similarity` to `shared` and every
field still reads MIT, every one of those tests still passes, and the permissive
surface the section promises is gone. Planting it on `shared` does not work:
cargo refuses the dependency cycle. `measure_core` is the plant, and §5 names it.

### `core-017`, and the half of its ruling the driver cannot reach

Reconciled. The cap now drops the answer and keeps the classification, which is
the ruling exactly: *a-priori* is about the rule, the rule reads only the query
and the reference, so the prior was never the outcome's to carry away.

**The ruling's last paragraph is not implementable as written, and I want that
recorded rather than rediscovered.** It says the parse-expiry case "resolves the
same way — a query abandoned before any handler ran still has a prior, because
the reference and the query are all its rule needs." True of the rule; the
*driver* has no reference, no resolution vocabulary and, by `core.md` §1's
design, no way to ask a handler for one. Worse, the case that will actually
occur in the field is not the abandoned parse: it is a handler that classified
the reference and then hit `ProjectView`'s expiry on a read and returned `Err`
via `?` — which §1 explicitly expects handlers to do — and `Result<Outcome,
Error>` gives an `Err` no way to carry a stratum out. That is `core-025`, and
all three answers to it are Class B.

So the residue keeps `Unimplemented` and is tagged. What changed is its size: it
was every capped answer, and it is now only a query nothing ever classified.

**How to reach the cap's drop from a test.** Not by queueing the request past
the deadline — that is what `the_deadline_is_measured_from_arrival_...` does,
and it expires in the *parse*, in front of the handler, so it never produces an
outcome to drop. The handler has to advance the fixture's clock from inside
`goto_definition`. `Slow` in `tests/actor.rs` does that, and it is the only way
I found to get an `ExpiredStrata::Assigned` end to end.

### Planted every assertion before believing it

Six plants, all of which failed the way they should: the seal never firing, the
seal always firing, a `std::process` marker in `replay.rs`, a second
`QueryRecord::new`, a `similarity` dependency on `measure_core`, and the cap
dropping the stratum again. The last printed the whole row with
`"stratum_prior":"unimplemented"`, which is the gap's sentence verbatim. Two of
the six passed against the *unplanted* wrong version first — worth the turns.

## Campaign 636bbd45 — a spec claim whose second half an answered decision had already refused

Assignment: `deps.md#8-parse-cache[fb0aa10250]`, one gap. Not stale — the first
assignment in several rounds that was live when I got it, and the staleness
check (`git log -1` on the `where:` file against `last_audited`) said so in one
turn because the section had been re-audited that evening.

### What the gap actually was, and why the fix is a document and not a cache

§8 said the parse cache "is keyed by `(uri, version)` for open docs and
`(path, mtime, len)` for disk files". The first is `driver::TreeCache` and is
real. The second is a cache **nothing has and nothing here may build**: the
only route to a disk-file parse is `ProjectView::parse`, behind the `Sync`
`&Query` several fan-out threads hold, and `conformance-005` was answered *no*
to a cache there — "no new caching or indexing until the corpus harness shows
the change is worth it and there is a benchmark", plus the fact that a cache on
`&self` is the lock this design does not have.

So the ruling had been applied to the *code* — `project.rs`'s module doc and
`trees.rs`'s `ParseKey` doc both say the disk half has no cache to be a key of
— and never to the *document that the ruling contradicted*. That is the shape
worth remembering: **when a decision record is answered, the code gets fixed
and the design section that stated the refused thing usually does not.** The
audit finds it later as a gap, one campaign per straggler. Anyone reconciling
an answered decision should grep the design corpus for the sentence, not only
the code.

### The one thing this edit deliberately did not do

`open-questions.md` question 5 asks whether `(path, mtime, len)` is sound at
all, since second-granularity mtime serves a stale tree for a same-second
rewrite of the same length. Deleting the disk key from §8 would have quietly
answered it — the question would still be in `open-questions.md` and its
subject would no longer be anywhere in `deps.md`. Numbered open questions are
Class B. So the key stays written down as the one that *would* be used, marked
as having no cache, and §8 now says in as many words that deferring the cache
defers the question. That is the reading that trades nothing off, and it is the
whole reason this stayed Class A.

### The test, and why equal length is the entire point

`a_second_parse_of_the_same_path_is_a_fresh_parse` writes `struct Beta;\n` over
the fixture's `fn beta() {}\n` — thirteen bytes either way — and asserts that
`read` returns the new text and `parse` returns a `struct_item`. Same path,
same length, and on any filesystem whose mtime resolution is a second, the same
mtime. So it fails against the exact key §8 claimed rather than against caching
in general, which a shorter or longer rewrite would not have done.

Planted both halves separately, each with a `thread_local` map so the plant
needed no lock and no signature change: a path-keyed cache in `read` (the read
assertion fired), then a `(path, len)`-keyed one in `parse` (the parse
assertion fired). Two plants, two runs, and worth the four turns — the first
assertion in this test is a *precondition* check (`function_item`), and a test
whose first assertion is the only live one passes for a reason nobody chose.

### Extending: two refusals and a stale grant

`hj claim` refused `deps.md#10-errors[d50e2285d0]` and
`deps.md#2-channels[8e707386b4]` — both live, both held by other workers, which
matches three campaigns' worth of findings saying `actor.rs` and `dispatch.rs`
are where the remaining work is. One turn each, and the right turns.

It granted `deps.md#fxhashmap[e83fd58b7a]`, which is stale for the third round
running: `shared::Map`/`Set` are at `shared.rs:85`, and
`the_default_map_and_set_are_the_aliases_shared_exports` in
`driver/tests/seam.rs` already scans **every** workspace member for
`rustc_hash`, `FxHashMap` and `FxHashSet`, exempting only the file that defines
the alias, with vacuity guards on both the member list and each source. There is
nothing left to do to that section and no commit came of taking it. **Do not
take it again** — the gap's own suggestion ("a scan for `rustc_hash::` outside
shared would make this mechanical") was implemented by campaign 5cc94daa, and
the gap has simply never been re-audited since.

That is the round's cost lesson: a granted claim is not evidence the gap is
live. The claim ledger knows who is working on what, not what is true.
