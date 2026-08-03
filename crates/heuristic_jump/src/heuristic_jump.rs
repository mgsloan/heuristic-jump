//! The shim binary, `heuristic-jump`. This crate is the wiring: argument
//! parsing, log setup, and the one place the language list is enumerated, so
//! that `driver` stays a library with no opinion about how it was invoked
//! (`design/core.md` §9, `design/deps.md` §11).

fn main() {}
