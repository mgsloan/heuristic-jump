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
//! crate that may not, and `driver` is one (`crossbeam-channel`, `serde_json`,
//! `shared`, `tracing`, and no rope).

use std::path::{Path, PathBuf};

use shared::{
    ByteColumn, ByteLen, ByteRange, CharCount, LanguageHandler, LineIndex, Offset, Utf16Column,
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

/// The grammar half is asserted against `[dependencies]` rather than against
/// every line of the manifest, which is what it used to read.
///
/// §9's graph is the graph the shipped binary has, and a `[dev-dependencies]`
/// grammar is not in it — `measure_core`'s own manifest already writes that
/// reading down, for the same reason: "taking a grammar directly rather than
/// reaching for `lang_rust` is what keeps the test honest about
/// `&dyn LanguageHandler`". `shared` needs one to test `ProjectView::parse`
/// at all, since a `tree_sitter::Language` cannot be constructed without a
/// grammar crate.
///
/// A `lang_*` edge stays banned in *every* table. A language crate in a test
/// is the thing the runtime edge would have been, only later: it would let
/// `driver` or `shared` be written against one language's behaviour and pass.
#[test]
fn neither_driver_nor_shared_depends_on_a_language() {
    for crate_name in ["driver", "shared"] {
        let text = manifest_text(crate_name);
        assert!(
            !text.is_empty(),
            "no manifest for {crate_name}, so every assertion below is vacuous"
        );

        for line in text.lines() {
            let named = line.split(['.', ' ', '=']).next().unwrap_or("").trim();
            assert!(
                !named.starts_with("lang_"),
                "{crate_name} names {named} in its manifest: core.md §9's graph has no edge \
                 from {crate_name} to a language crate, in any table, and grammar() exists so \
                 that it needs none"
            );
        }

        for dependency in dependencies_in(&text) {
            assert!(
                !dependency.starts_with("tree-sitter-"),
                "{crate_name} depends on {dependency}: core.md §9's graph has no edge from \
                 {crate_name} to a grammar, and grammar() returning a runtime \
                 tree_sitter::Language is what makes that possible"
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
        std::any::type_name::<Offset>(),
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

/// `core.md` §9: `heuristic_jump` "is also the single place where the language
/// list is enumerated", and `#adding-a-language` prices a new language at two
/// crate directories "plus one line in `heuristic_jump`".
///
/// The compiler checks half of this and cannot check the other half. A
/// `heuristic_jump` that names `lang_rust::Handler::new()` obviously needs the
/// dependency — but a `crates/lang_python/` added to the workspace and never
/// named builds perfectly, ships nothing, and reports no error anywhere: the
/// binary simply has no Python handler, and the first sign of it is a metrics
/// table with no Python rows. So the enumeration is checked against the
/// workspace members rather than against itself.
///
/// The source scan is deliberately for `Handler::new()` rather than for the
/// crate name alone, because a `lang_python` that reached the manifest and a
/// doc comment but not the registry is exactly the failure being caught.
#[test]
fn the_language_list_is_enumerated_in_heuristic_jump() {
    let languages = language_members();
    assert!(
        !languages.is_empty(),
        "no crates/lang_* workspace member, so this test would pass vacuously"
    );

    let wiring = workspace_file("crates/heuristic_jump/src/heuristic_jump.rs");
    let declared = dependencies_in(&manifest_text("heuristic_jump"));
    for language in languages {
        assert!(
            declared.iter().any(|name| name == &language),
            "{language} is a workspace member and heuristic_jump does not depend on it: \
             core.md §9 has an edge from heuristic_jump to every lang_*"
        );
        assert!(
            wiring.contains(&format!("{language}::Handler::new()")),
            "heuristic_jump does not register {language}::Handler::new(): core.md §9 makes \
             this the single place the language list is enumerated, so a language missing \
             from it is a language the shipped binary does not have"
        );
    }
}

/// `core.md` §9's directory tree, compared against the workspace in both
/// directions, with the document as the fixture rather than a list
/// transcribed here — the same arrangement `deps.md` §15's lint block gets,
/// and for the same reason: a third copy is the thing that drifts.
///
/// Both directions are the claim. A crate the tree names and the workspace
/// lacks is a crate nobody built; a crate the workspace has and the tree does
/// not name is the layout growing without the section that argued for it,
/// which is how `crates/` stops being "our code" and `vendor/` stops being
/// "copied-in Zed crates, kept separate so provenance and licensing stay
/// obvious".
///
/// The four `phase 2` entries are exempt and the marking is load-bearing:
/// `loops.md`'s decided question 10 puts a new `crates/lang_*` outside every
/// loop's owned paths, so a phase 1a campaign that tried to satisfy the tree
/// by creating `crates/lang_python/` would have its commit rejected by the
/// gate rather than merged (CHANGE-core-010). Without the marking this test
/// would be demanding exactly that.
///
/// The two names §9 calls "chosen rather than mechanical" are asserted with
/// it, since both are decisions that read as arbitrary later:
///
/// > **`driver`, not `core`.** A crate named `core` shadows Rust's own, and
/// > this document already uses "`core`" throughout for the single-threaded
/// > actor in section 2.
///
/// > **`heuristic_jump`** for the binary crate, with a two-line `[[bin]]`
/// > rename so that the produced binary is `heuristic-jump`. Cargo names a
/// > binary target after the package verbatim and does not hyphenate it.
#[test]
fn the_workspace_is_the_layout_section_9_prints() {
    let printed = fenced_block_of(&workspace_file("design/core.md"), "## 9. Workspace layout");
    assert!(
        printed.contains("crates/"),
        "no fenced block found under core.md §9, so this test would compare nothing"
    );

    let mut parent = String::new();
    let mut expected = Vec::new();
    for line in printed.lines() {
        let indented = line.starts_with(' ');
        let mut fields = line.split_whitespace();
        let Some(entry) = fields.next().and_then(|name| name.strip_suffix('/')) else {
            continue;
        };
        if !indented {
            parent = entry.to_owned();
            continue;
        }
        // The one thing phase 1a may not build. `state/phase.toml` names
        // `crates/lang_rust/` rather than globbing `crates/lang_*/`, so the
        // gate rejects the commit that would create the others.
        if fields.clone().eq(["phase", "2"]) {
            continue;
        }
        expected.push(format!("{parent}/{entry}"));
    }
    expected.sort();

    let mut members = workspace_members();
    members.sort();
    assert_eq!(
        members, expected,
        "the workspace is not the layout core.md §9 prints. A crate the tree names and \
         [workspace] members lacks is one nobody builds; a member the tree does not name is the \
         layout growing without the section that argued for it, which is what keeps crates/ our \
         code and vendor/ the copied-in Zed crates"
    );

    assert!(
        !members.iter().any(|member| member.ends_with("/core")),
        "a crate is named core: §9 chose `driver` deliberately, because a crate named core \
         shadows Rust's own and these documents already call the single-threaded actor core — \
         two things with that name in one system is the ambiguity the choice avoids"
    );
    assert!(
        table_of(&workspace_file("Cargo.toml"), "workspace.package")
            .iter()
            .any(|line| line.replace(' ', "") == "publish=false"),
        "[workspace.package] does not set publish = false: §9 makes it workspace-wide, and it \
         is what lets the crate names carry no project prefix — an accidental publish of a \
         crate named `shared` is not a mistake with a second chance"
    );

    for (member, artifact) in [
        ("crates/heuristic_jump", "heuristic-jump"),
        ("crates/measure_rust", "measure-rust"),
    ] {
        let named = table_of(&workspace_file(&format!("{member}/Cargo.toml")), "[bin]")
            .iter()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "name").then(|| value.trim().trim_matches('"').to_owned())
            })
            .unwrap_or_default();
        assert_eq!(
            named, artifact,
            "{member} builds a binary named {named:?}: §9 wants {artifact:?}, and cargo names a \
             binary target after the package verbatim without hyphenating it — so the two-line \
             [[bin]] rename is the whole of what makes the artifact match the name every \
             invocation in these documents uses (CHANGE-conformance-001)"
        );
    }
}

/// The first unlabelled fenced block after a heading. `fenced_toml_of` above
/// takes the `toml` ones; §9's directory tree carries no language tag, and a
/// reader that accepted either would take §15's `toml` block for a tree.
fn fenced_block_of(document: &str, heading: &str) -> String {
    let Some((_, after)) = document.split_once(heading) else {
        return String::new();
    };
    let Some((_, block)) = after.split_once("```\n") else {
        return String::new();
    };
    block
        .split_once("\n```")
        .map_or("", |(body, _)| body)
        .to_owned()
}

/// `core.md#adding-a-language`, which is a price rather than a description:
/// "New `crates/lang_<x>/` … `crates/measure_<x>/`, which is four lines; then
/// one line in `heuristic_jump`. **No crate other than `heuristic_jump`
/// changes**, which is the whole cost and the point of the graph above."
///
/// `the_language_list_is_enumerated_in_heuristic_jump` above holds the one
/// line. This holds the rest of the price, and every part of it is a claim
/// that decays quietly rather than breaking: the two template manifests are
/// correct until somebody adds a dependency that a later language then
/// inherits by copying; the four lines are four until a `measure_<x>` grows a
/// fifth; and "no crate other than `heuristic_jump`" is true until one crate
/// reaches for one language and nothing anywhere reports it.
///
/// The template's contents are asserted in **both** directions, which is what
/// makes them a template. A missing `similarity` is a language crate that
/// cannot rank, discovered by the seventh author rather than the first; an
/// extra dependency is the shape §9 fixes "once, by hand, before seven of
/// them exist" drifting after the first copy.
///
/// > **No tests.** … an empty `tests/fixtures/` directory in the template is
/// > an invitation to fill it, which converts a self-graded oracle into the
/// > thing a campaign optimises.
///
/// That one cannot be enforced by anything but a scan. A `tests/` directory
/// added to `crates/lang_python/` is a directory somebody made; nothing fails,
/// the suite gets larger, and the oracle quietly becomes the expectations the
/// session that wrote them held.
///
/// One clause here has no runnable control, and it is the `[workspace
/// .dependencies]` entry: deleting it does not fail this test, it stops the
/// workspace resolving, because `heuristic_jump` and `measure_<x>` both
/// inherit from it. The assertion is kept for the case the compiler does not
/// cover — a `lang_<x>` that arrives as a member before anything depends on
/// it, which resolves fine and is the state one commit before the entry is
/// needed.
#[test]
fn adding_a_language_costs_the_template_and_one_line() {
    /// Everything a `lang_*` may name, besides its own grammar. `tree-sitter`
    /// is the runtime rather than a grammar and is forced by
    /// `LanguageHandler::grammar` returning a `tree_sitter::Language`
    /// (CHANGE-core-008).
    const LANGUAGE: &[&str] = &["shared", "similarity", "tree-sitter"];

    /// Everything a `measure_*` may name, besides its own language. `clap` and
    /// `shared` are the four lines' own requirements — `Cli::parse()` needs
    /// the trait in scope and `main` returns `Result<(), shared::Error>`.
    const MEASURE: &[&str] = &["measure_core", "clap", "shared"];

    let languages = language_members();
    assert!(
        !languages.is_empty(),
        "no crates/lang_* workspace member, so this test would pass vacuously"
    );

    let path_dependencies = workspace_path_dependencies();
    for language in &languages {
        let measure = format!("measure_{}", language.trim_start_matches("lang_"));
        let grammar = format!("tree-sitter-{}", language.trim_start_matches("lang_"));

        let mut expected: Vec<String> = LANGUAGE.iter().map(|name| (*name).to_owned()).collect();
        expected.push(grammar);
        expected.sort();
        let mut declared = dependencies_in(&manifest_text(language));
        declared.sort();
        assert_eq!(
            declared, expected,
            "{language}'s [dependencies] is not core.md §9's template. Every entry there is \
             forced by a signature: shared for the seam, similarity for the ranking, the \
             runtime because grammar() returns a tree_sitter::Language, and the grammar itself. \
             One more is what the next six languages inherit by copying"
        );

        let mut expected: Vec<String> = MEASURE.iter().map(|name| (*name).to_owned()).collect();
        expected.push(language.clone());
        expected.sort();
        let mut declared = dependencies_in(&manifest_text(&measure));
        declared.sort();
        assert_eq!(
            declared, expected,
            "{measure}'s [dependencies] is not core.md §9's template. It is the one crate that \
             may depend on both measure_core and a language, and it contains four lines — a \
             dependency it does not need is one every measure_<x> after it acquires"
        );

        let main = workspace_file(&format!("crates/{measure}/src/{measure}.rs"));
        let four_lines: Vec<&str> = main
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with("//"))
            .collect();
        assert_eq!(
            four_lines.len(),
            4,
            "crates/{measure}/src/{measure}.rs is {n} lines of code and core.md §7 calls it \
             four: the count is the claim — it is what makes a language measurable without any \
             other language building, and what stops per-language logic accumulating on the \
             measurement side of the seam. {four_lines:?}",
            n = four_lines.len()
        );

        assert!(
            path_dependencies.contains(language),
            "{language} is a workspace member and has no [workspace.dependencies] entry: \
             core.md §9 makes that one of the four manifest lines a language costs \
             (CHANGE-core-009), and without it the two crates that name it would each write a \
             version — which deps.md §14 is what stops"
        );

        for member in workspace_members() {
            let named = member
                .trim_start_matches("crates/")
                .trim_start_matches("vendor/");
            if named == "heuristic_jump" || named == measure || named == *language {
                continue;
            }
            assert!(
                !declares(&workspace_file(&format!("{member}/Cargo.toml")), language),
                "{member} declares {language}: core.md §9 prices a language at two crate \
                 directories and one line in heuristic_jump, and no crate other than \
                 heuristic_jump changes — a second crate that knows a language by name is the \
                 graph the section exists to keep flat"
            );
        }

        for template in [language.clone(), measure] {
            assert!(
                !workspace_path(&format!("crates/{template}/tests")).exists(),
                "crates/{template}/tests exists: core.md §9's template has **No tests**, \
                 because the corpus is the oracle and is made of real repositories nobody here \
                 wrote — a fixture directory beside the template is an invitation to fill it, \
                 and what it converts the oracle into is the thing a campaign optimises"
            );
        }
    }
}

/// `core.md` §8.4: `PositionEncoding` "reaches the dispatch wrapper and stops
/// there", which is §3's rule that no encoding crosses the handler seam, seen
/// from the outbound side.
///
/// The compiler enforces the inbound half — `Query` has no encoding field, so
/// a handler has nothing to read — and enforces none of the outbound half. A
/// language crate can reach `shared::proto` directly, build a `WirePosition`
/// with `encode`, and hand the driver something already in the negotiated
/// units; the answer would be right for whatever encoding that author assumed
/// and silently wrong for the one the two ends actually negotiated. Nothing
/// in the build would notice, because the types line up.
///
/// So the rule is asserted against the source: the wire vocabulary is not
/// nameable from a `lang_*` crate. `driver` and `shared` are where it lives,
/// and `measure_core` is an LSP client that legitimately encodes the position
/// it *sends* — what neither it nor a handler does is put an answer on a wire
/// (CHANGE-conformance-012).
#[test]
fn no_language_crate_can_name_the_wire_vocabulary() {
    const WIRE: &[&str] = &[
        "PositionEncoding",
        "WireLocation",
        "WirePosition",
        "WireRange",
        "shared::proto",
    ];

    let languages = language_members();
    assert!(
        !languages.is_empty(),
        "no crates/lang_* workspace member, so this test would pass vacuously"
    );

    for language in languages {
        for (file, source) in sources_of(&language) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );
            for name in WIRE {
                assert!(
                    !source.contains(name),
                    "{file} names {name}: core.md §8.4 hands the encoding to the dispatch \
                     wrapper and stops there, so a handler that can reach the wire \
                     vocabulary can encode an answer in units nobody negotiated"
                );
            }
        }
    }
}

