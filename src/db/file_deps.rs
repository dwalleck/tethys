//! File dependency CRUD operations for the Tethys index.

use std::path::PathBuf;

use rusqlite::params;
use tracing::{trace, warn};

use super::Index;
use crate::error::Result;
use crate::types::FileId;

impl Index {
    /// Clear all file-level dependencies.
    ///
    /// Call before re-indexing to prevent stale edges from prior runs
    /// accumulating via the `ON CONFLICT … DO UPDATE` in
    /// `insert_file_dependency`.
    ///
    /// Mirrors [`Index::clear_all_call_edges`] in shape, but positioned
    /// differently in `index_with_options`: this clear runs *before*
    /// per-file processing because `insert_file_dependency` is invoked
    /// during the parse loop. `clear_all_call_edges` runs *after* all
    /// resolution passes because `populate_call_edges` derives edges
    /// from the post-resolution `refs` table.
    pub fn clear_all_file_deps(&self) -> Result<()> {
        trace!("Clearing all file deps");
        let conn = self.connection()?;

        conn.execute("DELETE FROM file_deps", [])?;
        Ok(())
    }

    /// Insert or update a file-level dependency.
    ///
    /// Records that `from_file_id` depends on `to_file_id`.
    pub fn insert_file_dependency(&self, from_file_id: FileId, to_file_id: FileId) -> Result<()> {
        let conn = self.connection()?;

        // Use upsert (ON CONFLICT) to handle duplicates (increments ref_count)
        conn.execute(
            "INSERT INTO file_deps (from_file_id, to_file_id, ref_count)
             VALUES (?1, ?2, 1)
             ON CONFLICT(from_file_id, to_file_id) DO UPDATE SET ref_count = ref_count + 1",
            params![from_file_id.as_i64(), to_file_id.as_i64()],
        )?;
        Ok(())
    }

    /// Get workspace-relative paths of the files `file_id` directly depends
    /// on.
    ///
    /// One set-oriented LEFT JOIN over `file_deps` × `files` (tethys-n8pu):
    /// hydration no longer performs one indexed-file lookup per returned
    /// dependency. Returns `(paths, missing_count)` where `missing_count`
    /// counts `file_deps` rows whose target file row is absent — possible
    /// only in a hand-edited database, since `Index::open` enforces foreign
    /// keys and `files` deletes cascade `file_deps` rows.
    pub fn get_file_dependency_paths(&self, file_id: FileId) -> Result<(Vec<PathBuf>, usize)> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(
            "SELECT fd.to_file_id, f.path
             FROM file_deps fd
             LEFT JOIN files f ON f.id = fd.to_file_id
             WHERE fd.from_file_id = ?1",
        )?;

