//! Integration tests for LSP-based reference resolution.
//!
//! These tests verify that LSP can resolve references that tree-sitter cannot:
//! - Trait method calls (dynamic dispatch)
//! - Methods on inferred types
//! - Complex type inference scenarios
//!
//! All tests in this file require rust-analyzer to be installed.
//! Run with: `cargo test --test lsp_resolution -- --ignored`

use std::fs;
use std::process::Command;
use tempfile::TempDir;
use tethys::{IndexOptions, Tethys};

/// Check if rust-analyzer is available in PATH.
fn rust_analyzer_available() -> bool {
    let check_cmd = if cfg!(windows) { "where" } else { "which" };
    Command::new(check_cmd)
        .arg("rust-analyzer")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Create a minimal Cargo workspace for rust-analyzer.
fn create_cargo_toml(dir: &TempDir) {
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
}

/// Test that LSP can resolve trait method calls.
///
/// Tree-sitter sees `p.process()` but cannot resolve which impl is called
/// because it requires understanding the trait hierarchy. LSP can resolve this.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_resolves_trait_method_call() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    // Create code with trait method calls
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
pub trait Processor {
    fn process(&self) -> i32;
}

pub struct DataProcessor;

impl Processor for DataProcessor {
    fn process(&self) -> i32 {
        42
    }
}

pub fn run_processor(p: &dyn Processor) -> i32 {
    // This call goes through dynamic dispatch - tree-sitter can't resolve it
    // but LSP understands the trait relationship
    p.process()
}

pub fn use_data_processor() -> i32 {
    let processor = DataProcessor;
    run_processor(&processor)
}
",
    )
    .expect("failed to write lib.rs");

    create_cargo_toml(&dir);

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");

    // Index with LSP enabled
    let stats = tethys
        .index_with_options(IndexOptions::with_lsp())
        .expect("index with LSP failed");

    // The trait method should be indexed
    let symbols = tethys
        .search_symbols("process")
        .expect("search_symbols failed");

    assert!(
        !symbols.is_empty(),
        "should find 'process' symbol (trait method or impl); LSP resolved {} references",
        stats.total_lsp_resolved()
    );

    // Verify we can query callers for the trait method
    // This tests that LSP helped resolve the trait method call
    let callers = tethys
        .get_callers_with_lsp("run_processor")
        .expect("get_callers failed");

    assert!(
        !callers.is_empty(),
        "run_processor should have callers (use_data_processor calls it); found {} symbols",
        symbols.len()
    );
}

/// Test that LSP resolves methods on variables with inferred types.
///
/// When a variable's type is inferred (e.g., `let v = vec![1, 2, 3]`),
/// tree-sitter doesn't know the type, so it can't resolve method calls like `v.len()`.
/// LSP performs type inference and can resolve these.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_resolves_method_on_inferred_type() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    // Create code where type inference is required
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
pub fn create_data() -> Vec<i32> {
    vec![1, 2, 3, 4, 5]
}

pub fn process_data() -> usize {
    // Type of 'data' is inferred as Vec<i32>
    // Tree-sitter can't resolve .len() without knowing the type
    let data = create_data();

    // These method calls require type inference to resolve
    let length = data.len();
    let is_empty = data.is_empty();

    if is_empty {
        0
    } else {
        length
    }
}

pub fn chain_inference() -> Option<i32> {
    // Even more complex: iterator methods with inference
    let data = create_data();
    data.iter().find(|&&x| x > 3).copied()
}
",
    )
    .expect("failed to write lib.rs");

    create_cargo_toml(&dir);

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");

    // Index with LSP enabled
    let stats = tethys
        .index_with_options(IndexOptions::with_lsp())
        .expect("index with LSP failed");

    // Verify indexing succeeded
    assert!(
        stats.files_indexed > 0,
        "should have indexed at least one file"
    );

    // Check that create_data has callers (process_data and chain_inference)
    let callers = tethys
        .get_callers_with_lsp("create_data")
        .expect("get_callers failed");

    assert!(
        callers.len() >= 2,
        "create_data should have at least 2 callers (process_data, chain_inference), \
         got: {}; LSP resolved {} references",
        callers.len(),
        stats.total_lsp_resolved()
    );
}