/// `deps.md` §12, on the `lsp-types` oracle: "Dev only — it must never appear
/// in a non-dev dependency table, and that is worth a CI check, since the whole
/// point of §3 is defeated the moment a runtime `use lsp_types::` appears".
///
/// This is that check, and it is here rather than left to review because the
/// failure is invisible. A runtime `lsp_types::Position` typechecks, runs, and
/// does not look wrong beside the code around it — §3's whole argument is that
/// the newtype has to be what deserialization *produces*, and a conversion
/// layer a few functions inward makes the discipline optional again.
///
/// Both halves are asserted, since either alone can be defeated. The manifest
/// scan would miss a crate that reached the types transitively; the source
/// scan would miss a dependency declared and not yet used, which is the state
/// one commit before the first use.
#[test]
fn the_lsp_types_oracle_never_leaves_the_dev_dependencies() {
    let members = crate_members();
    assert!(
        !members.is_empty(),
        "no crates/* workspace member, so this test would pass vacuously"
    );

    let mut declared_anywhere = false;
    for member in &members {
        let text = manifest_text(member);
        assert!(
            !text.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );
        // A declaration, not the word: the manifest comment beside the
        // dev-dependency names it too, and a check that counted that would
        // survive the oracle being deleted — which the control run for this
        // test found it doing.
        declared_anywhere |= declares(&text, "lsp-types");

        for dependency in dependencies_in(&text) {
            assert!(
                dependency != "lsp-types",
                "{member} depends on lsp-types at runtime: deps.md §3 keeps it as a \
                 dev-dependency oracle only, because a foreign types crate can only produce \
                 our newtypes through a conversion layer *after* deserialization — which is \
                 the discipline §3 exists to make non-optional"
            );
        }

        for (file, source) in sources_of(member) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );
            assert!(
                !source.contains("lsp_types"),
                "{file} names lsp_types: the oracle belongs to the tests, and core.md §8.5 \
                 makes the differential the condition on which hand-rolled wire types are \
                 acceptable — not a set of types to reach for when one is inconvenient"
            );
        }
    }

    assert!(
        declared_anywhere,
        "no crate declares lsp-types at all, so core.md §10's differential test is not \
         running and §8.5's second mitigation is absent rather than satisfied"
    );
}

/// `deps.md` §5's licensing subsection, whose per-crate table is the whole
/// content of the claim: our crates are MIT even though the binary is GPL,
/// because vendoring GPL code does not transfer copyright in code we wrote and
/// MIT is GPL-3.0-compatible. What that buys is an option — a taker who
/// supplies a different text layer can lift the permissive part — and an
/// option is exactly the kind of thing that is lost silently.
///
/// §14 asks for a `cargo-deny` config to hold this: "asserting that `GPL`
/// reaches the graph only through `vendor/rope` and `crates/similarity` is
/// worth having from the start… the check is what notices a *third* arriving
/// without anyone deciding, which is how a licence surface grows — not by a
/// decision but by a dependency." A root `deny.toml` is not a path this loop
/// may write, so the claim is asserted here instead, against the manifests,
/// which is the same evidence `cargo-deny` would read.
#[test]
fn every_member_declares_the_licence_section_5_assigns_it() {
    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );

        let declared = licence_of(&manifest);
        assert_eq!(
            declared.as_deref(),
            Some(expected_licence(member)),
            "{member} declares license = {declared:?}: deps.md §5's table assigns it \
             {expected}, and the two answers differ for a reason — GPL is carried by \
             vendor/rope and crates/similarity, and a third source arriving without a \
             decision is what widens the licence surface",
            expected = expected_licence(member)
        );
    }
}

/// The one claim in §5's licensing subsection that is not about a manifest
/// field:
///
/// > That is a project-level commitment and it should be stated plainly in
/// > `high-level.md`, with `rope`'s license text shipped alongside.
///
/// > **There are two GPL inputs, not one.** An earlier revision of this section
/// > said `rope` was the only one, and treated keeping everything else
/// > permissive as an exit: replace `rope`, relicense nothing, and the
/// > workspace could go permissive. `crates/similarity` closes that exit for
/// > the handler layer.
///
/// Three documents have to agree for that to be true — `high-level.md`'s
/// licence section, §5's own table, and `expected_licence` above, which is
/// what the manifests are held to — and until this test they were compared by
/// nobody. `high-level.md` still carried the superseded position verbatim,
/// two campaigns after §5 recorded it as superseded (CHANGE-core-007). That is
/// the failure this catches, and the direction that matters is the *next* one:
/// a third GPL input arriving is a licence surface that grew by a dependency
/// rather than by a decision, and the crate that carries it would be correct
/// on its own while every summary of the project's position stayed wrong.
///
/// The GPL *inputs* are not the GPL *members*. `crates/lang_*` is GPL and is
/// not an input — §5 marks it by the dependency rule, downstream of
/// `similarity` — so the inputs are the GPL members that are not language
/// crates, which is the set this compares. `heuristic_jump` is the reverse
/// case and is why "reaches GPL" is not the rule: it depends on every `lang_*`
/// and is MIT, because a `license` field describes copyright in that crate's
/// own text (`conformance-014`).
#[test]
fn the_gpl_inputs_are_the_two_the_documents_name() {
    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    let inputs: Vec<String> = members
        .into_iter()
        .filter(|member| !member.starts_with("crates/lang_"))
        .filter(|member| {
            licence_of(&workspace_file(&format!("{member}/Cargo.toml")))
                .is_some_and(|licence| licence.starts_with("GPL"))
        })
        .collect();
    assert_eq!(
        inputs,
        vec!["crates/similarity".to_owned(), "vendor/rope".to_owned()],
        "the GPL inputs are {inputs:?}, and deps.md §5 names exactly two — vendor/rope, which \
         everything reaches through DocumentSnapshot, and crates/similarity, which is a port. \
         A third is the licence surface growing by a dependency rather than by a decision, \
         which is the shape §5 says the check exists to notice"
    );

    let commitment = section_of(&workspace_file("design/high-level.md"), "\n## License");
    assert!(
        commitment.contains("GPL-3.0-or-later"),
        "high-level.md's licence section does not name GPL-3.0-or-later, so either the section \
         moved or this test is reading nothing: deps.md §5 calls the binary's licence a \
         project-level commitment that should be stated plainly there"
    );
    for input in &inputs {
        assert!(
            commitment.contains(input.as_str()),
            "high-level.md's licence section does not name {input}: §5 has two GPL inputs and \
             not one, and the revision that named only rope treated replacing it as an exit to \
             a permissively licensable workspace — an exit that has not existed since \
             similarity was ported"
        );
    }
    // Positive rather than a ban on the superseded sentence. The section that
    // supersedes it quotes it — "an earlier version of this section said `rope`
    // was the only GPL input" — so a scan for that phrase fires on the fix as
    // readily as on the fault, which is how a check gets deleted rather than
    // repaired.
    assert!(
        commitment.contains("two GPL inputs, not one"),
        "high-level.md's licence section does not state that there are two GPL inputs and not \
         one: the superseded revision said rope was the only one and treated replacing it as \
         an exit to a permissively licensable workspace — an exit that has not existed since \
         similarity was ported, and the claim is what a reader deciding whether the permissive \
         part is liftable would act on"
    );

    let subsection = section_of(&workspace_file("design/deps.md"), "\n### Licensing:");
    assert!(
        subsection.contains("two GPL inputs, not one"),
        "deps.md §5's licensing subsection no longer states that there are two GPL inputs: it \
         is the source both the manifests and high-level.md are checked against here, and a \
         comparison whose fixture moved compares nothing"
    );
    for input in &inputs {
        assert!(
            subsection.contains(input.as_str()),
            "deps.md §5's licensing subsection does not name {input} while its manifest \
             declares GPL: the per-crate table is the whole content of the claim, so a crate \
             the section never placed is one nobody decided to make GPL"
        );
    }
}

