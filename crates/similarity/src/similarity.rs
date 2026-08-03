//! Text similarity, ported from the prior implementation and frozen.
//!
//! `resolution.md` §5 says what came across and what deliberately did not. The
//! narrowing is the point: this served edit prediction, whose job is
//! retrieving *related* code, and here it serves go-to-definition, whose job
//! is finding *the* declaration. Two of the old signals are good at the first
//! and actively harmful at the second.
//!
//! **Kept.** The occurrence-hash machinery — [`Occurrences`],
//! [`SmallOccurrences`], [`HashFrom`], [`IdentifierParts`], and the Jaccard
//! and weighted-overlap metrics — and path/namespace similarity, which is what
//! stage 3 needs and the piece §5 calls genuinely hard to rewrite well.
//!
//! **Ported but unused.** Body-text similarity — [`CodeParts`], [`NGram`],
//! and [`SlidingWindow`] — is not a signal §5's pipeline uses, because it
//! prefers the definition that most resembles the call site's surroundings,
//! and among several same-named candidates that is a plausible-wrong-answer
//! generator. The code is here anyway: having the machinery and using the
//! signal are different questions, and `resolution.md` open question 7 asks
//! whether it comes back as a tiebreak. That question is cheap to answer with
//! the code present and expensive without it.
//!
//! **Not ported.** The retrieval score's tiering — same file, exact path
//! import, wildcard import, namespace similarity, then `1 / declaration_count`
//! — which §5 keeps as a *model* rather than as code: `resolution.md` §6
//! reproduces it in the handler, where the candidate set it ranks lives. Its
//! weights were tuned for a different objective and are discarded.
//!
//! **Frozen for the duration of phase 2** (`loops.md` §13): no loop may write
//! here, because shared resolution code is writable only during
//! whole-repository phases. What phase 3 extracts from the language crates
//! lands here.

mod occurrences;
mod sliding_window;
mod source;

use std::path::Path;

pub use crate::occurrences::{
    HashFrom, Occurrences, Similarity, SmallOccurrences, WeightedSimilarity,
};
pub use crate::sliding_window::SlidingWindow;
pub use crate::source::{CodeParts, IdentifierParts, NGram, OccurrenceSource};

/// How much a namespace like `a::b::c` looks like a candidate file's path.
///
/// Both sides are hashed into identifier parts and compared by Jaccard, which
/// is what makes `foo::bar::Baz` score against `src/foo/bar.rs` without either
/// side knowing the other's syntax. Separators do not survive hashing, so the
/// comparison is over the set of parts rather than their order — a namespace
/// whose parts appear scattered through a path scores the same as one whose
/// parts appear in order, and ranking has to break that tie some other way.
///
/// The file extension is excluded from the path, since it says which language
/// the file is rather than what it declares.
pub fn namespace_path_similarity(namespace: &str, path: &Path) -> f32 {
    let namespace_parts = Occurrences::new(IdentifierParts::occurrences_in_str(namespace));
    let path_parts = Occurrences::new(IdentifierParts::occurrences_in_path(path));
    namespace_parts.jaccard_similarity(&path_parts)
}