        let rows = stmt.query_map([file_id.as_i64()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut paths = Vec::new();
        let mut missing_count = 0;
        for row in rows {
            let (dep_id, path) = row?;
            if let Some(path) = path {
                paths.push(PathBuf::from(path));
            } else {
                warn!(
                    source_file_id = %file_id,
                    missing_file_id = dep_id,
                    "file_deps references non-existent file, possible database corruption"
                );
                missing_count += 1;
            }
        }
        Ok((paths, missing_count))
    }

    /// Get workspace-relative paths of the files directly depending on
    /// `file_id`.
    ///
    /// Mirror of [`Self::get_file_dependency_paths`] over the reverse edge
    /// direction; same set-oriented hydration and same `missing_count`
    /// semantics for dangling `file_deps` rows.
    pub fn get_file_dependent_paths(&self, file_id: FileId) -> Result<(Vec<PathBuf>, usize)> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(
            "SELECT fd.from_file_id, f.path
             FROM file_deps fd
             LEFT JOIN files f ON f.id = fd.from_file_id
             WHERE fd.to_file_id = ?1",
        )?;

        let rows = stmt.query_map([file_id.as_i64()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut paths = Vec::new();
        let mut missing_count = 0;
        for row in rows {
            let (dep_id, path) = row?;
            if let Some(path) = path {
                paths.push(PathBuf::from(path));
            } else {
                warn!(
                    source_file_id = %file_id,
                    missing_file_id = dep_id,
                    "file_deps references non-existent file, possible database corruption"
                );
                missing_count += 1;
            }
        }
        Ok((paths, missing_count))
    }
}

#[cfg(test)]
mod n8pu_probe {
    //! tethys-n8pu prove-it-prototype: direct dependency/dependent hydration.
    //!
    //! Probe: Tethys API output on a real temporary index + SQL statement
    //! counts from a rusqlite trace hook on the live connection.
    //! Oracle: direct JOIN SQL against the same db (independent mechanism)
    //! + code inspection of `file_ids_to_paths` (one `get_file_by_id` per id).

    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::Tethys;

    static TRACE_TOTAL: AtomicUsize = AtomicUsize::new(0);
    static TRACE_PER_ID: AtomicUsize = AtomicUsize::new(0);

    fn trace_cb(sql: &str) {
        TRACE_TOTAL.fetch_add(1, Ordering::Relaxed);
        if sql.contains("FROM files WHERE id = ") {
            TRACE_PER_ID.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reset_counts() {
        TRACE_TOTAL.store(0, Ordering::Relaxed);
        TRACE_PER_ID.store(0, Ordering::Relaxed);
    }

    fn counts() -> (usize, usize) {
        (
            TRACE_TOTAL.load(Ordering::Relaxed),
            TRACE_PER_ID.load(Ordering::Relaxed),
        )
    }

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    /// Real temp workspace: lib.rs depends on a.rs and b.rs; a.rs depends on
    /// c.rs. Both `mod` declarations and `use` paths contribute `file_deps`
    /// edges, so lib.rs -> a.rs is a double-contribution pair (PK-deduped).
    fn build_fixture(dir: &std::path::Path) {
        write(
            dir,
            "Cargo.toml",
            "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        );
        write(
            dir,
            "src/lib.rs",
            "mod a;\nmod b;\nmod c;\npub use crate::a::A;\npub use crate::b::B;\npub use crate::c::C;\n",
        );
        write(
            dir,
            "src/a.rs",
            "use crate::c::C;\npub struct A {\n    pub c: C,\n}\n",
        );
        write(dir, "src/b.rs", "pub struct B;\n");
        write(dir, "src/c.rs", "pub struct C;\n");
    }

    fn oracle_dep_paths(db: &rusqlite::Connection, from: &str) -> Vec<String> {
        let mut stmt = db
            .prepare(
                "SELECT f2.path FROM file_deps fd\n\
                 JOIN files f2 ON f2.id = fd.to_file_id\n\
                 WHERE fd.from_file_id = (SELECT id FROM files WHERE path = ?1)\n\
                 ORDER BY f2.path",
            )
            .unwrap();
        stmt.query_map([from], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    fn oracle_dependent_paths(db: &rusqlite::Connection, to: &str) -> Vec<String> {
        let mut stmt = db
            .prepare(
                "SELECT f1.path FROM file_deps fd\n\
                 JOIN files f1 ON f1.id = fd.from_file_id\n\
                 WHERE fd.to_file_id = (SELECT id FROM files WHERE path = ?1)\n\
                 ORDER BY f1.path",
            )
            .unwrap();
        stmt.query_map([to], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    /// Diagnostic dump of the raw `files` and `file_deps` tables — the
    /// probe's ground truth for set comparison.
    fn raw_tables(db: &rusqlite::Connection) {
        let files: Vec<(i64, String)> = {
            let mut stmt = db
                .prepare("SELECT id, path FROM files ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        };
        println!("RAW files = {files:?}");
        let edges: Vec<(i64, i64)> = {
            let mut stmt = db
                .prepare("SELECT from_file_id, to_file_id FROM file_deps ORDER BY 1, 2")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        };
        println!("RAW file_deps = {edges:?}");
    }

    #[test]
    fn probe_direct_dep_hydration() {
        let dir = tempfile::tempdir().unwrap();
        build_fixture(dir.path());
        let mut tethys = Tethys::new(dir.path()).unwrap();
        tethys.index().unwrap();
        let db = rusqlite::Connection::open(tethys.db_path()).unwrap();

        // Install the statement trace on the LIVE index connection.
        {
            let mut conn = tethys.db.connection().unwrap();
            conn.trace(Some(trace_cb));
        }

        reset_counts();
        let deps = tethys.get_dependencies(Path::new("src/lib.rs")).unwrap();
        let (total_deps, per_id_deps) = counts();
        reset_counts();
        let dependents = tethys.get_dependents(Path::new("src/c.rs")).unwrap();
        let (total_deps_rev, per_id_deps_rev) = counts();
        reset_counts();
        let missing = tethys.get_dependencies(Path::new("src/nope.rs"));
        let (total_missing, per_id_missing) = counts();
        reset_counts();
        let empty = tethys.get_dependencies(Path::new("src/b.rs")).unwrap();
        let (total_empty, per_id_empty) = counts();

        let mut dep_paths: Vec<String> = deps
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        dep_paths.sort();
        let mut dependent_paths: Vec<String> = dependents
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        dependent_paths.sort();

        println!("PROBE deps(lib.rs)   = {dep_paths:?}");
        println!("PROBE dependents(c.rs) = {dependent_paths:?}");
        println!(
            "PROBE missing(root)  = {:?}",
            missing
                .as_ref()
                .map_or_else(|e| format!("{e:?}"), |_| "ok".to_string())
        );
        println!("PROBE stmts deps     = total {total_deps}, per-id-lookup {per_id_deps}");
        println!(
            "PROBE stmts dependents = total {total_deps_rev}, per-id-lookup {per_id_deps_rev}"
        );
        println!("PROBE deps(b.rs)     = {empty:?}");
        println!("PROBE stmts empty    = total {total_empty}, per-id-lookup {per_id_empty}");
        println!("PROBE stmts missing  = total {total_missing}, per-id-lookup {per_id_missing}");
        println!(
            "ORACLE deps(lib.rs)   = {:?}",
            oracle_dep_paths(&db, "src/lib.rs")
        );
        println!(
            "ORACLE dependents(c.rs) = {:?}",
            oracle_dependent_paths(&db, "src/c.rs")
        );

        raw_tables(&db);

        // Behavior: workspace-relative, deduped, correct, NotFound root.
        assert_eq!(
            dep_paths,
            vec!["src/a.rs", "src/b.rs", "src/c.rs"],
            "deps of lib.rs"
        );
        assert_eq!(
            dependent_paths,
            vec!["src/a.rs", "src/lib.rs"],
            "dependents of c.rs"
        );
        assert!(deps.iter().all(|p| p.is_relative()), "workspace-relative");
        assert!(empty.is_empty(), "zero-dep root must return empty vec");
        assert!(missing.is_err(), "missing root must be NotFound");
        assert_eq!(
            (total_missing, per_id_missing),
            (1, 0),
            "missing root: root lookup only, no hydration"
        );
        // C6 (tethys-n8pu): set-oriented hydration — exactly 2 statements
        // (root lookup + one JOIN), zero per-ID lookups, any result count.
        // Pre-fix this was 2 + N statements with N per-ID lookups (measured
        // 5 total / 3 per-ID at N=3 in probe1-output.txt).
        assert_eq!(
            (total_deps, per_id_deps),
            (2, 0),
            "deps hydration must be set-oriented (root lookup + one JOIN)"
        );
        assert_eq!(
            (total_deps_rev, per_id_deps_rev),
            (2, 0),
            "dependents hydration must be set-oriented (root lookup + one JOIN)"
        );
        assert_eq!(
            (total_empty, per_id_empty),
            (2, 0),
            "empty result: root lookup + edge query, no per-ID lookups"
        );
    }
}