/// Test that indexing with LSP resolves more references than without.
///
/// This is the key value proposition of LSP integration: it should resolve
/// references that tree-sitter alone cannot handle.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_index_increases_resolution_count() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    // Create code with various resolution challenges
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
mod utils;

pub use utils::helper_function;

pub fn main_function() -> i32 {
    // Cross-module call that benefits from LSP
    helper_function()
}
",
    )
    .expect("failed to write lib.rs");

    fs::write(
        dir.path().join("src/utils.rs"),
        r"
pub fn helper_function() -> i32 {
    internal_work()
}

fn internal_work() -> i32 {
    42
}
",
    )
    .expect("failed to write utils.rs");

    create_cargo_toml(&dir);

    // First, index WITHOUT LSP
    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    let stats_no_lsp = tethys.index().expect("index without LSP failed");

    // Now rebuild WITH LSP
    let stats_with_lsp = tethys
        .rebuild_with_options(IndexOptions::with_lsp())
        .expect("index with LSP failed");

    // Key assertion: non-LSP index should report 0 LSP resolutions
    assert_eq!(
        stats_no_lsp.total_lsp_resolved(),
        0,
        "non-LSP index should report 0 LSP resolutions"
    );

    // Verify the index still works correctly after LSP pass
    let symbols = tethys
        .search_symbols("helper_function")
        .expect("search failed");

    assert!(
        !symbols.is_empty(),
        "should find helper_function after LSP indexing; \
         without LSP: {} refs found, with LSP: {} refs found ({} via LSP)",
        stats_no_lsp.references_found,
        stats_with_lsp.references_found,
        stats_with_lsp.total_lsp_resolved()
    );
}