/// The body of one markdown section, up to the next heading at the same
/// level, so a document can be a fixture the way `fenced_toml_of` makes §15's
/// `toml` block one. The heading is given with its leading newline, which is
/// what keeps `## License` from matching a link to it in a paragraph above.
fn section_of(document: &str, heading: &str) -> String {
    let Some((_, after)) = document.split_once(heading) else {
        return String::new();
    };
    let level = heading.trim_start().split(' ').next().unwrap_or("#").len();

    let mut body = String::new();
    for line in after.lines().skip(1) {
        let depth = line.len() - line.trim_start_matches('#').len();
        if depth > 0 && depth <= level && line.get(depth..).is_some_and(|r| r.starts_with(' ')) {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

/// The other half of §14's licensing convention, and the half that was
/// actually missing: "License texts live once at the workspace root and are
/// symlinked into each crate. Zed does this without exception — 245 symlinks
/// and not one regular copy."
///
/// Six of the seven `crates/*` had no licence text beside them at all, which
/// is the failure §14 describes with the copies absent rather than stale. The
/// symlink is asserted rather than the file, because a regular copy satisfies
/// "a crate directory that declares `license = "MIT"` should carry the text"
/// and fails the reason the convention exists: N copies drift, and a stale one
/// is a licensing problem rather than a formatting one.
///
/// It also guards the vendored crates, where §14 notes the practical
/// consequence: they *arrive* with these symlinks and `../../LICENSE-GPL`
/// resolves after the copy because `vendor/rope/` sits at the same depth
/// `crates/rope/` did — "provided the copy preserves them. Use `cp -a`; plain
/// `cp -r` dereferences, which silently turns each one into a 34 KB duplicate
/// and loses the property on the first re-sync."
#[test]
fn the_licence_text_is_symlinked_into_every_member() {
    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        let declared = licence_of(&manifest);
        let text = match declared.as_deref() {
            Some("MIT") => "LICENSE-MIT",
            Some("GPL-3.0-or-later") => "LICENSE-GPL",
            Some("Apache-2.0") => "LICENSE-APACHE",
            _ => "",
        };
        assert!(
            !text.is_empty(),
            "{member} declares license = {declared:?}, which names no text at the workspace \
             root: deps.md §14 keeps one copy of each and symlinks it, so a fourth licence \
             needs a fourth file at the root before it needs a fourth crate"
        );

        let path = workspace_path(&format!("{member}/{text}"));
        assert!(
            std::fs::symlink_metadata(&path)
                .ok()
                .is_some_and(|meta| meta.is_symlink()),
            "{member}/{text} is missing, or is a regular file rather than a symlink: \
             deps.md §14 keeps one copy of each licence text at the workspace root, \
             because N copies drift and a stale one is a licensing problem rather than a \
             formatting one"
        );
        assert!(
            path.exists(),
            "{member}/{text} is a symlink that does not resolve: §14's note on re-syncing \
             is that the link survives the copy only if it is copied as a link, and one \
             that points nowhere carries no licence text at all"
        );
    }
}

/// `deps.md` §6, which splits one crate family into two rules that are
/// opposites and are easy to apply to the wrong half.
///
/// > The tree-sitter **runtime** version is ours to choose and tracks a recent
/// > release; `high-level.md`'s requirement is that the *grammars* match Zed's
/// > pinned revisions, not that the runtime does.
///
/// The document's preamble calls this the one pin "that is a design constraint
/// rather than a resolution detail", and `CLAUDE.md` states it as a hard
/// constraint: "Tree-sitter grammars are pinned to the revisions Zed uses. Do
/// not bump a grammar crate."
///
/// A caret range is what defeats it, quietly. `tree-sitter-rust = "0.24"`
/// looks like a pin and is not: cargo takes the newest 0.24.x on any
/// `cargo update`, so the grammar Zed pinned and the grammar we parse with
/// drift apart without a diff anywhere, and the first symptom is a corpus
/// number that moved for no reason in the commit range. So the assertion is
/// that every grammar names all three components, or a git revision — which is
/// what §6 says the two exceptions will need, `tree-sitter-typescript` and
/// `tree-sitter-cpp`.
///
/// The runtime is asserted *not* to be pinned that way, which is the half that
/// makes this a real check rather than "everything is exact": the two rules
/// point in opposite directions, and a scan that could not tell them apart
/// would pass a workspace that had pinned the runtime and floated the grammar.
///
/// §6's third claim, `[profile.dev.package.tree-sitter] opt-level = 3`, is
/// here because it is a measurement precondition rather than a convenience:
/// "parsing in a debug build is otherwise slow enough to distort every latency
/// observation made while developing", which for this project would mean
/// tuning against a fiction.
#[test]
fn the_grammars_are_pinned_and_the_runtime_is_not() {
    let declared = table_of(&workspace_file("Cargo.toml"), "workspace.dependencies");

    let grammars: Vec<&String> = declared
        .iter()
        .filter(|line| line.starts_with("tree-sitter-"))
        .collect();
    assert!(
        !grammars.is_empty(),
        "no tree-sitter-* grammar in [workspace.dependencies], so this test would pass \
         vacuously"
    );

    for grammar in grammars {
        let version = grammar
            .split('"')
            .nth(1)
            .map(str::to_owned)
            .unwrap_or_default();
        let pinned = grammar.contains("rev = ") || version.split('.').count() == 3;
        assert!(
            pinned,
            "the grammar `{grammar}` names {version:?}, which is a range rather than a pin: \
             cargo takes the newest matching release on any update, so the revision Zed pinned \
             and the one we parse with drift apart with no diff anywhere — and the first \
             symptom is a corpus number that moved for no reason"
        );
    }

    let runtime = declared
        .iter()
        .find(|line| line.starts_with("tree-sitter "))
        .or_else(|| {
            declared
                .iter()
                .find(|line| line.starts_with("tree-sitter="))
        })
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        !runtime.is_empty(),
        "the tree-sitter runtime is not declared in [workspace.dependencies], so the \
         distinction this test exists to make is unreadable"
    );
    assert_ne!(
        runtime.split('"').nth(1).map(|v| v.split('.').count()),
        Some(3),
        "the tree-sitter runtime is pinned as `{runtime}`: deps.md §6 makes the runtime ours \
         to choose and tracks a recent release, and it is the *grammars* that match Zed's \
         revisions — the two meet through tree-sitter-language's ABI rather than through a \
         version constraint, which is what lets them differ"
    );

    // The entry is in Zed's inline form, under one `[profile.dev.package]`
    // table rather than a table per package, since §14's first claim is that
    // the workspace manifest follows Zed's conventions. `§14`'s own test holds
    // the whole list; this holds the one entry §6 asks for.
    let manifest = workspace_file("Cargo.toml");
    assert!(
        table_of(&manifest, "profile.dev.package")
            .iter()
            .any(|line| line
                .replace(' ', "")
                .starts_with("tree-sitter={opt-level=3")),
        "no `[profile.dev.package.tree-sitter] opt-level = 3`: deps.md §6 wants it because \
         parsing in a debug build is otherwise slow enough to distort every latency \
         observation made while developing, which would mean tuning against a fiction"
    );
}

/// `deps.md` §4's two feature decisions, which are the whole of what a
/// manifest can carry about JSON and are both invisible when wrong.
///
/// > `raw_value` is not optional here. Frame classification needs `method` and
/// > `id` out of a frame we are otherwise forwarding untouched … which borrows
/// > from the frame buffer and allocates nothing. Deserializing to
/// > `serde_json::Value` instead would allocate a whole tree per frame, which
/// > `shim.md` §1 budgets at "one message-copy."
///
/// `Cargo.toml`'s comment beside the dependency names the failure mode as "a
/// `serde_json` without it silently compiles", and **that has stopped being
/// true**, which the control run for this test is how we know: dropping the
/// feature now fails to build, because `shared` and `measure_core` already
/// `use serde_json::value::RawValue`. The claim was accurate when §4 was
/// written and there were no users; a feature is only silent until something
/// imports what it gates.
///
/// The assertion is kept rather than deleted, for the reason the vendored half
/// of `the_workspace_lints_reach_our_crates_and_not_the_vendored_ones` is
/// kept: the compiler's enforcement here is incidental. It holds while those
/// imports exist, and `RawValue` is exactly the kind of thing a refactor
/// moves.
///
/// The other half is a feature that must stay *off*:
///
/// > Deliberately **not** enabling `preserve_order` (Zed does). We never
/// > re-serialize a forwarded frame, so map order cannot leak, and
/// > `preserve_order` swaps in `indexmap` for no benefit.
///
/// That one is worth asserting because features unify across the graph: any
/// crate anywhere enabling it turns it on for everyone, and the only visible
/// consequence is `indexmap` in the lockfile — which is already there through
/// `toml` and `criterion`, so its presence proves nothing either way.
#[test]
fn serde_json_carries_raw_value_and_not_preserve_order() {
    let declared = table_of(&workspace_file("Cargo.toml"), "workspace.dependencies")
        .into_iter()
        .find(|line| line.starts_with("serde_json"))
        .unwrap_or_default();
    assert!(
        !declared.is_empty(),
        "serde_json is not declared in [workspace.dependencies], so both assertions below \
         would be vacuous"
    );

    assert!(
        declared.contains("\"raw_value\""),
        "serde_json is declared as `{declared}` without the raw_value feature: deps.md §4 \
         calls it not optional, because frame classification borrows `method` and `id` out of \
         a frame being forwarded untouched — and the failure is silent, since a serde_json \
         without it compiles and RawValue is simply never in scope"
    );
    assert!(
        !declared.contains("preserve_order"),
        "serde_json is declared as `{declared}` with preserve_order: deps.md §4 declines it \
         deliberately — we never re-serialize a forwarded frame, so map order cannot leak, and \
         it swaps in indexmap for no benefit. Features unify across the graph, so one crate \
         enabling it turns it on for every crate"
    );
}

/// `deps.md` §12's table, which places each testing crate, plus the two pins
/// its rows argue for and the four crates it declines.
///
/// The placements are narrow and the narrowness is the claim.
/// `criterion` is "`vendor/rope`'s benchmark only, per §5" — it exists in this
/// workspace to answer one question, whether the newtype wrappers and the
/// `*_raw` indirection inline away, and a second user would mean somebody
/// started benchmarking our own code, which §12 says explicitly is not
/// planned. `rand` is there for "upstream rope/sum_tree tests, kept per §5",
/// which is why it belongs to the vendored crates and not to ours.
///
/// The pins are two of the three the document's preamble calls still
/// load-bearing, and both have a reason that a bare version number does not
/// carry. `rand` is "pinned to Zed's 0.9 rather than crates.io's 0.10: the
/// tests are kept verbatim and are written against `rng.random_range(..)`.
/// Taking 0.10 would mean editing test bodies, which defeats keeping them" —
/// and keeping them is what `rope-modifications.md` §7 relies on as the
/// independent check on the newtype sweep. `lsp-types` is held below the
/// release that swapped `url::Url` for `fluent-uri`, because "an oracle whose
/// URIs are a different type compares nothing where percent-encoding and
/// drive letters live, which is where the bugs actually are". A routine
/// version bump defeats either one silently.
///
/// `tempfile` is now on the declined list rather than the placed one, on the
/// ruling on `conformance-015` (CHANGE-conformance-015). §12 had placed it in
/// the `ProjectView` scope tests; that suite exists and builds its fixtures
/// under `CARGO_TARGET_TMPDIR`, and the stale-fixture guarantee the crate was
/// chosen for is already held, because `fixture()` clears on entry.
#[test]
fn the_testing_crates_are_placed_where_section_12_puts_them() {
    /// §12's "Deliberately not adding". `tempfile` joined it on the ruling on
    /// `conformance-015`: the `ProjectView` scope fixtures build under
    /// `CARGO_TARGET_TMPDIR`, whose `fixture()` helper already clears on
    /// entry, so the stale-fixture guarantee `tempfile` was chosen for is held
    /// without it — and the directory surviving a failure is worth more here
    /// than cleanup on drop (CHANGE-conformance-015).
    const DECLINED: &[&str] = &[
        "mockall",
        "pretty_assertions",
        "arbitrary",
        "cargo-fuzz",
        "tempfile",
    ];

    /// §12's table. Every one is a testing crate, so in a crate of ours it
    /// belongs in a dev table — `lsp-types` most of all, which
    /// `the_lsp_types_oracle_never_leaves_the_dev_dependencies` asserts on its
    /// own grounds. The vendored crates are exempt: `sum_tree` carries
    /// `proptest` as an optional runtime dependency behind its `test-support`
    /// feature, which is upstream's shape and not ours to correct.
    const TESTING: &[&str] = &["insta", "proptest", "rand", "criterion", "lsp-types"];

    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    let mut criterion_declared_by = Vec::new();
    let mut rand_declared_by = Vec::new();
    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );

        for (table, key, _) in dependency_entries(&manifest) {
            let name = key.strip_suffix(".workspace").unwrap_or(&key);
            assert!(
                !DECLINED.contains(&name),
                "{member} declares {name} in [{table}], and deps.md §12 declines it: the fake \
                 child is a scripted frame list, which is a plain struct, and insta covers the \
                 cases where diff quality actually matters"
            );

            if name == "criterion" {
                criterion_declared_by.push(member.clone());
            }
            if name == "rand" {
                rand_declared_by.push(member.clone());
            }
            if TESTING.contains(&name) && !member.starts_with("vendor/") {
                assert!(
                    table.contains("dev-dependencies"),
                    "{member} declares {name} in [{table}] rather than a dev table: deps.md \
                     §12 places it in the tests, and a testing crate in the runtime graph is \
                     one the shipped binary links for nothing"
                );
            }
        }
    }

    assert_eq!(
        criterion_declared_by,
        vec!["vendor/rope".to_owned()],
        "deps.md §12 gives criterion to vendor/rope's benchmark only, and §5 gives the reason \
         — it answers whether the newtype wrappers and the *_raw indirection inline away. §12 \
         is explicit that no benchmark of our own code is planned, so a second user is a \
         decision nobody recorded"
    );
    assert_eq!(
        rand_declared_by,
        vec!["vendor/rope".to_owned(), "vendor/sum_tree".to_owned()],
        "deps.md §12 gives rand to the upstream rope/sum_tree tests kept per §5, and those \
         are the vendored crates"
    );

    assert!(
        workspace_version("rand").starts_with("0.9"),
        "rand is at {v}, and deps.md §12 holds it at Zed's 0.9 rather than crates.io's 0.10: \
         upstream's tests are kept verbatim and are written against rng.random_range(..), so a \
         bump means editing test bodies, which defeats keeping them — and keeping them is what \
         rope-modifications.md §7 rests the newtype sweep on",
        v = workspace_version("rand")
    );
    assert!(
        workspace_version("lsp-types").starts_with("0.94"),
        "lsp-types is at {v}, and deps.md §3 holds it below the release that swapped url::Url \
         for fluent-uri: the point of the oracle is that it produces the same Url we do, and \
         one whose URIs are a different type compares nothing where percent-encoding and drive \
         letters live",
        v = workspace_version("lsp-types")
    );
    assert!(
        workspace_version("criterion").starts_with("0.5"),
        "criterion is at {v}, and deps.md §12 pins Zed's 0.5 since the benchmark is kept \
         verbatim",
        v = workspace_version("criterion")
    );
}

/// The version `[workspace.dependencies]` resolves a crate at, whether written
/// bare or inside a table. Empty when the crate is not declared, which every
/// caller reports as a failed pin rather than passing silently.
fn workspace_version(name: &str) -> String {
    table_of(&workspace_file("Cargo.toml"), "workspace.dependencies")
        .iter()
        .find(|line| line.split(['.', ' ', '=']).next() == Some(name))
        .and_then(|line| {
            let rest = line.split_once('=')?.1;
            let rest = rest.split_once("version").map_or(rest, |(_, after)| after);
            let (_, quoted) = rest.split_once('"')?;
            Some(quoted.split_once('"')?.0.to_owned())
        })
        .unwrap_or_default()
}

