//! Ours, not upstream's: what `design/rope-modifications.md` asks for on top
//! of the tests the vendored copy already keeps.
//!
//! §3 is explicit that a mechanical diff check can no longer prove the newtype
//! sweep correct, so upstream's own tests are the verification and everything
//! here is the complement. They cover what upstream's cannot, which divides
//! into three:
//!
//! * **The API still names its units** — §6's bare-primitive scan, §4's
//!   operator table as an inventory, the declared types on the converted
//!   surface, and the constructors. Upstream's tests pass either way, since
//!   the bodies compile with or without the newtypes.
//! * **The four units agree with each other** — §7's round trip, one level
//!   below `shared::proto`, where a conversion bug originates.
//! * **The document and the crate still describe each other** — §4's
//!   conversion table read out of the document, and §7's test counts. This is
//!   the half that had already failed silently: the document said nine
//!   conversions where there are eight, and nothing could tell that from a
//!   dropped test.
//!
//! Several checks here can only fail on input that does not compile, and each
//! says so in place. They are kept because the compiler's enforcement is
//! incidental — it holds only while the mistake happens to be a build error,
//! and nothing else would record that the rule was deliberate.

use std::fmt::{Debug, Display};
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use rand::prelude::*;
use rope::{
    ByteColumn, ByteLen, ByteRange, CharCount, Chunk, ChunkSlice, LineIndex, Offset, OffsetUtf16,
    Point, PointUtf16, Rope, TextSummary, Utf16Column,
};

// The bench's trick, for the bench's reason: an integration test is its own
// crate and cannot see rope's `#[cfg(test)] mod test_support`.
#[path = "../src/test_support.rs"]
mod test_support;

use test_support::RandomCharIter;

/// The three modules §2 puts the vocabulary newtypes in. Every claim about
/// what they do and do not implement is scoped to these: an `impl` elsewhere
/// in the crate is rope's own code using them, which is what they are for.
const NEWTYPE_MODULES: [&str; 3] = ["offset.rs", "point.rs", "point_utf16.rs"];

