//! Regression tests for orphan files: rows for files deleted from disk that
//! survive in the DB across re-index runs.
//!
//! Pre-fix, `index_with_options` had no orphan-cleanup pass: the `files`-table
//! DELETE logic only fired when an existing file was re-indexed, so a file
//! deleted from disk kept its `files`/`symbols`/`refs`/`imports` rows forever.
//! Streaming mode (`IndexOptions::with_streaming()`) then computed file-level
//! dependencies from STORED data for every file in the DB — including orphans
//! — re-inserting `file_deps` rows with the orphan as `from_file_id`.
//! Downstream queries (coupling, callers, cycles, impact) saw the orphan as a
//! real source of cross-file edges.
//!
//! Post-fix, an orphan-cleanup pass purges deleted-from-disk file rows before
//! any dependency computation, and FK cascades remove the dependent rows.
//!
//! Both batch (`IndexOptions::default()`) and streaming (`with_streaming()`)
//! modes are exercised: pre-fix only streaming re-inserted orphan `file_deps`,
//! but the orphan rows themselves lingered in both modes.

use rstest::rstest;
use std::fs;
use tempfile::TempDir;
use tethys::{IndexOptions, Tethys};

/// Workspace-relative path of the file the tests delete from disk.
const ORPHAN_PATH: &str = "crate_caller/src/lib.rs";

/// Create a 2-crate workspace with a cross-crate dependency that keeps an
/// UNRESOLVED reference in the DB.
///
/// `crate_caller` imports `crate_target::Widget` (resolves in Pass 2) and
/// `crate_target::generated_helper` — a function produced by a
/// `macro_rules!` expansion, which tree-sitter extraction never sees as a
/// symbol. The `generated_helper()` call therefore stays unresolved with its
/// `reference_name` intact across runs. That surviving name is what
/// corroborates the stored import when streaming mode recomputes dependencies
/// from stored data, so the orphan re-inserts a `file_deps` edge pre-fix.
/// (Resolved refs can't reproduce this: resolution nulls `reference_name`,
/// and rewriting the target file cascade-deletes refs bound to its symbols.)
fn build_workspace_with_unresolved_cross_crate_ref(dir: &TempDir) {
    let files: [(&str, &str); 5] = [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crate_caller\", \"crate_target\"]\nresolver = \"2\"\n",
        ),
        (
            "crate_caller/Cargo.toml",
            "[package]\nname = \"crate_caller\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             [dependencies]\ncrate_target = { path = \"../crate_target\" }\n",
        ),
        (
            "crate_caller/src/lib.rs",
            "use crate_target::Widget;\n\
             use crate_target::generated_helper;\n\n\
             pub fn make_and_call() {\n\
                 let _w = Widget;\n\
                 generated_helper();\n\
             }\n",
        ),
        (
            "crate_target/Cargo.toml",
            "[package]\nname = \"crate_target\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "crate_target/src/lib.rs",
            "pub struct Widget;\n\n\
             macro_rules! gen_helper {\n\
                 () => {\n\
                     pub fn generated_helper() {}\n\
                 };\n\
             }\n\
             gen_helper!();\n",
        ),
    ];
    for (rel, content) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().expect("relative path has parent"))
            .expect("create parent");
        fs::write(&full, content).expect("write file");
    }
}

fn open_readonly(db_path: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open tethys.db read-only")
}

/// Look up the `files.id` for a workspace-relative path, if the row exists.
fn file_id_for(conn: &rusqlite::Connection, path: &str) -> Option<i64> {
    conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
        row.get(0)
    })
    .map_or_else(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => None,
            other => panic!("files lookup failed: {other}"),
        },
        Some,
    )
}

fn count(conn: &rusqlite::Connection, sql: &str, id: i64) -> i64 {
    conn.query_row(sql, [id], |row| row.get(0))
        .expect("count query")
}