/// `deps.md` §1, "Async runtime: none", which `CLAUDE.md` restates as a hard
/// constraint: "Do not add `async fn`, `.await`, or any executor."
///
/// The manifest half is `no_member_declares_a_crate_section_13_rejects` above,
/// since §13 carries `tokio` forward from here. This is the source half, and
/// it exists because the two failures look nothing alike. A `tokio` line in a
/// manifest is a decision someone made; an `async fn` is a shape that spreads
/// — §1's whole argument is structural rather than about crate count:
///
/// > The five pipe threads each do a blocking read or write on one fd forever.
/// > A dedicated OS thread is the natural expression of that; `async` buys
/// > nothing when a task never yields for any reason other than the one fd it
/// > owns.
///
/// `std::sync::mpsc` is scanned for too, which is the entry most likely to be
/// reached for innocently. §1 rejects it not on principle but on two specific
/// capabilities: "we need `select!` over (editor events, child events, worker
/// results, timer) in one loop, and std has no select. Also crossbeam gives
/// `Receiver::len()`, which `shim.md` §10's 'no heuristic work while `core` is
/// behind' rule needs to be able to read."
///
/// The `.await` scan requires a non-identifier character after it. Written
/// without that, it matches `frame.awaiting_reply()` in
/// `measure_core/src/client.rs` — found on the first run, and the kind of
/// false positive that gets a scan deleted rather than fixed.
#[test]
fn no_crate_of_ours_is_async_shaped() {
    const EXECUTORS: &[&str] = &[
        "tokio",
        "smol",
        "async-std",
        "futures",
        "futures-util",
        "async-lsp",
        "tower-lsp",
        "tower-lsp-server",
    ];

    let members = crate_members();
    assert!(
        !members.is_empty(),
        "no crates/* workspace member, so this test would pass vacuously"
    );

    for member in &members {
        let manifest = manifest_text(member);
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );
        for (table, key, _) in dependency_entries(&manifest) {
            let name = key.strip_suffix(".workspace").unwrap_or(&key);
            assert!(
                !EXECUTORS.contains(&name),
                "{member} declares {name} in [{table}]: deps.md §1 has no async runtime, and \
                 an executor between our bytes and the pipe is the class of 'why is this 3ms \
                 late' question a blocking read on a dedicated thread does not have"
            );
        }

        for (file, source) in sources_of(member) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );
            for shape in ["async fn", "async move", "async {"] {
                assert!(
                    !source.contains(shape),
                    "{file} contains `{shape}`: deps.md §1 is six long-lived threads and a \
                     CPU-bound pool, and nothing in it is async-shaped — `core` is a single \
                     thread in a recv loop whose whole point is that it is serial"
                );
            }
            assert!(
                !awaits(&source),
                "{file} contains `.await`: CLAUDE.md names this a hard constraint, and \
                 deps.md §1's reason is that a task here never yields for anything but the \
                 one fd it owns"
            );
            assert!(
                !source.contains("sync::mpsc"),
                "{file} uses std::sync::mpsc: deps.md §1 rejects it for two capabilities \
                 rather than on principle — `select!` over four receivers in one loop, which \
                 std has none of, and `Receiver::len()`, which shim.md §10's shed-load rule \
                 has to be able to read"
            );
        }
    }
}

/// Whether the source contains a real `.await`, as opposed to a method whose
/// name merely starts that way. `frame.awaiting_reply()` is one, and is why
/// this is not a `contains`.
fn awaits(source: &str) -> bool {
    source.match_indices(".await").any(|(at, _)| {
        source[at + ".await".len()..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '_')
    })
}

/// `deps.md` §15 is not prose about the lint configuration — it *is* the lint
/// configuration, printed as a `toml` block that `Cargo.toml`'s
/// `[workspace.lints]` tables are meant to be. So the document is the fixture
/// here, rather than a list transcribed from it into this file: the two are
/// compared directly, and a third copy would be the thing that drifts.
///
/// That matters more for §15 than for the other sections in this file, because
/// its content is a set of reasons attached to individual lints, and the reason
/// is what makes each one re-derivable. `iter_over_hash_type` is denied because
/// "hash iteration order varies between executions of the same program on the
/// same hardware. This project is a measurement harness with insta snapshots,
/// JSONL corpus records and candidate ranking." `integer_division` and
/// `float_cmp` are denied because "silent truncation produces
/// plausible-looking wrong numbers, which is the worst failure for a metric."
/// A lint quietly dropped from `Cargo.toml` takes its argument with it, and
/// the argument is not recoverable from the diff.
///
/// Levels are compared, not just names: `redundant_clone` at `warn` rather
/// than `deny` is a decision §15 spends a paragraph on — it is a nursery lint
/// with known false positives, kept because it is the one that would catch a
/// `Rope`/`Tree` clone that is not cheap, but not at a level that breaks the
/// build.
#[test]
fn the_workspace_lints_are_the_ones_section_15_prints() {
    let printed = fenced_toml_of(
        &workspace_file("design/deps.md"),
        "## 15. Clippy in workspace",
    );
    assert!(
        printed.contains("[workspace.lints.clippy]"),
        "no toml block found under deps.md §15, so this test would compare nothing"
    );

    let manifest = workspace_file("Cargo.toml");
    let mut compared = 0;
    for table in ["workspace.lints.rust", "workspace.lints.clippy"] {
        let spec = lint_entries(&printed, table);
        let real = lint_entries(&manifest, table);
        assert!(
            !spec.is_empty(),
            "deps.md §15 prints no [{table}] entries, so this comparison is vacuous"
        );

        for (lint, level) in &spec {
            let found = real.iter().find(|(name, _)| name == lint);
            assert_eq!(
                found.map(|(_, level)| level),
                Some(level),
                "Cargo.toml sets {lint} to {found:?} and deps.md §15 prints {level}: §15 is \
                 the configuration rather than prose about it, and each entry carries the \
                 reason it exists — a level that drifts takes the argument with it, and the \
                 argument is not recoverable from the diff"
            );
            compared += 1;
        }
        for (lint, _) in &real {
            assert!(
                spec.iter().any(|(name, _)| name == lint),
                "Cargo.toml sets {lint} and deps.md §15 does not print it: §15 states the \
                 convention that each entry carries a comment saying why, and a lint the \
                 section never argued for has nowhere to carry one"
            );
        }
    }

    assert!(
        compared > 40,
        "only {compared} lints compared, and §15 prints more than that — the block or the \
         manifest is not being read"
    );
}

/// `deps.md` §0's summary table, whose middle column is a claim per crate and
/// the only place the whole dependency set is written down at once.
///
/// Scoped as §0 scopes itself: "the core driver only — `shared`, `driver`,
/// `heuristic_jump`, plus the vendored text crates. `similarity` and `lang_*`
/// dependencies are named where they are already implied, but not settled
/// here." So `measure_core`, `measure_rust`, `lang_rust` and `similarity` are
/// out of this test's reach even though the table names `toml` for the first
/// of them — their edges are `core.md` §9's, asserted above.
///
/// A **subset**, not an equality, for the reason
/// `shared_declares_only_the_dependencies_section_9_lists` gives and §14
/// states: "each arrives with its first user", so a table entry not yet
/// declared is the intended state. Several are in exactly that state right now
/// — `rayon`, `lru`, `insta`, `tempfile` and `notify` are all chosen by the
/// table and declared by nobody, because the code that needs them is not
/// written. What is being caught is the other direction: a crate that acquires
/// a dependency the table never placed there, which is how a dependency set
/// stops being the one a document argued for.
///
/// Grammar crates are exempt. §6 puts them out of scope — "Grammar crates are
/// `lang_*` business and out of scope here" — and the rule that actually
/// governs them is `core.md` §9's, which
/// `neither_driver_nor_shared_depends_on_a_language` asserts.
#[test]
fn the_core_crates_declare_only_what_section_0_places_there() {
    /// §0's table, transposed: the `Where` column read per crate rather than
    /// per dependency. Dev-only entries carry no `Where` in §0 and take their
    /// homes from §12, so they are permitted anywhere in this set.
    const PLACED: &[(&str, &[&str])] = &[
        (
            "crates/driver",
            &[
                "crossbeam-channel",
                "rayon",
                "serde",
                "serde_json",
                "tree-sitter",
                "notify",
                "lru",
                "tracing",
                "rustc-hash",
            ],
        ),
        (
            "crates/shared",
            &[
                "rayon",
                "serde",
                "serde_json",
                "url",
                "tree-sitter",
                "ignore",
                "thiserror",
                "tracing",
                "rustc-hash",
            ],
        ),
        (
            "crates/heuristic_jump",
            &["clap", "tracing", "tracing-subscriber"],
        ),
        (
            "vendor/rope",
            &[
                "heapless",
                "unicode-segmentation",
                "log",
                "rayon",
                "tracing",
            ],
        ),
        (
            "vendor/sum_tree",
            &["heapless", "log", "rayon", "tracing", "proptest"],
        ),
    ];

    /// §0's rows with no `Where`, placed by §12 instead.
    const TESTING: &[&str] = &[
        "insta",
        "rand",
        "criterion",
        "proptest",
        "tempfile",
        "lsp-types",
    ];

    let internal = workspace_path_dependencies();
    assert!(
        internal.contains(&"shared".to_owned()),
        "no path dependencies parsed out of [workspace.dependencies], so every crates/* edge \
         below would be reported as an unplaced third-party dependency"
    );

    for (member, placed) in PLACED {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );

        for (table, key, _) in dependency_entries(&manifest) {
            let name = key.strip_suffix(".workspace").unwrap_or(&key).to_owned();
            if internal.contains(&name) || name.starts_with("tree-sitter-") {
                continue;
            }
            assert!(
                placed.contains(&name.as_str()) || TESTING.contains(&name.as_str()),
                "{member} declares {name} in [{table}], and deps.md §0's table does not place \
                 it there: the table is the one place the dependency set is written down at \
                 once, so a crate reaching for something outside it is the set drifting from \
                 the document that argued for it"
            );
        }
    }
}

/// `deps.md` §13, "Explicitly not depended on", which is a list of decisions
/// rather than a list of crates — each name is there because some section
/// argued it away, and §0's summary table repeats four of them in its verdict
/// column.
///
/// Two of the entries carry their own reason for existing as a *test* rather
/// than a note:
///
/// > **`parking_lot`** — `shim.md` §2 states there is no lock anywhere. If a
/// > `parking_lot` import ever appears, something has gone wrong
/// > architecturally and the fix is not a faster mutex.
///
/// > **`once_cell`** — … A `OnceLock` on the query path would have been a
/// > blocking primitive in a design that says it has none.
///
/// Those are the shape the whole list has: the dependency is not the problem,
/// it is the evidence. Adding `anyhow` does not break anything either — it
/// makes §10's closed error set stop being closed, quietly, one `?` at a time.
///
/// **Asserted against declarations, not against `Cargo.lock`.** Six of these
/// names are in the lockfile right now — `once_cell`, `regex`, `memchr`,
/// `aho-corasick`, `walkdir` and `indexmap` — reached transitively through
/// `ignore`, `toml`, `proptest`, `criterion` and `tracing-subscriber`. §13 is
/// about what we reach for, and a scan of the lock would be red on arrival and
/// would stay red for reasons nobody can act on.
///
/// The two qualified entries are qualified in §13's own words. `regex`:
/// "`DefinitionHints` in `resolution.md` wants it, so it will land in a
/// `lang_*` crate. Nothing in the driver needs it." And the scan family:
/// "the literal scan primitive is a handler's. `driver` executes the scan on
/// its pool but the matching itself lives behind that seam. `memchr` is the
/// likely pick when we get there." So both are banned everywhere except
/// `crates/lang_*`, which is where they are expected to arrive.
#[test]
fn no_member_declares_a_crate_section_13_rejects() {
    /// §13's list, plus the four §0's table marks rejected. The rejections
    /// argued in §4, §5, §8, §9, §11 and §12 — `simd-json`, `ropey`,
    /// `schnellru`, `env_logger`, `lexopt`, `mockall` — are deliberately not
    /// here: they are alternatives a section weighed, not commitments it made,
    /// and folding them in would make this scan a second copy of six sections
    /// rather than a check on one.
    const REJECTED: &[&str] = &[
        "tokio",
        "anyhow",
        "num_cpus",
        "once_cell",
        "parking_lot",
        "dashmap",
        "jiff",
        "chrono",
        "time",
        "gix",
        "git2",
    ];

    /// Rejected for the driver and expected in a handler, per §13.
    const HANDLER_ONLY: &[&str] = &["regex", "memchr", "aho-corasick", "grep-searcher"];

    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );
        let handler = member.starts_with("crates/lang_");

        for (table, key, _) in dependency_entries(&manifest) {
            let name = key.strip_suffix(".workspace").unwrap_or(&key);
            assert!(
                !REJECTED.contains(&name),
                "{member} declares {name} in [{table}]: deps.md §13 rejects it, and the \
                 dependency is the evidence rather than the problem — §13's entries are there \
                 because a section argued each one away, and the argument is what stops \
                 holding when the crate arrives"
            );
            assert!(
                handler || !HANDLER_ONLY.contains(&name),
                "{member} declares {name} in [{table}]: deps.md §13 puts the literal scan \
                 primitive and the regex behind the handler seam — driver executes the scan \
                 on its pool, but the matching itself is a lang_* crate's, and a driver that \
                 can match is one the seam no longer describes"
            );
        }
    }
}

