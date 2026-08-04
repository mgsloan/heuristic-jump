//! What the binary resolved from its argv, in the vocabulary `driver` thinks
//! in. `shim.md` §13 puts `Mode` and the deadline here and clap in
//! `heuristic_jump`, so `driver` stays a library with no opinion about how it
//! was invoked (`core.md` §9).
//!
//! The deadline defaults live here rather than next to the flag because
//! `core.md` §5 makes the driver, not the caller, the thing that enforces the
//! cap: a second copy of 750 in `heuristic_jump` would be a number two crates
//! could disagree about.

use std::ffi::OsString;
use std::time::Duration;

use shared::ServerProfile;

/// The hard cap, in the unit `--deadline-ms` and both of `shim.md` §14.6's
/// numbers are written in. A newtype because it is one of three durations in
/// the design — the cap, the debounce, the health probe — and they are not
/// interchangeable.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct DeadlineMs(u64);

impl DeadlineMs {
    /// `high-level.md`'s number. Proxying, blowing the cap degrades to an
    /// abstention, and an abstention costs the user a wait they were already
    /// having.
    pub const PROXYING: Self = Self(750);

    /// `shim.md` §14.6: standalone, an abstention costs them the answer
    /// entirely, so the cap is raised rather than removed — a wedged handler
    /// must still be bounded.
    pub const STANDALONE: Self = Self(2000);

    pub const fn new(milliseconds: u64) -> Self {
        Self(milliseconds)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// What `Deadline::new` wants. `Deadline` already turns a budget too large
    /// to add to an `Instant` into one that has already expired, so there is
    /// no saturation to do here.
    pub const fn budget(self) -> Duration {
        Duration::from_millis(self.0)
    }
}

/// How long a stale mark waits before the rescan actually goes out
/// (`core.md` §4). The second of the three durations [`DeadlineMs`] names, and
/// not interchangeable with it: this one bounds *wasted work*, where the
/// deadline bounds a user's wait.
///
/// The design gives no number — §4 says only that a burst of misses triggers
/// at most one rescan — so [`DebounceMs::RESCAN`] is a default rather than a
/// target, and nothing measured depends on it. It is a constructor argument
/// and not a constant so that raising it is configuration.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct DebounceMs(u64);

impl DebounceMs {
    /// Long enough that a branch switch's frames coalesce into one walk, short
    /// enough that §4's "the user asks again" lands after the rescan rather
    /// than during it.
    pub const RESCAN: Self = Self(500);

    pub const fn new(milliseconds: u64) -> Self {
        Self(milliseconds)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn window(self) -> Duration {
        Duration::from_millis(self.0)
    }
}

/// `--deadline-ms`, before a mode has been chosen. Absent is not zero and not
/// a sentinel: it means whichever of the two defaults the mode carries, which
/// is why this is an enum rather than the `Option<u64>` clap produces.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DeadlineOverride {
    ModeDefault,
    Explicit(DeadlineMs),
}

/// Whether the shim answers anything itself. `--proxy-only` is `shim.md` §11's
/// permanent degraded mode, and it is a named state rather than a `bool`
/// because it reaches the dispatch decision, where `if proxy_only` would read
/// as a negation of something.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Heuristics {
    Enabled,
    Disabled,
}

/// The child's command line, split so that the program is not an element of a
/// list that could be empty. `deps.md` §11's `--` makes everything after it
/// the child's, verbatim.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ServerCommand {
    program: OsString,
    arguments: Vec<OsString>,
}

