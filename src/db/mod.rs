//! `SQLite` storage layer for Tethys.
//!
//! This module manages the `SQLite` database that stores indexed symbols and references.
//! `SQLite` is the source of truth for all persistent data. See `graph` module for
//! graph traversal queries built on top of this storage layer.
//!
//! ## Module Structure
//!
//! - `schema` - Database schema (DDL)
//! - `helpers` - Row conversion and parsing utilities
//! - `files` - File CRUD operations
//! - `symbols` - Symbol CRUD operations
//! - `references` - Reference CRUD operations
//! - `imports` - Import CRUD operations
//! - `call_edges` - Call edge bulk operations
//! - `file_deps` - File dependency CRUD operations
//! - `panic_points` - Panic point CRUD operations
//! - `deprecated` - Deprecated-callers analysis queries
//! - `visibility` - Visibility-tightening analysis queries
//! - `graph` - Concrete `Index` graph traversal queries
//! - `architecture` - Architecture analysis (packages, coupling metrics)

mod architecture;
mod call_edges;
pub(crate) mod dead_code;
mod deprecated;
mod discovery;
mod file_deps;
mod files;
mod graph;
mod helpers;
mod hierarchy;
mod imports;
mod panic_points;
mod references;
mod revision;
mod schema;
mod symbols;
mod untested;
mod visibility;

pub use deprecated::{DeprecatedFinding, DeprecatedSymbol, ReferenceSite, Tier, Via};
pub use hierarchy::{HierarchyDirection, HierarchyNode, TypeHierarchy};
pub use untested::{UntestedFinding, UntestedReport};
pub use visibility::{Demotion, VisibilityFinding};

// Re-export helper functions and SQL constants used by other modules
pub(crate) use architecture::PackageInsert;
pub(crate) use call_edges::ORPHAN_PSEUDO_CRATE_PREFIX;
pub(crate) use files::normalize_path;
pub(crate) use graph::DEFAULT_MAX_DEPTH;
pub(crate) use helpers::{
    FILES_COLUMNS, REFS_COLUMNS, SYMBOLS_COLUMNS, parse_language, parse_symbol_kind, row_to_import,
    row_to_indexed_file, row_to_reference, row_to_symbol,
};
pub(crate) use schema::SCHEMA;

// Test-only re-exports: fixture helper for authoring ref rows directly,
// and the canonical qualified-name builder (production callers reach it
// inside `index_parsed_file_atomic`; lib.rs unit tests exercise it directly).
#[cfg(test)]
pub(crate) use files::build_qualified_name;
#[cfg(test)]
pub(crate) use references::InsertReferenceParams;

// Re-export parse_visibility for tests in types.rs
#[cfg(test)]
pub(crate) use helpers::parse_visibility;
#[cfg(test)]
pub(crate) use symbols::InsertSymbolParams;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::types::{Span, SymbolKind, Visibility};

/// Data required to insert a symbol into the database.
///
/// This is used by `index_file_atomic` to insert symbols within a transaction.
#[derive(Debug, Clone)]
pub struct SymbolData<'a> {
    pub name: &'a str,
    pub module_path: &'a str,
    pub qualified_name: &'a str,
    pub kind: SymbolKind,
    pub line: u32,
    pub column: u32,
    pub span: Option<Span>,
    pub signature: Option<&'a str>,
    pub visibility: Visibility,
    pub parent_symbol_id: Option<crate::types::SymbolId>,
    /// Name of the enclosing container as extracted (impl's implementing
    /// type, struct, enum, C# class) — linked to `parent_symbol_id`
    /// against SAME-FILE container symbols during the insert transaction
    /// (parent linkage, tethys-aay4). `None` for top-level symbols.
    /// Ignored when `parent_symbol_id` is already set explicitly.
    pub parent_name: Option<&'a str>,
    /// Whether this symbol is a test function.
    pub is_test: bool,
    /// Attributes to persist alongside the symbol row.
    pub attributes: &'a [crate::languages::common::ExtractedAttribute],
}

/// `SQLite` database wrapper for Tethys index.
///
/// The shared connection lets an owned revision guard span scoped writer work
/// without holding the mutex between database operations.
pub struct Index {
    conn: Arc<Mutex<Connection>>,
    invalid: Arc<AtomicBool>,
}

impl Index {
    /// Open or create the index database.
    pub fn open(path: &Path) -> Result<Self> {
        let index = Self::open_unconfigured(path)?;
        {
            let mut conn = index.connection()?;
            let initialize = revision::check_schema(&conn, path)?;
            revision::configure_connection(&conn)?;
            if initialize {
                let tx = conn.savepoint()?;
                tx.execute_batch(SCHEMA)?;
                tx.commit()?;
            }
        }
        Ok(index)
    }

