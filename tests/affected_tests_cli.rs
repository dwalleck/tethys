//! Binary-level fences for `tethys affected-tests` query standing
//! (tethys-09wx): exit-code contract, stdout purity, stderr reason lines.
//!
//! Every test drives the real binary via `CARGO_BIN_EXE_tethys` against a
//! throwaway on-disk workspace, so these fences pin the full CLI contract
//! (exit code + stdout bytes + stderr lines), not just facade behavior.

use std::path::Path;
use std::process::Output;

/// A minimal indexable workspace: `add` in `src/lib.rs` with an in-file
/// `#[test]`, so changing `src/lib.rs` affects exactly one test.
const LIB_RS: &str = r"
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[test]
fn test_add() {
    assert_eq!(add(2, 3), 5);
}
";

fn fixture_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"fixture_ws\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    std::fs::write(dir.path().join("src/lib.rs"), LIB_RS).expect("write lib.rs");
    dir
}

fn run_tethys(workspace: &Path, args: &[&str], rust_log: Option<&str>) -> Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"));
    cmd.arg("--workspace").arg(workspace).args(args);
    match rust_log {
        Some(level) => cmd.env("RUST_LOG", level),
        None => cmd.env_remove("RUST_LOG"),
    };
    cmd.output().expect("tethys binary should run")
}

fn index(workspace: &Path) {
    let out = run_tethys(workspace, &["index"], None);
    assert!(
        out.status.success(),
        "index failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn stdout_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// C2: with logging forced to maximum volume, `--names-only` stdout carries
/// only data lines (test names — identifier-ish, no spaces), and the log
/// lines land on stderr. Fails when the tracing subscriber writes to stdout.
#[test]
fn stdout_carries_only_data() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        Some("tethys=debug"),
    );

    let lines = stdout_lines(&out);
    assert!(!lines.is_empty(), "expected at least one affected test");
    for line in &lines {
        assert!(
            line.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':'),
            "stdout must carry only test names, got non-data line: {line:?}"
        );
    }
    assert!(
        lines.iter().any(|l| l.ends_with("test_add")),
        "expected test_add among affected tests, got: {lines:?}"
    );
    // The debug-level run must have produced logs somewhere — and that
    // somewhere must be stderr, or they'd have corrupted stdout above.
    assert!(!out.stderr.is_empty(), "debug logs should appear on stderr");
}

/// C10 (tethys-vk3z): a valid relative in-workspace path must not warn
/// "outside workspace root".
#[test]
fn relative_path_does_not_warn_outside_workspace() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        None,
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("outside workspace root"),
        "spurious outside-workspace warn for a valid relative path:\n{stderr}"
    );
}
