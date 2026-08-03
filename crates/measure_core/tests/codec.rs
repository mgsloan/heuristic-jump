//! `design/core.md` §10's fourth bullet: "fuzz the frame codec, including
//! split reads, oversized headers, and bogus `Content-Length`". `deps.md` §12
//! declines `arbitrary`/`cargo-fuzz` for now and says proptest covers those
//! three cases well enough to start, so this is that file.
//!
//! The three are not variations on one test, and the reason is worth stating
//! because it decides what the generators look like:
//!
//! * **Split reads** are about the codec's *state*, not its inputs. Every
//!   framed read is `read_line` until a blank line and then a body of exactly
//!   the declared length, and a reader that returns one byte at a time must
//!   produce the same bodies as one that returns the whole stream at once —
//!   including leaving the stream positioned so the *next* frame reads
//!   correctly, which is the half a single-frame test cannot see. So the
//!   generator varies the chunk size independently of the content, and every
//!   property below is checked at every chunk size.
//! * **Oversized headers** and **bogus `Content-Length`** are about what the
//!   codec does with a claim it cannot check. A `Content-Length` is a
//!   statement about bytes that have not arrived; a header line is unbounded
//!   until a `\n` arrives that may never. Both are refusals rather than
//!   errors-after-the-fact: the assertion that matters is that the refusal
//!   happens *before* the allocation the claim asks for, which is why
//!   `content_length_past_the_limit_costs_nothing` names `usize::MAX` and is
//!   expected to finish.
//!
//! What this file deliberately does not do is drive a `Client`. The codec is a
//! free function over `&mut dyn BufRead` precisely so that fuzzing it does not
//! mean spawning a language server, and a test that spawned one would be
//! measuring the server's framing rather than ours.

use std::io::{BufRead, BufReader, Read, Result as IoResult};
use std::path::Path;

use measure_core::{MAX_FRAME_BYTES, MAX_HEADER_BYTES, read_frame};
use proptest::prelude::{ProptestConfig, Strategy, prop_assert, prop_assert_eq};
use proptest::proptest;
use shared::{CodecError, Error};

/// A reader that hands back at most `chunk` bytes per call, so the codec meets
/// the partial reads a pipe actually produces. `BufReader` sits on top of it
/// the way it does over a child's stdout in `client.rs`, which is what makes
/// the buffering under test the same buffering that ships.
#[derive(Debug)]
struct Chunked {
    bytes: Vec<u8>,
    position: usize,
    chunk: usize,
}

impl Read for Chunked {
    fn read(&mut self, out: &mut [u8]) -> IoResult<usize> {
        let remaining = &self.bytes[self.position..];
        let take = remaining.len().min(out.len()).min(self.chunk);
        out[..take].copy_from_slice(&remaining[..take]);
        self.position += take;
        Ok(take)
    }
}

fn reader(bytes: Vec<u8>, chunk: usize) -> BufReader<Chunked> {
    BufReader::new(Chunked {
        bytes,
        position: 0,
        chunk: chunk.max(1),
    })
}

fn peer() -> &'static Path {
    Path::new("a-language-server")
}

fn read(stream: &mut dyn BufRead) -> Result<Vec<u8>, Error> {
    read_frame(stream, peer())
}

fn framed(body: &[u8]) -> Vec<u8> {
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend_from_slice(body);
    frame
}

/// Which `CodecError` a failure is, as a value that can be compared, so an
/// assertion reads as one equality rather than a `matches!` per call site.
///
/// `Unrecognised` is the arm that matters. `CodecError` is `#[non_exhaustive]`
/// and this is a different crate, so the compile error `CLAUDE.md` wants from
/// a new variant is not available here at any price — what is available is
/// that a new variant maps to a value no assertion below expects, which turns
/// it into a failing test instead of a silent reclassification.
///
/// The `_` arm below does not trip `wildcard_enum_match_arm`, and that is the
/// lint working rather than a hole in it: every variant `shared` declares is
/// named above, so the arm covers nothing the compiler can see and the lint
/// has nothing to complain about. Delete one of the named arms and it fires.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Failure {
    MalformedHeader,
    MissingContentLength,
    BadContentLength,
    HeaderTooLong,
    FrameTooLarge,
    Truncated,
    BodyNotUtf8,
    BodyNotJson,
    NotSerializable,
    Unrecognised,
    NotACodecFailure,
}

