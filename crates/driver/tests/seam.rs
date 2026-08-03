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

/// `core.md` §9's graph, for the two crates whose edges are claims rather than
/// conveniences.
///
/// `measure_core` "depends on `shared` and nothing else of ours" — not on
/// `driver`, because it is an LSP client and not a proxy, and not on any
/// language, because it takes its handler as `&dyn LanguageHandler`. And
/// `measure_rust` is a **separate crate rather than a `[[bin]]`** inside
/// `lang_rust`, because a `[[bin]]` shares its crate's dependency list:
/// `lang_rust` would acquire `measure_core`, and `heuristic_jump` would then
/// link an LSP client into the shipped binary.
///
/// Both are invisible to the build if broken — an extra edge only shows up as
/// a slower compile — which is why they are asserted against the manifests.
/// `[dev-dependencies]` are excluded deliberately: the claim is about the
/// crate graph the shipped artifacts are built from, and a test needing a
/// grammar is not an edge in it.
#[test]
fn the_measurement_crates_have_the_edges_section_9_gives_them() {
    let forbidden: &[(&str, &[&str])] = &[
        ("measure_core", &["driver", "lang_"]),
        // `lang_rust` must not know `measure_core` exists; the dependency
        // points the other way, which is the whole reason `measure_rust` is a
        // crate of its own.
        ("lang_rust", &["measure_"]),
    ];

    for (crate_name, banned) in forbidden {
        for dependency in dependencies_in(&manifest_text(crate_name)) {
            for prefix in *banned {
                assert!(
                    !dependency.starts_with(prefix),
                    "{crate_name} depends on {dependency}: core.md §9's graph \
                     has no edge from {crate_name} to anything named {prefix}*"
                );
            }
        }
    }

    let measure_core = dependencies_in(&manifest_text("measure_core"));
    let measure_rust = dependencies_in(&manifest_text("measure_rust"));
    assert!(
        measure_core.iter().any(|name| name == "shared"),
        "measure_core does not depend on shared, and core.md §9 says it depends \
         on shared and nothing else of ours"
    );
    assert!(
        measure_rust.iter().any(|name| name == "measure_core")
            && measure_rust.iter().any(|name| name == "lang_rust"),
        "measure_rust is the one crate depending on both measure_core and a \
         language, and it is not depending on both"
    );
}

/// The `[dependencies]` table only. A hand-rolled scan rather than
/// `cargo metadata`, which is a subprocess this suite may not spawn.
///
/// Pure, and taking the text rather than the crate name, because `panic` is
/// denied outside a `#[test]` and a helper that swallowed a missing manifest
/// would make every assertion below pass vacuously.
fn dependencies_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[dependencies]";
            continue;
        }
        if !inside || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.split(['.', ' ', '=']).next() {
            found.push(name.trim().to_owned());
        }
    }
    found
}

fn manifest_text(crate_name: &str) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("Cargo.toml");
    // `unwrap_or_default` would be a silent pass; the emptiness is asserted by
    // every caller, which all require a dependency to be present.
    std::fs::read_to_string(&manifest).unwrap_or_default()
}
