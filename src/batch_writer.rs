//! Streaming `SQLite` writer for parallel file indexing.
//!
//! This module provides [`BatchWriter`], a background writer that receives parsed
//! file data over an MPSC channel and writes it to `SQLite` in batches. This reduces
//! memory usage from O(n) to `O(batch_size)` during indexing of large codebases.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     index_with_options                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Main Thread          │  Background Writer Thread               │
//! │  ──────────────       │  ────────────────────────               │
//! │  rayon::par_iter()    │  recv() from channel                    │
//! │  parse files          │  accumulate until batch_size            │
//! │  send to channel ─────┼→ write batch (file-level savepoints)    │
//! │  ...                  │  propagate storage failures             │
//! │  drop sender          │  return Result<WriteStats>              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! std::thread::scope(|scope| {
//!     let batch_writer = BatchWriter::new(scope, &index, 100);
//!     let sent = source_files.par_iter().try_for_each(|(path, lang)| {
//!         let data = Tethys::parse_file_static(&workspace_root, path, *lang)?;
//!         batch_writer.send(data)
//!     });
//!     let result = batch_writer.finish()?;
//!     sent?;
//!     Ok::<_, tethys::Error>(result)
//! })?;
//! ```

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{Scope, ScopedJoinHandle};

use tracing::{debug, error, trace};

use crate::db::{Index, SymbolData};
use crate::error::{Error, Result};
use crate::parallel::ParsedFileData;

/// Statistics about the batch writing process.
#[derive(Debug, Default, Clone)]
pub struct WriteStats {
    /// Number of files successfully written to the database.
    pub files_written: usize,
    /// Number of files that failed to write.
    pub files_failed: usize,
    /// Number of symbols written.
    pub symbols_written: usize,
    /// Number of references written.
    pub references_written: usize,
    /// Number of batches committed (transactions).
    pub batches_committed: usize,
}

/// Result returned when the batch writer finishes.
#[derive(Debug)]
pub struct BatchWriteResult {
    /// Statistics about what was written.
    pub stats: WriteStats,
}

/// A background writer that receives parsed file data and writes it to `SQLite`
/// in batches.
///
/// This struct owns the sending end of an MPSC channel. Parsed files are sent
/// via [`send()`](Self::send) and accumulated in the background thread until
/// `batch_size` files are collected, at which point they're written using
/// file-level savepoints on the caller's database connection.
///
/// When [`finish()`](Self::finish) is called, the sender is dropped, the
/// background thread completes any remaining writes, and the final statistics
/// are returned.
pub struct BatchWriter<'scope> {
    /// Channel sender for parsed file data.
    sender: SyncSender<ParsedFileData>,
    /// Handle to the background writer thread.
    handle: ScopedJoinHandle<'scope, Result<BatchWriteResult>>,
}

