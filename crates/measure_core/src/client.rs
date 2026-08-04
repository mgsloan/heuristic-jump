//! The LSP client. `core.md` §7's table, second column: the proper LSP is
//! *waited for* rather than raced, throughput matters rather than latency,
//! there are no deadlines, and there is no editor on the other side.
//!
//! It is synchronous and single-request. `data-collection.md` §4 notes that
//! several requests in flight per server is worth tuning per server and that
//! multiple (repository, server) pairs in parallel is the safer axis; both are
//! optimisations over a working collector, and `CLAUDE.md`'s posture is to
//! build the slow simple version first.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;
use shared::proto::{ChildFrame, ClientNotification, ClientReply, ClientRequest, PositionEncoding};
use shared::{ChildError, Clock, CodecError, Error};

#[derive(Debug)]
pub(crate) struct Client {
    command: PathBuf,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Client {
    pub(crate) fn start(command: &[String]) -> Result<Self, Error> {
        let Some((program, arguments)) = command.split_first() else {
            return Err(ChildError::Spawn {
                command: PathBuf::new(),
                source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
            }
            .into());
        };
        let program = PathBuf::from(program);

        let mut child = Command::new(&program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Inherited rather than piped: a server's stderr is its own
            // diagnostics, and a piped one nobody drains fills its buffer and
            // deadlocks the child partway through a repository.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| ChildError::Spawn {
                command: program.clone(),
                source,
            })?;

        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(ChildError::StdioUnavailable { command: program }.into());
        };

