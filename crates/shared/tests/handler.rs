//! `design/core.md` §1's printed block, held against the seam it prints.
//!
//! The block went stale for a phase — one `stratum: Stratum` per `Outcome` arm
//! and a three-argument `decide`, months after `conformance-013` widened both —
//! and nothing in the repository could say so, because a fenced block is prose
//! to everything that reads it (CHANGE-core-018). `measure_core`'s
//! `the_command_line_is_section_7s_and_admits_no_flag_it_does_not_name` states
//! the general rule: the document is the fixture, because editing the document
//! is the way of faking progress an audit cannot catch.
//!
//! **Names and arity only.** What the block elides — bodies, derives it does
//! not print, the doc comments the source carries — it elides on purpose, and a
//! test that demanded a transcription would make the block unwritable and would
//! be repaired by weakening it. What cannot be elided is which members an
//! `Outcome` arm carries and what a handler has to pass to commit, because
//! those are the seam a `lang_*` crate is written against.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and every reader below is a free function. Failing loudly is the point: a section this cannot find would otherwise compare an empty block against the seam and pass."
)]

use std::path::Path;

use shared::{ByteLen, CandidateCount, FileCount, Margin, Micros, StageLabel, StageName, Trace};

/// The section that prints the seam. Sliced by heading rather than by line
/// number, which moves.
const HEADING: &str = "### The trait";

fn document() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/core.md");
    std::fs::read_to_string(&path).expect("design/core.md is at the workspace root")
}

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/handler.rs");
    std::fs::read_to_string(&path).expect("the seam this section prints")
}

/// The first fenced block after a heading, whatever its language tag: §1's is
/// tagged `rust` and §9's is untagged, and this file only ever asks for §1's.
fn printed(document: &str) -> String {
    let section = document
        .find(HEADING)
        .map(|start| &document[start..])
        .unwrap_or_else(|| panic!("core.md has no {HEADING}"));
    let open = section
        .find("```")
        .map(|start| start + "```".len())
        .expect("§1 prints the seam in a fenced block");
    let body = &section[open..];
    let body = &body[body.find('\n').map_or(0, |line| line + 1)..];
    body[..body.find("```").expect("an unclosed fenced block")].to_owned()
}

/// Named members of a brace- or paren-delimited group, in order.
///
/// Scanning rather than parsing, for the reason `proto.rs`'s derive scan gives:
/// both sides of the comparison are rustfmt's output or a hand-written block in
/// the same shape, and a piece is a member exactly when it reads `name:`. A
/// path (`std::path::Path`) is excluded by the second colon and a generic
/// argument (`BTreeMap<StageName, Micros>`) by having no colon at all, so the
/// two shapes that would otherwise be false positives are the two this checks.
fn members(text: &str, header: &str) -> Vec<String> {
    let start = text
        .find(header)
        .map(|at| at + header.len())
        .unwrap_or_else(|| panic!("neither side of this comparison declares `{header}`"));
    let mut depth = 1usize;
    let mut names = Vec::new();
    for line in text[start..].lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("#[") {
            continue;
        }
        for piece in trimmed.split([',', '{', '}', '(', ')', ';']) {
            let piece = piece.trim();
            let Some((name, rest)) = piece.split_once(':') else {
                continue;
            };
            // `Query`'s fields are `pub` and `Outcome`'s are not, because one
            // is a struct a handler reads and the other is an enum it builds.
            let name = name.trim_start_matches("pub ");
            if rest.starts_with(':') || name.is_empty() {
                continue;
            }
            if name
                .chars()
                .all(|scalar| scalar.is_ascii_lowercase() || scalar == '_')
            {
                names.push(name.to_owned());
            }
        }
        depth += trimmed.matches(['{', '(']).count();
        depth = depth.saturating_sub(trimmed.matches(['}', ')']).count());
        if depth == 0 {
            break;
        }
    }
    names
}

