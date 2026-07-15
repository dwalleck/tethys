//! Indexing pipeline: file discovery, parsing, and database population.
//!
//! This module contains the core indexing methods on [`Tethys`] that handle:
//! - File discovery and workspace scanning
//! - Parallel parsing with tree-sitter
//! - Database writes (both batch and streaming modes)
//! - File-level dependency computation
//! - Pending dependency resolution passes

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, UNIX_EPOCH};

use rayon::prelude::*;
use tracing::{debug, info, trace, warn};

use crate::Tethys;
use crate::batch_writer::BatchWriter;
use crate::db::SymbolData;
use crate::error::{Error, IndexError, IndexErrorKind, Result};
use crate::languages::module_resolver::{ModuleContext, NamespaceMap, get_module_resolver};
use crate::languages::{self, common};
use crate::lsp;
use crate::parallel::{OwnedSymbolData, ParsedFileData};
use crate::types::{
    ArchPhaseResult, FileId, Import, IndexOptions, IndexStats, Language, SymbolKind,
};

/// Pre-built file→crate assignment index for O(depth) ancestor-walk lookups.
///
/// Shared by [`Tethys::run_architecture_phase`] and
/// [`Tethys::build_file_crate_map`] (idxperf claim C8): the alternative —
/// `cargo::get_crate_for_file` — costs an O(crates) linear scan plus a
/// `canonicalize()` syscall per file. Skipping the canonicalize here is safe
/// because both `workspace_root` (canonicalized in `Tethys::new`) and each
/// `CrateInfo::path` (canonicalized in crate discovery) are canonical at
/// construction time.
struct CrateIndex<'a> {
    by_path: HashMap<&'a Path, &'a crate::types::CrateInfo>,
}

impl<'a> CrateIndex<'a> {
    fn new(crates: &'a [crate::types::CrateInfo]) -> Self {
        Self {
            by_path: crates.iter().map(|c| (c.path.as_path(), c)).collect(),
        }
    }

    /// Longest-prefix crate match for an absolute file path.
    ///
    /// `Path::ancestors()` yields the path itself first, then progressively
    /// shorter parents, so the first hit IS the longest-prefix match —
    /// matching `get_crate_for_file`'s nested-crate semantics (a file in
    /// `foo-utils/` must map to `foo-utils`, never to a sibling `foo`).
    fn crate_for(&self, abs: &Path) -> Option<&'a crate::types::CrateInfo> {
        abs.ancestors().find_map(|p| self.by_path.get(p).copied())
    }

    /// Longest-prefix crate match for a stored file path.
    ///
    /// Resolves the (workspace-relative) `file_path` to absolute against
    /// `workspace_root` before the ancestor walk, tolerating an
    /// already-absolute stored path. Both callers store the identical
    /// resolution rule here so it can never drift between them.
    fn crate_for_file(
        &self,
        file_path: &Path,
        workspace_root: &Path,
    ) -> Option<&'a crate::types::CrateInfo> {
        let abs = if file_path.is_absolute() {
            file_path.to_path_buf()
        } else {
            workspace_root.join(file_path)
        };
        self.crate_for(&abs)
    }
}

/// A dependency that couldn't be resolved because the target file wasn't indexed yet.
///
/// These are collected during the first indexing pass and resolved in subsequent passes.
#[derive(Debug)]
pub(crate) struct PendingDependency {
    /// The file ID that has the dependency.
    pub(crate) from_file_id: FileId,
    /// The path to the file being depended on (relative to workspace root).
    pub(crate) dep_path: PathBuf,
}

impl PendingDependency {
    /// Asserts struct invariants in debug builds.
    ///
    /// - `dep_path` must not be absolute (it should be relative to workspace root).
    #[cfg(debug_assertions)]
    pub(crate) fn debug_assert_valid(&self) {
        debug_assert!(
            !self.dep_path.is_absolute(),
            "dep_path should not be absolute: {}",
            self.dep_path.display()
        );
    }

    /// No-op in release builds.
    #[cfg(not(debug_assertions))]
    #[inline]
    pub(crate) fn debug_assert_valid(&self) {}
}

/// Reference names loaded from stored rows for used-import corroboration in
/// streaming mode (`compute_dependencies_from_stored`).
///
/// `all` holds every unresolved `reference_name`; `reexport` is the subset
/// from `Reexport`-kind refs, which carry the ORIGINAL name of the
/// re-exported item (tethys-v1w8) and therefore corroborate an aliased
/// re-export import that the bound-name lookup misses (tethys-sp24).
struct StoredRefNames {
    all: Vec<String>,
    reexport: Vec<String>,
}

#[expect(
    clippy::missing_errors_doc,
    reason = "error docs deferred to avoid churn during active development"
)]
impl Tethys {
    /// Index all source files in the workspace.
    ///
    /// Uses deferred dependency resolution to handle circular dependencies:
    /// 1. First pass: Index all files, queue dependencies that can't resolve
    /// 2. Resolution passes: Retry pending dependencies until no progress
    ///
    /// For LSP-based resolution, use [`Self::index_with_options`] with
    /// [`IndexOptions::with_lsp()`].
    pub fn index(&mut self) -> Result<IndexStats> {
        self.index_with_options(IndexOptions::default())
    }

