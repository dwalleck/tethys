//! Reference CRUD operations for the Tethys index.
//!
//! These operations support symbol-level "who calls X?" queries.
//! See graph module for higher-level graph traversal using these primitives.

use std::path::PathBuf;

use rusqlite::params;
use tracing::trace;

use super::{Index, REFS_COLUMNS, row_to_reference};
use crate::error::Result;
use crate::types::{FileId, RefId, Reference, ResolutionStrategy, SymbolId};

/// Single source of truth for the "resolve a reference" UPDATE, shared by the
/// batched Pass 2 path ([`Index::apply_resolutions`]) and the single-row
/// Pass 3/LSP path ([`Index::resolve_reference`]) so a future change to how a
/// resolution is recorded cannot silently diverge between the two.
const RESOLVE_REFERENCE_SQL: &str =
    "UPDATE refs SET symbol_id = ?2, reference_name = NULL, strategy = ?3 WHERE id = ?1";

/// Parameters for inserting a reference into the index.
///
/// Test-only: production references are written inside the per-file
/// transaction by [`Index::index_parsed_file_atomic`]; fixtures use this
/// to author ref rows directly.
#[cfg(test)]
pub(crate) struct InsertReferenceParams<'a> {
    /// The symbol this reference points to, or `None` for unresolved references.
    pub symbol_id: Option<SymbolId>,
    /// The file containing this reference.
    pub file_id: FileId,
    /// The kind of reference (e.g., "call", "use").
    pub kind: &'a str,
    /// Line number of the reference (1-indexed).
    pub line: u32,
    /// Column number of the reference (0-indexed).
    pub column: u32,
    /// The symbol that contains this reference, if any.
    pub in_symbol_id: Option<SymbolId>,
    /// The name used in the reference, for later resolution in Pass 2.
    pub reference_name: Option<&'a str>,
    /// Provenance for pre-resolved fixture rows (`None` = unresolved).
    /// Typed as the enum so fixtures cannot author labels the wire format
    /// doesn't have.
    pub strategy: Option<ResolutionStrategy>,
}

impl Index {
    /// Insert a reference to a symbol.
    ///
    /// If `symbol_id` is `None`, the reference is unresolved and `reference_name`
    /// should be provided for later resolution in Pass 2.
    ///
    /// Test-only: see [`InsertReferenceParams`].
    #[cfg(test)]
    pub fn insert_reference(&self, params: &InsertReferenceParams<'_>) -> Result<i64> {
        debug_assert!(
            params.line >= 1,
            "reference line should be >= 1, got {}",
            params.line
        );
        debug_assert!(
            params.symbol_id.is_some() || params.reference_name.is_some(),
            "unresolved reference (symbol_id is None) must have a reference_name for Pass 2"
        );
        let conn = self.connection()?;

        conn.execute(
            "INSERT INTO refs (symbol_id, file_id, kind, line, column, in_symbol_id, reference_name, strategy)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                params.symbol_id.map(SymbolId::as_i64),
                params.file_id.as_i64(),
                params.kind,
                params.line,
                params.column,
                params.in_symbol_id.map(SymbolId::as_i64),
                params.reference_name,
                params.strategy.map(ResolutionStrategy::as_str)
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Delete `value`- and `macro_call`-kind references that resolved to no
    /// in-crate symbol.
    ///
    /// Both kinds are emitted speculatively at extraction: fn-as-value
    /// (tethys-ygjx) records every non-locally-bound value-position
    /// identifier, and the macro token walk (tethys-8ym0) records every bare
    /// call-shaped token — most name locals, std, or externals that Pass-2
    /// cannot resolve. Once resolution is complete an unresolved row of
    /// either kind is noise — dropping it keeps `reference_name` queries
    /// clean and avoids padding the refs table with hundreds of dead rows.
    /// Call ONLY after all resolution passes finish; calling earlier would
    /// delete refs that are merely not-yet-resolved. Nothing enforces this
    /// ordering — it is relied on by placing the single call site after all
    /// resolution passes in the indexing pipeline.
    ///
    /// Returns the number of rows deleted.
    pub fn drop_unresolved_value_and_macro_call_refs(&self) -> Result<usize> {
        let conn = self.connection()?;
        let deleted = conn.execute(
            "DELETE FROM refs WHERE kind IN ('value', 'macro_call') AND symbol_id IS NULL",
            [],
        )?;
        trace!(deleted, "Dropped unresolved value/macro_call refs");
        Ok(deleted)
    }

    /// Get all unresolved references (where `symbol_id` is NULL).
    ///
    /// These references need to be resolved in Pass 2 by matching their
    /// `reference_name` to symbols discovered in other files.
    pub fn get_unresolved_references(&self) -> Result<Vec<Reference>> {
        trace!("Getting unresolved references");
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {REFS_COLUMNS} FROM refs WHERE symbol_id IS NULL ORDER BY file_id, line"
        ))?;

        let refs = stmt
            .query_map([], row_to_reference)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(refs)
    }

