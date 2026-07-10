//! File CRUD operations for the Tethys index.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::OptionalExtension;
use rusqlite::params;
use tracing::trace;

use super::{FILES_COLUMNS, Index, SymbolData, row_to_indexed_file};
use crate::error::Result;
use crate::languages::common::{ExtractedReference, ExtractedReferenceKind, ImportStatement};
use crate::languages::module_resolver::get_module_resolver;
use crate::types::ResolutionStrategy;
use crate::types::{FileId, IndexedFile, Language, Span, SymbolId, SymbolKind};

/// Build a qualified name from a simple name and optional path segments.
///
/// Canonical home of the logic previously duplicated between
/// `Tethys::build_qualified_name` and `batch_writer::build_qualified_name`
/// (both collapse onto this in the call-site conversion slices).
///
/// Examples:
/// - `("open", Some(["Index"]))` -> `"Index::open"`
/// - `("Foo", None)` -> `"Foo"`
/// - `("bar", Some([]))` -> `"bar"`
pub(crate) fn build_qualified_name(name: &str, path: Option<&[String]>) -> String {
    match path {
        Some(segments) if !segments.is_empty() => {
            format!("{}::{}", segments.join("::"), name)
        }
        _ => name.to_string(),
    }
}

/// Normalize a file path to use forward slashes for consistent DB storage.
///
/// On Windows, `Path::to_string_lossy()` preserves backslashes from OS APIs,
/// but tests and cross-platform code use forward slashes. Normalizing to `/`
/// ensures lookups match regardless of how the path was constructed.
///
/// Exposed `pub(crate)` so callers that need to compare paths against
/// DB-stored paths (e.g. staleness detection) use the same normalization.
pub(crate) fn normalize_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

impl Index {
    /// Insert or update a file record, returning the file ID.
    ///
    /// Delegates to [`Self::index_parsed_file_atomic`] with empty symbols,
    /// references, and imports.
    #[cfg(test)]
    pub fn upsert_file(
        &mut self,
        path: &Path,
        language: Language,
        mtime_ns: i64,
        size_bytes: u64,
        content_hash: Option<u64>,
    ) -> Result<FileId> {
        let (file_id, _symbol_ids, _refs) = self.index_parsed_file_atomic(
            path,
            language,
            mtime_ns,
            size_bytes,
            content_hash,
            &[],
            &[],
            &[],
        )?;
        Ok(file_id)
    }