/// §6: **CI asserts that no `pub fn` signature in `vendor/rope` mentions bare
/// `usize` or `u32`**, outside `allowed-primitives.txt`.
///
/// This is the enforcement the whole conversion rests on. An upstream change
/// to a signature conflicts on re-sync and is obvious on sight; an upstream
/// *new* public function arriving with a bare primitive is a hunk that looks
/// entirely normal, and nothing about the diff flags it.
#[test]
fn no_public_signature_names_a_bare_primitive() {
    let allowed = allowed_primitives();
    let mut offenders = Vec::new();

    for source in sources() {
        let text = fs::read_to_string(&source).expect("reading a source file of this crate");
        for (name, signature) in public_signatures(&text) {
            if allowed.iter().any(|entry| entry == name) {
                continue;
            }
            if mentions_bare_primitive(&signature) {
                offenders.push(format!(
                    "{}: pub fn {name}{signature}",
                    source.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a public signature names a bare `usize` or `u32`. Either convert it \
         to the newtype for its unit, or add it to `allowed-primitives.txt` \
         with a comment saying which unit it is \
         (`design/rope-modifications.md` §6):\n{}",
        offenders.join("\n")
    );
}

/// §6: "The allowlist is `vendor/rope/allowed-primitives.txt` and is
/// **empty**." `vendor/README.md` says it a second time — "**That file has no
/// entries** (CHANGE-core-015)" — and the file's own header says it a third.
/// Three assertions of one fact, and the only code that read the file was
/// [`no_public_signature_names_a_bare_primitive`], which *consumes* entries
/// rather than bounding them: an entry silently weakens the check by exactly
/// one function and every one of those three sentences goes quietly false.
///
/// That is the whole reason for asserting it here rather than trusting it. §6
/// is explicit that the file is not dead — "what it is for is the re-sync case
/// … not the conversion's leftovers" — so this does not forbid an entry. It
/// prices one: adding a function here means also editing the three places that
/// say there are none, which is what §6 asks for when it says "an entry is a
/// hole in the change" and "keep this short".
#[test]
fn the_bare_primitive_allowlist_is_empty_so_the_scan_forgives_nothing() {
    let allowed = allowed_primitives();
    assert!(
        allowed.is_empty(),
        "`allowed-primitives.txt` forgives {allowed:?}, and three places say it \
         forgives nothing: `rope-modifications.md` §6, `vendor/README.md`'s \
         patch-7 entry, and this file's own header. If the entry is right — a \
         re-synced upstream `pub fn` whose primitive is genuinely one — say so \
         in all three, with the comment §6 requires naming the unit. What must \
         not happen is the list growing while the documents go on calling it \
         empty"
    );
}

/// The negative control for the check above, which is what makes it evidence
/// rather than a test that has never failed: the scanner has to *find* a bare
/// primitive when one is there, and has to read a multi-line signature, which
/// is the shape a converted function is most likely to be re-broken in.
#[test]
fn the_signature_scan_finds_what_it_is_looking_for() {
    let planted = "
        impl Rope {
            pub fn len(&self) -> ByteLen { }
            pub fn resync_hazard(
                &self,
                offset: usize,
            ) -> Point { }
            fn private(&self, offset: usize) -> usize { }
        }
    ";

    let found: Vec<&str> = public_signatures(planted)
        .into_iter()
        .filter(|(_, signature)| mentions_bare_primitive(signature))
        .map(|(name, _)| name)
        .collect();

    assert_eq!(
        found,
        vec!["resync_hazard"],
        "the scan must see a multi-line signature, must not see a converted \
         one, and must not see a private function"
    );
}

/// §4 converts `TextSummary` too — all nine fields — and the argument it gives
/// is that "leaving the crate's central summary type as bare integers while
/// everything around it is typed would be the worst of both".
///
/// A field is not a signature, so the scan above does not see one: `pub len:
/// usize` reappearing on `TextSummary` would leave every `pub fn` converted
/// and put the bare integer back in the type the whole crate accumulates into.
/// Named fields only — `Offset(pub usize)` is the newtype's own contents and
/// is where the primitive is supposed to be.
#[test]
fn no_public_field_names_a_bare_primitive() {
    let mut offenders = Vec::new();

    for source in sources() {
        let text = fs::read_to_string(&source).expect("reading a source file of this crate");
        for field in public_fields(&text) {
            if mentions_bare_primitive(&field) {
                offenders.push(format!(
                    "{}: pub {field}",
                    source.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a public field names a bare `usize` or `u32` rather than the newtype \
         for its unit (`design/rope-modifications.md` §4):\n{}",
        offenders.join("\n")
    );
}

/// §4: **the newtypes are opaque.** The tempting shortcut is `Add<u32>`,
/// `AddAssign<u32>`, `PartialEq<u32>` and `PartialOrd<u32>`, so that
/// `p.row += 1` and `p.column == 0` keep compiling and no body needs editing —
/// and it is refused, because a type that compares and adds against bare
/// integers is a type bare integers flow into, which is the situation the
/// whole conversion exists to end. (It is also subtly broken: cross-type
/// `PartialOrd` does not participate in generic `Ord` code, so the ordering
/// would be asymmetric depending on how it was reached.)
///
/// That is a claim about what does *not* exist, so nothing but a scan can hold
/// it: the bodies compile either way, and every test in this crate would go on
/// passing if someone added the impls and deleted the unwraps.
#[test]
fn no_newtype_implements_a_trait_against_a_bare_primitive() {
    let offenders: Vec<String> = NEWTYPE_MODULES
        .into_iter()
        .flat_map(|file| {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
            let text = fs::read_to_string(path).expect("reading a newtype module");
            impl_headers(&text)
                .into_iter()
                .filter(|header| mentions_bare_primitive(header))
                .map(|header| format!("{file}: {header}"))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "a vocabulary newtype implements a trait against a bare integer, which \
         is the half-transparent type `design/rope-modifications.md` §4 \
         refuses:\n{}",
        offenders.join("\n")
    );
}

/// §4 prints an operator table — five rows that exist and one that must not —
/// and the scan above holds none of it. That scan rejects an impl naming a
/// bare integer, which is the *half-transparent* mistake; this one holds the
/// table itself, where the mistake is a missing row or an extra one between
/// two newtypes, and no bare integer appears in either case.
///
/// | `Offset + ByteLen` | `Offset` | advance a position |
/// | `Offset - ByteLen` | `Offset` | retreat a position |
/// | `Offset - Offset` | `ByteLen` | distance between positions |
/// | `ByteLen ± ByteLen` | `ByteLen` | accumulate |
/// | `Offset + Offset` | — | **not implemented**; meaningless |
///
/// The last row is the one that catches real mistakes and is the reason this
/// is an inventory rather than a list of things to look for: a prohibition
/// can only be checked by naming everything that *is* allowed. The comparison
/// therefore fails in both directions — an operator that disappears is as much
/// a change to the table as one that arrives.
///
/// `From` is scanned with the operators for the same reason. §4:
/// "There is deliberately **no `From<ByteLen> for Offset`**. Turning a length
/// into a position means measuring from somewhere, so it is spelled
/// `Offset::ZERO + len`, which names the origin." A `From` between two of
/// these types is an operator by another spelling, and would put back exactly
/// what the position/quantity split buys.
#[test]
fn the_operator_table_is_exactly_what_the_document_prints() {
    // §4, in order: the byte pair's five rows and the matching assigns, then
    // `Point` and `PointUtf16` keeping the `Add`/`Sub`/`AddAssign` impls
    // rope's internals rely on -- which §4 flags as a compromise rather than
    // leaving implicit, since `Point + Point` is the same conflation being
    // refused for `LineIndex` one paragraph up.
    //
    // `LineIndex`, `ByteColumn`, `Utf16Column` and `CharCount` are absent on
    // purpose, and their absence is the point of the whole test: "Adding two
    // line numbers is meaningless; there is no length interpretation to
    // rescue it."
    let expected = [
        "impl Add<ByteLen> for Offset",
        "impl Sub<ByteLen> for Offset",
        "impl Sub for Offset",
        "impl AddAssign<ByteLen> for Offset",
        "impl SubAssign<ByteLen> for Offset",
        "impl Add for ByteLen",
        "impl Sub for ByteLen",
        "impl AddAssign for ByteLen",
        "impl SubAssign for ByteLen",
        "impl<'a> Add<&'a Self> for Point",
        "impl Add for Point",
        "impl<'a> Sub<&'a Self> for Point",
        "impl Sub for Point",
        "impl<'a> AddAssign<&'a Self> for Point",
        "impl AddAssign<Self> for Point",
        "impl<'a> Add<&'a Self> for PointUtf16",
        "impl Add for PointUtf16",
        "impl<'a> Sub<&'a Self> for PointUtf16",
        "impl Sub for PointUtf16",
        "impl<'a> AddAssign<&'a Self> for PointUtf16",
        "impl AddAssign<Self> for PointUtf16",
    ];

    let mut found = Vec::new();
    for file in NEWTYPE_MODULES {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
        let text = fs::read_to_string(path).expect("reading a newtype module");
        found.extend(operator_impls(&text));
    }

    let mut expected: Vec<&str> = expected.to_vec();
    let mut found: Vec<&str> = found.iter().map(String::as_str).collect();
    expected.sort_unstable();
    found.sort_unstable();

    assert_eq!(
        found, expected,
        "the operator impls on the vocabulary newtypes are no longer the table \
         `design/rope-modifications.md` §4 prints. An extra one is a row the \
         document does not have -- `Offset + Offset` and any arithmetic at all \
         on `LineIndex`, `ByteColumn`, `Utf16Column` or `CharCount` are the \
         ones it names as meaningless. A missing one is the table changing \
         underneath the document. Either way, change both or neither"
    );
}

/// The negative control for the inventory, and it has to plant *both*
/// directions: the extractor must see an operator impl that the table does not
/// have, and must not mistake `Display` or an inherent block for one.
#[test]
fn the_operator_scan_sees_an_impl_the_table_does_not_have() {
    let planted = "
        impl Add for Offset { }
        impl From<ByteLen> for Offset { }
        impl AddAssign<u32> for LineIndex { }
        impl std::ops::Add for LineIndex { }
        impl fmt::Display for Offset { }
        impl Offset { pub const ZERO: Self = Self(0); }
        impl<'a> Add<&'a Self> for Point { }
    ";

    assert_eq!(
        operator_impls(planted),
        vec![
            "impl Add for Offset",
            "impl From<ByteLen> for Offset",
            "impl AddAssign<u32> for LineIndex",
            "impl std::ops::Add for LineIndex",
            "impl<'a> Add<&'a Self> for Point",
        ],
        "the scan must see an arithmetic or conversion impl however it is \
         spelled, must not see `Display`, and must not see an inherent block"
    );
}

/// §4's converted surface, written out as bindings whose types are declared.
/// Nothing here runs that could not be checked by reading; the point is that
/// the compiler now does the reading, and a signature that reverts stops the
/// crate compiling instead of being noticed.
///
/// The scans above cannot do this job. They see a bare `usize` or `u32`, so
/// they catch a signature going back to *no* unit — but `first_line_chars() ->
/// Utf16Column` names a unit and is wrong, and `len() -> Offset` names a unit
/// and is the position/quantity confusion §4 exists to end. Only the declared
/// type catches those, and this is the section's claim about *which* newtype
/// each thing got rather than that it got one.
///
/// The values asserted alongside are the ones that make the units observable:
/// `aé` is three bytes, two chars and two UTF-16 code units, so a line's
/// length is a different number in each and only the type says which.
#[test]
fn the_public_surface_speaks_in_the_units_it_measures_in() {
    let rope = Rope::from("aé\nbb\n");

    // Length: §4's table, "`Rope::len`, `ChunkSlice::len` — both return
    // `ByteLen`". Not `Offset`: that is the split §4 spends a section on.
    let length: ByteLen = rope.len();
    assert_eq!(length, ByteLen(7));

    // Rows: §5's `LineIndex` is rope's, so "a row obtained from a `Point` can
    // be handed straight back to the rope".
    let last: Point = rope.max_point();
    let row: LineIndex = last.row;
    let column: ByteColumn = last.column;
    assert_eq!((row, column), (LineIndex(2), ByteColumn::ZERO));

    let line_length: ByteColumn = rope.line_len(LineIndex(0));
    assert_eq!(
        line_length,
        ByteColumn(3),
        "`aé` is three bytes, and `line_len` measures in bytes"
    );

    let first_row: Rope = rope.slice_rows(LineIndex(0)..LineIndex(1));
    assert_eq!(
        first_row.len(),
        ByteLen(4),
        "the row range is half-open in *points* -- `LineIndex(1)` is the start \
         of the second line, so the slice carries the newline that `line_len` \
         does not count"
    );

    let utf16: PointUtf16 = rope.max_point_utf16();
    let utf16_column: Utf16Column = utf16.column;
    assert_eq!(utf16_column, Utf16Column::ZERO);

    // Slicing takes a `ByteRange`, and `ByteRange::len` is a `ByteLen` --
    // §4's last operator row, the one that is a method rather than an
    // operator.
    let span: ByteRange = ByteRange::new(Offset(0), Offset(3));
    let span_length: ByteLen = span.len();
    assert_eq!(
        (span_length, rope.slice(span).len()),
        (ByteLen(3), ByteLen(3))
    );

    // §4: `TextSummary` is converted too, all nine fields. A field is not a
    // signature, so `no_public_field_names_a_bare_primitive` catches only a
    // field reverting to `usize` -- these say which unit each one is.
    let summary: TextSummary = rope.summary();
    let summary_length: ByteLen = summary.len;
    let characters: CharCount = summary.chars;
    let utf16_length: OffsetUtf16 = summary.len_utf16;
    let lines: Point = summary.lines;
    let first_line: CharCount = summary.first_line_chars;
    let last_line: CharCount = summary.last_line_chars;
    let last_line_utf16: Utf16Column = summary.last_line_len_utf16;
    let longest: LineIndex = summary.longest_row;
    let longest_characters: CharCount = summary.longest_row_chars;
    assert_eq!(
        (summary_length, characters, utf16_length, lines),
        (
            ByteLen(7),
            CharCount(6),
            OffsetUtf16(6),
            Point::new(LineIndex(2), ByteColumn::ZERO)
        ),
        "the three ways of measuring the same text disagree, which is the \
         point of measuring them in different types"
    );
    assert_eq!(
        (
            first_line,
            last_line,
            last_line_utf16,
            longest,
            longest_characters
        ),
        (
            CharCount(2),
            CharCount::ZERO,
            Utf16Column::ZERO,
            LineIndex::ZERO,
            CharCount(2)
        )
    );

    // §4's second table: the four functions that are *not* byte offsets and
    // get the correct newtype rather than being left bare. Being left bare is
    // what the signature scan would have permitted, since `allowed-primitives`
    // is where a `u32` goes to be forgiven -- and `longest_row`'s out
    // parameter was its only entry until CHANGE-core-015.
    let chunk = Chunk::new("aé\nbb");
    let slice: ChunkSlice<'_> = chunk.as_slice();
    let slice_length: ByteLen = slice.len();
    let chunk_first_line: CharCount = slice.first_line_chars();
    let chunk_last_line: CharCount = slice.last_line_chars();
    let chunk_last_line_utf16: Utf16Column = slice.last_line_len_utf16();
    let mut total_characters: CharCount = CharCount::ZERO;
    let (chunk_longest, chunk_longest_characters): (LineIndex, CharCount) =
        slice.longest_row(&mut total_characters);
    assert_eq!(
        (
            slice_length,
            chunk_first_line,
            chunk_last_line,
            chunk_last_line_utf16
        ),
        (ByteLen(6), CharCount(2), CharCount(2), Utf16Column(2)),
        "`aé` is two chars and two UTF-16 code units but three bytes, so the \
         first line's length is a different number in each unit"
    );
    assert_eq!(
        (chunk_longest, chunk_longest_characters, total_characters),
        (LineIndex::ZERO, CharCount(2), CharCount(5)),
        "the out parameter is the third value this function returns and the \
         one whose unit is least visible at the call site, so it is asserted \
         with the other two: `aé\\nbb` is five scalar values counting the \
         newline"
    );
}

/// §4: each newtype gets `ZERO` and `MAX` "so a literal never has to appear",
/// and a **hand-written `Display`** that "must print the bare number so that
/// `Point`'s own `write!(f, \"Point({}:{})\", self.row, self.column)` keeps
/// its output".
///
/// That last clause is the whole reason `Display` is written by hand rather
/// than left off, and it is the one claim in §4 with an observable
/// consequence: a `Display` that printed `LineIndex(3)`, or a `#[derive(Debug)]`
/// standing in for it, changes what `{:?}` on a `Point` produces. So the
/// output is asserted, not just the existence of the impl.
#[test]
fn every_newtype_has_its_bounds_and_prints_as_a_bare_number() {
    assert_eq!(
        (
            format!("{}", Offset::ZERO),
            format!("{}", ByteLen::ZERO),
            format!("{}", LineIndex::ZERO),
            format!("{}", ByteColumn::ZERO),
            format!("{}", Utf16Column::ZERO),
            format!("{}", CharCount::ZERO),
            format!("{}", ByteRange::EMPTY),
        ),
        (
            "0".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "0..0".to_owned(),
        ),
        "a newtype's `Display` prints the bare number, and `ByteRange`'s is the \
         two of them"
    );

    assert_eq!(
        (
            Offset::MAX,
            ByteLen::MAX,
            LineIndex::MAX,
            ByteColumn::MAX,
            Utf16Column::MAX,
            CharCount::MAX,
        ),
        (
            Offset(usize::MAX),
            ByteLen(usize::MAX),
            LineIndex(u32::MAX),
            ByteColumn(u32::MAX),
            Utf16Column(u32::MAX),
            CharCount(usize::MAX),
        ),
        "the byte pair is `usize`-shaped, the *line*-shaped three are `u32`, \
         and `CharCount` is the one member of the family that goes with the \
         byte pair instead. §2: it is \"the width of the widest thing it has \
         to hold rather than a preference\" -- `TextSummary.chars` accumulates \
         across the whole rope and is a `usize` upstream, so a `u32` here \
         would cap a summary at 4G scalar values, which §3 forbids as an edit \
         to the arithmetic. The bound `Point.row` imposes on itself is not an \
         argument for imposing another one here"
    );

    // The consequence §4 names, and the reason `Display` could not simply be
    // derived: `Point`'s `Debug` writes its two fields through `Display`.
    assert_eq!(
        format!("{:?}", Point::new(LineIndex(3), ByteColumn(4))),
        "Point(3:4)"
    );
}

/// §4 converts `TextSummary`, and §3 says what a conversion edit may be:
/// "**Every edit changes representation, never arithmetic.**" `add_newline` is
/// where that is hardest to check, because it is the one method on the type
/// with no caller and no upstream test — nothing in the crate would notice
/// either a sweep that mangled it or the two upstream bugs it actually had.
///
/// So the oracle is the type's other constructor. `From<&str>` is what every
/// summary in the crate is built from, and adding a newline to a text has to
/// agree with summarising the text with a newline on the end; the empty case
/// is `TextSummary::newline()`, which is the same claim with the fields
/// written out by hand twelve lines above the method.
///
/// Both of the old bugs fail it, and differently: `len_utf16 +=
/// OffsetUtf16(len_utf16.0 + 1)` doubles rather than increments, so it passes
/// on the empty summary and fails on everything else, and the missing
/// `chars += 1` fails on all of them including the empty one.
#[test]
fn add_newline_agrees_with_the_summary_of_the_same_text_plus_a_newline() {
    let mut empty = TextSummary::default();
    empty.add_newline();
    assert_eq!(
        empty,
        TextSummary::newline(),
        "a newline added to nothing is `TextSummary::newline()`, which is the \
         same nine fields written out by hand"
    );

    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let length = rng.random_range(0..512);
        let text: String = RandomCharIter::new(&mut rng).take(length).collect();

        let mut incremented = TextSummary::from(text.as_str());
        incremented.add_newline();

        assert_eq!(
            incremented,
            TextSummary::from(format!("{text}\n").as_str()),
            "add_newline disagrees with the summary of the same text plus a \
             newline (seed {seed}, {} bytes)",
            text.len()
        );
    }
}

/// §2 makes `CharCount` the one member of the family backed by `usize` rather
/// than `u32`, and the reason is this type: `TextSummary.chars` "accumulates
/// across the whole rope" and is a `usize` upstream, so narrowing it "would
/// have silently capped a summary at 4G scalar values -- which is an edit to
/// the *arithmetic*, exactly what §3 says a hunk may never be".
///
/// The crate carried the narrowed version until this test arrived, recorded in
/// `vendor/README.md` as deliberate and attributed to §4 -- which prints
/// `chars: CharCount, // usize before and after`, so the attribution was to a
/// section saying the opposite. A repr is not the sort of thing a re-sync diff
/// makes obvious, and the whole file is otherwise about *which* newtype a
/// signature carries rather than how wide it is.
///
/// A 4G-character rope cannot be built in a test and does not have to be: the
/// accumulation is `AddAssign`, which is what the sum tree runs at every
/// internal node, so summing two summaries is the same arithmetic.
#[test]
fn a_summary_accumulates_scalar_values_across_the_whole_usize_range() {
    let mut total = TextSummary::default();
    total.chars = CharCount(usize::MAX - 1);
    let mut one = TextSummary::default();
    one.chars = CharCount(1);
    total += &one;
    assert_eq!(
        total.chars, CharCount(usize::MAX),
        "a summary's char count is bounded by `usize` and not by anything \
         narrower, which is upstream's own bound and is what §2 means by \
         \"the width of the widest thing it has to hold\""
    );

    // The bound `usize` imposes on a 32-bit target *is* `u32`'s, and is still
    // upstream's, so the straddle below degrades rather than becoming false.
    if usize::BITS > 32 {
        let four_billion = CharCount(u32::MAX as usize);
        let mut straddling = TextSummary::default();
        straddling.chars = four_billion;
        let mut again = TextSummary::default();
        again.chars = four_billion;
        straddling += &again;
        assert_eq!(
            straddling.chars,
            CharCount(2 * (u32::MAX as usize)),
            "two summaries that each fill a `u32` add to one that does not, \
             and this is the case §2 says a narrowed `chars` would have capped"
        );
    }
}

/// §7: round-trip property tests over `Offset` ↔ `Point` ↔ `PointUtf16` ↔
/// `OffsetUtf16` on random text with astral-plane characters. `core.md` §10
/// already requires them for the encoding layer; running them against the
/// rope directly puts them one level lower, where a conversion bug
/// originates.
///
/// Every offset tested is a character boundary, which is the whole domain of
/// the conversions: rope's `point_to_offset` and `point_utf16_to_offset`
/// reach `debug_panic!` inside a scalar value, and refusing that is
/// `shared::proto`'s job rather than this one.
#[test]
fn the_four_units_round_trip_against_each_other() {
    // §7 asks for the round trip "on random text **with astral-plane
    // characters**", and that is the corpus doing the work rather than the
    // assertions: below the astral plane a UTF-16 code unit and a Unicode
    // scalar value are the same count, so `Utf16Column` and `CharCount` agree
    // everywhere and a conversion that confused them would round-trip
    // perfectly. `RandomCharIter` emits four-byte characters about one time in
    // eight -- but nothing said so, and `SIMPLE_TEXT=1` in the environment
    // turns the whole corpus into lowercase ASCII.
    let mut astral = 0usize;
    let mut where_the_units_disagree = 0usize;

    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let length = rng.random_range(0..512);
        let text: String = RandomCharIter::new(&mut rng).take(length).collect();
        let rope = Rope::from(text.as_str());

        astral += text
            .chars()
            .filter(|character| character.len_utf16() == 2)
            .count();
        where_the_units_disagree +=
            usize::from(rope.offset_to_offset_utf16(Offset(text.len())).0 != text.chars().count());

        assert_eq!(rope.len(), ByteLen(text.len()), "seed {seed}");

        for (index, _) in text.char_indices().chain(Some((text.len(), '\0'))) {
            let offset = Offset(index);

            let point = rope.offset_to_point(offset);
            let point_utf16 = rope.offset_to_point_utf16(offset);
            let offset_utf16 = rope.offset_to_offset_utf16(offset);

            assert_eq!(
                rope.point_to_offset(point),
                offset,
                "offset -> point -> offset at {index} (seed {seed})"
            );
            assert_eq!(
                rope.point_utf16_to_offset(point_utf16),
                offset,
                "offset -> point_utf16 -> offset at {index} (seed {seed})"
            );
            assert_eq!(
                rope.offset_utf16_to_offset(offset_utf16),
                offset,
                "offset -> offset_utf16 -> offset at {index} (seed {seed})"
            );
            assert_eq!(
                rope.point_to_point_utf16(point),
                point_utf16,
                "point -> point_utf16 at {index} (seed {seed})"
            );
            assert_eq!(
                rope.point_utf16_to_point(point_utf16),
                point,
                "point_utf16 -> point at {index} (seed {seed})"
            );

            // The row is the one component the two point types share, and it
            // is a `LineIndex` in both — which is the sweep's claim about them
            // and is the thing a `u32` in either position would have hidden.
            assert_eq!(
                point.row, point_utf16.row,
                "the two point types disagree about the row at {index} (seed {seed})"
            );
            assert_eq!(
                rope.line_len(point.row),
                rope.offset_to_point(line_end(&text, point.row)).column,
                "line_len disagrees with the point at the end of its line at \
                 {index} (seed {seed})"
            );
        }
    }

    assert!(
        astral > 0 && where_the_units_disagree > 0,
        "the corpus held {astral} astral-plane characters across {} seeds where \
         the UTF-16 length and the scalar-value count differ. §7 asks for this \
         round trip on text *with* them, and without them `Utf16Column` and \
         `CharCount` count the same thing everywhere -- so every assertion \
         above would pass against a conversion that confused the two. \
         `SIMPLE_TEXT` in the environment does exactly this",
        where_the_units_disagree
    );
}

/// §7: "**Every test is preserved**", because upstream's tests "are the only
/// independent check that a signature sweep and the body edits that follow
/// from it did not change behaviour".
///
/// This one is here because its absence had already cost something. §7 and
/// `vendor/README.md` both said `#[gpui::test(iterations = N)]` was on **nine**
/// functions and that nine were converted; there are eight `seeded` sites.
/// Settling it needed the upstream revision fetched, and the answer was that
/// nine attributes became eight `seeded` conversions and one plain `#[test]`
/// (CHANGE-core-001). Nothing in the repository could tell that apart from a
/// dropped test, which is what these counts now do.
#[test]
fn every_upstream_randomised_test_is_still_run() {
    let text = fs::read_to_string(source("rope.rs")).expect("reading rope.rs")
        + &fs::read_to_string(source("chunk.rs")).expect("reading chunk.rs");

    let mut run: Vec<&str> = seeded_call_sites(&text);
    let mut defined: Vec<&str> = text
        .match_indices("fn ")
        .filter_map(|(index, _)| {
            let rest = &text[index + "fn ".len()..];
            let end = rest.find('(')?;
            let name = &rest[..end];
            name.ends_with("_inner").then_some(name)
        })
        .collect();
    run.sort_unstable();
    defined.sort_unstable();

    assert_eq!(
        run, defined,
        "every `_inner` body is one of upstream's randomised tests with its \
         attribute replaced, so a body with no `seeded` call in front of it is \
         a test that no longer runs (`design/rope-modifications.md` §7)"
    );
    assert_eq!(
        run.len(),
        8,
        "upstream has nine `#[gpui::test]` at the pinned revision, of which \
         eight carry `iterations = N` and become a `seeded` call. The ninth \
         takes no `rng` and is a plain `#[test]`. If this number falls, a \
         randomised test was dropped rather than converted"
    );

    assert!(
        text.contains("fn test_point_utf16_to_offset_clips_to_correct_absolute_offset"),
        "the ninth conversion -- upstream's one bare `#[gpui::test]` -- is a \
         plain `#[test]` and has no `seeded` call to be counted by, so it is \
         named here or it is held by nothing"
    );

    let tests = text.matches("#[test]").count();
    assert_eq!(
        tests, 24,
        "upstream has 24 test functions in these two files and so do we. \
         Upstream's tests are the verification of the sweep, so this number \
         going down is the sweep losing its oracle; a test of *ours* belongs \
         in this directory rather than in `src/`, which is why the number is \
         exact rather than a floor"
    );
}

/// §7's three substitutions, from the other side: what the vendored copy must
/// no longer mention. `gpui`, `zlog` and `ctor` are "**not** vendored and not
/// depended on", and the `#[ctor::ctor]` logger initialisers "only initialise
/// logging and nothing asserts on it". `util` is folded in (§4), so it is on
/// the same list.
///
/// **The compiler is the real enforcement here, and this records the intent.**
/// Written back today, `use gpui;` does not resolve and the crate does not
/// build — so the failure this scan is written for cannot be reached by
/// planting the text alone, which is why its control below runs on synthetic
/// input rather than on the crate. The regression it *can* see is the real
/// one: a re-sync that brings back `#[ctor::ctor] fn init_logger` together
/// with the dev-dependency compiles perfectly well, and puts a crate §7 spent
/// a paragraph removing back into the build.
///
/// The names all survive in prose — `test_support.rs` quotes
/// `#[gpui::test(iterations = N)]` to say what it replaces, and every
/// relocated `util` item carries an attribution naming its upstream path — so
/// this looks for the ways they would be *used*, and skips comments.
#[test]
fn neither_vendored_crate_reaches_for_gpui_zlog_or_ctor() {
    let mut offenders = Vec::new();
    for directory in ["rope", "sum_tree"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(directory);
        let manifest = fs::read_to_string(path.join("Cargo.toml")).expect("reading a manifest");
        offenders.extend(
            banished_dependencies(&manifest)
                .into_iter()
                .map(|name| format!("{directory}/Cargo.toml depends on {name}")),
        );
        for source in sources_in(&path.join("src")) {
            let text = fs::read_to_string(&source).expect("reading a vendored source file");
            offenders.extend(banished_uses(&text).into_iter().map(|used| {
                format!(
                    "{directory}/src/{}: {used}",
                    source.file_name().unwrap_or_default().to_string_lossy()
                )
            }));
        }
    }

    assert!(
        offenders.is_empty(),
        "a vendored crate reaches for something `design/rope-modifications.md` \
         §7 says is not vendored and not depended on. `util` is folded in \
         (§4), and `gpui`, `zlog` and `ctor` were what stood between \
         upstream's test modules and a plain `cargo test`:\n{}",
        offenders.join("\n")
    );
}

/// The control for the scan above, on synthetic input for the reason that
/// scan's own comment gives: planting `use gpui;` in the crate does not fail
/// the test, it fails the *build*, and a test that cannot run is not evidence.
#[test]
fn the_banished_crate_scan_finds_what_it_is_looking_for() {
    let planted = "
        // Replaces `#[gpui::test(iterations = N)]`, which does one job.
        //! Lifted from Zed's `crates/util/src/util.rs`.
        use util::RandomCharIter;
        #[ctor::ctor]
        fn init_logger() { zlog::init_test(); }
    ";

    assert_eq!(
        banished_uses(planted),
        vec!["use util::", "ctor::ctor", "zlog::"],
        "the scan must see the substitutions coming back and must not see the \
         comments that record them having gone"
    );

    let manifest = "
[dev-dependencies]
rand.workspace = true
ctor = \"0.2\"
gpui = { path = \"../gpui\" }
";
    assert_eq!(
        banished_dependencies(manifest),
        vec!["ctor", "gpui"],
        "a dependency is how the attribute above gets to compile, so it is \
         half of the same check"
    );
}

/// §4: "What each newtype gets: `#[derive(Copy, Clone, Default, PartialEq, Eq,
/// PartialOrd, Ord, Hash, Debug)]`", plus the hand-written `Display`.
///
/// A bound is the whole test: the list is a list of traits, and a trait that
/// stops being derived is a compile error at the call below rather than at the
/// use site three crates away that actually needed it.
#[test]
fn every_newtype_derives_the_whole_list() {
    fn derived<T: Copy + Clone + Default + Eq + Ord + Hash + Debug + Display>() {}

    derived::<Offset>();
    derived::<ByteLen>();
    derived::<ByteRange>();
    derived::<LineIndex>();
    derived::<ByteColumn>();
    derived::<Utf16Column>();
    derived::<CharCount>();
}

/// §2 names the vocabulary types, and §8 leaves one asymmetry open and
/// explains it: `OffsetUtf16` is used as both a position and a length, so the
/// same argument that splits `Offset` from `ByteLen` would give it a
/// `Utf16Len`. "It does not get one, because UTF-16 quantities exist only at
/// the wire edge and never accumulate anywhere in our code, so the split would
/// buy nothing but would still cost the edits. That is a judgement about where
/// the value is, not a principle, and it should be revisited if UTF-16
/// arithmetic ever appears outside the conversion functions."
///
/// An open judgement is exactly the thing that gets quietly settled by someone
/// adding the type, so the inventory is exact: four units and no fifth, and
/// the eighth type here would be the one that reopened the question.
#[test]
fn the_newtype_modules_define_these_types_and_no_others() {
    let mut defined: Vec<String> = NEWTYPE_MODULES
        .into_iter()
        .flat_map(|file| {
            let text = fs::read_to_string(source(file)).expect("reading a newtype module");
            text.lines()
                .filter_map(|line| {
                    let rest = line.trim().strip_prefix("pub struct ")?;
                    let end = rest.find(|character: char| {
                        !character.is_ascii_alphanumeric() && character != '_'
                    })?;
                    Some(rest[..end].to_owned())
                })
                .collect::<Vec<_>>()
        })
        .collect();
    defined.sort();

    assert_eq!(
        defined,
        [
            "ByteColumn",
            "ByteLen",
            "ByteRange",
            "CharCount",
            "LineIndex",
            "Offset",
            "Point",
            "PointUtf16",
            "Utf16Column"
        ],
        "the vocabulary is four units -- bytes, UTF-16 code units, lines and \
         chars -- in the seven newtypes §2 lists, beside the two point types \
         whose fields they are. A `Utf16Len` here is §8's open asymmetry being \
         settled by whoever added it rather than by the measurement it is \
         waiting on"
    );
}

/// §4's conversion tables, read out of the document and checked against the
/// crate. "**The conversion list is hand-audited, not grepped**" — so the list
/// is a list, and a list in prose is the kind of thing that goes stale without
/// anything happening.
///
/// Two failures this catches that nothing else does. An upstream re-sync
/// renames or drops a function the table names, and the table silently
/// describes a crate that no longer has it — §6 is explicit that signature
/// changes conflict but says nothing about a *rename*, which applies cleanly
/// on one side and leaves the document wrong on the other. And it makes the
/// document the fixture: quietly deleting a row to make the code fit is a
/// failing test rather than an edit nobody sees, which matters because moving
/// the spec toward the code is the one way of faking progress that reading the
/// diff cannot catch.
#[test]
fn every_function_section_4_names_is_still_a_public_function() {
    // The table, transcribed. This is deliberately a second copy: with only a
    // floor on the count, deleting a row to make the code fit still passes,
    // which is the failure this test claims to catch. A row is one claim about
    // one function, so adding or removing one means saying so twice.
    //
    // Deduplicated and in the document's order. `len` is two rows -- `Rope`'s
    // and `ChunkSlice`'s -- and `new`, `seek`, `slice` and `offset` are each
    // several cursors and iterators.
    let expected = [
        "slice",
        "replace",
        "chunks_in_range",
        "bytes_in_range",
        "offset_to_point",
        "offset_to_point_utf16",
        "offset_to_offset_utf16",
        "point_to_offset",
        "point_utf16_to_offset",
        "unclipped_point_utf16_to_offset",
        "offset_utf16_to_offset",
        "clip_offset",
        "is_char_boundary",
        "assert_char_boundary",
        "floor_char_boundary",
        "ceil_char_boundary",
        "len",
        "new",
        "seek_forward",
        "summary",
        "offset",
        "seek",
        "set_range",
        "chars_at",
        "reversed_chars_at",
        "slice_rows",
        "line_len",
        "first_line_chars",
        "last_line_chars",
        "longest_row",
        "last_line_len_utf16",
    ];

    let parsed = functions_named_in_the_signature_tables();
    let mut named: Vec<&str> = parsed.iter().map(String::as_str).collect();
    named.sort_unstable();
    named.dedup();
    let mut expected: Vec<&str> = expected.to_vec();
    expected.sort_unstable();

    assert_eq!(
        named, expected,
        "§4's conversion tables no longer name the functions this test was \
         written against. A row that disappeared is a claim withdrawn without \
         anything noticing, and a row that arrived is a function newly claimed \
         to be converted -- either way, say it here too"
    );

    let sources: String = sources()
        .into_iter()
        .map(|path| fs::read_to_string(path).expect("reading a source file of this crate"))
        .collect();

    let missing: Vec<&&str> = named
        .iter()
        .filter(|name| !sources.contains(&format!("pub fn {name}")))
        .collect();

    assert!(
        missing.is_empty(),
        "`design/rope-modifications.md` §4 names a converted function that is \
         no longer a `pub fn` in this crate. Either a re-sync renamed it -- \
         which applies cleanly and leaves the document describing a crate that \
         does not exist -- or the table was edited to fit the code:\n{missing:?}"
    );
}

/// The control for the parser above. It is the one check here whose fixture is
/// a *document*, so the way it fails silently is by matching nothing at all,
/// and the `>= 30` assertion is only half an answer to that.
#[test]
fn the_table_parser_reads_the_shape_the_document_writes() {
    let planted = "
### The signatures

| Area | Examples |
|---|---|
| Cursors | `Cursor::{new, seek_forward}`, `chars_at`, and the `reversed_*` variants |
| Rows | `line_len(row: LineIndex) -> ByteColumn` |
| `ChunkSlice` | All 27 of its public functions, and a bare-`usize` island |

Not converted, because these are not byte offsets:

| Function | Unit |
|---|---|
| `ChunkSlice::longest_row(&mut total_chars) -> (u32, u32)` | `(LineIndex, CharCount)`; `total_chars` is a `CharCount` |

### The next heading
";

    assert_eq!(
        functions_in_signature_tables(planted),
        vec!["new", "seek_forward", "chars_at", "line_len", "longest_row"],
        "the parser must expand a `Type::{{a, b}}` group, drop the \
         qualification and the parameters, skip a `reversed_*` wildcard and a \
         bare `usize`, and take the *first* column of the not-converted table \
         where it takes the second of the converted one -- since that is where \
         each puts its function names"
    );
}

/// §4: "**The constructors take newtypes.**" The declared bindings above hold
/// that `Point::new(LineIndex(3), ByteColumn(4))` compiles, which is not the
/// same claim — it compiles just as well against the `impl Into<LineIndex>`
/// constructor §4 considers and rejects: "it is a `u32`-shaped hole in the
/// constructor", rejected for the same reason as the lenient operators.
///
/// So the signature is asserted literally. This is the one place where a
/// generic parameter would let bare integers back in without any impl for the
/// other scans to find.
#[test]
fn the_constructors_take_the_newtype_and_not_something_convertible_to_it() {
    let expected = [
        ("point.rs", "(row: LineIndex, column: ByteColumn) -> Self"),
        (
            "point_utf16.rs",
            "(row: LineIndex, column: Utf16Column) -> Self",
        ),
    ];

    for (file, signature) in expected {
        let text = fs::read_to_string(source(file)).expect("reading a newtype module");
        let constructors: Vec<String> = public_signatures(&text)
            .into_iter()
            .filter(|(name, _)| *name == "new")
            .map(|(_, signature)| signature.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();

        assert_eq!(
            constructors,
            vec![signature.to_owned()],
            "{file}'s constructor no longer takes the newtype itself. A generic \
             parameter here is the `u32`-shaped hole `design/rope-modifications.md` \
             §4 rejects, and no impl anywhere would give the other scans \
             something to find"
        );
    }
}

/// §4: `Offset` "gains the two impls that make it usable as a seek dimension,
/// mirroring what `OffsetUtf16` already has", and — the part that matters
/// beyond rope — "**`sum_tree` needs no changes *for this*.** `Dimension` is
/// generic over the summary type, so the impls live in rope."
///
/// Seeking with `Offset` is what proves the impls are there and reachable;
/// `sum_tree` not naming any of rope's types is what proves they did not have
/// to be paid for on the other side.
///
/// *For this* is the whole of it, and is narrower than the sentence this test
/// was written against: `vendor/sum_tree` is patched, just not by this
/// document. [`the_dimension_impls_cost_sum_tree_nothing_but_sum_tree_is_not_pristine`]
/// is that half (CHANGE-core-027).
#[test]
fn offset_seeks_the_rope_and_sum_tree_never_hears_about_it() {
    let rope = Rope::from("aé\nbb\n");
    let mut cursor = rope.cursor(Offset::ZERO);
    let reached: Offset = cursor.summary::<Offset>(Offset(4));
    assert_eq!(
        reached,
        Offset(4),
        "a seek total is a position advanced by a length, which is the one \
         place §4's position/quantity split is crossed on purpose"
    );

    let mentions: Vec<String> = sources_in(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sum_tree")
            .join("src"),
    )
    .into_iter()
    .flat_map(|path| {
        let text = fs::read_to_string(&path).expect("reading a sum_tree source file");
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        text.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| {
                ["Offset", "ByteLen", "ByteRange", "LineIndex", "rope::"]
                    .iter()
                    .any(|name| line.contains(name))
            })
            .map(|line| format!("{file}: {}", line.trim()))
            .collect::<Vec<_>>()
    })
    .collect();

    assert!(
        mentions.is_empty(),
        "`sum_tree` names one of rope's vocabulary types. §4's argument for \
         putting the dimension impls in rope is that `sum_tree` needs no \
         changes *for this*, and this is that claim:\n{}",
        mentions.join("\n")
    );
}

/// The sweep's headline claim is stated twice — once as this document's
/// opening blockquote and once in `core.md#vendoring-the-zed-crates`, which
/// names `rope-modifications.md` as the document to read before touching
/// `vendor/`. Two statements of one claim is two chances to be wrong, and
/// `core.md`'s was: it said these newtypes replace "the bare `u32`s in `Point`,
/// `PointUtf16`, and `TextSummary`", where this document says "the bare
/// *integers*" (CHANGE-core-028).
///
/// The difference is `TextSummary`, whose `len` and `chars` are `usize`
/// upstream and not `u32` — and `chars` is the field whose width the code got
/// wrong for two days in exactly the direction `core.md`'s wording invited.
/// The clause below is the one both documents now share, so a revert of either
/// fires here.
#[test]
fn both_documents_describe_the_newtype_sweep_the_same_way() {
    const SHARED: &str =
        "instead of the bare integers in `Point`, `PointUtf16`, and `TextSummary`";

    for document in ["rope-modifications.md", "core.md"] {
        assert!(
            unwrapped(&design(document)).contains(SHARED),
            "design/{document} no longer says the sweep replaces \"the bare \
             integers\". `TextSummary.len` and `TextSummary.chars` are `usize` \
             upstream, so \"the bare `u32`s\" is false of the type the sweep \
             changes most, and `CharCount` is `usize` for that reason"
        );
    }
}

/// §4's dimension-impls paragraph used to end "`sum_tree` stays a pristine
/// copy, and [section 9]'s claim that it needs no patching survives". Both
/// halves were false (CHANGE-core-027), and the second was false in the way a
/// cross-reference goes false: `core.md#vendoring-the-zed-crates` had already
/// been corrected to say the opposite — in bold, "**`sum_tree` is patched,
/// minimally, and the newtype work is not why**" — and this document went on
/// citing the sentence it used to have. Nothing could say so, because a claim
/// about another document is prose to everything that reads it.
///
/// A quotation is the part of a cross-reference a scan *can* hold, so this
/// holds it both ways round: the sentence §4 quotes has to be one `core.md`
/// still contains, and the patch list that makes it true has to be non-empty
/// and observable in the tree. Checking only the list would pass against a
/// `vendor/README.md` that records a patch nobody applied — which is the
/// failure this campaign found in the entry two headings above it.
#[test]
fn the_dimension_impls_cost_sum_tree_nothing_but_sum_tree_is_not_pristine() {
    let vendored = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("sum_tree");
    assert!(
        !vendored.join("src").join("tree_map.rs").exists(),
        "vendor/README.md's first patch to `sum_tree` is `src/tree_map.rs` \
         deleted, and it is back"
    );
    for file in ["sum_tree.rs", "cursor.rs"] {
        let text = fs::read_to_string(vendored.join("src").join(file))
            .expect("an instrumented sum_tree source");
        assert!(
            text.contains("use tracing::instrument;") && !text.contains("ztracing"),
            "vendor/README.md's second patch rewrites `ztracing::instrument` to \
             `tracing::instrument` in {file}, because `ztracing` is a \
             Zed-internal crate that is not in this workspace"
        );
    }

    let readme = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../README.md"))
        .expect("vendor/README.md, which records every patch");
    let recorded = readme
        .split("## Patches to `sum_tree`")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("vendor/README.md records what was done to `sum_tree`")
        .lines()
        .filter(|line| line.starts_with(|first: char| first.is_ascii_digit()))
        .count();
    assert!(
        recorded >= 3,
        "vendor/README.md lists {recorded} patches to `sum_tree` and the tree \
         shows at least two applied, so the record is short of what was done \
         — which is the direction that makes a re-sync silently drop one"
    );

    // Both documents are hard-wrapped, so the quotation spans a line break in
    // one of them and would span a different one after any reflow. A scan for
    // a sentence has to read the prose the way a reader does.
    const QUOTED: &str = "`sum_tree` is patched, minimally, and the newtype work is not why.";
    assert!(
        unwrapped(&design("core.md")).contains(QUOTED),
        "`core.md` no longer contains the sentence `rope-modifications.md` §4 \
         quotes it for. Requote §4 against whatever it says now — do not delete \
         the quotation, which is what left §4 citing a claim `core.md` had \
         stopped making"
    );
    assert!(
        unwrapped(&design("rope-modifications.md")).contains(QUOTED),
        "§4's dimension-impls paragraph no longer quotes `core.md`'s claim \
         about `sum_tree`, so nothing connects the two documents' accounts of \
         the same crate and they may drift apart again"
    );
}

/// §4's `util` fold-in, in the three parts that are checkable: `vendor/` holds
/// two crates rather than three, each relocated item carries the attribution
/// §4 calls not optional, and `debug_panic!` is the only macro and is not
/// exported.
///
/// The attribution is the part worth a test rather than a review. §4:
/// "each relocated item carries a comment naming its upstream path and
/// original license... The items are trivial enough that whether they are
/// copyrightable at all is arguable; attributing anyway costs a comment and
/// settles the question." A comment is exactly what an edit deletes without
/// anything noticing.
#[test]
fn util_is_folded_in_and_says_where_each_piece_came_from() {
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut crates: Vec<String> = fs::read_dir(&vendor)
        .expect("reading vendor/")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.is_dir())
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    crates.sort();
    assert_eq!(
        crates,
        vec!["rope".to_owned(), "sum_tree".to_owned()],
        "§4: the items move into rope and `vendor/util` does not exist, so \
         `vendor/` drops from three crates to two and `sum_tree` is the only \
         Apache-2.0 input"
    );

    // Each relocated item, at the site §4's table sends it to.
    let attributions = [
        (
            "chunk.rs",
            "is_utf8_char_boundary",
            "crates/util/src/util.rs",
        ),
        ("rope.rs", "debug_panic", "crates/gpui_util/src/lib.rs"),
        (
            "test_support.rs",
            "RandomCharIter",
            "crates/util/src/util.rs",
        ),
    ];
    for (file, item, upstream) in attributions {
        let text = fs::read_to_string(source(file)).expect("reading a source file of this crate");
        assert!(
            text.contains(item) && text.contains(upstream) && text.contains("Apache-2.0"),
            "{file} holds {item}, which came from {upstream} under a different \
             licence than this crate's. §4: attribution is not optional"
        );
    }

    let rope = fs::read_to_string(source("rope.rs")).expect("reading rope.rs");
    assert_eq!(
        rope.matches("macro_rules!").count(),
        1,
        "§8 settles that the conversion is hand-written and not generated: \
         \"a macro over function signatures is unreadable in exactly the code \
         where readability is the only safety argument we have\". `debug_panic!` \
         is the crate's one macro and it generates no signatures"
    );
    // Comments again: the line above `macro_rules! debug_panic` is the one
    // that says it is deliberately not `#[macro_export]`ed, and saying so is
    // not doing it.
    assert!(
        !rope
            .lines()
            .any(|line| !line.trim_start().starts_with("//") && line.contains("#[macro_export]")),
        "`debug_panic!` is deliberately not exported -- §4: `rope::debug_panic!` \
         has no business being in a public API"
    );
}

/// §7's dependency plan, which is a list of versions and so is exactly the
/// kind of claim a manifest can be asked about directly.
///
/// `rand` must be **0.9**: "what Zed pins, and the API the tests are written
/// against (`rng.random_range(..)`). crates.io is at 0.10, and taking it would
/// mean editing test bodies, which defeats the point." And `criterion` is
/// here because §7 keeps the benchmark — "`deps.md` §12 previously declined;
/// the justification now exists".
#[test]
fn the_dependency_plan_is_the_one_section_7_settled() {
    let workspace = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Cargo.toml"),
    )
    .expect("reading the workspace manifest");
    assert!(
        workspace.contains("rand = \"0.9\""),
        "`rand` is pinned to 0.9 rather than crates.io's current major, because \
         0.10 would mean editing the test bodies the sweep is verified by \
         (`design/rope-modifications.md` §7)"
    );

    for crate_name in ["rope", "sum_tree"] {
        let manifest = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(crate_name)
                .join("Cargo.toml"),
        )
        .expect("reading a vendored manifest");
        assert!(
            manifest.contains("[dev-dependencies]") && manifest.contains("rand.workspace = true"),
            "§7: `rand` becomes a dev-dependency of both vendored crates, since \
             `RandomCharIter` needs it and {crate_name}'s randomised tests are \
             kept"
        );
    }

    let rope = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("reading rope's manifest");
    assert!(
        rope.contains("criterion.workspace = true") && rope.contains("name = \"rope_benchmark\""),
        "§7 keeps `benches/rope_benchmark.rs`, which is what answers whether \
         the wrapper indirection costs anything, and taking `criterion` is what \
         that costs"
    );
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("benches")
            .join("rope_benchmark.rs")
            .exists(),
        "the benchmark the manifest declares is the sixth `util` import site \
         and the reason `test_support.rs` is a file rather than an inline \
         module (§4)"
    );
}

/// The function names in §4's two conversion tables, from the document itself.
fn functions_named_in_the_signature_tables() -> Vec<String> {
    let text = design("rope-modifications.md");
    let section = text
        .split("### The signatures")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .expect("§4's signature section");
    functions_in_signature_tables(section)
}

/// Both tables in `section`, as function names. The converted table puts them
/// in its *Examples* column and the not-converted one in its *Function*
/// column, so which cell to read depends on which table the row is in — and
/// the sentence between the two is what separates them.
fn functions_in_signature_tables(section: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut converted = true;

    for line in section.lines() {
        if line.starts_with("Not converted") {
            converted = false;
        }
        let Some(row) = line.strip_prefix('|') else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').collect();
        let Some(cell) = cells.get(usize::from(converted)) else {
            continue;
        };
        for span in cell.split('`').skip(1).step_by(2) {
            names.extend(function_names_in(span));
        }
    }
    names
}

/// The function names in one code span. A span may be a `Type::{a, b}` group,
/// a qualified name, a whole signature, or none of those — `CharCount` is a
/// type and `reversed_*` is a wildcard standing for three functions the table
/// declines to list.
fn function_names_in(span: &str) -> Vec<String> {
    // A code span in these tables that is a bare primitive is the *subject* of
    // the sentence around it -- "a bare-`usize` island inside an otherwise
    // converted crate" -- rather than something to look for a `pub fn` of.
    const NOT_FUNCTIONS: [&str; 2] = ["usize", "u32"];

    let unqualified = span.rsplit("::").next().unwrap_or(span);
    let group = unqualified
        .strip_prefix('{')
        .and_then(|rest| rest.split('}').next());

    group
        .unwrap_or(unqualified)
        .split(',')
        .filter_map(|name| {
            let name = name.trim().split(['(', ' ']).next()?;
            let plausible = !name.is_empty()
                && !NOT_FUNCTIONS.contains(&name)
                && name.starts_with(|character: char| character.is_ascii_lowercase())
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_');
            plausible.then(|| name.to_owned())
        })
        .collect()
}

/// The crates §7 removed, in the order it names them, so a failure reads the
/// way the section does.
const BANISHED: [&str; 4] = ["gpui", "zlog", "ctor", "util"];

/// Every non-comment line in `text` that uses one of the banished crates. A
/// comment naming one is the record of its removal, which is required rather
/// than forbidden.
fn banished_uses(text: &str) -> Vec<&'static str> {
    const USES: [&str; 5] = [
        "use gpui",
        "#[gpui::test",
        "zlog::",
        "ctor::ctor",
        "use util::",
    ];

    let mut used = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        for marker in USES {
            if line.contains(marker) && !used.contains(&marker) {
                used.push(marker);
            }
        }
    }
    used
}

/// Every banished crate `manifest` declares as a dependency. A dependency line
/// starts at the beginning of a line and the crate name is what precedes the
/// `=` or the `.`, which is what keeps `rand.workspace` from reading as one.
fn banished_dependencies(manifest: &str) -> Vec<&'static str> {
    manifest
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let name = line.split(['=', '.']).next()?.trim();
            BANISHED.iter().copied().find(|banished| *banished == name)
        })
        .collect()
}