/// `deps.md` §14's first two conventions, which are one mechanism:
///
/// > **Every dependency version lives in `[workspace.dependencies]`.** Member
/// > crates never name a version. This is what stops the vendored crates and
/// > ours from resolving two copies of `heapless` or `rayon`.
///
/// > **Members reference deps as `foo.workspace = true`**, the dotted form, not
/// > `foo = { workspace = true }`. The braced form appears only when the member
/// > adds something — `util = { workspace = true, features = [...] }`.
///
/// The stated consequence is the reason this is asserted rather than reviewed.
/// A member that names `rayon = "1.8"` resolves, builds, and passes every test
/// in the suite; what it costs is that `vendor/rope` and `crates/shared` can
/// now compile against two different `rayon`s, and the first symptom of that
/// is a type error in an unrelated crate months later. Nothing about the
/// second convention is cosmetic either: the braced form is where a version
/// would go, so requiring the dotted one unless something *else* is added
/// makes the exception visible.
#[test]
fn no_member_names_a_dependency_version_of_its_own() {
    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    let mut entries = 0;
    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );

        for (table, key, value) in dependency_entries(&manifest) {
            entries += 1;
            let where_ = format!("{member}'s [{table}] entry `{key} = {value}`");

            if let Some(name) = key.strip_suffix(".workspace") {
                assert_eq!(
                    value, "true",
                    "{where_} writes the dotted form and then does not inherit: \
                     deps.md §14 has every version live in [workspace.dependencies], and \
                     `{name}` here names something else"
                );
                continue;
            }

            assert!(
                value.starts_with('{') && value.contains("workspace = true"),
                "{where_} does not inherit from [workspace.dependencies]: deps.md §14 is \
                 what stops the vendored crates and ours from resolving two copies of \
                 heapless or rayon, and two copies show up as a type error in an unrelated \
                 crate rather than as anything here"
            );
            assert!(
                value.matches('=').count() > 1,
                "{where_} uses the braced form and adds nothing: deps.md §14 keeps the \
                 dotted `{key}.workspace = true` unless the member adds a feature or a \
                 flag, because the braced form is where a version would go and the \
                 exception should be visible"
            );
        }
    }

    assert!(
        entries > 20,
        "only {entries} dependency entries across {} members, which is fewer than the \
         manifests have — the table scan is not reading them",
        members.len()
    );
}

/// `deps.md` §14 on lints, where the *absence* is as deliberate as the
/// presence:
///
/// > **`[lints] workspace = true` in every member we wrote** — one place, no
/// > `#![deny(...)]` scattered in `lib.rs` files.
///
/// > **`vendor/*` does not inherit them**, and this is deliberate rather than
/// > an oversight to fix later. Those crates are 7,400 lines of someone else's
/// > text-datastructure code, plus upstream tests kept *verbatim* … Bending
/// > them to `unwrap_used`, `panic`, and the `cast_*` family would be a large
/// > amount of work that buys no correctness, and every line of it would widen
/// > the re-sync diff.
///
/// The `crates/*` half fails silently and this scan is what catches it: a
/// crate created without the table compiles under no lints at all and the gate
/// stays green, because `-D warnings` only denies what some lint level turned
/// on. Deleting `shared`'s `[lints]` is the control, and it fires.
///
/// **The `vendor/*` half has no negative control, and the reason is worth more
/// than the assertion.** Every mutation that would violate it is rejected
/// before a test runs. Adding `[lints]` beside `vendor/rope`'s existing
/// `[lints.rust]` is a duplicate key cargo refuses to parse; replacing those
/// tables outright, or adding one to `vendor/sum_tree`, makes the crate stop
/// compiling — `elided_lifetimes_in_paths` and `unused_qualifications` alone
/// produce dozens of errors across upstream's text, before `unwrap_used` or
/// the `cast_*` family is reached.
///
/// That is §14's own claim, measured rather than argued: "Bending them to
/// `unwrap_used`, `panic`, and the `cast_*` family would be a large amount of
/// work that buys no correctness, and every line of it would widen the re-sync
/// diff." The assertion is kept anyway, because the compiler's enforcement is
/// incidental — it holds only while upstream's text happens to trip a denied
/// lint, and a future re-sync that arrives clean would leave nothing else
/// saying the exemption was deliberate.
#[test]
fn the_workspace_lints_reach_our_crates_and_not_the_vendored_ones() {
    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so this test would pass vacuously"
    );

    for member in &members {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        // Inside the `[lints]` table specifically. Scanning for the two lines
        // anywhere in the manifest passes on one where they are unrelated —
        // and, found by the control run for this test, cannot be exercised at
        // all on `vendor/rope`: its `[lints.rust]` and `[lints.clippy]` tables
        // make an *added* `[lints]` a duplicate key, which cargo rejects
        // before a single test runs. A control that produces no test result is
        // not a control.
        let inherits = table_of(&manifest, "lints")
            .iter()
            .any(|line| line == "workspace = true");

        if member.starts_with("vendor/") {
            assert!(
                !inherits,
                "{member} inherits [lints] from the workspace: deps.md §14 keeps the rules on \
                 crates/* only, so upstream's text stays unedited and the re-sync diff stays \
                 readable — a vendored crate under unwrap_used and the cast_* family is a \
                 large edit that buys no correctness"
            );
        } else {
            assert!(
                inherits,
                "{member} has no `[lints] workspace = true`: deps.md §14 puts the rules in one \
                 place rather than scattering #![deny(...)] through lib.rs files, and a crate \
                 that inherits none of them compiles clean under -D warnings because nothing \
                 turned a lint on"
            );
        }
    }
}

/// `deps.md` §14: "**Explicit `[lib] path`.** Zed writes
/// `path = "src/rope.rs"` rather than relying on `src/lib.rs`. We keep this for
/// the vendored crates because it is how they arrive, **and for our own crates
/// too**, per `CLAUDE.md`" — which states it as a rule for creating a crate,
/// "for a descriptive library root name", and which `core.md` §9's
/// language-crate template already assumes (`src/lang_rust.rs`, not `lib.rs`).
///
/// Asserted here because the failure is a non-event: a crate created the
/// default way has a `src/lib.rs`, no `[lib]` table, and builds. Nothing
/// notices until someone opens the directory. `sources_of` above also depends
/// on the convention — it derives a crate's root from its name — so a crate
/// that broke it would quietly make three of the scans in this file read an
/// empty string instead of a source file.
#[test]
fn every_library_names_its_root_explicitly() {
    for member in workspace_members() {
        let manifest = workspace_file(&format!("{member}/Cargo.toml"));
        if !manifest.contains("[lib]") {
            // A binary-only crate. `[[bin]]` carries its own path, and both of
            // ours state one because cargo would otherwise name the artifact
            // after the package (CHANGE-conformance-001).
            assert!(
                manifest.contains("[[bin]]"),
                "{member} declares neither [lib] nor [[bin]], so it builds a default target \
                 from a path nothing names"
            );
            continue;
        }

        let named = manifest
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("path = "))
            .map(|path| path.trim_matches('"').to_owned())
            .unwrap_or_default();
        assert!(
            !named.is_empty() && !named.ends_with("lib.rs"),
            "{member}'s [lib] names {named:?}: deps.md §14 and CLAUDE.md both want an explicit \
             descriptive root rather than src/lib.rs, and the scans in this file derive a \
             crate's sources from that convention"
        );
    }
}

/// `deps.md` §14: "**`[profile.dev.package]` opt-level bumps for the crates
/// that dominate debug runtime.** Zed sets `tree-sitter` and `serde_json` to
/// `opt-level = 3`, plus the proc-macro crates. We take exactly those".
///
/// Two directions, because "exactly" is a claim in both: a package the section
/// names and the manifest does not is a debug build that parses at the speed
/// §6 says distorts every latency observation made while developing, and a
/// package the manifest names and the section does not is the profile growing
/// by imitation, which is what "exactly those" refuses.
///
/// The proc-macro three are Zed's `# proc-macros start/end` block minus its
/// seven own crates, which are not in our graph at all.
///
/// The graph-membership half is the part that is not a transcription. Cargo
/// **warns** on a spec that matches nothing — "profile package spec `x` in
/// profile `dev` did not match any packages" — and builds anyway, so a bump
/// for a crate that was renamed, dropped, or never arrived reads as applied
/// forever. It is exactly what happened here: this table carried a comment
/// saying `serde_json`'s bump was waiting because "cargo rejects a profile
/// override naming a package that is not in the graph, and nothing depends on
/// it yet", where `serde_json` had been a dependency of three crates for some
/// time and cargo would not have rejected it either way.
#[test]
fn every_package_section_14_names_gets_its_opt_level() {
    // Sorted, and deliberately a second copy of the manifest's list: with only
    // a membership check, deleting a line from the manifest to make this pass
    // is the failure the test claims to catch.
    const BUMPED: [&str; 5] = ["proc-macro2", "quote", "serde_json", "syn", "tree-sitter"];

    let manifest = workspace_file("Cargo.toml");
    let mut bumped: Vec<String> = table_of(&manifest, "profile.dev.package")
        .into_iter()
        .filter_map(|entry| {
            let (package, value) = entry.split_once('=')?;
            value
                .contains("opt-level = 3")
                .then(|| package.trim().to_owned())
        })
        .collect();
    bumped.sort();

    assert_eq!(
        bumped,
        BUMPED.map(str::to_owned).to_vec(),
        "[profile.dev.package] is not the list deps.md §14 names. A missing one is a debug \
         build slow enough to distort every latency observation; an extra one is the profile \
         growing by imitation, which is what \"we take exactly those\" refuses"
    );

    let lock = workspace_file("Cargo.lock");
    let absent: Vec<&&str> = BUMPED
        .iter()
        .filter(|package| !lock.contains(&format!("name = \"{package}\"")))
        .collect();
    assert!(
        absent.is_empty(),
        "a profile override names a package that is not in the graph: {absent:?}. Cargo only \
         warns about this and builds anyway, so the bump goes on looking applied while doing \
         nothing -- which is why §14's list is checked against the lockfile rather than \
         against a build that would succeed either way"
    );
}

/// The rest of `deps.md` §14's conventions — the ones no other test in this
/// file reads, which is why they are grouped rather than argued one at a time.
///
/// They have a failure mode in common, and it is not that the workspace stops
/// building. Every one of them is a value that is *correct now* and that
/// nothing would report as wrong: `resolver = "2"` still resolves,
/// `lto = false` still links, a `[lib]` without `doctest = false` still tests,
/// and a `license` added to `[workspace.package]` still compiles while
/// quietly overriding the per-crate answers §5 spends a subsection on. The
/// only thing standing between each of them and a drift is that somebody
/// reads §14 again.
///
/// > **`[workspace.package]` carries only what is genuinely uniform.** For Zed
/// > that is `publish` and `edition`; members write `edition.workspace = true`
/// > and `publish.workspace = true`. We add `rust-version` and keep `license`
/// > out, since ours differs per crate (§5).
///
/// The inheritance half is asserted with it, because a `[workspace.package]`
/// nobody inherits is three keys with no effect — and `rust-version` is the
/// one whose absence is silent in the direction that matters: cargo enforces
/// it as a floor per package, so a member that does not inherit it is a member
/// with no minimum toolchain at all.
///
/// > **`[profile.release]`**: `lto = "thin"`, `codegen-units = 1`,
/// > `debug = "limited"` — Zed's values, and the right ones for a binary whose
/// > headline metric is latency but which still needs usable backtraces from
/// > user reports.
///
/// > **`[workspace.metadata.cargo-machete] ignored`** for deps that are used
/// > but invisible to static analysis. `rope` already needs `tracing` listed
/// > this way upstream, and our patched copy still will.
///
/// That bullet named the wrong table and CHANGE-core-006 moved it to
/// `[package.metadata.cargo-machete]`, which is where upstream's `rope` puts
/// it and the precedent the bullet's own second sentence cites. The check is
/// derived rather than listed: a crate whose only mention of `tracing` is the
/// `#[instrument]` redirect is exactly a dependency "used but invisible to
/// static analysis", so the condition computes which crates those are and
/// requires the record of each. `heuristic_jump` deliberately does not match —
/// it declares `tracing` and names it nowhere, which is a dependency that is
/// not used rather than one that is invisible, and §14 does not cover it.
///
/// The `rust-toolchain.toml` half is §14's file tree, "pin 1.95.0, so
/// grammar/rope behaviour is reproducible", read against `rust-version`. The
/// two are different mechanisms — a selection and a floor — and
/// `conformance-002` (answered) is the record of why both exist. What breaks
/// silently is them disagreeing: `rust-version` above the pinned channel makes
/// every build fail, and below it makes the floor a fiction, and neither is
/// visible in either file alone.
#[test]
fn the_workspace_manifest_has_the_shape_section_14_states() {
    let manifest = workspace_file("Cargo.toml");
    assert!(
        !manifest.is_empty(),
        "no workspace Cargo.toml, so every assertion below is vacuous"
    );

    assert!(
        table_of(&manifest, "workspace")
            .iter()
            .any(|line| line.replace(' ', "") == "resolver=\"3\""),
        "[workspace] does not set resolver = \"3\": deps.md §14 names it as one of the two \
         places we deliberately differ from Zed, whose \"2\" is legacy — and a resolver that \
         reverts resolves a different feature set for every crate in the graph while building \
         perfectly"
    );

    let mut uniform: Vec<String> = table_of(&manifest, "workspace.package")
        .iter()
        .filter_map(|line| Some(line.split_once('=')?.0.trim().to_owned()))
        .collect();
    uniform.sort();
    assert_eq!(
        uniform,
        ["edition", "publish", "rust-version"]
            .map(str::to_owned)
            .to_vec(),
        "[workspace.package] is not the three keys deps.md §14 calls genuinely uniform: Zed's \
         publish and edition, plus the rust-version we add. `license` most of all belongs out \
         of it — §5's answers differ per crate, and one here would override all seven while \
         leaving every manifest looking right"
    );

    let release = table_of(&manifest, "profile.release");
    for (key, expected) in [
        ("debug", "\"limited\""),
        ("lto", "\"thin\""),
        ("codegen-units", "1"),
    ] {
        let set = release.iter().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.split('#').next().unwrap_or("").trim().to_owned())
        });
        assert_eq!(
            set.as_deref(),
            Some(expected),
            "[profile.release] sets {key} to {set:?} and deps.md §14 takes Zed's {expected}: \
             the three are one decision — a binary whose headline metric is latency, which \
             still needs a usable backtrace out of a user's report"
        );
    }

    let pinned = table_of(&workspace_file("rust-toolchain.toml"), "toolchain")
        .iter()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == "channel").then(|| value.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_default();
    let floor = table_of(&manifest, "workspace.package")
        .iter()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == "rust-version").then(|| value.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_default();
    assert!(
        !pinned.is_empty(),
        "rust-toolchain.toml names no [toolchain] channel: deps.md §14's file tree makes the \
         pin what keeps grammar and rope behaviour reproducible across machines and across the \
         life of the metrics history"
    );
    assert_eq!(
        floor, pinned,
        "rust-version is {floor:?} and rust-toolchain.toml pins {pinned:?}: the two are a floor \
         and a selection rather than a duplicate, and they are only both true while they agree \
         — above the channel every build fails, below it the floor is a fiction \
         (conformance-002)"
    );

    let members = workspace_members();
    assert!(
        !members.is_empty(),
        "no workspace members parsed out of Cargo.toml, so the scan below would pass vacuously"
    );

    let mut invisible = 0;
    for member in &members {
        let text = workspace_file(&format!("{member}/Cargo.toml"));
        assert!(
            !text.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );

        for key in ["edition", "publish"] {
            assert!(
                text.contains(&format!("{key}.workspace = true")),
                "{member} does not write `{key}.workspace = true`: deps.md §14 puts the \
                 uniform keys in [workspace.package], and a member that inherits none of \
                 them is one the table does not reach"
            );
        }

        if member.starts_with("crates/") {
            assert!(
                text.contains("rust-version.workspace = true"),
                "{member} does not inherit rust-version: deps.md §14 adds it to Zed's two, and \
                 cargo enforces it per package — a member that does not inherit it declares no \
                 minimum toolchain at all, which is invisible until a build succeeds somewhere \
                 it should not have"
            );
            if text.contains("[lib]") {
                assert!(
                    table_of(&text, "lib")
                        .iter()
                        .any(|line| line.replace(' ', "") == "doctest=false"),
                    "{member}'s [lib] does not set doctest = false, which deps.md §14 asks for \
                     on crates with no doctests. The vendored crates are exempt for the reason \
                     §14 gives the [lib] path bullet — they arrive with upstream's answer — but \
                     ours are written here, and a doctest harness nobody uses is a build \
                     target nobody reads the output of"
                );
            }
        }

        if !declares(&text, "tracing") {
            continue;
        }
        const REDIRECT: &str = "use tracing::instrument;";
        let sources = member_sources(member);
        let attribute_only = sources.iter().any(|source| source.contains(REDIRECT))
            && !sources
                .iter()
                .any(|source| source.replace(REDIRECT, "").contains("tracing::"));
        if !attribute_only {
            continue;
        }
        invisible += 1;
        assert!(
            table_of(&text, "package.metadata.cargo-machete")
                .iter()
                .any(|line| line.starts_with("ignored") && line.contains("\"tracing\"")),
            "{member} reaches tracing only through the #[instrument] redirect and records no \
             cargo-machete exemption: deps.md §14 keeps the table for deps that are used but \
             invisible to static analysis, and this is the case it names — an unused-dependency \
             report that is wrong is one nobody acts on twice"
        );
    }
    assert_eq!(
        invisible, 2,
        "{invisible} member(s) reach tracing through the #[instrument] redirect alone, and \
         core.md §9 puts the redirect in rope and both of sum_tree's instrumented files — so \
         either the redirect moved or this scan is reading nothing"
    );
}

