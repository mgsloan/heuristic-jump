# Commands

*Provisional — the workspace does not exist yet. Fill in exact commands when it
does.*

* Build one crate: `cargo build -p <crate>`. Do not build the whole workspace
  routinely, and do not use `--all-features` — it expands the build matrix and
  `target/`.

* Lint: `cargo clippy -p <crate> --all-targets -- -D warnings`, on the crates
  you own. Must pass for every commit. The `--workspace` form is a phase-gate
  check, not a per-commit one — running it routinely means compiling every
  grammar to check one handler, which is the coupling `design/core.md` §7
  splits `measure_core` apart to avoid.

* **Do not run bare `cargo fmt` or `cargo fmt --all` incidentally** — it
  formats every workspace member including `vendor/`, and rustfmt's `ignore`
  option is nightly-only so `rustfmt.toml` cannot exclude them. Reformatting
  a vendored crate is permitted, but it produces a whole-crate diff that
  buries whatever real change was in the same commit. Formatting is handled
  per-file by a PostToolUse hook (`.claude/settings.json`); for a manual
  sweep use `cargo fmt -p <crate>`. If a vendored crate is to be reformatted,
  do it as its own commit that changes nothing else, and record it in
  `vendor/README.md`.

* Invoking `rustfmt` directly needs `--edition 2024`. Standalone rustfmt
  defaults to edition 2015 and hard-fails on modern syntax.

* Snapshots: update with `cargo insta accept`, never the interactive
  `cargo insta review` (it needs a TTY). Never hand-edit `.snap` files.

# Hard constraints

These are the ones that cost the most to get wrong.

* **No async runtime.** `tokio` is rejected. Concurrency is OS threads +
  `crossbeam-channel` + `rayon`. Do not add `async fn`, `.await`, or any
  executor. See `design/deps.md` §1.

* **No locks.** There is no `Mutex`, `RwLock`, `parking_lot`, or `dashmap`
  anywhere in the design; state is owned by one thread and moves over channels.
  Reaching for a lock means something is architecturally wrong — say so rather
  than adding one.

* **Do not add dependencies unprompted.** Be very conservative: no small helper
  crates, and prefer a shared in-repo utility over a new dependency. Explicitly
  rejected: `anyhow`, `tokio`, `num_cpus` (use `available_parallelism`),
  `once_cell` (use `std::sync::OnceLock`/`LazyLock`), `parking_lot`, `dashmap`.
  `lsp-types` is dev-only, as a differential oracle for our own protocol types —
  a runtime `use lsp_types::` defeats the point. If a new crate seems necessary,
  ask. `design/deps.md` records every choice and every rejection.

* **Vendored crates are ours to edit, but every edit has a re-sync cost.**
  `vendor/` holds copies of Zed crates. Editing them — reformatting,
  refactoring, fixing lints — is allowed; `design/rope-modifications.md`
  already rewrites rope's public API and concedes that a re-sync is a merge
  rather than a clean diff. What is still required is that the edit be
  *recorded*: `vendor/README.md` names the upstream revision and every patch
  applied, so a future re-sync can tell at a glance what changed and why.
  Read `design/rope-modifications.md` before touching `vendor/` — not for
  permission, but because it says what has already been decided there and
  what the tests are that verify it.

* **Tree-sitter grammars are pinned to the revisions Zed uses.** Do not bump a
  grammar crate. (The `tree-sitter` runtime version is ours to choose; the
  grammars are not.)

* **Use our own LSP protocol types, not `lsp-types`.** They live in
  `shared::proto` and exist so the newtypes are what deserialization *produces*.

* **`#[serde(untagged)]` only when the variants are disjoint by JSON kind or by
  a required field the others lack.** Never by an optional field, and never by
  declaration order — serde tries variants in order and takes the first that
  succeeds, so a lenient variant silently swallows values meant for another.
  `design/core.md` §18.5 has the worked example, where getting this wrong
  destroys documents.

* **One system-wide error enum**, not `anyhow`. Sub-enums are
  `#[non_exhaustive]`. **Do not use wildcard `match` arms on them** — a new
  variant should fail to compile until it is handled. That compile error is the
  entire point of the design.

* **Abstention is not an error.** `Outcome::Abstain` and `Result::Err` are
  different things and must not be merged.

# Testing

* **Do not write unit tests.** Coverage comes from integration tests (the
  overall metrics), property tests (`proptest`), and snapshot tests (`insta`).

* Commit `proptest-regressions/`. Never delete a failing-seed regression.

* No `#[should_panic]` — assert on the `Err`/`None` explicitly so the test says
  which failure it expects. No `#[ignore]`.