/// The `_inner` function each `seeded` call runs. `seeded(100,
/// test_random_rope_inner)` takes the function by value, so the name is the
/// second argument and nothing else in the crate has this shape.
fn seeded_call_sites(text: &str) -> Vec<&str> {
    text.match_indices("seeded(")
        .filter_map(|(index, _)| {
            let rest = &text[index + "seeded(".len()..];
            let end = rest.find(')')?;
            let (_, name) = rest[..end].split_once(',')?;
            Some(name.trim())
        })
        .collect()
}

/// The offset one past the last byte of `row`, which is where `line_len`'s
/// column has to land. Computed from the `&str` rather than from the rope, so
/// the comparison above has an oracle that is not the thing under test.
fn line_end(text: &str, row: LineIndex) -> Offset {
    let mut start = 0;
    for (index, line) in text.split('\n').enumerate() {
        if index == row.0 as usize {
            return Offset(start + line.len());
        }
        start += line.len() + 1;
    }
    Offset(text.len())
}

/// Prose with its hard wrapping removed, so that a scan for a sentence finds
/// one that a line break happens to fall inside. Every run of whitespace
/// becomes a single space, which also makes the scan survive a reflow.
///
/// A blockquote's `>` is wrapping too, and is stripped for the same reason: it
/// is per-*line* markup around prose that is one sentence, so a claim stated
/// inside one is otherwise unfindable — which is where `rope-modifications.md`
/// states the sweep's headline claim.
fn unwrapped(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_start().trim_start_matches(['>', ' ']))
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A design document, by file name. `vendor/rope` is two levels below the
/// workspace root, which is the only thing this and [`source`] disagree about.
fn design(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("design")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("reading design/{name}"))
}

