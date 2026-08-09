//! Integration tests for Phase 3 graph operations.
//!
//! These tests verify the graph analysis pipeline through the public Tethys API:
//! - File impact analysis (direct and transitive dependents)
//! - Dependency chain finding (shortest path between files)
//! - Cycle detection

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tethys::{CallEdgeSelection, CallerMode, Error, ReachabilityDirection, Tethys};

/// Create a workspace with a known dependency structure for testing.
///
/// Dependency graph:
/// ```text
///     main.rs
///      /    \
///     v      v
/// auth.rs  cache.rs
///      \    /
///       v  v
///      db.rs (leaf)
/// ```
fn workspace_with_call_graph() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    // Cargo.toml: makes this a valid single-crate workspace so tethys's
    // per-file crate_root lookup finds a crate. Without this, Pass-2-imports
    // is skipped for every file (no known crate).
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_call_graph\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

    // Create src directory
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");

    // main.rs uses auth::User and cache::Cache
    fs::write(
        dir.path().join("src/main.rs"),
        r"
use crate::auth::User;
use crate::cache::Cache;

fn main() {
    let _user = User;
    let _cache = Cache;
}
",
    )
    .expect("write main.rs");

    // auth.rs uses db::Connection
    fs::write(
        dir.path().join("src/auth.rs"),
        r"
use crate::db::Connection;

pub struct User;

impl User {
    pub fn connect() -> Connection {
        Connection
    }
}
",
    )
    .expect("write auth.rs");

    // cache.rs uses db::Connection
    fs::write(
        dir.path().join("src/cache.rs"),
        r"
use crate::db::Connection;

pub struct Cache;

impl Cache {
    pub fn get_conn() -> Connection {
        Connection
    }
}
",
    )
    .expect("write cache.rs");

    // db.rs is the leaf - exports Connection
    fs::write(
        dir.path().join("src/db.rs"),
        r"
pub struct Connection;
",
    )
    .expect("write db.rs");

    // lib.rs declares all modules
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
mod auth;
mod cache;
mod db;
",
    )
    .expect("write lib.rs");

    let tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    (dir, tethys)
}

// ============================================================================
// Impact Analysis Tests
// ============================================================================

/// Dependents of a file impact as `(path, depth)` pairs, in returned order.
fn dependent_depths(impact: &tethys::FileImpact) -> Vec<(&std::path::Path, usize)> {
    impact
        .dependents()
        .iter()
        .map(|dependent| (dependent.file.as_path(), dependent.depth))
        .collect()
}

#[test]
fn get_impact_returns_file_dependents() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let impact = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("get_impact failed");

    // db.rs should have auth.rs and cache.rs as direct dependents
    assert!(
        !impact.direct_dependents().is_empty(),
        "db.rs should have dependents"
    );
}

#[test]
fn get_impact_orders_dependents_by_depth_and_partitions_views() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let impact = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("get_impact failed");

    assert_eq!(
        dependent_depths(&impact),
        [
            (std::path::Path::new("src/auth.rs"), 1),
            (std::path::Path::new("src/cache.rs"), 1),
            (std::path::Path::new("src/main.rs"), 2),
        ]
    );
    assert_eq!(impact.direct_dependents().len(), 2);
    assert_eq!(impact.transitive_dependents().len(), 1);
}

#[test]
fn get_impact_returns_duplicate_routes_once_at_the_minimum_depth() {
    let (dir, mut tethys) = workspace_with_call_graph();
    fs::write(
        dir.path().join("src/main.rs"),
        r"
use crate::auth::User;
use crate::cache::Cache;
use crate::db::Connection;

fn main() {
    let _user = User;
    let _cache = Cache;
    let _connection = Connection;
}
",
    )
    .expect("write main.rs");
    tethys.index().expect("index failed");

    let impact = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("get_impact failed");
    let main_dependents: Vec<_> = impact
        .dependents()
        .iter()
        .filter(|dependent| dependent.file == std::path::Path::new("src/main.rs"))
        .collect();

    assert_eq!(main_dependents.len(), 1);
    assert_eq!(main_dependents[0].depth, 1);
}

#[test]
fn get_impact_returns_transitive_dependents() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let impact = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("get_impact failed");

    // db.rs's transitive dependents should include files that depend on auth.rs and cache.rs
    // (i.e., main.rs depends on auth.rs and cache.rs which depend on db.rs)
    let total_dependents = impact.direct_dependents().len() + impact.transitive_dependents().len();
    assert!(
        total_dependents >= 2,
        "db.rs should have at least 2 total dependents (auth, cache, possibly main), got: {total_dependents}"
    );
}

#[test]
fn get_impact_returns_empty_for_leaf_with_no_dependents() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // main.rs is at the top of the dependency tree - nothing depends on it
    let impact = tethys
        .get_impact(std::path::Path::new("src/main.rs"), None)
        .expect("get_impact failed");

    assert!(
        impact.direct_dependents().is_empty(),
        "main.rs should have no direct dependents, got: {:?}",
        impact.direct_dependents()
    );
}

#[test]
fn get_impact_obeys_the_shared_depth_contract() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let zero = tethys
        .get_impact(std::path::Path::new("src/db.rs"), Some(0))
        .expect("depth zero should validate the target");
    assert_eq!(zero.target, std::path::Path::new("src/db.rs"));
    assert!(
        zero.dependents().is_empty(),
        "depth zero traverses no edges"
    );
    assert!(
        tethys
            .get_impact(std::path::Path::new("src/missing.rs"), Some(0))
            .is_err(),
        "depth zero still validates the requested target"
    );

    let one = tethys
        .get_impact(std::path::Path::new("src/db.rs"), Some(1))
        .expect("depth one impact");
    assert_eq!(
        dependent_depths(&one),
        [
            (std::path::Path::new("src/auth.rs"), 1),
            (std::path::Path::new("src/cache.rs"), 1),
        ]
    );

    let two = tethys
        .get_impact(std::path::Path::new("src/db.rs"), Some(2))
        .expect("depth two impact");
    assert_eq!(
        dependent_depths(&two),
        [
            (std::path::Path::new("src/auth.rs"), 1),
            (std::path::Path::new("src/cache.rs"), 1),
            (std::path::Path::new("src/main.rs"), 2),
        ]
    );

    let default = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("default depth impact");
    assert_eq!(dependent_depths(&default), dependent_depths(&two));

    let oversized = tethys
        .get_impact(std::path::Path::new("src/db.rs"), Some(usize::MAX))
        .expect("oversized depth impact");
    assert_eq!(dependent_depths(&oversized), dependent_depths(&default));
}

#[test]
fn cli_file_impact_renders_depth_partitioned_dependents() {
    let (dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let run = |depth: &str| -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
            .args(["impact", "src/db.rs", "-w"])
            .arg(dir.path())
            .args(["--depth", depth])
            .output()
            .expect("run file impact");
        assert!(
            output.status.success(),
            "file impact failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout must be UTF-8")
    };

    let validate_only = run("0");
    assert!(validate_only.contains("Impact analysis for src/db.rs:"));
    assert!(validate_only.contains("Direct dependents (0 files):"));
    assert!(validate_only.contains("Transitive dependents (0 files beyond direct):"));
    assert!(!validate_only.contains("src/auth.rs"));

    let direct_only = run("1");
    assert!(direct_only.contains("Impact analysis for src/db.rs:"));
    assert!(direct_only.contains("Direct dependents (2 files):"));
    assert!(direct_only.contains("Transitive dependents (0 files beyond direct):"));
    assert!(direct_only.contains("src/auth.rs"));
    assert!(direct_only.contains("src/cache.rs"));
    assert!(!direct_only.contains("src/main.rs"));

    let transitive = run("2");
    assert!(transitive.contains("Transitive dependents (1 files beyond direct):"));
    assert!(transitive.contains("src/main.rs"));
}

