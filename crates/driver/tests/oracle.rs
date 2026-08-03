//! `design/core.md` §7, "the oracle is the server being proxied": the shim
//! stands in for **one specific server**, and that identity reaches handlers as
//! a `ServerProfile` carrying a `ServerId` "resolved from the child's command
//! name at startup".
//!
//! The identity is only worth having if it cannot be silently dropped, and
//! there are two ways to drop it. One is a call site that knows the server and
//! passes `None` anyway — structural now, since `ServerProfile`'s field is
//! private and the constructors are the two situations §7 describes. The other
//! is `ServerId::KNOWN` drifting from `servers.toml`, which is the real server
//! matrix and is in no loop's write list; a server added there and not here
//! would resolve to `None` for every invocation, and nothing else in the
//! workspace would notice. `every_server_in_the_matrix_resolves_to_its_own_id`
//! is that check, in both directions.
//!
//! This file reads `servers.toml` with its own six-line scan rather than
//! through `measure_core`'s manifest parser, on purpose: a test that shares a
//! parser with the code it checks shares its bugs, and the two shapes this
//! needs — the table keys and each entry's `command` — are a subset of TOML
//! small enough to read directly.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and `matrix` below is a free function. Failing loudly is the point: an unreadable or unparseable `servers.toml` must not degrade into an empty matrix that every assertion below then passes vacuously."
)]

use std::ffi::OsString;
use std::fs;

use driver::{Config, DeadlineOverride, Heuristics, Mode};
use shared::{ServerId, ServerProfile};

/// The canonical server matrix, beside the code that is scored against it
/// (`external-dependencies.md` §1).
const SERVERS_MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../servers.toml");

/// Every server in the matrix must be nameable *and* recognisable from the
/// command line the matrix itself says launches it.
///
/// Both directions, because they fail differently. A server in `servers.toml`
/// that `ServerId` does not know is an oracle the shim can never build a
/// profile for; a `ServerId` that `servers.toml` does not list is an id no
/// corpus can ever be collected against.
#[test]
fn every_server_in_the_matrix_resolves_to_its_own_id() {
    let matrix = matrix();

    // Before comparing anything: a scan that found nothing would satisfy every
    // assertion in the loop below without executing it once.
    assert!(
        matrix.len() >= 8,
        "servers.toml lists eight servers; the scan found {}",
        matrix.len()
    );

    for (name, command) in &matrix {
        let expected = ServerId::from_name(name);
        assert_eq!(
            expected.map(ServerId::as_str),
            Some(name.as_str()),
            "`{name}` is in servers.toml and not in ServerId::KNOWN"
        );

        let (program, arguments) = command
            .split_first()
            .expect("every servers.toml entry has a non-empty `command`");
        assert_eq!(
            ServerId::from_command(program, arguments),
            expected,
            "servers.toml launches `{name}` as {command:?}, which resolves elsewhere"
        );
    }

    let mut listed: Vec<&str> = matrix.iter().map(|(name, _)| name.as_str()).collect();
    let mut known: Vec<&str> = ServerId::KNOWN.iter().map(|id| id.as_str()).collect();
    listed.sort_unstable();
    known.sort_unstable();
    assert_eq!(
        known, listed,
        "ServerId::KNOWN and servers.toml's table keys have diverged"
    );
}

/// §7's "at startup": the profile is a property of the argv the process was
/// launched with, so `Config` — which is "resolved once at startup and then
/// read-only" — is where it settles.
#[test]
fn the_startup_resolution_follows_the_child_command() {
    // The plain case: the program *is* the server.
    assert_eq!(
        resolved(&["rust-analyzer"]).id().map(ServerId::as_str),
        Some("rust-analyzer")
    );

    // Half the matrix is launched through an interpreter, so the program name
    // is `node` and the identity is further along the command line. A resolver
    // that read only the program would miss exactly these.
    assert_eq!(
        resolved(&[
            "node",
            "/opt/hj/servers/node/pyright/node_modules/pyright/langserver.index.js",
            "--stdio",
        ])
        .id()
        .map(ServerId::as_str),
        Some("pyright")
    );

    // Standalone has no oracle at all, which is a different thing from an
    // oracle we failed to identify — and both are `None`, which is why the
    // constructor rather than the value is what distinguishes them.
    let standalone = Config::new(
        Mode::from_server_argv(Vec::new(), Heuristics::Enabled),
        DeadlineOverride::ModeDefault,
    );
    assert_eq!(standalone.server(), &ServerProfile::standalone());
    assert_eq!(standalone.server().id(), None);
}

/// A server we have no profile for is `None` rather than a synthesised id, and
/// a command line we cannot read unambiguously is treated the same way.
///
/// The ambiguous case is not hypothetical: `basedpyright` is a pyright fork and
/// installs npm bin names `pyright` and `pyright-langserver` alongside its own
/// (`external-dependencies.md`), so a tree holding both is the ordinary
/// arrangement rather than a contrived one. Answering with whichever component
/// came first would attach one server's profile to the other.
#[test]
fn an_unidentifiable_server_has_no_id() {
    assert_eq!(resolved(&["some-editors-own-lsp", "--stdio"]).id(), None);

    assert_eq!(
        resolved(&[
            "node",
            "/opt/hj/servers/node/basedpyright/node_modules/pyright/langserver.index.js",
        ])
        .id(),
        None,
        "two servers named on one command line is not an identification"
    );
}

fn resolved(argv: &[&str]) -> ServerProfile {
    let config = Config::new(
        Mode::from_server_argv(
            argv.iter().map(OsString::from).collect(),
            Heuristics::Enabled,
        ),
        DeadlineOverride::ModeDefault,
    );
    config.server().clone()
}

/// The `[server.<name>]` keys of `servers.toml`, each with its `command`.
///
/// `command` arrays are written across several lines for the node-launched
/// entries, so the value is accumulated until its closing bracket rather than
/// read one line at a time.
fn matrix() -> Vec<(String, Vec<OsString>)> {
    let text =
        fs::read_to_string(SERVERS_MANIFEST).expect("servers.toml is at the repository root");
    let mut lines = text.lines();
    let mut found: Vec<(String, Vec<OsString>)> = Vec::new();

    while let Some(line) = lines.next() {
        let line = line.trim();

        if let Some(name) = line
            .strip_prefix("[server.")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            found.push((name.to_owned(), Vec::new()));
            continue;
        }

        // `version_command` does not start with `command`, so it is skipped
        // here without a second rule.
        let Some(value) = line
            .strip_prefix("command")
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix('='))
        else {
            continue;
        };

        let mut block = value.to_owned();
        while !block.contains(']') {
            let Some(more) = lines.next() else { break };
            block.push_str(more);
        }

        let words = block.split('"').skip(1).step_by(2).map(OsString::from);
        let (_, command) = found
            .last_mut()
            .expect("a `command` belongs to the [server.<name>] table above it");
        command.extend(words);
    }

    found
}
