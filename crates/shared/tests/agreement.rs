//! `design/core.md` §6, which is not a reporting detail: this predicate *is*
//! how precision is measured, so every row of its table and both halves of its
//! set lift are asserted here rather than read.
//!
//! The child's side is deserialized from JSON rather than constructed, because
//! that is the only way `LocationLink` arrives — nothing in the workspace
//! builds one — and because it keeps §8.5's untagged union on the path this
//! test exercises.

use rope::LineIndex;
use serde_json::json;
use shared::proto::DefinitionResult;
use shared::{Agreement, DefinitionSite, DocumentUri, Severity};

const MAIN: &str = "file:///work/repo/src/main.rs";
const SIBLING: &str = "file:///work/repo/src/parser.rs";
const FAR: &str = "file:///work/repo/tests/golden/fixture.rs";

/// One `Location` on the wire, at `line`. The column is deliberately varied by
/// the callers that care; the predicate never reads it.
///
/// A macro rather than a function because a function would have to *name*
/// `serde_json::Value`, which `clippy.toml` bans workspace-wide: a forwarded
/// frame must never be materialized into a tree, and the ban is what keeps
/// that true. `json!` builds one without naming it, which is how the rest of
/// the suite already works.
macro_rules! at {
    ($uri:expr, $line:expr) => {
        json!({
            "uri": $uri,
            "range": {
                "start": {"line": $line, "character": 4},
                "end": {"line": $line, "character": 9}
            }
        })
    };
}

#[test]
fn the_strings_are_the_ones_the_record_carries() {
    // §7's `agreement` and `severity` fields, spelled exactly. The classifier
    // and the metric must not acquire separate vocabularies, which is only
    // true while these are the only place either string is written.
    assert_eq!(Agreement::MatchTop1.to_string(), "match_top1");
    assert_eq!(Agreement::MatchContained.to_string(), "match_contained");
    assert_eq!(
        Agreement::Mismatch {
            severity: Severity::SameFile
        }
        .to_string(),
        "mismatch"
    );
    assert_eq!(Severity::SameFile.to_string(), "same_file");
    assert_eq!(Severity::NearModule.to_string(), "near_module");
    assert_eq!(Severity::Unrelated.to_string(), "unrelated");

    // `severity` is `null` unless `agreement` is `mismatch`, and it is carried
    // inside the variant so that is a fact about the type.
    assert_eq!(Agreement::MatchTop1.severity(), None);
    assert_eq!(Agreement::MatchContained.severity(), None);
    assert_eq!(
        Agreement::Mismatch {
            severity: Severity::NearModule
        }
        .severity(),
        Some(Severity::NearModule)
    );
}

#[test]
fn three_lines_is_a_match_and_four_is_not() {
    let uri = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&uri, LineIndex(40))];

    for line in [37, 40, 43] {
        let child: DefinitionResult = serde_json::from_value(at!(MAIN, line)).unwrap();
        assert_eq!(
            Agreement::classify(&ours, &child),
            Agreement::MatchTop1,
            "{line} is within the tolerance"
        );
    }

    for line in [36, 44] {
        let child: DefinitionResult = serde_json::from_value(at!(MAIN, line)).unwrap();
        assert_eq!(
            Agreement::classify(&ours, &child),
            Agreement::Mismatch {
                severity: Severity::SameFile
            },
            "{line} is outside it"
        );
    }
}

#[test]
fn columns_are_never_compared() {
    let uri = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&uri, LineIndex(12))];
    // The shim landed on the `fn` keyword and the server on an identifier
    // forty columns along. §6 says that is a match, because the row it would
    // decide is subsumed by the three-line tolerance above it.
    let child: DefinitionResult = serde_json::from_value(json!({
        "uri": MAIN,
        "range": {
            "start": {"line": 12, "character": 60},
            "end": {"line": 12, "character": 71}
        }
    }))
    .unwrap();

    assert_eq!(Agreement::classify(&ours, &child), Agreement::MatchTop1);
}

#[test]
fn all_four_shapes_normalise_to_the_same_verdict() {
    let uri = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&uri, LineIndex(40))];

    let one: DefinitionResult = serde_json::from_value(at!(MAIN, 41)).unwrap();
    let many: DefinitionResult = serde_json::from_value(json!([at!(MAIN, 41)])).unwrap();
    let links: DefinitionResult = serde_json::from_value(json!([{
        "targetUri": MAIN,
        "targetRange": {
            "start": {"line": 38, "character": 0},
            "end": {"line": 44, "character": 1}
        },
        "targetSelectionRange": {
            "start": {"line": 41, "character": 3},
            "end": {"line": 41, "character": 7}
        }
    }]))
    .unwrap();

    for child in [one, many, links] {
        assert_eq!(Agreement::classify(&ours, &child), Agreement::MatchTop1);
    }
}

#[test]
fn a_link_is_read_at_its_selection_range_not_its_whole_item() {
    let uri = DocumentUri::parse(MAIN).unwrap();
    // The item spans lines 10..30 and its name is on line 29. Taking
    // `targetRange` would put the child eighteen lines away and score a
    // mismatch; §6 takes `targetSelectionRange`.
    let ours = [DefinitionSite::new(&uri, LineIndex(29))];
    let child: DefinitionResult = serde_json::from_value(json!([{
        "targetUri": MAIN,
        "targetRange": {
            "start": {"line": 10, "character": 0},
            "end": {"line": 30, "character": 1}
        },
        "targetSelectionRange": {
            "start": {"line": 29, "character": 8},
            "end": {"line": 29, "character": 12}
        }
    }]))
    .unwrap();

    assert_eq!(Agreement::classify(&ours, &child), Agreement::MatchTop1);
}

