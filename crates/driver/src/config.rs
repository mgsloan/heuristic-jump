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

/// Resolved once at startup and then read-only.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Config {
    mode: Mode,
    deadline: DeadlineMs,
}

impl Config {
    /// The override wins in either mode, including when it is larger than the
    /// standalone default: `core.md` §5 says nothing below it depends on the
    /// specific value, so there is no range to police here.
    pub fn new(mode: Mode, deadline: DeadlineOverride) -> Self {
        let deadline = match deadline {
            DeadlineOverride::ModeDefault => mode.default_deadline(),
            DeadlineOverride::Explicit(milliseconds) => milliseconds,
        };
        Self { mode, deadline }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn deadline(&self) -> DeadlineMs {
        self.deadline
    }
}