/// Every source file of a workspace member, whichever half of the workspace it
/// is in. `sources_of` and `vendored_sources` differ in return type and in
/// what they assert, because each grew for one caller; a rule quantified over
/// `[workspace] members` needs both without caring which.
fn member_sources(member: &str) -> Vec<String> {
    match member.strip_prefix("vendor/") {
        Some(crate_name) => vendored_sources(crate_name),
        None => sources_of(member.strip_prefix("crates/").unwrap_or(member))
            .into_iter()
            .map(|(_, source)| source)
            .collect(),
    }
}

/// §5's table, as a rule rather than a list, because six more `lang_*` and six
/// more `measure_*` arrive by copying the template and a hardcoded list would
/// stop applying the moment one did.
///
/// The rule is narrower than "reaches a GPL input", and `conformance-014`
/// (answered) states it: **a `license` field describes copyright in that
/// crate's own text.** What it links is a property of the artifact, recorded
/// once in §14 — "binary crate; the artifact it builds is GPL".
///
/// §14's own layout is what showed it. `heuristic_jump` depends on every
/// `lang_*` and so on `similarity`, and is listed MIT; so "reaching GPL makes
/// you GPL" was already false in this workspace, and `measure_rust` moved to
/// MIT rather than a third case being added to a section that has two rules.
///
/// One thing this scan therefore encodes and `deps.md` does not yet: GPL marks
/// `similarity`, which is a port and so a derivative work, and `lang_*`, which
/// §5 marks by the dependency rule just rejected. `conformance-014`'s
/// follow-up leaves that open — `lang_*` may well stay GPL for the first
/// reason rather than the second, and until §5 says which, this list is the
/// only place the two are distinguished.
fn expected_licence(member: &str) -> &'static str {
    const GPL: &str = "GPL-3.0-or-later";

    match member {
        "vendor/rope" => GPL,
        "vendor/sum_tree" => "Apache-2.0",
        "crates/similarity" => GPL,
        _ if member.starts_with("crates/lang_") => GPL,
        _ => "MIT",
    }
}

/// A member's `license` field, and only the literal form. §14 sets it "per
/// crate rather than in `[workspace.package]`, because the two answers differ",
/// so a `license.workspace = true` is not a value this returns — it comes back
/// `None` and the caller reports it as a missing declaration, which is what it
/// would be.
fn licence_of(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("license")?.trim_start();
        Some(rest.strip_prefix('=')?.trim().trim_matches('"').to_owned())
    })
}

/// Every `[workspace] members` entry, as its path from the workspace root.
/// `crate_members` above is the `crates/*` half of this; the licensing and
/// manifest-shape rules quantify over the vendored crates too, and in §14's
/// case the vendored ones are where the rule *differs*.
fn workspace_members() -> Vec<String> {
    workspace_file("Cargo.toml")
        .lines()
        .map(|line| line.trim().trim_matches(['"', ',']))
        .filter(|line| line.starts_with("crates/") || line.starts_with("vendor/"))
        .map(str::to_owned)
        .collect()
}

/// Whether a manifest declares `name` in any table. Comments are skipped,
/// which is the whole reason this is not a `contains`.
fn declares(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#')
            && line
                .split(['.', ' ', '='])
                .next()
                .is_some_and(|declared| declared.trim() == name)
    })
}

/// Every `crates/*` workspace member.
fn crate_members() -> Vec<String> {
    workspace_file("Cargo.toml")
        .lines()
        .filter_map(|line| {
            line.trim()
                .trim_matches(['"', ','])
                .strip_prefix("crates/")
                .map(str::to_owned)
        })
        .collect()
}

/// The `crates/lang_*` workspace members, which is the list every language
/// rule below is quantified over — six more of them arrive by copying the
/// template, and a rule that named `lang_rust` would stop applying the moment
/// it stopped being the only one.
fn language_members() -> Vec<String> {
    workspace_file("Cargo.toml")
        .lines()
        .filter_map(|line| {
            line.trim()
                .trim_matches(['"', ','])
                .strip_prefix("crates/")
                .filter(|member| member.starts_with("lang_"))
                .map(str::to_owned)
        })
        .collect()
}

/// A crate's sources, as (path, contents).
///
/// `read_dir` is banned by `clippy.toml`, so this follows the crate root's own
/// `mod` declarations rather than listing a directory — which is the stricter
/// reading anyway: a file no `mod` reaches is a file the crate does not
/// compile. `CLAUDE.md` fixes the two names it needs, forbidding `mod.rs` and
/// naming the library root after the crate.
///
/// Pure, like `workspace_file` and for the same reason: a missing file comes
/// back as an empty string, and the caller asserts it is not one.
fn sources_of(crate_name: &str) -> Vec<(String, String)> {
    let root = format!("crates/{crate_name}/src/{crate_name}.rs");
    let text = workspace_file(&root);

    let mut sources = vec![(root, text.clone())];
    for line in text.lines() {
        let declared = line
            .trim()
            .strip_prefix("mod ")
            .or_else(|| line.trim().strip_prefix("pub mod "))
            .and_then(|rest| rest.strip_suffix(';'));
        if let Some(module) = declared {
            let path = format!("crates/{crate_name}/src/{module}.rs");
            let source = workspace_file(&path);
            sources.push((path, source));
        }
    }
    sources
}

/// `deps.md` §10's rules for keeping the error set closed, which are the ones
/// `anyhow` would erase one `?` at a time.
///
/// > **No `Other(String)`, no `Message(String)`, no `Box<dyn Error>` variant.**
/// > Adding a failure mode means adding a variant, which is the point.
///
/// > **Foreign errors are the one unavoidable leak.** `std::io::Error` and
/// > `serde_json::Error` are themselves open. They are wrapped as `#[source]`
/// > fields on our own variants, always alongside our own context (which path,
/// > which frame), so the *classification* is ours even though the detail is
/// > theirs.
///
/// The second rule is the one with teeth and the one that had drifted: two
/// variants wrapped a foreign error and carried nothing else —
/// `CodecError::BodyNotJson` and `ProjectError::Scanner`. Neither is a
/// compile error and neither reads as wrong; what they cost is that the
/// classification stops being ours at the point it matters. A
/// `serde_json::Error` names a line and column inside a body nobody kept, and
/// an `io::Error` from a thread spawn says only that the process is out of
/// something. Both now carry §10's "which frame" and "which path".
///
/// The document is the fixture, in the sense §15's test uses: the enum is read
/// out of `shared/src/error.rs` rather than transcribed here, so a variant
/// added tomorrow is checked without anyone updating a list. What that costs
/// is a scan that assumes one field per line, which is how the file is written
/// and what `cargo fmt` keeps true.
///
/// The `Box<dyn` ban is the one clause with a *partial* compiler backstop, and
/// the boundary is worth writing down because it is not where it looks. A bare
/// `Box<dyn Error>` variant does not compile: it costs `Error` its `Send`, and
/// `files.rs` moves one into the scanner thread. A
/// `Box<dyn Error + Send + Sync>` compiles perfectly — and is the form anyone
/// reaching for an escape hatch would actually write, since it is the one the
/// error ecosystem hands out. The control run for this test confirmed both, so
/// the scan is checking exactly the case the compiler does not.
#[test]
fn every_foreign_error_is_wrapped_beside_context_of_ours() {
    let source = workspace_file("crates/shared/src/error.rs");
    assert!(
        source.contains("pub enum Error {"),
        "shared/src/error.rs holds no `pub enum Error`, so this test would read nothing"
    );

    for banned in ["Other(String)", "Message(String)", "Box<dyn"] {
        assert!(
            !source.contains(banned),
            "shared/src/error.rs contains {banned}: deps.md §10 keeps the set closed, and an \
             escape hatch is anyhow with extra steps — the failure classes are only a table \
             shim.md §11 can match on while every one of them is a variant"
        );
    }

    // Each variant runs from its `#[error(...)]` attribute to the next one, or
    // to the end of the file. Within it, a field is a line reading `name:`.
    let variants: Vec<&str> = source.split("#[error(").skip(1).collect();
    assert!(
        variants.len() > 40,
        "only {} variants found in shared/src/error.rs, and §10's tree has more than that — \
         the scan is not reading the file",
        variants.len()
    );

    let mut checked = 0;
    for variant in variants {
        let body = variant.split("\n}").next().unwrap_or(variant);
        if !body.contains("#[source]") {
            continue;
        }
        let named = variant
            .lines()
            .find_map(|line| line.trim().strip_suffix(" {"))
            .unwrap_or("<unnamed>");
        let fields = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#') && !line.starts_with("//"))
            .filter(|line| {
                line.split_once(':').is_some_and(|(name, _)| {
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|character| character.is_ascii_lowercase() || character == '_')
                })
            })
            .count();
        assert!(
            fields > 1,
            "{named} wraps a foreign error and carries nothing else: deps.md §10 keeps the \
             classification ours by pairing every #[source] with context of our own — which \
             path, which frame — because the foreign detail alone describes a failure in \
             somebody else's vocabulary"
        );
        checked += 1;
    }
    assert!(
        checked > 8,
        "only {checked} variants wrap a foreign error, and error.rs has more than that — the \
         field scan is matching nothing and would pass whatever the file said"
    );

    // §10 puts `#[non_exhaustive]` on the sub-enums and deliberately not on
    // `Error`: "within one workspace, an exhaustive match on the top level is
    // a feature", and it is what CLAUDE.md's ban on wildcard arms rests on.
    let (before_total, sub_enums) = source
        .split_once("pub enum Error {")
        .expect("the total enum, asserted present above");
    assert!(
        !before_total.trim_end().ends_with("#[non_exhaustive]"),
        "Error is #[non_exhaustive]: deps.md §10 marks the sub-enums and not the top level, \
         because an exhaustive match on the nine classes is what makes shim.md §11's table a \
         table"
    );
    for arm in [
        "ConfigError",
        "CodecError",
        "ChildError",
        "ProtocolError",
        "DocumentError",
        "ParseError",
        "ProjectError",
        "HandlerError",
        "EncodingError",
    ] {
        assert!(
            sub_enums.contains(&format!("#[non_exhaustive]\npub enum {arm} {{")),
            "{arm} is not #[non_exhaustive]: deps.md §10 marks every sub-enum, so that adding \
             a leaf is not a breaking change to the class table above it"
        );
    }
}

