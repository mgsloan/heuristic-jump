//! The shared vocabulary: the types every other crate needs to talk about,
//! and almost no behaviour. `design/core.md` §1 has the handler seam, §8 the
//! hand-written LSP wire types, and §9 why this is a crate of its own rather
//! than the bottom of `driver`.
//!
//! Empty so far. The workspace layout exists before the types do, deliberately
//! — `core.md` §9 is what the first campaign could satisfy without `vendor/`,
//! and the seam it holds is frozen at the phase 1a gate, so it is written once
//! and with the escalation that decision needs.