/// Deleting a source file from disk and re-indexing (non-rebuild) must not
/// leave `file_deps` rows originating from the deleted file.
///
/// Pre-fix this failed in streaming mode: `compute_all_dependencies` iterated
/// every file in the DB, loaded the orphan's stale stored imports + refs, and
/// re-inserted the orphan's outgoing edge — a phantom contribution to
/// coupling (Ce), cycles, and impact analysis.
#[rstest]
#[case::batch(IndexOptions::default)]
#[case::streaming(IndexOptions::with_streaming)]
fn reindex_after_disk_delete_leaves_no_file_deps_from_orphan(
    #[case] options_factory: fn() -> IndexOptions,
) {
    let dir = tempfile::tempdir().expect("create tempdir");
    build_workspace_with_unresolved_cross_crate_ref(&dir);
    let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");

    tethys
        .index_with_options(options_factory())
        .expect("first index");
    let db_path = tethys.db_path().to_path_buf();

    let orphan_id = {
        let conn = open_readonly(&db_path);
        let orphan_id = file_id_for(&conn, ORPHAN_PATH).expect("caller file indexed by run 1");
        // Vacuousness guard: run 1 must record the caller's outgoing edge,
        // otherwise the post-delete assertion below can't catch anything.
        let outgoing = count(
            &conn,
            "SELECT COUNT(*) FROM file_deps WHERE from_file_id = ?1",
            orphan_id,
        );
        assert!(
            outgoing >= 1,
            "fixture must produce >=1 outgoing file_deps edge from {ORPHAN_PATH} \
             after the first index; got {outgoing}"
        );
        orphan_id
    };

    fs::remove_file(dir.path().join(ORPHAN_PATH)).expect("delete caller source from disk");

    tethys
        .index_with_options(options_factory())
        .expect("second index");

    let conn = open_readonly(&db_path);
    let phantom_edges = count(
        &conn,
        "SELECT COUNT(*) FROM file_deps WHERE from_file_id = ?1",
        orphan_id,
    );
    assert_eq!(
        phantom_edges, 0,
        "no file_deps row may originate from a file deleted from disk; \
         found {phantom_edges} edge(s) from orphan {ORPHAN_PATH} after re-index"
    );
}

/// Re-indexing after a disk delete must purge the orphan's `files` row, and
/// FK cascades must remove its dependent `symbols`, `refs`, and `imports`
/// rows — so no downstream query can see the orphan at all.
#[rstest]
#[case::batch(IndexOptions::default)]
#[case::streaming(IndexOptions::with_streaming)]
fn reindex_after_disk_delete_purges_orphan_rows(#[case] options_factory: fn() -> IndexOptions) {
    let dir = tempfile::tempdir().expect("create tempdir");
    build_workspace_with_unresolved_cross_crate_ref(&dir);
    let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");

    tethys
        .index_with_options(options_factory())
        .expect("first index");
    let db_path = tethys.db_path().to_path_buf();

    let orphan_id = {
        let conn = open_readonly(&db_path);
        let orphan_id = file_id_for(&conn, ORPHAN_PATH).expect("caller file indexed by run 1");
        // Vacuousness guard: the orphan must actually have dependent rows.
        for (table, sql) in [
            ("symbols", "SELECT COUNT(*) FROM symbols WHERE file_id = ?1"),
            ("imports", "SELECT COUNT(*) FROM imports WHERE file_id = ?1"),
        ] {
            let rows = count(&conn, sql, orphan_id);
            assert!(
                rows >= 1,
                "fixture must produce >=1 {table} row for {ORPHAN_PATH} after run 1; got {rows}"
            );
        }
        orphan_id
    };

    fs::remove_file(dir.path().join(ORPHAN_PATH)).expect("delete caller source from disk");

    tethys
        .index_with_options(options_factory())
        .expect("second index");

    let conn = open_readonly(&db_path);
    assert_eq!(
        file_id_for(&conn, ORPHAN_PATH),
        None,
        "the files row for a deleted-from-disk file must be purged on re-index"
    );
    for (table, sql) in [
        ("symbols", "SELECT COUNT(*) FROM symbols WHERE file_id = ?1"),
        ("refs", "SELECT COUNT(*) FROM refs WHERE file_id = ?1"),
        ("imports", "SELECT COUNT(*) FROM imports WHERE file_id = ?1"),
    ] {
        let leftover = count(&conn, sql, orphan_id);
        assert_eq!(
            leftover, 0,
            "orphan purge must cascade to {table}; found {leftover} row(s) \
             still referencing the deleted file"
        );
    }
}

/// A file that still exists on disk must never be purged as an orphan —
/// re-indexing an unchanged workspace keeps every file row.
///
/// Guards against an over-eager cleanup (e.g. purging files the directory
/// walk skipped, or comparing paths in mismatched forms).
#[rstest]
#[case::batch(IndexOptions::default)]
#[case::streaming(IndexOptions::with_streaming)]
fn reindex_without_deletion_keeps_all_file_rows(#[case] options_factory: fn() -> IndexOptions) {
    let dir = tempfile::tempdir().expect("create tempdir");
    build_workspace_with_unresolved_cross_crate_ref(&dir);
    let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");

    tethys
        .index_with_options(options_factory())
        .expect("first index");
    let db_path = tethys.db_path().to_path_buf();
    let before: i64 = open_readonly(&db_path)
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .expect("count files");

    tethys
        .index_with_options(options_factory())
        .expect("second index");

    let after: i64 = open_readonly(&db_path)
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .expect("count files");
    assert_eq!(
        before, after,
        "re-indexing an unchanged workspace must not purge live file rows"
    );
}