#[test]
fn get_symbol_impact_max_depth_limits_transitive_traversal() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Connection sits at the leaf of the call graph. Depth=1 must stop at
    // direct callers (no transitive hops), and direct callers must be
    // invariant under max_depth.
    let depth_1 = tethys
        .get_symbol_impact("Connection", Some(1), CallEdgeSelection::All)
        .expect("get_symbol_impact depth=1 failed");
    let unbounded = tethys
        .get_symbol_impact("Connection", None, CallEdgeSelection::All)
        .expect("get_symbol_impact default depth failed");

    assert!(
        depth_1.transitive_callers().is_empty(),
        "depth=1 should not traverse past direct callers, got {:?}",
        depth_1.transitive_callers()
    );
    assert_eq!(
        depth_1.direct_callers().len(),
        unbounded.direct_callers().len(),
        "direct callers must be invariant under max_depth"
    );
}

// ============================================================================
// Dependency Chain Tests
// ============================================================================

#[test]
fn get_dependency_chain_finds_path() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/auth.rs"),
            std::path::Path::new("src/db.rs"),
        )
        .expect("get_dependency_chain failed");

    assert!(chain.is_some(), "should find path from auth.rs to db.rs");
    let chain = chain.expect("chain should exist");
    assert!(chain.len() >= 2, "path should have at least 2 files");
}

#[test]
fn get_dependency_chain_returns_none_for_unconnected() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // db.rs doesn't depend on main.rs (reverse direction)
    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/db.rs"),
            std::path::Path::new("src/main.rs"),
        )
        .expect("get_dependency_chain failed");

    assert!(chain.is_none(), "should not find path in reverse direction");
}

#[test]
fn get_dependency_chain_returns_single_file_for_same_file() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Equal indexed endpoints are a defined one-file path (tethys-4m9o C4);
    // this replaces the pre-4m9o "either None or trivial" hedge.
    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/db.rs"),
            std::path::Path::new("src/db.rs"),
        )
        .expect("get_dependency_chain failed");

    assert_eq!(
        chain,
        Some(vec![std::path::PathBuf::from("src/db.rs")]),
        "same-file chain must be exactly the one-file path"
    );
}

#[test]
fn get_dependency_chain_finds_shortest_path() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // auth.rs -> db.rs is a direct edge: exactly 2 nodes, no hedge
    // (tethys-4m9o C2; the pre-4m9o form was `if let Some` and could
    // vacuously pass on None).
    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/auth.rs"),
            std::path::Path::new("src/db.rs"),
        )
        .expect("get_dependency_chain failed")
        .expect("direct dependency must produce a chain");

    assert_eq!(
        chain,
        vec![
            std::path::PathBuf::from("src/auth.rs"),
            std::path::PathBuf::from("src/db.rs")
        ],
        "direct dependency is a 2-node chain"
    );
}

/// Workspace whose import graph contains a genuine cycle (a ⇄ b), a target
/// reachable only through the cycle region, and an island file connected to
/// nothing. Imports are USED (bare `use` alone creates no `file_deps` edge).
fn workspace_with_dependency_cycle() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_cycle_graph\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");

    let files = [
        (
            "src/main.rs",
            "use crate::a::A;\n\nfn main() {\n    let _a = A;\n}\n",
        ),
        (
            "src/a.rs",
            "use crate::b::B;\n\npub struct A;\n\npub fn from_a() {\n    let _b = B;\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::a::A;\nuse crate::target::T;\n\npub struct B;\n\npub fn from_b() {\n    let _a = A;\n    let _t = T;\n}\n",
        ),
        ("src/target.rs", "pub struct T;\n"),
        ("src/island.rs", "pub struct Island;\n"),
    ];
    for (path, content) in files {
        fs::write(dir.path().join(path), content).expect("write source file");
    }

    let tethys = Tethys::new(dir.path()).expect("failed to create tethys");
    (dir, tethys)
}

#[test]
fn get_dependency_chain_traverses_cycle_region_and_terminates() {
    let (_dir, mut tethys) = workspace_with_dependency_cycle();
    tethys.index().expect("index failed");

    // Pre-4m9o, the walk-enumerating CTE hung on ANY query whose source
    // reaches a cycle (tethys-vwrn); this fixture is the regression fence
    // for termination (C1) and for shortest-through-cycle correctness (C2).
    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/a.rs"),
            std::path::Path::new("src/target.rs"),
        )
        .expect("get_dependency_chain failed")
        .expect("target is reachable through the cycle region");

    assert_eq!(
        chain,
        vec![
            std::path::PathBuf::from("src/a.rs"),
            std::path::PathBuf::from("src/b.rs"),
            std::path::PathBuf::from("src/target.rs")
        ],
        "shortest route through the cycle region"
    );
}

#[test]
fn get_dependency_chain_returns_none_for_island_despite_cycle() {
    let (_dir, mut tethys) = workspace_with_dependency_cycle();
    tethys.index().expect("index failed");

    // Disconnected must be a fast None even when the source's reachable
    // region contains a cycle (C5) — the pre-4m9o hang applied to every
    // disconnected query on cyclic indexes.
    let chain = tethys
        .get_dependency_chain(
            std::path::Path::new("src/a.rs"),
            std::path::Path::new("src/island.rs"),
        )
        .expect("get_dependency_chain failed");

    assert_eq!(
        chain, None,
        "island is unreachable: None, not error or hang"
    );
}

#[test]
fn get_dependency_chain_reports_from_first_when_both_endpoints_missing() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Endpoint validation order is part of the established contract (C6):
    // `from` is checked before `to`, so the error names the from-path.
    let err = tethys
        .get_dependency_chain(
            std::path::Path::new("src/nope_from.rs"),
            std::path::Path::new("src/nope_to.rs"),
        )
        .expect_err("both endpoints missing must error");

    assert!(
        matches!(err, tethys::Error::NotFound(_)),
        "both-missing must be the established NotFound, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("nope_from.rs") && !msg.contains("nope_to.rs"),
        "error must name the from-endpoint (checked first), got: {msg}"
    );
}

fn db_file_id(connection: &Connection, path: &str) -> i64 {
    connection
        .query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
            row.get(0)
        })
        .expect("indexed fixture file")
}

fn workspace_with_exact_cycle_graph() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create cycle workspace");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_exact_cycles\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src/weird dir")).expect("create weird source dir");
    fs::write(dir.path().join("src/lib.rs"), "mod a;\nmod b;\nmod c;\n").expect("write lib.rs");
    for (path, source) in [
        ("src/a.rs", "pub struct A;\n"),
        ("src/b.rs", "pub struct B;\n"),
        ("src/c.rs", "pub struct C;\n"),
        ("src/weird dir/Ω file.rs", "pub struct Weird;\n"),
    ] {
        fs::write(dir.path().join(path), source).expect("write cycle fixture source");
    }

    let mut tethys = Tethys::new(dir.path()).expect("create Tethys");
    tethys.index().expect("index exact cycle workspace");

    let connection = Connection::open(tethys.db_path()).expect("open cycle fixture database");
    connection
        .execute("DELETE FROM file_deps", [])
        .expect("clear generated file dependencies");
    let edges = [
        ("src/a.rs", "src/b.rs"),
        ("src/a.rs", "src/c.rs"),
        ("src/b.rs", "src/a.rs"),
        ("src/b.rs", "src/c.rs"),
        ("src/c.rs", "src/a.rs"),
        ("src/c.rs", "src/b.rs"),
        ("src/weird dir/Ω file.rs", "src/weird dir/Ω file.rs"),
    ];
    for (from, to) in edges {
        connection
            .execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                rusqlite::params![db_file_id(&connection, from), db_file_id(&connection, to)],
            )
            .expect("insert exact cycle edge");
    }
    drop(connection);
    (dir, tethys)
}

