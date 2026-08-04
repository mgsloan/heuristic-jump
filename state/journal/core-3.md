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
