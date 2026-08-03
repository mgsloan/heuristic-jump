//! The shared vocabulary: the types every other crate needs to talk about,
//! and almost no behaviour. `design/core.md` §1 has the handler seam, §8 the
//! hand-written LSP wire types, and §9 why this is a crate of its own rather
//! than the bottom of `driver`.
//!
//! The text-shaped newtypes are re-exported rather than defined: they live in
//! `vendor/rope`, because `shared` depends on `rope` and the dependency cannot
//! run the other way (`rope-modifications.md` §2). Every other crate says
//! `shared::ByteOffset` and never has to know which side of that edge it came
//! from.

mod error;
mod vocabulary;

pub use error::{Error, HandlerError, ParseError, ProjectError, ProtocolError};
pub use rope::{ByteLen, ByteOffset, ByteRange, LineIndex, Rope};
pub use vocabulary::{
    Confidence, DocumentUri, DocumentVersion, FileExtension, LanguageId, Location, ServerId,
};
