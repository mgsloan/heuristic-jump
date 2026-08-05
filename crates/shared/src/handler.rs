//! `design/core.md` §1: the seam this project commits to, and everything
//! behind it is out of scope here. Dispatch is direct — no framework, and no
//! config format that languages have to be expressed in.
//!
//! It lives in `shared` rather than in `driver` because `measure_core` calls
//! it a whole phase before a shim exists (`core.md` §9's dependency graph), so
//! a language can be measured before there is anything to proxy.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use tree_sitter::Language;

use crate::deadline::Deadline;
use crate::document::DocumentSnapshot;
use crate::error::Error;
use crate::project::{FileCount, ProjectView};
use crate::vocabulary::{Confidence, FileExtension, LanguageId, Location, ServerId};
use rope::{ByteLen, Offset};

pub trait LanguageHandler: Send + Sync {
    /// LSP `languageId` values, for open documents.
    fn language_ids(&self) -> &'static [LanguageId];

    /// File extensions, for candidate files found by search. Closed files
    /// arrive as a bare path with no `languageId` attached.
    fn file_extensions(&self) -> &'static [FileExtension];

    /// The tree-sitter grammar, supplied at runtime so that `driver` can
    /// maintain its parse cache without depending on any grammar crate. This
    /// method is the whole reason that is possible.
    fn grammar(&self) -> Language;

    /// `Err` is a *failure*, never a decision. Abstention lives in `Outcome`.
    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error>;
}

/// Handlers are `Send + Sync` and re-entrant: the same handler serves
/// concurrent queries, and per-query mutable state lives in locals — which is
/// what this type is.
#[derive(Debug)]
pub struct Query<'a> {
    pub doc: &'a DocumentSnapshot,
    pub position: Offset,
    /// Scoped reads and searches. A handler cannot reach a file this did not
    /// give it.
    pub project: &'a ProjectView,
    pub deadline: &'a Deadline,
    /// Which oracle we are standing in for.
    pub server: &'a ServerProfile,
    /// The commit decision. Inert in v1; the only way to build an `Outcome`.
    pub policy: &'a CommitPolicy,
}

/// The behavioural differences between language servers for one language, as
/// observed rather than predicted (`core.md` §7). Empty in v1: a field appears
/// only once the corpus shows a systematic divergence that a field would fix.
///
/// Data rather than a trait, because a handler must not dispatch on server
/// *identity* — `if server.id == PYRIGHT` scattered through a handler is the
/// per-language configuration format `resolution.md` §1.2 rules out. A handler
/// reads a field describing a behaviour; it does not ask who it is talking to.
///
/// The field is private and there is a constructor per situation — standalone,
/// proxying the child on a command line, and standing in for the server a
/// corpus run names (`core.md` §1) — rather than `id: Option<ServerId>` left
/// open. The absence has
/// to be representable — standalone has no oracle, and a proxied server we
/// have no profile for is a different thing from one we do — but with a public
/// field the third case is representable too: a call site that *knows* which
/// server it is standing in for and passes `None` anyway. That was the whole
/// of this section's gap, and a constructor taking the name is what stops it
/// being expressible.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ServerProfile {
    id: Option<ServerId>,
}

impl ServerProfile {
    /// No oracle at all: nothing is being proxied, so there is no identity to
    /// resolve rather than one we failed to resolve.
    pub const fn standalone() -> Self {
        Self { id: None }
    }

    /// Proxying the child on this command line, as the shim was invoked
    /// (`core.md` §7: "resolved from the child's command name at startup").
    pub fn proxying_command(program: &OsStr, arguments: &[OsString]) -> Self {
        Self {
            id: ServerId::from_command(program, arguments),
        }
    }

    /// Standing in for the server a corpus run names — `measure`'s `--server`,
    /// which is a `servers.toml` key rather than a command line because a
    /// replay has no child to look at.
    pub fn proxying_named(name: &str) -> Self {
        Self {
            id: ServerId::from_name(name),
        }
    }