fn workspace_with_four_file_cycle() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create four-file cycle workspace");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_four_file_cycle\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create source directory");
    for path in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
        fs::write(dir.path().join(path), "pub struct Node;\n").expect("write cycle source");
    }

    let mut tethys = Tethys::new(dir.path()).expect("create Tethys");
    tethys.index().expect("index four-file cycle workspace");

    let connection = Connection::open(tethys.db_path()).expect("open cycle fixture database");
    connection
        .execute("DELETE FROM file_deps", [])
        .expect("clear generated file dependencies");
    for (from, to) in [
        ("src/a.rs", "src/b.rs"),
        ("src/b.rs", "src/c.rs"),
        ("src/c.rs", "src/d.rs"),
        ("src/d.rs", "src/a.rs"),
    ] {
        connection
            .execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                rusqlite::params![db_file_id(&connection, from), db_file_id(&connection, to)],
            )
            .expect("insert four-file cycle edge");
    }
    drop(connection);
    (dir, tethys)
}

fn cycle_paths(cycles: &[tethys::Cycle]) -> Vec<Vec<PathBuf>> {
    cycles.iter().map(|cycle| cycle.files.clone()).collect()
}

// ============================================================================
// Cycle Detection Tests
// ============================================================================

#[test]
fn detect_cycles_returns_exact_canonical_directed_set() {
    let (_dir, tethys) = workspace_with_exact_cycle_graph();
    let cycles = tethys.detect_cycles().expect("detect exact cycles");
    let expected = vec![
        vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
        vec![
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/b.rs"),
            PathBuf::from("src/c.rs"),
        ],
        vec![PathBuf::from("src/a.rs"), PathBuf::from("src/c.rs")],
        vec![
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/c.rs"),
            PathBuf::from("src/b.rs"),
        ],
        vec![PathBuf::from("src/b.rs"), PathBuf::from("src/c.rs")],
        vec![PathBuf::from("src/weird dir/Ω file.rs")],
    ];

    assert_eq!(cycle_paths(&cycles), expected);
    for cycle in &cycles {
        let mut unique = cycle.files.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), cycle.files.len());
        assert!(
            cycle.files.len() <= 1 || cycle.files.first() != cycle.files.last(),
            "cycle API must not repeat its first path"
        );
    }
    for repeat in 1..=2 {
        assert_eq!(
            cycle_paths(&tethys.detect_cycles().expect("repeat exact cycle query")),
            expected,
            "unchanged index must return stable cycle order on repeat {repeat}"
        );
    }
}

#[test]
fn detect_cycles_returns_exact_four_file_cycle() {
    let (_dir, tethys) = workspace_with_four_file_cycle();
    let cycles = tethys.detect_cycles().expect("detect four-file cycle");

    assert_eq!(
        cycle_paths(&cycles),
        vec![vec![
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/b.rs"),
            PathBuf::from("src/c.rs"),
            PathBuf::from("src/d.rs"),
        ]],
        "four-file cycle should be returned exactly once in dependency order"
    );
}

#[test]
fn detect_cycles_rejects_dangling_dependency_endpoint() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let connection = Connection::open(tethys.db_path()).expect("open indexed database");
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("disable foreign keys for corruption fixture");
    let from_id = db_file_id(&connection, "src/main.rs");
    connection
        .execute(
            "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
            rusqlite::params![from_id, 999_999_i64],
        )
        .expect("insert dangling dependency edge");
    drop(connection);

    let error = tethys
        .detect_cycles()
        .expect_err("dangling dependency endpoint must fail");
    assert!(
        matches!(&error, tethys::Error::NotFound(message) if message.contains("999999")),
        "expected typed dangling endpoint error, got: {error:?}"
    );
}

#[test]
fn detect_cycles_returns_empty_for_acyclic_workspace() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let result = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed");

    // The workspace_with_call_graph has no cycles (acyclic)
    assert!(
        result.is_empty(),
        "acyclic workspace should have no cycles, got: {result:?}"
    );
}

/// This test verifies that two-file cyclic dependencies are detected.
#[test]
fn cyclic_dependencies_are_detected() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_cycle_two\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

    fs::create_dir_all(dir.path().join("src")).expect("create src dir");

    // Create a simple A -> B -> A cycle
    fs::write(
        dir.path().join("src/lib.rs"),
        r"
mod a;
mod b;
",
    )
    .expect("write lib.rs");

    fs::write(
        dir.path().join("src/a.rs"),
        r"
use crate::b::B;

pub struct A;

impl A {
    pub fn get_b() -> B { B }
}
",
    )
    .expect("write a.rs");

    fs::write(
        dir.path().join("src/b.rs"),
        r"
use crate::a::A;

pub struct B;

impl B {
    pub fn get_a() -> A { A }
}
",
    )
    .expect("write b.rs");

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    tethys.index().expect("index failed");

    // Cycle detection should find the A <-> B cycle
    let cycles = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed");
    assert!(!cycles.is_empty(), "should detect the A <-> B cycle");

    // Verify the cycle contains both a.rs and b.rs
    let cycle = &cycles[0];
    let paths: Vec<String> = cycle
        .files
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        paths.iter().any(|p| p.contains("a.rs")),
        "cycle should contain a.rs"
    );
    assert!(
        paths.iter().any(|p| p.contains("b.rs")),
        "cycle should contain b.rs"
    );

    // Also verify the cyclic dependencies ARE recorded in the file_deps table
    let deps_a = tethys
        .get_dependencies(std::path::Path::new("src/a.rs"))
        .expect("get_dependencies failed");
    let deps_b = tethys
        .get_dependencies(std::path::Path::new("src/b.rs"))
        .expect("get_dependencies failed");

    assert!(
        deps_a.iter().any(|p| p.to_string_lossy().contains("b.rs")),
        "a.rs should depend on b.rs"
    );
    assert!(
        deps_b.iter().any(|p| p.to_string_lossy().contains("a.rs")),
        "b.rs should depend on a.rs"
    );
}

/// This test verifies that three-file cyclic dependencies are detected.
#[test]
fn three_file_cycle_dependencies_are_detected() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_cycle_three\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

    fs::create_dir_all(dir.path().join("src")).expect("create src dir");

    // Create A -> B -> C -> A cycle
    fs::write(dir.path().join("src/lib.rs"), "mod a;\nmod b;\nmod c;").expect("write lib.rs");

    fs::write(
        dir.path().join("src/a.rs"),
        r"
use crate::b::B;

pub struct A;

impl A {
    pub fn get() -> B { B }
}
",
    )
    .expect("write a.rs");

    fs::write(
        dir.path().join("src/b.rs"),
        r"
use crate::c::C;

pub struct B;

impl B {
    pub fn get() -> C { C }
}
",
    )
    .expect("write b.rs");

    fs::write(
        dir.path().join("src/c.rs"),
        r"
use crate::a::A;

pub struct C;

impl C {
    pub fn get() -> A { A }
}
",
    )
    .expect("write c.rs");

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    tethys.index().expect("index failed");

    // Verify all cycle edges are recorded in file_deps
    let deps_a = tethys
        .get_dependencies(std::path::Path::new("src/a.rs"))
        .expect("get_dependencies failed");
    let deps_b = tethys
        .get_dependencies(std::path::Path::new("src/b.rs"))
        .expect("get_dependencies failed");
    let deps_c = tethys
        .get_dependencies(std::path::Path::new("src/c.rs"))
        .expect("get_dependencies failed");

    assert!(
        deps_a.iter().any(|p| p.to_string_lossy().contains("b.rs")),
        "a.rs should depend on b.rs, got: {deps_a:?}"
    );
    assert!(
        deps_b.iter().any(|p| p.to_string_lossy().contains("c.rs")),
        "b.rs should depend on c.rs, got: {deps_b:?}"
    );
    assert!(
        deps_c.iter().any(|p| p.to_string_lossy().contains("a.rs")),
        "c.rs should depend on a.rs, got: {deps_c:?}"
    );

    // Cycle detection should find the A -> B -> C -> A cycle
    let cycles = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed");
    assert!(!cycles.is_empty(), "should detect the 3-file cycle");

    let cycle = &cycles[0];
    assert_eq!(cycle.files.len(), 3, "cycle should have 3 files");

    let paths: Vec<String> = cycle
        .files
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        paths.iter().any(|p| p.contains("a.rs")),
        "cycle should contain a.rs"
    );
    assert!(
        paths.iter().any(|p| p.contains("b.rs")),
        "cycle should contain b.rs"
    );
    assert!(
        paths.iter().any(|p| p.contains("c.rs")),
        "cycle should contain c.rs"
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn get_impact_returns_error_for_nonexistent_file() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let result = tethys.get_impact(std::path::Path::new("src/nonexistent.rs"), None);

    assert!(
        result.is_err(),
        "should return error for non-indexed file, got: {result:?}"
    );
}

