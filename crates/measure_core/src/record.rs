//! `design/core.md` §7's per-query JSONL record — the single record type that
//! covers coverage, precision, error-severity classification, the per-stratum
//! breakdown, latency percentiles and the LSP-latency value weighting.
//!
//! **One shape, two producers.** The shim emits this in the field and `replay`
//! emits it from the frozen truth, and a completed replay row is byte
//! comparable with a field row. That is not a convenience: it is what keeps
//! the measured metric and the shipped metric the same number, and it is why
//! this type is written once here rather than once per consumer.
//!
//! Field order is the declaration order and the declaration order is §7's,
//! because `serde_json` writes a struct in declaration order and the harness
//! diffs these.

use std::collections::BTreeMap;

use serde::Serialize;
use shared::{
    AbstainReason, Agreement, DocumentUri, FileCount, LineIndex, Micros, Offset, StageLabel,
    StageName, Stratum,
};

#[derive(Debug, Serialize)]
pub struct QueryRecord {
    pub uri: Box<str>,
    /// A byte offset, like every other position inside the shim. It is what
    /// `data-collection.md` records and what `replay` joins on, so a
    /// line/column pair here would need a conversion in the one place the two
    /// halves of the metric have to line up exactly.
    pub position: usize,
    pub language: Box<str>,
    pub mode: Mode,
    pub server_health: Option<Box<str>>,
    pub decision: Decision,
    /// The `Error` sub-enum that was converted, and `null` otherwise. The
    /// whole error is deliberately not carried: the class is what a metrics
    /// table can group on, and the detail is already in the log.
    pub failure: Option<Box<str>>,

    /// Coverage is reported on the prior so the denominator is fixed by the
    /// reference and does not move when the implementation changes; precision
    /// on the final so an answer is judged against the class it turned out to
    /// be. One field cannot do both.
    pub stratum_prior: StratumName,
    pub stratum_final: StratumName,
    pub confidence: Option<f32>,
    /// `margin` and `considered` are the features a floor would be set on.
    /// Nothing reads them in v1; a corpus run that kept only the collapsed
    /// confidence could never answer *what would a floor have cost?*
    pub margin: Option<f32>,
    pub considered: Option<u32>,
    /// The handler's own account of what it did, bounded and stable across
    /// runs for the same input — which is what lets failures be *grouped* by
    /// it rather than merely listed.
    pub stages: Vec<Box<str>>,
    pub bytes_scanned: usize,
    pub files_parsed: u32,

    /// Request arrival to dispatch into a worker. Zero in replay, which has no
    /// queue.
    pub queued_us: u64,
    pub stage_us: BTreeMap<Box<str>, u64>,
    pub heuristic_latency_us: u64,
    /// Ordered. `returned` is its length — redundant, and carried anyway,
    /// because computing it by measuring an array length in every consumer is
    /// how a metric acquires two definitions.
    pub heuristic_locations: Vec<Box<str>>,
    pub returned: usize,
    /// Whether the ranked list hit the cap, which is the difference between
    /// "this is everything" and "this is the best of more than we will show" —
    /// and it is what makes `match_contained` mean something weaker.
    pub truncated_list: bool,

    /// A property of the frozen truth, copied through untouched: it describes
    /// how slow the real server was on this repository at this commit, which
    /// is exactly what the value weighting wants.
    pub lsp_latency_us: Option<u64>,
    pub lsp_locations: Option<Vec<Box<str>>>,
    pub agreement: Option<Box<str>>,
    pub severity: Option<Box<str>>,
}

/// `"proxy"` or `"standalone"`. Without this field a mixed log silently
/// pollutes the precision numerator with rows that could never have had an
/// `agreement`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Also replay's, and deliberately: `mode` says whether a second answer
    /// exists to compare against, and replay has one — frozen rather than
    /// raced, which is a fact about *when* it was measured and not about
    /// whether it is there. Writing `standalone` here would null the four
    /// fields replay exists to fill.
    Proxy,
    Standalone,
}

impl Serialize for Mode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Mode::Proxy => "proxy",
            Mode::Standalone => "standalone",
        })
    }
}

/// Three values, not two. On the wire a failure is served as an abstention,
/// because that is what is useful to a user; in the record it must not be one,
/// or the per-stratum table cannot tell a hard stratum from a broken handler.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Decision {
    Committed,
    Abstained,
    Failed,
}