* Keep fixtures minimal. A fixture that exercises one thing is worth more than a
  realistic one.

* Assert on abstentions, not just successes. A suite that only checks correct
  answers will pass a change that starts guessing.

# Performance posture

* Respect the latency budgets, and **abstain rather than block** when a deadline
  is at risk. Blowing the budget must cost coverage, never correctness.

* Avoid premature optimization — implement the slow simple version first to
  check the idea works.

* Once validated, optimize hard. But no SIMD, no `unsafe`, and no new
  caching/indexing until the corpus harness shows the change is worth it *and*
  there is a benchmark. Ask before adding caching or indexing.

* Push allocations to the call site; avoid intermediate collections
  (accumulator-first recursion).

* Prefer `rustc-hash`'s `FxHashMap`/`FxHashSet` over the std defaults. Nothing
  here is keyed by untrusted input, so SipHash is pure overhead.

* Avoid monomorphization across crate boundaries — prefer `&dyn` and `&Path`
  over `impl AsRef<Path>` — to keep compile times down.

# Rust style

* Prioritize correctness and clarity. Speed is secondary unless specified.

* Use the newtype pattern frequently. Primitive fields should usually be
  newtypes.

* Avoid `bool` and bare `Option` parameters that make call sites read
  `foo(false)` or `bar(None)`. Prefer an enum, a named method, or a newtype.

* Express preconditions in types; push control flow to the caller ("push ifs up
  and fors down"). Prefer enums that enforce an invariant over a comment
  describing it.

* Make `match` exhaustive. Avoid wildcard arms generally, not only on error
  enums.

* Avoid `unwrap()` and panicking indexing; propagate with `?`.

* Never silently discard errors with `let _ =` on a fallible operation:
  - propagate with `?` when the caller should handle it,
  - log visibly when deliberately ignoring it,
  - use an explicit `match` / `if let Err(...)` when the logic is custom.

* Use `#[expect(...)]` rather than `#[allow(...)]`, so a suppression that stops
  being needed surfaces as a warning. Never ignore deprecation warnings.

* Inline `format!` arguments: `format!("{x}")`, not `format!("{}", x)`.

* No wildcard imports.

* Full words for variable names — no `q` for `queue`.

* Do not write organizational comments or comments that summarize the code.
  Comment only to explain *why*, where the reason is non-obvious.

* Do not add doc comments where purpose and behavior are obvious. Prefer the
  external `.md` files (and reference them), plus a few central spots with
  longer prose.

* Doc comments are full sentences: capitalized, ending in a period. Ordinary
  comments may be fragments.

* Prefer implementing in existing files unless it is a genuinely new component.
  Avoid creating many small files.

* Never create `mod.rs` — use `src/some_module.rs`.

* When creating a crate, specify the library root in `Cargo.toml` with
  `[lib] path = "...rs"` rather than defaulting to `lib.rs`, for a descriptive
  name.

* Avoid creative additions unless asked.

# Rules Hygiene

This file is read by every agent session. Keep it high-signal.

## After any agentic session
If you discover a non-obvious pattern that would help future sessions, file it as a decision (`state/decisions/`) with the proposed text. Do **not** edit this file inline during normal work.

Changing this file is a bigger deal than it looks: Claude Code loads it into every session, so an edit changes the behaviour of every future campaign, and metrics either side of it are not strictly comparable. It is therefore a human change, logged as an intervention like a prompt revision — `design/loops.md` §16.

## High bar for new rules
Editing or clarifying existing rules is always welcome. New rules must meet **all three** criteria:
1. **Non-obvious** — someone familiar with the codebase would still get it wrong without the rule.
2. **Repeatedly encountered** — it came up more than once (multiple hits in one session counts).
3. **Specific enough to act on** — a concrete instruction, not a vague principle.

Rules that apply to a single crate belong in that crate's own rules file, not the repo root. `vendor/` and the tree-sitter-facing crates are the likely candidates.

## What NOT to put here
Avoid architectural descriptions of a crate (module layout, data flow, key types). These go stale fast and the agent can gather them by reading the code. Rules should be **traps to avoid**, not **maps to follow**.

## No drive-by additions
Rules emerge from validated patterns, not one-off observations. The workflow is:
1. A campaign notes a pattern and files it as a decision.
2. It is validated — by recurrence across campaigns, or by a human at a phase gate.
3. A dedicated commit adds the rule with context on *why* it exists.

Recurrence is the useful signal: the same suggestion arriving from three campaigns is evidence, where one campaign's observation is a hypothesis.
