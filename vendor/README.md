# Vendored crates

Copies of Zed crates, kept here rather than in `crates/` so provenance and
licensing stay obvious (`design/core.md` §9). Editing them is permitted
(`CLAUDE.md`); what is required is that every edit be recorded below, so a
future re-sync can tell at a glance what changed and why.

Read `design/rope-modifications.md` before touching `rope`.

| Crate | Upstream | Revision | License |
|---|---|---|---|
| `rope` | `zed/crates/rope` | `90d024b88abc91264d9a0ad260eb4f365fa695c3` | GPL-3.0-or-later |
| `sum_tree` | `zed/crates/sum_tree` | `90d024b88abc91264d9a0ad260eb4f365fa695c3` | Apache-2.0 |

The copy was made with `cp -a`, which preserves `LICENSE-GPL ->
../../LICENSE-GPL` and `LICENSE-APACHE -> ../../LICENSE-APACHE` as symlinks.
`vendor/<crate>/` sits at the same depth `crates/<crate>/` did upstream, so
they resolve against this workspace's root licence texts with no fixing up.
Plain `cp -r` would dereference them into 34 KB duplicates and lose the
property (`deps.md` §14).

## Patches to `rope`

1. **`util` folded in.** There is no `vendor/util`: vendoring it whole would
   drag `async_zip`, `rust-embed`, `schemars` and `regex` in to support a text
   data structure (`core.md` §9). rope used exactly three items, each of which
   carries an attribution comment at its new site:

   | Item | Upstream path | License | Now at |
   |---|---|---|---|
   | `is_utf8_char_boundary` | `crates/util/src/util.rs` | Apache-2.0 | private `const fn` in `src/chunk.rs`, its only caller |
   | `debug_panic!` | `crates/gpui_util/src/lib.rs` | Apache-2.0 | `macro_rules!` at the top of `src/rope.rs`, not `#[macro_export]`ed |
   | `RandomCharIter` | `crates/util/src/util.rs` | Apache-2.0 | `src/test_support.rs` |

   `debug_panic!` is not in `util` upstream — `util` re-exports it from
   `gpui_util` via `pub use gpui_util::*`, so a grep in `util` finds nothing.
   It is defined *before* `mod chunk;` because `macro_rules!` scoping is
   textual and `chunk.rs` is the only caller.

   Apache-2.0 is one-way compatible into GPL-3.0, so the move is fine legally
   (`deps.md` §5); the attributions are here and in the source because the
   items being trivial is not a reason to drop them.

2. **`ztracing::instrument` → `tracing::instrument`**, one line in
   `src/rope.rs`. `ztracing`'s `instrument` is either `tracing`'s or a no-op
   passthrough depending on a cfg, and rope already depended on `tracing`.

3. **Upstream's `#[cfg(test)]` modules are kept**, which is the point: they are
   the only independent check that the newtype sweep in
   `rope-modifications.md` changed nothing. Three substitutions made them run
   under a plain `cargo test`:

   * `#[gpui::test(iterations = N)]` on nine functions → `#[test]` calling
     `seeded(N, <name>_inner)`, bodies verbatim. `seeded` is in
     `src/test_support.rs` and honours `SEED`/`ITERATIONS` the way gpui's
     `calculate_seeds` does. The `gpui` dev-dependency is gone.
   * `#[ctor::ctor] fn init_logger` + `zlog::init_test()` deleted — they only
     initialised logging and nothing asserts on it. `ctor` and `zlog` go with
     them.
   * `util::RandomCharIter` → `crate::test_support::RandomCharIter`.

4. **`benches/rope_benchmark.rs` is kept**, and is the one thing
   `rope-modifications.md` §4 got wrong: it is a *sixth* `util` import site,
   and being its own crate it can see neither `util` nor a `#[cfg(test)]`
   module. It `#[path]`-includes `src/test_support.rs` instead, so there is
   still one copy. See `state/spec-changelog.md`, CHANGE-conformance-002.