        Ok(Self {
            command: program,
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    /// Send-to-receive, in microseconds — from writing the request frame to
    /// reading the matching response. Not the server's internal timing, which
    /// is not observable and is not what a user waits for.
    pub(crate) fn request<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: P,
        clock: &dyn Clock,
    ) -> Result<(Option<R>, Duration), Error> {
        let id = self.next_id;
        self.next_id += 1;

        let started = clock.now();
        self.send(&ClientRequest::new(id, method, params), "a request")?;

        loop {
            let frame = self.read_message::<R>()?;
            if frame.answers(id) {
                let elapsed = clock.now().saturating_duration_since(started);
                if let Some(failure) = frame.error {
                    return Err(ChildError::Failed {
                        method: method.into(),
                        code: failure.code,
                        message: failure.message,
                    }
                    .into());
                }
                return Ok((frame.result, elapsed));
            }
            // A server request we do not implement. Answering `null` is what
            // leaves it free to continue; ignoring it hangs whichever of its
            // own tasks was waiting.
            if let Some(id) = frame.awaiting_reply() {
                self.send(&ClientReply::nothing(id), "a reply")?;
            }
        }
    }

    pub(crate) fn notify<P: Serialize>(
        &mut self,
        method: &'static str,
        params: P,
    ) -> Result<(), Error> {
        self.send(&ClientNotification::new(method, params), "a notification")
    }

    /// LSP's shutdown handshake, then the process. Failures here are logged
    /// rather than propagated: the run's results are already collected, and a
    /// server that will not exit cleanly is not a reason to discard them.
    pub(crate) fn stop(mut self, clock: &dyn Clock) {
        if let Err(error) = self.request::<_, Option<()>>("shutdown", (), clock) {
            tracing::warn!(%error, "shutdown was refused");
        }
        if let Err(error) = self.notify("exit", ()) {
            tracing::warn!(%error, "exit could not be sent");
        }
        if let Err(error) = self.child.kill() {
            tracing::debug!(%error, "the server had already exited");
        }
    }

    fn send<T: Serialize>(&mut self, message: &T, what: &'static str) -> Result<(), Error> {
        let body = serde_json::to_string(message)
            .map_err(|source| CodecError::NotSerializable { what, source })?;
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.stdin
            .write_all(frame.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|source| ChildError::Io {
                command: self.command.clone(),
                source,
            })?;
        Ok(())
    }

    fn read_message<R: DeserializeOwned>(&mut self) -> Result<ChildFrame<R>, Error> {
        let body = read_frame(&mut self.stdout, &self.command)?;
        serde_json::from_slice::<ChildFrame<R>>(&body).map_err(|source| {
            CodecError::BodyNotJson {
                length: body.len(),
                source,
            }
            .into()
        })
    }
}

/// A header line is refused past this, before the line is complete.
///
/// LSP names two headers and neither is long — `Content-Length` and a
/// `Content-Type` whose only defined value is thirty-odd characters — so this
/// is orders of magnitude above anything a conforming peer sends, which is
/// what a limit whose purpose is to bound memory should be.
pub const MAX_HEADER_BYTES: usize = 8 * 1024;

/// A `Content-Length` is refused past this, before anything is allocated.
///
/// The frame that justifies a large number is `didOpen` carrying a whole file,
/// and the corpus holds generated files in the low megabytes; the limit is set
/// well above them because exceeding it is a hard failure rather than a
/// degradation.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// One `Content-Length`-framed message body, read from `reader`. The header
/// block is read line by line and the body by exact length, because a JSON-RPC
/// body may contain anything and there is no delimiter to scan for.
///
/// It is a free function over `&mut dyn BufRead`, rather than the method on
/// [`Client`] it used to be, for the reason `replay_table` is public: `core.md`
/// §10 asks for the frame codec to be fuzzed, and the only way to reach a
/// method on a `BufReader<ChildStdout>` is to spawn a language server — which
/// is exactly what a property test over split reads and bogus lengths is
/// trying not to need. `deps.md` §12 declines `cargo-fuzz` on the grounds that
/// "`proptest` covers the split-read / bogus-`Content-Length` cases well enough
/// to start", so the fuzzing is `tests/codec.rs` and this is its entry point.
///
/// `command` names the peer for the two failures that are the peer's rather
/// than the frame's — it went away, or the pipe broke — which are
/// [`ChildError`] and carry which child it was.
pub fn read_frame(reader: &mut dyn BufRead, command: &Path) -> Result<Vec<u8>, Error> {
    let mut length = None;
    loop {
        let mut line = String::new();
        // `take` is the memory bound: `read_line` on an unbounded reader
        // buffers until a line ending arrives, so a peer that sends megabytes
        // without one is an allocation failure rather than a codec error. The
        // limit is read back off `line` below, because `read_line` reports the
        // bytes it consumed and not why it stopped.
        let read = (&mut *reader)
            .take(as_u64(MAX_HEADER_BYTES))
            .read_line(&mut line)
            .map_err(|source| ChildError::Io {
                command: command.to_path_buf(),
                source,
            })?;
        if read == 0 {
            return Err(ChildError::Exited {
                command: command.to_path_buf(),
            }
            .into());
        }
        if read == MAX_HEADER_BYTES && !line.ends_with('\n') {
            return Err(CodecError::HeaderTooLong {
                limit: MAX_HEADER_BYTES,
            }
            .into());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(CodecError::MalformedHeader { text: line.into() }.into());
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            length =
                Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| CodecError::BadContentLength {
                            text: value.trim().into(),
                        })?,
                );
        }
    }

    let Some(length) = length else {
        return Err(CodecError::MissingContentLength.into());
    };
    if length > MAX_FRAME_BYTES {
        return Err(CodecError::FrameTooLarge {
            length,
            limit: MAX_FRAME_BYTES,
        }
        .into());
    }

    // Grown as the bytes arrive rather than `vec![0; length]`, so a length
    // under the limit but past what the peer actually sends costs what was
    // sent. It is also what lets `Truncated` say how much arrived: the
    // pre-sized read reported zero whatever it had read, which is the field
    // somebody debugging a half-written frame would look at first.
    let mut body = Vec::new();
    (&mut *reader)
        .take(as_u64(length))
        .read_to_end(&mut body)
        .map_err(|source| ChildError::Io {
            command: command.to_path_buf(),
            source,
        })?;
    if body.len() != length {
        return Err(CodecError::Truncated {
            expected: length,
            read: body.len(),
        }
        .into());
    }
    Ok(body)
}

/// `usize` to `u64` without an `as`, which the workspace denies for the reason
/// `core.md` §3 gives. Saturating is right and unreachable: it would take a
/// 128-bit `usize` to lose anything.
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// What we ask for and what the server chose. LSP says a server that names no
/// encoding means UTF-16, and that default is applied here — where the value
/// is settled — rather than in the projection, which keeps "what the child
/// chose" and "what LSP says when it chose nothing" distinguishable.
pub(crate) const OFFERED_ENCODINGS: [PositionEncoding; 2] =
    [PositionEncoding::Utf16, PositionEncoding::Utf8];

pub(crate) fn settled_encoding(chosen: Option<PositionEncoding>) -> PositionEncoding {
    chosen.unwrap_or(PositionEncoding::Utf16)
}

/// The `result` field of a response, kept raw. `truth.jsonl` stores the bytes
/// the server sent rather than a projection written back out.
pub(crate) type RawResult = Box<RawValue>;
