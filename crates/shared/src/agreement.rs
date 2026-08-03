//! The agreement predicate (`design/core.md` §6): the definition of
//! "different" that both the shipped divergence report and the corpus metric
//! read from.
//!
//! It lives here, and [`Agreement::classify`] is the only way in, because §6
//! says this must not fork between the two — `measure_core` does not depend on
//! `driver` (§9's dependency graph), so a predicate in `driver` would mean the
//! corpus scores a tool that is not the one that ships.
//!
//! **It reads nothing.** Divergence is classified when the child responds,
//! seconds after the answer, when the per-query read cache is gone and the
//! target document may never have been open. That is why the comparison is
//! `(uri, line)` and why normalisation stops at a row rather than reaching
//! byte space: a wire range's `character` is in the negotiated encoding, and
//! resolving one needs the document's text
//! (`state/spec-changelog.md`, CHANGE-conformance-005).

use core::fmt;

use rope::LineIndex;

use crate::proto::DefinitionResult;
use crate::vocabulary::{DocumentUri, Location};

/// §6: at this distance the correct definition is on screen and the user is
/// already reading it, so scoring it as wrong would measure something nobody
/// experiences as wrong.
const LINE_TOLERANCE: u32 = 3;

/// One row of either side, after normalisation — the whole of what the
/// predicate compares.
///
/// Public, and the shim's side is a slice of these rather than of
/// [`Location`], for two reasons. It is what §6 says both sides collapse to
/// once byte space is out of reach (CHANGE-conformance-005), so the type names
/// the normalised form rather than hiding it. And `Location`'s only
/// constructor is `Location::at_node`, which needs a `tree_sitter::Node` and
/// therefore a grammar crate that is not in the workspace yet — so a predicate
/// taking `&[Location]` could not be tested at all until `lang_rust` lands,
/// and §6 is the one place in this document where an untested reading corrupts
/// the numbers a precision floor would later be derived from.
///
/// This is a projection, not a second predicate: the tolerance, the severity
/// table and the set lift all stay in [`Agreement::classify`], which is still
/// the only public function here that decides anything.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DefinitionSite<'a> {
    uri: &'a DocumentUri,
    line: LineIndex,
}

impl<'a> DefinitionSite<'a> {
    /// The shim's side. `line` is redundant with `range` and carried anyway,
    /// which is exactly what saves this from needing a whole-file line index
    /// at classification time (`core.md` §8.4).
    pub fn of(location: &'a Location) -> Self {
        Self {
            uri: location.uri(),
            line: location.line(),
        }
    }

    /// The child's side, and the corpus's: a row that arrived over the wire,
    /// or was recorded from one.
    pub fn new(uri: &'a DocumentUri, line: LineIndex) -> Self {
        Self { uri, line }
    }
}

/// How bad a divergence was, and — through `Display` — the exact string §7's
/// record carries in its `severity` field. One vocabulary, so the number that
/// ships and the number that gets measured cannot come apart.
///
/// The derived order is the severity order: `high-level.md` attaches a
/// separate budget to each of the last two, so `SameFile < NearModule <
/// Unrelated` is load-bearing rather than incidental — [`Agreement::classify`]
/// takes the minimum over the child's set.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity {
    SameFile,
    NearModule,
    Unrelated,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Severity::SameFile => "same_file",
            Severity::NearModule => "near_module",
            Severity::Unrelated => "unrelated",
        })
    }
}

/// The three values §6 permits, and `Display` is again §7's field. A bare
/// `match` is deliberately not one of them.
///
/// `severity` sits *inside* `Mismatch` rather than beside the enum, so §6's
/// "undefined otherwise" is unrepresentable instead of asserted, and so
/// [`Agreement::severity`] can hand §7 the `null` it writes on the other two
/// arms without anyone having to remember the rule.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Agreement {
    /// The shim's *first* location matched. Cannot be improved by returning
    /// more, which is why it is the number that gets optimised.
    MatchTop1,
    /// Some later location matched. What the user could have reached through
    /// the picker, and meaningful only beside the result count.
    MatchContained,
    Mismatch {
        severity: Severity,
    },
}

impl fmt::Display for Agreement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Agreement::MatchTop1 => "match_top1",
            Agreement::MatchContained => "match_contained",
            Agreement::Mismatch { .. } => "mismatch",
        })
    }
}

