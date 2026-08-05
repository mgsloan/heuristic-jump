//! `design/core.md` §2: the parse happens inside the worker **and inside the
//! deadline**, and the tree a handler gets was produced from the text it was
//! given.
//!
//! The deadline half is the one nothing else would notice being wrong. A
//! `realise` that ignored its deadline would pass every other test in the
//! workspace — the fixtures are small and parse in microseconds — and would
//! only show up as a proxy that stops answering on somebody's large generated
//! file, which is the failure `high-level.md` puts a latency budget in front
//! of. So the document here is deliberately large enough for tree-sitter's
//! progress callback to fire, and one test asserts what happens when it does
//! not.
//!
//! **The section's printed block is held against these two types**, in the
//! shape and for the reason `tests/handler.rs` holds §1's — the block is prose
//! to everything that reads it, and editing the document is the way of faking
//! progress an audit cannot catch. The scanner is copied rather than shared
//! because an integration test file that exported one would be compiled as a
//! test binary of its own; `handler.rs` is the original and carries the
//! argument for what the comparison elides.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "`clippy.toml`'s allow-expect-in-tests and allow-panic-in-tests reach only `#[test]` bodies, and the builders and the block readers below are free functions. Failing loudly is the point: a seed that will not parse makes every assertion here vacuous, and so does a section this cannot find — it would compare an empty block against the source and pass."
)]

use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::{Strategy, prop_assert_eq};
use proptest::proptest;
use shared::{
    ByteLen, ByteRange, Clock, Deadline, DocumentSnapshot, DocumentUri, DocumentVersion, Error,
    HandlerError, LanguageId, Offset, Rope, SnapshotSeed, TestClock, input_edit,
};
use tree_sitter::{InputEdit, Language, Point};

const RUST: LanguageId = LanguageId::new("rust");

/// Small enough that tree-sitter finishes it inside one progress-check
/// interval and never calls the callback — which is the point of
/// `a_parse_too_small_to_report_progress_is_not_abandoned`.
const SMALL: &str = "fn caller() {\n    target();\n}\n";

/// `core.md` §2's printed block, held against the two types it prints.
///
/// The field lists are the claim, not decoration. "A snapshot is three
/// refcount bumps and a struct move, regardless of file size" is true of
/// exactly the fields printed there — a `Rope` and a `Tree` clone by refcount
/// and the rest are small — so a field added to either type without touching
/// the section leaves the document asserting an O(1) that the type no longer
/// has. There is nowhere else a reader is told what a snapshot costs, and a
/// `String` or a `Vec` here would be invisible to every other test in the
/// workspace: they would all still pass, a little slower, on fixtures small
/// enough that nobody could tell.
///
/// The seed is checked beside the snapshot because the split is the design.
/// `base` and `grammar` are what `core` holds and a handler must not, and a
/// field that moved from one type to the other would be §2's whole argument
/// inverted with both lists still the right length.
#[test]
fn the_printed_snapshot_types_have_the_fields_the_real_ones_have() {
    let block = printed(&document(), SNAPSHOTS);
    let source = source();

    for header in ["pub struct SnapshotSeed {", "pub struct DocumentSnapshot {"] {
        let documented = members(&block, header);
        let declared = members(&source, header);
        assert!(
            !documented.is_empty(),
            "§2's printed block declares no fields for `{header}`, so this comparison would \
             pass against anything"
        );
        assert_eq!(
            documented, declared,
            "§2's printed `{header}` and `shared::document`'s disagree. The section's claim \
             is that a snapshot is three refcount bumps and a struct move whatever the file's \
             size, and that claim is about this field list and no other"
        );
    }
}

