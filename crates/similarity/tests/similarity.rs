//! Ported with the crate. The originals were a `#[cfg(test)]` module inside
//! `source.rs`; they live out here because `CLAUDE.md` takes coverage from
//! integration, property and snapshot tests rather than unit tests.
//!
//! The splitting cases are asserted against hashes of the expected parts
//! rather than against the parts themselves, because a part *is* its hash on
//! the far side of `occurrences_in_str` — a single lowercase alphanumeric run
//! hashes to itself, so `["pascal", "case"]` is a legitimate spelling of the
//! expected output without exposing the hasher.

use proptest::prelude::{Strategy, any};
use proptest::{prop_assert, prop_assume, proptest};
use similarity::{
    IdentifierParts, OccurrenceSource, Occurrences, Similarity, SmallOccurrences,
    WeightedSimilarity, namespace_path_similarity,
};
use std::path::Path;

fn parts(text: &str) -> Vec<u32> {
    IdentifierParts::occurrences_in_str(text)
        .map(u32::from)
        .collect()
}

#[track_caller]
fn splits_into(text: &str, expected: &[&str]) {
    let expected: Vec<u32> = expected.iter().flat_map(|part| parts(part)).collect();
    assert_eq!(parts(text), expected, "splitting {text:?}");
}

#[test]
fn identifier_parts_split_on_case_and_separators() {
    splits_into("", &[]);
    splits_into("a", &["a"]);
    splits_into("abc", &["abc"]);
    splits_into("ABC", &["abc"]);
    splits_into("123", &["123"]);
    splits_into("snake_case", &["snake", "case"]);
    splits_into("kebab-case", &["kebab", "case"]);
    splits_into("PascalCase", &["pascal", "case"]);
    splits_into("camelCase", &["camel", "case"]);
    splits_into("XMLParser", &["xml", "parser"]);
    splits_into("a1B2c3", &["a1", "b2c3"]);
}

/// Exact equality is deliberate here and not sloppiness. Every value is a
/// ratio of small integers — 6/13, 7/17, 6/8, 7/10 — and the division is the
/// only float operation, so the result is whatever `f32` rounds that single
/// division to on both sides of the assertion. Comparing approximately would
/// let the counts drift by one without the test noticing, which is the whole
/// thing this example pins.
#[expect(
    clippy::float_cmp,
    reason = "both sides are one rounding of the same division"
)]
#[test]
fn the_metrics_agree_with_a_worked_example() {
    // 10 identifier parts, 8 unique; repeats are 2 "outline" and 2 "items".
    let call_site = "let mut outline_items = query_outline_items(&language, &tree, &source);";
    // 14 identifier parts, 11 unique; repeats are 2 "outline", 2 "language", 2 "tree".
    let declaration = "pub fn query_outline_items(language: &Language, tree: &Tree, source: &str) -> Vec<OutlineItem> {";

    let multiset_a = Occurrences::new(IdentifierParts::occurrences_in_str(call_site));
    let set_a =
        SmallOccurrences::<8, IdentifierParts>::new(IdentifierParts::occurrences_in_str(call_site));
    let set_b = Occurrences::new(IdentifierParts::occurrences_in_str(declaration));

    // 6 overlap: outline, items, query, language, tree, source.
    // 7 do not: let, mut, pub, fn, vec, item, str.
    assert_eq!(multiset_a.jaccard_similarity(&set_b), 6.0 / (6.0 + 7.0));
    assert_eq!(set_a.jaccard_similarity(&set_b), 6.0 / (6.0 + 7.0));

    // One more in the numerator, because both have 2 "outline"; three more in
    // the denominator, from the non-overlapping duplicates.
    assert_eq!(
        multiset_a.weighted_jaccard_similarity(&set_b),
        7.0 / (7.0 + 7.0 + 3.0)
    );

    // Same numerator as Jaccard, over the smaller set's 8 distinct parts.
    assert_eq!(multiset_a.overlap_coefficient(&set_b), 6.0 / 8.0);
    assert_eq!(set_a.overlap_coefficient(&set_b), 6.0 / 8.0);

    // Same numerator as weighted Jaccard, over the smaller set's total weight.
    assert_eq!(multiset_a.weighted_overlap_coefficient(&set_b), 7.0 / 10.0);
}

#[test]
fn a_namespace_scores_against_the_path_that_declares_it() {
    let matching = namespace_path_similarity("foo::bar::Baz", Path::new("src/foo/bar.rs"));
    let unrelated = namespace_path_similarity("foo::bar::Baz", Path::new("src/net/socket.rs"));
    assert!(
        matching > unrelated,
        "expected src/foo/bar.rs to beat src/net/socket.rs, got {matching} and {unrelated}"
    );

    // The extension is excluded, so it cannot be the thing that matches.
    assert!(namespace_path_similarity("rs", Path::new("src/foo.rs")) == 0.0);
}

fn identifier_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..40).prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    /// Jaccard is a ratio of a subset to a superset, so it cannot leave [0, 1]
    /// however the two sides are built. Worth asserting because the union is
    /// computed rather than counted — `|a| + |b| - |intersection|` — and an
    /// intersection over-counted by even one makes it exceed 1.
    #[test]
    fn jaccard_stays_within_zero_and_one(left in identifier_text(), right in identifier_text()) {
        let a = Occurrences::new(IdentifierParts::occurrences_in_str(&left));
        let b = Occurrences::new(IdentifierParts::occurrences_in_str(&right));
        let score = a.jaccard_similarity(&b);
        prop_assert!((0.0..=1.0).contains(&score), "jaccard was {score}");
    }

    #[test]
    fn jaccard_is_symmetric(left in identifier_text(), right in identifier_text()) {
        let a = Occurrences::new(IdentifierParts::occurrences_in_str(&left));
        let b = Occurrences::new(IdentifierParts::occurrences_in_str(&right));
        prop_assert!((a.jaccard_similarity(&b) - b.jaccard_similarity(&a)).abs() < f32::EPSILON);
    }

    /// A non-empty set is identical to itself. Empty scores 0 rather than 1,
    /// which is the deliberate choice in the ported code: an empty candidate
    /// should not look like a perfect match for an empty query.
    #[test]
    fn a_set_matches_itself(text in identifier_text()) {
        let occurrences = Occurrences::new(IdentifierParts::occurrences_in_str(&text));
        prop_assume!(!occurrences.is_empty());
        prop_assert!((occurrences.jaccard_similarity(&occurrences) - 1.0).abs() < f32::EPSILON);
    }
}