#[test]
fn get_dependency_chain_returns_error_for_nonexistent_from() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let err = tethys
        .get_dependency_chain(
            std::path::Path::new("src/nonexistent.rs"),
            std::path::Path::new("src/db.rs"),
        )
        .expect_err("missing 'from' file must error");

    assert!(
        matches!(err, tethys::Error::NotFound(_)),
        "missing 'from' must be the established NotFound, got: {err:?}"
    );
}

#[test]
fn get_dependency_chain_returns_error_for_nonexistent_to() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let err = tethys
        .get_dependency_chain(
            std::path::Path::new("src/db.rs"),
            std::path::Path::new("src/nonexistent.rs"),
        )
        .expect_err("missing 'to' file must error");

    assert!(
        matches!(err, tethys::Error::NotFound(_)),
        "missing 'to' must be the established NotFound, got: {err:?}"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn graph_operations_work_after_reindex() {
    let (_dir, mut tethys) = workspace_with_call_graph();

    // Index twice
    tethys.index().expect("first index failed");
    tethys.rebuild().expect("rebuild failed");

    // Graph operations should still work
    let impact = tethys
        .get_impact(std::path::Path::new("src/db.rs"), None)
        .expect("get_impact failed after reindex");

    assert!(
        !impact.direct_dependents().is_empty(),
        "impact analysis should work after reindex"
    );

    // Cycle detection should work and return empty (acyclic graph)
    let cycles = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed after reindex");
    assert!(
        cycles.is_empty(),
        "acyclic workspace should have no cycles after reindex"
    );
}

#[test]
fn empty_workspace_detect_cycles_returns_empty() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");

    tethys.index().expect("index failed");

    // Empty workspace has no dependencies and thus no cycles
    let cycles = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed");
    assert!(cycles.is_empty(), "empty workspace should have no cycles");
}

#[test]
fn single_file_workspace_detect_cycles_returns_empty() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::write(dir.path().join("src/lib.rs"), "pub fn hello() {}").expect("write lib.rs");

    let mut tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    tethys.index().expect("index failed");

    // Single file with no dependencies has no cycles
    let cycles = tethys
        .detect_cycles()
        .expect("detect_cycles should succeed");
    assert!(
        cycles.is_empty(),
        "single file workspace should have no cycles"
    );
}

// ============================================================================
// Symbol-Level Graph Analysis Tests: get_callers
// ============================================================================

/// Create a workspace with intra-file symbol references for symbol graph testing.
///
/// This workspace has symbols that call other symbols within the same file,
/// which is required for the symbol graph since cross-file reference resolution
/// is not yet implemented.
///
/// Symbol graph:
/// ```text
///   process() -> validate() -> Helper::new()
///                            -> Helper::check()
/// ```
fn workspace_with_intra_file_calls() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_intra_file\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

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

    let tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    (dir, tethys)
}

#[test]
fn get_callers_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let result = tethys.get_callers(
        "NonExistent",
        CallerMode::Indexed {
            call_edges: CallEdgeSelection::All,
        },
    );

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
fn get_callers_returns_empty_for_uncalled_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process is the top-level function - nothing calls it
    let callers = tethys
        .get_callers(
            "process",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for process should succeed");

    assert!(
        callers.is_empty(),
        "process should have no callers, got: {callers:?}"
    );
}

#[test]
fn get_callers_returns_caller_symbol_and_indexed_file() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let callers = tethys
        .get_callers(
            "validate",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for validate should succeed");

    assert_eq!(callers.len(), 1, "validate has one direct caller");
    assert_eq!(callers[0].symbol.qualified_name, "process");
    assert_eq!(callers[0].file.to_string_lossy(), "src/lib.rs");
    assert!(
        !callers[0].symbol.is_test,
        "a non-test caller must not inherit call-count evidence as is_test"
    );
}

#[test]
fn get_callers_cross_file_refs_resolved() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Connection is referenced from other files via `use crate::db::Connection`,
    // Cross-file references are now resolved in Pass 2.
    let callers = tethys
        .get_callers(
            "Connection",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for Connection should succeed");

    assert!(
        !callers.is_empty(),
        "cross-file callers should be resolved, got empty"
    );
}

// ============================================================================
// Symbol-Level Graph Analysis Tests: get_symbol_dependencies
// ============================================================================

#[test]
fn get_symbol_dependencies_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let result = tethys.get_symbol_dependencies("DoesNotExist");

    assert!(
        result.is_err(),
        "should return error for non-existent symbol"
    );
}

#[test]
fn get_symbol_dependencies_returns_empty_for_leaf_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // Helper is a leaf struct with no outgoing calls
    let deps = tethys
        .get_symbol_dependencies("Helper")
        .expect("get_symbol_dependencies for Helper should succeed");

    assert!(
        deps.is_empty(),
        "Helper (leaf struct) should have no dependencies, got: {deps:?}"
    );
}

#[test]
fn get_symbol_dependencies_finds_callees() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // validate calls Helper::new and Helper::check
    let deps = tethys
        .get_symbol_dependencies("validate")
        .expect("get_symbol_dependencies for validate should succeed");

    assert!(!deps.is_empty(), "validate should have dependencies");
}

#[test]
fn get_symbol_dependencies_cross_file_not_resolved() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // main uses User and Cache ONLY as values (`let _user = User;`). Those are
    // recorded as `value` refs (tethys-ygjx) but deliberately excluded from the
    // call graph — a value-use is not a call — so main has no call-graph
    // dependencies. (Before fn-as-value extraction this was empty because the
    // refs were never emitted at all; now it is empty by call-graph design.)
    let deps = tethys
        .get_symbol_dependencies("main")
        .expect("get_symbol_dependencies for main should succeed");

    assert!(
        deps.is_empty(),
        "value-uses must not appear as call-graph dependencies, got: {deps:?}"
    );
}

// ============================================================================
// Symbol-Level Graph Analysis Tests: get_symbol_impact
// ============================================================================

#[test]
fn get_symbol_impact_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    let result = tethys.get_symbol_impact("NoSuchSymbol", None, CallEdgeSelection::All);

    assert!(
        result.is_err(),
        "should return error for non-existent symbol"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("Not found") || err.contains("NoSuchSymbol"),
        "error should indicate symbol not found, got: {err}"
    );
}

