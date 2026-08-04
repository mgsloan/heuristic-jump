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
//! It lives in `shared` for that reason and not for tidiness. §9's graph gives
//! `driver` no edge to `measure_core`, so a record type in `measure_core`
//! would have made "the driver owns the measurement of every metric in
//! `high-level.md`" unimplementable — and the obvious repair, a second struct
//! on the driver's side, is exactly what "byte comparable" rules out. The
//! *writer* is still `driver`'s (`shim.md` §13's `report/trace.rs`); what is
//! shared is the shape and the assembly.
//!
//! [`Answered`] is where the sharing bites. Turning a `Result<Outcome, Error>`
//! into §7's `decision`/`failure`/`stages` columns is the step where two
//! producers would drift — a driver that recorded a converted failure as an
//! abstention would report a broken handler as a hard stratum, and nothing
//! downstream could tell — so the conversion is written once here and both
//! producers call it.
//!
//! Field order is the declaration order and the declaration order is §7's,
//! because `serde_json` writes a struct in declaration order and the harness
//! diffs these.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;

use crate::proto::DefinitionResult;
use crate::{
    AbstainReason, Agreement, DocumentUri, Error, FileCount, LanguageId, LineIndex, Location,
    Margin, Micros, Offset, Outcome, StageLabel, StageName, Strata, Stratum, Trace, TraceParts,
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

/// Everything about a query that is not the handler's account of it: which
/// query it was, in which mode, and what it cost from the user's point of
/// view.
///
/// Separate from [`Answered`] because the split is §7's own — "everything from
/// `stratum_prior` through `files_parsed` is reported *by the handler*" — and
/// because the two halves have different producers. A replay fills this from a
/// truth row and the driver fills it from a request that arrived.
#[derive(Debug)]
pub struct QueryContext<'a> {
    pub uri: &'a DocumentUri,
    pub position: Offset,
    pub language: LanguageId,
    pub mode: Mode,
    pub server_health: Option<Box<str>>,
    /// Request arrival to dispatch. §5 starts the deadline at arrival rather
    /// than at handler entry, and without this field a handler that started
    /// 200ms late shows up as a fast handler and an unexplained abstention.
    pub queued: Micros,
    /// Dispatch to outcome: the handler's whole cost.
    pub elapsed: Micros,
}

/// The handler's half of §7's record, from one `Result<Outcome, Error>`.
///
/// Public fields because it is §7's columns and nothing more. What it is *for*
/// is that [`Answered::of`] is the only place a dispatch's three endings are
/// classified: a commit, an abstention, and the failure that is served as an
/// abstention on the wire and must never be recorded as one.
#[derive(Debug)]
pub struct Answered {
    pub decision: Decision,
    pub failure: Option<Box<str>>,
    pub strata: Strata,
    pub locations: Vec<Location>,
    pub confidence: Option<f32>,
    pub margin: Option<f32>,
    pub considered: Option<u32>,
    pub stages: Vec<Box<str>>,
    pub bytes_scanned: usize,
    pub files_parsed: u32,
    pub stage_us: BTreeMap<Box<str>, u64>,
}

impl Answered {
    /// The outcome is consumed rather than borrowed: a [`Trace`] is write-only
    /// until it is taken apart, and taking it apart is what `into_parts` does —
    /// so a record can only be assembled from an outcome nobody is going to
    /// read again.
    pub fn of(answered: Result<Outcome, Error>) -> Self {
        let (decision, failure, strata, locations, confidence, parts, extra) = match answered {
            Ok(Outcome::Committed {
                locations,
                confidence,
                strata,
                trace,
            }) => (
                Decision::Committed,
                None,
                strata,
                locations,
                Some(confidence.get()),
                trace.into_parts(),
                None,
            ),
            Ok(Outcome::Abstain {
                reason,
                strata,
                trace,
            }) => (
                Decision::Abstained,
                None,
                strata,
                Vec::new(),
                None,
                trace.into_parts(),
                // The reason goes into `stages` rather than into a column of
                // its own, because `stages` is the field §7 makes the
                // handler's account of what it did and a second reason column
                // would be two vocabularies for one question.
                Some(abstain_label(&reason)),
            ),
            // A failure is served as an abstention on the wire and recorded as
            // a failure here, or the per-stratum table cannot tell a hard
            // stratum from a broken handler. There is no outcome and therefore
            // no trace: a handler that returned `Err` reported nothing, and an
            // empty account is the honest record of that.
            Err(error) => (
                Decision::Failed,
                Some(failure_class(&error)),
                Strata::from_reference(Stratum::Unimplemented),
                Vec::new(),
                None,
                Trace::new().into_parts(),
                None,
            ),
        };
        let TraceParts {
            stages,
            stage_us,
            bytes_scanned,
            files_parsed,
            margin,
            considered,
        } = parts;
        let mut stages = stage_labels(stages);
        stages.extend(extra);

        Self {
            decision,
            failure,
            strata,
            locations,
            confidence,
            margin: margin.map(Margin::get),
            considered: considered.map(|considered| considered.0),
            stages,
            bytes_scanned: bytes_scanned.0,
            files_parsed: file_count(files_parsed),
            stage_us: stage_timings(stage_us),
        }
    }
}