    /// Get a file by path.
    pub fn get_file(&self, path: &Path) -> Result<Option<IndexedFile>> {
        let path_str = normalize_path(path);
        let conn = self.connection()?;

        conn.query_row(
            &format!("SELECT {FILES_COLUMNS} FROM files WHERE path = ?1"),
            [&path_str],
            row_to_indexed_file,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Get file ID by path.
    pub fn get_file_id(&self, path: &Path) -> Result<Option<FileId>> {
        let path_str = normalize_path(path);
        let conn = self.connection()?;

        conn.query_row("SELECT id FROM files WHERE path = ?1", [&path_str], |row| {
            row.get::<_, i64>(0).map(FileId::from)
        })
        .optional()
        .map_err(Into::into)
    }

    /// Get a file by its database ID.
    pub fn get_file_by_id(&self, id: FileId) -> Result<Option<IndexedFile>> {
        let conn = self.connection()?;

        conn.query_row(
            &format!("SELECT {FILES_COLUMNS} FROM files WHERE id = ?1"),
            [id.as_i64()],
            row_to_indexed_file,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Atomically write a file's COMPLETE parse output — file row, symbols,
    /// attributes, references, and imports — in ONE transaction with cached
    /// statements (idxperf design claims C4/C5).
    ///
    /// All-or-nothing: a failure anywhere rolls back the entire file, so a
    /// crash can never leave symbols without their refs (partial data
    /// silently corrupts usage evidence). One commit per file is also the
    /// performance contract: the index write path was measured ~96%
    /// fdatasync-bound under per-row autocommits (see `.idxperf/`).
    ///
    /// Same-file reference resolution happens here: a reference whose name
    /// (or qualified name) matches a symbol in THIS file is stored resolved;
    /// otherwise `reference_name` is stored for Pass 2. Duplicate symbol
    /// names within a file resolve to the most recently inserted symbol
    /// (pre-existing last-wins behavior, preserved verbatim).
    ///
    /// On the update path (file already indexed), old refs/symbols/imports
    /// rows are deleted inside the same transaction. Refs must be deleted by
    /// `file_id` explicitly: the symbols delete only cascades refs pointing
    /// at THIS file's symbols, so a top-level ref (`in_symbol_id` NULL) that
    /// is unresolved or resolved to another file's symbol would otherwise
    /// survive and duplicate on every re-index.
    ///
    /// Returns `(file_id, symbol_ids, references_stored)` with `symbol_ids`
    /// in input order.
    #[expect(
        clippy::too_many_arguments,
        reason = "the file write is one atomic unit; bundling into a struct is churn without callers needing it"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "single transaction body; splitting would force the borrow of `tx` across functions"
    )]
    pub fn index_parsed_file_atomic(
        &mut self,
        path: &Path,
        language: Language,
        mtime_ns: i64,
        size_bytes: u64,
        content_hash: Option<u64>,
        symbols: &[SymbolData],
        references: &[ExtractedReference],
        imports: &[ImportStatement],
    ) -> Result<(FileId, Vec<SymbolId>, usize)> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;

        let path_str = normalize_path(path);
        let lang_str = language.as_str();
        let indexed_at = Self::now_ns()?;

        // u64 size_bytes/content_hash reinterpreted as i64 for SQLite storage;
        // round-trips correctly via the reverse cast in row_to_indexed_file
        #[expect(
            clippy::cast_possible_wrap,
            reason = "u64 bit-pattern stored as i64 for SQLite; round-trips via reverse cast"
        )]
        let size_bytes_i64 = size_bytes as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "u64 bit-pattern stored as i64 for SQLite; round-trips via reverse cast"
        )]
        let content_hash_i64 = content_hash.map(|h| h as i64);

        // Try to update first
        let updated = tx.execute(
            "UPDATE files SET language = ?2, mtime_ns = ?3, size_bytes = ?4,
             content_hash = ?5, indexed_at = ?6 WHERE path = ?1",
            params![
                path_str,
                lang_str,
                mtime_ns,
                size_bytes_i64,
                content_hash_i64,
                indexed_at
            ],
        )?;

        let file_id = if updated > 0 {
            // Get the existing ID
            let id: i64 =
                tx.query_row("SELECT id FROM files WHERE path = ?1", [&path_str], |row| {
                    row.get(0)
                })?;

            // Clear old refs, symbols, and imports for this file (re-indexing).
            // See the doc comment for why refs need an explicit delete.
            tx.execute("DELETE FROM refs WHERE file_id = ?1", [id])?;
            tx.execute("DELETE FROM symbols WHERE file_id = ?1", [id])?;
            tx.execute("DELETE FROM imports WHERE file_id = ?1", [id])?;
            id
        } else {
            // Insert new
            tx.execute(
                "INSERT INTO files (path, language, mtime_ns, size_bytes, content_hash, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    path_str,
                    lang_str,
                    mtime_ns,
                    size_bytes_i64,
                    content_hash_i64,
                    indexed_at
                ],
            )?;
            tx.last_insert_rowid()
        };

        // Insert all symbols, capturing generated IDs. Statements are
        // prepare_cached: parsed once per CONNECTION, reused across every
        // file in the indexing run.
        let mut symbol_ids = Vec::with_capacity(symbols.len());
        {
            let mut insert_symbol_stmt = tx.prepare_cached(
                "INSERT INTO symbols (file_id, name, module_path, qualified_name, kind, line, column,
                 end_line, end_column, signature, visibility, parent_symbol_id, is_test)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?;
            let mut insert_attribute_stmt = tx.prepare_cached(
                "INSERT INTO attributes (symbol_id, name, args, line)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;

            for sym in symbols {
                insert_symbol_stmt.execute(params![
                    file_id,
                    sym.name,
                    sym.module_path,
                    sym.qualified_name,
                    sym.kind.as_str(),
                    sym.line,
                    sym.column,
                    sym.span.map(|s| s.end_line()),
                    sym.span.map(|s| s.end_column()),
                    sym.signature,
                    sym.visibility.as_str(),
                    sym.parent_symbol_id.map(SymbolId::as_i64),
                    sym.is_test
                ])?;
                let symbol_id = tx.last_insert_rowid();
                symbol_ids.push(SymbolId::from(symbol_id));

                for attr in sym.attributes {
                    insert_attribute_stmt
                        .execute(params![symbol_id, attr.name, attr.args, attr.line])?;
                }
            }
        }

        // Same-file resolution maps, built from the data just inserted (no
        // read-back query). Duplicate names: last wins, preserved behavior.
        let mut name_to_id: HashMap<&str, SymbolId> = HashMap::new();
        // Macro definitions only: a macro invocation (`foo!()`) must bind to a
        // `macro_rules! foo`, never a same-named fn/type — so it routes through
        // this map instead of `name_to_id` (which a colliding fn could
        // overwrite, forging a phantom `foo!` -> `fn foo` call edge).
        let mut macro_name_to_id: HashMap<&str, SymbolId> = HashMap::new();
        // Data members (properties, events, fields) get the same treatment in
        // the other direction: they are consulted only by `field_access` reads
        // and are kept OUT of the general map, so a same-file `new Exception()`
        // can never bind to a property named `Exception` (tethys-xebx D10;
        // the general kind-aware binding work is tethys-0aqj).
        let mut data_member_name_to_id: HashMap<&str, SymbolId> = HashMap::new();
        let mut span_to_id: HashMap<Span, SymbolId> = HashMap::new();
        for (sym, &id) in symbols.iter().zip(&symbol_ids) {
            if sym.kind.is_data_member() {
                if let Some(prev_id) = data_member_name_to_id.insert(sym.name, id) {
                    trace!(
                        name = %sym.name,
                        new_id = %id,
                        prev_id = %prev_id,
                        "Duplicate data-member name in file, using newer"
                    );
                }
            } else if let Some(prev_id) = name_to_id.insert(sym.name, id) {
                trace!(
                    name = %sym.name,
                    new_id = %id,
                    prev_id = %prev_id,
                    "Duplicate symbol name in file, using newer"
                );
            }
            if sym.kind == SymbolKind::Macro {
                macro_name_to_id.insert(sym.name, id);
            }
            if let Some(span) = sym.span {
                span_to_id.insert(span, id);
            }
        }

        // Insert references with same-file resolution.
        let mut refs_stored = 0usize;
        {
            let mut insert_ref_stmt = tx.prepare_cached(
                "INSERT INTO refs (symbol_id, file_id, kind, line, column, in_symbol_id, reference_name, strategy)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for r in references {
                let qualified_name = build_qualified_name(&r.name, r.path.as_deref());
                // Macro invocations resolve only to macro definitions (see
                // `macro_name_to_id`); member reads prefer data members and
                // fall through to the general map for method-group/delegate
                // reads; every other kind uses the general map only, so
                // calls/constructs can never bind a data member (D10).
                let symbol_id = if r.kind == ExtractedReferenceKind::Macro {
                    macro_name_to_id.get(r.name.as_str()).copied()
                } else if r.kind == ExtractedReferenceKind::Method {
                    // Method calls never bind by bare name at Pass 1: the
                    // receiver decides. Pass 2 handles them — qualified_exact
                    // for derived receivers, unique-or-decline name arms for
                    // unknown ones (tethys-53iv).
                    None
                } else if r.kind == ExtractedReferenceKind::FieldAccess {
                    data_member_name_to_id
                        .get(r.name.as_str())
                        .or_else(|| name_to_id.get(r.name.as_str()))
                        .copied()
                } else {
                    name_to_id
                        .get(r.name.as_str())
                        .or_else(|| name_to_id.get(qualified_name.as_str()))
                        .copied()
                };
                // Unresolved refs keep their name for Pass 2 resolution.
                let reference_name = if symbol_id.is_none() {
                    Some(qualified_name)
                } else {
                    None
                };
                // Provenance (ADR-0003): any insert-time bind — general or
                // macro map — is by definition a same-file bind;
                // unresolved rows stay NULL until a later pass stamps them.
                let strategy = symbol_id.map(|_| ResolutionStrategy::SameFile.as_str());
                let in_symbol_id = r
                    .containing_symbol_span
                    .and_then(|span| span_to_id.get(&span).copied());

                insert_ref_stmt.execute(params![
                    symbol_id.map(SymbolId::as_i64),
                    file_id,
                    r.kind.to_db_kind().as_str(),
                    r.line,
                    r.column,
                    in_symbol_id.map(SymbolId::as_i64),
                    reference_name.as_deref(),
                    strategy
                ])?;
                refs_stored += 1;
            }
        }

        // Insert imports. Stored import format is owned by the language's
        // ModuleResolver. Glob and bare-module imports store "*".
        {
            let mut insert_import_stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO imports (file_id, symbol_name, source_module, alias)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let resolver = get_module_resolver(language);
            for import in imports {
                let source = resolver.join_import(&import.path);
                if import.is_glob || import.imported_names.is_empty() {
                    insert_import_stmt.execute(params![
                        file_id,
                        "*",
                        source,
                        import.alias.as_deref()
                    ])?;
                } else {
                    for name in &import.imported_names {
                        insert_import_stmt.execute(params![
                            file_id,
                            name,
                            source,
                            import.alias.as_deref()
                        ])?;
                    }
                }
            }
        }

        tx.commit()?;
        Ok((FileId::from(file_id), symbol_ids, refs_stored))
    }

    /// Get all files of a specific language.
    ///
    /// Used for language-specific dependency resolution passes.
    pub fn get_files_by_language(&self, language: Language) -> Result<Vec<IndexedFile>> {
        let lang_str = language.as_str();
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {FILES_COLUMNS} FROM files WHERE language = ?1"
        ))?;

        let files = stmt
            .query_map([lang_str], row_to_indexed_file)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(files)
    }

    /// Get all indexed files.
    ///
    /// Used for dependency computation after streaming writes.
    pub fn list_all_files(&self) -> Result<Vec<IndexedFile>> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!("SELECT {FILES_COLUMNS} FROM files ORDER BY path"))?;

        let files = stmt
            .query_map([], row_to_indexed_file)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(files)
    }
}