#[test]
fn get_symbol_impact_reports_callers_at_their_minimum_depth() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let impact = tethys
        .get_symbol_impact("Helper::check", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for Helper::check should succeed");

    assert_eq!(impact.target.qualified_name, "Helper::check");
    let callers: Vec<_> = impact
        .callers()
        .iter()
        .map(|entry| {
            (
                entry.symbol.qualified_name.as_str(),
                entry.file.to_string_lossy(),
                entry.depth,
            )
        })
        .collect();
    assert_eq!(
        callers,
        [
            ("validate", "src/lib.rs".into(), 1),
            ("process", "src/lib.rs".into(), 2),
        ]
    );
    assert_eq!(impact.direct_callers().len(), 1);
    assert_eq!(impact.transitive_callers().len(), 1);
}

#[test]
fn get_symbol_impact_returns_each_caller_once_at_shortest_depth() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"impact_depths\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn leaf() {}\n\
         pub fn middle() { leaf(); }\n\
         pub fn direct() { leaf(); middle(); }\n\
         pub fn top() { direct(); }\n",
    )
    .expect("write lib.rs");
    let mut tethys = Tethys::new(dir.path()).expect("create Tethys");
    tethys.index().expect("index failed");

    let impact = tethys
        .get_symbol_impact("leaf", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for leaf should succeed");
    let callers: Vec<_> = impact
        .callers()
        .iter()
        .map(|entry| (entry.symbol.qualified_name.as_str(), entry.depth))
        .collect();

    assert_eq!(
        callers,
        [("direct", 1), ("middle", 1), ("top", 2)],
        "the direct caller reached again through middle stays unique at depth one"
    );
    assert_eq!(
        impact
            .direct_callers()
            .iter()
            .map(|entry| entry.symbol.qualified_name.as_str())
            .collect::<Vec<_>>(),
        ["direct", "middle"]
    );
    assert_eq!(
        impact
            .transitive_callers()
            .iter()
            .map(|entry| entry.symbol.qualified_name.as_str())
            .collect::<Vec<_>>(),
        ["top"]
    );
}

#[test]
fn get_symbol_impact_obeys_the_shared_depth_contract() {
    fn caller_depths(impact: &tethys::SymbolImpact) -> Vec<(&str, usize)> {
        impact
            .callers()
            .iter()
            .map(|entry| (entry.symbol.qualified_name.as_str(), entry.depth))
            .collect()
    }

    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let zero = tethys
        .get_symbol_impact("Helper::check", Some(0), CallEdgeSelection::All)
        .expect("depth zero should validate the target");
    assert_eq!(zero.target.qualified_name, "Helper::check");
    assert!(zero.callers().is_empty(), "depth zero traverses no edges");
    assert!(
        tethys
            .get_symbol_impact("NoSuchSymbol", Some(0), CallEdgeSelection::All)
            .is_err(),
        "depth zero still validates the requested target"
    );

    let one = tethys
        .get_symbol_impact("Helper::check", Some(1), CallEdgeSelection::All)
        .expect("depth one impact");
    assert_eq!(caller_depths(&one), [("validate", 1)]);

    let two = tethys
        .get_symbol_impact("Helper::check", Some(2), CallEdgeSelection::All)
        .expect("depth two impact");
    assert_eq!(caller_depths(&two), [("validate", 1), ("process", 2)]);

    let default = tethys
        .get_symbol_impact("Helper::check", None, CallEdgeSelection::All)
        .expect("default depth impact");
    assert_eq!(
        caller_depths(&default),
        caller_depths(&two),
        "the default depth of 50 saturates this finite graph"
    );

    let oversized = usize::try_from(u64::from(u32::MAX) + 1).unwrap_or(usize::MAX);
    let saturated = tethys
        .get_symbol_impact("Helper::check", Some(oversized), CallEdgeSelection::All)
        .expect("oversized depth impact");
    assert_eq!(
        caller_depths(&saturated),
        caller_depths(&two),
        "increasing depth past the storage width cannot reduce results"
    );
}

#[test]
fn get_symbol_impact_returns_empty_for_uncalled_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process is never called by other symbols
    let impact = tethys
        .get_symbol_impact("process", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for process should succeed");

    assert!(
        impact.direct_callers().is_empty(),
        "process should have no direct callers, got: {:?}",
        impact.direct_callers()
    );
    assert!(
        impact.transitive_callers().is_empty(),
        "process should have no transitive callers, got: {:?}",
        impact.transitive_callers()
    );
}

#[test]
fn get_symbol_impact_finds_direct_callers() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // validate is called by process directly
    let impact = tethys
        .get_symbol_impact("validate", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for validate should succeed");

    assert!(
        !impact.direct_callers().is_empty(),
        "validate should have direct callers (process)"
    );
}

#[test]
fn get_symbol_impact_targets_correct_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let impact = tethys
        .get_symbol_impact("validate", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for validate should succeed");

    assert_eq!(
        impact.target.qualified_name, "validate",
        "impact should identify the requested target symbol"
    );
}

#[test]
fn get_symbol_impact_cross_file_resolved() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Connection's callers are cross-file - now resolved in Pass 2
    let impact = tethys
        .get_symbol_impact("Connection", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for Connection should succeed");

    assert!(
        !impact.direct_callers().is_empty(),
        "cross-file callers should be resolved, got empty"
    );
}

// ============================================================================
// Call Edges Tests
// ============================================================================

/// Verify that `call_edges` table is populated after indexing.
#[test]
fn call_edges_populated_after_indexing() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // The workspace has: process() -> validate() -> Helper::new(), Helper::check()
    // All intra-file calls should result in call_edges being populated

    // validate is called by process
    let callers = tethys
        .get_callers(
            "validate",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for validate should succeed");

    assert!(
        !callers.is_empty(),
        "validate should have callers via call_edges"
    );

    let all_symbols: Vec<&str> = callers
        .iter()
        .map(|caller| caller.symbol.qualified_name.as_str())
        .collect();
    assert!(
        all_symbols.iter().any(|n| n.contains("process")),
        "validate should be called by process, got: {all_symbols:?}"
    );
}

/// Member reads must not mint call edges (tethys-xebx C10/D3).
///
/// The fixture's `this.Data` read RESOLVES same-file with `in_symbol_id`
/// set — exactly the row shape `populate_call_edges` selects — so a
/// forgotten `field_access` exclusion would list `Reader` as a caller of
/// the property. The sibling `Helper()` call proves the filter is not
/// over-broad.
#[test]
fn member_reads_produce_no_call_edges() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    // Workspace discovery needs a crate root, same as workspace_with_files
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"member_reads_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        dir.path().join("Widget.cs"),
        r"
namespace App
{
    public class Widget
    {
        public int Data { get; set; }
        public int Reader()
        {
            Helper();
            return this.Data;
        }
        public void Helper() { }
    }
}
",
    )
    .expect("write Widget.cs");
    let mut tethys = Tethys::new(dir.path()).expect("tethys init");
    tethys.index().expect("index failed");

    let helper_callers = tethys
        .get_callers(
            "Widget::Helper",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for Widget::Helper should succeed");
    assert!(
        !helper_callers.is_empty(),
        "the real call must still produce a call edge"
    );

    let data_callers = tethys
        .get_callers(
            "Widget::Data",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for Widget::Data should succeed");
    assert!(
        data_callers.is_empty(),
        "a resolved member read minted a call edge: {data_callers:?}"
    );
}

/// Calls/constructs must never bind a same-file data member (tethys-xebx
/// D10). The fixture embeds the exact drift shape the corpus audit caught:
/// `new Exception(...)` in a file whose type declares a property named
/// `Exception`. A kind-blind Pass-1 map binds the BCL constructor to the
/// property and fabricates a `Thrower -> Exception` call edge.
#[test]
fn construct_ref_does_not_bind_same_file_property() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"construct_decoy_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        dir.path().join("StepFailed.cs"),
        r#"
using System;
namespace App
{
    public class StepFailed
    {
        public Exception Exception { get; }
        public StepFailed Thrower()
        {
            return Fail(new Exception("timeout"));
        }
        public StepFailed Fail(Exception e) { return this; }
    }
}
"#,
    )
    .expect("write StepFailed.cs");
    let mut tethys = Tethys::new(dir.path()).expect("tethys init");
    tethys.index().expect("index failed");

    let property_callers = tethys
        .get_callers(
            "StepFailed::Exception",
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        )
        .expect("get_callers for the Exception property should succeed");
    assert!(
        property_callers.is_empty(),
        "a construct ref bound to the same-file property: {property_callers:?}"
    );
}