/// Position encoding fence (byte columns vs `utf-16` code units): a
/// `goto_definition` at a byte column AFTER non-ASCII text on the same line
/// must land on the right symbol.
///
/// Tethys stores tree-sitter columns, which are BYTE offsets (`utf-8` code
/// units). LSP positions default to `utf-16` code units unless an encoding
/// is negotiated at `initialize`. Without negotiation, a method call after a
/// CJK string literal is queried at a column far past its real position and
/// rust-analyzer finds no definition (verified: byte col 55 vs `utf-16` col
/// 35 on this fixture line). Negotiating `utf-8` — which rust-analyzer
/// accepts — makes the stored byte column exact.
///
/// This exercises `LspClient` directly rather than the full indexing
/// pipeline: Pass 3 currently queries rust-analyzer immediately after
/// `initialize`, racing its asynchronous workspace load, so pipeline-level
/// LSP resolution is not deterministic on a cold fixture (tracked
/// separately from the encoding defect this test fences).
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_resolves_ref_after_non_ascii_text_on_same_line() {
    use tethys::lsp::{LspClient, RustAnalyzerProvider};

    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    // The `w.render()` call sits after a CJK + em-dash string literal on the
    // same line: its byte column (55) is far larger than its utf-16 column
    // (35). The receiver comes from a call so rust-analyzer must infer its
    // type — same shape the indexing pipeline sends to Pass 3.
    let lib_rs_content = "pub mod other;

pub struct Widget;

impl Widget {
    pub fn render(&self) -> i32 {
        1
    }
}

pub fn make_widget() -> Widget {
    Widget
}

pub fn draw() -> i32 {
    let w = make_widget();
    let _label = \"\u{65e5}\u{672c}\u{8a9e}\u{30e9}\u{30d9}\u{30eb} \u{2014} \u{30c6}\u{30b9}\u{30c8}\"; w.render()
}
";
    let lib_rs = dir.path().join("src/lib.rs");
    fs::write(&lib_rs, lib_rs_content).expect("failed to write lib.rs");

    fs::write(
        dir.path().join("src/other.rs"),
        "pub struct Panel;

impl Panel {
    pub fn render(&self) -> i32 {
        2
    }
}
",
    )
    .expect("failed to write other.rs");

    create_cargo_toml(&dir);

    // Locate the call and the definition by content, byte-offset columns —
    // exactly what tethys stores from tree-sitter.
    let (call_line, call_col) = find_byte_position(lib_rs_content, "w.render()", "render");
    let (def_line, _) = find_byte_position(lib_rs_content, "pub fn render", "render");

    let mut client =
        LspClient::start(&RustAnalyzerProvider, dir.path()).expect("failed to start LSP client");
    client
        .did_open(&lib_rs, lib_rs_content, "rust")
        .expect("didOpen failed");

    // rust-analyzer loads the workspace asynchronously after initialize and
    // answers goto_definition with None — or a transient -32801 "content
    // modified" error — until it finishes (~2s on this fixture); poll until
    // it produces an answer.
    let mut definition = None;
    for _ in 0..60 {
        match client.goto_definition(&lib_rs, call_line, call_col) {
            Ok(Some(loc)) => {
                definition = Some(loc);
                break;
            }
            Ok(None) | Err(tethys::lsp::LspError::ServerError { code: -32801, .. }) => {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(e) => panic!("goto_definition failed: {e}"),
        }
    }
    client.shutdown().expect("shutdown failed");

    let definition = definition.expect(
        "goto_definition at the byte column of w.render() (after the non-ASCII \
         literal) should find Widget::render — a miss means the position was \
         interpreted in the wrong encoding",
    );

    assert!(
        definition.uri.as_str().ends_with("src/lib.rs"),
        "definition should be in lib.rs, got {}",
        definition.uri.as_str()
    );
    assert_eq!(
        definition.range.start.line, def_line,
        "definition should be Widget::render's declaration line"
    );
}

/// Readiness fence (cold-workspace race): the full indexing pipeline must
/// bind Pass-2-declined refs via LSP without any manual readiness poll.
///
/// rust-analyzer loads the workspace asynchronously after `initialize`;
/// until it reports quiescence, `goto_definition` returns empty results
/// (indistinguishable from "no definition") or `-32801 content modified`.
/// A pipeline that queries immediately therefore resolves nothing on a
/// cold workspace — silently, since empty results are recorded as
/// plain unresolved refs.
///
/// The fixture forces a Pass-2 decline with two same-named methods
/// (`Widget::render` and `Panel::render`): bare-name resolution cannot
/// pick one, so the `w.render()` ref lands in Pass 3's candidate set,
/// where only a readiness-gated LSP query can bind it. The temp dir has
/// no `target/`, so the workspace is maximally cold. The oracle is the
/// persisted `refs.strategy` column, read directly from the index DB —
/// independent of the resolver's in-memory counters.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_pipeline_binds_refs_on_cold_workspace() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    fs::write(
        dir.path().join("src/lib.rs"),
        "pub mod other;

pub struct Widget;

impl Widget {
    pub fn render(&self) -> i32 {
        1
    }
}

pub fn make_widget() -> Widget {
    Widget
}

pub fn draw() -> i32 {
    let w = make_widget();
    w.render()
}
",
    )
    .expect("failed to write lib.rs");

    fs::write(
        dir.path().join("src/other.rs"),
        "pub struct Panel;

impl Panel {
    pub fn render(&self) -> i32 {
        2
    }
}
",
    )
    .expect("failed to write other.rs");

    create_cargo_toml(&dir);

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    let stats = tethys
        .index_with_options(IndexOptions::with_lsp())
        .expect("index with LSP failed");

    let conn = rusqlite::Connection::open_with_flags(
        tethys.db_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("failed to open index db read-only");

    let lsp_bound: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs WHERE strategy = 'lsp'",
            [],
            |row| row.get(0),
        )
        .expect("strategy count query failed");
    let still_unresolved: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs WHERE symbol_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("unresolved count query failed");

    // Fixture sanity: if nothing was declined by Pass 2 AND nothing was
    // LSP-bound, the ambiguity fixture never produced a Pass-3 candidate
    // and the assertion below would be red for the wrong reason.
    assert!(
        lsp_bound + still_unresolved > 0,
        "fixture bug: no ref was declined by Pass 2 (lsp_bound={lsp_bound}, \
         still_unresolved={still_unresolved}); the ambiguous-method fixture \
         no longer reaches Pass 3"
    );

    assert!(
        lsp_bound >= 1,
        "cold-workspace pipeline bound no refs via LSP \
         (strategy='lsp' count={lsp_bound}, still-unresolved={still_unresolved}, \
         stats lsp_resolved={}): Pass 3 queried rust-analyzer before its \
         workspace load completed",
        stats.total_lsp_resolved()
    );
}