/// `deps.md` §9, whose three claims are each about something that is invisible
/// from inside the process.
///
/// > The thing to be careful about: the child's stderr is forwarded verbatim to
/// > our stderr (`shim.md` §2), so our own log lines interleave with
/// > rust-analyzer's in the editor's log panel. Every line we emit gets a
/// > distinguishing prefix, and the default filter is `warn` so we are quiet
/// > unless asked.
///
/// The prefix was absent, and its absence produces no failure anywhere: the
/// logs are correct, well-formatted and indistinguishable from the child's. The
/// cost is paid by a user reading one editor panel who reports our warning
/// against rust-analyzer.
///
/// It is asserted per *line* rather than per event, which is the part a
/// hand-rolled prefix gets wrong: a multi-line message prefixed once carries it
/// on the first line only, and the continuation lines are exactly the ones that
/// look like somebody else's output.
///
/// > The JSONL metric records of `core.md` §7 are **not** tracing output. They
/// > go to their own file via `serde_json`, because they are structured data
/// > with a fixed schema that `measure_core` also writes, and routing them
/// > through a log subscriber would make the schema a formatting concern.
///
/// The mechanism for that one is the subscriber being installed in exactly one
/// place: a metrics writer routed through a log subscriber needs to reach
/// `tracing_subscriber`, and only the binary can.
#[test]
fn our_log_lines_are_distinguishable_and_the_subscriber_is_installed_once() {
    assert_eq!(
        driver::DEFAULT_LOG_FILTER,
        "warn",
        "deps.md §9 makes `warn` the default so that we are quiet unless asked — our lines \
         share a panel with the child's, and a chatty default spends the user's attention on \
         a tool they did not ask to hear from"
    );

    let mut written = Vec::new();
    let mut writer = driver::PrefixedWriter::new(&mut written);
    let event = "a warning\nwith a second line\n";
    assert_eq!(
        std::io::Write::write(&mut writer, event.as_bytes()).expect("a write to a Vec"),
        event.len(),
        "PrefixedWriter reported a length that counted its own prefix: a Write that claims \
         more than it was given makes every caller's loop arithmetic wrong"
    );
    let text = String::from_utf8(written).expect("the prefix and the event are both UTF-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "a two-line event came out as {} line(s): {text:?}",
        lines.len()
    );
    for line in lines {
        assert!(
            line.starts_with(driver::LOG_PREFIX),
            "the line {line:?} carries no prefix: deps.md §9 wants every line distinguishable \
             from the child's, and a continuation line is the one most easily read as the \
             child's"
        );
    }

    let members = crate_members();
    assert!(
        !members.is_empty(),
        "no crates/* workspace member, so this test would pass vacuously"
    );

    /// The crates that own a process, in sorted order so the comparison below
    /// does not depend on how the workspace lists its members
    /// (`DECISION-core-002: provisional`).
    const INSTALLS_A_SUBSCRIBER: [&str; 2] = ["heuristic_jump", "measure_core"];

    let mut installs = Vec::new();
    for member in &members {
        let manifest = manifest_text(member);
        assert!(
            !manifest.is_empty(),
            "no manifest for {member}, so every assertion below is vacuous"
        );
        for (table, key, _) in dependency_entries(&manifest) {
            let name = key.strip_suffix(".workspace").unwrap_or(&key);
            if name == "tracing-subscriber" {
                installs.push(format!("{member} in [{table}]"));
            }
        }
        for (file, source) in sources_of(member) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );
            // DECISION-core-002: provisional. The permitted set is the crates
            // that own a process, not the one crate that is a `[[bin]]`:
            // `core.md` §7 makes a `measure_<lang>` main four lines and puts
            // the rest — `clap`, and with it the log setup — in `measure_core`.
            // What the assertion still holds is the half of §9's reason that
            // survives either answer: `driver` and `shared`, the crates the
            // shim links, have no opinion about where logs go.
            assert!(
                INSTALLS_A_SUBSCRIBER.contains(&member.as_str())
                    || !source.contains("tracing_subscriber"),
                "{file} names tracing_subscriber: deps.md §9 leaves the subscriber to the \
                 crate that owns the process — a library the shim links with an opinion \
                 about where logs go is one that fights whoever links it, and §7's JSONL \
                 records stay out of the log subscriber by not being able to reach one"
            );
        }
    }
    installs.sort();
    assert_eq!(
        installs,
        INSTALLS_A_SUBSCRIBER
            .iter()
            .map(|member| format!("{member} in [dependencies]"))
            .collect::<Vec<String>>(),
        "deps.md §9 gives tracing-subscriber to the crates that own a process: driver and \
         shared emit through the tracing facade and have no opinion about where it goes \
         (DECISION-core-002: provisional)"
    );

    let declared = table_of(&workspace_file("Cargo.toml"), "workspace.dependencies")
        .into_iter()
        .find(|line| line.starts_with("tracing-subscriber"))
        .unwrap_or_default();
    assert!(
        declared.contains("\"env-filter\""),
        "tracing-subscriber is declared as `{declared}` without env-filter: deps.md §9 names \
         it as one of the two features, and it is what --log is a string for"
    );
}

/// `deps.md` §7, whose two claims are both about a manifest and neither about
/// any code.
///
/// > **It is a dependency of `shared`, not `driver`.** `ProjectView` is a
/// > concrete struct in `shared` (`core.md` §1), because `measure_core` needs
/// > the same scope rules the shim uses and gets them a whole phase earlier.
///
/// The cost of getting that backwards is not a build error. `driver` walking
/// with its own `ignore` and `shared` walking with another is two
/// implementations of the rules that decide what a search can find, and §7
/// states what that costs: "the corpus scores a tool that is not the one that
/// ships."
///
/// > **`notify`** — **deferred behind a non-default `watch` feature.** … The
/// > dependency is written into `Cargo.toml` as optional so the decision is
/// > visible, not lost.
///
/// That sentence is the whole assertion, and it is unusual in being about a
/// dependency that must be *declared and not used*. A deferral recorded only in
/// prose is one the next person re-decides from scratch, and §7's reasons —
/// the editor is already watching in proxy mode, inotify runs out of
/// descriptors on large repos — are not re-derivable from an absence.
///
/// **`optional = true` has no negative control**, for the reason
/// `the_workspace_lints_reach_our_crates_and_not_the_vendored_ones` records
/// about `vendor/rope`: the mutation is rejected before a test runs. Dropping
/// it while `watch = ["dep:notify"]` stands makes cargo refuse to parse the
/// manifest — "feature `watch` includes `dep:notify`, but `notify` is not an
/// optional dependency" — and a control that produces no test result is not a
/// control. The assertion is kept because cargo's enforcement is incidental to
/// it: delete the feature and the optionality stops being checked by anything,
/// which is the state §7 is guarding against rather than a separate one.
#[test]
fn the_walker_belongs_to_shared_and_the_watcher_is_declared_but_off() {
    assert!(
        dependencies_in(&manifest_text("shared"))
            .iter()
            .any(|name| name == "ignore"),
        "shared does not depend on ignore: deps.md §7 puts the walker there because \
         ProjectView is shared's, so that measure_core scores the same scope rules the shim \
         ships — two walkers would mean the corpus scores a tool that is not the one shipped"
    );
    assert!(
        !dependencies_in(&manifest_text("driver"))
            .iter()
            .any(|name| name == "ignore"),
        "driver depends on ignore directly: deps.md §7 says it is shared's, and a driver that \
         can walk is one that can grow a second set of exclusion rules nobody compares to the \
         first"
    );

    let manifest = manifest_text("driver");
    let notify = dependency_entries(&manifest)
        .into_iter()
        .find(|(_, key, _)| key.strip_suffix(".workspace").unwrap_or(key) == "notify");
    let Some((table, _, value)) = notify else {
        panic!(
            "driver declares no notify: deps.md §7 defers the watcher and asks for the \
             dependency to be written in as optional \"so the decision is visible, not lost\" \
             — an absent crate records no decision, and the next person re-derives it"
        )
    };
    assert_eq!(
        table, "dependencies",
        "driver declares notify in [{table}]: §7 defers it behind a feature rather than \
         moving it to the tests, which is a different decision"
    );
    assert!(
        value.contains("optional = true"),
        "driver declares notify as `{value}`, not optional: §7's deferral is the whole entry, \
         and a non-optional notify is the watcher built rather than recorded"
    );

    let features = table_of(&manifest, "features");
    assert!(
        features.iter().any(|line| line.starts_with("watch")),
        "driver has no `watch` feature: §7 names it, and an optional dependency no feature \
         gates is one nothing can turn on"
    );
    assert!(
        !features.iter().any(|line| line.starts_with("default")),
        "driver declares a default feature set: §7 keeps `watch` non-default, because in \
         proxy mode the editor is already watching and a notify watcher would be a second, \
         worse copy of a signal already on the wire"
    );

    for member in crate_members() {
        if member == "driver" {
            continue;
        }
        for (table, key, _) in dependency_entries(&manifest_text(&member)) {
            assert!(
                key.strip_suffix(".workspace").unwrap_or(&key) != "notify",
                "{member} declares notify in [{table}]: §7 places the watcher behind driver's \
                 feature, and a second declaration is a watcher nobody deferred"
            );
        }
    }
}

/// `deps.md` §12's last line, which is the one row of its table that names no
/// crate: "The injected clock for `shim.md` §12's protocol race tests is a
/// `trait Clock` with a `TestClock` impl in `shared`, not a dependency."
///
/// The trait and `SystemClock` were in `shared`; the test impl was not, and
/// five suites had each written their own — four copies of a frozen clock and
/// one drivable one, in `shared/tests/{project,document}.rs` and
/// `driver/tests/{deadline,snapshots,file_list}.rs`. That is the failure §12
/// describes with the dependency declined and the replacement never built, and
/// it is not free: the driven copy converted through `as_millis`, so a suite
/// advancing by less than a millisecond advanced by nothing and asserted
/// against a clock that had not moved.
///
/// Placing it in `shared` rather than behind a `test-support` feature creates
/// one risk in exchange — a clock production code could drive — so that is what
/// the scan below checks. The behavioural half is asserted too, because "a
/// `TestClock` in `shared`" is satisfied by a type of that name that does
/// nothing, and the whole point is that `Deadline` reads it.
///
/// What this cannot reach is the `tests/` directories: `sources_of` follows the
/// crate root's `mod` declarations, and a test file is reached by none. A sixth
/// hand-rolled `impl Clock` in a suite would not fail here. What stops it is
/// that there is now no reason to write one.
#[test]
fn the_injected_clock_is_shareds_and_only_the_tests_can_drive_it() {
    let defined_in = std::any::type_name::<shared::TestClock>();
    assert!(
        defined_in.starts_with("shared::"),
        "TestClock is defined in {defined_in}: deps.md §12 puts it in shared, so that every \
         suite injects the same double rather than five that differ where nobody compared them"
    );

    // It is a `Clock`, and driving it moves a `Deadline` — which is the claim,
    // rather than the existence of a type with the right name.
    let clock = std::sync::Arc::new(shared::TestClock::new());
    let arrived_at = shared::Clock::now(clock.as_ref());
    let budget = std::time::Duration::from_millis(20);
    let deadline = shared::Deadline::new(
        std::sync::Arc::clone(&clock) as std::sync::Arc<dyn shared::Clock>,
        arrived_at,
        budget,
    );
    assert!(
        !deadline.expired(),
        "a deadline built against an unadvanced TestClock is already expired, so the clock is \
         not the one it reads"
    );
    // Sub-millisecond, which is the resolution the driven copy in
    // file_list.rs truncated away: it advanced in whole milliseconds, so this
    // step would have moved nothing.
    clock.advance(budget + std::time::Duration::from_micros(1));
    assert!(
        deadline.expired(),
        "advancing the TestClock past the budget left the deadline live: core.md §5's cap is \
         enforced by reading the clock, so a double the deadline does not read makes every \
         expiry test assert on nothing"
    );

    let members = crate_members();
    assert!(
        !members.is_empty(),
        "no crates/* workspace member, so this test would pass vacuously"
    );

    for member in &members {
        for (file, source) in sources_of(member) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );
            let defines_it = file == "crates/shared/src/deadline.rs";
            for line in source.lines() {
                if !line.contains("TestClock") {
                    continue;
                }
                // The crate root is the other place the name legitimately
                // appears, and only on the re-export: a `TestClock` reached
                // from a function body there would be the thing being caught.
                let re_exports_it =
                    file == "crates/shared/src/shared.rs" && line.starts_with("pub use deadline::");
                assert!(
                    defines_it || re_exports_it,
                    "{file} names TestClock in `{line}`: it is exported unconditionally rather \
                     than behind a feature, and the price of that is that production code must \
                     not be able to reach a clock it can drive — a shim whose deadline never \
                     expires abstains for nobody"
                );
            }
            assert!(
                defines_it || !source.contains("impl Clock for"),
                "{file} implements Clock: deps.md §12 has one trait and the two impls shared \
                 provides, and a third is a suite writing its own double again"
            );
        }
    }
}