impl Serialize for Decision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Decision::Committed => "committed",
            Decision::Abstained => "abstained",
            Decision::Failed => "failed",
        })
    }
}

/// A `Stratum` as the record spells it. A newtype over the enum rather than a
/// `Serialize` derive on `Stratum` itself, because `Stratum` is inside the
/// seam `state/phase.toml` freezes and a wire spelling is not a reason to
/// reach into it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StratumName(pub Stratum);

impl StratumName {
    pub fn as_str(self) -> &'static str {
        match self.0 {
            Stratum::LocalBinding => "local_binding",
            Stratum::SameFileModule => "same_file_module",
            Stratum::ExplicitImport => "explicitly_imported",
            Stratum::WildcardImport => "wildcard_imported",
            Stratum::AmbiguousName => "ambiguous_name",
            Stratum::ExternalDependency => "external_dependency",
            Stratum::MacroGenerated => "macro_generated",
            Stratum::TypeInferenceRequired => "type_inference_required",
            Stratum::Unimplemented => "unimplemented",
        }
    }
}

impl Serialize for StratumName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The abstention reason, for the digest's `(stratum_prior, reason, stages)`
/// grouping. Carried in `stages` rather than as its own column, because
/// `stages` is the field §7 makes the handler's account of what it did and a
/// second reason column would be two vocabularies for one question.
/// The trailing arm is forced rather than chosen: `AbstainReason` is
/// `#[non_exhaustive]` and this is not `shared`, so an exhaustive match does
/// not compile from here. The rule that a new variant must fail to compile is
/// enforced inside `shared`, where it can be; what this arm has to do instead
/// is make the unlabelled reason *visible* in the metrics rather than folding
/// it into an existing bucket.
pub(crate) fn abstain_label(reason: &AbstainReason) -> Box<str> {
    match reason {
        AbstainReason::NotAnIdentifier => "abstain:not_an_identifier".into(),
        AbstainReason::UnsupportedRole => "abstain:unsupported_role".into(),
        AbstainReason::NoCandidates => "abstain:no_candidates".into(),
        AbstainReason::Deadline => "abstain:deadline".into(),
        AbstainReason::External { name } => format!("abstain:external({name})").into(),
        unlabelled => {
            tracing::warn!(
                ?unlabelled,
                "an abstention reason with no label in the record"
            );
            "abstain:unlabelled".into()
        }
    }
}

/// How a location is spelled in `heuristic_locations` and `lsp_locations`.
///
/// `uri:line`, in byte-space terms nothing has to resolve — which is the same
/// projection `Agreement` compares on, so a row's locations and its
/// `agreement` cannot describe different things.
pub(crate) fn location_label(uri: &DocumentUri, line: LineIndex) -> Box<str> {
    format!("{uri}:{}", line.0).into()
}

pub(crate) fn agreement_labels(agreement: Agreement) -> (Box<str>, Option<Box<str>>) {
    (
        agreement.to_string().into(),
        agreement
            .severity()
            .map(|severity| severity.to_string().into()),
    )
}

/// The handler-reported half of the record — everything from `margin` through
/// `files_parsed`, plus `stage_us` — spelled the way the record writes it.
///
/// A translation and not a second model: `shared::Trace` is the seam's type
/// and these three functions are its JSON. The direction matters, because the
/// alternative is a `Serialize` derive on types inside the seam
/// `state/phase.toml` freezes, and a wire spelling is not a reason to reach
/// into it — the same argument [`StratumName`] makes for `Stratum`.
pub(crate) fn stage_labels(stages: Vec<StageLabel>) -> Vec<Box<str>> {
    stages
        .into_iter()
        .map(|label| label.as_str().into())
        .collect()
}

pub(crate) fn stage_timings(stage_us: BTreeMap<StageName, Micros>) -> BTreeMap<Box<str>, u64> {
    stage_us
        .into_iter()
        .map(|(name, elapsed)| (name.as_str().into(), elapsed.0))
        .collect()
}

/// Saturating rather than fallible: `files_parsed` is a counter nothing gates
/// on, so a repository that somehow parsed four billion files is worth
/// reporting as `u32::MAX` and not worth failing a whole replay over.
pub(crate) fn file_count(files: FileCount) -> u32 {
    u32::try_from(files.0).unwrap_or(u32::MAX)
}

/// The position, spelled the one way `data-collection.md` and §7 agree on.
pub(crate) fn position_of(offset: Offset) -> usize {
    offset.0
}