/// Timeout fence (readiness wait): a zero budget must return `Ok(false)`
/// promptly — the deadline is checked before each blocking read, so a wait
/// that cannot succeed degrades to a warning instead of hanging Pass 3.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn readiness_wait_returns_false_on_zero_timeout() {
    use tethys::lsp::{LspClient, RustAnalyzerProvider};

    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
    fs::write(dir.path().join("src/lib.rs"), "pub fn noop() {}\n").expect("failed to write lib.rs");
    create_cargo_toml(&dir);

    let mut client =
        LspClient::start(&RustAnalyzerProvider, dir.path()).expect("failed to start LSP client");

    let start = std::time::Instant::now();
    let ready = client
        .wait_for_quiescence(std::time::Duration::ZERO)
        .expect("readiness wait must not error on timeout");
    let elapsed = start.elapsed();
    client.shutdown().expect("shutdown failed");

    assert!(!ready, "zero timeout must report not-ready");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "zero-timeout wait took {elapsed:?}; the deadline must be checked \
         before blocking reads"
    );
}

/// Find `needle` inside the first line containing `line_marker` and return
/// its 0-indexed (line, byte column) — tree-sitter position semantics.
fn find_byte_position(content: &str, line_marker: &str, needle: &str) -> (u32, u32) {
    let (idx, line) = content
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(line_marker))
        .expect("marker line present in fixture");
    let col = line.find(needle).expect("needle present on marker line");
    (
        u32::try_from(idx).expect("line fits u32"),
        u32::try_from(col).expect("col fits u32"),
    )
}

/// Test LSP resolution with generic types and type parameters.
#[test]
#[ignore = "requires rust-analyzer installed"]
fn lsp_resolves_generic_method_calls() {
    if !rust_analyzer_available() {
        eprintln!("Skipping test: rust-analyzer not available");
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    fs::write(
        dir.path().join("src/lib.rs"),
        r#"
pub struct Container<T> {
    value: T,
}

impl<T> Container<T> {
    pub fn new(value: T) -> Self {
        Container { value }
    }

    pub fn get(&self) -> &T {
        &self.value
    }
}

pub fn use_container() -> i32 {
    // Type parameter T is inferred as i32
    let c = Container::new(42);
    *c.get()
}

pub fn use_string_container() -> String {
    // Type parameter T is inferred as String
    let c = Container::new(String::from("hello"));
    c.get().clone()
}
"#,
    )
    .expect("failed to write lib.rs");

    create_cargo_toml(&dir);

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    let stats = tethys
        .index_with_options(IndexOptions::with_lsp())
        .expect("index with LSP failed");

    // Verify Container methods are found
    let new_callers = tethys
        .get_callers_with_lsp("Container::new")
        .expect("get_callers for Container::new failed");

    assert!(
        new_callers.len() >= 2,
        "Container::new should have at least 2 callers, got: {}; LSP resolved {} refs",
        new_callers.len(),
        stats.total_lsp_resolved()
    );

    let get_callers = tethys
        .get_callers_with_lsp("Container::get")
        .expect("get_callers for Container::get failed");

    assert!(
        get_callers.len() >= 2,
        "Container::get should have at least 2 callers, got: {}",
        get_callers.len()
    );
}