    /// Get unresolved references with file path information for LSP queries.
    ///
    /// Returns references where `symbol_id` is NULL, including the file path
    /// from the files table. LSP clients need file paths (not database IDs) to
    /// issue `goto_definition` requests.
    ///
    /// Positions are returned as 1-indexed (as stored in DB). Convert to 0-indexed
    /// before passing to LSP clients.
    pub fn get_unresolved_references_for_lsp(
        &self,
    ) -> Result<Vec<crate::types::UnresolvedRefForLsp>> {
        trace!("Getting unresolved references for LSP resolution");
        let conn = self.connection()?;

        let mut stmt = conn.prepare(
            "SELECT r.id, r.file_id, f.path, r.line, r.column, r.reference_name
             FROM refs r
             JOIN files f ON r.file_id = f.id
             WHERE r.symbol_id IS NULL AND r.reference_name IS NOT NULL
             ORDER BY f.path, r.line, r.column",
        )?;

        let refs = stmt
            .query_map([], |row| {
                Ok(crate::types::UnresolvedRefForLsp {
                    ref_id: RefId::from(row.get::<_, i64>(0)?),
                    file_id: FileId::from(row.get::<_, i64>(1)?),
                    file_path: PathBuf::from(row.get::<_, String>(2)?),
                    line: row.get(3)?,
                    column: row.get(4)?,
                    reference_name: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        trace!(
            unresolved_count = refs.len(),
            "Found unresolved references for LSP"
        );

        Ok(refs)
    }

    /// Apply a batch of Pass 2 resolutions in ONE transaction.
    ///
    /// Each pair is `(ref row id, resolved symbol id)`. Replaces the
    /// per-resolution autocommit UPDATE pattern (idxperf claim C7): on a
    /// workspace with thousands of resolutions, one fsync instead of one
    /// per resolution. Empty input is a no-op without touching the
    /// connection.
    ///
    /// Sanity hint (not load-bearing): ref ids are expected to come from
    /// the same index — a stale id makes its UPDATE a silent no-op, which
    /// the caller's resolved-count accounting surfaces.
    pub fn apply_resolutions(
        &self,
        resolutions: &[(i64, SymbolId, ResolutionStrategy)],
    ) -> Result<()> {
        if resolutions.is_empty() {
            return Ok(());
        }
        trace!(
            resolution_count = resolutions.len(),
            "Applying Pass 2 resolutions in one transaction"
        );
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(RESOLVE_REFERENCE_SQL)?;
            for (ref_id, symbol_id, strategy) in resolutions {
                stmt.execute(params![ref_id, symbol_id.as_i64(), strategy.as_str()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Resolve a reference by setting its `symbol_id`.
    ///
    /// This is used in Pass 3 (LSP) to link unresolved references to their
    /// target symbols one at a time as the language server answers; Pass 2
    /// uses the batched [`Self::apply_resolutions`] instead.
    pub fn resolve_reference(
        &self,
        ref_id: i64,
        symbol_id: SymbolId,
        strategy: ResolutionStrategy,
    ) -> Result<()> {
        trace!(
            ref_id = ref_id,
            symbol_id = %symbol_id,
            strategy = strategy.as_str(),
            "Resolving reference"
        );
        let conn = self.connection()?;

        conn.execute(
            RESOLVE_REFERENCE_SQL,
            params![ref_id, symbol_id.as_i64(), strategy.as_str()],
        )?;
        Ok(())
    }

    /// Get all references to a symbol.
    pub fn get_references_to_symbol(&self, symbol_id: SymbolId) -> Result<Vec<Reference>> {
        trace!(symbol_id = %symbol_id, "Getting references to symbol");
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {REFS_COLUMNS} FROM refs WHERE symbol_id = ?1 ORDER BY file_id, line"
        ))?;

        let refs = stmt
            .query_map([symbol_id.as_i64()], row_to_reference)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(refs)
    }

    /// List all outgoing references from a file.
    pub fn list_references_in_file(&self, file_id: FileId) -> Result<Vec<Reference>> {
        trace!(file_id = %file_id, "Listing references in file");
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {REFS_COLUMNS} FROM refs WHERE file_id = ?1 ORDER BY line, column"
        ))?;

        let refs = stmt
            .query_map([file_id.as_i64()], row_to_reference)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(refs)
    }
}

#[cfg(test)]
mod apply_resolutions_tests {
    use super::*;
    use crate::db::Index;
    use crate::db::symbols::InsertSymbolParams;
    use crate::types::{Language, SymbolKind, Visibility};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Fixture: one file, one symbol, two unresolved refs. Returns the
    /// ref row ids and the target symbol id.
    fn fixture(index: &mut Index) -> (Vec<i64>, SymbolId) {
        let file_id = index
            .upsert_file(Path::new("src/a.rs"), Language::Rust, 0, 0, None)
            .expect("file");
        let sym_id = index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "target",
                module_path: "",
                qualified_name: "target",
                kind: SymbolKind::Function,
                line: 1,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("symbol");
        let mut ref_ids = Vec::new();
        for line in [10, 11] {
            ref_ids.push(
                index
                    .insert_reference(&InsertReferenceParams {
                        symbol_id: None,
                        file_id,
                        kind: "call",
                        line,
                        column: 1,
                        in_symbol_id: None,
                        reference_name: Some("target"),
                        strategy: None,
                    })
                    .expect("ref"),
            );
        }
        (ref_ids, sym_id)
    }

    /// tethys-9z7i B6 (design C6): the single-row LSP path stamps `lsp`
    /// through the SAME widened SQL as the batch — the readback checks
    /// strategy AND the seam's existing `reference_name` null-out, so a
    /// forked single-row statement missing either fails.
    #[test]
    fn lsp_path_stamps_strategy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");
        let (ref_ids, sym_id) = fixture(&mut index);

        index
            .resolve_reference(ref_ids[0], sym_id, ResolutionStrategy::Lsp)
            .expect("resolve via lsp");

        let conn = index.connection().expect("conn");
        let (strategy, name_nulled): (String, bool) = conn
            .query_row(
                "SELECT COALESCE(strategy, '(null)'), reference_name IS NULL
                 FROM refs WHERE id = ?1",
                [ref_ids[0]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("readback");
        assert_eq!(strategy, "lsp");
        assert!(name_nulled, "the seam still nulls reference_name");
    }

    /// C7 fence: the whole batch applies in EXACTLY one transaction, every
    /// pair lands, and `reference_name` is cleared. A buggy implementation
    /// that falls back to per-resolution autocommit fails the commit count;
    /// one that loses pairs fails the row asserts.
    #[test]
    fn apply_resolutions_batches_in_one_commit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");
        let (ref_ids, sym_id) = fixture(&mut index);

        let commits = Arc::new(AtomicUsize::new(0));
        {
            let counter = Arc::clone(&commits);
            index.connection().expect("conn").commit_hook(Some(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                false
            }));
        }

        let pairs: Vec<(i64, SymbolId, ResolutionStrategy)> = ref_ids
            .iter()
            .map(|&r| (r, sym_id, ResolutionStrategy::ExplicitImport))
            .collect();
        index.apply_resolutions(&pairs).expect("apply");

        index
            .connection()
            .expect("conn")
            .commit_hook(None::<fn() -> bool>);

        assert_eq!(commits.load(Ordering::SeqCst), 1, "one transaction total");

        let conn = index.connection().expect("conn");
        let (resolved, named): (i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM refs WHERE symbol_id = ?1),
                   (SELECT COUNT(*) FROM refs WHERE reference_name IS NOT NULL)",
                [sym_id.as_i64()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("counts");
        assert_eq!(resolved, 2, "both resolutions applied");
        assert_eq!(named, 0, "reference_name cleared on resolution");
    }

    /// Empty input must not open a transaction at all.
    #[test]
    fn apply_resolutions_empty_is_a_no_op() {
        let dir = tempfile::tempdir().expect("tempdir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open");

        let commits = Arc::new(AtomicUsize::new(0));
        {
            let counter = Arc::clone(&commits);
            index.connection().expect("conn").commit_hook(Some(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                false
            }));
        }
        index.apply_resolutions(&[]).expect("apply empty");
        index
            .connection()
            .expect("conn")
            .commit_hook(None::<fn() -> bool>);
        assert_eq!(commits.load(Ordering::SeqCst), 0);
    }
}
