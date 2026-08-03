# Findings — conformance, after 51628b98

**Verify a gap against the code before working it; the audit lags by hours.**
`git log --since` the `last_audited` stamp in `state/audit/core.toml`. A
campaign the harness recorded `crashed` may still have committed real work.

**`deps.md` is largely satisfied and almost entirely unheld.** The manifests
were written against it and quote it in their comments, so its sections need
mechanism, not repair — one real defect in ten sections. I closed §0, §1, §2,
§4, §5-licensing, §6, §12, §13, §14, §15, all in
`crates/driver/tests/seam.rs`, which is where a manifest-shaped assertion
belongs. Remaining and *not* as cheap: §3, §7, §8, §9, §10, §11, `#fxhashmap`.

**Three assertions I wrote cannot be made to fail, and each taught the same
lesson.** Adding `[lints]` to `vendor/rope` is a duplicate TOML key — cargo
rejects the manifest and **no test runs**; adding one to `sum_tree` makes it
stop compiling on `elided_lifetimes_in_paths` alone. Dropping `serde_json`'s
`raw_value` fails to build, because `shared` already imports `RawValue` — §4's
"silently compiles" was true before its first user. Keep such assertions, and
write *in place* that the compiler is the real enforcement; it is incidental
and the assertion records intent. **Grep control output for `test result`, not
just for a failure** — my first vendor control produced neither, and I nearly
scored it as passing.

**Do not scan `Cargo.lock` for §13's rejected crates.** `once_cell`, `regex`,
`memchr`, `aho-corasick`, `walkdir` and `indexmap` are all in it transitively.
§13 is about what we reach for: assert declarations.

**§15 parses `design/deps.md` itself as the fixture.** It is the one assertion
where editing the *document* fails, which matters because moving the spec
toward the code is this loop's only uncatchable way of faking progress.

**Subset, never equality, for dependency tables.** §14's "each arrives with
its first user" makes `rayon`, `lru`, `insta` and `notify` chosen-and-absent
on purpose.

**Known open, in descending value:**

* **§5 states two licence rules and marks `lang_*` by the one
  `conformance-014` rejected** (a `license` field describes copyright in that
  crate's own text; `heuristic_jump` proves reaching GPL is not enough).
  `lang_*` may still be GPL because `similarity` is a *port*. `seam.rs`'s
  `expected_licence` is the only place the two are distinguished. Cheap, clean
  next target.
* **§7 cannot go clean until `notify` is settled** — it should be an optional
  dependency "so the decision is visible", and is absent. Its other claim
  (`ignore` in `shared`, not `driver`) already holds.
* **`shared` exports no `pub type Map`/`Set`**, which §8 requires; five files
  use `FxHashMap` directly.
* **The driver's request path** (`#5-deadlines`, `#both-sides-are-sets`) is
  `shim.md`'s transport and `core` actor — **phase 2b**. Escalate the phase
  question before building it. `conformance-016` (answered) already removed
  `clippy.toml`'s `unbounded` ban that would have blocked it.
* **`#85`'s corpus** needs pyright and gopls; only rust-analyzer is on `PATH`.
* **`#9-workspace-layout` can never close** — it names `lang_python` and
  `lang_typescript`, outside every owned path.

**Still true.** Never `git checkout` over uncommitted work. `harness/**` is
denied and was being edited concurrently — stage explicit paths, never
`git add -A`, then gate with `--rev HEAD`. Loop: commit, gate, `hj record`.