/// `deps.md`'s "`FxHashMap` and `FxHashSet` are the default", whose two rules
/// are both about what a *use site* is allowed to name:
///
/// > **A type alias, not a naked import.** `shared` exports
/// > `pub type Map<K, V> = FxHashMap<K, V>` and `pub type Set<T> = FxHashSet<T>`,
/// > so switching hashers later is one line rather than a sweep, and so the
/// > choice is visible at every use site.
///
/// > **Reach for `std::collections::HashMap` when a key is genuinely external
/// > and unbounded** … When it happens, say so in a comment; an unexplained
/// > `HashMap` should read as an oversight.
///
/// Neither is a claim the build can check on its own, and the failure is
/// nothing: `use rustc_hash::FxHashMap` compiles, runs, hashes identically, and
/// costs only that the alias's stated benefit — one line rather than a sweep —
/// is false, which is invisible until the day someone tries the one line. Five
/// files were in that state, and the alias did not exist to be preferred.
///
/// The first rule now has an enforcement stronger than this scan for `driver`,
/// as a side effect of applying it: with every use site on `shared::Map`,
/// `driver` no longer names `rustc_hash` at all, so its manifest stopped
/// declaring it and a naked import there fails to resolve. That is `deps.md`
/// §14's "each arrives with its first user" run backwards, and it is why §0's
/// table still placing `rustc-hash` in `driver` is not a contradiction — the
/// table is asserted as a subset by
/// `the_core_crates_declare_only_what_section_0_places_there`, and a placed
/// dependency with no user is the intended state.
///
/// **The second rule is vacuous today and is written down anyway.** No crate
/// uses `std::collections::HashMap`, so the loop below has nothing to check;
/// what it does is fix the shape of the exception in advance, so the first
/// genuinely-external key arrives with the comment §8 asks for rather than
/// with an argument about whether one is needed.
#[test]
fn the_default_map_and_set_are_the_aliases_shared_exports() {
    // An alias to *std's* `HashMap` carrying rustc_hash's hasher, rather than a
    // second map type of our own — which would compile, satisfy every use site,
    // and quietly not be what `deps.md` argued for.
    let map = std::any::type_name::<shared::Map<u8, u8>>();
    let set = std::any::type_name::<shared::Set<u8>>();
    assert!(
        map.contains("HashMap") && map.contains("rustc_hash"),
        "shared::Map is {map}: deps.md makes it an alias for rustc_hash's FxHashMap, whose \
         argument is that nothing here is keyed by untrusted input and SipHash's fixed setup \
         cost is pure overhead on the definition path"
    );
    assert!(
        set.contains("HashSet") && set.contains("rustc_hash"),
        "shared::Set is {set}: deps.md makes it an alias for rustc_hash's FxHashSet"
    );

    let members = crate_members();
    assert!(
        !members.is_empty(),
        "no crates/* workspace member, so this test would pass vacuously"
    );

    for member in &members {
        for (file, source) in sources_of(member) {
            assert!(
                !source.is_empty(),
                "{file} is missing or empty, so the scan below would pass vacuously"
            );

            // `shared`'s root is where the alias is defined, so it is the one
            // file that names the hasher crate. `similarity` is exempt from
            // nothing here: its `FxHasher32` is a hand-rolled hasher rather
            // than a map, and does not match.
            let defines_the_alias = file == "crates/shared/src/shared.rs";
            for named in ["rustc_hash", "FxHashMap", "FxHashSet"] {
                assert!(
                    defines_the_alias || !source.contains(named),
                    "{file} names {named} directly: deps.md wants the map and set reached \
                     through shared::Map and shared::Set, because a naked import makes \
                     'switching hashers later is one line rather than a sweep' false without \
                     anything saying so"
                );
            }

            for (number, line) in source.lines().enumerate() {
                if !line.contains("std::collections::HashMap")
                    && !line.contains("std::collections::HashSet")
                {
                    continue;
                }
                let explained = source
                    .lines()
                    .take(number)
                    .skip(number.saturating_sub(3))
                    .any(|above| above.trim_start().starts_with("//"));
                assert!(
                    explained,
                    "{file} reaches for std's hasher at line {}, with no comment above saying \
                     why: deps.md keeps std::collections::HashMap for a key that is genuinely \
                     external and unbounded, and wants an unexplained one to read as an \
                     oversight rather than as a choice",
                    number + 1
                );
            }
        }
    }
}

/// `core.md` §9 gives `shared`'s dependencies and says "this list is the
/// authoritative one", which is a claim nothing enforces: an added crate is
/// invisible until someone rereads the section beside the manifest, and
/// `tracing` sat outside the list for four campaigns that way.
///
/// A subset rather than an equality, because §9 lists `rayon` for
/// `ProjectView::scan` and `deps.md` §14 has each dependency arrive with its
/// first user — a listed crate not yet declared is the intended state, where a
/// declared crate not on the list is the thing being caught.
#[test]
fn shared_declares_only_the_dependencies_section_9_lists() {
    const AUTHORITATIVE: &[&str] = &[
        "ignore",
        "rayon",
        "rope",
        "rustc-hash",
        "serde",
        "serde_json",
        "thiserror",
        "tracing",
        "tree-sitter",
        "url",
    ];

    for dependency in dependencies_in(&manifest_text("shared")) {
        assert!(
            AUTHORITATIVE.contains(&dependency.as_str()),
            "shared depends on {dependency}, which is not on core.md §9's list — and that \
             list calls itself the authoritative one, so either the dependency or the \
             section is wrong and a spec-changelog entry says which"
        );
    }
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

/// The first fenced `toml` block after a heading, so a design document can be
/// used as a fixture rather than transcribed into a constant here.
fn fenced_toml_of(document: &str, heading: &str) -> String {
    let Some((_, after)) = document.split_once(heading) else {
        return String::new();
    };
    let Some((_, block)) = after.split_once("```toml\n") else {
        return String::new();
    };
    block
        .split_once("\n```")
        .map(|(body, _)| body)
        .unwrap_or("")
        .to_owned()
}

/// One lint table as (name, level) pairs, with trailing comments dropped.
/// Reused across a manifest and a design document, which is the whole point:
/// the same reader on both sides means a difference is a difference and not a
/// parsing artefact.
fn lint_entries(text: &str, table: &str) -> Vec<(String, String)> {
    table_of(text, table)
        .iter()
        .filter_map(|line| {
            let (name, rest) = line.split_once('=')?;
            let level = rest.split('#').next().unwrap_or("").trim();
            Some((name.trim().to_owned(), level.to_owned()))
        })
        .collect()
}

/// The workspace's own crates, by name, read out of `[workspace.dependencies]`
/// rather than listed here — a hardcoded list would need editing for each of
/// the six `lang_*` and six `measure_*` still to arrive, and would report each
/// one as a stray third-party dependency until someone did.
fn workspace_path_dependencies() -> Vec<String> {
    table_of(&workspace_file("Cargo.toml"), "workspace.dependencies")
        .iter()
        .filter(|line| line.contains("path ="))
        .filter_map(|line| Some(line.split_once('=')?.0.trim().to_owned()))
        .collect()
}

/// The lines of one named table, exactly — `table_of(m, "lints")` returns
/// `[lints]`'s body and not `[lints.clippy]`'s, which is the distinction the
/// vendored crates turn on.
fn table_of(manifest: &str, wanted: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == wanted;
            continue;
        }
        if inside && !line.starts_with('#') && !line.is_empty() {
            lines.push(line.to_owned());
        }
    }
    lines
}

/// Every entry of every dependency table, as (table, key, value), where the
/// key is everything left of the first `=` and the value everything right of
/// it. `dependencies_in` above answers "what does this crate depend on" and
/// deliberately reads `[dependencies]` alone; §14's conventions are about the
/// *form* of an entry and hold in `[dev-dependencies]` just as much, which is
/// where a stray version would be least noticed.
///
/// `[workspace.dependencies]` is skipped by name, since it is the one table
/// where a version belongs.
fn dependency_entries(manifest: &str) -> Vec<(String, String, String)> {
    let mut entries = Vec::new();
    let mut table = String::new();
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name.to_owned();
            continue;
        }
        if !table.ends_with("dependencies") || table.starts_with("workspace.") {
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            entries.push((
                table.clone(),
                key.trim().to_owned(),
                value.trim().to_owned(),
            ));
        }
    }
    entries
}

fn manifest_text(crate_name: &str) -> String {
    workspace_file(&format!("crates/{crate_name}/Cargo.toml"))
}

/// Relative to the workspace root, which is two levels up from `crates/driver`.
///
/// `unwrap_or_default` rather than a panic, because `panic` is denied outside a
/// `#[test]` — and it is not a silent pass: every caller asserts that something
/// is *present* in what comes back, so an empty string fails them all.
/// `core.md` §9 makes two counting claims about the `ztracing` redirect, and
/// they were wrong in the same way: "That is a single-line patch to `rope`" is
/// right, but "`sum_tree` needs no patching" is not — the same redirect is in
/// both of `sum_tree`'s instrumented files, beside a `ctor`/`zlog` deletion and
/// a dropped `tree_map.rs` (CHANGE-core-004).
///
/// The section is where the claim was, and `vendor/README.md` is where the
/// answer already was — it lists all three under "Patches to `sum_tree`". So
/// what failed was not the record but the absence of anything comparing the
/// two. That is what this is: the crates `vendor/` holds, the crates the
/// README records patches for, and the redirect's actual site count, all
/// checked against each other.
///
/// It sits here rather than in `vendor/rope/tests/` because it is a claim
/// about `vendor/` as a whole and about a document that describes the
/// workspace, and because the vendored crates are not linted — this file is.
#[test]
fn every_vendored_crate_records_the_patches_it_carries() {
    let readme = workspace_file("vendor/README.md");
    let vendored: Vec<String> = workspace_members()
        .into_iter()
        .filter_map(|member| Some(member.strip_prefix("vendor/")?.to_owned()))
        .collect();

    assert_eq!(
        vendored.len(),
        2,
        "core.md §9 vendors `rope` and `sum_tree` and rope-modifications.md §4 \
         says `vendor/util` does not exist, so a third here is a crate arriving \
         without either document saying so: {vendored:?}"
    );

    for crate_name in &vendored {
        assert!(
            readme.contains(&format!("## Patches to `{crate_name}`")),
            "vendor/README.md has no `## Patches to \\`{crate_name}\\`` section. \
             §9: the README records, per crate, the upstream revision and the \
             exact patches applied, \"so that a future re-sync can tell at a \
             glance whether upstream changed anything that matters\" -- a \
             vendored crate with no such section is patched by nobody's account"
        );
    }

    // §9, after CHANGE-core-004: "one line in `rope` and one line in each of
    // `sum_tree`'s two instrumented files -- three in all". The count is the
    // claim, so the count is what is asserted.
    let redirects: Vec<(String, usize)> = vendored
        .iter()
        .map(|crate_name| {
            let sites = vendored_sources(crate_name)
                .iter()
                .filter(|source| source.contains("use tracing::instrument;"))
                .count();
            (crate_name.clone(), sites)
        })
        .collect();

    assert_eq!(
        redirects,
        vec![("rope".to_owned(), 1), ("sum_tree".to_owned(), 2)],
        "the `ztracing` -> `tracing` redirect is not where §9 says it is. It is \
         the one patch the section counts rather than describes, and counting \
         it wrong is what made the section claim `sum_tree` needs no patching \
         at all"
    );

    let surviving: Vec<&String> = vendored
        .iter()
        .filter(|crate_name| {
            vendored_sources(crate_name)
                .iter()
                .any(|source| source.contains("ztracing"))
                || workspace_file(&format!("vendor/{crate_name}/Cargo.toml")).contains("ztracing")
        })
        .collect();
    assert!(
        surviving.is_empty(),
        "§9: `ztracing` is not vendored, and its `instrument` is redirected to \
         `tracing` because both crates already depend on it. A surviving \
         reference is a dependency on a crate that is not here: {surviving:?}"
    );
}

/// Every source file of a vendored crate, as text: its `[lib] path` root and
/// the modules that root declares. `sources_of` above reads one file, and
/// these claims are about all of them.
///
/// Reached through the `mod` declarations rather than by listing the
/// directory, because `clippy.toml` disallows `std::fs::read_dir` — and the
/// substitute is the better answer anyway: a `.rs` file the crate root does
/// not declare is not compiled, so a patch hiding in one would not be a patch
/// to the crate at all.
fn vendored_sources(crate_name: &str) -> Vec<String> {
    let manifest = workspace_file(&format!("vendor/{crate_name}/Cargo.toml"));
    let root = manifest
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("path = "))
        .map(|path| path.trim_matches('"').to_owned())
        .unwrap_or_default();
    assert!(
        root.ends_with(".rs"),
        "vendor/{crate_name}/Cargo.toml names no [lib] path, so nothing below reaches its sources"
    );

    let root_text = workspace_file(&format!("vendor/{crate_name}/{root}"));
    let mut sources = vec![root_text.clone()];
    for line in root_text.lines() {
        let line = line.trim();
        let Some(module) = line
            .strip_prefix("mod ")
            .or_else(|| line.strip_prefix("pub mod "))
            .and_then(|rest| rest.strip_suffix(';'))
        else {
            continue;
        };
        let source = workspace_file(&format!("vendor/{crate_name}/src/{module}.rs"));
        assert!(
            !source.is_empty(),
            "vendor/{crate_name}/src/{module}.rs is declared by the crate root and is empty or \
             unreadable, so a scan over it would pass vacuously"
        );
        sources.push(source);
    }

    assert!(
        sources.len() > 1,
        "vendor/{crate_name}'s root declares no modules, so the scans over them would pass \
         vacuously"
    );
    sources
}

fn workspace_file(relative: &str) -> String {
    std::fs::read_to_string(workspace_path(relative)).unwrap_or_default()
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}