#[test]
fn the_printed_outcome_carries_what_the_returned_one_carries() {
    let documented = members(&printed(&document()), "pub enum Outcome {");
    let declared = members(&source(), "pub enum Outcome {");

    assert_eq!(
        documented, declared,
        "§1's printed `Outcome` and `shared::handler`'s disagree. This is the \
         seam a `lang_*` crate is written against, and the block is the only \
         description of it a language author reads — a member that is in one \
         and not the other is a handler written against a type that does not \
         exist (CHANGE-core-018)"
    );
    assert!(
        documented.iter().filter(|name| *name == "strata").count() == 2,
        "the strata are reported on one arm of {documented:?}. §7 needs them on \
         both: a stratum with no coverage and a stratum whose searches all \
         abstained are different findings"
    );
}

#[test]
fn the_printed_policy_takes_what_a_handler_has_to_hand_it() {
    let documented = members(&printed(&document()), "pub fn decide(");
    let declared = members(&source(), "pub fn decide(");

    assert_eq!(
        documented, declared,
        "§1's printed `CommitPolicy::decide` and the real one disagree. Every \
         committed answer in every language crate goes through this call, so \
         its parameter list is the most expensive line in the block to have \
         wrong"
    );
}

#[test]
fn the_printed_trait_has_the_methods_a_language_implements() {
    let documented = methods(&printed(&document()));
    let declared = methods(&source());

    assert_eq!(
        documented, declared,
        "§1's printed `LanguageHandler` and the real trait disagree on their \
         methods. Unlike the types beside it this one is *implemented* \
         downstream, so a method printed here and absent there is a language \
         crate that does not compile, and the reverse is a requirement nobody \
         reading the design would know about"
    );
}

/// The three enums §1 prints that a `lang_*` crate has to *name*, rather than
/// merely receive: a stratum and an abstention reason are what a handler
/// returns, and a refinement is the only thing `Strata::refine` accepts.
///
/// Held for the reason the two tests above are (CHANGE-core-018): the block is
/// the only description of the seam a language author reads. A variant printed
/// here and absent there is a handler that does not compile; a variant present
/// there and unprinted is one nobody knows to return — and for `Stratum` that
/// is a `high-level.md` stratification row with no coverage in any table, which
/// reads as a resolution failure rather than as a class nobody classified.
///
/// `Stratum` specifically is the one where the count is load-bearing beyond the
/// seam: §7 groups coverage by it, so the denominator is the variant list, and
/// `Unimplemented` is the gate check that the template has been replaced.
/// The input side of the seam. `Outcome` is what a handler builds and `Query`
/// is everything it is given, so a field printed here and absent there is a
/// handler that does not compile, and a field present there and unprinted is a
/// capability no language author knows they have — `policy`, before
/// `conformance-013`, was exactly that shape of omission.
#[test]
fn the_printed_query_gives_a_handler_what_the_real_one_gives_it() {
    let documented = members(&printed(&document()), "pub struct Query<'a> {");
    let declared = members(&source(), "pub struct Query<'a> {");

    assert!(
        !documented.is_empty(),
        "§1 prints a `Query` with no fields, so this comparison is vacuous"
    );
    assert_eq!(
        documented, declared,
        "§1's printed `Query` and `shared::handler`'s disagree. This is the whole \
         of what a handler is handed — the snapshot, the position, the project \
         view, the deadline, the server it stands in for and the commit policy — \
         and the block is where a language author reads it"
    );
}

/// §1 argues at length that `ServerProfile`'s identity is private behind "one
/// constructor per situation", so that the case which silently loses
/// information — a caller that knows which server it is standing in for and
/// passes `None` anyway — is not expressible.
///
/// That argument is exactly as good as the constructor list, which is why the
/// list is what is held. The prose beside it had already drifted: the source
/// said "the constructors are the two situations" while three were declared
/// under it, which is the same sentence in the same shape as the one
/// CHANGE-core-018 found in the document.
#[test]
fn the_printed_server_profile_has_one_constructor_per_situation() {
    let documented = functions(&printed(&document()), "impl ServerProfile {");
    let declared = functions(&source(), "impl ServerProfile {");

    assert!(
        documented.len() > 1,
        "§1 prints {documented:?} for `ServerProfile`, so there is no list here to \
         hold"
    );
    assert_eq!(
        documented, declared,
        "§1's printed `ServerProfile` and the real one disagree on their \
         constructors. A situation with no constructor is one a caller spells as \
         another, which is the absence §1 spends a paragraph making unspellable"
    );
}