/// §2: "`DocumentSnapshot` contains no synchronisation primitive, and that is
/// the point of the two-step shape."
///
/// The section arrives there by rejecting a working design. Handlers fan out
/// across candidate files, so `&Query` — and therefore `&DocumentSnapshot` —
/// crosses threads and must be `Sync`; an earlier revision got that by
/// memoising the parse in a `std::sync::OnceLock`, "which works and is `Sync`,
/// but is a blocking primitive on the query path in a design whose stated rule
/// is that there are no locks anywhere". Parsing eagerly removes the question
/// instead of excusing it.
///
/// `clippy.toml` does not hold this. It denies `Mutex`, `RwLock` and `Condvar`
/// workspace-wide, and the tempting thing here is none of them: a `OnceLock` or
/// a `Cell` reintroduces exactly what §2 removed while passing every lint, and
/// it would arrive with a plausible reason — a memoised line index, a lazily
/// resolved root — because a type this widely shared is where memoisation
/// looks free.
///
/// Comment lines are skipped, and the doc comment on the struct names
/// `OnceLock` in as many words. A scan that read prose would fail on the
/// sentence explaining why the thing it looks for is absent.
#[test]
fn a_snapshot_carries_nothing_a_handler_could_contend_on() {
    let source = source();
    for header in ["pub struct SnapshotSeed {", "pub struct DocumentSnapshot {"] {
        let body = body(&source, header);
        assert!(
            body.contains("uri"),
            "no fields found for `{header}`, so this scan would pass against anything"
        );
        for primitive in [
            "OnceLock", "OnceCell", "Cell<", "RefCell", "Mutex", "RwLock", "Atomic",
        ] {
            assert!(
                !body.contains(primitive),
                "`{header}` carries a {primitive}. §2's two-step split exists so that the \
                 snapshot a handler fans out across threads with needs no primitive at all — \
                 a memoised field here is a blocking call on the query path, and the design's \
                 rule is that there are no locks anywhere"
            );
        }
    }
}

/// A declaration's body, comments dropped, from `header` to its closing brace.
fn body(text: &str, header: &str) -> String {
    let Some(start) = text.find(header).map(|at| at + header.len()) else {
        return String::new();
    };
    let mut depth = 1usize;
    let mut kept = String::new();
    for line in text[start..].lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            kept.push_str(trimmed);
            kept.push('\n');
        }
        depth += trimmed.matches(['{', '(']).count();
        depth = depth.saturating_sub(trimmed.matches(['}', ')']).count());
        if depth == 0 {
            break;
        }
    }
    kept
}

/// The section that prints the two types. Sliced by heading rather than by
/// line number, which moves.
const SNAPSHOTS: &str = "### Snapshots are O(1) to take";

fn document() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/core.md");
    std::fs::read_to_string(&path).expect("design/core.md is at the workspace root")
}

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/document.rs");
    std::fs::read_to_string(&path).expect("the types this section prints")
}

/// The first fenced block after a heading.
fn printed(document: &str, heading: &str) -> String {
    let section = document
        .find(heading)
        .map(|start| &document[start..])
        .unwrap_or_else(|| panic!("core.md has no {heading}"));
    let open = section
        .find("```")
        .map(|start| start + "```".len())
        .expect("§2 prints the snapshot types in a fenced block");
    let body = &section[open..];
    let body = &body[body.find('\n').map_or(0, |line| line + 1)..];
    body[..body.find("```").expect("an unclosed fenced block")].to_owned()
}

