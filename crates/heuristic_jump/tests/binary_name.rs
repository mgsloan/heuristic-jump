//! `design/core.md` §9: the artifact is `heuristic-jump`, whatever the package
//! is called. Cargo derives a binary target's name from the package name
//! verbatim, so this holds only for as long as the `[[bin]]` rename does, and
//! nothing else in the build would notice it being dropped.

use std::path::Path;

#[test]
fn artifact_is_named_heuristic_jump() {
    let artifact = Path::new(env!("CARGO_BIN_EXE_heuristic-jump"));
    assert_eq!(
        artifact.file_stem().and_then(|stem| stem.to_str()),
        Some("heuristic-jump"),
        "built {}",
        artifact.display()
    );
    assert!(artifact.is_file(), "not built: {}", artifact.display());
}