fn sources() -> Vec<PathBuf> {
    sources_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
}

fn source(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(name)
}

fn sources_in(directory: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> = fs::read_dir(directory)
        .expect("reading a src directory")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    // Sorted so a failure names the same file first on every run.
    sources.sort();
    assert!(!sources.is_empty(), "no sources found to scan");
    sources
}

fn allowed_primitives() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("allowed-primitives.txt");
    let text = fs::read_to_string(path).expect("reading allowed-primitives.txt");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Every `pub fn` in `text`, as (name, everything from the generics or the
/// parameter list up to the body). Signatures span lines, so this reads to the
/// opening brace or the semicolon rather than to the end of the line.
fn public_signatures(text: &str) -> Vec<(&str, String)> {
    let mut signatures = Vec::new();
    for (index, _) in text.match_indices("pub fn ") {
        let rest = &text[index + "pub fn ".len()..];
        let name_end = rest
            .find(|character: char| !character.is_alphanumeric() && character != '_')
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(name_end);
        let body = tail
            .find(['{', ';'])
            .map_or_else(|| tail.to_owned(), |end| tail[..end].to_owned());
        signatures.push((name, body));
    }
    signatures
}

/// Every named `pub` field declaration in `text`, as `name: type`. A tuple
/// field — `pub struct Offset(pub usize)` — has no name and no colon, and is
/// deliberately not one of these.
fn public_fields(text: &str) -> Vec<String> {
    let mut fields = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub ") else {
            continue;
        };
        let Some((name, kind)) = rest.split_once(':') else {
            continue;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        fields.push(format!("{name}:{}", kind.trim_end_matches(',')));
    }
    fields
}

