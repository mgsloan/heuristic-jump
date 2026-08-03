//! `design/core.md` §1: the seam this project commits to, and everything
//! behind it is out of scope here. Dispatch is direct — no framework, and no
//! config format that languages have to be expressed in.
//!
//! It lives in `shared` rather than in `driver` because `measure_core` calls
//! it a whole phase before a shim exists (`core.md` §9's dependency graph), so
//! a language can be measured before there is anything to proxy.

use tree_sitter::Language;

use crate::deadline::Deadline;
use crate::document::DocumentSnapshot;
use crate::error::Error;
use crate::project::ProjectView;
use crate::vocabulary::{Confidence, FileExtension, LanguageId, Location, ServerId};
use rope::ByteOffset;

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
    pub position: ByteOffset,
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
#[derive(Debug)]
pub struct ServerProfile {
    /// `None` in standalone, and when proxying a server we have no profile
    /// for. The absence has to be representable because the two modes are not
    /// the same situation and a synthesised identity would hide that.
    pub id: Option<ServerId>,
}

/// Not `Result`. Abstention is a normal, expected, frequently correct outcome
/// — the query genuinely had nothing to return, or the deadline expired — and
/// it does not share a type with "something went wrong".
#[derive(Debug)]
pub enum Outcome {
    Committed {
        locations: Vec<Location>,
        confidence: Confidence,
        stratum: Stratum,
    },
    Abstain {
        reason: AbstainReason,
        stratum: Stratum,
    },
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
        stratum: Stratum,
        confidence: Confidence,
        locations: Vec<Location>,
    ) -> Outcome {
        Outcome::Committed {
            locations,
            confidence,
            stratum,
        }
    }
}
