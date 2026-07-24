//! Integration tests for LSP-refined direct caller queries.
//!
//! These tests verify graceful fallback, indexed/LSP merging, and
//! deduplication through [`tethys::CallerMode::LspRefined`].
//!
//! Tests marked with `#[ignore]` require rust-analyzer to be installed.
//! Run with: `cargo test --test lsp_callers -- --ignored`

use std::fs;
use std::process::Command;
use tempfile::TempDir;
use tethys::Tethys;

/// Check if rust-analyzer is available in PATH.
fn rust_analyzer_available() -> bool {
    let check_cmd = if cfg!(windows) { "where" } else { "which" };
    Command::new(check_cmd)
        .arg("rust-analyzer")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Create a workspace with intra-file symbol references for testing.
///
/// Symbol graph:
/// ```text
///   process() -> validate() -> Helper::new()
///                            -> Helper::check()
/// ```
fn workspace_with_intra_file_calls() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    fs::write(
        dir.path().join("src/lib.rs"),
        r"
pub struct Helper;

impl Helper {
    pub fn new() -> Helper {
        Helper
    }

    pub fn check(&self) -> bool {
        true
    }
}

pub fn validate() -> bool {
    let h = Helper::new();
    h.check()
}

pub fn process() -> bool {
    validate()
}
",
    )
    .expect("failed to write lib.rs");

    // Create Cargo.toml so rust-analyzer can analyze the project
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[package]
name = "test_workspace"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("failed to write Cargo.toml");

    let tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    (dir, tethys)
}

// ============================================================================
// Fallback Behavior Tests (run without LSP)
// ============================================================================

#[test]
fn lsp_refined_mode_includes_indexed_callers() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // Even if LSP fails, should return DB callers gracefully
    // validate is called by process
    let callers = tethys
        .get_callers("validate", tethys::CallerMode::LspRefined)
        .expect("LSP-refined caller query should succeed");

    // Should have at least the DB callers
    assert!(
        !callers.is_empty(),
        "should have callers from DB even if LSP fails"
    );
}

#[test]
fn lsp_refined_mode_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys.get_callers("NonExistent", tethys::CallerMode::LspRefined);

    assert!(
        result.is_err(),
        "should return error for non-existent symbol"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("Not found") || err.contains("NonExistent"),
        "error should indicate symbol not found, got: {err}"
    );
}

#[test]
fn lsp_refined_mode_returns_empty_for_uncalled_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process is never called
    let callers = tethys
        .get_callers("process", tethys::CallerMode::LspRefined)
        .expect("LSP-refined caller query should succeed");

    assert!(
        callers.is_empty(),
        "process should have no callers, got: {callers:?}"
    );
}

#[test]
fn lsp_refined_mode_includes_indexed_baseline() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // LSP refinement must retain every indexed caller.
    let db_callers = tethys
        .get_callers(
            "validate",
            tethys::CallerMode::Indexed {
                call_edges: tethys::CallEdgeSelection::All,
            },
        )
        .expect("get_callers failed");
    let lsp_callers = tethys
        .get_callers("validate", tethys::CallerMode::LspRefined)
        .expect("LSP-refined caller query failed");

    // LSP version should have >= DB version (may have additional from LSP)
    assert!(
        lsp_callers.len() >= db_callers.len(),
        "LSP callers ({}) should be >= DB callers ({})",
        lsp_callers.len(),
        db_callers.len()
    );
}

// ============================================================================
// LSP Integration Tests (require rust-analyzer)
// ============================================================================

#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_refined_mode_adds_semantic_caller_and_deduplicates_overlap() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"lsp_caller_merge\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
pub struct Worker;

impl Worker {
    pub fn run(&self) {}

    pub fn indexed_caller(&self) {
        self.run();
    }
}

pub struct Decoy;

impl Decoy {
    pub fn run(&self) {}
}

pub fn make_worker() -> Worker {
    Worker
}


pub fn inferred_caller() {
    make_worker().run();
}
",
    )
    .expect("write lib.rs");

    let mut tethys = Tethys::new(dir.path()).expect("create Tethys");
    tethys.index().expect("index failed");

    let indexed = tethys
        .get_callers(
            "Worker::run",
            tethys::CallerMode::Indexed {
                call_edges: tethys::CallEdgeSelection::All,
            },
        )
        .expect("indexed caller query");
    let mut indexed_names: Vec<_> = indexed
        .iter()
        .map(|caller| caller.symbol.qualified_name.as_str())
        .collect();
    indexed_names.sort_unstable();
    assert_eq!(
        indexed_names,
        ["Worker::indexed_caller"],
        "the ambiguous inferred receiver must be absent before LSP refinement"
    );

    let refined = tethys
        .get_callers("Worker::run", tethys::CallerMode::LspRefined)
        .expect("LSP-refined caller query");
    let mut refined_names: Vec<_> = refined
        .iter()
        .map(|caller| caller.symbol.qualified_name.as_str())
        .collect();
    refined_names.sort_unstable();
    assert_eq!(
        refined_names,
        ["Worker::indexed_caller", "inferred_caller"],
        "LSP must add the inferred caller while retaining the indexed overlap exactly once"
    );
}

// ============================================================================
// Deduplication Tests
// ============================================================================

#[test]
fn lsp_refined_mode_deduplicates_caller_symbols() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let callers = tethys
        .get_callers("validate", tethys::CallerMode::LspRefined)
        .expect("LSP-refined caller query should succeed");

    let caller_ids: Vec<_> = callers.iter().map(|caller| caller.symbol.id).collect();
    let unique_ids: std::collections::HashSet<_> = caller_ids.iter().collect();

    assert_eq!(
        caller_ids.len(),
        unique_ids.len(),
        "should not have duplicate caller symbols: {caller_ids:?}"
    );
}