#[test]
fn matching_any_of_the_childs_answers_is_a_match() {
    let uri = DocumentUri::parse(SIBLING).unwrap();
    let ours = [DefinitionSite::new(&uri, LineIndex(200))];
    // The server is expressing ambiguity. Picking one of its own candidates is
    // not an error.
    let child: DefinitionResult =
        serde_json::from_value(json!([at!(MAIN, 10), at!(SIBLING, 201), at!(FAR, 3),])).unwrap();

    assert_eq!(Agreement::classify(&ours, &child), Agreement::MatchTop1);
}

#[test]
fn a_later_location_matching_is_contained_and_not_top1() {
    let main = DocumentUri::parse(MAIN).unwrap();
    let sibling = DocumentUri::parse(SIBLING).unwrap();
    let ours = [
        DefinitionSite::new(&main, LineIndex(4)),
        DefinitionSite::new(&sibling, LineIndex(88)),
    ];
    let child: DefinitionResult = serde_json::from_value(at!(SIBLING, 88)).unwrap();

    assert_eq!(
        Agreement::classify(&ours, &child),
        Agreement::MatchContained
    );
}

#[test]
fn top1_cannot_be_improved_by_returning_more() {
    // The flaw §6 rejects, checked directly: appending candidates must never
    // turn a mismatch into `match_top1`, or the number that gets optimised
    // improves monotonically with list length.
    let main = DocumentUri::parse(MAIN).unwrap();
    let sibling = DocumentUri::parse(SIBLING).unwrap();
    let far = DocumentUri::parse(FAR).unwrap();
    let child: DefinitionResult = serde_json::from_value(at!(SIBLING, 88)).unwrap();

    let alone = [DefinitionSite::new(&main, LineIndex(4))];
    assert_eq!(
        Agreement::classify(&alone, &child),
        Agreement::Mismatch {
            severity: Severity::NearModule
        }
    );

    let padded = [
        DefinitionSite::new(&main, LineIndex(4)),
        DefinitionSite::new(&far, LineIndex(1)),
        DefinitionSite::new(&sibling, LineIndex(88)),
    ];
    assert_eq!(
        Agreement::classify(&padded, &child),
        Agreement::MatchContained
    );
}

#[test]
fn severity_walks_the_table() {
    let main = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&main, LineIndex(40))];

    let same_file: DefinitionResult = serde_json::from_value(at!(MAIN, 90)).unwrap();
    assert_eq!(
        Agreement::classify(&ours, &same_file).severity(),
        Some(Severity::SameFile)
    );

    // `src/main.rs` against `src/parser.rs`: a different file, one directory.
    let near: DefinitionResult = serde_json::from_value(at!(SIBLING, 90)).unwrap();
    assert_eq!(
        Agreement::classify(&ours, &near).severity(),
        Some(Severity::NearModule)
    );

    // `src/` against `tests/golden/`: the trust-destroying class.
    let unrelated: DefinitionResult = serde_json::from_value(at!(FAR, 90)).unwrap();
    assert_eq!(
        Agreement::classify(&ours, &unrelated).severity(),
        Some(Severity::Unrelated)
    );
}

#[test]
fn severity_is_the_mildest_class_over_the_childs_set() {
    let main = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&main, LineIndex(40))];
    // The unrelated answer is listed first. The shim is charged for the
    // server's mildest candidate, not its least convenient one.
    let child: DefinitionResult =
        serde_json::from_value(json!([at!(FAR, 3), at!(SIBLING, 7), at!(MAIN, 90)])).unwrap();

    assert_eq!(
        Agreement::classify(&ours, &child).severity(),
        Some(Severity::SameFile)
    );
}

#[test]
fn severity_is_classified_from_the_top_ranked_location_only() {
    let main = DocumentUri::parse(MAIN).unwrap();
    let far = DocumentUri::parse(FAR).unwrap();
    // Our second answer is in the child's file; our first is not. §6 reads the
    // severity off the top-ranked one, because that is where a user who trusts
    // the ordering looks first.
    let ours = [
        DefinitionSite::new(&far, LineIndex(1)),
        DefinitionSite::new(&main, LineIndex(500)),
    ];
    let child: DefinitionResult = serde_json::from_value(at!(MAIN, 90)).unwrap();

    assert_eq!(
        Agreement::classify(&ours, &child).severity(),
        Some(Severity::Unrelated)
    );
}

#[test]
fn a_child_with_no_answer_against_a_commit_is_unrelated() {
    let main = DocumentUri::parse(MAIN).unwrap();
    let ours = [DefinitionSite::new(&main, LineIndex(40))];

    let null: DefinitionResult = serde_json::from_value(json!(null)).unwrap();
    let empty: DefinitionResult = serde_json::from_value(json!([])).unwrap();
    for child in [null, empty] {
        assert_eq!(
            Agreement::classify(&ours, &child),
            Agreement::Mismatch {
                severity: Severity::Unrelated
            }
        );
    }
}

#[test]
fn both_empty_is_a_match_and_one_sided_emptiness_is_not() {
    let null: DefinitionResult = serde_json::from_value(json!(null)).unwrap();
    assert_eq!(Agreement::classify(&[], &null), Agreement::MatchTop1);

    // CHANGE-conformance-006: the row §6's table does not have. There is no
    // top-ranked location to classify from, so it takes the pessimistic class.
    let child: DefinitionResult = serde_json::from_value(at!(MAIN, 90)).unwrap();
    assert_eq!(
        Agreement::classify(&[], &child),
        Agreement::Mismatch {
            severity: Severity::Unrelated
        }
    );
}
