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

/// A leaf file no test depends on: changing it affects nothing, so a fresh
/// index answers "no affected tests" with confirmed standing.
const LEAF_RS: &str = r"
pub fn leaf() -> i32 { 7 }
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
    std::fs::write(dir.path().join("src/leaf.rs"), LEAF_RS).expect("write leaf.rs");
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

/// The machine-readable contract lines on stderr (`^indeterminate: `),
/// excluding any tracing/log noise.
fn reason_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|l| l.starts_with("indeterminate: "))
        .map(str::to_owned)
        .collect()
}

fn exit_code(out: &Output) -> i32 {
    out.status.code().expect("no exit code (killed by signal?)")
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

/// C3: all-current changed file with no dependent tests → exit 0, empty
/// stdout, zero reason lines. Also the inverse assert for C16: a pristine
/// workspace must not emit stale-index.
#[test]
fn confirmed_empty_exits_zero() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/leaf.rs"],
        None,
    );
    assert_eq!(
        exit_code(&out),
        0,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "confirmed-empty must print nothing");
    assert!(
        reason_lines(&out).is_empty(),
        "pristine workspace must emit no reasons"
    );
}

/// C4: an unindexed changed file → exit 2 + `indeterminate: unindexed:`
/// line; a never-existed path in a pristine workspace emits exactly that
/// one reason (no stale-index piggyback).
#[test]
fn unindexed_is_indeterminate() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/phantom.rs"],
        None,
    );
    assert_eq!(exit_code(&out), 2);
    assert!(out.stdout.is_empty());
    assert_eq!(
        reason_lines(&out),
        vec!["indeterminate: unindexed: src/phantom.rs".to_owned()]
    );
}

/// C5: an mtime-only bump (rewrite with identical content) → exit 2 +
/// `indeterminate: stale:` line. Fails if staleness compares size only.
#[test]
fn stale_is_indeterminate() {
    let ws = fixture_workspace();
    index(ws.path());
    std::fs::write(ws.path().join("src/lib.rs"), LIB_RS).expect("rewrite same content");

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        None,
    );
    assert_eq!(
        exit_code(&out),
        2,
        "mtime-only divergence must be indeterminate"
    );
    let reasons = reason_lines(&out);
    assert!(
        reasons.contains(&"indeterminate: stale: src/lib.rs".to_owned()),
        "expected per-file stale reason, got: {reasons:?}"
    );
}

/// C6: a stale changed file that HAS dependent tests still reports them on
/// stdout while exiting 2 — indeterminacy must not suppress found results.
#[test]
fn indeterminate_still_reports_found_tests() {
    let ws = fixture_workspace();
    index(ws.path());
    std::fs::write(ws.path().join("src/lib.rs"), LIB_RS).expect("rewrite same content");

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        None,
    );
    assert_eq!(exit_code(&out), 2);
    let lines = stdout_lines(&out);
    assert!(
        lines.iter().any(|l| l.ends_with("test_add")),
        "stdout must still carry found tests, got: {lines:?}"
    );
}

/// C8: every spelling of the same changed file — plain, ./-prefixed,
/// intra-path .., absolute — produces identical stdout, exit code, and
/// (empty) reason set on a pristine workspace.
#[test]
fn path_forms_equivalent() {
    let ws = fixture_workspace();
    index(ws.path());
    let abs = ws.path().join("src/lib.rs");
    let abs = abs.to_str().expect("utf8 tempdir path");

    let baseline = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        None,
    );
    assert_eq!(exit_code(&baseline), 0);
    let expected = stdout_lines(&baseline);
    assert!(expected.iter().any(|l| l.ends_with("test_add")));

    for spelling in ["./src/lib.rs", "src/../src/lib.rs", "src/./lib.rs", abs] {
        let out = run_tethys(
            ws.path(),
            &["affected-tests", "--names-only", spelling],
            None,
        );
        assert_eq!(
            exit_code(&out),
            0,
            "spelling {spelling:?} must be confirmed"
        );
        assert_eq!(
            stdout_lines(&out),
            expected,
            "spelling {spelling:?} must match the plain form"
        );
        assert!(
            reason_lines(&out).is_empty(),
            "spelling {spelling:?} emitted reasons"
        );
    }
}

/// C9: a changed path outside the workspace is unindexable → deterministic
/// exit 2 with an `unindexed` reason, never a hard error (exit 1).
#[test]
fn outside_workspace_is_indeterminate() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "../outside.rs"],
        None,
    );
    assert_eq!(
        exit_code(&out),
        2,
        "outside path must fail open, not hard-error"
    );
    let reasons = reason_lines(&out);
    assert_eq!(reasons.len(), 1, "reasons: {reasons:?}");
    assert!(
        reasons[0].starts_with("indeterminate: unindexed: "),
        "expected unindexed kind, got: {reasons:?}"
    );
}

/// C11: duplicate inputs — including two spellings of one file — collapse
/// to a single reason line.
#[test]
fn reasons_deduped() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(
        ws.path(),
        &[
            "affected-tests",
            "--names-only",
            "src/phantom.rs",
            "src/phantom.rs",
            "./src/phantom.rs",
        ],
        None,
    );
    assert_eq!(exit_code(&out), 2);
    assert_eq!(
        reason_lines(&out),
        vec!["indeterminate: unindexed: src/phantom.rs".to_owned()],
        "three spellings of one file must yield one reason"
    );
}

/// C12: empty input → exit 0 (vacuously confirmed: nothing changed means no
/// affected tests), empty stdout, warning preserved on stderr.
#[test]
fn empty_input_confirmed() {
    let ws = fixture_workspace();
    index(ws.path());

    let out = run_tethys(ws.path(), &["affected-tests", "--names-only"], None);
    assert_eq!(exit_code(&out), 0);
    assert!(out.stdout.is_empty());
    assert!(reason_lines(&out).is_empty());
}

/// C15: the same indeterminate invocation twice → byte-identical stdout,
/// identical reason lines, same exit code. Fails if unordered-map iteration
/// leaks into output order.
#[test]
fn deterministic_output() {
    let ws = fixture_workspace();
    index(ws.path());
    std::fs::write(ws.path().join("src/lib.rs"), LIB_RS).expect("rewrite same content");

    let args = [
        "affected-tests",
        "--names-only",
        "src/lib.rs",
        "src/phantom.rs",
        "src/zeta.rs",
    ];
    let first = run_tethys(ws.path(), &args, None);
    let second = run_tethys(ws.path(), &args, None);
    assert_eq!(exit_code(&first), 2);
    assert_eq!(exit_code(&first), exit_code(&second));
    assert_eq!(first.stdout, second.stdout, "stdout must be byte-identical");
    assert_eq!(reason_lines(&first), reason_lines(&second));
}

/// C16: the changed file itself is current, but an unrelated file appeared
/// on disk after indexing → exit 2 with the fixed stale-index line, while
/// stdout still carries the traversal result.
#[test]
fn stale_index_is_indeterminate() {
    let ws = fixture_workspace();
    index(ws.path());
    std::fs::write(ws.path().join("src/unrelated.rs"), "pub fn u() {}\n")
        .expect("write unrelated file");

    let out = run_tethys(
        ws.path(),
        &["affected-tests", "--names-only", "src/lib.rs"],
        None,
    );
    assert_eq!(exit_code(&out), 2);
    assert_eq!(
        reason_lines(&out),
        vec!["indeterminate: stale-index: workspace changed since last index".to_owned()]
    );
    let lines = stdout_lines(&out);
    assert!(
        lines.iter().any(|l| l.ends_with("test_add")),
        "traversal result must survive stale-index standing: {lines:?}"
    );
}
