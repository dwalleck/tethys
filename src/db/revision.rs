//! Owned whole-run publication and transactional schema replacement.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::Index;
use super::schema::{DROP_SCHEMA, SCHEMA_VERSION, install_schema};
use crate::error::{Error, Result};

/// An unpublished revision on the index's shared connection.
///
/// No connection lock is retained between operations. Scoped writers must finish
/// before this guard commits or drops. Dropping an unfinished revision rolls back.
pub(crate) struct Revision {
    conn: Arc<Mutex<Connection>>,
    invalid: Arc<AtomicBool>,
    committed: bool,
}

impl Index {
    /// Open without applying or upgrading any schema, for explicit rebuild only.
    pub(crate) fn open_for_rebuild(path: &Path) -> Result<Self> {
        let index = Self::open_unconfigured(path)?;
        configure_connection(&*index.connection()?)?;
        Ok(index)
    }

    /// Open before any persistent configuration so ordinary schema refusal is read-only.
    pub(super) fn open_unconfigured(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            invalid: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Begin the sole publication transaction; schema replacement is part of it.
    pub(crate) fn begin_revision(&self, rebuild: bool) -> Result<Revision> {
        {
            let conn = self.connection()?;
            conn.execute_batch("BEGIN IMMEDIATE")?;
        }
        let revision = Revision {
            conn: Arc::clone(&self.conn),
            invalid: Arc::clone(&self.invalid),
            committed: false,
        };
        if rebuild {
            let conn = self.connection()?;
            conn.flush_prepared_statement_cache();
            conn.execute_batch(DROP_SCHEMA)?;
            install_schema(&conn)?;
        }
        Ok(revision)
    }
}

/// Configure only accepted indexes and explicitly requested rebuilds.
pub(super) fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

impl Revision {
    /// Publish all facts and their revision identity together.
    pub(crate) fn commit(mut self) -> Result<()> {
        {
            let conn = self
                .conn
                .lock()
                .map_err(|e| Error::Internal(format!("database connection mutex poisoned: {e}")))?;
            let changed = conn.execute(
                "UPDATE index_revision SET revision = revision + 1
                 WHERE singleton = 1 AND schema_version = ?1 AND revision < 9223372036854775807",
                [SCHEMA_VERSION],
            )?;
            if changed != 1 {
                return Err(Error::Internal(
                    "missing or exhausted index revision identity".into(),
                ));
            }
            conn.execute_batch("COMMIT")?;
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for Revision {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let conn = match self.conn.lock() {
            Ok(conn) => conn,
            Err(poisoned) => {
                tracing::warn!("Recovering poisoned connection to roll back index revision");
                poisoned.into_inner()
            }
        };
        // SQLite may already have rolled back a failed COMMIT (e.g. a hook).
        if !conn.is_autocommit()
            && let Err(error) = conn.execute_batch("ROLLBACK")
        {
            self.invalid.store(true, Ordering::Release);
            tracing::error!(%error, "Revision rollback failed; index connection invalidated");
        }
        conn.flush_prepared_statement_cache();
    }
}

/// Refuse recognized legacy caches before issuing any schema DDL. Unrelated
/// tables alone do not turn an otherwise fresh database into a legacy cache.
/// Returns whether a fresh schema must be installed.
pub(super) fn check_schema(conn: &Connection, path: &Path) -> Result<bool> {
    let has_metadata: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'index_revision')",
        [],
        |row| row.get(0),
    )?;
    let current = if has_metadata {
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let metadata: rusqlite::Result<(i64, i64)> = conn.query_row(
            "SELECT schema_version, revision FROM index_revision WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match metadata {
            Ok((schema, revision)) => {
                version == SCHEMA_VERSION && schema == SCHEMA_VERSION && revision >= 0
            }
            Err(error) => {
                tracing::warn!(%error, "Invalid index revision metadata; rebuild required");
                false
            }
        }
    } else {
        !conn.query_row::<bool, _, _>(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name IN
             ('files','symbols','refs','file_deps','imports','call_edges','attributes',
              'arch_packages','arch_file_packages','arch_package_deps'))",
            [],
            |row| row.get(0),
        )?
    };
    if !current {
        return Err(Error::Config(format!(
            "index schema is outdated or invalid; run `tethys index --rebuild` (db: {})",
            path.display()
        )));
    }
    Ok(!has_metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Language;

    fn paths(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT path FROM files ORDER BY path")
            .expect("paths")
            .query_map([], |row| row.get(0))
            .expect("query paths")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("path rows")
    }

    fn schema(conn: &Connection) -> Vec<(String, String, Option<String>)> {
        conn.prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
            .expect("schema")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("query schema")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("schema rows")
    }

    fn write_file(index: &Index, path: &str) {
        index
            .index_parsed_file_atomic(Path::new(path), Language::Rust, 1, 1, None, &[], &[], &[])
            .expect("write complete parse");
    }

    #[test]
    fn released_savepoints_publish_only_on_revision_commit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        write_file(&index, "old.rs");
        let observer = Connection::open(&path).expect("observer");
        let pinned = Connection::open(&path).expect("pinned reader");
        pinned.execute_batch("BEGIN").expect("pin transaction");
        assert_eq!(paths(&pinned), ["old.rs"]);

        let revision = index.begin_revision(false).expect("revision");
        write_file(&index, "new.rs");
        let old_id = index
            .get_file_id(Path::new("old.rs"))
            .expect("lookup")
            .expect("old file");
        index.delete_files(&[old_id]).expect("delete old");
        assert_eq!(
            paths(&observer),
            ["old.rs"],
            "released file savepoints must remain unpublished"
        );
        revision.commit().expect("publish");
        assert_eq!(paths(&observer), ["new.rs"]);
        assert_eq!(
            paths(&pinned),
            ["old.rs"],
            "pinned reader retains its complete revision"
        );
        pinned.execute_batch("COMMIT").expect("release reader");
        assert_eq!(paths(&pinned), ["new.rs"]);
        let published: i64 = observer
            .query_row("SELECT revision FROM index_revision", [], |row| row.get(0))
            .expect("revision identity");
        assert_eq!(published, 1);

        let revision = index.begin_revision(false).expect("next revision");
        write_file(&index, "aborted.rs");
        drop(revision);
        assert_eq!(paths(&observer), ["new.rs"]);
    }

