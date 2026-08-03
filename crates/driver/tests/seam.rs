//! `design/core.md` §1 makes two claims about *where* things are, and both
//! are the kind that stay true by accident until the day they do not: "This
//! trait lives in `shared`, which is deliberately *not* `driver`", and — §9's
//! dependency graph — `driver` depends on no language crate, which is what
//! `LanguageHandler::grammar` returning a runtime `tree_sitter::Language`
//! exists to make possible.
//!
//! Nothing else in the build would notice either one being dropped. A
//! `use shared::LanguageHandler` in `driver` still compiles if the trait moves
//! here, and a grammar dependency added to `driver` would only show up as a
//! slower build.

use std::path::Path;

use shared::LanguageHandler;

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