    /// Index all source files with custom options.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration for the indexing process, including whether
    ///   to use LSP for additional resolution.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::{Tethys, IndexOptions};
    /// use std::path::Path;
    ///
    /// let mut tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    ///
    /// // Index with LSP refinement enabled
    /// let stats = tethys.index_with_options(IndexOptions::with_lsp())?;
    /// println!("Resolved {} references via LSP", stats.total_lsp_resolved());
    /// # Ok::<(), tethys::Error>(())
    /// ```
    #[expect(
        clippy::too_many_lines,
        reason = "orchestration method with sequential indexing phases"
    )]
    pub fn index_with_options(&mut self, options: IndexOptions) -> Result<IndexStats> {
        let start = Instant::now();
        let mut files_indexed = 0;
        let mut symbols_found = 0;
        let mut references_found = 0;
        let mut files_skipped = 0;
        let mut directories_skipped = Vec::new();
        let mut errors = Vec::new();
        let mut pending: Vec<PendingDependency> = Vec::new();

        // Walk the workspace and find source files
        let all_files = self.discover_files(&mut directories_skipped)?;

        // Filter to supported languages and count skipped files
        let source_files: Vec<(PathBuf, Language)> = all_files
            .into_iter()
            .filter_map(|file_path| {
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if let Some(language) = Language::from_extension(ext) {
                    Some((file_path, language))
                } else {
                    files_skipped += 1;
                    None
                }
            })
            .collect();

        let total_files = source_files.len();
        let workspace_root = self.workspace_root.clone();

        // Orphan-cleanup pass (tethys-dhxo): purge rows for files deleted
        // from disk since their last index BEFORE any write/dependency pass.
        // Nothing else ever deletes them — the files-table DELETE logic only
        // fires when an existing file is re-indexed — and streaming mode's
        // `compute_all_dependencies` walks every DB file, so a surviving
        // orphan re-inserts `file_deps` edges from its stale stored imports
        // and refs. FK cascades take the orphan's dependent rows with it.
        let purged_orphans = self.purge_orphan_files(&source_files)?;
        if purged_orphans > 0 {
            info!(
                purged_orphans,
                "Removed index rows for files deleted from disk"
            );
        }

        // Clear file_deps before per-file dependency computation so stale rows
        // from prior runs don't accumulate via the UPSERT in
        // `insert_file_dependency`. Mirrors `clear_all_call_edges` at the
        // start of the populate phase below; positioned earlier here because
        // file_deps is written during per-file processing, not post-hoc like
        // call_edges.
        self.db.clear_all_file_deps().map_err(|e| {
            warn!(
                error = %e,
                phase = "pre_clear_file_deps",
                "failed to clear stale file deps before re-index"
            );
            e
        })?;

        if options.use_streaming() {
            // =====================================================================
            // STREAMING MODE: Parse in parallel, write immediately to background thread
            // Memory usage is O(batch_size) instead of O(n)
            // =====================================================================
            info!(
                total_files,
                batch_size = options.streaming_batch_size(),
                "Starting streaming indexing (parse + write in parallel)"
            );

            let batch_writer =
                BatchWriter::new(self.db_path.clone(), options.streaming_batch_size());

            let progress_counter = AtomicUsize::new(0);
            let parse_errors: Mutex<Vec<IndexError>> = Mutex::new(Vec::new());

            // Parse in parallel and send to background writer
            source_files.par_iter().for_each(|(file_path, language)| {
                let current = progress_counter.fetch_add(1, Ordering::Relaxed);
                if current.is_multiple_of(100) {
                    trace!(progress = current, total = total_files, "Parsing files...");
                }

                match Self::parse_file_static(&workspace_root, file_path, *language) {
                    Ok(data) => {
                        batch_writer.send(data);
                    }
                    Err(e) => {
                        let kind = IndexErrorKind::from(&e);
                        match parse_errors.lock() {
                            Ok(mut guard) => {
                                guard.push(IndexError::new(file_path.clone(), kind, e.to_string()));
                            }
                            Err(poisoned) => {
                                tracing::warn!(
                                    file = %file_path.display(),
                                    "Mutex poisoned during error collection, recovering"
                                );
                                poisoned.into_inner().push(IndexError::new(
                                    file_path.clone(),
                                    kind,
                                    e.to_string(),
                                ));
                            }
                        }
                    }
                }
            });

            // Wait for batch writer to finish
            let write_result = batch_writer.finish()?;
            files_indexed = write_result.stats.files_written;
            symbols_found = write_result.stats.symbols_written;
            references_found = write_result.stats.references_written;

            if write_result.stats.files_failed > 0 {
                warn!(
                    files_failed = write_result.stats.files_failed,
                    files_written = write_result.stats.files_written,
                    "Some files failed to write to database during streaming indexing"
                );
            }

            // Collect parse errors
            match parse_errors.into_inner() {
                Ok(parse_errors_vec) => {
                    errors.extend(parse_errors_vec);
                }
                Err(poisoned) => {
                    tracing::warn!(
                        "Mutex was poisoned during parallel parsing, recovering collected errors"
                    );
                    errors.extend(poisoned.into_inner());
                }
            }

            info!(
                files_indexed,
                symbols_found,
                references_found,
                batches = write_result.stats.batches_committed,
                "Streaming indexing complete"
            );

            // Compute file-level dependencies from stored data
            // This is done after all files are written so all file IDs are known
            self.compute_all_dependencies(&mut pending)?;
        } else {
            // =====================================================================
            // BATCH MODE (default): Parse all files in parallel, then write sequentially
            // Memory usage is O(n) where n = number of files
            // =====================================================================
            info!(total_files, "Starting parallel file parsing (Pass 1a)");

            // Phase 1a: Parallel parsing with rayon
            // Use AtomicUsize for thread-safe progress tracking
            let progress_counter = AtomicUsize::new(0);
            let parse_errors: Mutex<Vec<IndexError>> = Mutex::new(Vec::new());

            let parsed_files: Vec<ParsedFileData> = source_files
                .par_iter()
                .filter_map(|(file_path, language)| {
                    let current = progress_counter.fetch_add(1, Ordering::Relaxed);
                    if current.is_multiple_of(100) {
                        trace!(progress = current, total = total_files, "Parsing files...");
                    }

                    match Self::parse_file_static(&workspace_root, file_path, *language) {
                        Ok(data) => Some(data),
                        Err(e) => {
                            let kind = IndexErrorKind::from(&e);
                            // Handle mutex poisoning - we still want to collect errors even if
                            // another thread panicked. PoisonError contains the guard.
                            match parse_errors.lock() {
                                Ok(mut guard) => {
                                    guard.push(IndexError::new(
                                        file_path.clone(),
                                        kind,
                                        e.to_string(),
                                    ));
                                }
                                Err(poisoned) => {
                                    tracing::warn!(
                                        file = %file_path.display(),
                                        "Mutex poisoned during error collection, recovering"
                                    );
                                    poisoned.into_inner().push(IndexError::new(
                                        file_path.clone(),
                                        kind,
                                        e.to_string(),
                                    ));
                                }
                            }
                            None
                        }
                    }
                })
                .collect();

            // Collect parse errors, recovering from mutex poisoning if needed
            match parse_errors.into_inner() {
                Ok(parse_errors_vec) => {
                    errors.extend(parse_errors_vec);
                }
                Err(poisoned) => {
                    tracing::warn!(
                        "Mutex was poisoned during parallel parsing, recovering collected errors"
                    );
                    errors.extend(poisoned.into_inner());
                }
            }

            info!(
                parsed_count = parsed_files.len(),
                error_count = errors.len(),
                "Parallel parsing complete (Pass 1a), starting sequential write (Pass 1b)"
            );

            // Phase 1b: Sequential database writes
            // This must be sequential because rusqlite Connection is not Sync
            for data in &parsed_files {
                match self.write_parsed_file(data, &mut pending) {
                    Ok((sym_count, ref_count)) => {
                        files_indexed += 1;
                        symbols_found += sym_count;
                        references_found += ref_count;
                    }
                    Err(e) => {
                        let kind = IndexErrorKind::from(&e);
                        errors.push(IndexError::new(
                            data.relative_path.clone(),
                            kind,
                            e.to_string(),
                        ));
                    }
                }
            }

            info!(
                files_indexed,
                symbols_found, references_found, "Sequential write complete (Pass 1b)"
            );
        }

        // Resolution passes: retry pending dependencies until stable
        let mut prev_count = pending.len() + 1;
        let mut pass = 0;
        while !pending.is_empty() && pending.len() < prev_count {
            pass += 1;
            let before = pending.len();
            prev_count = before;
            pending = self.resolve_pending(pending)?;
            debug!(
                pass,
                resolved = before - pending.len(),
                remaining = pending.len(),
                "Dependency resolution pass completed"
            );
        }

        // Convert remaining pending to (from_path, dep_path) for reporting
        let unresolved_dependencies: Vec<(PathBuf, PathBuf)> = pending
            .into_iter()
            .filter_map(|p| match self.db.get_file_by_id(p.from_file_id) {
                Ok(Some(f)) => Some((f.path, p.dep_path)),
                Ok(None) => {
                    warn!(
                        file_id = %p.from_file_id,
                        "File not found when building unresolved deps list"
                    );
                    None
                }
                Err(e) => {
                    warn!(
                        file_id = %p.from_file_id,
                        error = %e,
                        "DB error when building unresolved deps list"
                    );
                    None
                }
            })
            .collect();

        // Log unresolved dependencies with actual file paths
        for (from_path, dep_path) in &unresolved_dependencies {
            debug!(
                from_file = %from_path.display(),
                dep_path = %dep_path.display(),
                "Dependency unresolved after all passes (likely external crate)"
            );
        }

        // Pass 2: Resolve cross-file references using import information
        let resolved_refs = self.resolve_cross_file_references()?;
        if resolved_refs > 0 {
            tracing::info!(
                resolved_count = resolved_refs,
                "Resolved cross-file references"
            );
        }

        // Pass 3: LSP-based resolution (optional)
        // Resolve each language separately using its appropriate LSP server
        let lsp_sessions = if options.use_lsp() {
            let mut sessions = Vec::new();

            let rust_provider = lsp::AnyProvider::for_language(Language::Rust);
            let rust_result =
                self.resolve_via_lsp(&rust_provider, Language::Rust, options.lsp_timeout_secs())?;
            if rust_result.has_resolutions() {
                tracing::info!(
                    language = "rust",
                    resolved_count = rust_result.resolved_count(),
                    "Resolved references via LSP"
                );
            }
            sessions.push(rust_result);

            let csharp_provider = lsp::AnyProvider::for_language(Language::CSharp);
            let csharp_result = self.resolve_via_lsp(
                &csharp_provider,
                Language::CSharp,
                options.lsp_timeout_secs(),
            )?;
            if csharp_result.has_resolutions() {
                tracing::info!(
                    language = "csharp",
                    resolved_count = csharp_result.resolved_count(),
                    "Resolved references via LSP"
                );
            }
            sessions.push(csharp_result);

            sessions
        } else {
            Vec::new()
        };

        // Drop value refs (tethys-ygjx) and macro-token call refs
        // (tethys-8ym0) that never resolved to an in-crate symbol — they
        // name locals/externals and would otherwise pad the refs table.
        // Must run after all resolution passes, before call-edge population
        // reads the refs table.
        let dropped_unresolved_refs = self.db.drop_unresolved_value_and_macro_call_refs()?;
        if dropped_unresolved_refs > 0 {
            tracing::debug!(
                dropped = dropped_unresolved_refs,
                "Dropped unresolved value/macro_call refs"
            );
        }

        // Populate pre-computed call graph edges after all resolution passes
        self.db.clear_all_call_edges()?;
        let call_edges_count = self.db.populate_call_edges()?;
        if call_edges_count > 0 {
            tracing::debug!(call_edges = call_edges_count, "Populated call graph edges");
        }

        // Derive file-level dependencies from call edges.
        // K-hybrid filter (rivets-3d0s): intra-crate edges always count;
        // cross-crate edges only count when the caller file has an import
        // into the callee file's crate. Filters phantom edges out at the
        // file_deps aggregation step — the resolver still produces the
        // call_edge (e.g., `.len()` resolving to a workspace method on
        // a type the caller never imported), but the aggregation refuses
        // to record it as a file-level dependency without corroborating
        // import evidence.
        let file_crate_map = self.build_file_crate_map()?;
        let file_deps_from_calls = self
            .db
            .populate_file_deps_from_call_edges(&file_crate_map)?;
        if file_deps_from_calls > 0 {
            tracing::debug!(
                file_deps = file_deps_from_calls,
                "Derived file deps from call edges"
            );
        }

        // Update query planner statistics after bulk writes
        self.db.analyze()?;

        let arch_phase = match self.run_architecture_phase() {
            Ok(arch) => {
                tracing::debug!(
                    packages = arch.packages_recorded,
                    files = arch.files_assigned,
                    edges = arch.package_deps_recorded,
                    "architecture phase complete"
                );
                Some(ArchPhaseResult::Completed(arch))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "architecture phase failed; index data is otherwise valid"
                );
                Some(ArchPhaseResult::Failed(e.to_string()))
            }
        };

        Ok(IndexStats {
            files_indexed,
            symbols_found,
            references_found,
            duration: start.elapsed(),
            files_skipped,
            directories_skipped,
            errors,
            unresolved_dependencies,
            lsp_sessions,
            arch_phase,
        })
    }

    /// Build a map of `FileId` -> crate name for every indexed file.
    ///
    /// Used by the K-hybrid filter in
    /// [`crate::db::Index::populate_file_deps_from_call_edges`]
    /// (rivets-3d0s). Cargo-known files use the canonical crate name from
    /// [`crate::types::CrateInfo`] via the shared [`CrateIndex`] ancestor
    /// walk (idxperf claim C8 — no per-file `canonicalize()` syscalls);
    /// orphan files (outside any `Cargo.toml`-known crate) get a
    /// pseudo-crate name prefixed with
    /// [`crate::db::call_edges::ORPHAN_PSEUDO_CRATE_PREFIX`] based on
    /// their top-level directory (e.g., `bruno-examples/types.rs` becomes
    /// `orphan:bruno-examples`; files at the workspace root become
    /// `orphan:<filename>`). The pseudo-crate prefix is centralized as
    /// [`crate::db::ORPHAN_PSEUDO_CRATE_PREFIX`].
    fn build_file_crate_map(&self) -> Result<HashMap<crate::types::FileId, String>> {
        let crate_index = CrateIndex::new(&self.crates);
        let map = self
            .db
            .list_all_files()?
            .into_iter()
            .map(|file| {
                let crate_name = crate_index
                    .crate_for_file(&file.path, &self.workspace_root)
                    .map_or_else(
                        || {
                            let top = file
                                .path
                                .components()
                                .next()
                                .and_then(|c| c.as_os_str().to_str())
                                .unwrap_or("");
                            format!("{}{}", crate::db::ORPHAN_PSEUDO_CRATE_PREFIX, top)
                        },
                        |info| info.name.clone(),
                    );
                (file.id, crate_name)
            })
            .collect();
        Ok(map)
    }

    /// Parse a single file for parallel indexing (Phase 1a).
    ///
    /// This is a static method that can be called from parallel threads.
    /// It reads the file, parses it with tree-sitter, and extracts symbols,
    /// references, and imports without touching the database.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - The workspace root for computing relative paths
    /// * `file_path` - Absolute path to the file to parse
    /// * `language` - The detected language of the file
    ///
    /// # Returns
    ///
    /// `ParsedFileData` containing all extracted information, ready for
    /// database insertion via `BatchWriter`.
    pub(crate) fn parse_file_static(
        workspace_root: &Path,
        file_path: &Path,
        language: Language,
    ) -> Result<ParsedFileData> {
        use std::cell::RefCell;

        // Thread-local parser to avoid re-initialization overhead
        thread_local! {
            static PARSER: RefCell<tree_sitter::Parser> = RefCell::new(tree_sitter::Parser::new());
        }

        // Read file content
        let content = std::fs::read(file_path)?;
        let content_str = std::str::from_utf8(&content)
            .map_err(|_| Error::Parser("file is not valid UTF-8".to_string()))?;

        let lang_support = languages::get_language_support(language);

        // try_borrow_mut prevents panics if code structure changes to allow re-entrant calls
        let tree = PARSER.with(|parser| {
            let mut parser = parser.try_borrow_mut().map_err(|_| {
                Error::Parser("thread-local parser already borrowed (re-entrant call?)".to_string())
            })?;
            parser
                .set_language(&lang_support.tree_sitter_language())
                .map_err(|e| {
                    tracing::error!(
                        language = ?language,
                        file = %file_path.display(),
                        error = %e,
                        "Failed to set parser language"
                    );
                    Error::Parser(format!("failed to set language {language:?}: {e}"))
                })?;
            parser
                .parse(content_str, None)
                .ok_or_else(|| Error::Parser("failed to parse file".to_string()))
        })?;

        let extracted = lang_support.extract_symbols(&tree, content_str.as_bytes());
        let imports = lang_support.extract_imports(&tree, content_str.as_bytes());
        let references = lang_support.extract_references(&tree, content_str.as_bytes());

        let metadata = std::fs::metadata(file_path)?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "nanosecond timestamp fits in i64 until year 2262"
        )]
        let mtime_ns = match metadata.modified() {
            Ok(mtime) => match mtime.duration_since(UNIX_EPOCH) {
                Ok(duration) => duration.as_nanos() as i64,
                Err(e) => {
                    debug!(path = %file_path.display(), error = %e, "File modification time before Unix epoch, using 0");
                    0
                }
            },
            Err(e) => {
                debug!(path = %file_path.display(), error = %e, "Cannot read file modification time, using 0");
                0
            }
        };
        let size_bytes = metadata.len();

        // Convert extracted symbols to owned versions.
        //
        // Consume `extracted` via into_iter so name, signature, and attributes
        // can be moved into OwnedSymbolData rather than cloned. The attributes
        // Vec in particular can be large (one row per #[...] on the symbol).
        let symbols: Vec<OwnedSymbolData> = extracted
            .into_iter()
            .map(|sym| {
                let qualified_name = match &sym.parent_name {
                    Some(parent) => format!("{}::{}", parent, sym.name),
                    None => sym.name.clone(),
                };
                // NOTE: module_path is left empty here because parse_file_static runs in
                // parallel threads without access to the crate list. The module_path is
                // computed later during write_parsed_file (batch mode) or post-indexing
                // for streaming mode. Streaming mode currently does not populate module_path.
                let owned = OwnedSymbolData {
                    name: sym.name,
                    module_path: String::new(),
                    qualified_name,
                    kind: sym.kind,
                    line: sym.line,
                    column: sym.column,
                    span: sym.span,
                    signature: sym.signature,
                    visibility: sym.visibility,
                    parent_symbol_id: None,
                    // Linked to an id against same-file containers during
                    // the insert transaction (parent linkage, tethys-aay4).
                    parent_name: sym.parent_name,
                    is_test: sym.is_test,
                    attributes: sym.attributes,
                };
                owned.debug_assert_valid();
                owned
            })
            .collect();

        // Compute relative path — reject files outside the workspace boundary
        let relative_path = file_path
            .strip_prefix(workspace_root)
            .map_err(|_| {
                Error::Config(format!(
                    "file '{}' is outside workspace root '{}'",
                    file_path.display(),
                    workspace_root.display()
                ))
            })?
            .to_path_buf();

        let parsed = ParsedFileData {
            relative_path,
            language,
            mtime_ns,
            size_bytes,
            symbols,
            references,
            imports,
        };
        parsed.debug_assert_valid();
        Ok(parsed)
    }

    /// Write a single parsed file to the database and compute its dependencies.
    ///
    /// This is Phase 1b of indexing - the sequential database write that must
    /// happen after parallel parsing. The complete write (file row, symbols,
    /// attributes, references with same-file resolution, imports) happens in
    /// ONE transaction via [`crate::db::Index::index_parsed_file_atomic`] —
    /// the per-row autocommit pattern this replaced was ~96% of indexing
    /// wall time (see `.idxperf/probe-findings.md`).
    pub(crate) fn write_parsed_file(
        &mut self,
        data: &ParsedFileData,
        pending: &mut Vec<PendingDependency>,
    ) -> Result<(usize, usize)> {
        // Compute module path for all symbols in this file
        let full_path = self.workspace_root.join(&data.relative_path);
        let module_path = self.compute_module_path_for_file(&full_path);

        // Convert owned symbols to borrowed for insertion, using computed module_path
        let symbol_data: Vec<SymbolData<'_>> = data
            .symbols
            .iter()
            .map(|s| {
                let mut sd = s.as_symbol_data();
                sd.module_path = &module_path;
                sd
            })
            .collect();

        // Insert file, symbols, references, and imports atomically
        let (file_id, _symbol_ids, refs_stored) = self.db.index_parsed_file_atomic(
            &data.relative_path,
            data.language,
            data.mtime_ns,
            data.size_bytes,
            None,
            &symbol_data,
            &data.references,
            &data.imports,
        )?;

        // Compute and store file dependencies (reusing full_path from above)
        self.compute_dependencies(
            &full_path,
            file_id,
            data.language,
            &data.imports,
            &data.references,
            pending,
        )?;

        Ok((data.symbols.len(), refs_stored))
    }

    /// Compute and store file-level dependencies based on use statements and actual references.
    ///
    /// This is L2 dependency detection: we only count a dependency if the imported symbol
    /// is actually used in the code, not just imported.
    ///
    /// Dependencies that can't be resolved (target file not yet indexed) are added to
    /// `pending` for retry in subsequent passes.
    ///
    /// The per-file anchor is derived by
    /// [`ModuleResolver::file_anchor`](crate::languages::module_resolver::ModuleResolver::file_anchor)
    /// and import resolution is dispatched per the file's language. For
    /// Rust files the anchor is the crate's source root (orphan files fall
    /// back to the file's parent directory, where `crate::*` paths have no
    /// valid anchor but `self::`/`super::` — resolved off `current_file`
    /// directly — continue to produce dep edges). Glob imports (including
    /// every C# using-directive) record no edges here; C# file deps derive
    /// from resolved references via the call-edge phase, with cross-bucket
    /// edges corroborated against the caller's usings (see
    /// `Index::populate_file_deps_from_call_edges`).
    fn compute_dependencies(
        &self,
        current_file: &Path,
        file_id: FileId,
        language: Language,
        imports: &[common::ImportStatement],
        refs: &[common::ExtractedReference],
        pending: &mut Vec<PendingDependency>,
    ) -> Result<()> {
        use std::collections::HashSet;

        let resolver = get_module_resolver(language);
        let module_ctx = ModuleContext {
            current_file,
            crates: self.crates(),
            anchor: resolver.file_anchor(current_file, &self.workspace_root, self.crates()),
            namespaces: None,
        };

        // Names referenced in this file: direct names plus first path segments.
        // Shared with unused-import detection via `common::referenced_names`.
        let referenced_names = common::referenced_names(refs);
        // Reexport refs carry the ORIGINAL name (tethys-v1w8), so an aliased
        // re-export import is corroborated by that name, not its alias
        // (tethys-sp24). See `common::reexport_referenced_names`.
        let reexport_names = common::reexport_referenced_names(refs);

        // Track which files we depend on (dedupe)
        let mut depended_files: HashSet<PathBuf> = HashSet::new();

        for import_stmt in imports {
            // Skip glob imports - can't determine what's used
            if import_stmt.is_glob {
                continue;
            }

            // Check if any imported name from this import statement is actually referenced
            let is_used = import_stmt.imported_names.iter().any(|name| {
                let lookup_name = import_stmt.alias.as_ref().unwrap_or(name);
                referenced_names.contains(lookup_name.as_str())
                    || reexport_names.contains(name.as_str())
            });

            // Only record dependency if the import is actually used (L2 behavior)
            if !is_used {
                continue;
            }

            // Resolve the module path to a file
            if let Some(resolved) = resolver.resolve_import_segments(&import_stmt.path, &module_ctx)
            {
                // Make the path relative to workspace root
                let dep_path = self.relative_path(&resolved).to_path_buf();
                depended_files.insert(dep_path);
            }
        }

        // Store dependencies in the database, queueing unresolved ones for later
        for dep_path in depended_files {
            if let Some(dep_file_id) = self.db.get_file_id(&dep_path)? {
                self.db.insert_file_dependency(file_id, dep_file_id)?;
            } else {
                // Target file not indexed yet - queue for resolution pass
                let dep = PendingDependency {
                    from_file_id: file_id,
                    dep_path,
                };
                dep.debug_assert_valid();
                pending.push(dep);
            }
        }

        Ok(())
    }

    /// Compute file-level dependencies for all indexed files from stored data.
    ///
    /// This is used in streaming mode where dependencies cannot be computed during
    /// writing because the batch writer operates in a separate thread without access
    /// to Tethys state (`workspace_root`, module resolution).
    ///
    /// The method iterates over all indexed files, loads their imports and references
    /// from the database, and computes dependencies using the same logic as
    /// `compute_dependencies`.
    fn compute_all_dependencies(&mut self, pending: &mut Vec<PendingDependency>) -> Result<()> {
        let files = self.db.list_all_files()?;
        let file_count = files.len();

        debug!(
            file_count,
            "Computing file-level dependencies from stored data"
        );

        for file in files {
            // Load imports and references from database
            let imports = self.db.get_imports_for_file(file.id)?;
            let refs = self.db.list_references_in_file(file.id)?;

            // Convert database imports to the format compute_dependencies expects
            let import_statements = Self::convert_imports_to_statements(&imports, file.language);

            // Extract just the reference names for dependency checking.
            // Reexport refs carry the ORIGINAL name (tethys-v1w8); collected
            // separately so aliased re-export imports corroborate by that
            // name, mirroring `compute_dependencies` (tethys-sp24).
            let ref_names = StoredRefNames {
                all: refs
                    .iter()
                    .filter_map(|r| r.reference_name.clone())
                    .collect(),
                reexport: refs
                    .iter()
                    .filter(|r| r.kind == crate::types::ReferenceKind::Reexport)
                    .filter_map(|r| r.reference_name.clone())
                    .collect(),
            };

            // Compute dependencies using the converted data
            let full_path = self.workspace_root.join(&file.path);
            self.compute_dependencies_from_stored(
                &full_path,
                file.id,
                file.language,
                &import_statements,
                &ref_names,
                pending,
            )?;
        }

        debug!(
            file_count,
            pending_count = pending.len(),
            "File dependency computation complete"
        );

        Ok(())
    }

    /// Convert database Import records back to `ImportStatement` format.
    ///
    /// This is needed for streaming mode where we need to compute dependencies
    /// from stored data rather than the original parsed data.
    fn convert_imports_to_statements(
        imports: &[Import],
        language: Language,
    ) -> Vec<common::ImportStatement> {
        use std::collections::HashMap;

        // Group imports by source_module to reconstruct ImportStatement
        let mut grouped: HashMap<&str, Vec<&Import>> = HashMap::new();
        for import in imports {
            grouped
                .entry(&import.source_module)
                .or_default()
                .push(import);
        }

        let separator = get_module_resolver(language).import_separator();

        grouped
            .into_iter()
            .map(|(source_module, module_imports)| {
                let path: Vec<String> = source_module.split(separator).map(String::from).collect();
                let is_glob = module_imports.iter().any(|i| i.symbol_name == "*");
                let imported_names: Vec<String> = module_imports
                    .iter()
                    .filter(|i| i.symbol_name != "*")
                    .map(|i| i.symbol_name.clone())
                    .collect();
                let alias = module_imports.first().and_then(|i| i.alias.clone());

                common::ImportStatement {
                    path,
                    imported_names,
                    is_glob,
                    alias,
                    line: 0,            // Not needed for dependency computation
                    is_reexport: false, // Not persisted in the imports table
                }
            })
            .collect()
    }

    /// Compute dependencies from stored import/reference data.
    ///
    /// Similar to `compute_dependencies` but takes pre-processed data rather than
    /// `ExtractedReference` objects. Used in streaming mode. See
    /// [`ModuleResolver::file_anchor`](crate::languages::module_resolver::ModuleResolver::file_anchor)
    /// for the per-file anchor contract.
    fn compute_dependencies_from_stored(
        &self,
        current_file: &Path,
        file_id: FileId,
        language: Language,
        imports: &[common::ImportStatement],
        reference_names: &StoredRefNames,
        pending: &mut Vec<PendingDependency>,
    ) -> Result<()> {
        use std::collections::HashSet;

        let resolver = get_module_resolver(language);
        let module_ctx = ModuleContext {
            current_file,
            crates: self.crates(),
            anchor: resolver.file_anchor(current_file, &self.workspace_root, self.crates()),
            namespaces: None,
        };

        // Build a set of actually referenced names
        let refs_set: HashSet<&str> = reference_names.all.iter().map(String::as_str).collect();
        // Reexport-kind refs carry the ORIGINAL name (tethys-v1w8): they
        // corroborate an aliased re-export import that the bound-name lookup
        // below misses (tethys-sp24).
        let reexport_set: HashSet<&str> = reference_names
            .reexport
            .iter()
            .map(String::as_str)
            .collect();
        let mut depended_files: HashSet<PathBuf> = HashSet::new();

        for import_stmt in imports {
            if import_stmt.is_glob {
                continue;
            }

            // Check if any imported name is actually referenced
            let is_used = import_stmt.imported_names.iter().any(|name| {
                let lookup_name = import_stmt.alias.as_ref().unwrap_or(name);
                refs_set.contains(lookup_name.as_str()) || reexport_set.contains(name.as_str())
            });

            if !is_used {
                continue;
            }

            if let Some(resolved) = resolver.resolve_import_segments(&import_stmt.path, &module_ctx)
            {
                let dep_path = self.relative_path(&resolved).to_path_buf();
                depended_files.insert(dep_path);
            }
        }

        for dep_path in depended_files {
            if let Some(dep_file_id) = self.db.get_file_id(&dep_path)? {
                self.db.insert_file_dependency(file_id, dep_file_id)?;
            } else {
                let dep = PendingDependency {
                    from_file_id: file_id,
                    dep_path,
                };
                dep.debug_assert_valid();
                pending.push(dep);
            }
        }

        Ok(())
    }

    /// Maximum number of namespace symbols to query for C# dependency resolution.
    ///
    /// This limit exists to prevent unbounded queries on very large codebases.
    /// 10,000 covers typical enterprise C# projects (1000-5000 files averaging 2-3
    /// namespaces each). If this limit is reached, some C# dependencies may not
    /// be resolved and a warning is logged.
    const NAMESPACE_QUERY_LIMIT: usize = 10_000;

    /// Build a namespace-to-file map from indexed C# Module symbols.
    ///
    /// This enables C# `using` directive resolution: `using MyApp.Services`
    /// resolves to whichever files declare `namespace MyApp.Services`.
    pub(crate) fn build_namespace_map(&self) -> Result<NamespaceMap> {
        let mut map: NamespaceMap = NamespaceMap::new();

        // Query all Module-kind symbols (namespaces)
        let symbols = self
            .db
            .search_symbols_by_kind(SymbolKind::Module, Self::NAMESPACE_QUERY_LIMIT)?;

        if symbols.len() >= Self::NAMESPACE_QUERY_LIMIT {
            warn!(
                limit = Self::NAMESPACE_QUERY_LIMIT,
                "Namespace query limit reached, some C# dependencies may not be resolved"
            );
        }

        // Fetch all C# file paths upfront to avoid N+1 queries
        let csharp_file_paths: HashMap<FileId, PathBuf> = self
            .db
            .get_files_by_language(Language::CSharp)?
            .into_iter()
            .map(|f| (f.id, f.path))
            .collect();

        for sym in symbols {
            // Only include C# files (Rust modules use different resolution)
            if let Some(path) = csharp_file_paths.get(&sym.file_id) {
                map.entry(sym.name.clone()).or_default().push(path.clone());
            }
        }

        // Sort for determinism (Module symbols arrive in id order, which
        // depends on parallel parse scheduling). NOT deduped: one entry per
        // Module symbol — see the NamespaceMap doc.
        for paths in map.values_mut() {
            paths.sort();
        }

        Ok(map)
    }

    /// Retry resolving pending dependencies.
    ///
    /// Returns dependencies that still couldn't be resolved.
    fn resolve_pending(&self, pending: Vec<PendingDependency>) -> Result<Vec<PendingDependency>> {
        let mut still_pending = Vec::new();

        for p in pending {
            match self.db.get_file_id(&p.dep_path)? {
                Some(dep_file_id) => {
                    self.db
                        .insert_file_dependency(p.from_file_id, dep_file_id)?;
                }
                None => {
                    // Still not found - keep for next pass or final logging
                    still_pending.push(p);
                }
            }
        }

        Ok(still_pending)
    }

    /// Discover source files in the workspace.
    pub(crate) fn discover_files(
        &self,
        directories_skipped: &mut Vec<(PathBuf, String)>,
    ) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        Self::walk_dir(&self.workspace_root, &mut files, directories_skipped)?;
        Ok(files)
    }

    /// Recursively walk a directory, collecting source files.
    ///
    /// Directories that cannot be read (e.g., due to permissions) are tracked
    /// in `directories_skipped` for reporting.
    fn walk_dir(
        dir: &Path,
        files: &mut Vec<PathBuf>,
        directories_skipped: &mut Vec<(PathBuf, String)>,
    ) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    directory = %dir.display(),
                    error = %e,
                    "Cannot read directory, skipping"
                );
                directories_skipped.push((dir.to_path_buf(), e.to_string()));
                return Ok(());
            }
        };

        // Parent name for context-aware exclusions (e.g., `src/bin` is Rust
        // source, not build output).
        let dir_name = dir.file_name().and_then(|n| n.to_str());

        for entry in entries {
            // Explicitly handle entry errors instead of silently skipping with flatten()
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!(
                        directory = %dir.display(),
                        error = %e,
                        "Failed to read directory entry, skipping"
                    );
                    continue;
                }
            };

            let path = entry.path();

            // Skip hidden directories and common build directories
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && (name.starts_with('.') || Self::is_excluded_dir(name, dir_name))
            {
                continue;
            }

            if path.is_dir() {
                Self::walk_dir(&path, files, directories_skipped)?;
            } else if path.is_file() {
                // Check if it's a supported file type
                if let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && Language::from_extension(ext).is_some()
                {
                    files.push(path);
                }
            }
        }

        Ok(())
    }

    /// Check if a directory should be excluded from indexing.
    ///
    /// `parent_name` is the name of the directory containing `name`, used for
    /// context-aware exclusions: `bin` is .NET build output everywhere EXCEPT
    /// under `src`, where it is Cargo's binary-target source directory
    /// (`src/bin/*.rs`). `obj` stays excluded unconditionally — .NET `obj`
    /// directories contain *generated* `.cs` sources that must never be
    /// indexed, and no language convention places real sources there.
    fn is_excluded_dir(name: &str, parent_name: Option<&str>) -> bool {
        match name {
            "bin" => parent_name != Some("src"),
            "target" | "node_modules" | "vendor" | "obj" | "build" | "dist" | "__pycache__" => true,
            _ => false,
        }
    }

    /// Final indexing phase: rebuild `arch_*` tables from current files + `file_deps`.
    /// Returns `ArchStats`, or propagates DB errors. Skips files outside any crate.
    /// Returns `ArchStats::default()` (all zeros) when no Rust crates were discovered.
    pub(crate) fn run_architecture_phase(&self) -> Result<crate::types::ArchStats> {
        use crate::db::PackageInsert;
        use crate::types::PackageSource;

        // Non-Rust workspaces have no crates; succeed with all-zero stats
        // rather than returning Err. The upstream call site wraps Ok(_) into
        // Some(ArchPhaseResult::Completed) and Err(_) into Failed, so this
        // path produces Some(Completed(zeros)) — distinct from a real phase
        // failure (Some(Failed)) and from "phase didn't run" (None).
        if self.crates.is_empty() {
            return Ok(crate::types::ArchStats::default());
        }

        // Materialize the relative paths first so `PackageInsert<'_>` can borrow
        // them as `&str` — `Cow::into_owned` drops the borrow that `PackageInsert`
        // requires, so we need an owning backing vec that outlives `packages`.
        let package_paths: Vec<String> = self
            .crates
            .iter()
            .map(|c| self.relative_path(&c.path).to_string_lossy().into_owned())
            .collect();

        let packages: Vec<PackageInsert<'_>> = self
            .crates
            .iter()
            .zip(package_paths.iter())
            .map(|(c, p)| PackageInsert {
                name: c.name.as_str(),
                path: p.as_str(),
                source: PackageSource::Manifest,
            })
            .collect();

        // Map each file to its containing crate via the shared CrateIndex
        // ancestor walk: O(files × depth), zero syscalls.
        let crate_index = CrateIndex::new(&self.crates);

        let mut file_to_package: Vec<(crate::types::FileId, &str)> = Vec::new();
        for file in self.db.list_all_files()? {
            if let Some(info) = crate_index.crate_for_file(&file.path, &self.workspace_root) {
                file_to_package.push((file.id, info.name.as_str()));
            } else {
                tracing::trace!(
                    file = %file.path.display(),
                    "file outside any crate, skipping from architecture phase"
                );
            }
        }

        self.db.repopulate_architecture(&packages, &file_to_package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileId, Import, Language};

    // ========================================================================
    // build_namespace_map Tests (separator-fix follow-on: csharp-ns claim C1)
    // ========================================================================

    /// Index a temp workspace from (path, content) pairs and return Tethys.
    fn indexed_workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Tethys) {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, content) in files {
            let full = dir.path().join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, content).expect("write fixture file");
        }
        let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");
        tethys.index().expect("index");
        (dir, tethys)
    }

    /// Two files declaring the same namespace: values sorted by path
    /// (parallel-parse insertion order must not leak — determinism bug class).
    #[test]
    fn namespace_map_sorts_multi_file_namespaces_by_path() {
        let (_dir, tethys) = indexed_workspace(&[
            ("z/Beta.cs", "namespace Shared { public class B { } }\n"),
            ("a/Alpha.cs", "namespace Shared { public class A { } }\n"),
        ]);
        let map = tethys.build_namespace_map().expect("map");
        let paths: Vec<String> = map["Shared"]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths, vec!["a/Alpha.cs", "z/Beta.cs"]);
    }

    /// Nested block namespaces store un-dotted segment symbols: the dotted
    /// form must NOT be a key (tethys-nnst), while dotted DECLARATIONS and
    /// file-scoped namespaces key exactly.
    #[test]
    fn namespace_map_keys_flat_names_only() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Nested.cs",
                "namespace Outer1 { namespace Inner1 { public class T { } } }\n",
            ),
            ("Dotted.cs", "namespace My.Models { public class W { } }\n"),
            ("Scoped.cs", "namespace My.Scoped;\n\npublic class F { }\n"),
        ]);
        let map = tethys.build_namespace_map().expect("map");
        assert!(
            !map.contains_key("Outer1.Inner1"),
            "nested dotted form must be absent"
        );
        assert!(map.contains_key("Outer1") && map.contains_key("Inner1"));
        assert_eq!(
            map["My.Models"],
            vec![std::path::PathBuf::from("Dotted.cs")]
        );
        assert_eq!(
            map["My.Scoped"],
            vec![std::path::PathBuf::from("Scoped.cs")]
        );
    }

    /// Rust `mod` symbols are Module-kind too — they must be excluded
    /// (language-filter bug class).
    #[test]
    fn namespace_map_excludes_rust_modules() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub mod shared;\n"),
            ("src/shared.rs", "pub fn f() {}\n"),
            ("cs/Thing.cs", "namespace Cs.Side { public class T { } }\n"),
        ]);
        let map = tethys.build_namespace_map().expect("map");
        assert!(map.contains_key("Cs.Side"));
        assert!(
            !map.contains_key("shared"),
            "Rust module must not enter the namespace map"
        );
    }

    /// Empty input shape: a workspace with no C# files yields an empty map.
    #[test]
    fn namespace_map_empty_for_rust_only_workspace() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub fn f() {}\n"),
        ]);
        let map = tethys.build_namespace_map().expect("map");
        assert!(map.is_empty());
    }

    // ========================================================================
    // build_file_crate_map Tests (idxperf claim C8)
    // ========================================================================

    /// Stress fixture from the idxperf plan (slice 6): overlapping crate
    /// name prefixes (`foo` vs `foo-utils` — defeats first-prefix-match
    /// bugs), a file nested deep inside a crate, an orphan in a
    /// subdirectory, and an orphan at the workspace root. Expected map
    /// hand-computed before the `CrateIndex` implementation.
    #[test]
    fn fast_crate_map_matches_expected() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"foo\", \"foo-utils\"]\nresolver = \"2\"\n",
            ),
            (
                "foo/Cargo.toml",
                "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("foo/src/lib.rs", "pub fn a() {}\n"),
            ("foo/src/deep/inner.rs", "pub fn b() {}\n"),
            (
                "foo-utils/Cargo.toml",
                "[package]\nname = \"foo-utils\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("foo-utils/src/lib.rs", "pub fn c() {}\n"),
            ("tools/helper.rs", "pub fn d() {}\n"),
            ("loose.rs", "pub fn e() {}\n"),
        ]);

        let map = tethys.build_file_crate_map().expect("crate map");

        let expected = [
            ("foo/src/lib.rs", "foo"),
            ("foo/src/deep/inner.rs", "foo"),
            ("foo-utils/src/lib.rs", "foo-utils"),
            ("tools/helper.rs", "orphan:tools"),
            ("loose.rs", "orphan:loose.rs"),
        ];
        for (path, crate_name) in expected {
            let file_id = tethys
                .db
                .get_file_id(Path::new(path))
                .expect("query")
                .unwrap_or_else(|| panic!("{path} must be indexed"));
            assert_eq!(
                map.get(&file_id).map(String::as_str),
                Some(crate_name),
                "wrong crate assignment for {path}"
            );
        }
        assert_eq!(map.len(), 5, "exactly the five fixture files mapped");
    }

    // ========================================================================
    // is_excluded_dir Tests
    // ========================================================================

    #[test]
    fn is_excluded_dir_excludes_target() {
        assert!(Tethys::is_excluded_dir("target", None));
    }

    #[test]
    fn is_excluded_dir_excludes_node_modules() {
        assert!(Tethys::is_excluded_dir("node_modules", None));
    }

    #[test]
    fn is_excluded_dir_does_not_match_dot_git() {
        // .git is NOT in the exclusion list — it's handled separately by
        // the hidden-directory filter (starts with '.'). Verify it is not
        // matched here so the two filters stay orthogonal.
        assert!(!Tethys::is_excluded_dir(".git", None));
    }

    #[test]
    fn is_excluded_dir_excludes_bin_outside_src() {
        // .NET build output: <project>/bin
        assert!(Tethys::is_excluded_dir("bin", None));
        assert!(Tethys::is_excluded_dir("bin", Some("MyProject")));
    }

    #[test]
    fn is_excluded_dir_allows_bin_under_src() {
        // Cargo binary targets live in src/bin/*.rs — real source, not
        // build output. Excluding it makes every symbol reachable only
        // from those binaries look dead.
        assert!(!Tethys::is_excluded_dir("bin", Some("src")));
    }

    #[test]
    fn is_excluded_dir_excludes_obj_even_under_src() {
        // .NET obj dirs hold GENERATED .cs sources; never index them,
        // regardless of where they appear.
        assert!(Tethys::is_excluded_dir("obj", None));
        assert!(Tethys::is_excluded_dir("obj", Some("src")));
    }

    #[test]
    fn is_excluded_dir_excludes_build() {
        assert!(Tethys::is_excluded_dir("build", None));
    }

    #[test]
    fn is_excluded_dir_excludes_dist() {
        assert!(Tethys::is_excluded_dir("dist", None));
    }

    #[test]
    fn is_excluded_dir_excludes_vendor() {
        assert!(Tethys::is_excluded_dir("vendor", None));
    }

    #[test]
    fn is_excluded_dir_excludes_pycache() {
        assert!(Tethys::is_excluded_dir("__pycache__", None));
    }

    #[test]
    fn is_excluded_dir_allows_src() {
        assert!(!Tethys::is_excluded_dir("src", None));
    }

    #[test]
    fn is_excluded_dir_allows_lib() {
        assert!(!Tethys::is_excluded_dir("lib", None));
    }

    #[test]
    fn is_excluded_dir_allows_tests() {
        assert!(!Tethys::is_excluded_dir("tests", None));
    }

    #[test]
    fn is_excluded_dir_allows_my_module() {
        assert!(!Tethys::is_excluded_dir("my_module", None));
    }

    #[test]
    fn is_excluded_dir_is_case_sensitive() {
        assert!(!Tethys::is_excluded_dir("Target", None));
        assert!(!Tethys::is_excluded_dir("NODE_MODULES", None));
        assert!(!Tethys::is_excluded_dir("Vendor", None));
    }

    #[test]
    fn is_excluded_dir_rejects_empty_string() {
        assert!(!Tethys::is_excluded_dir("", None));
    }

    /// Re-indexing must not grow the refs table. The accumulating shape:
    /// a top-level ref (`in_symbol_id` NULL — here a type alias to an
    /// external type) survives the symbols-delete cascade because nothing
    /// it points at gets deleted, so before the explicit
    /// `DELETE FROM refs WHERE file_id` in `index_file_atomic`, every
    /// re-index inserted a duplicate row next to it.
    #[test]
    fn reindex_does_not_accumulate_refs() {
        let (_dir, mut tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "src/lib.rs",
                // Top-level unresolved type ref (external type, no fn body)
                // plus an ordinary resolved in-function call ref.
                "pub type Alias = ExternalThing;\n\
                 pub fn used() {}\n\
                 pub fn caller() { used(); }\n",
            ),
        ]);

        let first = tethys.db.get_stats().expect("stats").reference_count;
        assert!(first > 0, "fixture must produce at least one ref");

        tethys.index().expect("second index");
        let second = tethys.db.get_stats().expect("stats").reference_count;
        assert_eq!(
            second, first,
            "re-index must not accumulate refs (top-level unresolved refs previously duplicated)"
        );

        tethys.index().expect("third index");
        let third = tethys.db.get_stats().expect("stats").reference_count;
        assert_eq!(
            third, first,
            "ref count must stay stable across N re-indexes"
        );
    }

    /// End-to-end fence for the src/bin fix: a Cargo binary target under
    /// src/bin/ must be discovered and indexed, while a .NET-style bin/
    /// directory at any other level stays excluded.
    #[test]
    fn index_includes_src_bin_but_excludes_other_bin_dirs() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub fn shared() {}\n"),
            ("src/bin/tool.rs", "fn main() { app::shared(); }\n"),
            ("bin/Generated.cs", "namespace Gen { public class G { } }\n"),
        ]);
        assert!(
            tethys
                .db
                .get_file_id(Path::new("src/bin/tool.rs"))
                .expect("query")
                .is_some(),
            "src/bin/tool.rs must be indexed (Cargo binary target)"
        );
        assert!(
            tethys
                .db
                .get_file_id(Path::new("bin/Generated.cs"))
                .expect("query")
                .is_none(),
            "top-level bin/ must remain excluded (.NET build output)"
        );
    }

    /// A macro invocation (`write!(...)`) must NOT resolve to a same-named
    /// `fn write`: macros live in a separate namespace. Before the kind-aware
    /// resolution fix, the Macro-kind ref resolved by bare name to the fn,
    /// forging a phantom `caller -> write` call edge that corrupted
    /// callers/reachable/impact/coupling.
    #[test]
    fn macro_invocation_does_not_bind_to_same_named_fn() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "src/lib.rs",
                "use std::fmt::Write;\n\
                 pub fn write() {}\n\
                 pub fn caller() {\n\
                 \x20   let mut s = String::new();\n\
                 \x20   let _ = write!(s, \"x\");\n\
                 }\n",
            ),
        ]);

        let deps = tethys.get_symbol_dependencies("caller").expect("deps");
        assert!(
            !deps.iter().any(|s| s.name == "write"),
            "macro invocation must not forge a phantom edge to fn write: {:?}",
            deps.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    /// The intended enrichment is preserved: invoking a workspace
    /// `macro_rules!` definition still links the caller to that macro (a real
    /// dependency edge). Macro refs enrich the call graph — they just can't
    /// bind to a same-named non-macro symbol.
    #[test]
    fn macro_invocation_resolves_to_workspace_macro_definition() {
        let (_dir, tethys) = indexed_workspace(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "src/lib.rs",
                "macro_rules! shout { () => {} }\n\
                 pub fn caller() { shout!(); }\n",
            ),
        ]);

        let deps = tethys.get_symbol_dependencies("caller").expect("deps");
        assert!(
            deps.iter()
                .any(|s| s.name == "shout" && s.kind == SymbolKind::Macro),
            "macro invocation should resolve to the workspace macro definition: {:?}",
            deps.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // convert_imports_to_statements Tests
    // ========================================================================

    #[test]
    fn convert_imports_to_statements_empty_imports() {
        let result = Tethys::convert_imports_to_statements(&[], Language::Rust);
        assert!(result.is_empty());
    }

    #[test]
    fn convert_imports_to_statements_single_rust_import() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "HashMap".to_string(),
            source_module: "std::collections".to_string(),
            alias: None,
        }];

        let result = Tethys::convert_imports_to_statements(&imports, Language::Rust);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, vec!["std", "collections"]);
        assert_eq!(result[0].imported_names, vec!["HashMap"]);
        assert!(!result[0].is_glob);
        assert!(result[0].alias.is_none());
    }

    #[test]
    fn convert_imports_to_statements_glob_import() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "*".to_string(),
            source_module: "crate::prelude".to_string(),
            alias: None,
        }];

        let result = Tethys::convert_imports_to_statements(&imports, Language::Rust);

        assert_eq!(result.len(), 1);
        assert!(result[0].is_glob);
        assert!(result[0].imported_names.is_empty());
    }

    #[test]
    fn convert_imports_to_statements_groups_by_source_module() {
        let imports = vec![
            Import {
                file_id: FileId::from(1),
                symbol_name: "HashMap".to_string(),
                source_module: "std::collections".to_string(),
                alias: None,
            },
            Import {
                file_id: FileId::from(1),
                symbol_name: "HashSet".to_string(),
                source_module: "std::collections".to_string(),
                alias: None,
            },
        ];

        let result = Tethys::convert_imports_to_statements(&imports, Language::Rust);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, vec!["std", "collections"]);
        let mut names = result[0].imported_names.clone();
        names.sort();
        assert_eq!(names, vec!["HashMap", "HashSet"]);
    }

    #[test]
    fn convert_imports_to_statements_csharp_uses_dot_separator() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "List".to_string(),
            source_module: "System.Collections.Generic".to_string(),
            alias: None,
        }];

        let result = Tethys::convert_imports_to_statements(&imports, Language::CSharp);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, vec!["System", "Collections", "Generic"]);
    }

    #[test]
    fn convert_imports_to_statements_preserves_alias() {
        let imports = vec![Import {
            file_id: FileId::from(1),
            symbol_name: "HashMap".to_string(),
            source_module: "std::collections".to_string(),
            alias: Some("Map".to_string()),
        }];

        let result = Tethys::convert_imports_to_statements(&imports, Language::Rust);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].alias, Some("Map".to_string()));
    }
}

