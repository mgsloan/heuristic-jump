# Findings — core, after 37a6d098

**Verify a gap against the code before working it; the audit lags by hours.**
`git log --since` the `last_audited` stamp in `state/audit/core.toml`. One
recorded `crashed` may still have committed real work.

**Two documents in a row were satisfied but unheld. Assume the third is too.**
`deps.md` (51628b98) and now `rope-modifications.md`: twenty-four sections
closed across them, not one line of implementation between them. They were
written before the code and the code against them, and nothing notices the two
drifting apart. Budget for mechanism, not repair.

**`rope-modifications.md` is finished — do not re-read it.** Its thirteen §4/§7
sections are held by `vendor/rope/tests/newtype_api.rs`, and
`core.md#vendoring-the-zed-crates` by `seam.rs`'s
`every_vendored_crate_records_the_patches_it_carries`.

**Run the control before writing the commit message.** It found holes in three
of nine checks. `impl std::ops::Add for LineIndex` compiles and walked through
a scan matching traits by whole path. `pub fn new(row: impl Into<LineIndex>)`
compiles, keeps every call site, passes both bare-primitive scans, and is the
hole §4 rejects by name. And `use gpui;` cannot be planted — it fails the
*build*, so no test runs and the output holds no "FAIL". **Grep for `test
result`.** Where the compiler is the real enforcement, keep the assertion and
say so in its doc comment.

**A floor does not hold a list.** `named.len() >= 30` over a table parsed from
the document passed when I deleted a row. Transcribe the list into the test:
the second copy is the mechanism, not a smell.

**Parse the design document as the fixture** where a claim is a list or a count
(`deps.md` §15, `rope-modifications.md` §4). It is the only shape where editing
the *document* fails — which is the one way of faking progress the audit
cannot catch.

**Fetch upstream to settle a fact about upstream.** `curl` to
`raw.githubusercontent.com` at the pinned rev works here, and answered in one
turn a count I nearly escalated.

**The gate now runs `rope` and `sum_tree`** (`conformance-003`; lint and
fmt withheld). `clippy.toml` disallows `std::fs::read_dir` in the
crates that *are* linted — walk a crate root's `mod` declarations instead.
`rustfmt --edition 2024 <file>` after any non-`Edit` write; the hook misses
those and `cargo fmt -p rope` reformats upstream.

**Known open, descending value:**

* **`deps.md#licensing`** — cheapest remaining. `lang_*`'s licence is Class B
  and ends in a decision record; `seam.rs`'s `expected_licence` is where the
  two rules are distinguished.
* **`deps.md` §7 needs `notify` optional-and-absent settled; §8 wants `shared`
  to export `pub type Map`/`Set`** — five files use `FxHashMap` directly.
  §3, §9, §10, §11 and `#fxhashmap` are also open and none is held.
* **The driver request path** (`#5-deadlines`, `#both-sides-are-sets`) is
  phase 2b. Escalate the phase question before building it.
* **`#85`'s corpus** needs pyright and gopls; only rust-analyzer is on `PATH`.
* **`#9-workspace-layout` can never close** — it names `lang_python` and
  `lang_typescript`, outside every owned path.

**Still true.** Stage explicit paths, never `git add -A`; `harness/**` is
denied and may be edited concurrently. Loop: commit, gate, `hj record`.
Subset, never equality, for dependency tables. Never scan `Cargo.lock` for
§13's rejects — six are in it transitively; assert declarations.
