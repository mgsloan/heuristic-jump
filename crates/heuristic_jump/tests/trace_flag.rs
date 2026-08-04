//! `design/deps.md` §11's last flag: `--trace=<path>`, "JSONL metric records,
//! `core.md` §7".
//!
//! What the records *contain* is `crates/driver/tests/actor.rs`, which drives
//! the actor directly. What this adds is the half that only the artifact can
//! show: that the flag is spelled the way §11 spells it, that the path reaches
//! the sink rather than being parsed and dropped, and that a path nothing can
//! be written to is refused at startup rather than at the first query — a run
//! whose observability was asked for and is silently absent is the failure the
//! records exist to prevent.
//!
//! Stdout is asserted empty for the reason `deadline_flag.rs` gives: it is the
//! JSON-RPC wire, and a parser that writes there corrupts the protocol before
//! the first frame.

#![expect(
    clippy::expect_used,
    reason = "`clippy.toml`'s allow-expect-in-tests reaches only `#[test]` bodies, and `scratch` below is a free function. Failing loudly is right there: a leftover file from a previous run is what every assertion here would otherwise be made against."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Waiting for the child is the banned shape everywhere except here: the ban is
/// on a thread that owes an answer blocking, and a test has nothing else to do.
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

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if path.exists() {
        fs::remove_file(&path).expect("clearing a previous run");
    }
    path
}

/// The sink is opened when the flag is resolved, not when the first record is
/// written — so an empty file is the evidence that the path arrived. A run that
/// parsed `--trace` into a field nobody read would leave nothing here.
#[test]
fn the_trace_path_reaches_the_sink() {
    let path = scratch("flag-trace.jsonl");
    let output = run(&[
        "--log=info",
        &format!("--trace={}", path.display()),
        "--",
        "rust-analyzer",
    ])
    .expect("spawning the shim");

    let stderr = logged(&output);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(
        path.exists(),
        "--trace named a path and nothing opened it:\n{stderr}"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the trace"),
        "",
        "a run that answered no queries wrote records anyway"
    );
}

/// Without the flag there is no file, which is the shipped default: the records
/// are what a corpus run is made of, and an editor session should not pay for
/// them unasked.
#[test]
fn no_flag_writes_no_records() {
    let path = scratch("flag-absent.jsonl");
    let output = run(&["--log=info"]).expect("spawning the shim");

    assert_eq!(output.status.code(), Some(0), "{}", logged(&output));
    assert!(!path.exists(), "a trace was written without --trace");
}

/// Refused at startup. The alternative — discovering it per query — is a run
/// that looks traced and is not, and by then the queries it was measuring have
/// gone.
#[test]
fn an_unwritable_trace_is_refused_before_anything_runs() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("flag-no-such-directory")
        .join("trace.jsonl");
    let output =
        run(&["--log=info", &format!("--trace={}", path.display())]).expect("spawning the shim");

    let stderr = logged(&output);
    assert_ne!(
        output.status.code(),
        Some(0),
        "a trace that cannot be opened was accepted:\n{stderr}"
    );
    assert!(
        stderr.contains("TraceUnwritable"),
        "the failure does not name what went wrong:\n{stderr}"
    );
}
