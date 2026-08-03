---
id: conformance-015
status: open
opened: 2026-08-03T22:46:00+00:00
campaign: 51628b98-b5ea-48b1-bb77-696ecc51face
kind: class-b
---

# Does `tempfile` still arrive, now that its first user chose `CARGO_TARGET_TMPDIR` instead?

## Context

`deps.md` §12's table places `tempfile` precisely:

> | `tempfile` | Fixture repositories for `ProjectView` scope tests |

and §0's summary table marks it **chosen**. Under §14's rule that "each
arrives with its first user", a crate that is chosen but not yet declared is
the intended state — which is how five of §0's rows read today.

`tempfile` is not in that state. Its named user exists.
`crates/shared/tests/project.rs` is the `ProjectView` scope suite, it builds
fixture repositories, and it does so without the dependency:

```rust
fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    …
    fs::create_dir_all(root.join(".git")).expect("the fixture repository marker");
    fs::write(root.join(".gitignore"), "vendored/\n").expect("the fixture gitignore");
```

So the choice §12 recorded has been made differently by the code, and nothing
says which one is now the plan. That is the whole content of the question: not
that either mechanism is wrong, but that a dependency marked **chosen** with a
named user, whose named user does not use it, is indistinguishable from one
that is merely early — and the next campaign to read §0's table cannot tell
those apart.

## Options

**A — keep `CARGO_TARGET_TMPDIR`; amend §12.** No dependency, which is the
direction `CLAUDE.md` asks for ("prefer a shared in-repo utility over a new
dependency"). Cargo supplies the directory per test target and it is
inspectable after a failure, which a `TempDir` that dropped is not — for a
suite whose fixtures encode `.gitignore` semantics, being able to look at what
was actually on disk is worth something. Costs cleanup: the directory persists
between runs, so a fixture left over from an earlier version of a test can
mask its failure, and `fixture()` has to be careful to build from scratch
rather than trust what is there.

**B — declare `tempfile` and convert the suite.** What §12 says. Deletion on
drop makes stale-fixture masking impossible by construction. Costs a
dependency, and costs the post-mortem: a failing test's fixture is gone by the
time anyone looks.

The trade is stale state against inspectability, and it is not obviously one
way — which is why this is a record rather than a fix.

## Decision

Undecided — waiting on a human.

## Provisional choice in force

**Option A**, by doing nothing: the code already works this way and this
campaign does not change it.

It is the more reversible of the two because the reversal is mechanical and
local. Going A→B is an edit to one helper function, `fixture()` in
`crates/shared/tests/project.rs`, plus a dev-dependency line — the call sites
do not change, since they take the `PathBuf` the helper returns. Going B→A
would additionally mean re-deciding cleanup at every site that held a
`TempDir` alive for a scope. Doing nothing also declines to add a dependency,
which is the direction that is cheap to undo.

No source is tagged, and that is deliberate rather than an omission: there is
nothing provisional *in* the code — it is `deps.md` §12's table that is
ahead of it, and a `// DECISION-` comment on a working `fixture()` would read
as a defect in the function.

## Consequences

If the answer is B: add `tempfile` to `[workspace.dependencies]` and to
`crates/shared`'s `[dev-dependencies]`, and rewrite `fixture()` to return a
`TempDir` the caller holds. One helper, one manifest line, no call-site
changes. The scan added alongside this record
(`the_testing_crates_are_placed_where_section_12_puts_them`) permits
`tempfile` in a dev table already, so it needs no edit either way.

If the answer is A, §12's table row and §0's `tempfile` verdict need amending
to name `CARGO_TARGET_TMPDIR` and say why — otherwise the next campaign
rediscovers exactly this, which is the cost the record exists to avoid.