/// Verify transitive callers work with `call_edges`.
#[test]
fn transitive_callers_via_call_edges() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // Helper::check is called by validate, which is called by process
    // So Helper::check should have process as a transitive caller
    let impact = tethys
        .get_symbol_impact("Helper::check", None, CallEdgeSelection::All)
        .expect("get_symbol_impact for Helper::check should succeed");

    let total = impact.callers().len();
    assert!(
        total >= 1,
        "Helper::check should have at least 1 caller, got: {total}"
    );
}

/// Verify `get_symbol_dependencies` works with `call_edges`.
#[test]
fn symbol_dependencies_via_call_edges() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // validate calls Helper::new and Helper::check
    let deps = tethys
        .get_symbol_dependencies("validate")
        .expect("get_symbol_dependencies for validate should succeed");

    assert!(
        !deps.is_empty(),
        "validate should have dependencies (Helper::new, Helper::check)"
    );
}

// ============================================================================
// Reachability Analysis Tests
// ============================================================================

#[test]
fn get_forward_reachable_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys.get_forward_reachable("NoSuchSymbol", Some(10));

    assert!(
        result.is_err(),
        "should return error for non-existent symbol"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("Not found") || err.contains("NoSuchSymbol"),
        "error should indicate symbol not found, got: {err}"
    );
}

#[test]
fn get_backward_reachable_returns_error_for_nonexistent_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys.get_backward_reachable("NoSuchSymbol", Some(10));

    assert!(
        result.is_err(),
        "should return error for non-existent symbol"
    );
}

#[test]
fn get_forward_reachable_finds_direct_callees() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process calls validate
    let result = tethys
        .get_forward_reachable("process", Some(1))
        .expect("get_forward_reachable for process should succeed");

    assert!(
        !result.is_empty(),
        "process should have forward reachable symbols (validate)"
    );
    assert_eq!(
        result.direction,
        tethys::ReachabilityDirection::Forward,
        "direction should be Forward"
    );

    // All results should be at depth 1 (max_depth=1)
    for path in &result.reachable {
        assert_eq!(path.depth, 1, "all results should be at depth 1");
    }
}

#[test]
fn get_forward_reachable_finds_transitive_callees() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process -> validate -> Helper::new, Helper::check
    // With depth 3, we should reach Helper::new and Helper::check
    let result = tethys
        .get_forward_reachable("process", Some(3))
        .expect("get_forward_reachable for process should succeed");

    assert!(
        result.reachable_count() >= 2,
        "process should reach at least 2 symbols with depth 3, got: {}",
        result.reachable_count()
    );

    // Check that we have symbols at different depths
    let depths: std::collections::HashSet<usize> =
        result.reachable.iter().map(|r| r.depth).collect();
    assert!(
        !depths.is_empty(),
        "should have symbols at different depths, got depths: {depths:?}"
    );
}

#[test]
fn get_forward_reachable_returns_empty_for_leaf_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // Helper::check doesn't call anything
    let result = tethys
        .get_forward_reachable("Helper::check", Some(10))
        .expect("get_forward_reachable for Helper::check should succeed");

    assert!(
        result.is_empty(),
        "Helper::check should have no forward reachable symbols, got: {:?}",
        result.reachable
    );
}

#[test]
fn get_backward_reachable_finds_direct_callers() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // validate is called by process
    let result = tethys
        .get_backward_reachable("validate", Some(1))
        .expect("get_backward_reachable for validate should succeed");

    assert!(
        !result.is_empty(),
        "validate should have backward reachable symbols (process)"
    );
    assert_eq!(
        result.direction,
        tethys::ReachabilityDirection::Backward,
        "direction should be Backward"
    );
}

#[test]
fn get_backward_reachable_finds_transitive_callers() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // Helper::new is called by validate, which is called by process
    // With depth 3, we should reach both validate and process
    let result = tethys
        .get_backward_reachable("Helper::new", Some(3))
        .expect("get_backward_reachable for Helper::new should succeed");

    assert!(
        result.reachable_count() >= 1,
        "Helper::new should have at least 1 backward reachable symbol, got: {}",
        result.reachable_count()
    );
}

#[test]
fn get_backward_reachable_returns_empty_for_uncalled_symbol() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // process is not called by anything
    let result = tethys
        .get_backward_reachable("process", Some(10))
        .expect("get_backward_reachable for process should succeed");

    assert!(
        result.is_empty(),
        "process should have no backward reachable symbols, got: {:?}",
        result.reachable
    );
}

#[test]
fn reachability_respects_max_depth() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // With depth 1, process should only reach validate (direct callee)
    let result_depth_1 = tethys
        .get_forward_reachable("process", Some(1))
        .expect("get_forward_reachable depth 1 should succeed");

    // With depth 3, process should reach more symbols (validate, Helper::new, Helper::check)
    let result_depth_3 = tethys
        .get_forward_reachable("process", Some(3))
        .expect("get_forward_reachable depth 3 should succeed");

    assert!(
        result_depth_3.reachable_count() >= result_depth_1.reachable_count(),
        "depth 3 should reach at least as many symbols as depth 1"
    );
}

#[test]
fn reachability_result_at_depth_filters_correctly() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys
        .get_forward_reachable("process", Some(3))
        .expect("get_forward_reachable should succeed");

    let at_depth_1 = result.at_depth(1);
    let at_depth_2 = result.at_depth(2);

    // All results at depth 1 should have depth == 1
    for path in &at_depth_1 {
        assert_eq!(path.depth, 1, "at_depth(1) should only return depth 1");
    }

    // All results at depth 2 should have depth == 2
    for path in &at_depth_2 {
        assert_eq!(path.depth, 2, "at_depth(2) should only return depth 2");
    }
}

#[test]
fn reachability_paths_are_valid() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys
        .get_forward_reachable("process", Some(3))
        .expect("get_forward_reachable should succeed");

    for path in &result.reachable {
        // Path length should equal depth
        assert_eq!(
            path.path.len(),
            path.depth,
            "path length should equal depth for {:?}",
            path.target.qualified_name
        );

        // Path should end with the target
        if !path.path.is_empty() {
            let last = path.path.last().expect("path should not be empty");
            assert_eq!(
                last.id, path.target.id,
                "path should end with target symbol"
            );
        }
    }
}

#[test]
fn reachability_cross_file_works() {
    let (_dir, mut tethys) = workspace_with_call_graph();
    tethys.index().expect("index failed");

    // Connection is in db.rs and is referenced from auth.rs and cache.rs
    // Those references are now resolved via cross-file resolution
    let result = tethys
        .get_backward_reachable("Connection", Some(5))
        .expect("get_backward_reachable for Connection should succeed");

    // Cross-file references should be resolved
    assert!(
        !result.is_empty(),
        "Connection should have backward reachable symbols from other files"
    );
}

