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
    let offenders: Vec<String> = ["offset.rs", "point.rs", "point_utf16.rs"]
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