/// The oracle's half, once it is known.
///
/// `agreement` is an `Option` for §6's reason and not for convenience: a query
/// the shim never answered has no answer of ours to compare, which is a
/// different fact from the two sides disagreeing.
#[derive(Debug)]
pub struct ChildAnswer {
    pub latency: Micros,
    pub locations: Vec<Box<str>>,
    pub agreement: Option<Agreement>,
}

impl QueryRecord {
    /// Everything both producers know at the moment the handler returns. The
    /// four oracle columns are left `null`, which is already the right answer
    /// in standalone mode — there is no second answer to compare against —
    /// and is filled in by [`QueryRecord::answered_by`] otherwise.
    pub fn new(context: &QueryContext<'_>, answered: Answered) -> Self {
        Self {
            uri: context.uri.to_string().into(),
            position: position_of(context.position),
            language: context.language.as_str().into(),
            mode: context.mode,
            server_health: context.server_health.clone(),
            decision: answered.decision,
            failure: answered.failure,
            stratum_prior: StratumName(answered.strata.prior()),
            stratum_final: StratumName(answered.strata.settled()),
            confidence: answered.confidence,
            margin: answered.margin,
            considered: answered.considered,
            stages: answered.stages,
            bytes_scanned: answered.bytes_scanned,
            files_parsed: answered.files_parsed,
            queued_us: context.queued.0,
            stage_us: answered.stage_us,
            heuristic_latency_us: context.elapsed.0,
            heuristic_locations: answered.locations.iter().map(location_of).collect(),
            returned: answered.locations.len(),
            // `resolution.md`'s ranked list has no cap yet, so nothing can hit
            // one. It is written as a value rather than left out because the
            // day a cap appears, a `false` here is a claim somebody has to
            // come back and correct.
            truncated_list: false,
            lsp_latency_us: None,
            lsp_locations: None,
            agreement: None,
            severity: None,
        }
    }