    #[test]
    fn rebuild_rollback_restores_legacy_schema_rows_and_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy.db");
        let observer = Connection::open(&path).expect("legacy");
        observer
            .execute_batch(
                "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT NOT NULL);
             INSERT INTO files VALUES (7, 'legacy.rs'); PRAGMA user_version = 42;",
            )
            .expect("legacy fixture");
        let before = schema(&observer);
        assert!(matches!(Index::open(&path), Err(Error::Config(_))));
        assert_eq!(
            schema(&observer),
            before,
            "ordinary refusal must not apply DDL"
        );

        let index = Index::open_for_rebuild(&path).expect("raw rebuild open");
        let revision = index.begin_revision(true).expect("replace schema");
        write_file(&index, "replacement.rs");
        assert_eq!(
            paths(&observer),
            ["legacy.rs"],
            "DDL and rows are still unpublished"
        );
        drop(revision);
        assert_eq!(
            schema(&observer),
            before,
            "rollback restores exact legacy DDL"
        );
        assert_eq!(paths(&observer), ["legacy.rs"]);
        let version: i64 = observer
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version");
        assert_eq!(version, 42);

        let revision = index.begin_revision(true).expect("retry rebuild");
        write_file(&index, "replacement.rs");
        revision.commit().expect("publish replacement");
        assert_eq!(paths(&observer), ["replacement.rs"]);
        Index::open(&path).expect("current rebuilt schema opens normally");
    }

    #[test]
    fn competing_writer_is_busy_until_revision_release() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        let competitor = Connection::open(&path).expect("competitor");
        competitor
            .busy_timeout(std::time::Duration::ZERO)
            .expect("no waiting");
        let revision = index.begin_revision(false).expect("revision");
        let error = competitor
            .execute_batch("BEGIN IMMEDIATE")
            .expect_err("writer must be excluded");
        assert!(matches!(error, rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseBusy));
        drop(revision);
        competitor
            .execute_batch("BEGIN IMMEDIATE; ROLLBACK")
            .expect("writer succeeds after release");
    }
}