#[test]
fn reachability_max_depth_none_uses_default() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    // When max_depth is None, the implementation should use default (50)
    let result = tethys
        .get_forward_reachable("process", None)
        .expect("get_forward_reachable with None depth should succeed");

    // Verify the result captures the default max_depth
    assert_eq!(
        result.max_depth, 50,
        "max_depth should be 50 when None is passed"
    );

    // Verify it still finds reachable symbols (same as with explicit depth)
    let result_explicit = tethys
        .get_forward_reachable("process", Some(50))
        .expect("get_forward_reachable with explicit depth should succeed");

    assert_eq!(
        result.reachable_count(),
        result_explicit.reachable_count(),
        "None and Some(50) should produce same results"
    );
}

#[test]
fn canonical_reachability_obeys_depth_contract() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let omitted = tethys
        .get_reachable("process", ReachabilityDirection::Forward, None)
        .expect("omitted depth");
    let zero = tethys
        .get_reachable("process", ReachabilityDirection::Forward, Some(0))
        .expect("zero depth");
    let one = tethys
        .get_reachable("process", ReachabilityDirection::Forward, Some(1))
        .expect("one depth");
    let two = tethys
        .get_reachable("process", ReachabilityDirection::Forward, Some(2))
        .expect("two depth");
    let maximum = tethys
        .get_reachable(
            "process",
            ReachabilityDirection::Forward,
            Some(u32::MAX as usize),
        )
        .expect("u32 max depth");

    assert_eq!(omitted.max_depth, 50);
    assert_eq!(zero.max_depth, 0);
    assert!(zero.is_empty());
    assert_eq!(one.max_depth, 1);
    assert!(one.reachable.iter().all(|entry| entry.depth == 1));
    assert_eq!(two.max_depth, 2);
    assert!(two.reachable.iter().all(|entry| entry.depth <= 2));
    assert!(two.reachable_count() >= one.reachable_count());
    assert_eq!(maximum.max_depth, u32::MAX as usize);
    assert_eq!(maximum.reachable_count(), omitted.reachable_count());

    let error = tethys
        .get_reachable("NoSuchSymbol", ReachabilityDirection::Forward, Some(0))
        .expect_err("depth zero must validate the source");
    assert!(matches!(error, Error::NotFound(message) if message == "symbol: NoSuchSymbol"));
}

#[cfg(target_pointer_width = "64")]
#[tracing_test::traced_test]
#[test]
fn canonical_reachability_saturates_oversized_depth() {
    let (_dir, mut tethys) = workspace_with_intra_file_calls();
    tethys.index().expect("index failed");

    let result = tethys
        .get_reachable(
            "process",
            ReachabilityDirection::Forward,
            Some(u32::MAX as usize + 1),
        )
        .expect("oversized depth");

    assert_eq!(result.max_depth, u32::MAX as usize);
    assert!(logs_contain(
        "max_depth exceeds u32::MAX; saturating to u32::MAX"
    ));
}

fn workspace_with_reachability_routes() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"reachability_routes\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn source() { alpha(); beta(); long_1(); }\n\
         pub fn alpha() { target(); }\n\
         pub fn beta() { target(); }\n\
         pub fn long_1() { long_2(); }\n\
         pub fn long_2() { target(); }\n\
         pub fn target() {}\n",
    )
    .expect("write lib.rs");
    let tethys = Tethys::new(dir.path()).expect("create Tethys");
    (dir, tethys)
}

#[test]
fn canonical_reachability_paths_are_shortest_unique_and_valid() {
    let (_dir, mut tethys) = workspace_with_reachability_routes();
    tethys.index().expect("index failed");
    let edges = std::collections::HashSet::from([
        ("source", "alpha"),
        ("source", "beta"),
        ("source", "long_1"),
        ("alpha", "target"),
        ("beta", "target"),
        ("long_1", "long_2"),
        ("long_2", "target"),
    ]);

    for (direction, start) in [
        (ReachabilityDirection::Forward, "source"),
        (ReachabilityDirection::Backward, "target"),
    ] {
        let result = tethys
            .get_reachable(start, direction, Some(4))
            .expect("canonical reachability");
        let mut target_ids = std::collections::HashSet::new();
        for entry in &result.reachable {
            assert!(target_ids.insert(entry.target.id), "target must be unique");
            assert_eq!(entry.path.len(), entry.depth);
            assert!(
                entry
                    .path
                    .iter()
                    .all(|symbol| symbol.qualified_name != start)
            );
            assert_eq!(
                entry.path.last().map(|symbol| symbol.id),
                Some(entry.target.id)
            );

            let mut previous = start;
            for symbol in &entry.path {
                let edge = match direction {
                    ReachabilityDirection::Forward => (previous, symbol.qualified_name.as_str()),
                    ReachabilityDirection::Backward => (symbol.qualified_name.as_str(), previous),
                };
                assert!(edges.contains(&edge), "invalid {direction:?} edge {edge:?}");
                previous = &symbol.qualified_name;
            }
        }
    }

    let forward = tethys
        .get_reachable("source", ReachabilityDirection::Forward, Some(4))
        .expect("forward reachability");
    let target = forward
        .reachable
        .iter()
        .find(|entry| entry.target.qualified_name == "target")
        .expect("target reachable");
    assert_eq!(
        target
            .path
            .iter()
            .map(|symbol| symbol.qualified_name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "target"]
    );

    let backward = tethys
        .get_reachable("target", ReachabilityDirection::Backward, Some(4))
        .expect("backward reachability");
    let source = backward
        .reachable
        .iter()
        .find(|entry| entry.target.qualified_name == "source")
        .expect("source reaches target");
    assert_eq!(
        source
            .path
            .iter()
            .map(|symbol| symbol.qualified_name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "source"]
    );
}

#[test]
fn canonical_reachability_preserves_bfs_discovery_order() {
    let (_dir, mut tethys) = workspace_with_reachability_routes();
    tethys.index().expect("index failed");

    let result = tethys
        .get_reachable("source", ReachabilityDirection::Forward, Some(4))
        .expect("forward reachability");
    let observed = result
        .reachable
        .iter()
        .map(|entry| entry.target.qualified_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        vec!["alpha", "beta", "long_1", "target", "long_2"]
    );

    let mut globally_sorted = result
        .reachable
        .iter()
        .map(|entry| (entry.depth, entry.target.qualified_name.as_str()))
        .collect::<Vec<_>>();
    globally_sorted.sort_unstable();
    assert_ne!(
        observed,
        globally_sorted
            .iter()
            .map(|(_, name)| *name)
            .collect::<Vec<_>>(),
        "fixture must distinguish queue discovery from global sorting"
    );

    let connection = Connection::open(tethys.db_path()).expect("open index");
    connection
        .execute(
            "UPDATE symbols SET qualified_name = 'aaa_same'
             WHERE name IN ('alpha', 'beta')",
            [],
        )
        .expect("create qualified-name tie");
    let tied_ids = {
        let mut statement = connection
            .prepare(
                "SELECT id FROM symbols
                 WHERE qualified_name = 'aaa_same'
                 ORDER BY id",
            )
            .expect("prepare tied ids");
        statement
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("query tied ids")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect tied ids")
    };
    drop(connection);

    let tied = tethys
        .get_reachable("source", ReachabilityDirection::Forward, Some(1))
        .expect("tied reachability");
    assert_eq!(
        tied.reachable
            .iter()
            .take(2)
            .map(|entry| entry.target.id.as_i64())
            .collect::<Vec<_>>(),
        tied_ids
    );
}