fn failure(error: &Error) -> Failure {
    let Error::Codec(codec) = error else {
        return Failure::NotACodecFailure;
    };
    match codec {
        CodecError::MalformedHeader { .. } => Failure::MalformedHeader,
        CodecError::MissingContentLength => Failure::MissingContentLength,
        CodecError::BadContentLength { .. } => Failure::BadContentLength,
        CodecError::HeaderTooLong { .. } => Failure::HeaderTooLong,
        CodecError::FrameTooLarge { .. } => Failure::FrameTooLarge,
        CodecError::Truncated { .. } => Failure::Truncated,
        CodecError::BodyNotUtf8 => Failure::BodyNotUtf8,
        CodecError::BodyNotJson { .. } => Failure::BodyNotJson,
        CodecError::NotSerializable { .. } => Failure::NotSerializable,
        _ => Failure::Unrecognised,
    }
}

fn bodies() -> impl Strategy<Value = Vec<Vec<u8>>> {
    proptest::collection::vec(
        proptest::collection::vec(proptest::num::u8::ANY, 0..64),
        1..4,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The split-read property, and the one that would notice the codec
    /// consuming a byte too many or too few: several frames back to back read
    /// back as themselves, whatever the chunking. A body is arbitrary bytes
    /// rather than JSON because framing is length-delimited — a codec that
    /// only survives valid UTF-8 has a scanner in it that should not be there.
    #[test]
    fn frames_round_trip_at_every_chunk_size(bodies in bodies(), chunk in 1_usize..40) {
        let mut stream = Vec::new();
        for body in &bodies {
            stream.extend_from_slice(&framed(body));
        }
        let mut stream = reader(stream, chunk);
        for body in &bodies {
            let read = read(&mut stream).expect("a well-formed frame reads back");
            prop_assert_eq!(&read, body);
        }
    }

    /// Arbitrary bytes terminate, and do so as an `Err` rather than a panic or
    /// a hang. This is the fuzz half: nothing here says which failure is
    /// right, only that a malformed stream is a decision the codec reaches.
    #[test]
    fn arbitrary_bytes_are_refused_rather_than_trusted(
        noise in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
        chunk in 1_usize..40,
    ) {
        let mut stream = reader(noise, chunk);
        if let Ok(body) = read(&mut stream) {
            // A stream that happens to be a valid frame is fine, and the
            // generator will find one: `Content-Length: 0\r\n\r\n` is short
            // enough to occur. What must not happen is a body appearing from a
            // stream too short to hold one.
            prop_assert!(body.len() <= 256);
        }
    }

    /// The bogus-`Content-Length` case in the half that does not parse. Every
    /// one of these is refused as a length rather than read as a body.
    #[test]
    fn a_content_length_that_is_not_a_number_is_refused(
        text in "[^0-9\r\n:][^\r\n:]{0,20}",
        chunk in 1_usize..40,
    ) {
        let stream = format!("Content-Length: {text}\r\n\r\n{{}}").into_bytes();
        let mut stream = reader(stream, chunk);
        let error = read(&mut stream).expect_err("a length that is not a number is not a length");
        prop_assert_eq!(failure(&error), Failure::BadContentLength);
    }

    /// The other half: a length that parses and is a lie. The body is far
    /// shorter than declared, and what the codec reports is how much actually
    /// arrived — the field that pre-sizing the buffer used to make a constant
    /// zero, and the first thing anybody debugging a half-written frame looks
    /// at.
    #[test]
    fn a_short_body_reports_what_arrived(
        body in proptest::collection::vec(proptest::num::u8::ANY, 0..32),
        extra in 1_usize..64,
        chunk in 1_usize..40,
    ) {
        let mut stream = format!("Content-Length: {}\r\n\r\n", body.len() + extra).into_bytes();
        stream.extend_from_slice(&body);
        let mut stream = reader(stream, chunk);
        let error = read(&mut stream).expect_err("a body shorter than its length is truncated");
        prop_assert_eq!(failure(&error), Failure::Truncated);
        let Error::Codec(CodecError::Truncated { expected, read }) = error else {
            prop_assert!(false, "the variant was just checked");
            return Ok(());
        };
        prop_assert_eq!(expected, body.len() + extra);
        prop_assert_eq!(read, body.len());
    }

    /// Oversized headers. The line never ends, so a codec that buffered until
    /// it did would buffer forever; this one refuses at a bound it names.
    #[test]
    fn a_header_line_that_never_ends_is_refused(chunk in 1_usize..40) {
        let stream = "X".repeat(MAX_HEADER_BYTES * 2).into_bytes();
        let mut stream = reader(stream, chunk);
        let error = read(&mut stream).expect_err("an unterminated header is refused");
        prop_assert_eq!(failure(&error), Failure::HeaderTooLong);
    }
}

/// A positive control. Every other assertion in this file is that something is
/// refused, and a codec that refused *everything* — a mistyped limit, a
/// reversed comparison — would pass all of them.
#[test]
fn a_well_formed_frame_is_accepted() {
    let body = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
    let mut stream = reader(framed(body), 4096);
    let read = read(&mut stream).expect("a well-formed frame is accepted");
    assert_eq!(read, body);
}

/// Headers are `Name: value` and LSP's are case-insensitive, so the accepted
/// spelling is not the only one. Here so that the refusals above are known to
/// be about length rather than about spelling.
#[test]
fn header_names_are_case_insensitive_and_extra_headers_are_ignored() {
    let body = b"{}";
    let mut stream = Vec::new();
    stream.extend_from_slice(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n");
    stream.extend_from_slice(b"content-length: 2\r\n\r\n");
    stream.extend_from_slice(body);
    let mut stream = reader(stream, 3);
    let read = read(&mut stream).expect("a lowercase Content-Length is a Content-Length");
    assert_eq!(read, body);
}

/// The bogus-`Content-Length` case that is a memory bound rather than a shape
/// check: the largest length expressible, with nothing behind it. The test is
/// that this *returns* — a codec that sized a buffer from the claim would
/// abort the process here, and an abort is not an assertion failure anybody
/// can read.
#[test]
fn content_length_past_the_limit_costs_nothing() {
    let stream = format!("Content-Length: {}\r\n\r\n", usize::MAX).into_bytes();
    let mut stream = reader(stream, 4096);
    let error = read(&mut stream).expect_err("a length past the limit is refused");
    assert_eq!(failure(&error), Failure::FrameTooLarge);
    let Error::Codec(CodecError::FrameTooLarge { length, limit }) = error else {
        panic!("the variant was just checked");
    };
    assert_eq!(length, usize::MAX);
    assert_eq!(limit, MAX_FRAME_BYTES);
}

/// A length one past the limit is refused and one at the limit is not, so the
/// bound is the one that is documented rather than one off it. The at-limit
/// half stops at `Truncated`, which is proof enough that the length was
/// accepted: nothing allocates `MAX_FRAME_BYTES` here.
#[test]
fn the_frame_limit_is_where_it_says_it_is() {
    let over = format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1).into_bytes();
    let error = read(&mut reader(over, 4096)).expect_err("one past the limit is refused");
    assert_eq!(failure(&error), Failure::FrameTooLarge);

    let at = format!("Content-Length: {MAX_FRAME_BYTES}\r\n\r\n").into_bytes();
    let error = read(&mut reader(at, 4096)).expect_err("the body is not there");
    assert_eq!(failure(&error), Failure::Truncated);
}

/// A header block that ends without saying how long the body is. Distinct from
/// a malformed header: the lines were all well-formed and the frame is still
/// unreadable, and reading zero bytes as an empty body would be the silent
/// wrong answer.
#[test]
fn a_header_block_with_no_content_length_is_refused() {
    let stream = b"Content-Type: application/vscode-jsonrpc\r\n\r\n{}".to_vec();
    let error = read(&mut reader(stream, 4096)).expect_err("no length is not zero length");
    assert_eq!(failure(&error), Failure::MissingContentLength);
}

/// A line in the header block that is not `Name: value` at all.
#[test]
fn a_header_line_without_a_colon_is_refused() {
    let stream = b"Content-Length 2\r\n\r\n{}".to_vec();
    let error = read(&mut reader(stream, 4096)).expect_err("a header needs a colon");
    assert_eq!(failure(&error), Failure::MalformedHeader);
}

/// A peer that closed the pipe. Not a codec failure — there is no frame to be
/// wrong about — and the distinction is what lets `collect` tell "the server
/// died" from "the server is speaking badly", which are different things to do
/// about.
#[test]
fn a_closed_stream_is_the_peer_and_not_the_frame() {
    let error = read(&mut reader(Vec::new(), 4096)).expect_err("a closed pipe has no frame");
    assert_eq!(failure(&error), Failure::NotACodecFailure);
}