/// Function names declared directly inside the block `header` opens, in order.
///
/// Separate from [`methods`] because an inherent impl writes `pub fn` and
/// `pub const fn` where a trait writes a bare `fn`.
fn functions(text: &str, header: &str) -> Vec<String> {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("no `{header}` in this side of the comparison"));
    enclosed(&text[start + header.len() - 1..])
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let rest = line.trim_start_matches("pub ").trim_start_matches("const ");
            rest.strip_prefix("fn ")
        })
        .filter_map(|rest| rest.split('(').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_printed_enums_a_handler_returns_have_the_variants_it_may_return() {
    let block = printed(&document());
    let source = source();

    for header in [
        "pub enum Stratum {",
        "pub enum AbstainReason {",
        "pub enum Refinement {",
    ] {
        let documented = variants(&block, header);
        let declared = variants(&source, header);
        assert!(
            !documented.is_empty(),
            "§1 prints no variants under `{header}`, so this comparison is vacuous"
        );
        assert_eq!(
            documented, declared,
            "§1's printed `{header}` and `shared::handler`'s disagree"
        );
    }
}

/// Variant names of the enum introduced by `header`, in declaration order.
///
/// Scanning rather than parsing, for the reason [`members`] gives — but the
/// body is delimited by counting braces rather than by looking for a `}` in
/// column one. The document prints `Refinement` on a single line and the source
/// spreads it over four, and `AbstainReason::External` carries a braced field:
/// a line rule gets one of those three wrong whichever way it is written, and
/// the failure is silent, because swallowing the *next* enum still yields a
/// list of plausible variant names.
fn variants(text: &str, header: &str) -> Vec<String> {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("no `{header}` in this side of the comparison"));
    let body = enclosed(&text[start + header.len() - 1..]);

    body.split(',')
        // The last line, so that the doc comments both sides carry are behind
        // the name rather than in front of it. Their own commas split the body
        // too and leave pieces ending in a `///` line, which the capital below
        // drops.
        .filter_map(|piece| piece.lines().next_back())
        .map(str::trim)
        .filter_map(|line| line.split([' ', '(', '{', ':']).next())
        .filter(|name| {
            name.starts_with(|first: char| first.is_ascii_uppercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_owned)
        .collect()
}

/// The text between `text`'s leading `{` and the `}` that closes it.
fn enclosed(text: &str) -> &str {
    let mut depth = 0_usize;
    for (index, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[1..index];
                }
            }
            _ => {}
        }
    }
    panic!("an unclosed body in {text:.60}")
}

