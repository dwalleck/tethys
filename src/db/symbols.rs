//! Symbol CRUD operations for the Tethys index.

use rusqlite::OptionalExtension;
use rusqlite::params;
use tracing::{debug, trace};

use super::{Index, SYMBOLS_COLUMNS, row_to_symbol};
use crate::error::Result;
use crate::types::{FileId, Symbol, SymbolId, SymbolKind};

/// Parameters for inserting a symbol into the index (test-only).
#[cfg(test)]
pub(crate) struct InsertSymbolParams<'a> {
    pub file_id: FileId,
    pub name: &'a str,
    pub module_path: &'a str,
    pub qualified_name: &'a str,
    pub kind: SymbolKind,
    pub line: u32,
    pub column: u32,
    pub span: Option<crate::types::Span>,
    pub signature: Option<&'a str>,
    pub visibility: crate::types::Visibility,
    pub parent_symbol_id: Option<SymbolId>,
    pub is_test: bool,
}

impl Index {
    /// Insert a symbol, returning the symbol ID.
    #[cfg(test)]
    pub fn insert_symbol(&self, params: &InsertSymbolParams<'_>) -> Result<SymbolId> {
        let conn = self.connection()?;

        conn.execute(
            "INSERT INTO symbols (file_id, name, module_path, qualified_name, kind, line, column,
             end_line, end_column, signature, visibility, parent_symbol_id, is_test)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                params.file_id.as_i64(),
                params.name,
                params.module_path,
                params.qualified_name,
                params.kind.as_str(),
                params.line,
                params.column,
                params.span.map(|s| s.end_line()),
                params.span.map(|s| s.end_column()),
                params.signature,
                params.visibility.as_str(),
                params.parent_symbol_id.map(SymbolId::as_i64),
                params.is_test
            ],
        )?;
        Ok(SymbolId::from(conn.last_insert_rowid()))
    }

    /// List symbols in a file.
    pub fn list_symbols_in_file(&self, file_id: FileId) -> Result<Vec<Symbol>> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE file_id = ?1 ORDER BY line"
        ))?;

        let symbols = stmt
            .query_map([file_id.as_i64()], row_to_symbol)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(symbols)
    }

    /// Search symbols by name pattern.
    pub fn search_symbols(&self, query: &str, limit: usize) -> Result<Vec<Symbol>> {
        if query.is_empty() {
            return Ok(vec![]);
        }

        let pattern = format!("%{query}%");
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols \
             WHERE name LIKE ?1 OR qualified_name LIKE ?1 \
             ORDER BY CASE WHEN name = ?2 THEN 0 ELSE 1 END, length(qualified_name) \
             LIMIT ?3"
        ))?;

        let symbols = stmt
            .query_map(params![pattern, query, limit_i64], row_to_symbol)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(symbols)
    }

    /// Get a symbol by its database ID.
    pub fn get_symbol_by_id(&self, id: SymbolId) -> Result<Option<Symbol>> {
        trace!(symbol_id = %id, "Looking up symbol by ID");
        let conn = self.connection()?;

        conn.query_row(
            &format!("SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE id = ?1"),
            [id.as_i64()],
            row_to_symbol,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Get a symbol by its qualified name (exact match).
    pub fn get_symbol_by_qualified_name(&self, qualified_name: &str) -> Result<Option<Symbol>> {
        trace!(qualified_name = %qualified_name, "Looking up symbol by qualified name");
        let conn = self.connection()?;

        conn.query_row(
            &format!("SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE qualified_name = ?1"),
            [qualified_name],
            row_to_symbol,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Search symbols by their kind (e.g., `SymbolKind::Module` for namespaces).
    ///
    /// This is used to build namespace-to-file maps for C# dependency resolution.
    pub fn search_symbols_by_kind(&self, kind: SymbolKind, limit: usize) -> Result<Vec<Symbol>> {
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE kind = ?1 LIMIT ?2"
        ))?;

        let symbols = stmt
            .query_map(params![kind.as_str(), limit_i64], row_to_symbol)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(symbols)
    }

    /// Get all test symbols in the index.
    ///
    /// Returns all symbols where `is_test = true`, useful for test topology
    /// analysis and "affected tests" queries.
    pub fn get_test_symbols(&self) -> Result<Vec<Symbol>> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE is_test = 1 ORDER BY file_id, line"
        ))?;

        let symbols = stmt
            .query_map([], row_to_symbol)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(symbols)
    }

    /// Search for a symbol by name within a specific file.
    ///
    /// This is used in Pass 2 for cross-file reference resolution. Given a symbol
    /// name and the file ID it should be defined in, find the matching symbol.
    pub fn search_symbol_in_file(&self, name: &str, file_id: FileId) -> Result<Option<Symbol>> {
        trace!(
            symbol_name = %name,
            file_id = %file_id,
            "Searching for symbol in file"
        );

        let conn = self.connection()?;

        // Try exact name match in the specified file
        let result = conn
            .query_row(
                &format!(
                    "SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE name = ?1 AND file_id = ?2 LIMIT 1"
                ),
                params![name, file_id.as_i64()],
                row_to_symbol,
            )
            .optional()?;

        if result.is_some() {
            return Ok(result);
        }

        // Also try matching by qualified_name for nested symbols (e.g., "Struct::method")
        // where the reference might be to the nested name
        let qualified_pattern = format!("%::{name}");
        conn.query_row(
            &format!(
                "SELECT {SYMBOLS_COLUMNS} FROM symbols \
                 WHERE qualified_name LIKE ?1 AND file_id = ?2 LIMIT 1"
            ),
            params![qualified_pattern, file_id.as_i64()],
            row_to_symbol,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Search for a symbol by qualified name in a specific file.
    ///
    /// This is used for resolving qualified references like `Index::open` where
    /// we know the module (file) the type is imported from.
    pub fn search_symbol_by_qualified_name_in_file(
        &self,
        qualified_name: &str,
        file_id: FileId,
    ) -> Result<Option<Symbol>> {
        trace!(
            qualified_name = %qualified_name,
            file_id = %file_id,
            "Searching for symbol by qualified name in file"
        );

        let conn = self.connection()?;

        conn.query_row(
            &format!(
                "SELECT {SYMBOLS_COLUMNS} FROM symbols \
                 WHERE qualified_name = ?1 AND file_id = ?2 LIMIT 1"
            ),
            params![qualified_name, file_id.as_i64()],
            row_to_symbol,
        )
        .optional()
        .map_err(Into::into)
    }

    /// Search for a symbol by name, restricted to files whose path begins
    /// with `path_prefix`.
    ///
    /// The function normalizes the prefix internally (Windows backslashes →
    /// forward slashes) and ensures a trailing separator so that, e.g.,
    /// `"crates/foo"` does not accidentally match `"crates/foobar/"`.
    /// Callers can pass either `"crates/foo"` or `"crates/foo/"`.
    ///
    /// Returns `None` when:
    /// - No symbol with `name` exists under the prefix
    /// - The prefix normalizes to empty or `"/"` (would otherwise match
    ///   everything and silently degrade to the unscoped query — the
    ///   rivets-0gom bug class)
    /// - Multiple candidates exist (genuine intra-prefix ambiguity —
    ///   matches the unique-match semantics of [`Self::search_unique_symbol_by_name`])
    ///
    /// `path_prefix` is treated as a literal LIKE-prefix; `%` or `_` in the
    /// prefix would behave as LIKE wildcards. In practice prefixes come from
    /// crate directory paths and don't contain those characters.
    pub fn search_symbol_by_name_in_path_prefix(
        &self,
        name: &str,
        path_prefix: &str,
    ) -> Result<Option<Symbol>> {
        let normalized = crate::db::normalize_path(std::path::Path::new(path_prefix));
        // Degenerate prefixes that would silently match every file:
        // - "" → LIKE '%' matches everything (workspace-wide; rivets-0gom)
        // - "/" → LIKE '/%' matches nothing on rivets-style relative paths
        //   but is the result of a flat-workspace crate_root and would
        //   silently disable scoping without a useful match
        if normalized.is_empty() || normalized == "/" {
            return Ok(None);
        }
        let bounded_prefix = if normalized.ends_with('/') {
            normalized
        } else {
            format!("{normalized}/")
        };
        let like_pattern = format!("{bounded_prefix}%");

        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols
             WHERE name = ?1
               AND file_id IN (SELECT id FROM files WHERE path LIKE ?2)
             LIMIT 2"
        ))?;
        let mut iter = stmt.query_map(params![name, like_pattern], row_to_symbol)?;
        let Some(first) = iter.next().transpose()? else {
            return Ok(None);
        };
        if iter.next().transpose()?.is_some() {
            debug!(
                symbol_name = %name,
                path_prefix = %bounded_prefix,
                "Refusing ambiguous name match within path prefix (multiple candidates)"
            );
            return Ok(None);
        }
        Ok(Some(first))
    }

    /// Search for a symbol by name across all files, returning the unique
    /// match or `None` (when there are zero or multiple candidates).
    ///
    /// Callers wanting a crate-scoped lookup should use
    /// [`Self::search_symbol_by_name_in_path_prefix`] first; this function
    /// is the last-resort workspace-wide fallback and deliberately refuses
    /// to pick arbitrarily among ambiguous matches.
    pub fn search_unique_symbol_by_name(&self, name: &str) -> Result<Option<Symbol>> {
        trace!(
            symbol_name = %name,
            "Searching for symbol by name (workspace-wide, unique-only)"
        );

        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {SYMBOLS_COLUMNS} FROM symbols WHERE name = ?1 LIMIT 2"
        ))?;
        let mut iter = stmt.query_map([name], row_to_symbol)?;
        let Some(first) = iter.next().transpose()? else {
            return Ok(None);
        };
        if iter.next().transpose()?.is_some() {
            debug!(
                symbol_name = %name,
                first_match_file_id = %first.file_id,
                "Refusing ambiguous workspace-wide name match (multiple candidates)"
            );
            return Ok(None);
        }
        Ok(Some(first))
    }

    /// Up to `limit` symbols named `name` (optionally kind-filtered) declared
    /// in any of `file_paths` — the un-collapsed primitive behind
    /// the resolve.rs candidate union (usgf) and the unique-or-decline
    /// reductions above it. Empty `file_paths` is a documented refusal:
    /// returns an empty `Vec` without touching SQL (load-bearing — an empty
    /// `IN ()` is a syntax error). `file_paths` are chunked (500 per query)
    /// against the `SQLite` host-parameter limit; the `limit` is applied
    /// GLOBALLY across chunks.
    pub fn search_symbols_by_name_in_files(
        &self,
        name: &str,
        kinds: Option<&[SymbolKind]>,
        file_paths: &[std::path::PathBuf],
        limit: usize,
    ) -> Result<Vec<Symbol>> {
        if file_paths.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.connection()?;
        let kind_strs: Option<Vec<&'static str>> =
            kinds.map(|ks| ks.iter().map(SymbolKind::as_str).collect());

        let mut out: Vec<Symbol> = Vec::new();
        for chunk in file_paths.chunks(500) {
            if out.len() >= limit {
                break;
            }
            let path_marks = vec!["?"; chunk.len()].join(", ");
            let kind_clause = match &kind_strs {
                Some(ks) => format!(" AND kind IN ({})", vec!["?"; ks.len()].join(", ")),
                None => String::new(),
            };
            let sql = format!(
                "SELECT {SYMBOLS_COLUMNS} FROM symbols
                 WHERE name = ?
                   AND file_id IN (SELECT id FROM files WHERE path IN ({path_marks})){kind_clause}
                 LIMIT {limit}"
            );
            let params: Vec<String> = std::iter::once(name.to_string())
                .chain(chunk.iter().map(|p| super::files::normalize_path(p)))
                .chain(kind_strs.iter().flatten().map(|k| (*k).to_string()))
                .collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_symbol)?;
            for row in rows {
                out.push(row?);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Up to `limit` members named `name` belonging to type `type_name`,
    /// declared in any of `file_paths`, kind-filtered to `member_kinds` —
    /// the `using static Type;` member-resolution primitive (usgf).
    ///
    /// Scopes to the type via an EXACT `qualified_name = 'Type::name'` match
    /// (the type-scoping handle is `qualified_name`, not `parent_symbol_id`,
    /// which is `None` for functions; probe). Exact match avoids the
    /// `LIKE 'Type::%'` underscore-wildcard hazard for identifiers
    /// containing `_`. Two members sharing `Type::name` (overloads) return
    /// both → the caller declines.
    ///
    /// Empty `file_paths`, empty `type_name`, OR empty `member_kinds` is a
    /// documented refusal: returns an empty `Vec` without SQL (load-bearing —
    /// empty `type_name` would otherwise match every `::name` across types, and
    /// empty `member_kinds` would emit `kind IN ()`, a `SQLite` syntax error).
    pub fn search_type_members_by_name(
        &self,
        name: &str,
        type_name: &str,
        file_paths: &[std::path::PathBuf],
        member_kinds: &[SymbolKind],
        limit: usize,
    ) -> Result<Vec<Symbol>> {
        // Empty type_name is a load-bearing runtime refusal (not a
        // debug_assert): a trailing-dot using like `using static My.Models.;`
        // can reach here with an empty suffix, and `'::name'` would otherwise
        // over-match every member of that name across all types. Empty
        // member_kinds is refused for the same reason empty file_paths is:
        // the `kind IN (...)` clause would become `IN ()`, a SQLite syntax error.
        if file_paths.is_empty() || type_name.is_empty() || member_kinds.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.connection()?;
        let qualified = format!("{type_name}::{name}");
        let kind_marks = vec!["?"; member_kinds.len()].join(", ");

        let mut out: Vec<Symbol> = Vec::new();
        for chunk in file_paths.chunks(500) {
            if out.len() >= limit {
                break;
            }
            let path_marks = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT {SYMBOLS_COLUMNS} FROM symbols
                 WHERE qualified_name = ?
                   AND kind IN ({kind_marks})
                   AND file_id IN (SELECT id FROM files WHERE path IN ({path_marks}))
                 LIMIT {limit}"
            );
            let params: Vec<String> = std::iter::once(qualified.clone())
                .chain(member_kinds.iter().map(|k| k.as_str().to_string()))
                .chain(chunk.iter().map(|p| super::files::normalize_path(p)))
                .collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_symbol)?;
            for row in rows {
                out.push(row?);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Find a symbol at a specific file and line.
    ///
    /// This is used to match LSP `goto_definition` results to our indexed symbols.
    /// The LSP returns file path and line number; we find the symbol defined at that line.
    ///
    /// Returns the symbol whose definition starts at the given line. If multiple symbols
    /// are defined on the same line, returns the one with the lowest column number.
    pub fn find_symbol_at_line(&self, file_id: FileId, line: u32) -> Result<Option<Symbol>> {
        trace!(
            file_id = %file_id,
            line = line,
            "Finding symbol at line"
        );

        let conn = self.connection()?;

        conn.query_row(
            &format!(
                "SELECT {SYMBOLS_COLUMNS} FROM symbols \
                 WHERE file_id = ?1 AND line = ?2 ORDER BY column ASC LIMIT 1"
            ),
            params![file_id.as_i64(), line],
            row_to_symbol,
        )
        .optional()
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod search_in_prefix_tests {
    use super::*;
    use crate::db::Index;
    use crate::types::{Language, Visibility};
    use tempfile::TempDir;

    /// Set up an index with two synthetic crates, each defining a symbol `Foo`.
    /// Returns `(dir, index, foo_in_a_id, foo_in_b_id)`. Dir kept alive by caller.
    fn two_crate_workspace_with_shared_foo() -> (TempDir, Index, SymbolId, SymbolId) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let file_a = index
            .upsert_file(
                std::path::Path::new("crate_a/src/lib.rs"),
                Language::Rust,
                0,
                0,
                None,
            )
            .expect("file a");
        let file_b = index
            .upsert_file(
                std::path::Path::new("crate_b/src/lib.rs"),
                Language::Rust,
                0,
                0,
                None,
            )
            .expect("file b");

        let foo_in_a = index
            .insert_symbol(&InsertSymbolParams {
                file_id: file_a,
                name: "Foo",
                module_path: "crate",
                qualified_name: "crate_a::Foo",
                kind: SymbolKind::Struct,
                line: 1,
                column: 1,
                span: None,
                signature: Some("pub struct Foo"),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("foo in a");
        let foo_in_b = index
            .insert_symbol(&InsertSymbolParams {
                file_id: file_b,
                name: "Foo",
                module_path: "crate",
                qualified_name: "crate_b::Foo",
                kind: SymbolKind::Struct,
                line: 1,
                column: 1,
                span: None,
                signature: Some("pub struct Foo"),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("foo in b");

        (dir, index, foo_in_a, foo_in_b)
    }

    #[test]
    fn returns_same_crate_match_when_caller_in_crate_a() {
        let (_dir, index, foo_in_a, _) = two_crate_workspace_with_shared_foo();
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_a/")
            .expect("query")
            .expect("found");
        assert_eq!(
            result.id, foo_in_a,
            "must return crate_a's Foo, not crate_b's"
        );
    }

    #[test]
    fn returns_same_crate_match_when_caller_in_crate_b() {
        let (_dir, index, _, foo_in_b) = two_crate_workspace_with_shared_foo();
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_b/")
            .expect("query")
            .expect("found");
        assert_eq!(
            result.id, foo_in_b,
            "must return crate_b's Foo, not crate_a's"
        );
    }

    #[test]
    fn returns_none_when_prefix_matches_no_files() {
        let (_dir, index, _, _) = two_crate_workspace_with_shared_foo();
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_c/")
            .expect("query");
        assert!(
            result.is_none(),
            "must return None when no file's path begins with the prefix, got {result:?}"
        );
    }

    #[test]
    fn returns_none_for_empty_prefix() {
        // Empty prefix must NOT degrade to workspace-wide match.
        let (_dir, index, _, _) = two_crate_workspace_with_shared_foo();
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "")
            .expect("query");
        assert!(
            result.is_none(),
            "empty prefix must return None to prevent silent workspace-wide degradation, got {result:?}"
        );
    }

    #[test]
    fn returns_none_for_degenerate_slash_prefix() {
        // A "/" prefix arises from flat-workspace `relative_path` returning
        // Path("") plus the (legacy, now removed) caller-side trailing-slash
        // logic. The function must refuse rather than silently match nothing.
        let (_dir, index, _, _) = two_crate_workspace_with_shared_foo();
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "/")
            .expect("query");
        assert!(result.is_none(), "got {result:?}");
    }

    #[test]
    fn prefix_does_not_match_neighboring_crate_directory() {
        // The trailing-slash invariant: searching under `crate_a/` must NOT
        // match files in `crate_ab/`. The function adds a trailing slash
        // when missing; this test pins that contract.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let file_ab = index
            .upsert_file(
                std::path::Path::new("crate_ab/src/lib.rs"),
                Language::Rust,
                0,
                0,
                None,
            )
            .expect("file ab");
        let foo_in_ab = index
            .insert_symbol(&InsertSymbolParams {
                file_id: file_ab,
                name: "Foo",
                module_path: "crate",
                qualified_name: "crate_ab::Foo",
                kind: SymbolKind::Struct,
                line: 1,
                column: 1,
                span: None,
                signature: Some("pub struct Foo"),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("foo in ab");

        // Searching under crate_a/ must NOT find Foo in crate_ab/.
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_a/")
            .expect("query");
        assert!(
            result.is_none(),
            "prefix 'crate_a/' must not match files in 'crate_ab/', got {result:?}"
        );
        // Sanity: caller asking for crate_ab/ DOES find it.
        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_ab/")
            .expect("query");
        assert_eq!(result.expect("found").id, foo_in_ab);
    }

    #[test]
    fn returns_none_when_intra_prefix_ambiguous() {
        // Two symbols named `Foo` in the SAME prefix (different files).
        // Function must refuse to pick arbitrarily.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        for path in ["crate_a/src/error.rs", "crate_a/src/io.rs"] {
            let file_id = index
                .upsert_file(std::path::Path::new(path), Language::Rust, 0, 0, None)
                .expect("file");
            index
                .insert_symbol(&InsertSymbolParams {
                    file_id,
                    name: "Foo",
                    module_path: "crate",
                    qualified_name: "crate_a::Foo",
                    kind: SymbolKind::Struct,
                    line: 1,
                    column: 1,
                    span: None,
                    signature: Some("pub struct Foo"),
                    visibility: Visibility::Public,
                    parent_symbol_id: None,
                    is_test: false,
                })
                .expect("foo");
        }

        let result = index
            .search_symbol_by_name_in_path_prefix("Foo", "crate_a/")
            .expect("query");
        assert!(
            result.is_none(),
            "two Foo symbols in same crate must return None (refuse to guess), got {result:?}"
        );
    }
}

#[cfg(test)]
mod search_by_name_ambiguity_tests {
    use super::*;
    use crate::db::Index;
    use crate::types::{Language, Visibility};
    use tempfile::TempDir;

    fn insert_bar_in(index: &mut Index, file_path: &str) -> SymbolId {
        let file_id = index
            .upsert_file(std::path::Path::new(file_path), Language::Rust, 0, 0, None)
            .expect("file");
        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name: "Bar",
                module_path: "crate",
                qualified_name: "Bar",
                kind: SymbolKind::Struct,
                line: 1,
                column: 1,
                span: None,
                signature: Some("pub struct Bar"),
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("symbol")
    }

    fn fresh_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open");
        (dir, index)
    }

    #[test]
    fn unique_match_returned() {
        let (_dir, mut index) = fresh_index();
        let bar_id = insert_bar_in(&mut index, "crate_a/src/lib.rs");
        let result = index
            .search_unique_symbol_by_name("Bar")
            .expect("query")
            .expect("unique Bar should resolve");
        assert_eq!(result.id, bar_id);
    }

    #[test]
    fn multiple_matches_return_none_not_arbitrary_winner() {
        let (_dir, mut index) = fresh_index();
        let _bar_a = insert_bar_in(&mut index, "crate_a/src/lib.rs");
        let _bar_b = insert_bar_in(&mut index, "crate_b/src/lib.rs");
        let result = index.search_unique_symbol_by_name("Bar").expect("query");
        assert!(
            result.is_none(),
            "ambiguous Bar (two cross-crate candidates) must return None, got {result:?}"
        );
    }

    #[test]
    fn no_match_returns_none() {
        let (_dir, mut index) = fresh_index();
        insert_bar_in(&mut index, "crate_a/src/lib.rs");
        let result = index
            .search_unique_symbol_by_name("Nonexistent")
            .expect("query");
        assert!(
            result.is_none(),
            "missing name must return None, got {result:?}"
        );
    }

    // ========================================================================
    // file-scoped unique-or-decline Tests (csharp-ns claims C3–C5)
    // ========================================================================

    /// The unique-or-decline reduction the resolve.rs types arm applies over
    /// [`Index::search_symbols_by_name_in_files`] (cap 2). Kept test-only —
    /// the production union does this inline across both arms.
    fn unique_in_files(
        index: &Index,
        name: &str,
        kinds: Option<&[SymbolKind]>,
        file_paths: &[std::path::PathBuf],
    ) -> Result<Option<Symbol>> {
        let mut found = index.search_symbols_by_name_in_files(name, kinds, file_paths, 2)?;
        Ok((found.len() == 1).then(|| found.pop().unwrap()))
    }

    fn insert_sym(index: &mut Index, file_path: &str, name: &str, kind: SymbolKind) -> SymbolId {
        let file_id = index
            .upsert_file(
                std::path::Path::new(file_path),
                Language::CSharp,
                0,
                0,
                None,
            )
            .expect("file");
        index
            .insert_symbol(&InsertSymbolParams {
                file_id,
                name,
                module_path: "",
                qualified_name: name,
                kind,
                line: 1,
                column: 1,
                span: None,
                signature: None,
                visibility: Visibility::Public,
                parent_symbol_id: None,
                is_test: false,
            })
            .expect("symbol")
    }

    fn paths(strs: &[&str]) -> Vec<std::path::PathBuf> {
        strs.iter().map(std::path::PathBuf::from).collect()
    }

    /// Kind-filter bug class: a method sharing the class's name must be
    /// excluded when kinds = types-only. (Both symbols inserted against ONE
    /// upsert — re-upserting a path atomically clears its prior symbols.)
    #[test]
    fn in_files_kind_filter_picks_class_over_same_named_method() {
        let (_dir, mut index) = fresh_index();
        let file_id = index
            .upsert_file(std::path::Path::new("a/W.cs"), Language::CSharp, 0, 0, None)
            .expect("file");
        let insert = |name: &str, kind: SymbolKind| {
            index
                .insert_symbol(&InsertSymbolParams {
                    file_id,
                    name,
                    module_path: "",
                    qualified_name: name,
                    kind,
                    line: 1,
                    column: 1,
                    span: None,
                    signature: None,
                    visibility: Visibility::Public,
                    parent_symbol_id: None,
                    is_test: false,
                })
                .expect("symbol")
        };
        let class_id = insert("Widget", SymbolKind::Class);
        let _method_id = insert("Widget", SymbolKind::Method);
        let result = unique_in_files(
            &index,
            "Widget",
            Some(&[SymbolKind::Class, SymbolKind::Struct]),
            &paths(&["a/W.cs"]),
        )
        .expect("query")
        .expect("class must match through the kind filter");
        assert_eq!(result.id, class_id);
    }

    /// Unique rule: two same-kind matches across the listed files decline.
    #[test]
    fn in_files_two_candidates_decline() {
        let (_dir, mut index) = fresh_index();
        insert_sym(&mut index, "a/W1.cs", "Widget", SymbolKind::Class);
        insert_sym(&mut index, "b/W2.cs", "Widget", SymbolKind::Class);
        let result = unique_in_files(
            &index,
            "Widget",
            Some(&[SymbolKind::Class]),
            &paths(&["a/W1.cs", "b/W2.cs"]),
        )
        .expect("query");
        assert!(result.is_none(), "ambiguity must decline, got {result:?}");
    }

    /// Scope check: a symbol outside the listed files must not match.
    #[test]
    fn in_files_out_of_scope_symbol_is_invisible() {
        let (_dir, mut index) = fresh_index();
        insert_sym(&mut index, "elsewhere/W.cs", "Widget", SymbolKind::Class);
        let result = unique_in_files(&index, "Widget", None, &paths(&["a/W1.cs"])).expect("query");
        assert!(result.is_none());
    }

    /// Documented refusal: empty file set returns None without SQL.
    #[test]
    fn in_files_empty_path_set_declines() {
        let (_dir, mut index) = fresh_index();
        insert_sym(&mut index, "a/W.cs", "Widget", SymbolKind::Class);
        let result = unique_in_files(&index, "Widget", None, &[]).expect("query");
        assert!(result.is_none());
    }

    /// Host-parameter-limit bug class: 1,200 paths must chunk, find the one
    /// match, and aggregate uniqueness ACROSS chunks (a second candidate in
    /// a different chunk must still decline).
    #[test]
    fn in_files_chunking_finds_match_and_aggregates_uniqueness() {
        let (_dir, mut index) = fresh_index();
        let target = insert_sym(&mut index, "dir/file0777.cs", "Widget", SymbolKind::Class);

        let many: Vec<std::path::PathBuf> = (0..1200)
            .map(|i| std::path::PathBuf::from(format!("dir/file{i:04}.cs")))
            .collect();

        let result = unique_in_files(&index, "Widget", Some(&[SymbolKind::Class]), &many)
            .expect("query")
            .expect("single match across chunks must resolve");
        assert_eq!(result.id, target);

        // Second candidate lands in a different chunk (index 0010 vs 0777
        // straddles the 500-path chunk boundary): cross-chunk ambiguity.
        insert_sym(&mut index, "dir/file0010.cs", "Widget", SymbolKind::Class);
        let result =
            unique_in_files(&index, "Widget", Some(&[SymbolKind::Class]), &many).expect("query");
        assert!(
            result.is_none(),
            "cross-chunk ambiguity must decline, got {result:?}"
        );
    }

    // ========================================================================
    // search_type_members_by_name Tests (usgf claim C4 primitive)
    // ========================================================================

    const METHOD_KINDS: &[SymbolKind] = &[SymbolKind::Function, SymbolKind::Method];

    /// Insert several `(type, name, kind)` members into ONE file via a single
    /// upsert. (Re-upserting a path routes through `index_file_atomic`, which
    /// clears the file's prior symbols — so all members of a file must be
    /// inserted against one upsert.)
    fn insert_members(
        index: &mut Index,
        file: &str,
        members: &[(&str, &str, SymbolKind)],
    ) -> Vec<SymbolId> {
        let file_id = index
            .upsert_file(std::path::Path::new(file), Language::CSharp, 0, 0, None)
            .expect("file");
        members
            .iter()
            .map(|(type_name, name, kind)| {
                index
                    .insert_symbol(&InsertSymbolParams {
                        file_id,
                        name,
                        module_path: "",
                        qualified_name: &format!("{type_name}::{name}"),
                        kind: *kind,
                        line: 1,
                        column: 1,
                        span: None,
                        signature: None,
                        visibility: Visibility::Public,
                        parent_symbol_id: None,
                        is_test: false,
                    })
                    .expect("symbol")
            })
            .collect()
    }

    /// Prefix-scoping bug class: `Helper::Zap` and `Other::Zap` both in scope;
    /// `using static Ns.Helper` must match ONLY `Helper::Zap`.
    #[test]
    fn type_members_scope_to_the_type_prefix() {
        let (_dir, mut index) = fresh_index();
        let ids = insert_members(
            &mut index,
            "a/Both.cs",
            &[
                ("Helper", "Zap", SymbolKind::Function),
                ("Other", "Zap", SymbolKind::Function),
            ],
        );
        let hits = index
            .search_type_members_by_name("Zap", "Helper", &paths(&["a/Both.cs"]), METHOD_KINDS, 2)
            .expect("query");
        assert_eq!(hits.len(), 1, "only Helper::Zap, not Other::Zap");
        assert_eq!(hits[0].id, ids[0]);
    }

    /// Kind filter: a non-callable symbol with the same `qualified_name` is excluded.
    #[test]
    fn type_members_kind_filtered() {
        let (_dir, mut index) = fresh_index();
        // A class literally qualified-named "Helper::Inner" (nested type shape)
        // must not be returned by a method lookup.
        insert_members(
            &mut index,
            "a/T.cs",
            &[("Helper", "Inner", SymbolKind::Class)],
        );
        let hits = index
            .search_type_members_by_name("Inner", "Helper", &paths(&["a/T.cs"]), METHOD_KINDS, 2)
            .expect("query");
        assert!(
            hits.is_empty(),
            "class kind excluded by method-kinds filter"
        );
    }

    /// Overloads (two `Helper::Assist`) return both → caller declines.
    #[test]
    fn type_members_overloads_return_both() {
        let (_dir, mut index) = fresh_index();
        insert_members(
            &mut index,
            "a/H.cs",
            &[
                ("Helper", "Assist", SymbolKind::Function),
                ("Helper", "Assist", SymbolKind::Method),
            ],
        );
        let hits = index
            .search_type_members_by_name("Assist", "Helper", &paths(&["a/H.cs"]), METHOD_KINDS, 2)
            .expect("query");
        assert_eq!(hits.len(), 2, "overloads surface as multiple candidates");
    }

    /// Documented refusals: empty files, empty `type_name`, and empty
    /// `member_kinds` each return empty without touching SQL.
    #[test]
    fn type_members_empty_inputs_decline() {
        let (_dir, mut index) = fresh_index();
        insert_members(
            &mut index,
            "a/H.cs",
            &[("Helper", "Assist", SymbolKind::Function)],
        );
        assert!(
            index
                .search_type_members_by_name("Assist", "Helper", &[], METHOD_KINDS, 2)
                .expect("query")
                .is_empty(),
            "empty files → empty"
        );
        assert!(
            index
                .search_type_members_by_name("Assist", "", &paths(&["a/H.cs"]), METHOD_KINDS, 2)
                .expect("query")
                .is_empty(),
            "empty type_name → empty (no '::name' over-match across types)"
        );
        // Empty member_kinds must refuse, not emit `kind IN ()` (a SQLite
        // syntax error). A non-empty match exists, so a missing guard would
        // surface as a query error rather than the empty Vec asserted here.
        assert!(
            index
                .search_type_members_by_name("Assist", "Helper", &paths(&["a/H.cs"]), &[], 2)
                .expect("empty member_kinds must not error")
                .is_empty(),
            "empty member_kinds → empty (no `kind IN ()`)"
        );
    }

    /// Cross-chunk: the one match lands in a late chunk past the 500 boundary.
    #[test]
    fn type_members_chunking_finds_late_match() {
        let (_dir, mut index) = fresh_index();
        let target = insert_members(
            &mut index,
            "dir/file0777.cs",
            &[("Helper", "Assist", SymbolKind::Function)],
        )[0];
        let many: Vec<std::path::PathBuf> = (0..1200)
            .map(|i| std::path::PathBuf::from(format!("dir/file{i:04}.cs")))
            .collect();
        let hits = index
            .search_type_members_by_name("Assist", "Helper", &many, METHOD_KINDS, 2)
            .expect("query");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, target);
    }
}
