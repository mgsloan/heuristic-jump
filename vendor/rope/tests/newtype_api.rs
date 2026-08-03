//! Ours, not upstream's: the two checks `design/rope-modifications.md` §6 and
//! §7 ask for on top of the tests the vendored copy already keeps.
//!
//! §3 is explicit that a mechanical diff check can no longer prove the newtype
//! sweep correct, so upstream's own tests are the verification and these two
//! are the complement. They cover the one thing upstream's cannot: that the
//! *API* still names its units, and that the four units agree with each other
//! at the level where a conversion bug originates rather than one layer up in
//! `shared::proto`.

use std::fs;
use std::path::{Path, PathBuf};

use rand::prelude::*;
use rope::{ByteLen, LineIndex, Offset, Rope};

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
    for seed in 0..64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let length = rng.random_range(0..512);
        let text: String = RandomCharIter::new(&mut rng).take(length).collect();
        let rope = Rope::from(text.as_str());

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

fn sources() -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources: Vec<PathBuf> = fs::read_dir(&directory)
        .expect("reading this crate's src directory")
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
        .filter(|header| {
            implemented_trait(header).is_some_and(|name| OPERATORS.contains(&name))
        })
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
                character.is_none_or(|character| {
                    !character.is_alphanumeric() && character != '_'
                })
            };
            boundary(before) && boundary(after)
        })
    })
}
