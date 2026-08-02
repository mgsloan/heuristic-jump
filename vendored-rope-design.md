# Changes to the vendored rope

`vendor/rope` is a copy of Zed's `crates/rope`. `core-implementation-design.md`
[section 16](core-implementation-design.md#vendoring-the-zed-crates) covers why
it is vendored and what patches the copy needs to compile. This document covers
one further change, which is larger than those and is a design decision rather
than a mechanical fix-up:

> **The rope's public API speaks in newtypes: `ByteOffset` and `ByteRange`
> instead of bare `usize` and `Range<usize>`, and `LineIndex`, `ByteColumn`,
> `Utf16Column`, and `CharCount` instead of the bare `u32`s in `Point`,
> `PointUtf16`, and `TextSummary`. The newtypes are opaque — no operators
> against bare integers.**

`claude.md` asks for newtypes on primitive fields, and the driver's correctness
rests on byte offsets more than on anything else — `core-implementation-design.md`
[section 4](core-implementation-design.md#position-encoding) calls position
handling the highest-risk detail in the whole driver, and
[section 18](core-implementation-design.md#18-protocol-types) drops `lsp-types`
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
[section 4](core-implementation-design.md#position-encoding) of the core
design calls the highest-risk in the driver — invisible on ASCII, wrong by a
few columns on any line that is not.

So the change is better understood as **completing rope's existing newtype
family** than as patching it.

## 2. Where the types live is forced

`shared` depends on `rope`
([section 16](core-implementation-design.md#the-dependency-graph)), so rope
cannot depend on `shared`. `ByteOffset` therefore **lives in `rope`**, and
`shared` re-exports it:

```rust
// vendor/rope/src/byte_offset.rs
pub struct ByteOffset(pub usize);
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
keeps [section 12](core-implementation-design.md#vocabulary-types)'s claim —
that these are the shared vocabulary — true from the outside.

There is one method this placement cannot accommodate.
[Section 12](core-implementation-design.md#vocabulary-types) gives `ByteRange`
a `shifted_by(&InputEdit)` for spot anchoring, and `InputEdit` is
tree-sitter's. rope must not grow a tree-sitter dependency for one method, so:

* rope's `ByteRange` carries only the text-shaped operations — `contains`,
  `overlaps`, `len`, `is_empty`, `intersect`.
* `shared` supplies `shifted_by` through an extension trait.

That split is a little awkward to read and it is the honest consequence of the
dependency direction. The alternative — a third crate below both, existing to
hold two structs — is worse.

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

And no arithmetic operators at all, with one exception.

**`ByteOffset` gets `Add`, `Sub`, `AddAssign`, `SubAssign` against `Self`** —
never against `usize`. That is consistent rather than special: this crate
deliberately treats an offset and a length as the same type
([below](#offsets-and-lengths-are-one-type-deliberately)), so adding two of
them is meaningful.

**`LineIndex`, `ByteColumn`, `Utf16Column`, and `CharCount` get none.** Adding
two line numbers is meaningless; there is no length interpretation to rescue
it. Where rope needs the arithmetic it unwraps explicitly, which is the
unwrap-operate-rewrap shape from [section 3](#3-what-keeps-this-safe) and is
visible at the point it happens.

`Point` and `PointUtf16` keep their existing `Add`, `Sub`, and `AddAssign`
impls, with the bodies edited to unwrap. This is a compromise and it is flagged
as such: `Point + Point` treats one operand as absolute and the other as
relative, which is the same conflation being rejected for `LineIndex` one
paragraph up. It is kept because rope's internals rely on it throughout, and it
is recorded as future question 12 in `readme.md` — the fix is a distinct
`PointDelta`, and it is a bigger change than this one.

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
| Length | `Rope::len`, `ChunkSlice::len` |
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
    pub len: ByteOffset,
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
[section 16](core-implementation-design.md#vendoring-the-zed-crates)'s claim
that it needs no patching survives.

### Offsets and lengths are one type, deliberately

`Rope::len()` returns a length, not a position, and the pedantic version of
this change would give it a separate `ByteLen`. It does not.

The original reason — that separating them would force conversions inside
bodies — no longer applies, since bodies are editable. The reason that remains
is semantic and is the better one anyway: in this crate a length genuinely *is*
an offset. `TextSummary.len` is fed straight to cursor seeks, summaries add
lengths to produce positions, and a separate type would mean a conversion at
every one of those, each of which is a place to get the direction wrong. One
type is worth more than the distinction, and it is what makes `Add<Self>` on
`ByteOffset` coherent.

`resolution-design.md` names a `ByteLen` for its byte-budget accounting. That
is a different quantity — bytes read, not a position in a document — and it
stays where it is.

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
* **`sum_tree` and `vendor/util`.** Untouched by this.

### `LineIndex` is rope's, and `shared` re-exports it

`Location` carries a `LineIndex`
([section 18.4](core-implementation-design.md#184-location-is-byte-based-and-this-fixes-a-real-inconsistency)).
Since `Point.row` is now a `LineIndex`, the type has to live in rope for the
same dependency reason as `ByteOffset`, and `shared` re-exports it. The
row-taking APIs — `slice_rows(Range<LineIndex>)`, `line_len(row: LineIndex)` —
take it too, so a row obtained from a `Point` can be handed straight back to
the rope.

## 6. Consequences for re-syncing

[Section 16](core-implementation-design.md#vendoring-the-zed-crates) argues
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
section 16.

## 7. Testing

### All of upstream's tests are kept

`core-implementation-design.md`
[section 16](core-implementation-design.md#vendoring-the-zed-crates) proposes
deleting rope's `#[cfg(test)]` modules because they reach for `gpui`, `zlog`,
and `ctor`. **That is reversed: every test is preserved.** Once we are editing
the crate, deleting its tests is exactly backwards — they are the only
independent check that a 51-function signature sweep and six hand-edited lines
did not change behaviour, and several of them are randomised differential
tests against a `String` oracle, which is precisely the kind of test nobody
would write from scratch.

What they need turns out to be small. Only three things stand between the test
modules and a plain `cargo test`:

**`#[gpui::test(iterations = N)]` on nine functions.** Despite the name,
nothing about gpui is involved for these: rope's randomised tests take
`mut rng: StdRng` and nothing else — no `TestAppContext`, no async. The macro
is doing one job, which is to run the body N times with deterministic seeds and
print the seed on failure. That is replaced by a helper in the cut-down
`vendor/util`, under a `test-support` feature:

```rust
// upstream
#[gpui::test(iterations = 100)]
fn test_random_rope(mut rng: StdRng) { /* body */ }

// vendored
#[test]
fn test_random_rope() { util::seeded(100, test_random_rope_inner) }
fn test_random_rope_inner(mut rng: StdRng) { /* body, untouched */ }
```

Two changed lines per test, nine tests, bodies verbatim. `util::seeded` is
about twenty lines: derive the seed list, honour `SEED` and `ITERATIONS`
environment overrides as gpui does, print the seed, and run. **No proc macro
is needed** — the alternative of writing our own attribute macro would mean a
whole proc-macro crate to save nine lines.

**`#[ctor::ctor]` + `zlog::init_test()`** at `rope.rs:1735` and
`sum_tree.rs:1399`. These only initialise logging; the tests assert nothing
about it. Deleted, which drops the `ctor` and `zlog` dev-dependencies
entirely.

**`util::RandomCharIter`.** Already on
[section 16](core-implementation-design.md#vendoring-the-zed-crates)'s list of
items the cut-down `vendor/util` keeps. It needs `rand`.

### Consequences for the dependency plan

* `rand` becomes a dev-dependency of `vendor/rope` and `vendor/sum_tree`, and
  must be pinned to **0.9** — what Zed pins, and the API the tests are written
  against (`rng.random_range(..)`). crates.io is at 0.10, and taking it would
  mean editing test bodies, which defeats the point.
* `vendor/util` grows a `test-support` feature carrying `RandomCharIter` and
  `seeded`, plus its own `rand` dependency behind that feature.
* `gpui`, `zlog`, and `ctor` are **not** vendored and not depended on.

### Beyond upstream's tests

* **The CI diff check** from
  [section 3](#3-what-keeps-this-safe) remains the primary
  defence. The tests tell you the crate still works; the diff check tells you
  *why*, which is what makes the change reviewable rather than merely passing.
* **Round-trip property tests** over `ByteOffset` ↔ `Point` ↔ `PointUtf16` ↔
  `OffsetUtf16` on random text with astral-plane characters. Already required
  by [section 15](core-implementation-design.md#15-testing) for the encoding
  layer; running them against the rope directly puts them one level lower,
  where a conversion bug originates.
* **Keep `benches/rope_benchmark.rs` too.** It is not a test, but it is the
  direct answer to open question 2 below — whether the wrapper indirection
  costs anything — and it is already written. This means taking `criterion` as
  a dev-dependency, which `dependency-plan.md` §12 previously declined; the
  justification now exists.

## 8. Open questions

1. **Should the conversion be generated rather than hand-written?** Fifty-one
   near-identical wrappers is the kind of thing a macro or a small codegen
   script does more reliably than a person, and it would make a re-sync a
   re-run instead of a re-edit. Against: a macro over function signatures is
   hard to read, and the whole point of the invariant in
   [section 3](#3-what-keeps-this-safe) is that the patch be
   obvious on inspection.

2. **Does the `*_raw` indirection cost anything measurable?** It should inline
   away completely, and so should every newtype operator. Worth confirming once
   rather than assuming, since these are the hottest functions in the system —
   and upstream's `rope_benchmark.rs`, kept per
   [section 7](#beyond-upstreams-tests), answers it directly.

3. **Should `ByteLen` be separate after all?**
   [Section 4](#offsets-and-lengths-are-one-type-deliberately) keeps one type,
   and the argument is now semantic rather than forced — allowing body edits
   removed the practical objection. Worth revisiting once there is code using
   the API in anger, since the answer depends on whether length-versus-position
   mistakes actually show up.