#[test]
fn canonical_reachability_preserves_is_test() {
    let (_dir, mut tethys) = workspace_with_reachability_routes();
    tethys.index().expect("index failed");
    let connection = Connection::open(tethys.db_path()).expect("open index");
    connection
        .execute_batch(
            "UPDATE symbols SET is_test = 0 WHERE name = 'alpha';
             UPDATE symbols SET is_test = 1 WHERE name = 'beta';
             UPDATE call_edges SET call_count = 5
             WHERE caller_symbol_id = (SELECT id FROM symbols WHERE name = 'source')
               AND callee_symbol_id = (SELECT id FROM symbols WHERE name = 'alpha');
             UPDATE call_edges SET call_count = 0
             WHERE caller_symbol_id = (SELECT id FROM symbols WHERE name = 'source')
               AND callee_symbol_id = (SELECT id FROM symbols WHERE name = 'beta');
             UPDATE call_edges SET call_count = 5
             WHERE caller_symbol_id = (SELECT id FROM symbols WHERE name = 'alpha')
               AND callee_symbol_id = (SELECT id FROM symbols WHERE name = 'target');
             UPDATE call_edges SET call_count = 0
             WHERE caller_symbol_id = (SELECT id FROM symbols WHERE name = 'beta')
               AND callee_symbol_id = (SELECT id FROM symbols WHERE name = 'target');",
        )
        .expect("seed projection trap");
    let raw_flags = {
        let mut statement = connection
            .prepare(
                "SELECT qualified_name, is_test FROM symbols
                 WHERE name IN ('alpha', 'beta')",
            )
            .expect("prepare symbol flags");
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .expect("query symbol flags")
            .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()
            .expect("collect symbol flags")
    };
    drop(connection);

    for (start, direction) in [
        ("source", ReachabilityDirection::Forward),
        ("target", ReachabilityDirection::Backward),
    ] {
        let result = tethys
            .get_reachable(start, direction, Some(1))
            .expect("canonical reachability");
        let projected = result
            .reachable
            .iter()
            .filter(|entry| matches!(entry.target.qualified_name.as_str(), "alpha" | "beta"))
            .map(|entry| (entry.target.qualified_name.as_str(), entry.target.is_test))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(projected.get("alpha"), Some(&raw_flags["alpha"]));
        assert_eq!(projected.get("beta"), Some(&raw_flags["beta"]));
        assert!(
            !projected["alpha"],
            "call_count=5 must not decode as is_test"
        );
        assert!(projected["beta"], "call_count=0 must not erase is_test");
    }
}

#[test]
fn canonical_reachability_preserves_source_and_dangling_posture() {
    let (_dir, mut tethys) = workspace_with_reachability_routes();
    tethys.index().expect("index failed");
    let connection = Connection::open(tethys.db_path()).expect("open index");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable foreign keys");
    let source_id = connection
        .query_row(
            "SELECT id FROM symbols WHERE qualified_name = 'source' ORDER BY id LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("source id");
    connection
        .execute(
            "INSERT INTO symbols (
                file_id, name, module_path, qualified_name, kind, line, column,
                end_line, end_column, signature, visibility, parent_symbol_id, is_test
             )
             SELECT
                file_id, name, module_path, qualified_name, kind, line, column,
                end_line, end_column, signature, visibility, parent_symbol_id, is_test
             FROM symbols WHERE id = ?1",
            [source_id],
        )
        .expect("insert duplicate source");
    connection
        .execute(
            "INSERT INTO call_edges (caller_symbol_id, callee_symbol_id, call_count)
             VALUES (?1, 999999, 1)",
            [source_id],
        )
        .expect("insert dangling edge");
    let expected_direct = {
        let mut statement = connection
            .prepare(
                "SELECT s.id
                 FROM call_edges ce
                 JOIN symbols s ON s.id = ce.callee_symbol_id
                 WHERE ce.caller_symbol_id = ?1
                 ORDER BY s.qualified_name, s.id",
            )
            .expect("prepare inner-join oracle");
        statement
            .query_map([source_id], |row| row.get::<_, i64>(0))
            .expect("query inner-join oracle")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect inner-join oracle")
    };
    drop(connection);

    let result = tethys
        .get_reachable("source", ReachabilityDirection::Forward, Some(1))
        .expect("dangling edge must be omitted");
    assert_eq!(result.source.id.as_i64(), source_id);
    assert_eq!(
        result
            .reachable
            .iter()
            .map(|entry| entry.target.id.as_i64())
            .collect::<Vec<_>>(),
        expected_direct
    );

    let connection = Connection::open(tethys.db_path()).expect("reopen index");
    connection
        .execute("UPDATE symbols SET is_test = X'01' WHERE name = 'beta'", [])
        .expect("corrupt symbol projection");
    drop(connection);
    let error = tethys
        .get_reachable("source", ReachabilityDirection::Forward, Some(1))
        .expect_err("corrupt row must fail");
    assert!(matches!(
        error,
        Error::Database(rusqlite::Error::InvalidColumnType(..))
    ));
}

fn workspace_with_strongly_connected_calls() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"strong_calls\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn s() { s(); a(); }\n\
         pub fn a() { b(); }\n\
         pub fn b() { b(); c(); }\n\
         pub fn c() { d(); }\n\
         pub fn d() { s(); }\n",
    )
    .expect("write lib.rs");
    let tethys = Tethys::new(dir.path()).expect("create Tethys");
    (dir, tethys)
}

#[test]
fn canonical_reachability_excludes_source_in_cycles() {
    let (_dir, mut tethys) = workspace_with_strongly_connected_calls();
    tethys.index().expect("index failed");
    let expected = std::collections::HashSet::from(["a", "b", "c", "d"]);

    for direction in [
        ReachabilityDirection::Forward,
        ReachabilityDirection::Backward,
    ] {
        for depth in [None, Some(100)] {
            let result = tethys
                .get_reachable("s", direction, depth)
                .expect("cyclic reachability");
            let names = result
                .reachable
                .iter()
                .map(|entry| entry.target.qualified_name.as_str())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(names, expected, "{direction:?} at depth {depth:?}");
            assert_eq!(result.reachable.len(), 4);
            assert!(
                result
                    .reachable
                    .iter()
                    .all(|entry| entry.target.qualified_name != "s")
            );
        }
    }
}

/// Helper that creates a workspace with a cyclic call pattern: a -> b -> c -> a
fn workspace_with_cyclic_calls() -> (TempDir, Tethys) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test_cyclic\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");

    fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");

    fs::write(
        dir.path().join("src/lib.rs"),
        r"
pub fn cycle_a() {
    cycle_b();
}

pub fn cycle_b() {
    cycle_c();
}

pub fn cycle_c() {
    cycle_a();  // Creates the cycle back to a
}

pub fn entry_point() {
    cycle_a();
}
",
    )
    .expect("failed to write lib.rs");

    let tethys = Tethys::new(dir.path()).expect("failed to create Tethys");
    (dir, tethys)
}

#[test]
fn reachability_terminates_on_cyclic_call_graph() {
    let (_dir, mut tethys) = workspace_with_cyclic_calls();
    tethys.index().expect("index failed");

    // Forward reachability from cycle_a should find cycle_b and cycle_c
    // but should terminate (not infinite loop) due to visited tracking
    let result = tethys
        .get_forward_reachable("cycle_a", Some(10))
        .expect("get_forward_reachable should terminate on cyclic graph");

    // Should find b and c, but not revisit a (already visited as source)
    // The exact count depends on what gets resolved, but it should terminate
    assert!(
        result.reachable_count() <= 10,
        "BFS should terminate and not produce infinite results, got: {}",
        result.reachable_count()
    );

    // Backward reachability should also terminate
    let result_backward = tethys
        .get_backward_reachable("cycle_a", Some(10))
        .expect("get_backward_reachable should terminate on cyclic graph");

    // cycle_a is called by cycle_c and entry_point
    // cycle_c is called by cycle_b, which is called by cycle_a (but a is source, so skipped)
    assert!(
        result_backward.reachable_count() <= 10,
        "backward BFS should terminate, got: {}",
        result_backward.reachable_count()
    );
}