/// Named members of a brace-delimited group, in order. `handler.rs` explains
/// why this scans rather than parses; the two shapes that would otherwise be
/// false positives are a path (excluded by its second colon) and a generic
/// argument (excluded by having no colon).
fn members(text: &str, header: &str) -> Vec<String> {
    let Some(start) = text.find(header).map(|at| at + header.len()) else {
        return Vec::new();
    };
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

/// §2's central claim, and the reason `tree()` is infallible: there is one
/// tree and it was produced from `text`.
#[test]
fn the_tree_a_handler_gets_spans_the_text_it_was_given() {
    let document = realised(SMALL, &Deadline::none());

    assert_eq!(
        ByteLen(document.tree().root_node().end_byte()),
        document.text.len(),
        "core.md §2: the tree and the text cannot disagree, because the tree was \
         parsed from that text"
    );
}

/// The stale tree never leaves the seed: an incremental seed carries a tree
/// parsed from an *older* version plus the edits that reconcile it, and what
/// comes out spans the *new* text.
///
/// Handing a handler the v1 tree with the v2 text is the trap §2 is written
/// against — every offset in it is wrong for that text, and nothing detects it
/// until it produces a confidently wrong jump.
#[test]
fn an_incremental_reparse_produces_a_tree_for_the_new_text() {
    let before = realised(SMALL, &Deadline::none());
    let addition = "fn target() {}\n";
    let after = Rope::from(format!("{SMALL}{addition}").as_str());

    let edit = InputEdit {
        start_byte: SMALL.len(),
        old_end_byte: SMALL.len(),
        new_end_byte: SMALL.len() + addition.len(),
        start_position: Point::new(3, 0),
        old_end_position: Point::new(3, 0),
        new_end_position: Point::new(4, 0),
    };
    let document = SnapshotSeed::incremental(
        uri(),
        after.clone(),
        DocumentVersion(2),
        RUST,
        grammar(),
        before.tree().clone(),
        Arc::new(vec![edit]),
    )
    .realise(&Deadline::none())
    .expect("reparsing incrementally");

    assert_eq!(
        ByteLen(document.tree().root_node().end_byte()),
        after.len(),
        "the reparse returned a tree that stops where the *old* text stopped, so \
         every offset past the edit is wrong for the text beside it (core.md §2)"
    );
    assert_eq!(
        document.version,
        DocumentVersion(2),
        "the realised snapshot is not at the version its seed was built for"
    );
}

/// A parse that runs past its deadline is abandoned, and reported as the
/// **expiry** rather than as the document failing to parse.
///
/// The distinction is what §7's record is built to keep: a query that ran out
/// of time is an abstention and costs coverage, where a document that will not
/// parse is a failure and means something is broken. `driver::dispatch` maps
/// only the first back to `AbstainReason::Deadline`, so putting this in
/// `ParseError` would log every slow parse as a handler bug.
#[test]
fn a_parse_that_runs_past_its_deadline_is_abandoned_rather_than_failed() {
    let clock = Arc::new(TestClock::new());
    let started = clock.now();
    let budget = Duration::from_millis(20);
    let deadline = Deadline::new(Arc::clone(&clock) as Arc<dyn Clock>, started, budget);
    // Already over, before the parse begins. §5's deadline is absolute and
    // starts at request arrival, so a query that queued for longer than its
    // budget arrives in exactly this state.
    clock.advance(budget + Duration::from_millis(1));

    match seed(&large()).realise(&deadline) {
        Err(Error::Handler(HandlerError::DeadlineExpired { classified: None })) => {}
        Err(other) => panic!(
            "an abandoned parse was reported as {other:?}, where core.md §1 has exactly \
             one error class mapped back to an abstention"
        ),
        Ok(document) => panic!(
            "a {} byte parse ran to completion under an expired deadline, so nothing \
             bounds it: core.md §2 pays the parse inside the deadline",
            document.text.len()
        ),
    }
}

/// `$/cancelRequest` and the client going away are not latency, and
/// `Deadline::expired` reports both — so the parse has to observe the flag as
/// well as the clock, or a cancelled query keeps a worker busy for as long as
/// the parse takes.
#[test]
fn a_cancelled_query_abandons_its_parse_too() {
    let deadline = Deadline::none();
    deadline.cancel();

    match seed(&large()).realise(&deadline) {
        Err(Error::Handler(HandlerError::DeadlineExpired { classified: None })) => {}
        Err(other) => panic!("a cancelled parse was reported as {other:?}"),
        Ok(_) => panic!(
            "Deadline::none() is unbounded in *time* only: cancellation still has to \
             stop the parse (shared::Deadline)"
        ),
    }
}

/// The honest limit on the claim above, asserted rather than left as prose.
///
/// tree-sitter calls the progress callback once every 100 parser operations
/// (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `src/parser.c:81`), so a parse that
/// finishes inside one interval observes no deadline at all and returns a tree
/// however expired the query was. That is why `driver::hard_cap` still
/// exists behind this: the cap is what makes a late answer harmless, and the
/// callback is only what stops the *work*.
///
/// If this test starts failing, tree-sitter has become more eager and the
/// abstention is tighter than §2 promises — which is a better world, and a
/// deliberate decision to record rather than a regression to fix.
#[test]
fn a_parse_too_small_to_report_progress_is_not_abandoned() {
    let deadline = Deadline::none();
    deadline.cancel();

    let document = seed(SMALL)
        .realise(&deadline)
        .expect("a parse below the progress callback's granularity still finishes");

    assert_eq!(
        ByteLen(document.tree().root_node().end_byte()),
        document.text.len(),
        "the tree returned by an unbounded-in-practice parse is still a tree for its text"
    );
    assert!(
        SMALL.len() < 1024,
        "this test only says anything while the document is small enough that \
         tree-sitter never reports progress on it"
    );
}

proptest! {
    /// `input_edit` against a reference implementation, for the same reason
    /// §10 tests position encoding against one: the six fields are three byte
    /// offsets and three row-and-column pairs describing the same replacement,
    /// and a value that lands in the wrong one of them is still a plausible
    /// number.
    ///
    /// It has to be a reference rather than a tree, and this is worth being
    /// explicit about because the obvious test does not work. Driving an edit
    /// through `SnapshotSeed::incremental` and comparing the tree against a
    /// from-scratch parse — node kinds, byte ranges and point ranges, over
    /// randomised edits — *passes with all three point fields wrong*, because
    /// `realise`'s read callback is byte-based and tree-sitter recomputes
    /// every position it re-lexes from the text itself. So the point fields
    /// are inert as far as any tree can tell, and the only thing that can
    /// observe them is a second implementation. `driver`'s
    /// `the_tree_matches_the_text_at_every_staleness` covers the byte fields,
    /// which are the ones the reparse does consume.
    ///
    /// The reference is deliberately the slow obvious one: `&str`, counting
    /// newlines, and computing the new end from the text that *results* —
    /// where `input_edit` computes it from the inserted text alone, without
    /// building the result at all.
    #[test]
    fn an_input_edit_describes_the_replacement_it_was_built_from(
        (before, start, end, inserted) in replacement(),
    ) {
        let edit = input_edit(
            &Rope::from(before.as_str()),
            ByteRange { start: Offset(start), end: Offset(end) },
            &inserted,
        );
        let after = format!("{}{inserted}{}", &before[..start], &before[end..]);

        prop_assert_eq!(edit.start_byte, start);
        prop_assert_eq!(edit.old_end_byte, end);
        prop_assert_eq!(edit.new_end_byte, start + inserted.len());
        prop_assert_eq!(edit.start_position, at(&before, start));
        prop_assert_eq!(edit.old_end_position, at(&before, end));
        prop_assert_eq!(edit.new_end_position, at(&after, start + inserted.len()));
    }
}

/// A text, a character-aligned range within it, and something to put there.
fn replacement() -> impl Strategy<Value = (String, usize, usize, String)> {
    let fragment = || proptest::sample::select(FRAGMENTS);
    let text = || proptest::collection::vec(fragment(), 0..8).prop_map(|parts| parts.concat());
    (
        text(),
        text(),
        proptest::num::usize::ANY,
        proptest::num::usize::ANY,
    )
        .prop_map(|(before, inserted, first, second)| {
            let start = boundary(&before, first % (before.len() + 1));
            let end = boundary(&before, start + second % 32);
            (before, start, end, inserted)
        })
}

/// Fragments whose widths differ in every way that matters: one-byte
/// characters, a two-byte one, a four-byte one, and line endings both bare and
/// as `\r\n`, since a column is bytes and `\r` is part of the line it ends.
const FRAGMENTS: &[&str] = &[
    "",
    " ",
    "\n",
    "\r\n",
    "fn f() {}\n",
    "é",
    "😀",
    "let x = 1;\n",
    "//\n\n",
    "}",
];

fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// Row and byte-column of `offset`, counted the slow way.
fn at(text: &str, offset: usize) -> Point {
    let head = &text[..offset];
    Point {
        row: head.matches('\n').count(),
        column: head.len() - head.rfind('\n').map_or(0, |newline| newline + 1),
    }
}

/// Big enough that tree-sitter reports progress while parsing it, which is
/// what makes the deadline observable at all. Around 46 KB.
///
/// The granularity is 100 *parser operations* rather than a byte count
/// (`OP_COUNT_PER_PARSER_CALLBACK_CHECK`, `tree-sitter/src/parser.c:81`), so
/// there is no size at which a document is guaranteed to be interruptible —
/// only sizes at which it reliably is, and this is one.
fn large() -> String {
    (0..800)
        .map(|index| {
            format!("fn generated_{index}(argument: u32) -> u32 {{ argument + {index} }}\n")
        })
        .collect()
}

fn realised(text: &str, deadline: &Deadline) -> DocumentSnapshot {
    seed(text)
        .realise(deadline)
        .expect("parsing a fixture document")
}

fn seed(text: &str) -> SnapshotSeed {
    SnapshotSeed::fresh(uri(), Rope::from(text), DocumentVersion(1), RUST, grammar())
}

fn uri() -> DocumentUri {
    DocumentUri::from_file_path(std::path::Path::new("/fixture/src/lib.rs"))
        .expect("a file URI for a fixture path")
}

fn grammar() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}