    /// The four columns that need the oracle. Splitting them out is what §7's
    /// "once both answers are known" means for a shim that answers first: the
    /// row exists from the moment the handler returns, and the query is not
    /// finished until this has been called.
    pub fn answered_by(&mut self, child: ChildAnswer) {
        self.lsp_latency_us = Some(child.latency.0);
        self.lsp_locations = Some(child.locations);
        let (agreement, severity) = match child.agreement {
            Some(agreement) => agreement_labels(agreement),
            None => (None, None),
        };
        self.agreement = agreement;
        self.severity = severity;
    }
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

/// The `Error` sub-enum that was converted. Written as an exhaustive match
/// rather than a `Display` string, so a new sub-enum has to be given a name
/// here instead of appearing in the metrics as whatever `thiserror` produced.
fn failure_class(error: &Error) -> Box<str> {
    match error {
        Error::Config(_) => "Config".into(),
        Error::Codec(_) => "Codec".into(),
        Error::Child(_) => "Child".into(),
        Error::Protocol(_) => "Protocol".into(),
        Error::Document(_) => "Document".into(),
        Error::Parse(_) => "Parse".into(),
        Error::Project(_) => "Project".into(),
        Error::Handler(_) => "Handler".into(),
        Error::Encoding(_) => "Encoding".into(),
    }
}

/// The abstention reason, for the digest's `(stratum_prior, reason, stages)`
/// grouping. Carried in `stages` rather than as its own column, because
/// `stages` is the field §7 makes the handler's account of what it did and a
/// second reason column would be two vocabularies for one question.
///
/// Exhaustive, which it can be here and could not be in `measure_core`:
/// `AbstainReason` is `#[non_exhaustive]`, so a match from outside `shared`
/// needs a trailing arm and a new variant reaches the metrics as whatever that
/// arm says. Here a new one fails to compile until somebody spells it, which
/// is the rule `deps.md` §10 asks for and the one thing the move buys that
/// tidiness would not.
pub fn abstain_label(reason: &AbstainReason) -> Box<str> {
    match reason {
        AbstainReason::NotAnIdentifier => "abstain:not_an_identifier".into(),
        AbstainReason::UnsupportedRole => "abstain:unsupported_role".into(),
        AbstainReason::NoCandidates => "abstain:no_candidates".into(),
        AbstainReason::Deadline => "abstain:deadline".into(),
        AbstainReason::External { name } => format!("abstain:external({name})").into(),
    }
}

/// How a location is spelled in `heuristic_locations` and `lsp_locations`.
///
/// `uri:line`, in byte-space terms nothing has to resolve — which is the same
/// projection `Agreement` compares on, so a row's locations and its
/// `agreement` cannot describe different things.
pub fn location_label(uri: &DocumentUri, line: LineIndex) -> Box<str> {
    format!("{uri}:{}", line.0).into()
}

fn location_of(location: &Location) -> Box<str> {
    location_label(location.uri(), location.line())
}

/// The oracle's locations, in the same spelling, from whichever of the four
/// shapes `textDocument/definition` came back as. Here rather than beside
/// either producer because a driver that spelled the child's side differently
/// from the replay's would make the two runs' rows incomparable in exactly the
/// column the comparison is about.
pub fn definition_labels(child: &DefinitionResult) -> Vec<Box<str>> {
    match child {
        DefinitionResult::Null => Vec::new(),
        DefinitionResult::One(location) => {
            vec![location_label(
                location.uri(),
                location.range().start.line(),
            )]
        }
        DefinitionResult::Many(locations) => locations
            .iter()
            .map(|location| location_label(location.uri(), location.range().start.line()))
            .collect(),
        DefinitionResult::Links(links) => links
            .iter()
            .map(|link| location_label(&link.target_uri, link.target_selection_range.start.line()))
            .collect(),
    }
}

fn agreement_labels(agreement: Agreement) -> (Option<Box<str>>, Option<Box<str>>) {
    (
        Some(agreement.to_string().into()),
        agreement
            .severity()
            .map(|severity| severity.to_string().into()),
    )
}

/// The handler-reported half of the record — everything from `margin` through
/// `files_parsed`, plus `stage_us` — spelled the way the record writes it.
///
/// A translation and not a second model: `Trace` is the seam's type and these
/// functions are its JSON. The direction matters, because the alternative is a
/// `Serialize` derive on types inside the seam `state/phase.toml` freezes, and
/// a wire spelling is not a reason to reach into it — the same argument
/// [`StratumName`] makes for `Stratum`.
fn stage_labels(stages: Vec<StageLabel>) -> Vec<Box<str>> {
    stages
        .into_iter()
        .map(|label| label.as_str().into())
        .collect()
}

fn stage_timings(stage_us: BTreeMap<StageName, Micros>) -> BTreeMap<Box<str>, u64> {
    stage_us
        .into_iter()
        .map(|(name, elapsed)| (name.as_str().into(), elapsed.0))
        .collect()
}

/// Saturating rather than fallible: `files_parsed` is a counter nothing gates
/// on, so a repository that somehow parsed four billion files is worth
/// reporting as `u32::MAX` and not worth failing a whole replay over.
fn file_count(files: FileCount) -> u32 {
    u32::try_from(files.0).unwrap_or(u32::MAX)
}

/// The position, spelled the one way `data-collection.md` and §7 agree on.
fn position_of(offset: Offset) -> usize {
    offset.0
}

/// Saturating for the same reason [`file_count`] is: a latency too large to
/// represent is worth reporting as the largest representable one, and is not
/// worth failing a query over.
pub fn micros(elapsed: Duration) -> Micros {
    Micros(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX))
}
