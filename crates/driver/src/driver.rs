//! The LSP driver: everything `design/shim.md` describes, generic over the
//! handler set. Named `driver` rather than `core` because a crate named `core`
//! shadows Rust's own and the prose already uses "core" for the single-threaded
//! actor (`design/core.md` §9).
//!
//! It depends on `shared` and on no language crate, and that direction is the
//! one rule the workspace graph is built around. `tests/seam.rs` asserts it
//! rather than leaving it to be read.

mod config;
mod dispatch;

pub use config::{Config, DeadlineMs, DeadlineOverride, Heuristics, Mode, ServerCommand};
pub use dispatch::{Dispatched, Registry, dispatch, hard_cap};
