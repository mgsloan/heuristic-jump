//! `design/core.md` §9's language-crate template, instantiated for Rust.
//!
//! **This is the shape every `lang_*` inherits**, fixed once by hand before
//! seven of them exist. Adding a language is a copy and a rename of this
//! directory and `crates/measure_rust/`, plus one line in `heuristic_jump`.
//!
//! It is not nothing and it is not a baseline. It declares its real
//! `language_ids`, `file_extensions` and `grammar` — all three come from the
//! grammar crate and are correct from the start — so an instantiated template
//! compiles, links, runs under `measure-rust replay` and produces a complete
//! per-stratum table. The first measurement of a new language therefore
//! exercises the whole pipeline rather than failing to build and saying
//! nothing about whether any of it is wired up.
//!
//! Beyond that it implements exactly one thing, and it is the one thing that
//! is genuinely language-independent: deciding whether the cursor is on an
//! identifier at all. That rule is [`shared::identifier_at`], not a copy of it
//! — `measure_core` enumerates corpus positions with the same function, and
//! two implementations that agreed today would stop agreeing silently.
//!
//! **No tests, deliberately.** The corpus is the oracle, it replays without a
//! language server, and it is made of real repositories nobody here wrote. A
//! `tests/fixtures/` directory in the template is an invitation to fill it,
//! which converts a self-graded oracle into the thing a campaign optimises.
//! The pinning fixtures `resolution.md` §11 describes are added by a campaign
//! that can say which behaviour the corpus could not isolate, and not by
//! default.

use shared::{
    AbstainReason, Error, FileExtension, LanguageHandler, LanguageId, Outcome, Query, Stratum,
};
use tree_sitter::Language;

const LANGUAGE_IDS: &[LanguageId] = &[LanguageId::new("rust")];
const FILE_EXTENSIONS: &[FileExtension] = &[FileExtension::new("rs")];

/// Unit for now, and constructed through `new` anyway: `heuristic_jump` and
/// `measure_rust` both name `Handler::new()`, and a real handler acquires
/// state — an interned query set, a `similarity` index — without either call
/// site changing.
#[derive(Clone, Copy, Default, Debug)]
pub struct Handler;

impl Handler {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageHandler for Handler {
    fn language_ids(&self) -> &'static [LanguageId] {
        LANGUAGE_IDS
    }

    fn file_extensions(&self) -> &'static [FileExtension] {
        FILE_EXTENSIONS
    }

    fn grammar(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    /// Everything abstains, and the abstention is **self-identifying**.
    ///
    /// A template that abstained as `NoCandidates` would be indistinguishable
    /// in the metrics from a real handler that searched and found nothing, so
    /// a half-migrated language would look like a language that was merely bad
    /// at its job. `Stratum::Unimplemented` is what no real handler may return
    /// (`resolution.md` §8), so its presence in a metrics table means the
    /// template has not been replaced — a gate check rather than something
    /// anybody has to notice.
    ///
    /// `Unimplemented` is reported on the `NotAnIdentifier` arm too, though
    /// that arm is already correct. A stratum is a claim about where the
    /// definition turned out to be, and this handler never looked; reporting
    /// anything else would put rows in a real stratum's denominator that no
    /// resolution logic produced.
    fn goto_definition(&self, query: &Query<'_>) -> Result<Outcome, Error> {
        let reason = match shared::identifier_at(query.doc, query.position) {
            None => AbstainReason::NotAnIdentifier,
            // On an identifier, and this template resolves no reference of any
            // kind. `UnsupportedRole` is the honest reading of "an identifier,
            // but of a kind this language does not resolve".
            Some(_) => AbstainReason::UnsupportedRole,
        };

        Ok(Outcome::Abstain {
            reason,
            stratum: Stratum::Unimplemented,
        })
    }
}