5. **The vocabulary newtypes are added** — `src/byte_offset.rs`
   (`ByteOffset`, `ByteLen`, `ByteRange`), `LineIndex` / `ByteColumn` /
   `CharCount` at the end of `src/point.rs`, `Utf16Column` at the end of
   `src/point_utf16.rs`, and `ByteOffset`'s two seek-dimension impls beside
   `OffsetUtf16`'s in `src/rope.rs`. They live here rather than in `shared`
   because `shared` depends on `rope` and the dependency cannot run the other
   way (`rope-modifications.md` §2).

   **The substantive half of that document is still to do.** These types are
   inert: not one of rope's ~51 public signatures has been converted, so
   `Rope::len()` still returns `usize` and `Point.row` is still a bare `u32`.
   The sweep is `rope-modifications.md` §4 and is a campaign of its own; the
   29 kept tests are what will check it.

6. **`pub use sum_tree::Bias;` added** to `src/rope.rs`, beside the other
   re-exports. `clip_offset`, `clip_point` and `clip_point_utf16` are public
   and take a `Bias`, so without it a caller that does not itself depend on
   `sum_tree` cannot name an argument to a function it can call. Upstream has
   no reason to notice: inside Zed every caller of `rope` depends on
   `sum_tree` anyway. `core.md` §9's authoritative dependency list for
   `shared` does not include `sum_tree`, so the re-export is the alternative
   to widening that list.

   This matters more than a re-export usually would, because the clip
   functions are not an optimisation for `shared::proto`, they are the only
   safe entry: `point_to_offset` and `point_utf16_to_offset` reach
   `debug_panic!` on an out-of-range or mid-scalar position, which **panics in
   debug and silently clips in release**. A caller that must not do either has
   to clip first and compare, which is what `core.md` §3's conversion does.

## Patches to `sum_tree`

1. **`src/tree_map.rs` deleted** (`TreeMap`, `TreeSet`, `MapSeekTarget`),
   with its `mod` and `pub use` lines. Unused here; a whole-file deletion
   still leaves a clean diff.
2. **`ztracing::instrument` → `tracing::instrument`**, one line each in
   `src/sum_tree.rs` and `src/cursor.rs`.
3. **`#[ctor::ctor] fn init_logger` + `zlog::init_test()` deleted**, as in
   rope.

`sum_tree` is otherwise unpatched, and the newtype work does not change that:
`sum_tree::Dimension` is generic over the summary type, so `ByteOffset`'s
impls live in `rope`.

## Recorded differences that are not patches to the source

* **Neither crate takes `[lints] workspace = true`.** `deps.md` §14 keeps the
  workspace rules on `crates/*`: bending 7,400 lines of someone else's
  text-datastructure code to `unwrap_used`, `panic` and the `cast_*` family
  would buy no correctness and would widen every re-sync diff. A practical
  consequence, so that nobody rediscovers it as a bug:
  **`cargo clippy -p rope -p sum_tree --all-targets -- -D warnings` is not
  clean and is not meant to be.** The root `clippy.toml` still applies (its
  own header says so), so the failures are mostly default-level lints promoted
  by `-D warnings`.

  `rope` carries one entry, `unexpected_cfgs = { level = "allow" }`, which is
  not a rule but a false positive: `rust_analyzer` is a cfg Zed sets from
  outside cargo and allows workspace-wide.

  It now carries a `[lints.clippy]` block for a different reason, and the
  reason is worth knowing before someone deletes it: **lints leak along the
  dependency edge.** `-D warnings` is a command-line flag, so
  `cargo clippy -p shared --all-targets -- -D warnings` — gate step 2, on a
  crate the gate *does* lint — applies it to every crate it compiles from
  source, and `shared` depends on `rope`. The two entries are the two
  default-level clippy lints upstream's `rope.rs` trips
  (`should_implement_trait` on the lending `Lines::next`, which cannot
  implement `Iterator`; `from_over_into` on `impl Into<Chunk> for
  ChunkSlice`). Allowing them in the vendored manifest keeps the edit out of
  upstream's source. Anything the *tests* trip is still not covered — that is
  `conformance-003` and this does not answer it.

* **Dependency versions** are Zed's at `90d024b8`, in this workspace's
  `[workspace.dependencies]`, with two deliberate differences recorded there:
  `log` without Zed's `kv_unstable_serde`/`serde` features, and crates.io
  `proptest` rather than Zed's git rev.

* **Neither crate is in the conformance loop's gate crate list**, so
  `harness/gate` does not build, lint or run them — decision
  `conformance-003`. Until that is answered, a campaign that touches `vendor/`
  runs `cargo nextest run -p rope -p sum_tree` itself and says so.
