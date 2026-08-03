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

    let manifest = workspace_file("Cargo.toml");
    assert!(
        manifest.contains("[profile.dev.package.tree-sitter]")
            && table_of(&manifest, "profile.dev.package.tree-sitter")
                .iter()
                .any(|line| line.replace(' ', "") == "opt-level=3"),
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
fn workspace_file(relative: &str) -> String {
    std::fs::read_to_string(workspace_path(relative)).unwrap_or_default()
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}