/// Every operator or conversion impl in `text`, normalized. `Display` and the
/// inherent blocks are not these: an operator is what lets two of these types
/// combine, and a `From` is one by another spelling
/// (`design/rope-modifications.md` §4).
fn operator_impls(text: &str) -> Vec<String> {
    const OPERATORS: [&str; 13] = [
        "Add",
        "Sub",
        "Mul",
        "Div",
        "Rem",
        "Neg",
        "AddAssign",
        "SubAssign",
        "MulAssign",
        "DivAssign",
        "RemAssign",
        "From",
        "Into",
    ];

    impl_headers(text)
        .into_iter()
        .filter(|header| implemented_trait(header).is_some_and(|name| OPERATORS.contains(&name)))
        .collect()
}

/// The trait a normalized `impl` header implements, as its last path segment,
/// or `None` for an inherent block. The header may open with a lifetime or
/// type parameter list, which is not the trait — `impl<'a> Add<&'a Self> for
/// Point` implements `Add`.
///
/// The last segment rather than the whole path, because `impl std::ops::Add
/// for LineIndex` is the same lenient operator as `impl Add for LineIndex` and
/// spelling it out must not be a way past the inventory. That is not
/// hypothetical: written the first way, it walked through this scan.
fn implemented_trait(header: &str) -> Option<&str> {
    if !header.contains(" for ") {
        return None;
    }
    let rest = header.strip_prefix("impl")?;
    let rest = match rest.strip_prefix('<') {
        None => rest,
        Some(parameters) => {
            let mut depth = 1usize;
            let end = parameters.char_indices().find_map(|(index, character)| {
                match character {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                (depth == 0).then_some(index)
            })?;
            &parameters[end + 1..]
        }
    };
    let rest = rest.trim_start();
    let end = rest.find(['<', ' ']).unwrap_or(rest.len());
    let path = &rest[..end];
    Some(path.rsplit("::").next().unwrap_or(path))
}

/// Every `impl` header in `text`, from `impl` to the opening brace. The
/// header is where a lenient operator would appear — `impl Add<u32> for
/// LineIndex` — and stopping at the brace is what keeps `usize::MAX` inside a
/// body from reading as one.
fn impl_headers(text: &str) -> Vec<String> {
    let mut headers = Vec::new();
    for (index, _) in text.match_indices("impl") {
        let before = text[..index].chars().next_back();
        if before.is_some_and(|character| character.is_alphanumeric() || character == '_') {
            continue;
        }
        let rest = &text[index..];
        let Some(end) = rest.find('{') else { continue };
        headers.push(rest[..end].split_whitespace().collect::<Vec<_>>().join(" "));
    }
    headers
}

/// A whole-word `usize` or `u32`, so `Utf16Column` and `to_u32` do not match
/// and `&mut usize` does.
fn mentions_bare_primitive(signature: &str) -> bool {
    ["usize", "u32"].iter().any(|primitive| {
        signature.match_indices(primitive).any(|(index, _)| {
            let before = signature[..index].chars().next_back();
            let after = signature[index + primitive.len()..].chars().next();
            let boundary = |character: Option<char>| {
                character.is_none_or(|character| !character.is_alphanumeric() && character != '_')
            };
            boundary(before) && boundary(after)
        })
    })
}