impl<'scope> BatchWriter<'scope> {
    /// Create a new batch writer with the given database and batch size.
    ///
    /// # Arguments
    /// * `scope` - Thread scope that bounds the writer's lifetime
    /// * `db` - The caller's index, including its active revision
    /// * `batch_size` - Number of files to accumulate before writing
    ///
    /// # Panics
    /// Panics if `batch_size` is 0 (would cause infinite accumulation without writes).
    #[must_use]
    pub fn new<'env>(
        scope: &'scope Scope<'scope, 'env>,
        db: &'env Index,
        batch_size: usize,
    ) -> Self {
        assert!(batch_size > 0, "batch_size must be at least 1");

        let (sender, receiver) = mpsc::sync_channel(batch_size);

        let handle = scope.spawn(move || Self::writer_thread(db, &receiver, batch_size));

        Self { sender, handle }
    }

    /// Send parsed file data to the background writer.
    ///
    /// Applies backpressure when the bounded queue is full; disconnection returns an error.
    ///
    /// # Arguments
    /// * `data` - The parsed file data to write
    ///
    /// # Errors
    /// Returns an error if the background writer has disconnected.
    pub fn send(&self, data: ParsedFileData) -> Result<()> {
        self.sender.send(data).map_err(|e| {
            Error::Internal(format!(
                "Failed to send {} to batch writer (receiver disconnected)",
                e.0.relative_path.display()
            ))
        })
    }

    /// Finish writing and return the final statistics.
    ///
    /// This drops the sender, causing the background thread to complete any
    /// remaining batch and return. Blocks until the background thread finishes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The background thread panicked
    /// - A database write failed
    pub fn finish(self) -> Result<BatchWriteResult> {
        // Drop sender to signal the background thread to finish
        drop(self.sender);

        // Wait for the background thread
        match self.handle.join() {
            Ok(result) => result,
            Err(panic_payload) => {
                let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    format!("Batch writer thread panicked: {s}")
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    format!("Batch writer thread panicked: {s}")
                } else {
                    "Batch writer thread panicked with unknown payload".to_string()
                };
                error!(panic_msg = %msg, "Background batch writer thread panicked");
                Err(Error::Internal(msg))
            }
        }
    }

    /// Background thread function that receives and writes file data.
    fn writer_thread(
        db: &Index,
        receiver: &Receiver<ParsedFileData>,
        batch_size: usize,
    ) -> Result<BatchWriteResult> {
        let mut stats = WriteStats::default();
        let mut batch: Vec<ParsedFileData> = Vec::with_capacity(batch_size);

        loop {
            // Use match (not if-let) to make the channel-closed path explicit
            // rather than hiding it in an else branch.
            #[expect(
                clippy::single_match_else,
                reason = "explicit match makes the channel-closed path visible"
            )]
            match receiver.recv() {
                Ok(data) => {
                    batch.push(data);

                    if batch.len() >= batch_size {
                        Self::write_batch(db, &mut batch, &mut stats)?;
                    }
                }
                Err(_) => {
                    // Channel closed (all senders dropped) -- write remaining batch and exit.
                    if !batch.is_empty() {
                        Self::write_batch(db, &mut batch, &mut stats)?;
                    }
                    break;
                }
            }
        }

        debug!(
            files = stats.files_written,
            failed = stats.files_failed,
            symbols = stats.symbols_written,
            references = stats.references_written,
            batches = stats.batches_committed,
            "Batch writer finished"
        );

        Ok(BatchWriteResult { stats })
    }

    /// Write a batch of files.
    fn write_batch(
        db: &Index,
        batch: &mut Vec<ParsedFileData>,
        stats: &mut WriteStats,
    ) -> Result<()> {
        trace!(batch_size = batch.len(), "Writing batch");

        // File savepoints preserve atomic writes without publishing the
        // enclosing revision. Batches amortize channel overhead.
        for data in batch.drain(..) {
            let (sym_count, ref_count) = Self::write_single_file(db, &data)?;
            stats.files_written += 1;
            stats.symbols_written += sym_count;
            stats.references_written += ref_count;
        }

        stats.batches_committed += 1;
        Ok(())
    }

    /// Write a single file to the database.
    ///
    /// The complete write (file record, symbols, references, imports) happens
    /// in ONE savepoint via [`Index::index_parsed_file_atomic`] — shared
    /// with the batch-mode path, so the two write modes can no longer drift.
    /// Does NOT compute file-level dependencies - that requires access to
    /// Tethys state (workspace root, module path resolution) and is done
    /// after all files are written.
    fn write_single_file(db: &Index, data: &ParsedFileData) -> Result<(usize, usize)> {
        // Convert owned symbols to borrowed for insertion
        let symbol_data: Vec<SymbolData<'_>> =
            data.symbols.iter().map(|s| s.as_symbol_data()).collect();

        let (_file_id, _symbol_ids, refs_stored) = db.index_parsed_file_atomic(
            &data.relative_path,
            data.language,
            data.mtime_ns,
            data.size_bytes,
            None,
            &symbol_data,
            &data.references,
            &data.imports,
        )?;

        Ok((data.symbols.len(), refs_stored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parallel::OwnedSymbolData;
    use crate::types::{Language, SymbolKind, Visibility};
    use std::path::PathBuf;
    use std::thread;
    use tempfile::TempDir;

    fn temp_db_path() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.db");
        (dir, path)
    }

    #[test]
    fn batch_writer_writes_single_file() {
        let (_dir, db_path) = temp_db_path();

        let db = Index::open(&db_path).expect("open");

        let data = ParsedFileData {
            relative_path: PathBuf::from("src/main.rs"),
            language: Language::Rust,
            mtime_ns: 1_234_567_890,
            size_bytes: 100,
            symbols: vec![OwnedSymbolData {
                name: "main".to_string(),
                module_path: "crate".to_string(),
                qualified_name: "crate::main".to_string(),
                kind: SymbolKind::Function,
                line: 1,
                column: 0,
                span: None,
                signature: Some("fn main()".to_string()),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                parent_name: None,
                is_test: false,
                attributes: Vec::new(),
            }],
            references: vec![],
            imports: vec![],
        };

        let result = thread::scope(|scope| {
            let writer = BatchWriter::new(scope, &db, 10);
            writer.send(data).expect("send");
            writer.finish().expect("finish")
        });

        assert_eq!(result.stats.files_written, 1);
        assert_eq!(result.stats.symbols_written, 1);
        assert_eq!(result.stats.batches_committed, 1);
    }

    #[test]
    fn batch_writer_respects_batch_size() {
        let (_dir, db_path) = temp_db_path();

        let db = Index::open(&db_path).expect("open");
        let result = thread::scope(|scope| {
            let writer = BatchWriter::new(scope, &db, 3);

            // Send 7 files - should result in 3 batches (3 + 3 + 1)
            for i in 0..7 {
                let data = ParsedFileData {
                    relative_path: PathBuf::from(format!("src/file{i}.rs")),
                    language: Language::Rust,
                    mtime_ns: 1_234_567_890 + i64::from(i),
                    size_bytes: 100,
                    symbols: vec![],
                    references: vec![],
                    imports: vec![],
                };
                writer.send(data).expect("send");
            }

            writer.finish().expect("finish")
        });

        assert_eq!(result.stats.files_written, 7);
        assert_eq!(result.stats.batches_committed, 3);
    }

    #[test]
    fn batch_writer_handles_empty_input() {
        let (_dir, db_path) = temp_db_path();

        let db = Index::open(&db_path).expect("open");
        let result =
            thread::scope(|scope| BatchWriter::new(scope, &db, 10).finish().expect("finish"));

        assert_eq!(result.stats.files_written, 0);
        assert_eq!(result.stats.batches_committed, 0);
    }

    // build_qualified_name tests live with the canonical implementation in
    // db/files.rs (build_qualified_name_shapes); the duplicate this module
    // once carried was deleted with the duplicated write path.

    /// A storage failure must reach the revision owner and roll back every file.
    #[test]
    fn bad_file_in_batch_aborts_revision() {
        let (_dir, db_path) = temp_db_path();

        let db = Index::open(&db_path).expect("open");
        let revision = db.begin_revision(false).expect("begin");

        let good = |name: &str| ParsedFileData {
            relative_path: PathBuf::from(format!("src/{name}.rs")),
            language: Language::Rust,
            mtime_ns: 1,
            size_bytes: 1,
            symbols: vec![OwnedSymbolData {
                name: name.to_string(),
                module_path: String::new(),
                qualified_name: name.to_string(),
                kind: SymbolKind::Function,
                line: 1,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                parent_name: None,
                is_test: false,
                attributes: Vec::new(),
            }],
            references: vec![],
            imports: vec![],
        };

        let mut bad = good("poisoned");
        bad.symbols[0].parent_symbol_id = Some(crate::types::SymbolId::from(999_999));

        let result = thread::scope(|scope| {
            let writer = BatchWriter::new(scope, &db, 3);
            writer.send(good("a")).expect("send first");
            writer.send(bad).expect("send bad");
            writer.send(good("b")).expect("send last");
            writer.finish()
        });
        assert!(result.is_err(), "storage failure must abort indexing");
        drop(revision);
        assert!(
            db.get_file_id(std::path::Path::new("src/a.rs"))
                .expect("query")
                .is_none(),
            "earlier successful file must roll back"
        );
        assert!(
            db.get_file_id(std::path::Path::new("src/poisoned.rs"))
                .expect("query")
                .is_none(),
            "the failed file must leave no rows"
        );
    }
}
