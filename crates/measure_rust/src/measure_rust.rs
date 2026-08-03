//! `measure-rust`, and essentially the whole of it — `core.md` §7's four
//! lines. Adding a language is a copy of this file and its manifest with the
//! names changed.

use clap::Parser;

fn main() -> Result<(), shared::Error> {
    measure_core::run(&lang_rust::Handler::new(), measure_core::Cli::parse())
}