    /// `None` in standalone, and when proxying a server we have no profile
    /// for.
    pub fn id(&self) -> Option<ServerId> {
        self.id
    }
}

/// Not `Result`. Abstention is a normal, expected, frequently correct outcome
/// — the query genuinely had nothing to return, or the deadline expired — and
/// it does not share a type with "something went wrong".
///
/// `strata` and `trace` are everything `core.md` §7 calls handler-reported:
/// "only it knows which resolution path produced the answer and what it cost".
/// They are on both arms, because a stratum with no coverage and a stratum
/// whose searches all cost 40ms before abstaining are different findings and
/// the abstaining one is the more interesting.
// `conformance-013` (answered). The reporting channel is the value a
// handler already returns, rather than an out-parameter on `goto_definition`.
#[derive(Debug)]
pub enum Outcome {
    Committed {
        locations: Vec<Location>,
        confidence: Confidence,
        strata: Strata,
        trace: Trace,
    },
    Abstain {
        reason: AbstainReason,
        strata: Strata,
        trace: Trace,
    },
}

/// `core.md` §7's `stratum_prior` and `stratum_final`, which are two fields
/// and not one.
///
/// `resolution.md` §8 assigns a stratum a-priori from the reference, then
/// permits one refinement during the search. Coverage is reported on the prior
/// so the denominator is fixed by the reference and does not move when the
/// implementation changes; precision on the settled one, so an answer is
/// judged against the class it turned out to be. Collapsing them makes
/// `high-level.md`'s central table non-comparable across versions, which is
/// the one property it needs.
// `conformance-013` (answered).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Strata {
    prior: Stratum,
    settled: Stratum,
}

impl Strata {
    /// Before the search: the two agree, because nothing has refined anything.
    pub fn from_reference(stratum: Stratum) -> Self {
        Self {
            prior: stratum,
            settled: stratum,
        }
    }

    /// The one refinement the search may make. It takes a [`Refinement`]
    /// rather than a `Stratum` so that refining to a class which *is* knowable
    /// before the search does not compile.
    pub fn refine(self, refinement: Refinement) -> Self {
        Self {
            prior: self.prior,
            settled: refinement.stratum(),
        }
    }

    pub fn prior(self) -> Stratum {
        self.prior
    }

    /// §7's `stratum_final`, spelled `settled` because `final` is reserved.
    pub fn settled(self) -> Stratum {
        self.settled
    }
}

/// The only two strata a search may refine *to*: neither is knowable before it
/// runs, which is the whole reason `resolution.md` §8 permits a refinement at
/// all.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Refinement {
    AmbiguousName,
    ExternalDependency,
}

impl Refinement {
    fn stratum(self) -> Stratum {
        match self {
            Refinement::AmbiguousName => Stratum::AmbiguousName,
            Refinement::ExternalDependency => Stratum::ExternalDependency,
        }
    }
}

/// One ordered entry of `core.md` §7's `stages`: "which role the reference
/// got, what each stage found or missed, how many candidates survived
/// verification". The vocabulary is entirely the handler's — this is the
/// sanctioned channel §1 means when it says the detail a handler knows reaches
/// the metrics through the trace record rather than through the seam.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StageLabel(Box<str>);

