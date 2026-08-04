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
