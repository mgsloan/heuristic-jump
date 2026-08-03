//! `design/core.md` §1 makes three claims about *where* things are, and each
//! is the kind that stays true by accident until the day it does not: "This
//! trait lives in `shared`, which is deliberately *not* `driver`"; — §9's
//! dependency graph — `driver` depends on no language crate, which is what
//! `LanguageHandler::grammar` returning a runtime `tree_sitter::Language`
//! exists to make possible; and the text-shaped vocabulary is defined in the
//! vendored `rope` and re-exported by `shared`, so that a crate which does not
//! depend on `rope` can still name all of it.
//!
//! Nothing else in the build would notice any of them being dropped. A
//! `use shared::LanguageHandler` in `driver` still compiles if the trait moves
//! here, a grammar dependency added to `driver` would only show up as a slower
//! build, and a name missing from the re-export list is invisible until some
//! crate that cannot reach `rope` needs it.
//!
//! This file is where the third one belongs rather than in `shared`'s own
//! tests, because `shared` *does* depend on `rope`: the property is about a
//! crate that may not, and `driver` is one (`rustc-hash`, `shared`, `tracing`,
//! and no rope).

use std::path::Path;

use shared::{
    ByteColumn, ByteLen, ByteOffset, ByteRange, CharCount, LanguageHandler, LineIndex, Utf16Column,
};

#[test]
fn the_handler_seam_is_defined_in_shared() {
    // `type_name` names the *defining* crate, not the path it was reached by,
    // so a re-export from `driver` would not satisfy this.
    let seam = std::any::type_name::<dyn LanguageHandler>();
    assert!(
        seam.starts_with("dyn shared::"),
        "the handler seam is defined in {seam}, and core.md §1 puts it in shared"
    );
}

#[test]
fn neither_driver_nor_shared_depends_on_a_language() {
    for crate_name in ["driver", "shared"] {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(crate_name)
            .join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|error| panic!("reading {}: {error}", manifest.display()));

        for line in text.lines() {
            let dependency = line.split(['.', ' ', '=']).next().unwrap_or("").trim();
            assert!(
                !dependency.starts_with("lang_") && !dependency.starts_with("tree-sitter-"),
                "{crate_name} depends on {dependency}: core.md §9's graph has no edge from \
                 {crate_name} to a language, and grammar() exists so that it needs none"
            );
        }
    }
}

/// The seven names §1 lists, and the direction of the edge that puts them in
/// `rope`. Three of them — `ByteColumn`, `Utf16Column`, `CharCount` — were
/// absent from the re-export for two campaigns without anything failing,
/// because a re-export list is only ever checked by reading it.
#[test]
fn the_text_vocabulary_is_nameable_through_shared_and_defined_in_rope() {
    // `type_name` names the *defining* crate rather than the path the type was
    // reached by, which is what separates a re-export from a second definition
    // in `shared` — one that would compile, satisfy every use site here, and
    // silently not be the type rope's own signatures speak in.
    for defined_in in [
        std::any::type_name::<ByteOffset>(),
        std::any::type_name::<ByteLen>(),
        std::any::type_name::<ByteRange>(),
        std::any::type_name::<LineIndex>(),
        std::any::type_name::<ByteColumn>(),
        std::any::type_name::<Utf16Column>(),
        std::any::type_name::<CharCount>(),
    ] {
        assert!(
            defined_in.starts_with("rope::"),
            "{defined_in} reaches driver through shared, but core.md §1 defines the text \
             vocabulary in rope and re-exports it, because shared depends on rope and the \
             dependency cannot run the other way"
        );
    }
}