impl StageLabel {
    pub fn new(label: &str) -> Self {
        Self(label.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A key of §7's `stage_us`, which is a different vocabulary from
/// [`StageLabel`] and is meant to stay one: the section's example pairs
/// `stages: ["ref:Type", "scope:miss", ...]` with
/// `stage_us: {"reference": 12, "scope": 40, ...}`. One is an account of what
/// happened and the other is a fixed set of pipeline stages to attribute cost
/// to, and a single type would invite the two to drift into each other.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StageName(Box<str>);

impl StageName {
    pub fn new(name: &str) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Microseconds of wall clock. An *observation*: §7 says it does not have to
/// be reproducible the way the rest of the record does, and that nothing is
/// ever gated on it.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Micros(pub u64);

/// §7's `margin`: the gap between the top-ranked candidate and the runner-up,
/// and one of the two features a precision floor would be set on.
///
/// Non-negative and finite, which is all that can be required of it: the
/// scores it is a difference of are the handler's own, so unlike
/// [`Confidence`] there is no upper bound to check against.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Margin(f32);

impl Margin {
    pub const ZERO: Self = Self(0.0);

    /// `None` for a negative margin — the runner-up cannot outrank the top —
    /// and for a NaN, which is neither.
    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

/// §7's `considered`: how many candidates the ranking chose between. The other
/// feature a floor would be set on, and meaningless without the margin.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CandidateCount(pub u32);

/// Everything `core.md` §7 calls handler-reported and `Outcome` did not
/// previously carry: `margin`, `considered`, `stages`, `stage_us`,
/// `bytes_scanned`, `files_parsed`.
///
/// **Write-only from a handler's side.** §7 gives `stages` three rules — it is
/// bounded, it is stable across runs for the same input, and *nothing branches
/// on it, ever*. The third is the one a type can hold: the fields are private
/// and the only reader is [`Trace::into_parts`], which consumes the trace a
/// handler still has to return. A handler that read its own account back would
/// have to give up the thing it is building.
///
/// `bytes_scanned` and `files_parsed` are counters and not limits — nothing
/// compares them against a budget and no search stops because of them
/// (`resolution.md` §1.3). They are here so a latency regression can be
/// attributed to a diff rather than guessed at.
///
/// **Boxed, and not allocated until something is reported.** A `Trace` is one
/// pointer wide, so widening `Outcome` with one did not widen every `Result`
/// that carries an outcome — `driver`'s `Dispatched` crossed clippy's
/// `result_large_err` threshold when this was six fields inline. The `None`
/// case is not only an optimisation: the commonest abstention on the query
/// path is `NotAnIdentifier`, decided from the tree before any work happens,
/// and it should not pay for a reporting channel it never writes to.
// `conformance-013` (answered).
#[derive(Debug)]
pub struct Trace(Option<Box<TraceParts>>);

impl Default for Trace {
    fn default() -> Self {
        Self::new()
    }
}

impl Trace {
    /// §7's "a small fixed maximum number of short labels, truncated rather
    /// than grown". A bound rather than a budget: exceeding it costs the tail
    /// of the account and nothing else, because nothing reads it back.
    pub const MAX_STAGES: usize = 32;

    pub fn new() -> Self {
        Self(None)
    }

    /// Appends to the ordered account, silently dropping past
    /// [`Trace::MAX_STAGES`]. Dropping the tail rather than the head keeps the
    /// prefix stable across runs, which is what lets failures be *grouped* by
    /// `stages` rather than merely listed.
    pub fn stage(&mut self, label: StageLabel) {
        let parts = self.parts();
        if parts.stages.len() < Self::MAX_STAGES {
            parts.stages.push(label);
        }
    }

    /// Attributes wall clock to a pipeline stage. Accumulates, because a stage
    /// re-entered during a fan-out is one stage that cost more.
    pub fn timed(&mut self, stage: StageName, elapsed: Micros) {
        let total = self.parts().stage_us.entry(stage).or_insert(Micros(0));
        total.0 = total.0.saturating_add(elapsed.0);
    }

    pub fn scanned(&mut self, bytes: ByteLen) {
        let parts = self.parts();
        parts.bytes_scanned = ByteLen(parts.bytes_scanned.0.saturating_add(bytes.0));
    }

    pub fn parsed(&mut self, files: FileCount) {
        let parts = self.parts();
        parts.files_parsed = FileCount(parts.files_parsed.0.saturating_add(files.0));
    }

    /// The two together, because a margin without the count it was measured
    /// over cannot be read: a margin of 0.6 over two candidates and over two
    /// hundred are different claims.
    pub fn ranked(&mut self, margin: Margin, considered: CandidateCount) {
        let parts = self.parts();
        parts.margin = Some(margin);
        parts.considered = Some(considered);
    }

    /// The one reader, and it consumes. See the type's own documentation for
    /// why that is the signature rather than a set of getters.
    pub fn into_parts(self) -> TraceParts {
        self.0.map_or_else(TraceParts::empty, |parts| *parts)
    }

    fn parts(&mut self) -> &mut TraceParts {
        self.0.get_or_insert_with(|| Box::new(TraceParts::empty()))
    }
}

/// A [`Trace`] taken apart, for the one consumer that assembles §7's record.
/// Public fields because it is a destructuring and nothing more; the
/// invariants it was built under were enforced on the way in.
#[derive(Debug)]
pub struct TraceParts {
    pub stages: Vec<StageLabel>,
    pub stage_us: BTreeMap<StageName, Micros>,
    pub bytes_scanned: ByteLen,
    pub files_parsed: FileCount,
    pub margin: Option<Margin>,
    pub considered: Option<CandidateCount>,
}

impl TraceParts {
    /// What a handler that reported nothing produces, and what a `Err` return
    /// produces: §7's columns at the values that say "no account was given".
    fn empty() -> Self {
        Self {
            stages: Vec::new(),
            stage_us: BTreeMap::new(),
            bytes_scanned: ByteLen::ZERO,
            files_parsed: FileCount(0),
            margin: None,
            considered: None,
        }
    }
}

/// One per row of `high-level.md`'s stratification list, plus a placeholder.
/// What each means, and how a query is assigned one, is `resolution.md` §8.
///
/// Reported on both arms of `Outcome`, because coverage per stratum is
/// meaningless without knowing which stratum the abstentions belonged to.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Stratum {
    LocalBinding,
    SameFileModule,
    ExplicitImport,
    WildcardImport,
    AmbiguousName,
    ExternalDependency,
    MacroGenerated,
    TypeInferenceRequired,
    /// The language crate template, unmodified. No real handler may return
    /// this: its presence in a metrics table means the template has not been
    /// replaced, which is a gate check rather than something anybody has to
    /// notice (`core.md` §9).
    Unimplemented,
}

/// Every stratum, once. The single list: [`Stratum::from_index`] reads it, and
/// `shared/tests/handler.rs` walks it to hold the codec below against the enum.
///
/// The array's length is written out, so a variant added to `Stratum` without
/// being added here fails to compile rather than dropping out of the round trip
/// quietly.
pub(crate) const EVERY_STRATUM: [Stratum; 9] = [
    Stratum::LocalBinding,
    Stratum::SameFileModule,
    Stratum::ExplicitImport,
    Stratum::WildcardImport,
    Stratum::AmbiguousName,
    Stratum::ExternalDependency,
    Stratum::MacroGenerated,
    Stratum::TypeInferenceRequired,
    Stratum::Unimplemented,
];

impl Stratum {
    /// A dense index, for the lock-free cell a handler publishes its prior
    /// through (`core-025`, and [`crate::ProjectView::classified`]).
    ///
    /// `pub(crate)`: it is how one `shared` type reaches another across an
    /// atomic, and not something a handler has any use for. An exhaustive match
    /// rather than `#[repr(u8)]` and a cast, so a new stratum has to be given an
    /// index here instead of silently taking whichever one the layout gave it —
    /// and so the two directions are written in the same place, where a reader
    /// can see they agree.
    pub(crate) fn index(self) -> u8 {
        match self {
            Stratum::LocalBinding => 0,
            Stratum::SameFileModule => 1,
            Stratum::ExplicitImport => 2,
            Stratum::WildcardImport => 3,
            Stratum::AmbiguousName => 4,
            Stratum::ExternalDependency => 5,
            Stratum::MacroGenerated => 6,
            Stratum::TypeInferenceRequired => 7,
            Stratum::Unimplemented => 8,
        }
    }

    /// The inverse, and `None` for the sentinel the cell holds until something
    /// publishes — which is why this is fallible rather than saturating: "no
    /// classification" is a value the cell really holds, and the whole of
    /// `core-025` is about not inventing a stratum for it.
    pub(crate) fn from_index(index: u8) -> Option<Self> {
        EVERY_STRATUM
            .into_iter()
            .find(|stratum| stratum.index() == index)
    }
}

/// Carries no resolution vocabulary: the variants are unit or carry
/// primitives, so `resolution.md`'s internal types stay out of the seam. What
/// a handler knows beyond this reaches the metrics through the trace record
/// (`core.md` §7) rather than through here.
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum AbstainReason {
    /// The cursor is not on a resolvable identifier.
    NotAnIdentifier,
    /// An identifier, but of a kind this language does not resolve.
    UnsupportedRole,
    /// Searched exhaustively, found nothing. The one reason that is evidence
    /// about the file list, which is why it and not any other triggers
    /// `core.md` §4's debounced rescan.
    NoCandidates,
    /// The deadline expired mid-search. The one latency-shaped abstention
    /// `high-level.md` allows, and the only reason here that is not a fact
    /// about the code.
    Deadline,
    /// The only plausible target is outside the workspace. Carries the name
    /// because standalone puts it in the error text (`shim.md` §8).
    External { name: Box<str> },
}

impl AbstainReason {
    /// `core.md` §4's rule that the rescan trigger is `NoCandidates`
    /// *specifically*, not any abstention.
    ///
    /// Here rather than in `driver`, for two reasons. It is a fact about what
    /// a reason means, which is this enum's business and not its consumer's;
    /// and the enum is `#[non_exhaustive]`, so the same match written in
    /// `driver` would need the wildcard arm `CLAUDE.md` bans — the arm that
    /// would silently classify the next variant as inconclusive instead of
    /// failing to compile until somebody decides.
    pub fn file_list_evidence(&self) -> FileListEvidence {
        match self {
            // An exhaustive search found nothing, which is evidence about the
            // list: the file it wanted may have been created since the walk.
            Self::NoCandidates => FileListEvidence::Stale,
            // Evidence about the cursor and about the language, not about the
            // filesystem. `Deadline` is the one that matters: the search was
            // cut off, so it says nothing about what a complete one would have
            // found, and rescanning on it would spend I/O inside the window
            // that just proved to be short of it.
            Self::NotAnIdentifier
            | Self::UnsupportedRole
            | Self::Deadline
            | Self::External { name: _ } => FileListEvidence::Inconclusive,
        }
    }
}

/// Whether an abstention is evidence that the file list is out of date. An
/// enum rather than a `bool` so the call site reads as the question §4 asks.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileListEvidence {
    Stale,
    Inconclusive,
}

/// Stratum -> minimum `Confidence`. Empty in v1, where `decide` returns
/// `Committed` for every input.
///
/// Handlers never construct `Outcome::Committed` themselves; every path ends
/// here. The funnel buys nothing today — what it buys is that "a per-mode
/// precision floor is a data change rather than a code change" is true when
/// the floor arrives, instead of being an audit of every commit site in every
/// `lang_*` crate at the moment when there are the most of them
/// (`resolution.md` §7.4).
#[derive(Debug)]
pub struct CommitPolicy;

impl CommitPolicy {
    /// v1's only policy: commit everything, gate nothing. `resolution.md` §7.5
    /// argues that bootstrapping has to be permissive, because a floor can
    /// only be derived from `(stratum, confidence, agreed?)` triples collected
    /// while nothing was being gated.
    pub fn permissive() -> Self {
        Self
    }

    pub fn decide(
        &self,
        strata: Strata,
        confidence: Confidence,
        locations: Vec<Location>,
        trace: Trace,
    ) -> Outcome {
        Outcome::Committed {
            locations,
            confidence,
            strata,
            trace,
        }
    }
}
