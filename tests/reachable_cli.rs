//! Binary-level fences for the canonical `tethys reachable` adapter.

use std::path::Path;
use std::process::Output;

use tethys::{ReachabilityDirection, Tethys};

const LIB_RS: &str = r"
pub fn leaf() {}
pub fn left() { leaf(); }
pub fn right() {}
pub fn source() { left(); right(); }
";

fn fixture_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"reachable_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    std::fs::write(dir.path().join("src/lib.rs"), LIB_RS).expect("write lib.rs");
    dir
}

fn run_tethys(workspace: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .arg("--workspace")
        .arg(workspace)
        .args(args)
        .env_remove("RUST_LOG")
        .output()
        .expect("tethys binary should run")
}

fn indexed_workspace() -> tempfile::TempDir {
    let workspace = fixture_workspace();
    let output = run_tethys(workspace.path(), &["index"]);
    assert!(
        output.status.success(),
        "index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    workspace
}

fn assert_cli_matches_canonical(
    workspace: &Path,
    symbol: &str,
    spelling: &str,
    direction: ReachabilityDirection,
    depth: usize,
) {
    let output = run_tethys(
        workspace,
        &[
            "reachable",
            symbol,
            "--direction",
            spelling,
            "--max-depth",
            &depth.to_string(),
        ],
    );
    assert!(
        output.status.success(),
        "reachable failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let canonical = Tethys::new(workspace)
        .expect("open indexed workspace")
        .get_reachable(symbol, direction, Some(depth))
        .expect("canonical reachability");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let title = match direction {
        ReachabilityDirection::Forward => "Forward reachability",
        ReachabilityDirection::Backward => "Backward reachability",
    };
    assert!(stdout.contains(title), "missing {title:?} in {stdout:?}");
    assert!(
        stdout.contains(&format!("Summary: {} symbols", canonical.reachable_count())),
        "CLI count differs from canonical result: {stdout:?}"
    );
    assert!(
        stdout.contains(&format!("max depth: {depth}")),
        "CLI effective depth differs: {stdout:?}"
    );
}

#[test]
fn accepted_direction_spellings_match_canonical_operation() {
    let workspace = indexed_workspace();

    for spelling in ["forward", "f", "FoRwArD"] {
        assert_cli_matches_canonical(
            workspace.path(),
            "source",
            spelling,
            ReachabilityDirection::Forward,
            3,
        );
    }
    for spelling in ["backward", "b", "BaCkWaRd"] {
        assert_cli_matches_canonical(
            workspace.path(),
            "leaf",
            spelling,
            ReachabilityDirection::Backward,
            3,
        );
    }
}

#[test]
fn omitted_reachable_options_use_forward_depth_ten() {
    let workspace = indexed_workspace();
    let output = run_tethys(workspace.path(), &["reachable", "source"]);
    assert!(output.status.success());

    let canonical = Tethys::new(workspace.path())
        .expect("open indexed workspace")
        .get_reachable("source", ReachabilityDirection::Forward, Some(10))
        .expect("canonical reachability");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Forward reachability"));
    assert!(stdout.contains(&format!("Summary: {} symbols", canonical.reachable_count())));
    assert!(stdout.contains("max depth: 10"));
}

#[test]
fn invalid_direction_is_a_stderr_configuration_error() {
    let workspace = indexed_workspace();
    let output = run_tethys(
        workspace.path(),
        &["reachable", "source", "--direction", "sideways"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "error: configuration error: Invalid direction 'sideways'. Use 'forward' (or 'f') or 'backward' (or 'b')."
    );
}