impl ServerCommand {
    pub fn program(&self) -> &OsString {
        &self.program
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

/// `deps.md` §11: the mode *is* whether a server command was given. The argv
/// lives inside the mode rather than beside it, so there is no second source
/// of truth to contradict it and no conflict rule to write.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Mode {
    Proxy {
        server: ServerCommand,
        heuristics: Heuristics,
    },
    Standalone,
}

impl Mode {
    /// The one place the question is asked. `heuristics` is dropped in
    /// standalone: `--proxy-only` without a server is refused by clap's
    /// `requires`, so a disabled-heuristics standalone is unreachable rather
    /// than silently ignored.
    pub fn from_server_argv(argv: Vec<OsString>, heuristics: Heuristics) -> Self {
        let mut argv = argv.into_iter();
        match argv.next() {
            Some(program) => Self::Proxy {
                server: ServerCommand {
                    program,
                    arguments: argv.collect(),
                },
                heuristics,
            },
            None => Self::Standalone,
        }
    }

    pub fn default_deadline(&self) -> DeadlineMs {
        match self {
            Self::Proxy {
                server: _,
                heuristics: _,
            } => DeadlineMs::PROXYING,
            Self::Standalone => DeadlineMs::STANDALONE,
        }
    }

    /// Which oracle we are standing in for (`core.md` §7). Derived from the
    /// mode rather than stored beside it, for the reason the argv is: the mode
    /// *is* whether a server was given, so a profile built anywhere else would
    /// be a second answer to a question this enum already settles.
    pub fn server_profile(&self) -> ServerProfile {
        match self {
            Self::Proxy {
                server,
                heuristics: _,
            } => ServerProfile::proxying_command(server.program(), server.arguments()),
            Self::Standalone => ServerProfile::standalone(),
        }
    }

    /// For the log line and the trace record, which name the mode rather than
    /// printing its argv.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Proxy {
                server: _,
                heuristics: _,
            } => "proxy",
            Self::Standalone => "standalone",
        }
    }
}

/// `deps.md` §9's default: "the default filter is `warn` so we are quiet unless
/// asked". Here rather than beside `--log` for the reason `DeadlineMs`'s
/// defaults are here — how loud the shipped binary is by default is a property
/// of the binary, and a second copy of the string in `heuristic_jump` is one
/// two crates could disagree about.
pub const DEFAULT_LOG_FILTER: &str = "warn";

/// What starts every line we emit.
///
/// `shim.md` §2 forwards the child's stderr to ours verbatim, so our lines and
/// rust-analyzer's arrive interleaved in one editor log panel. `deps.md` §9's
/// requirement is that ours are distinguishable there, and the failure without
/// it is not that a line is hard to read — it is that a user reads one of our
/// warnings as the language server's and reports it against the wrong project.
pub const LOG_PREFIX: &str = "[heuristic-jump] ";

/// `tracing-subscriber`'s writer, wrapped so that [`LOG_PREFIX`] starts every
/// *line* rather than every event. An event whose message spans lines would
/// otherwise carry the prefix on its first line only, and the continuation
/// lines are precisely the ones that look like the child's output.
///
/// It lives in `driver` rather than in `heuristic_jump` — where §9 puts the
/// subscriber, and where it is still installed — only so that it can be
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

/// Resolved once at startup and then read-only.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Config {
    mode: Mode,
    deadline: DeadlineMs,
    server: ServerProfile,
}

impl Config {
    /// The override wins in either mode, including when it is larger than the
    /// standalone default: `core.md` §5 says nothing below it depends on the
    /// specific value, so there is no range to police here.
    ///
    /// The profile is resolved here and not per query, because `core.md` §7
    /// says "at startup" and this is it: the child's argv cannot change while
    /// the process runs, so a resolution anywhere further in would be the same
    /// answer recomputed under a deadline.
    pub fn new(mode: Mode, deadline: DeadlineOverride) -> Self {
        let deadline = match deadline {
            DeadlineOverride::ModeDefault => mode.default_deadline(),
            DeadlineOverride::Explicit(milliseconds) => milliseconds,
        };
        let server = mode.server_profile();
        Self {
            mode,
            deadline,
            server,
        }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn deadline(&self) -> DeadlineMs {
        self.deadline
    }

    /// What every `Query` this process dispatches carries (`core.md` §1).
    pub fn server(&self) -> &ServerProfile {
        &self.server
    }
}