#[cfg(test)]
mod index_parsed_file_atomic_tests {
    use super::*;
    use crate::db::Index;
    use crate::languages::common::{
        ExtractedAttribute, ExtractedReference, ExtractedReferenceKind, ImportStatement,
    };
    use crate::types::{Language, Span, SymbolKind, Visibility};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open index");
        (dir, index)
    }

    fn sym(name: &str, line: u32, span: Option<Span>) -> SymbolData<'_> {
        SymbolData {
            name,
            module_path: "",
            qualified_name: name,
            kind: SymbolKind::Function,
            line,
            column: 1,
            span,
            signature: None,
            visibility: Visibility::Public,
            parent_symbol_id: None,
            is_test: false,
            attributes: &[],
        }
    }

    fn call_ref(name: &str, line: u32, containing: Option<Span>) -> ExtractedReference {
        ExtractedReference {
            name: name.to_string(),
            kind: ExtractedReferenceKind::Call,
            line,
            column: 1,
            path: None,
            containing_symbol_span: containing,
        }
    }

    /// Shape-complete fixture (plan slice 1): duplicate symbol names,
    /// same-file-resolved / unresolved / top-level refs, glob + aliased +
    /// multi-name imports, attributes. Expected outcomes written in the
    /// plan BEFORE this implementation.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the fixture IS the test: every input shape asserted in one atomic write"
    )]
    fn shape_complete_file_writes_all_rows_with_expected_resolution() {
        let (_dir, mut index) = temp_index();

        let span_a = Span::new(1, 1, 5, 2).expect("span");
        let span_b = Span::new(10, 1, 15, 2).expect("span");
        let attrs = [ExtractedAttribute {
            name: "derive".to_string(),
            args: Some("Clone".to_string()),
            line: 9,
        }];
        let mut dup_late = sym("dup", 10, Some(span_b));
        dup_late.attributes = &attrs;
        let symbols = vec![sym("dup", 1, Some(span_a)), dup_late, sym("only", 20, None)];

        let references = vec![
            // Same-file ref to duplicate name: must attribute to the LAST
            // inserted `dup` (pre-existing last-wins behavior).
            call_ref("dup", 21, Some(span_a)),
            // Unresolved cross-file ref inside a symbol.
            call_ref("external_thing", 3, Some(span_a)),
            // Top-level unresolved ref (no containing span).
            call_ref("TopLevelThing", 30, None),
        ];

        let imports = vec![
            ImportStatement {
                path: vec!["std".into(), "collections".into()],
                imported_names: vec!["HashMap".into(), "HashSet".into()],
                is_glob: false,
                alias: None,
                line: 1,
                is_reexport: false,
            },
            ImportStatement {
                path: vec!["crate".into(), "prelude".into()],
                imported_names: vec![],
                is_glob: true,
                alias: None,
                line: 2,
                is_reexport: false,
            },
            ImportStatement {
                path: vec!["foo".into()],
                imported_names: vec!["Bar".into()],
                is_glob: false,
                alias: Some("Baz".into()),
                line: 3,
                is_reexport: false,
            },
        ];

        let (file_id, symbol_ids, refs_stored) = index
            .index_parsed_file_atomic(
                Path::new("src/shape.rs"),
                Language::Rust,
                123,
                456,
                None,
                &symbols,
                &references,
                &imports,
            )
            .expect("atomic write");

        assert_eq!(symbol_ids.len(), 3);
        assert_eq!(refs_stored, 3);

        let conn = index.connection().expect("conn");
        // Ref to "dup" resolved to the LAST dup symbol (symbol_ids[1]).
        let resolved_target: i64 = conn
            .query_row("SELECT symbol_id FROM refs WHERE line = 21", [], |row| {
                row.get(0)
            })
            .expect("resolved ref");
        assert_eq!(
            resolved_target,
            symbol_ids[1].as_i64(),
            "duplicate-name ref must attribute to the most recently inserted symbol"
        );
        // Unresolved refs keep reference_name; top-level ref has NULL in_symbol_id.
        let (unresolved_count, top_level_unresolved): (i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM refs WHERE symbol_id IS NULL AND reference_name IS NOT NULL),
                   (SELECT COUNT(*) FROM refs WHERE in_symbol_id IS NULL AND reference_name = 'TopLevelThing')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("counts");
        assert_eq!(unresolved_count, 2);
        assert_eq!(top_level_unresolved, 1);
        // Imports: 2 explicit names + 1 glob '*' + 1 aliased = 4 rows.
        let import_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM imports WHERE file_id = ?1",
                [file_id.as_i64()],
                |row| row.get(0),
            )
            .expect("imports");
        assert_eq!(import_rows, 4);
        let glob_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM imports WHERE symbol_name = '*' AND source_module = 'crate::prelude'",
                [],
                |row| row.get(0),
            )
            .expect("glob");
        assert_eq!(glob_rows, 1);
        // Attribute landed on the second dup symbol.
        let attr_sym: i64 = conn
            .query_row(
                "SELECT symbol_id FROM attributes WHERE name = 'derive'",
                [],
                |row| row.get(0),
            )
            .expect("attr");
        assert_eq!(attr_sym, symbol_ids[1].as_i64());
    }

    /// C4 fence: the complete file write commits EXACTLY once. A buggy
    /// implementation that reintroduces per-row autocommit (e.g., inserting
    /// refs via the standalone `insert_reference`) fails this immediately.
    #[test]
    fn one_commit_per_file_write() {
        let (_dir, mut index) = temp_index();

        let commits = Arc::new(AtomicUsize::new(0));
        {
            let counter = Arc::clone(&commits);
            let conn = index.connection().expect("conn");
            conn.commit_hook(Some(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                false // do not abort the commit
            }));
        }

        let symbols = vec![sym("a", 1, None), sym("b", 2, None)];
        let references = vec![call_ref("a", 3, None), call_ref("x", 4, None)];
        let imports = vec![ImportStatement {
            path: vec!["std".into()],
            imported_names: vec!["Thing".into()],
            is_glob: false,
            alias: None,
            line: 1,
            is_reexport: false,
        }];

        index
            .index_parsed_file_atomic(
                Path::new("src/one_commit.rs"),
                Language::Rust,
                1,
                1,
                None,
                &symbols,
                &references,
                &imports,
            )
            .expect("atomic write");

        // Unregister the hook before the index drops.
        index
            .connection()
            .expect("conn")
            .commit_hook(None::<fn() -> bool>);

        assert_eq!(
            commits.load(Ordering::SeqCst),
            1,
            "file + symbols + refs + imports must commit exactly once"
        );
    }

    /// C5 fence (fresh-file arm): a constraint violation while writing a
    /// brand-new file rolls back the file row and its symbols. The
    /// symbol-insert loop runs BEFORE the refs/imports loops, so this input
    /// aborts before any ref or import is ever written — the zero-row checks
    /// on refs/imports/attributes here only confirm nothing partial leaked,
    /// NOT that those writes are transactional. The re-index arm
    /// (`failed_reindex_preserves_prior_refs_imports_attributes`) is what
    /// actually fences refs/imports/attributes atomicity.
    #[test]
    fn failed_file_write_leaves_no_rows() {
        let (_dir, mut index) = temp_index();

        let good = sym("good", 1, None);
        let mut bad = sym("bad", 2, None);
        // Dangling parent FK — the insert of this symbol must fail.
        bad.parent_symbol_id = Some(SymbolId::from(999_999));
        let symbols = vec![good, bad];
        let references = vec![call_ref("good", 3, None)];

        let result = index.index_parsed_file_atomic(
            Path::new("src/poisoned.rs"),
            Language::Rust,
            1,
            1,
            None,
            &symbols,
            &references,
            &[],
        );
        assert!(result.is_err(), "dangling parent_symbol_id must fail");

        assert!(
            index
                .get_file_id(Path::new("src/poisoned.rs"))
                .expect("query")
                .is_none(),
            "file row must be rolled back"
        );
        let conn = index.connection().expect("conn");
        for table in ["symbols", "refs", "imports", "attributes"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count");
            assert_eq!(count, 0, "{table} must have zero rows after rollback");
        }
    }

    /// C5 fence (re-index arm): refs, imports, and attributes are part of
    /// the SAME atomic unit as the file row and symbols. A first successful
    /// write populates all four child tables; a failing re-index of that
    /// file (dangling parent FK) must roll back its interior DELETEs, leaving
    /// the original rows intact. This is the coverage the fresh-file arm
    /// cannot provide — that path aborts before any ref/import is written, so
    /// only here can a regression that ran the refs/imports DELETE (or their
    /// re-insert) outside the transaction be caught (it would lose the
    /// originals on rollback).
    #[test]
    fn failed_reindex_preserves_prior_refs_imports_attributes() {
        let (_dir, mut index) = temp_index();

        let span = Span::new(1, 1, 2, 2).expect("span");
        let attrs = [ExtractedAttribute {
            name: "derive".to_string(),
            args: Some("Clone".to_string()),
            line: 1,
        }];
        let mut decorated = sym("decorated", 1, Some(span));
        decorated.attributes = &attrs;
        let symbols = vec![decorated];
        // Unresolved cross-file ref inside `decorated` -> a ref row carrying
        // a reference_name and a non-null in_symbol_id.
        let references = vec![call_ref("external_thing", 2, Some(span))];
        let imports = vec![ImportStatement {
            path: vec!["std".into()],
            imported_names: vec!["Thing".into()],
            is_glob: false,
            alias: None,
            line: 1,
            is_reexport: false,
        }];

        index
            .index_parsed_file_atomic(
                Path::new("src/f.rs"),
                Language::Rust,
                1,
                1,
                None,
                &symbols,
                &references,
                &imports,
            )
            .expect("initial write");

        let counts = |index: &Index| -> (i64, i64, i64, i64) {
            let conn = index.connection().expect("conn");
            conn.query_row(
                "SELECT (SELECT COUNT(*) FROM symbols),
                        (SELECT COUNT(*) FROM refs),
                        (SELECT COUNT(*) FROM imports),
                        (SELECT COUNT(*) FROM attributes)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("counts")
        };
        let before = counts(&index);
        assert_eq!(
            before,
            (1, 1, 1, 1),
            "initial write must populate symbols + refs + imports + attributes"
        );

        // Re-index the SAME file, now with a poisoned symbol: the interior
        // DELETEs run, then the symbol re-insert fails on the dangling FK.
        let mut bad = sym("bad", 1, None);
        bad.parent_symbol_id = Some(SymbolId::from(999_999));
        let result = index.index_parsed_file_atomic(
            Path::new("src/f.rs"),
            Language::Rust,
            2,
            2,
            None,
            &[bad],
            &[],
            &[],
        );
        assert!(result.is_err(), "poisoned re-index must fail");

        assert_eq!(
            counts(&index),
            before,
            "failed re-index must preserve the original refs/imports/attributes (interior DELETEs rolled back)"
        );
        assert!(
            index
                .get_file_id(Path::new("src/f.rs"))
                .expect("query")
                .is_some(),
            "the original file row must survive a failed re-index"
        );
    }

    #[test]
    fn build_qualified_name_shapes() {
        assert_eq!(
            build_qualified_name("open", Some(&["Index".to_string()])),
            "Index::open"
        );
        assert_eq!(build_qualified_name("Foo", None), "Foo");
        assert_eq!(build_qualified_name("bar", Some(&[])), "bar");
    }
}