/// §1's "handlers never construct `Outcome::Committed`; every path ends
/// through `policy.decide(..)`", which is the one claim in the section that the
/// types deliberately do not hold.
///
/// The section says so itself, and says what does: "what holds this is review
/// and not the type system … the check available before then is mechanical
/// rather than architectural: a source scan over `crates/lang_*` for the
/// construction, in the shape `driver/tests/seam.rs` already uses for the wire
/// vocabulary". This is that scan. Until it existed the sentence described a
/// check nobody had written, which is the weaker of the two things it could
/// have meant.
///
/// Why it matters only later is the section's own argument, and is why the
/// answer is a scan rather than a private variant: in v1 `decide` returns
/// `Committed` for every input, so a handler that skips the funnel is
/// indistinguishable from one that does not. It becomes distinguishable the
/// moment a precision floor arrives — and that is the moment there are the most
/// `lang_*` crates to audit by hand, which is exactly the audit
/// `resolution.md` §7.4 refuses to schedule.
#[test]
fn no_language_crate_constructs_the_committed_arm_itself() {
    let sources = language_sources_that_implement_the_seam();

    let mut constructing = Vec::new();
    for (file, text) in &sources {
        for line in text.lines() {
            let code = line.trim_start();
            // A `lang_*` that explains in a comment why it does not build one
            // must not be what fails this. `driver/tests/file_list.rs`'s
            // channel scan skips comments for the same reason, and there the
            // trap had already been set.
            if code.starts_with("//") {
                continue;
            }
            if code.contains("Outcome::Committed") {
                constructing.push(format!("{file}: {code}"));
            }
        }
    }

    assert!(
        constructing.is_empty(),
        "a language crate names `Outcome::Committed` in code: {constructing:?}. §1 \
         routes every committed answer through `CommitPolicy::decide`, so that a \
         per-mode precision floor is a data change rather than an audit of every \
         commit site in every `lang_*` crate at the moment when there are the most \
         of them"
    );
}

/// §1's other claim that the types deliberately do not hold: "handlers must
/// not dispatch on server *identity* — `if server.id == PYRIGHT` scattered
/// through a handler is the per-language configuration format
/// `resolution.md` §1.2 rules out, wearing yet another hat. A handler reads a
/// field describing a behaviour; it does not ask which server it is talking
/// to."
///
/// Worth a scan because it is one accessor away from possible, and it took
/// reading `vocabulary.rs` to be sure it was not already prevented: the field
/// inside `ServerId` is private and the type carries no public constructor
/// from a `&'static str`, which reads like the identity cannot be named at
/// all. It can. `ServerId::KNOWN` is a `pub const` of all eight servers in the
/// matrix and `ServerId::from_name` is public, so `q.server.id() ==
/// ServerId::from_name("pyright")` compiles today from any language crate.
///
/// `ServerProfile::id` is the same claim from the other side, and §1 names
/// that one too: "`None` in standalone, and when proxying a server we have no
/// profile for. A handler that branches on this is doing something wrong."
/// There is nothing else on `ServerProfile` for a handler to read — §1 has it
/// "Empty in v1: a field appears only once the corpus shows a systematic
/// divergence that a field would fix" — so in v1 every use of the accessor is
/// the identity dispatch this refuses, and a behaviour field arriving is what
/// makes the distinction need drawing.
#[test]
fn no_language_crate_asks_which_server_it_is_standing_in_for() {
    let sources = language_sources_that_implement_the_seam();

    let mut asking = Vec::new();
    for (file, text) in &sources {
        for line in text.lines() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if code.contains("ServerId") || code.contains("server.id()") {
                asking.push(format!("{file}: {code}"));
            }
        }
    }

    assert!(
        asking.is_empty(),
        "a language crate asks which server it is standing in for: {asking:?}. core.md §1 \
         makes ServerProfile data rather than a trait for exactly this reason — a handler \
         reads a field describing a behaviour, and a handler that branches on identity has \
         rebuilt the per-language configuration format resolution.md §1.2 rules out"
    );
}

/// The `lang_*` sources, with the two vacuity checks every scan over them
/// needs: that there are some, and that they are the files the seam is
/// actually implemented in rather than whatever the module walk happened to
/// reach.
fn language_sources_that_implement_the_seam() -> Vec<(String, String)> {
    let sources = language_crate_sources();
    assert!(
        !sources.is_empty(),
        "no crates/lang_* workspace member, so this scan would pass vacuously"
    );
    assert!(
        sources
            .iter()
            .any(|(_, text)| text.contains("impl LanguageHandler")),
        "none of {:?} implements the seam, so this walked the wrong files and would pass \
         against a handler doing the very thing being scanned for",
        sources.iter().map(|(file, _)| file).collect::<Vec<_>>()
    );
    sources
}

