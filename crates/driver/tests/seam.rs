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
/// The rule is narrower than "reaches a GPL input", and §5's own layout in §14
/// is what shows it: `heuristic_jump` depends on every `lang_*` and so on
/// `similarity`, and is listed `MIT -- binary crate; the artifact it builds is
/// GPL`. So the marking describes copyright in the crate's own text, and GPL
/// marks exactly two things — `similarity`, which is ported, and `lang_*`,
/// which §5 calls the handler layer beside it.
///
/// That reading is why `measure_rust` moved to MIT and is tagged
/// `DECISION-conformance-014`: it is `heuristic_jump`'s case exactly, a binary
/// crate whose artifact is GPL, and its manifest previously reasoned "GPL
/// through `lang_rust`" from a dependency rule `heuristic_jump` falsifies.
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