#[cfg(test)]
mod arch_phase_tests {
    use crate::Tethys;
    use crate::types::ArchPhaseResult;
    use std::fs;
    use tempfile::TempDir;

    fn make_workspace_with_two_crates() -> (TempDir, Tethys) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["crate_a", "crate_b"]
resolver = "2"
"#,
        )
        .expect("write workspace toml");

        fs::create_dir_all(root.join("crate_a/src")).expect("mkdir a");
        fs::write(
            root.join("crate_a/Cargo.toml"),
            r#"[package]
name = "crate_a"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write a toml");
        fs::write(
            root.join("crate_a/src/lib.rs"),
            "pub fn hello() -> String { String::from(\"hi\") }\n",
        )
        .expect("write a lib");

        fs::create_dir_all(root.join("crate_b/src")).expect("mkdir b");
        fs::write(
            root.join("crate_b/Cargo.toml"),
            r#"[package]
name = "crate_b"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write b toml");
        fs::write(
            root.join("crate_b/src/lib.rs"),
            "pub fn world() -> u32 { 42 }\n",
        )
        .expect("write b lib");

        let tethys = Tethys::new(root).expect("Tethys::new");
        (dir, tethys)
    }

    #[test]
    fn architecture_phase_records_packages() {
        let (_dir, mut tethys) = make_workspace_with_two_crates();
        let stats = tethys.index().expect("index");
        match stats.arch_phase {
            Some(ArchPhaseResult::Completed(arch)) => {
                assert_eq!(arch.packages_recorded, 2);
                assert!(arch.files_assigned >= 2);
            }
            _ => panic!("expected arch_phase to be Some(Completed(...))"),
        }
    }

    /// Workspaces with overlapping-prefix crate names (e.g. `foo` and `foo-utils`)
    /// must map each file to the deepest matching crate, not the first prefix match.
    /// The `HashMap` + ancestor-walk strategy gets this for free via exact-key lookup,
    /// but exercising it here locks the contract in.
    #[test]
    fn architecture_phase_handles_overlapping_crate_prefixes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["foo", "foo-utils"]
resolver = "2"
"#,
        )
        .expect("workspace toml");

        for name in ["foo", "foo-utils"] {
            fs::create_dir_all(root.join(format!("{name}/src"))).expect("mkdir");
            fs::write(
                root.join(format!("{name}/Cargo.toml")),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            )
            .expect("crate toml");
            fs::write(root.join(format!("{name}/src/lib.rs")), "pub fn x() {}\n").expect("lib");
        }

        let mut tethys = Tethys::new(root).expect("Tethys::new");
        let stats = tethys.index().expect("index");
        let Some(ArchPhaseResult::Completed(arch)) = stats.arch_phase else {
            panic!("expected Completed arch_phase, got: {:?}", stats.arch_phase);
        };
        assert_eq!(arch.packages_recorded, 2);
        assert_eq!(
            arch.files_assigned, 2,
            "each crate's lib.rs must be assigned to its own crate, not the prefix match"
        );
    }

    #[test]
    fn non_rust_workspace_yields_zero_arch_stats_not_none() {
        // A directory with no Cargo.toml has no crates; discover_crates returns [].
        // The phase should succeed with all-zero ArchStats and no error message.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");
        let stats = tethys.index().expect("index");
        assert!(
            matches!(
                &stats.arch_phase,
                Some(ArchPhaseResult::Completed(arch))
                    if arch.packages_recorded == 0
                        && arch.files_assigned == 0
                        && arch.package_deps_recorded == 0
            ),
            "expected arch_phase to be Some(Completed(all-zeros)) for non-Rust workspace, got: {:?}",
            stats.arch_phase
        );
    }
}