    /// Acquire the connection lock.
    ///
    /// Returns a `MutexGuard` providing exclusive access to the underlying connection.
    /// Used internally by all database operations.
    pub(crate) fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        if self.invalid.load(Ordering::Acquire) {
            return Err(Error::Internal(
                "database connection invalid after failed revision rollback; reopen the index"
                    .into(),
            ));
        }
        self.conn.lock().map_err(|e| {
            Error::Internal(format!(
                "database connection mutex poisoned (a thread panicked while holding the lock): {e}"
            ))
        })
    }

    /// Get the current unix timestamp in nanoseconds.
    ///
    /// Returns an error if the system time is before the Unix epoch, which would
    /// break timestamp comparison logic for incremental indexing.
    // u128 nanoseconds won't exceed i64::MAX until year 2262
    #[expect(
        clippy::cast_possible_truncation,
        reason = "nanosecond timestamp fits in i64 until year 2262"
    )]
    fn now_ns() -> Result<i64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .map_err(|e| {
                Error::Config(format!(
                    "System clock is before Unix epoch: {e}. Fix system time before indexing."
                ))
            })
    }

    /// Delete a database file and its `-wal`/`-shm` sidecars, ignoring
    /// missing files. `SQLite` names sidecars by appending to the FULL
    /// filename ("tethys.db-wal"), so this pushes onto the `OsString` rather
    /// than using `Path::with_extension`. This is standalone deletion only;
    /// rebuilds replace schema transactionally without removing files.
    pub(crate) fn remove_db_files(db_path: &Path) -> Result<()> {
        Self::remove_file_if_exists(db_path)?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = db_path.as_os_str().to_owned();
            sidecar.push(suffix);
            Self::remove_file_if_exists(Path::new(&sidecar))?;
        }
        Ok(())
    }

    /// Remove a file, ignoring `NotFound` errors (the file may not exist).
    fn remove_file_if_exists(path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to delete file during reset"
                );
                Err(Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to delete {}: {e}", path.display()),
                )))
            }
        }
    }

    /// Get statistics about the database contents.
    pub fn get_stats(&self) -> Result<crate::types::DatabaseStats> {
        use std::collections::HashMap;

        let conn = self.connection()?;
        let mut stats = crate::types::DatabaseStats::default();

        // File counts by language
        let mut stmt = conn.prepare("SELECT language, COUNT(*) FROM files GROUP BY language")?;
        let rows = stmt.query_map([], |row| {
            let lang_str: String = row.get(0)?;
            let count: usize = row.get(1)?;
            Ok((lang_str, count))
        })?;

        let mut files_by_language: HashMap<crate::types::Language, usize> = HashMap::new();
        for row in rows {
            let (lang_str, count) = row?;
            if let Ok(lang) = parse_language(&lang_str) {
                files_by_language.insert(lang, count);
                stats.file_count += count;
            } else {
                tracing::warn!(
                    language = %lang_str,
                    count = count,
                    "Unknown language in database, skipping from stats"
                );
                stats.skipped_unknown_languages += count;
            }
        }
        stats.files_by_language = files_by_language;

        // Symbol counts by kind
        let mut stmt = conn.prepare("SELECT kind, COUNT(*) FROM symbols GROUP BY kind")?;
        let rows = stmt.query_map([], |row| {
            let kind_str: String = row.get(0)?;
            let count: usize = row.get(1)?;
            Ok((kind_str, count))
        })?;

        let mut symbols_by_kind: HashMap<crate::types::SymbolKind, usize> = HashMap::new();
        for row in rows {
            let (kind_str, count) = row?;
            if let Ok(kind) = parse_symbol_kind(&kind_str) {
                symbols_by_kind.insert(kind, count);
                stats.symbol_count += count;
            } else {
                tracing::warn!(
                    kind = %kind_str,
                    count = count,
                    "Unknown symbol kind in database, skipping from stats"
                );
                stats.skipped_unknown_kinds += count;
            }
        }
        stats.symbols_by_kind = symbols_by_kind;

        // Reference count
        let ref_count: usize = conn.query_row("SELECT COUNT(*) FROM refs", [], |row| row.get(0))?;
        stats.reference_count = ref_count;

        // File dependency count
        let dep_count: usize =
            conn.query_row("SELECT COUNT(*) FROM file_deps", [], |row| row.get(0))?;
        stats.file_dependency_count = dep_count;

        Ok(stats)
    }

    /// Update `SQLite` query planner statistics.
    ///
    /// Should be called after bulk data changes (full re-index) so the query
    /// planner can make better index-selection decisions. Not needed after
    /// small incremental updates.
    pub fn analyze(&self) -> Result<()> {
        let conn = self.connection()?;

        conn.execute_batch("ANALYZE")?;
        Ok(())
    }

    /// Vacuum the database.
    pub fn vacuum(&self) -> Result<()> {
        let conn = self.connection()?;

        conn.execute_batch("VACUUM")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Language, SymbolKind, Visibility};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn temp_db() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("should create temp directory");
        let path = dir.path().join("test.db");
        (dir, path)
    }

    #[test]
    fn open_creates_database_and_schema() {
        let (_dir, path) = temp_db();

        let index = Index::open(&path).expect("failed to open database");
        let conn = index.connection().expect("should get connection");

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"symbols".to_string()));
        assert!(tables.contains(&"refs".to_string()));
        assert!(tables.contains(&"file_deps".to_string()));
        assert!(tables.contains(&"imports".to_string()));
        assert!(tables.contains(&"call_edges".to_string()));
    }

    #[test]
    fn upsert_file_inserts_new_file() {
        let (_dir, path) = temp_db();
        let mut index = Index::open(&path).unwrap();

        let file_id = index
            .upsert_file(
                Path::new("src/main.rs"),
                Language::Rust,
                1_234_567_890,
                100,
                Some(0xDEAD_BEEF),
            )
            .unwrap();

        assert!(file_id.as_i64() > 0);

        let file = index.get_file(Path::new("src/main.rs")).unwrap();
        assert!(file.is_some());
        let file = file.unwrap();
        assert_eq!(file.language, Language::Rust);
        assert_eq!(file.size_bytes, 100);
    }

    #[test]
    fn upsert_file_updates_existing() {
        let (_dir, path) = temp_db();
        let mut index = Index::open(&path).unwrap();

        let id1 = index
            .upsert_file(Path::new("src/main.rs"), Language::Rust, 1000, 100, None)
            .unwrap();

        let id2 = index
            .upsert_file(Path::new("src/main.rs"), Language::Rust, 2000, 200, None)
            .unwrap();

        assert_eq!(id1, id2); // Same file, same ID

        let file = index.get_file(Path::new("src/main.rs")).unwrap().unwrap();
        assert_eq!(file.size_bytes, 200); // Updated
    }

    #[test]
    fn insert_and_list_symbols() {
        let (_dir, path) = temp_db();
        let mut index = Index::open(&path).unwrap();

        let file_id = index
            .upsert_file(Path::new("src/lib.rs"), Language::Rust, 1000, 100, None)
            .unwrap();

        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "foo",
                module_path: "crate",
                qualified_name: "foo",
                kind: SymbolKind::Function,
                line: 10,
                column: 1,
                span: None,
                signature: Some("fn foo()"),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .unwrap();

        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "bar",
                module_path: "crate",
                qualified_name: "bar",
                kind: SymbolKind::Function,
                line: 20,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Private,
                parent_symbol_id: None,
                is_test: false,
            })
            .unwrap();

        let symbols = index.list_symbols_in_file(file_id).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "foo");
        assert_eq!(symbols[1].name, "bar");
    }

    #[test]
    fn search_symbols_finds_matches() {
        let (_dir, path) = temp_db();
        let mut index = Index::open(&path).unwrap();

        let file_id = index
            .upsert_file(Path::new("src/lib.rs"), Language::Rust, 1000, 100, None)
            .unwrap();

        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "authenticate",
                module_path: "crate::auth",
                qualified_name: "authenticate",
                kind: SymbolKind::Function,
                line: 10,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .unwrap();

        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "authorize",
                module_path: "crate::auth",
                qualified_name: "authorize",
                kind: SymbolKind::Function,
                line: 20,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .unwrap();

        let results = index.search_symbols("auth", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_symbols_empty_query_returns_empty() {
        let (_dir, path) = temp_db();
        let index = Index::open(&path).unwrap();

        let results = index.search_symbols("", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn standalone_removal_deletes_database_and_sidecars() {
        let (_dir, path) = temp_db();

        // Build sidecar paths using OsString::push (append) to match
        // how SQLite names these files.
        let mut wal_os = path.as_os_str().to_owned();
        wal_os.push("-wal");
        let wal_path = PathBuf::from(&wal_os);
        let mut shm_os = path.as_os_str().to_owned();
        shm_os.push("-shm");
        let shm_path = PathBuf::from(&shm_os);

        // Open and immediately drop the index so SQLite releases its memory-mapped
        // hold on the SHM file. On Windows, SQLite's WAL-mode memory mapping
        // prevents external writes to the SHM file while a connection is open.
        {
            let _index = Index::open(&path).expect("should open database");
        }

        // With no active SQLite connection, we can safely write fake sidecar data.
        std::fs::write(&wal_path, b"stale wal data").expect("should create WAL file");
        std::fs::write(&shm_path, b"stale shm data").expect("should create SHM file");
        Index::remove_db_files(&path).expect("remove closed database");
        assert!(!path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }
}
