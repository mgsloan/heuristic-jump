# Changes to the vendored rope

`vendor/rope` is a copy of Zed's `crates/rope`. `core.md`
[section 9](core.md#vendoring-the-zed-crates) covers why
it is vendored and what patches the copy needs to compile. This document covers
one further change, which is larger than those and is a design decision rather
than a mechanical fix-up:

> **The rope's public API speaks in newtypes: `ByteOffset`, `ByteLen`, and
> `ByteRange` instead of bare `usize` and `Range<usize>`, and `LineIndex`,
> `ByteColumn`, `Utf16Column`, and `CharCount` instead of the bare `u32`s in
> `Point`, `PointUtf16`, and `TextSummary`. The newtypes are opaque — no
> operators against bare integers, and positions and lengths are distinct.**

`CLAUDE.md` asks for newtypes on primitive fields, and the driver's correctness
rests on byte offsets more than on anything else — `core.md`
[section 3](core.md#3-position-encoding) calls position
handling the highest-risk detail in the whole driver, and
[section 8](core.md#8-protocol-types) drops `lsp-types`
specifically so that `ByteOffset` is what deserialization *produces* rather than
what a conversion layer produces afterwards.

That argument does not stop at the LSP boundary. If `ByteOffset` is unwrapped to
a `usize` the moment it touches the text, the type has been enforcing the
boundary and nothing else — and the text is where offsets are actually used.

## 1. rope already works this way; byte offsets are the omission

This is not an alien discipline being imposed on the crate. rope's dimension
types are already newtypes:

```rust
pub struct OffsetUtf16(pub usize);                // offset_utf16.rs
pub struct Point { row: u32, column: u32 }        // point.rs
pub struct PointUtf16 { row: u32, column: u32 }   // point_utf16.rs
pub struct Unclipped<T>(pub T);                   // unclipped.rs
```

The *dimensions* are newtyped. What is not newtyped is one level down: the
plain byte offset is a bare `usize` — including as a `sum_tree::Dimension` and
a `TextDimension` (`rope.rs:1492`, `rope.rs:1502`) — and the row and column
inside `Point` and `PointUtf16` are bare `u32`.

Both are reasonable upstream. The byte offset is the default dimension, the one
you reach for without thinking; and a row is a row. They are the wrong defaults
here for exactly that reason: `PointUtf16.column` and `Point.column` are both
`u32` and mean different things, and mixing them is the failure
[section 3](core.md#3-position-encoding) of the core
design calls the highest-risk in the driver — invisible on ASCII, wrong by a
few columns on any line that is not.

So the change is better understood as **completing rope's existing newtype
family** than as patching it.

## 2. Where the types live is forced

`shared` depends on `rope`
([section 9](core.md#the-dependency-graph)), so rope
cannot depend on `shared`. `ByteOffset` therefore **lives in `rope`**, and
`shared` re-exports it:

```rust
// vendor/rope/src/byte_offset.rs
pub struct ByteOffset(pub usize);   // a position in a document
pub struct ByteLen(pub usize);      // a quantity of bytes
pub struct ByteRange { pub start: ByteOffset, pub end: ByteOffset }

// vendor/rope/src/point.rs, point_utf16.rs
pub struct LineIndex(pub u32);
pub struct ByteColumn(pub u32);     // Point.column: bytes into the line
pub struct Utf16Column(pub u32);    // PointUtf16.column: UTF-16 code units
pub struct CharCount(pub u32);      // Unicode scalar values -- the fourth unit

pub struct Point       { pub row: LineIndex, pub column: ByteColumn }
pub struct PointUtf16  { pub row: LineIndex, pub column: Utf16Column }
```

`ByteColumn` and `Utf16Column` being distinct types is most of the value here.
They are the pair that is currently interchangeable and must not be —
and `CharCount` is a third way of measuring the same span
([section 4](#the-signatures)), which the crate currently spells `u32` as
well.

`shared` re-exports both, so every other crate says `shared::ByteOffset` and
never knows or cares that the definition sits in the vendored crate. That
keeps [section 1](core.md#vocabulary-types)'s claim —
that these are the shared vocabulary — true from the outside.

`ByteRange` carries only text-shaped operations — `contains`, `overlaps`,
`len`, `is_empty`, `intersect` — all of which rope can define without
depending on anything of ours.

An earlier revision needed one more, `shifted_by(&InputEdit)`, to re-anchor a
pending query's position through an edit, and `InputEdit` is tree-sitter's —
so it could not live in rope, and `shared` supplied it through an extension
trait. That awkwardness is gone with its caller: `shim.md` §7 no longer tracks
positions across edits, so nothing needs the method and `shared` needs no
extension trait over rope's types at all.

## 3. What keeps this safe

Body edits are allowed. Newtyping `Point`'s fields strictly — which
[section 4](#4-what-changes) does — makes them unavoidable: about nine `as`
casts (`self.row as usize`, `point.column as usize`), every `Point::new` call
site, every `== 0` comparison, and the arithmetic inside `Point`'s own `Add`
and `Sub` impls. An earlier sketch of this document tried to avoid all of that
with a six-line whitelist and lenient operators, and both halves of that were
wrong: the count was off by several times over, and the leniency gave back the
type safety the change exists to buy.

So the safety argument is not "nothing changed." It is:

> **Every edit changes representation, never arithmetic.** Each one is the
> compiler rejecting a type mismatch and being answered with a wrapper, an
> unwrap, or a named constant — never with different control flow, different
> operands, or a different operation.

That is reviewable as a class rather than line by line, because the edits come
in exactly five shapes:

| Shape | Example |
|---|---|
| Unwrap for a foreign operation | `self.row as usize` → `self.row.0 as usize` |
| Wrap a computed primitive | `point.row = row as u32` → `LineIndex(row as u32)` |
| Named constant for a literal | `point.column = 0` → `ByteColumn::ZERO` |
| Wrap at a constructor call | `Point::new(0, 0)` → `Point::new(LineIndex::ZERO, ByteColumn::ZERO)` |
| Unwrap-operate-rewrap | `self.row + other.row` → `LineIndex(self.row.0 + other.row.0)` |

A hunk that is not one of these five is a bug in the patch. Review checks the
shape; **the tests check the behaviour**, and that is the real reason
[section 7](#7-testing) keeps every one of them. A mechanical diff check can no
longer prove this change correct, so the tests are not a nicety here — they are
the verification.

Signature changes still keep bodies verbatim where they can, by the same two
mechanisms as before:

* **Parameters only.** Change the signature and shadow the parameter on the
  first line:

  ```rust
  pub fn slice(&self, range: ByteRange) -> Rope {
      let range = range.start.0..range.end.0;   // added
      /* upstream body, untouched */
  }
  ```

* **Return values too.** Rename the upstream function, body untouched, and add
  a wrapper:

  ```rust
  fn point_to_offset_raw(&self, point: Point) -> usize {
      /* upstream body, untouched */
  }
  pub fn point_to_offset(&self, point: Point) -> ByteOffset {
      ByteOffset(self.point_to_offset_raw(point))
  }
  ```

  The rename is preferred over wrapping at each `return` site, because a
  function with several returns would otherwise need several edits inside the
  body.

## 4. What changes

### The newtypes are opaque

The tempting shortcut is to give each newtype `Add<u32>`, `AddAssign<u32>`,
`PartialEq<u32>`, and `PartialOrd<u32>`, so that `p.row += 1` and
`p.column == 0` keep compiling and no body needs editing.

**They do not get those impls.** A type that compares and adds against bare
integers is a type that bare integers flow into, which is the situation this
change exists to end. It would also be subtly broken: cross-type `PartialOrd`
does not participate in generic `Ord` code, so the ordering would be
asymmetric depending on how it was reached. Buying back a few dozen body edits
by making the type half-transparent is not a trade worth making.

What each newtype gets:

* `#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]`
* `ZERO` and `MAX` associated consts, so a literal never has to appear
* A **hand-written `Display`** — `Display` cannot be derived, and it must print
  the bare number so that `Point`'s own
  `write!(f, "Point({}:{})", self.row, self.column)` (`point.rs:16`) keeps its
  output.

And no arithmetic operators at all, except for the byte pair, which gets
exactly the operations that make sense between a position and a quantity:

| Expression | Result | Meaning |
|---|---|---|
| `ByteOffset + ByteLen` | `ByteOffset` | advance a position |
| `ByteOffset - ByteLen` | `ByteOffset` | retreat a position |
| `ByteOffset - ByteOffset` | `ByteLen` | distance between positions |
| `ByteLen ± ByteLen` | `ByteLen` | accumulate |
| `ByteOffset + ByteOffset` | — | **not implemented**; meaningless |

Plus the matching `AddAssign`/`SubAssign`, and `ByteRange::len() -> ByteLen`.
This is the ordinary point-and-vector split, and the row above that has no
result type is the one that catches real mistakes.

**`LineIndex`, `ByteColumn`, `Utf16Column`, and `CharCount` get none.** Adding
two line numbers is meaningless; there is no length interpretation to rescue
it. Where rope needs the arithmetic it unwraps explicitly, which is the
unwrap-operate-rewrap shape from [section 3](#3-what-keeps-this-safe) and is
visible at the point it happens.

`Point` and `PointUtf16` keep their existing `Add`, `Sub`, and `AddAssign`
impls, with the bodies edited to unwrap. This is a compromise and it is flagged
as such: `Point + Point` treats one operand as absolute and the other as
relative, which is the same conflation being rejected for `LineIndex` one
paragraph up. It is kept because rope's internals rely on it throughout: the fix is a
distinct `PointDelta` type, which is a much larger change than this document
describes, and it is not being taken. Recorded here rather than left as an
open question, because leaving it open implies someone will come back to it.

### The constructors take newtypes

```rust
impl Point {
    pub fn new(row: LineIndex, column: ByteColumn) -> Self { … }
}
```

All 54 `Point::new` / `PointUtf16::new` call sites inside rope are edited. This
is the largest single block of body edits and it is mechanical: literals become
`LineIndex::ZERO` and friends, and the 19 sites that already pass a typed field
(`Point::new(self.row, self.column + other.column)`) need no wrapping at all.

An `impl Into<LineIndex>` constructor would have avoided the literal edits, and
is rejected for the same reason as the lenient operators: it is a `u32`-shaped
hole in the constructor.

### The signatures

**The conversion list is hand-audited, not grepped.** A grep for `pub fn`
signatures mentioning `usize` or `u32` overcounts, because this crate has
**four** units, not three — bytes, UTF-16 code units, lines, and *chars*
(Unicode scalar values) — and the char dimension is also spelled `usize` and
`u32`.

Converted:

| Area | Examples |
|---|---|
| Slicing and ranges | `slice`, `replace`, `chunks_in_range`, `bytes_in_range`, and the `reversed_*` variants |
| Conversions | `offset_to_point`, `offset_to_point_utf16`, `offset_to_offset_utf16`, `point_to_offset`, `point_utf16_to_offset`, `unclipped_point_utf16_to_offset`, `offset_utf16_to_offset` |
| Clipping and boundaries | `clip_offset`, `is_char_boundary`, `assert_char_boundary`, `floor_char_boundary`, `ceil_char_boundary` |
| Length | `Rope::len`, `ChunkSlice::len` — both return `ByteLen` |
| Cursors and iterators | `Cursor::{new, seek_forward, slice, summary, offset}`, `Chunks::{new, seek, set_range, offset}`, `Bytes::new`, `Lines::{seek, offset}`, `chars_at`, `reversed_chars_at` |
| Rows | `slice_rows(Range<LineIndex>)`, `line_len(row: LineIndex) -> ByteColumn` |
| `ChunkSlice` | All 17 of its public functions. It is low-level and handlers have little reason to touch it, but a bare-`usize` island inside an otherwise converted crate is exactly the escape hatch the change is meant to close |

Not converted, because these are not byte offsets:

| Function | Unit |
|---|---|
| `Chunk::first_line_chars() -> u32` | `CharCount` |
| `Chunk::last_line_chars() -> u32` | `CharCount` |
| `Chunk::longest_row(&mut total_chars) -> (u32, u32)` | `(LineIndex, CharCount)`; `total_chars` is a `CharCount` |
| `Chunk::last_line_len_utf16() -> u32` | `Utf16Column` |

These get the *correct* newtype rather than being left bare — which is the
point of auditing rather than grepping. `CharCount` is the fourth member of the
family, and it exists precisely because `first_line_chars` and `Point.column`
are both "how far into a line" in different units, which is the confusion class
this whole document is about.

### `TextSummary` is converted too

An earlier draft excluded it on the grounds that its fields are accumulated
throughout the crate and changing them means editing bodies. Body edits are now
allowed, so the exclusion has no argument behind it — and leaving the crate's
central summary type as bare integers while everything around it is typed would
be the worst of both.

```rust
pub struct TextSummary {
    pub len: ByteLen,
    pub chars: CharCount,
    pub len_utf16: OffsetUtf16,
    pub lines: Point,                    // typed already, via Point
    pub first_line_chars: CharCount,
    pub last_line_chars: CharCount,
    pub longest_row: LineIndex,
    pub longest_row_chars: CharCount,
    pub last_line_len_utf16: Utf16Column,
}
```

This is the largest scope increase that follows from allowing body edits, and
it is called out rather than absorbed.

### The dimension impls

`ByteOffset` gains the two impls that make it usable as a seek dimension,
mirroring what `OffsetUtf16` already has (`rope.rs:1516`, `rope.rs:1526`):

```rust
impl<'a> sum_tree::Dimension<'a, ChunkSummary> for ByteOffset { … }
impl TextDimension for ByteOffset { … }
```

**`sum_tree` needs no changes at all.** `Dimension` is generic over the
summary type, so the impls live in rope. That matters:
`sum_tree` stays a pristine copy, and
[section 9](core.md#vendoring-the-zed-crates)'s claim
that it needs no patching survives.

### Offsets and lengths are separate types, and `ByteLen` is shared

`Rope::len()` and `ChunkSlice::len()` return `ByteLen`, not `ByteOffset`.

An earlier draft merged them, on the grounds that in this crate a length *is*
an offset — `TextSummary.len` is fed straight to cursor seeks, and summaries
add lengths to produce positions. That is true, and it is exactly the reason to
keep them apart: those are two different operations that a single type spells
identically. With the operator table above, "advance a position by a length"
and "how far apart are these positions" have different signatures, and
"add two positions" does not typecheck at all.

The practical objection to splitting — that it forces conversions inside
function bodies — no longer applies, since bodies are editable
([section 3](#3-what-keeps-this-safe)). The conversions it forces are the point:
each one is a place where the direction could have been wrong and now has to be
written down.

There is deliberately **no `From<ByteLen> for ByteOffset`**. Turning a length
into a position means measuring from somewhere, so it is spelled
`ByteOffset::ZERO + len`, which names the origin.

**`ByteLen` is one type, shared with resolution code.** `resolution.md` needs a byte
quantity for `bytes_scanned` — the running total across the files a query
read — and that is this type, not a parallel one. The two uses look different
(a document's length; a running total across files) but they are the same
quantity with the same arithmetic, and the one place they meet is adding a
file's length to the total. A separate `ScannedBytes` would put a conversion
at exactly that point, which is the one place a unit error would be invisible
anyway. So `shared` re-exports `ByteLen` and handlers use it directly.

Note the total is a *counter*, not a budget: nothing compares it against a
limit, since a search reads every candidate file (`resolution.md` §1.3). An
earlier revision justified this type by the budget arithmetic, which no longer
exists; the sharing argument survives the change because it never depended on
the comparison, only on the addition.

### Folding `vendor/util` in

The plan was a third vendored crate holding the handful of items rope uses,
keeping the crate name `util` so that rope's `use util::…` lines stayed
untouched. That rested on one sentence: *rope needs no patching at all for
this, which keeps re-syncing a clean diff rather than a merge.*

**That justification is gone.** This document rewrites rope's public API, and
[section 6](#6-consequences-for-re-syncing) already concedes the clean-diff
property. Once the crate is being substantially patched anyway, preserving
five import lines buys nothing and costs a whole vendored crate — a
Cargo.toml, a license file, a provenance entry, and a workspace member called
`util`, which is the most accretion-prone name available in any codebase.

So the items move into rope and `vendor/util` does not exist.

**Everything `util` supplies is rope's alone.** Verified rather than assumed:
`sum_tree` does not depend on `util` at all, and its randomised tests use a
plain `#[test]` with a hand-rolled seed loop rather than `#[gpui::test]`, so it
needs no helper either. The "and/or `sum_tree`" question resolves to rope.

| Item | Size | Where it lands in rope |
|---|---|---|
| `is_utf8_char_boundary` | 4-line `const fn` | private in `chunk.rs`, its only caller |
| `debug_panic!` | ~10-line macro | `macro_rules!` at the crate root, **not** `#[macro_export]`ed |
| `RandomCharIter` | ~40 lines, tests only | `src/test_support.rs`, a `#[cfg(test)] mod` at the crate root |
| `seeded` | ~20 lines, ours not upstream's | the same module ([section 7](#7-testing)) |

`test_support` is a file rather than an inline module because the benchmark
needs it too, and for that it has to be `#[path]`-includable — see the import
sites below.

Details that matter:

* **`debug_panic!` is not where the design said it was.** Upstream defines it
  in `gpui_util`; `util` re-exports it via `pub use gpui_util::*`, which is why
  a grep for it in `util` finds nothing. The vendored copy inlines the
  definition. It is deliberately not `#[macro_export]`ed — `rope::debug_panic!`
  has no business being in our public API.
* **No new dependencies.** `log` is already a rope dependency, which is what
  `debug_panic!` needs; `rand` is already a dev-dependency, which is what
  `RandomCharIter` needs. rope's `Cargo.toml` loses both `util` lines and gains
  nothing.
* **Six import sites change**, across three files: `chunk.rs:6`, `:76`,
  `:192`, `:825`, `rope.rs:1733`, and `benches/rope_benchmark.rs:10`.

  The sixth is the one that constrains the table above. A bench target is
  compiled as its own crate, so it can reach neither `util` (not vendored) nor
  rope's `#[cfg(test)]` module, and upstream only gets away with
  `use util::RandomCharIter;` there because rope dev-depends on
  `util = { features = ["test-support"] }`. The bench therefore
  `#[path = "../src/test_support.rs"] mod test_support;`, which keeps one copy
  of the source without putting it in rope's public API or pulling `rand` out
  of dev-dependencies. `state/spec-changelog.md`, CHANGE-conformance-002.

**Attribution is not optional.** `util` is Apache-2.0 and rope is
GPL-3.0-or-later. Apache-2.0 is one-way compatible into GPL-3.0, so the move
is fine legally, but each relocated item carries a comment naming its upstream
path and original license, and `vendor/README.md` records the move. The items
are trivial enough that whether they are copyrightable at all is arguable;
attributing anyway costs a comment and settles the question.

One simplification falls out: `vendor/` drops from three crates to two, and
`sum_tree` becomes the only Apache-2.0 input. `deps.md` §5's table shrinks to
match.

**Re-sync cost.** Upstream edits touching those five lines will now conflict
where previously they would not. Weighed against a patch that rewrites the
entire public API, that is noise.

**This is contingent, and worth knowing before revisiting either decision.**
If the newtype work in this document were ever dropped, the original argument
returns intact and a separate cut-down `util` becomes right again. The two
were independent; they are not any more.

## 5. What deliberately does not change

* **`OffsetUtf16` and `Unclipped`.** Already newtypes, already right.
* **The `usize` dimension impls** (`rope.rs:1492`, `:1502`). rope uses `usize`
  as a dimension internally in about seven places — `find::<usize, _>`,
  `Dimensions<usize, Point>` — and removing the impls would mean editing those
  bodies. They stay.

  The residual: `TextDimension for usize` remains public, so a handler holding
  a `Rope` could in principle do `cursor.summary::<usize>()` and get a bare
  offset. That is a much narrower hole than an API that took `usize`
  everywhere, and it is not one anybody falls into by accident — you have to
  name the type. Recorded rather than fixed.
* **`sum_tree`.** Untouched by this. It needs nothing from `util` and
  nothing from the newtypes — `sum_tree::Dimension` is generic over the
  summary type, so `ByteOffset`'s impls live in rope.

### `LineIndex` is rope's, and `shared` re-exports it

`Location` carries a `LineIndex`
([section 8.4](core.md#84-location-is-byte-based-and-this-fixes-a-real-inconsistency)).
Since `Point.row` is now a `LineIndex`, the type has to live in rope for the
same dependency reason as `ByteOffset`, and `shared` re-exports it. The
row-taking APIs —`slice_rows(Range<LineIndex>)`, `line_len(row: LineIndex)`
—take it too, so a row obtained from a `Point` can be handed straight back to
the rope.

## 6. Consequences for re-syncing

[Section 9](core.md#vendoring-the-zed-crates) argues
that the patches to `rope` should stay small enough for a re-sync to be a clean
diff rather than a merge. **This change makes that less true, and the claim
should be read as weakened rather than intact.**

Precisely how much:

* An upstream change to a function *body* usually applies cleanly. Most bodies
  are untouched; the ones we edited are concentrated in `point.rs`,
  `point_utf16.rs`, and the `Point::new` call sites, so a conflict is likely
  only where upstream also touched those.
* An upstream change to a *signature* conflicts. Signatures change rarely, and
  the conflict is a one-line manual resolution that is obvious on sight.
* An upstream *new* public function arrives with `usize` or `u32` in its
  signature. Nothing about the diff flags this, since an unconverted function
  is not an unexpected hunk — it is a hunk that looks entirely normal.

That last case has a direct fix, and it is the enforcement this change relies
on:

> **CI asserts that no `pub fn` signature in `vendor/rope` mentions bare
> `usize` or `u32`**, outside an explicit allowlist.

The allowlist is `vendor/rope/allowed-primitives.txt` and is short: the
`total_chars: &mut usize` parameter of `Chunk::longest_row` and anything else
[section 4](#the-signatures) records as a genuine primitive. Every entry needs
a comment saying which unit it is, since "it is a `usize`" is the problem
rather than the explanation.

The check is cheap, it catches the one failure mode the diff cannot, and it
turns "someone notices" into a build failure.

`vendor/README.md` records the conversion as a patch class rather than a patch
list, with these checks as the enforcement.

The honest summary is that this trades some re-sync convenience for type safety
in the place the design says is riskiest. It is worth it, but it is a real
cost and it is the reason this is a separate document rather than a bullet in
`core.md` section 9.

## 7. Testing

### All of upstream's tests are kept

rope's `#[cfg(test)]` modules reach for `gpui`, `zlog`, and `ctor`, which looks
like a reason to delete them. **Every test is preserved instead**, and
`core.md` [section 9](core.md#vendoring-the-zed-crates) states the same
conclusion. Once we are editing the crate, deleting its tests is exactly
backwards — they are the only independent check that a 51-function signature
sweep and the body edits that follow from it did not change behaviour, and
several are randomised differential tests against a `String` oracle, which is
precisely the kind of test nobody would write from scratch.

What they need turns out to be small. Only three things stand between the test
modules and a plain `cargo test`:

** `#[gpui::test(iterations = N)]` on nine functions.** Despite the name,
nothing about gpui is involved for these: rope's randomised tests take
`mut rng: StdRng` and nothing else —no `TestAppContext`, no async. The macro
is doing one job, which is to run the body N times with deterministic seeds
and print the seed on failure. That is replaced by a helper in rope's own test
module ([above](#folding-vendorutil-in)):

```rust
// upstream
#[gpui::test(iterations = 100)]
fn test_random_rope(mut rng: StdRng) { /* body */ }

// vendored
#[test]
fn test_random_rope() { seeded(100, test_random_rope_inner) }
fn test_random_rope_inner(mut rng: StdRng) { /* body, untouched */ }
```

Two changed lines per test, nine tests, bodies verbatim. `seeded` is
about twenty lines: derive the seed list, honour `SEED` and `ITERATIONS`
environment overrides as gpui does, print the seed, and run. **No proc macro
is needed** — the alternative of writing our own attribute macro would mean a
whole proc-macro crate to save nine lines.

**`#[ctor::ctor]` + `zlog::init_test()`** at `rope.rs:1735` and
`sum_tree.rs:1399`. These only initialise logging; the tests assert nothing
about it. Deleted, which drops the `ctor` and `zlog` dev-dependencies
entirely.

**`RandomCharIter`.** Moves into rope's test module with the rest of `util`
([above](#folding-vendorutil-in)). It needs `rand`.

### Consequences for the dependency plan

* `rand` becomes a dev-dependency of `vendor/rope` and `vendor/sum_tree`, and
  must be pinned to **0.9** — what Zed pins, and the API the tests are written
  against (`rng.random_range(..)`). crates.io is at 0.10, and taking it would
  mean editing test bodies, which defeats the point.
* `vendor/util` is not created at all; `RandomCharIter` and `seeded` live in
  rope's `#[cfg(test)]` module, needing no feature flag and no extra crate.
* `gpui`, `zlog`, and `ctor` are **not** vendored and not depended on.

### Beyond upstream's tests

* **The signature check in CI** from
  [section 6](#6-consequences-for-re-syncing) is the complement, not the
  primary defence — [section 3](#3-what-keeps-this-safe) is explicit that a
  mechanical diff check can no longer prove this change correct, so the tests
  are the verification. What the signature check catches is the one thing they
  cannot: a new upstream `pub fn` arriving with a bare `usize` in its
  signature.
* **Round-trip property tests** over `ByteOffset` ↔ `Point` ↔ `PointUtf16` ↔
  `OffsetUtf16` on random text with astral-plane characters. Already required
  by [section 10](core.md#10-testing) for the encoding
  layer; running them against the rope directly puts them one level lower,
  where a conversion bug originates.
* **Keep `benches/rope_benchmark.rs` too.** It is not a test, but it answers
  directly whether the wrapper indirection costs anything —
  [section 8](#8-decided-and-what-remains) argues it does not — and it is
  already written. This means taking `criterion` as a dev-dependency, which
  `deps.md` §12 previously declined; the justification now exists.

## 8. Decided, and what remains

The three questions this document opened are settled:

* **Hand-convert; do not generate.** Fifty-one near-identical wrappers is the
  kind of thing a macro could produce, and a macro over function signatures is
  unreadable in exactly the code where readability is the only safety argument
  we have ([section 3](#3-what-keeps-this-safe)). Hand-written it is.
* **The wrapper indirection is not a performance question.** `*_raw` calls and
  newtype operators are the canonical case for inlining; there is nothing here
  for a benchmark to find. `rope_benchmark.rs` is still kept
  ([section 7](#beyond-upstreams-tests)) because it guards against a real
  regression in the rope, not because this change is suspected of causing one.
* **`ByteLen` is separate and shared** —
  [section 4](#offsets-and-lengths-are-separate-types-and-bytelen-is-shared).

One asymmetry is left open, and it follows directly from that last decision.
**Why bytes and not UTF-16?** `OffsetUtf16` is used as both a position and a
length — `TextSummary.len_utf16` is a length — so the same argument would give
it a `Utf16Len`. It does not get one, because UTF-16 quantities exist only at
the wire edge ([section 8.3](core.md#83-the-wire-position-type-is-inert))
and never accumulate anywhere in our code, so the split would buy nothing but
would still cost the edits. That is a judgement about where the value is, not a
principle, and it should be revisited if UTF-16 arithmetic ever appears outside
the conversion functions.