/// Every source file of every `crates/lang_*` member, as `(crate/src/name.rs,
/// text)`.
///
/// Reached by following each crate root's `mod` declarations, which is
/// `clippy.toml`'s rule — `std::fs::read_dir` bypasses gitignore semantics —
/// and `core.md` §9's convention that the library root is named for the crate.
fn language_crate_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/shared is two levels below the workspace root")
        .to_owned();
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest");

    let mut sources = Vec::new();
    for line in manifest.lines() {
        let quoted = line.trim().trim_end_matches(',');
        let Some(member) = quoted
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .and_then(|member| member.strip_prefix("crates/"))
            .filter(|member| member.starts_with("lang_"))
        else {
            continue;
        };
        let entry = format!("crates/{member}/src/{member}.rs");
        let text = std::fs::read_to_string(root.join(&entry))
            .unwrap_or_else(|_| panic!("{entry}, which §9 names the library root"));
        for declared in text.lines().filter_map(module_of) {
            let path = format!("crates/{member}/src/{declared}.rs");
            let source = std::fs::read_to_string(root.join(&path)).expect("a declared module");
            sources.push((path, source));
        }
        sources.push((entry, text));
    }
    sources
}

fn module_of(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("mod ")
        .or_else(|| line.trim().strip_prefix("pub mod "))
        .and_then(|rest| rest.strip_suffix(';'))
}

/// §1 prints `Trace` with two claims attached to the type rather than to its
/// members: "**Boxed and not allocated until something is reported**, so the
/// commonest abstention does not pay for a channel it never writes to."
///
/// **The first half is held here and the second is not, and the difference is
/// worth writing down rather than rediscovering.** A `Trace` that inlined its
/// six fields is caught by the width below, which is the property the source
/// says the shape was for: one pointer wide, so that widening `Outcome` did not
/// widen every `Result` carrying one past clippy's `result_large_err`
/// threshold. A `Trace` that kept `Option<Box<_>>` but filled it in `new()` is
/// **not** caught, by this or by anything else in the repository — it produces
/// a byte-identical record, so every test in `driver` and `measure_core` passes
/// against it too. Observing an allocation needs a counting
/// `#[global_allocator]`, and `GlobalAlloc` cannot be implemented without
/// `unsafe`, which `CLAUDE.md` bans outright. So the second half is a
/// performance claim held by review, and the assertion that a fresh trace
/// reports *nothing* is the part of it that has observable consequences.
#[test]
fn a_trace_is_one_pointer_wide_and_an_unwritten_one_reports_nothing() {
    assert_eq!(
        size_of::<Trace>(),
        size_of::<*const ()>(),
        "a `Trace` is one pointer wide, which is what let §1 widen `Outcome` \
         with one. Inlined, this is six fields, and `driver`'s `Dispatched` \
         crossed `result_large_err` the last time it was"
    );

    let parts = Trace::new().into_parts();
    assert!(
        parts.stages.is_empty()
            && parts.stage_us.is_empty()
            && parts.bytes_scanned == ByteLen::ZERO
            && parts.files_parsed == FileCount(0)
            && parts.margin.is_none()
            && parts.considered.is_none(),
        "a handler that reported nothing produces §7's columns at the values \
         that say no account was given, not zeroes standing in for \
         measurements nobody took: {parts:?}"
    );
}

