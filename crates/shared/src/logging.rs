//! `deps.md` §9's prefix, and nothing else about logging.
//!
//! Installing a subscriber is the process owner's — `heuristic_jump` and
//! `measure_core`, which are the two crates §9 names. What lives here is the
//! `Write` adapter they both hand to it, because there are two of them and the
//! §9 graph gives `measure_core` no edge to `driver`, where this used to be.
//! Nothing here reaches `tracing-subscriber`, which is the line §9 actually
//! draws and the one `driver/tests/seam.rs` scans for: the opinion a library
//! must not have is *where logs go*, and a `std::io::Write` that inserts bytes
//! has no opinion about that.

/// What starts every line we emit.
///
/// `shim.md` §2 forwards the child's stderr to ours verbatim, so our lines and
/// rust-analyzer's arrive interleaved in one editor log panel. `deps.md` §9's
/// requirement is that ours are distinguishable there, and the failure without
/// it is not that a line is hard to read — it is that a user reads one of our
/// warnings as the language server's and reports it against the wrong project.
///
/// **`measure collect` is the same situation and not a second one**, which is
/// why there is one constant rather than one per binary. It spawns the server
/// with `Stdio::inherit()` (`measure_core::client`), so the server's stderr is
/// this process's stderr, and a `measure` operator reading a failed collection
/// has exactly the shim user's problem. A per-binary prefix would answer a
/// question nobody is asking — there is only ever one of ours in a process —
/// at the price of a second string the first can drift from.
pub const LOG_PREFIX: &str = "[heuristic-jump] ";

/// `tracing-subscriber`'s writer, wrapped so that [`LOG_PREFIX`] starts every
/// *line* rather than every event. An event whose message spans lines would
/// otherwise carry the prefix on its first line only, and the continuation
/// lines are precisely the ones that look like the child's output.
///
/// It is in a library rather than in either binary — where §9 puts the
/// subscriber, and where both are still installed — only so that it can be
/// asserted on: a binary crate exports nothing, and an unasserted prefix
/// survives exactly until someone tidies the log setup.
///
/// Generic over the writer rather than fixed to `Stderr` so a test can read
/// back what was written. One extra instantiation, against a claim that has no
/// other way to be checked.
#[derive(Debug)]
pub struct PrefixedWriter<W> {
    inner: W,
    at_line_start: bool,
}

impl<W: std::io::Write> PrefixedWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            at_line_start: true,
        }
    }
}

impl<W: std::io::Write> std::io::Write for PrefixedWriter<W> {
    /// Returns the length of `buf` consumed, never counting the prefix bytes:
    /// a `Write` that reported more than it was given makes every caller's
    /// loop arithmetic wrong.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut consumed = 0_usize;
        for line in buf.split_inclusive(|byte| *byte == b'\n') {
            if self.at_line_start {
                self.inner.write_all(LOG_PREFIX.as_bytes())?;
            }
            self.inner.write_all(line)?;
            consumed = consumed.saturating_add(line.len());
            self.at_line_start = line.ends_with(b"\n");
        }
        Ok(consumed)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
