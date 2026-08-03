//! `design/core.md` §5's cap is configurable and mode-dependent, asserted
//! through the artifact rather than through the types `driver` exposes:
//! `crates/driver/tests/deadline.rs` already pins the two numbers, and what
//! this adds is that the argv reaches them — that `--deadline-ms` is spelled
//! the way `deps.md` §11 spells it, and that the mode is decided by the
//! presence of a server command and nothing else.
//!
//! Every case also asserts that stdout stayed empty. Stdout is the JSON-RPC
//! wire, and a parser that writes its usage or its errors there corrupts the
//! protocol before the first frame (`deps.md` §11, `clippy.toml` group 2).

use std::process::{Command, Output, Stdio};

/// Waiting for the child is the banned shape everywhere except here: the ban
/// is on a thread that owes an answer blocking, and a test has nothing else to
/// do. The shim exits immediately in either mode, since there is no run loop.
fn run(arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_heuristic-jump"))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()
}

fn logged(output: &Output) -> String {
    assert!(
        output.stdout.is_empty(),
        "wrote {} bytes to stdout, which is the JSON-RPC wire",
        output.stdout.len()
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_default_cap_is_750_proxying_and_2000_standalone() {
    let standalone = run(&["--log=info"]).expect("spawning the shim");
    assert!(
        logged(&standalone).contains("mode=\"standalone\" deadline_ms=2000"),
        "no server command means standalone, where core.md §5 raises the cap \
         because an abstention costs the answer entirely:\n{}",
        logged(&standalone)
    );

    let proxying = run(&["--log=info", "--", "rust-analyzer"]).expect("spawning the shim");
    assert!(
        logged(&proxying).contains("mode=\"proxy\" deadline_ms=750"),
        "a server command means proxy, at high-level.md's 750ms:\n{}",
        logged(&proxying)
    );
}

#[test]
fn deadline_ms_overrides_the_mode_default() {
    for arguments in [
        vec!["--log=info", "--deadline-ms=37"],
        vec!["--log=info", "--deadline-ms=37", "--", "rust-analyzer"],
    ] {
        let output = run(&arguments).expect("spawning the shim");
        assert!(
            logged(&output).contains("deadline_ms=37"),
            "--deadline-ms did not reach the cap with {arguments:?}:\n{}",
            logged(&output)
        );
    }
}

/// `heuristic-jump -- $SERVER` with `$SERVER` unset. clap reads the bare `--`
/// as the argument being absent, i.e. standalone — which would swap the oracle
/// and the cap silently. `deps.md` §11 asks for the hand-written check.
#[test]
fn a_bare_separator_is_refused_rather_than_read_as_standalone() {
    let output = run(&["--log=info", "--"]).expect("spawning the shim");

    assert_eq!(
        output.status.code(),
        Some(2),
        "a bare `--` is a usage error:\n{}",
        logged(&output)
    );
    let stderr = logged(&output);
    assert!(
        stderr.contains("no server command"),
        "the error does not say what was wrong:\n{stderr}"
    );
    assert!(
        !stderr.contains("deadline_ms"),
        "it resolved a configuration anyway:\n{stderr}"
    );
}

/// The child's arguments reach the child, `--help` included. If clap
/// intercepted one, the invocation would be answered by us instead of being
/// proxied — and the mode, and so the cap, would be whatever was left.
#[test]
fn the_childs_own_arguments_are_not_intercepted() {
    let output = run(&["--log=info", "--", "rust-analyzer", "--help", "--version"])
        .expect("spawning the shim");

    let stderr = logged(&output);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains("mode=\"proxy\""),
        "a `--help` after `--` was answered rather than passed on:\n{stderr}"
    );
}