#[cfg(test)]
mod list_all_files_tests {
    use crate::db::Index;
    use crate::types::Language;
    use std::path::Path;
    use tempfile::TempDir;

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("idx.db");
        let index = Index::open(&path).expect("open index");
        (dir, index)
    }

    #[test]
    fn list_all_files_returns_every_indexed_file() {
        let (_dir, mut index) = temp_index();

        for p in ["a.rs", "b.rs", "c.rs"] {
            index
                .upsert_file(Path::new(p), Language::Rust, 0, 0, None)
                .expect("insert file");
        }

        let mut files = index.list_all_files().expect("list_all_files");
        files.sort_by(|a, b| a.path.cmp(&b.path));

        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path.to_str().expect("path is valid UTF-8"), "a.rs");
        assert_eq!(files[1].path.to_str().expect("path is valid UTF-8"), "b.rs");
        assert_eq!(files[2].path.to_str().expect("path is valid UTF-8"), "c.rs");
    }

    #[test]
    fn list_all_files_returns_empty_for_fresh_index() {
        let (_dir, index) = temp_index();
        let files = index.list_all_files().expect("list_all_files");
        assert!(files.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn normalize_path_is_idempotent(s in "[a-zA-Z0-9_./\\\\-]{0,100}") {
            let path = Path::new(&s);
            let once = normalize_path(path);
            let twice = normalize_path(Path::new(&once));
            prop_assert_eq!(&once, &twice, "normalize_path should be idempotent");
        }

        /// On Windows, backslashes are replaced with forward slashes.
        /// On Unix, backslashes are valid filename chars and preserved.
        #[cfg(windows)]
        #[test]
        fn normalize_path_replaces_backslashes_on_windows(s in "[a-zA-Z0-9_./\\\\-]{0,100}") {
            let path = Path::new(&s);
            let normalized = normalize_path(path);
            prop_assert!(
                !normalized.contains('\\'),
                "normalized path should not contain backslashes on Windows: {normalized}"
            );
        }
    }
}
