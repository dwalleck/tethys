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
        self.hydrate_paths(
            "SELECT fd.to_file_id, f.path
             FROM file_deps fd
             LEFT JOIN files f ON f.id = fd.to_file_id
             WHERE fd.from_file_id = ?1",
            file_id,
        )
    }

    /// Get workspace-relative paths of the files directly depending on
    /// `file_id`.
    ///
    /// Mirror of [`Self::get_file_dependency_paths`] over the reverse edge
    /// direction; same set-oriented hydration and same `missing_count`
    /// semantics for dangling `file_deps` rows.
    pub fn get_file_dependent_paths(&self, file_id: FileId) -> Result<(Vec<PathBuf>, usize)> {
        self.hydrate_paths(
            "SELECT fd.from_file_id, f.path
             FROM file_deps fd
             LEFT JOIN files f ON f.id = fd.from_file_id
             WHERE fd.to_file_id = ?1",
            file_id,
        )
    }

    /// Run a set-oriented `file_deps` × `files` LEFT JOIN and turn its rows
    /// into `(paths, missing_count)`, warning per dangling row.
    ///
    /// `sql` must select the neighbor file id in column 0 and the joined
    /// `files.path` in column 1, and bind `file_id` as parameter `?1`.
    fn hydrate_paths(&self, sql: &str, file_id: FileId) -> Result<(Vec<PathBuf>, usize)> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(sql)?;

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
mod hydration_fence_tests {
    //! Statement-count fences for the set-oriented hydration (tethys-n8pu).
    //!
    //! Pairs with `tests/file_deps_hydration.rs` the way
    //! `call_edges::k_hybrid_filter_tests` pairs with
    //! `tests/file_deps_corroboration.rs`: this module exercises the
    //! hydration queries against an `Index` with hand-inserted rows, while
    //! the integration file drives the full pipeline through `Tethys` on a
    //! real temp workspace. The statement counter needs the live
    //! connection's `rusqlite` trace hook, which only crate code can reach
    //! — that is why this fence lives here and not in `tests/`.
    //!
    //! Pre-rewrite, hydration issued one `get_file_by_id` lookup per
    //! returned id (N statements for N results); the `LEFT JOIN` does it in
    //! exactly one statement regardless of N. The per-id counter keys on
    //! the `get_file_by_id` SQL shape so a regression to per-id lookups
    //! fails loudly.

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rusqlite::params;
    use tempfile::TempDir;

    use crate::db::Index;
    use crate::types::{FileId, Language};

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

    fn fresh_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open index");
        (dir, index)
    }

    fn upsert(index: &mut Index, p: &str) -> FileId {
        index
            .upsert_file(Path::new(p), Language::Rust, 0, 0, None)
            .expect("upsert file")
    }

    fn sorted(paths: &[PathBuf]) -> Vec<String> {
        let mut out: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    /// Both hydration directions run exactly one SQL statement and zero
    /// per-id `files` lookups, at any result count — including zero.
    ///
    /// All statement counting lives in this single test because the trace
    /// counters are process-global statics.
    #[test]
    fn hydration_is_one_statement_with_zero_per_id_lookups() {
        let (_dir, mut index) = fresh_index();
        let lib = upsert(&mut index, "src/lib.rs");
        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");
        let c = upsert(&mut index, "src/c.rs");
        for to in [a, b, c] {
            index.insert_file_dependency(lib, to).expect("edge");
        }
        index.insert_file_dependency(a, c).expect("edge");

        {
            let mut conn = index.connection().expect("connection");
            conn.trace(Some(trace_cb));
        }

        reset_counts();
        let (deps, missing) = index.get_file_dependency_paths(lib).expect("deps");
        assert_eq!(
            counts(),
            (1, 0),
            "deps hydration must be one JOIN, no per-id lookups"
        );
        assert_eq!(missing, 0, "no dangling rows in this fixture");
        assert_eq!(sorted(&deps), vec!["src/a.rs", "src/b.rs", "src/c.rs"]);

        reset_counts();
        let (dependents, missing) = index.get_file_dependent_paths(c).expect("dependents");
        assert_eq!(
            counts(),
            (1, 0),
            "dependents hydration must be one JOIN, no per-id lookups"
        );
        assert_eq!(missing, 0, "no dangling rows in this fixture");
        assert_eq!(sorted(&dependents), vec!["src/a.rs", "src/lib.rs"]);

        reset_counts();
        let (empty, missing) = index.get_file_dependency_paths(b).expect("leaf deps");
        assert_eq!(
            counts(),
            (1, 0),
            "empty result must still be exactly one statement"
        );
        assert_eq!(missing, 0, "no dangling rows in this fixture");
        assert!(empty.is_empty(), "leaf file has no dependencies");
    }

    /// A dangling `file_deps` row (target `files` row absent — possible
    /// only in a hand-edited database, since `Index::open` enforces foreign
    /// keys and `files` deletes cascade) is skipped and counted; valid rows
    /// are still returned and the call does not error.
    #[test]
    fn dangling_edge_is_skipped_and_counted() {
        let (_dir, mut index) = fresh_index();
        let lib = upsert(&mut index, "src/lib.rs");
        let a = upsert(&mut index, "src/a.rs");
        index.insert_file_dependency(lib, a).expect("edge");

        {
            let conn = index.connection().expect("connection");
            conn.pragma_update(None, "foreign_keys", "OFF")
                .expect("fk off");
            conn.execute(
                "INSERT INTO file_deps (from_file_id, to_file_id, ref_count)
                 VALUES (?1, 99999, 1)",
                params![lib.as_i64()],
            )
            .expect("inject dangling edge");
            conn.pragma_update(None, "foreign_keys", "ON")
                .expect("fk on");
        }

        let (paths, missing) = index
            .get_file_dependency_paths(lib)
            .expect("dangling row must not error the call");
        assert_eq!(missing, 1, "dangling row must be counted, not returned");
        assert_eq!(
            sorted(&paths),
            vec!["src/a.rs"],
            "valid rows must still be returned"
        );
    }
}