/// The three accumulating writers on §1's reporting channel, each of which the
/// source describes and none of which anything reads back.
///
/// `stage` is the one with two ways to be wrong rather than one. §7 asks for "a
/// small fixed maximum number of short labels, **truncated rather than
/// grown**", and the source adds which end goes: "Dropping the tail rather than
/// the head keeps the prefix stable across runs, which is what lets failures be
/// *grouped* by `stages` rather than merely listed." A ring buffer that dropped
/// the head would satisfy the bound and destroy the grouping, and nothing in
/// the repository reports more than two stages, so no existing test reaches the
/// bound at all.
#[test]
fn an_account_is_truncated_at_its_tail_and_a_re_entered_stage_accumulates() {
    let mut trace = Trace::new();
    let overrun = Trace::MAX_STAGES + 8;
    for stage in 0..overrun {
        trace.stage(StageLabel::new(&format!("stage:{stage}")));
    }

    // A stage re-entered during a fan-out is one stage that cost more, so the
    // second call adds to the first rather than replacing it.
    trace.timed(StageName::new("search"), Micros(900));
    trace.timed(StageName::new("search"), Micros(100));
    trace.scanned(ByteLen(1234));
    trace.scanned(ByteLen(1));
    trace.parsed(FileCount(3));
    trace.parsed(FileCount(1));
    trace.ranked(
        Margin::new(0.5).expect("a finite, non-negative margin"),
        CandidateCount(7),
    );

    let parts = trace.into_parts();
    let labels: Vec<&str> = parts.stages.iter().map(StageLabel::as_str).collect();
    assert_eq!(
        labels.len(),
        Trace::MAX_STAGES,
        "{overrun} stages were reported and the bound is {}, so the account \
         grew instead of truncating",
        Trace::MAX_STAGES
    );
    assert_eq!(
        (labels.first(), labels.last()),
        (Some(&"stage:0"), Some(&"stage:31")),
        "the tail is what goes. A trace that kept the last {} labels would \
         satisfy the bound and still make two runs of the same query \
         ungroupable, which is the only thing §7 asks `stages` for",
        Trace::MAX_STAGES
    );

    assert_eq!(
        parts.stage_us.get(&StageName::new("search")),
        Some(&Micros(1000)),
        "a stage entered twice cost 900µs and then 100µs, so it cost 1000µs"
    );
    assert_eq!(
        (parts.bytes_scanned, parts.files_parsed),
        (ByteLen(1235), FileCount(4)),
        "`scanned` and `parsed` are counters, so a second report adds to the \
         first rather than replacing it"
    );
    assert_eq!(parts.considered, Some(CandidateCount(7)));
}

/// §1 gives `margin` a newtype so that "the 0.0..=1.0 invariant is checked once
/// in the constructor instead of assumed at every comparison" — the argument it
/// makes for `Confidence`, applied here to the half of it that survives: a
/// margin is a difference between the handler's own scores, so there is no
/// upper bound, and what is left to check is that it is a difference at all.
///
/// Both non-finite cases have to be named separately, because the two halves of
/// the guard catch different ones and either half alone looks sufficient: `>=
/// 0.0` is false for a NaN, so dropping `is_finite` still rejects it and lets
/// an infinity through as a margin that outranks every real one.
#[test]
fn a_margin_is_a_gap_between_two_scores_or_it_is_not_a_margin() {
    assert!(
        Margin::new(-0.1).is_none(),
        "a negative margin says the runner-up outranked the top candidate"
    );
    assert!(Margin::new(f32::NAN).is_none(), "a NaN is not a gap");
    assert!(
        Margin::new(f32::INFINITY).is_none(),
        "an infinite margin is accepted by `>= 0.0` alone, and would outrank \
         every margin a handler could actually measure"
    );
    assert!(
        Margin::new(0.0).is_some() && Margin::new(12.5).is_some(),
        "a tie and an ordinary gap are both margins -- without this the \
         rejections above pass against a constructor that refuses everything"
    );
}

/// The trait's method names, which are the part of it a language crate has to
/// write. Bodies and signatures are left alone: `Query<'_>`'s parameter is
/// spelled `q` in the document and `query` in the source, and neither spelling
/// is a claim about anything.
fn methods(text: &str) -> Vec<String> {
    let start = text
        .find("pub trait LanguageHandler")
        .expect("both sides declare the trait");
    let body = &text[start..];
    let body = &body[..body.find("\n}").expect("an unclosed trait body")];
    body.lines()
        .filter_map(|line| line.trim().strip_prefix("fn "))
        .filter_map(|rest| rest.split('(').next())
        .map(str::to_owned)
        .collect()
}