impl Agreement {
    /// `shim` is the ranked list the shim committed, order load-bearing;
    /// `child` is the oracle's `textDocument/definition` answer in whichever
    /// of the four shapes it arrived in.
    ///
    /// Both empty is a match: the two ends agree there is no definition here.
    pub fn classify(shim: &[DefinitionSite<'_>], child: &DefinitionResult) -> Self {
        let theirs = normalize(child);

        let Some((top, rest)) = shim.split_first() else {
            // The shim committed nothing. Either the child did too, and the
            // two agree, or it did not — and there is then no top-ranked
            // location to classify a severity from, which is the row §6's
            // table does not have. CHANGE-conformance-006 fills it with the
            // pessimistic class rather than a fourth one.
            return if theirs.is_empty() {
                Agreement::MatchTop1
            } else {
                Agreement::Mismatch {
                    severity: Severity::Unrelated,
                }
            };
        };

        if theirs.is_empty() {
            // §6's table, row 5: the child answered null or empty and the shim
            // committed anyway.
            return Agreement::Mismatch {
                severity: Severity::Unrelated,
            };
        }

        if theirs.iter().any(|theirs| matches(*top, *theirs)) {
            return Agreement::MatchTop1;
        }
        if rest
            .iter()
            .any(|ours| theirs.iter().any(|theirs| matches(*ours, *theirs)))
        {
            return Agreement::MatchContained;
        }

        // Severity is classified from the shim's top-ranked location, since
        // that is where a user who trusts the ordering looks first — against
        // the child's *whole* set, taking the mildest. Several answers are the
        // LSP expressing ambiguity, and §6 already refuses to call picking one
        // of its own candidates an error; the same reasoning refuses to charge
        // the shim for the child's least convenient one.
        // `conformance-009` (answered).
        let severity = theirs
            .iter()
            .map(|theirs| severity_of(*top, *theirs))
            .min()
            .unwrap_or(Severity::Unrelated);
        Agreement::Mismatch { severity }
    }

    /// What §7's record writes: the class on a mismatch, `null` otherwise.
    pub fn severity(self) -> Option<Severity> {
        match self {
            Agreement::MatchTop1 | Agreement::MatchContained => None,
            Agreement::Mismatch { severity } => Some(severity),
        }
    }
}

/// §6's pairwise row 1: same file, within the tolerance. Columns are
/// deliberately not compared — the tolerance already grants three lines, and a
/// column test would be a stricter check nested inside a looser one.
fn matches(shim: DefinitionSite<'_>, child: DefinitionSite<'_>) -> bool {
    shim.uri == child.uri && shim.line.0.abs_diff(child.line.0) <= LINE_TOLERANCE
}

/// Rows 2 to 4 of the same table, for a pair already known to differ.
fn severity_of(shim: DefinitionSite<'_>, child: DefinitionSite<'_>) -> Severity {
    if shim.uri == child.uri {
        Severity::SameFile
    } else if same_module_tree(shim.uri, child.uri) {
        Severity::NearModule
    } else {
        Severity::Unrelated
    }
}

/// `conformance-009` (answered). "Same module tree" is read as "same
/// containing directory", which is the strongest test available to something
/// that may not read the disk and does not know the language.
///
/// On the URI text rather than through a `PathBuf`, so there is no allocation
/// and no platform-specific path parsing: two URIs share a directory exactly
/// when they share everything up to their last `/`, and normalisation already
/// happened when the [`DocumentUri`] was built (`core.md` §8.1).
fn same_module_tree(shim: &DocumentUri, child: &DocumentUri) -> bool {
    match (parent(shim.as_str()), parent(child.as_str())) {
        (Some(shim), Some(child)) => shim == child,
        // A URI with no `/` in it is not a document either side found by
        // walking a project, so there is nothing for it to be near.
        (None, _) | (_, None) => false,
    }
}

fn parent(uri: &str) -> Option<&str> {
    uri.rsplit_once('/').map(|(parent, _)| parent)
}

/// §6: all four shapes collapse to the pairs the predicate compares, taking
/// `targetSelectionRange` for links — that is the identifier a link points at,
/// where `targetRange` is the whole item and would put the row on the doc
/// comment above it.
fn normalize(child: &DefinitionResult) -> Vec<DefinitionSite<'_>> {
    match child {
        DefinitionResult::Null => Vec::new(),
        DefinitionResult::One(location) => vec![DefinitionSite::new(
            location.uri(),
            location.range().start.line(),
        )],
        DefinitionResult::Many(locations) => locations
            .iter()
            .map(|location| DefinitionSite::new(location.uri(), location.range().start.line()))
            .collect(),
        DefinitionResult::Links(links) => links
            .iter()
            .map(|link| {
                DefinitionSite::new(&link.target_uri, link.target_selection_range.start.line())
            })
            .collect(),
    }
}
