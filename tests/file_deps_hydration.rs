//! Integration fences for direct file dependency/dependent hydration
//! (tethys-n8pu): the set-oriented rewrite must preserve directed edge
//! behavior, workspace-relative deduped paths, `NotFound` for missing
//! roots, and the dangling-row warn/skip defense — verified through the
//! full `Tethys` pipeline on a real temp workspace, against a direct-SQL
//! oracle that computes the same answer through an independent mechanism.
//!
//! Pairs with the unit fences in `db::file_deps::hydration_fence_tests`,
//! which count SQL statements via the live connection's trace hook —
//! reachable only from inside the crate.

use std::path::Path;

use tempfile::TempDir;
use tethys::{Error, Tethys};
use tracing_test::traced_test;

mod common;

use common::{open_db, workspace_with_files};

/// Fixture workspace. Edges are L2 (used imports only — bare `mod`
/// declarations contribute no `file_deps` edges): `lib.rs` re-exports
/// a/b/c, `a.rs` uses `C`, `b.rs` is a leaf. So `lib.rs` has 3 deps,
/// `a.rs` has 1, `b.rs` 0; `c.rs` has 2 dependents.
fn indexed_fixture() -> (TempDir, Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "mod a;\nmod b;\nmod c;\npub use crate::a::A;\npub use crate::b::B;\npub use crate::c::C;\n",
        ),
        (
            "src/a.rs",
            "use crate::c::C;\npub struct A {\n    pub c: C,\n}\n",
        ),
        ("src/b.rs", "pub struct B;\n"),
        ("src/c.rs", "pub struct C;\n"),
    ]);
    tethys.index().expect("indexing fixture should succeed");
    (dir, tethys)
}

fn sorted_strings(paths: &[std::path::PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn oracle_dep_paths(db: &rusqlite::Connection, from: &str) -> Vec<String> {
    let mut stmt = db
        .prepare(
            "SELECT f2.path FROM file_deps fd\n\
             JOIN files f2 ON f2.id = fd.to_file_id\n\
             WHERE fd.from_file_id = (SELECT id FROM files WHERE path = ?1)\n\
             ORDER BY f2.path",
        )
        .expect("prepare oracle deps query");
    stmt.query_map([from], |r| r.get::<_, String>(0))
        .expect("run oracle deps query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect oracle deps rows")
}

fn oracle_dependent_paths(db: &rusqlite::Connection, to: &str) -> Vec<String> {
    let mut stmt = db
        .prepare(
            "SELECT f1.path FROM file_deps fd\n\
             JOIN files f1 ON f1.id = fd.from_file_id\n\
             WHERE fd.to_file_id = (SELECT id FROM files WHERE path = ?1)\n\
             ORDER BY f1.path",
        )
        .expect("prepare oracle dependents query");
    stmt.query_map([to], |r| r.get::<_, String>(0))
        .expect("run oracle dependents query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect oracle dependents rows")
}

/// Both directions return the expected workspace-relative sets and agree
/// with the independent direct-SQL oracle; a zero-dep root returns an
/// empty vec rather than erroring.
#[test]
fn hydrated_paths_agree_with_direct_sql_oracle() {
    let (_dir, tethys) = indexed_fixture();
    let db = open_db(&tethys);

    let deps = tethys
        .get_dependencies(Path::new("src/lib.rs"))
        .expect("deps of lib.rs");
    let dep_paths = sorted_strings(&deps);
    assert_eq!(
        dep_paths,
        vec!["src/a.rs", "src/b.rs", "src/c.rs"],
        "deps of lib.rs"
    );
    assert_eq!(
        dep_paths,
        oracle_dep_paths(&db, "src/lib.rs"),
        "deps must equal the direct-SQL oracle"
    );
    assert!(
        deps.iter().all(|p| p.is_relative()),
        "dep paths must be workspace-relative"
    );

    let dependents = tethys
        .get_dependents(Path::new("src/c.rs"))
        .expect("dependents of c.rs");
    let dependent_paths = sorted_strings(&dependents);
    assert_eq!(
        dependent_paths,
        vec!["src/a.rs", "src/lib.rs"],
        "dependents of c.rs"
    );
    assert_eq!(
        dependent_paths,
        oracle_dependent_paths(&db, "src/c.rs"),
        "dependents must equal the direct-SQL oracle"
    );
    assert!(
        dependents.iter().all(|p| p.is_relative()),
        "dependent paths must be workspace-relative"
    );

    let empty = tethys
        .get_dependencies(Path::new("src/b.rs"))
        .expect("deps of leaf");
    assert!(empty.is_empty(), "zero-dep root must return an empty vec");
}

/// A root that was never indexed is `NotFound` in both directions.
#[test]
fn missing_root_is_not_found_in_both_directions() {
    let (_dir, tethys) = indexed_fixture();

    let missing_deps = tethys.get_dependencies(Path::new("src/nope.rs"));
    assert!(
        matches!(&missing_deps, Err(Error::NotFound(_))),
        "missing root must be NotFound for dependencies, got {missing_deps:?}"
    );

    let missing_dependents = tethys.get_dependents(Path::new("src/nope.rs"));
    assert!(
        matches!(&missing_dependents, Err(Error::NotFound(_))),
        "missing root must be NotFound for dependents, got {missing_dependents:?}"
    );
}

/// A dangling `file_deps` row (target file absent — possible only in a
/// hand-edited database, since the index connection enforces foreign keys
/// and file deletes cascade the edges) is skipped with the established
/// per-row warn; valid rows are still returned and the call does not
/// error.
#[traced_test]
#[test]
fn dangling_dep_row_is_warned_and_skipped() {
    let (_dir, tethys) = indexed_fixture();

    // Inject a dangling edge from lib.rs, bypassing FK enforcement via a
    // second connection (the index's own connection enforces FKs).
    {
        let conn = rusqlite::Connection::open(tethys.db_path()).expect("open injection connection");
        conn.pragma_update(None, "foreign_keys", "OFF")
            .expect("disable FKs");
        let lib_id: i64 = conn
            .query_row("SELECT id FROM files WHERE path = 'src/lib.rs'", [], |r| {
                r.get(0)
            })
            .expect("lib.rs file id");
        conn.execute(
            "INSERT INTO file_deps (from_file_id, to_file_id, ref_count)
             VALUES (?1, 99999, 1)",
            rusqlite::params![lib_id],
        )
        .expect("inject dangling edge");
    }

    let deps = tethys
        .get_dependencies(Path::new("src/lib.rs"))
        .expect("dangling row must not error the call");
    assert_eq!(
        sorted_strings(&deps),
        vec!["src/a.rs", "src/b.rs", "src/c.rs"],
        "valid rows must still be returned; dangling row skipped"
    );
    assert!(
        logs_contain("99999"),
        "per-row warn must name the dangling id"
    );
    assert!(
        logs_contain("corruption"),
        "warn must keep the established message shape"
    );
    assert!(
        logs_contain("could not be resolved"),
        "Tethys summary debug must report the missing count"
    );
}
