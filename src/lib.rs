//! # Tethys: Code Intelligence Cache and Query Interface
//!
//! Tethys provides fast, approximate code intelligence by indexing source files
//! with tree-sitter and caching results in `SQLite`. It is designed for programmatic
//! use by CLI tools, MCP servers, and AI agents.
//!
//! ## Design Philosophy
//!
//! - **Cache, not analyzer** - Tethys indexes and caches; LSPs do the hard semantic work
//! - **Layered accuracy** - Fast approximate results (tree-sitter), optional precision (LSP integration)
//! - **Language extensible** - Start with Rust + C#, design for adding more
//! - **Embeddable** - Library first, CLI second
//! - **Intelligence, not policy** - Reports facts ("12 callers"), not judgments ("too risky")
//!
//! ## Quick Start
//!
//! ```no_run
//! use tethys::Tethys;
//! use std::path::Path;
//!
//! let mut tethys = Tethys::new(Path::new("/path/to/workspace"))?;
//!
//! // Index the workspace
//! let stats = tethys.index()?;
//! println!("Indexed {} files, found {} symbols", stats.files_indexed, stats.symbols_found);
//!
//! // Search for symbols
//! let symbols = tethys.search_symbols("authenticate")?;
//!
//! // Get impact analysis
//! let impact = tethys.get_impact(Path::new("src/auth.rs"), None)?;
//! println!("{} direct dependents", impact.direct_dependents().len());
//! # Ok::<(), tethys::Error>(())
//! ```

mod batch_writer;
pub mod cargo;
mod db;
mod dead_code;
mod error;
mod graph;
mod indexing;
mod languages;
pub mod lsp;
mod parallel;
mod reindex;
mod resolve;
mod resolver;
mod types;
mod unused_imports;

pub use cargo::discover_crates;
pub use db::{
    Demotion, DeprecatedFinding, DeprecatedSymbol, HierarchyDirection, HierarchyNode,
    ReferenceSite, Tier, TypeHierarchy, UntestedFinding, UntestedReport, Via, VisibilityFinding,
};
pub use dead_code::{DeadCodeFinding, DeadCodeReport, DeadCodeSummary};
pub use error::{Error, IndexError, IndexErrorKind, Result};
pub use graph::{FileImpact, FileImpactDependent, SymbolImpact, SymbolImpactCaller};
pub use types::{
    AffectedTestsReport, ArchPhaseResult, ArchStats, CallEdgeSelection, Caller, CallerMode,
    CouplingDetail, CouplingMetrics, CouplingSort, CrateInfo, Cycle, DatabaseStats, FileAnalysis,
    FileId, FunctionSignature, Import, IndexOptions, IndexStats, IndexUpdate, IndexedFile,
    Language, LspCompletedSession, LspOutcome, LspSessionResult, Package, PackageDependency,
    PackageId, PackageSource, PanicKind, PanicPoint, Parameter, ParameterKind, QueryStanding,
    ReachabilityDirection, ReachabilityResult, ReachablePath, Reference, ReferenceKind,
    ResolutionStrategy, Span, StalenessReport, StandingReason, StandingReasonKind, Symbol,
    SymbolId, SymbolKind, UnresolvedRefForLsp, Visibility,
};
pub use unused_imports::{UnusedImport, UnusedImportConfidence};

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use db::Index;
use tracing::{debug, trace, warn};

/// Code intelligence cache and query interface.
///
/// `Tethys` is the main entry point for code intelligence operations. It manages
/// a `SQLite` index of symbols and references extracted from source files using
/// tree-sitter, and provides query methods for searching, dependency analysis,
/// and impact assessment.
pub struct Tethys {
    workspace_root: PathBuf,
    db_path: PathBuf,
    db: Index,
    crates: Vec<CrateInfo>,
}

/// The canonical on-disk location of a workspace's index:
/// `.rivets/index/tethys.db` under the workspace root. Single source for
/// [`Tethys::new`] and [`Tethys::remove_index_files`].
fn index_db_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join(".rivets")
        .join("index")
        .join("tethys.db")
}

/// Convert a `usize` depth to `u32`, saturating at `u32::MAX` with a `warn!`.
///
/// The public API takes `usize`, while the DB layer binds depth as `u32`.
/// This helper bridges the gap. Saturating (rather than
/// truncating) keeps the requested behavior monotone, and the log makes the
/// cap discoverable.
fn saturating_depth_to_u32(depth: usize) -> u32 {
    u32::try_from(depth).unwrap_or_else(|_| {
        warn!(
            requested = depth,
            cap = u32::MAX,
            "max_depth exceeds u32::MAX; saturating to u32::MAX"
        );
        u32::MAX
    })
}

/// Lexically normalize a relative path: drop `.` components and resolve
/// intra-path `..` against preceding segments, so `./src/lib.rs` and
/// `src/../src/lib.rs` match the DB row stored as `src/lib.rs`.
///
/// Purely textual — no filesystem access, so it works for paths that no
/// longer (or never did) exist. Paths that escape upward (a `..` with
/// nothing left to pop) are returned as-is: they cannot name an indexed
/// file, and query standing reports them as `unindexed` downstream.
/// (Distinct from `cargo::sanitize_target_path`, which *rejects* `..` in
/// manifest targets rather than resolving it.)
fn lexically_normalize(path: &Path) -> Cow<'_, Path> {
    use std::path::Component;

    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Cow::Borrowed(path);
                }
            }
            Component::Normal(seg) => parts.push(seg),
            // Unreachable for relative inputs; bail rather than mangle.
            Component::Prefix(_) | Component::RootDir => return Cow::Borrowed(path),
        }
    }

    // Compare the rebuilt form against the raw input rather than trusting a
    // components() scan: components() itself hides interior `.` segments
    // (`src/./lib.rs` iterates as src, lib.rs), so a "looks already normal"
    // fast path would return the unnormalized original string.
    let rebuilt: PathBuf = parts.iter().collect();
    if rebuilt.as_os_str() == path.as_os_str() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(rebuilt)
    }
}

#[expect(
    clippy::missing_errors_doc,
    reason = "error docs deferred to avoid churn during active development"
)]
impl Tethys {
    /// Create a new Tethys instance for a workspace.
    ///
    /// Uses convention-based defaults:
    /// - Excludes hidden directories (starting with `.`)
    /// - Excludes common build directories (`target/`, `node_modules/`, `bin/`, `obj/`, `build/`, `dist/`, `vendor/`, `__pycache__`)
    /// - Database stored at `.rivets/index/tethys.db`
    pub fn new(workspace_root: &Path) -> Result<Self> {
        let workspace_root = workspace_root.canonicalize().map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("workspace root not found: {}", workspace_root.display()),
            ))
        })?;

        let db_path = index_db_path(&workspace_root);
        let db = Index::open(&db_path)?;

        let crates = cargo::discover_crates(&workspace_root);

        debug_assert!(
            {
                let mut sorted: Vec<&str> = crates.iter().map(|c| c.name.as_str()).collect();
                sorted.sort_unstable();
                sorted.windows(2).all(|w| w[0] != w[1])
            },
            "discover_crates returned duplicate crate names; Cargo's manifest layer should prevent this"
        );

        Ok(Self {
            workspace_root,
            db_path,
            db,
            crates,
        })
    }

    /// Delete the on-disk index files (db + WAL/SHM sidecars), if any.
    ///
    /// The rebuild escape hatch for an index whose schema predates the
    /// current binary: `Index::open` refuses outdated schemas with a
    /// "run `tethys index --rebuild`" error, so the rebuild path must be
    /// able to clear the files BEFORE opening — otherwise the guard would
    /// brick its own remedy. No connection exists yet at call time, so
    /// plain file removal is safe (no `SQLite` locks to dance around).
    pub fn remove_index_files(workspace_root: &Path) -> Result<()> {
        Index::remove_db_files(&index_db_path(workspace_root))
    }

    /// Create a Tethys instance with LSP refinement enabled.
    ///
    /// LSP integration is controlled via [`IndexOptions::with_lsp()`] when calling
    /// [`index_with_options()`](Self::index_with_options). The `lsp_command` parameter
    /// is reserved for future use (custom LSP server paths); currently LSP providers
    /// are selected automatically based on language.
    pub fn with_lsp(workspace_root: &Path, _lsp_command: &str) -> Result<Self> {
        Self::new(workspace_root)
    }

    /// Compute the module path for a file in this workspace.
    ///
    /// Returns an empty string if the file is not part of any crate's module tree
    /// (e.g., files in `examples/`, `benches/`, or outside any crate).
    fn compute_module_path_for_file(&self, file_path: &Path) -> String {
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                debug!(
                    file = %file_path.display(),
                    error = %e,
                    "Failed to canonicalize path for module path computation, using original"
                );
                file_path.to_path_buf()
            }
        };

        let Some(crate_info) = cargo::get_crate_for_file(&canonical, &self.crates) else {
            debug!(
                file = %canonical.display(),
                crate_count = self.crates.len(),
                "File not within any known crate"
            );
            return String::new();
        };

        if let Some(module_path) = cargo::compute_module_path(&canonical, crate_info) {
            trace!(
                file = %canonical.display(),
                crate_name = %crate_info.name,
                module_path = %module_path,
                "Computed module path"
            );
            module_path
        } else {
            debug!(
                file = %canonical.display(),
                crate_name = %crate_info.name,
                "File is within crate but not in module tree (examples/benches/tests?)"
            );
            String::new()
        }
    }

    /// Get the path relative to the workspace root.
    ///
    /// Handles symlink differences (e.g., `/var` -> `/private/var` on macOS) by
    /// attempting canonicalization when the initial `strip_prefix` fails on
    /// absolute paths. Relative inputs are the documented "relative to
    /// workspace root" form: they are lexically normalized (`./` dropped,
    /// intra-path `..` resolved) so every spelling of the same file matches
    /// the same DB row (tethys-xetb), and they never warn (tethys-vk3z).
    /// Returns `Cow::Borrowed` for the common fast path, `Cow::Owned` only
    /// when canonicalization or normalization rewrote the path.
    pub(crate) fn relative_path<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        if let Ok(relative) = path.strip_prefix(&self.workspace_root) {
            return Cow::Borrowed(relative);
        }

        if path.is_absolute() {
            // Try canonicalizing to resolve symlinks
            if let Ok(canonical) = path.canonicalize()
                && let Ok(relative) = canonical.strip_prefix(&self.workspace_root)
            {
                return Cow::Owned(relative.to_path_buf());
            }
            // An unindexable input, not an anomaly: query standing reports
            // these as `unindexed` rather than a log line shouting about it.
            debug!(
                path = %path.display(),
                workspace = %self.workspace_root.display(),
                "Absolute path outside workspace root, using as-is"
            );
            return Cow::Borrowed(path);
        }

        lexically_normalize(path)
    }

    // === File Queries ===

    /// Get metadata for an indexed file.
    pub fn get_file(&self, path: &Path) -> Result<Option<IndexedFile>> {
        self.db.get_file(&self.relative_path(path))
    }

    // === Symbol Queries ===

    /// Search for symbols by name (fuzzy/partial matching).
    pub fn search_symbols(&self, query: &str) -> Result<Vec<Symbol>> {
        self.db.search_symbols(query, 100)
    }

    /// List all symbols defined in a file.
    pub fn list_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;
        self.db.list_symbols_in_file(file_id)
    }

    /// Get a symbol by its qualified name (exact match).
    pub fn get_symbol(&self, qualified_name: &str) -> Result<Option<Symbol>> {
        self.db.get_symbol_by_qualified_name(qualified_name)
    }

    /// Get a symbol by its database ID.
    pub fn get_symbol_by_id(&self, id: SymbolId) -> Result<Option<Symbol>> {
        self.db.get_symbol_by_id(id)
    }

    /// Get file information by its database ID.
    ///
    /// Returns the indexed file metadata including its path.
    #[must_use = "returns file info without side effects"]
    pub fn get_file_by_id(&self, id: FileId) -> Result<Option<IndexedFile>> {
        self.db.get_file_by_id(id)
    }

    // === Reference Queries ===

    /// Get all references to a symbol.
    pub fn get_references(&self, qualified_name: &str) -> Result<Vec<Reference>> {
        // First find the symbol by qualified name
        let symbol = self
            .db
            .get_symbol_by_qualified_name(qualified_name)?
            .ok_or_else(|| Error::NotFound(format!("symbol: {qualified_name}")))?;

        // Then get all references to it
        self.db.get_references_to_symbol(symbol.id)
    }

    /// List all outgoing references from a file.
    pub fn list_references_in_file(&self, path: &Path) -> Result<Vec<Reference>> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;

        self.db.list_references_in_file(file_id)
    }

    // === Import Queries ===

    /// List all imports for a file.
    ///
    /// Returns the import statements extracted from the file during indexing.
    /// Each import includes the symbol name, source module, and optional alias.
    pub fn list_imports_in_file(&self, path: &Path) -> Result<Vec<Import>> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;

        self.db.get_imports_for_file(file_id)
    }

    // === Dependency Queries ===

    /// Get files that directly depend on the given file.
    pub fn get_dependents(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;

        let (paths, missing_count) = self.db.get_file_dependent_paths(file_id)?;
        if missing_count > 0 {
            debug!(
                file = %path.display(),
                missing_count,
                "Some dependent file IDs could not be resolved"
            );
        }
        Ok(paths)
    }

    /// Get files that the given file directly depends on.
    pub fn get_dependencies(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;

        let (paths, missing_count) = self.db.get_file_dependency_paths(file_id)?;
        if missing_count > 0 {
            debug!(
                file = %path.display(),
                missing_count,
                "Some dependency file IDs could not be resolved"
            );
        }
        Ok(paths)
    }

    /// Get direct and transitive dependent files at their minimum depth.
    ///
    /// `max_depth` limits transitive traversal depth. `None` uses the
    /// crate-wide default of 50. Zero validates the file and returns no
    /// dependents; one returns direct dependents only. Values larger than
    /// `u32::MAX` are capped (with a `warn!` log) since the underlying SQL CTE
    /// depth is a `u32`.
    pub fn get_impact(&self, path: &Path, max_depth: Option<usize>) -> Result<FileImpact> {
        let file_id = self
            .db
            .get_file_id(&self.relative_path(path))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", path.display())))?;

        let depth = max_depth.map_or(db::DEFAULT_MAX_DEPTH, saturating_depth_to_u32);
        self.db.get_transitive_dependents(file_id, depth)
    }

    /// Get symbols that directly call/use the given symbol.
    ///
    /// Indexed mode reads retained call edges with the requested
    /// [`CallEdgeSelection`]. LSP-refined mode augments all indexed call edges
    /// with language-server findings and deduplicates callers by symbol.
    pub fn get_callers(&self, qualified_name: &str, mode: CallerMode) -> Result<Vec<Caller>> {
        let symbol = self
            .db
            .get_symbol_by_qualified_name(qualified_name)?
            .ok_or_else(|| Error::NotFound(format!("symbol: {qualified_name}")))?;

        match mode {
            CallerMode::Indexed { call_edges } => self.db.get_callers(symbol.id, call_edges),
            CallerMode::LspRefined => self.get_lsp_refined_callers(qualified_name, &symbol),
        }
    }

    /// Get symbols that the given symbol calls/uses.
    pub fn get_symbol_dependencies(&self, qualified_name: &str) -> Result<Vec<Symbol>> {
        let symbol = self
            .db
            .get_symbol_by_qualified_name(qualified_name)?
            .ok_or_else(|| Error::NotFound(format!("symbol: {qualified_name}")))?;

        self.db.get_callees(symbol.id)
    }

    /// Get direct and transitive callers of a symbol at their minimum depth.
    ///
    /// `max_depth` limits transitive traversal depth. `None` uses the
    /// crate-wide default of 50. Zero validates the symbol and returns no
    /// callers; one returns direct callers only. Values larger than `u32::MAX`
    /// are capped (with a `warn!` log) since the underlying SQL CTE depth is a
    /// `u32`.
    ///
    /// `call_edges` selects which retained call edges the traversal follows
    /// at every hop (see [`CallEdgeSelection`]).
    pub fn get_symbol_impact(
        &self,
        qualified_name: &str,
        max_depth: Option<usize>,
        call_edges: CallEdgeSelection,
    ) -> Result<SymbolImpact> {
        let symbol = self
            .db
            .get_symbol_by_qualified_name(qualified_name)?
            .ok_or_else(|| Error::NotFound(format!("symbol: {qualified_name}")))?;

        let depth = max_depth.map_or(db::DEFAULT_MAX_DEPTH, saturating_depth_to_u32);
        let callers = self
            .db
            .get_transitive_callers(symbol.id, depth, call_edges)?;

        Ok(SymbolImpact::new(symbol, callers))
    }

    // === Graph Analysis ===

    /// Detect circular dependencies in the indexed workspace.
    ///
    /// Each returned cycle is canonicalized by its lexicographically smallest
    /// workspace-relative path, follows stored dependency direction, and does
    /// not repeat its first path. Results are deterministically ordered.
    ///
    /// # Errors
    ///
    /// Returns database errors, including `Error::NotFound` for a dangling
    /// indexed dependency endpoint.
    pub fn detect_cycles(&self) -> Result<Vec<Cycle>> {
        self.db.detect_cycles()
    }

    /// Get the shortest dependency path between two files.
    pub fn get_dependency_chain(&self, from: &Path, to: &Path) -> Result<Option<Vec<PathBuf>>> {
        let from_id = self
            .db
            .get_file_id(&self.relative_path(from))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", from.display())))?;
        let to_id = self
            .db
            .get_file_id(&self.relative_path(to))?
            .ok_or_else(|| Error::NotFound(format!("file: {}", to.display())))?;

        let path = self.db.find_dependency_path(from_id, to_id)?;

        Ok(path.map(|p| p.into_files().into_iter().map(|f| f.path).collect()))
    }

    // === Reachability Analysis ===

    /// Get every symbol reachable in one call-graph direction.
    ///
    /// `Forward` follows callees; `Backward` follows callers. Results preserve
    /// BFS discovery order and contain one shortest path per reachable symbol.
    /// `max_depth` limits traversal depth. `None` uses the crate-wide default
    /// of 50; values larger than `u32::MAX` saturate with a warning.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if no symbol matches `qualified_name`.
    /// Database and row-decoding errors are returned unchanged.
    pub fn get_reachable(
        &self,
        qualified_name: &str,
        direction: ReachabilityDirection,
        max_depth: Option<usize>,
    ) -> Result<ReachabilityResult> {
        let source = self
            .db
            .get_symbol_by_qualified_name(qualified_name)?
            .ok_or_else(|| Error::NotFound(format!("symbol: {qualified_name}")))?;
        let depth = max_depth.map_or(db::DEFAULT_MAX_DEPTH, saturating_depth_to_u32);
        let reachable = self.db.get_reachable(source.id, direction, depth)?;

        Ok(ReachabilityResult {
            source,
            reachable,
            max_depth: usize::try_from(depth).unwrap_or(usize::MAX),
            direction,
        })
    }

    /// Get forward reachable symbols: what can this symbol reach?
    ///
    /// Delegates to [`Tethys::get_reachable`] with
    /// [`ReachabilityDirection::Forward`]. The graph is loaded in bulk before
    /// traversal.
    ///
    /// # Arguments
    ///
    /// * `qualified_name` - Qualified name of the symbol to analyze (e.g., `"auth::validate"`)
    /// * `max_depth` - Maximum depth to traverse (None uses default of 50)
    ///
    /// # Errors
    ///
    /// Returns `Error::NotFound` if no symbol matches `qualified_name`.
    /// Returns database errors if the call graph lookup fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let result = tethys.get_forward_reachable("main::run", Some(3))?;
    /// println!("main::run can reach {} symbols", result.reachable_count());
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_forward_reachable(
        &self,
        qualified_name: &str,
        max_depth: Option<usize>,
    ) -> Result<types::ReachabilityResult> {
        self.get_reachable(qualified_name, ReachabilityDirection::Forward, max_depth)
    }

    /// Get backward reachable symbols: who can reach this symbol?
    ///
    /// Delegates to [`Tethys::get_reachable`] with
    /// [`ReachabilityDirection::Backward`]. The graph is loaded in bulk before
    /// traversal.
    ///
    /// # Arguments
    ///
    /// * `qualified_name` - Qualified name of the symbol to analyze (e.g., `"db::query"`)
    /// * `max_depth` - Maximum depth to traverse (None uses default of 50)
    ///
    /// # Errors
    ///
    /// Returns `Error::NotFound` if no symbol matches `qualified_name`.
    /// Returns database errors if the call graph lookup fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let result = tethys.get_backward_reachable("db::query", Some(3))?;
    /// println!("{} symbols can reach db::query", result.reachable_count());
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_backward_reachable(
        &self,
        qualified_name: &str,
        max_depth: Option<usize>,
    ) -> Result<types::ReachabilityResult> {
        self.get_reachable(qualified_name, ReachabilityDirection::Backward, max_depth)
    }

    // === Crate Resolution ===

    /// Get all discovered crates in this workspace.
    pub fn crates(&self) -> &[CrateInfo] {
        &self.crates
    }

    /// Find the crate that contains a given file path.
    ///
    /// Returns the crate whose `path` is a prefix of the given file path.
    /// For workspaces with multiple crates, this finds the most specific match
    /// (longest path). This handles nested crate structures where a file could
    /// technically be under multiple crate directories.
    ///
    /// Returns `None` if the file path cannot be canonicalized or is not under
    /// any discovered crate.
    pub fn get_crate_for_file(&self, file_path: &Path) -> Option<&CrateInfo> {
        let file_path = match file_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                debug!(
                    path = %file_path.display(),
                    error = %e,
                    "Failed to canonicalize path for crate lookup"
                );
                return None;
            }
        };

        self.crates
            .iter()
            .filter(|c| file_path.starts_with(&c.path))
            .max_by_key(|c| c.path.components().count())
    }

    /// Get the crate root directory for a given file path.
    ///
    /// This is a convenience method that returns just the path component
    /// of the containing crate.
    pub fn get_crate_root_for_file(&self, file_path: &Path) -> Option<&Path> {
        self.get_crate_for_file(file_path).map(|c| c.path.as_path())
    }

    // === Database ===

    /// Get path to the `SQLite` database file.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Vacuum the database to reclaim space.
    pub fn vacuum(&self) -> Result<()> {
        self.db.vacuum()
    }

    /// Get statistics about the index database.
    pub fn get_stats(&self) -> Result<types::DatabaseStats> {
        self.db.get_stats()
    }

    // === Test Topology ===

    /// Get all test symbols in the index.
    ///
    /// Returns symbols where `is_test = true`. These are functions/methods
    /// annotated with test framework attributes:
    /// - Rust: `#[test]`, `#[tokio::test]`, `#[rstest]`, etc.
    /// - C#: `[Test]`, `[Fact]`, `[Theory]`, `[TestMethod]`, etc.
    pub fn get_test_symbols(&self) -> Result<Vec<Symbol>> {
        self.db.get_test_symbols()
    }

    /// Get tests that are affected by changes to the specified files.
    ///
    /// This uses the file dependency graph to find test files that depend
    /// (directly or transitively) on the changed files, then returns the
    /// test symbols from those files.
    ///
    /// # Arguments
    ///
    /// * `changed_files` - Paths to files that have changed (relative to workspace root)
    ///
    /// # Returns
    ///
    /// A list of test symbols from files that depend on the changed files.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::{Path, PathBuf};
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let changed = vec![PathBuf::from("src/auth.rs")];
    /// let affected_tests = tethys.get_affected_tests(&changed)?;
    /// for test in affected_tests {
    ///     println!("Run test: {} in {:?}", test.qualified_name, test.file_id);
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_affected_tests(&self, changed_files: &[PathBuf]) -> Result<Vec<Symbol>> {
        // Straight to the traversal: this entry point reports no standing,
        // so it must not pay for classification plus the needs_update()
        // workspace walk only to discard them. Standing-aware callers use
        // get_affected_tests_with_standing.
        self.traverse_affected_tests(changed_files)
    }

    /// Get affected tests together with the query standing — whether the
    /// index can stand behind the result being complete.
    ///
    /// The `tests` list is always the best-effort traversal result, even
    /// when standing is indeterminate (fail-open with signal). Standing is
    /// [`QueryStanding::Indeterminate`] when any of the v1 triggers fire:
    ///
    /// - `unindexed`: a changed file has no index row (including
    ///   unindexable outside-workspace inputs);
    /// - `stale`: a changed file's indexed mtime/size diverge from disk,
    ///   including deleted-on-disk;
    /// - `stale-index`: any indexed file was added/modified/deleted on disk
    ///   since indexing — the dependency graph itself may be missing edges,
    ///   so even current inputs cannot be vouched for. Emitted last.
    ///
    /// Changed-file paths may be workspace-relative in any lexical spelling
    /// (`src/x.rs`, `./src/x.rs`, `a/../b`) or absolute; unknown spellings
    /// degrade to deterministic `unindexed` reasons, never silent skips.
    ///
    /// An empty `changed_files` slice is vacuously [`QueryStanding::Confirmed`]:
    /// if nothing changed, "no affected tests" is complete regardless of
    /// index freshness.
    pub fn get_affected_tests_with_standing(
        &self,
        changed_files: &[PathBuf],
    ) -> Result<AffectedTestsReport> {
        if changed_files.is_empty() {
            return Ok(AffectedTestsReport {
                tests: Vec::new(),
                standing: QueryStanding::Confirmed,
            });
        }

        let mut reasons = self.classify_changed_files(changed_files)?;
        if self.needs_update()? {
            reasons.push(StandingReason {
                kind: StandingReasonKind::StaleIndex,
                path: None,
            });
        }

        let tests = self.traverse_affected_tests(changed_files)?;
        let standing = if reasons.is_empty() {
            QueryStanding::Confirmed
        } else {
            QueryStanding::Indeterminate(reasons)
        };

        Ok(AffectedTestsReport { tests, standing })
    }

    /// Reverse-traversal core shared by the affected-tests entry points:
    /// resolve changed files to ids (unknown files contribute nothing here —
    /// standing classification reports them), walk dependents, filter test
    /// symbols.
    fn traverse_affected_tests(&self, changed_files: &[PathBuf]) -> Result<Vec<Symbol>> {
        use std::collections::HashSet;

        // Get file IDs for the changed files
        let changed_file_ids: Vec<FileId> = changed_files
            .iter()
            .filter_map(|path| {
                let relative = self.relative_path(path);
                match self.db.get_file_id(&relative) {
                    Ok(Some(id)) => Some(id),
                    Ok(None) => {
                        debug!(
                            path = %path.display(),
                            "Changed file not in index, skipping"
                        );
                        None
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Error looking up changed file"
                        );
                        None
                    }
                }
            })
            .collect();

        if changed_file_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Use reverse traversal: find all files that depend on changed files
        // This is O(V+E) total instead of O(T * V) where T = test files
        let mut affected_file_ids: HashSet<FileId> = HashSet::new();

        // Changed files themselves are affected
        affected_file_ids.extend(changed_file_ids.iter().copied());

        // Traverse each changed root independently so one failure does not
        // discard useful dependents from the other roots.
        for &file_id in &changed_file_ids {
            match self.db.get_transitive_dependent_file_ids(file_id) {
                Ok(dependent_file_ids) => {
                    let dependent_count = dependent_file_ids.len();
                    affected_file_ids.extend(dependent_file_ids);
                    debug!(
                        file_id = %file_id,
                        dependent_count,
                        "Found dependents for changed file"
                    );
                }
                Err(e) => {
                    // File might not exist or other error - log and continue
                    warn!(
                        file_id = %file_id,
                        error = %e,
                        "Error getting transitive dependents"
                    );
                }
            }
        }

        // Get all test symbols and filter to affected files
        let all_tests = self.db.get_test_symbols()?;
        let affected_tests: Vec<Symbol> = all_tests
            .into_iter()
            .filter(|test| affected_file_ids.contains(&test.file_id))
            .collect();

        debug!(
            affected_test_count = affected_tests.len(),
            affected_file_count = affected_file_ids.len(),
            changed_file_count = changed_files.len(),
            "Found affected tests"
        );

        Ok(affected_tests)
    }

    // === Panic Points Analysis ===

    /// Get all panic points in the codebase.
    ///
    /// Panic points are `.unwrap()` and `.expect()` calls that could panic at runtime.
    /// Only calls within functions and methods are included.
    ///
    /// # Arguments
    ///
    /// * `include_tests` - If true, include panic points in test code
    /// * `file_filter` - If provided, only return panic points in the specified file
    ///   (path should be relative to workspace root)
    ///
    /// # Returns
    ///
    /// A vector of `PanicPoint` structs, ordered by file path and line number.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    ///
    /// // Get all production panic points
    /// let prod_panics = tethys.get_panic_points(false, None)?;
    /// println!("Found {} panic points in production code", prod_panics.len());
    ///
    /// // Get panic points in a specific file, including tests
    /// let file_panics = tethys.get_panic_points(true, Some("src/lib.rs"))?;
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_panic_points(
        &self,
        include_tests: bool,
        file_filter: Option<&str>,
    ) -> Result<Vec<types::PanicPoint>> {
        self.db.get_panic_points(include_tests, file_filter)
    }

    /// All symbols in the index carrying a Rust `#[deprecated]` or C#
    /// `[Obsolete]` attribute, with `since`/`note` (Rust) or message/error
    /// flag (C#) parsed from the attribute when present.
    ///
    /// Requires a built index — `Tethys::new` errors when the database is
    /// missing, matching the other analysis entry points.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// for dep in tethys.get_deprecated_symbols()? {
    ///     println!("{} {} ({}:{})", dep.kind, dep.name, dep.file, dep.line);
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_deprecated_symbols(&self) -> Result<Vec<DeprecatedSymbol>> {
        self.db.get_deprecated_symbols()
    }

    /// Full deprecated-callers report: every `#[deprecated]` / `[Obsolete]`
    /// symbol with its reference sites, tiered by resolution trustworthiness
    /// (see [`Tier`]).
    ///
    /// An entry with empty `sites` means "clean — no known callers remain".
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// for finding in tethys.get_deprecated_callers()? {
    ///     println!("{}: {} site(s)", finding.symbol.name, finding.sites.len());
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_deprecated_callers(&self) -> Result<Vec<DeprecatedFinding>> {
        self.db.get_deprecated_callers()
    }

    /// Pub Rust items whose observed use is consistent with `pub(crate)`,
    /// tiered by evidence trustworthiness (see [`Tier`] and [`Demotion`]).
    ///
    /// `workspace_closed` asserts that no consumer outside the indexed
    /// workspace exists (nothing is published), lifting the default
    /// root-reachability ceiling that otherwise caps externally nameable
    /// items at [`Tier::Maybe`].
    ///
    /// Caveat: evidence is package-granular, and a package's bin targets,
    /// integration tests, and benches are separate crates from its lib —
    /// their consumption of lib items is invisible here. Asserting
    /// `workspace_closed` on a lib+bin package can therefore promote a
    /// bin-consumed lib item to a false [`Tier::Definite`]; applying
    /// `pub(crate)` to one fails to compile, so the error is loud.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// for finding in tethys.get_visibility_candidates(false)? {
    ///     println!("{} ({:?})", finding.name, finding.tier);
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_visibility_candidates(
        &self,
        workspace_closed: bool,
    ) -> Result<Vec<VisibilityFinding>> {
        self.db.get_visibility_candidates(workspace_closed)
    }

    /// Product functions/methods that no test can reach: multi-root forward
    /// closure from `is_test` symbols over the reference graph, complemented
    /// against product `function`/`method` symbols.
    ///
    /// Reachability, not verification — a reached function may still be
    /// asserted on weakly. Known false-positive sources and the zero-roots
    /// indeterminate posture are documented on [`UntestedReport`] and its
    /// module. The traversal reads `refs`, not `call_edges`, so functions
    /// tested only through macro arguments (`assert_eq!(helper(), 1)`)
    /// count as reached (tethys-8ym0).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let report = tethys.get_untested_code()?;
    /// for finding in &report.findings {
    ///     println!("{}:{} {}", finding.file, finding.line, finding.name);
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_untested_code(&self) -> Result<UntestedReport> {
        self.db.get_untested_code()
    }

    /// Dead-code candidates: non-public, non-test symbols with zero
    /// inbound evidence, tiered by a textual word-boundary scan —
    /// [`Tier::Definite`] findings have no occurrence of their name
    /// anywhere in the indexed corpus outside their own definition span;
    /// [`Tier::Maybe`] findings appear somewhere reference extraction
    /// cannot see (macro token trees, format-string captures, fn-as-value
    /// shapes) and need human verification before deletion.
    ///
    /// Suppression channels (any sign of life removes a candidate):
    /// resolved references in EVERY confidence band including speculative
    /// (ADR-0003), unresolved name matches, trait-impl `inherit` markers
    /// (tethys-j2r1), live descendants for containers, entry points.
    /// Known false-positive sources are documented in the `dead_code`
    /// module docs. Public symbols are never reported — external
    /// consumers are invisible to the index; compose with
    /// `get_visibility_candidates` to shrink the public surface first.
    ///
    /// `limit` truncates `findings` after the (file, line, name) sort;
    /// the summary always carries full-population counts.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let report = tethys.find_dead_code(None)?;
    /// for finding in &report.findings {
    ///     println!("{}:{} {} ({:?})", finding.file, finding.line, finding.name, finding.tier);
    /// }
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn find_dead_code(&self, limit: Option<usize>) -> Result<DeadCodeReport> {
        let candidates = self.db.dead_code_zero_evidence()?;
        let files = self.db.list_all_files()?;
        Ok(dead_code::build_report(
            &self.workspace_root,
            &files,
            candidates,
            limit,
        ))
    }

    /// Walk the type hierarchy from the type named `name` over `inherit`
    /// edges (tethys-j2r1): up = supertypes (implemented traits, extended
    /// bases — external ones as name-only nodes), down = subtypes.
    /// Method-level inherit markers are excluded from both walks.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::{HierarchyDirection, Tethys};
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let h = tethys.get_type_hierarchy("Widget", HierarchyDirection::Both)?;
    /// println!("{} supertypes, {} subtypes", h.up.len(), h.down.len());
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn get_type_hierarchy(
        &self,
        name: &str,
        direction: HierarchyDirection,
    ) -> Result<TypeHierarchy> {
        self.db.get_type_hierarchy(name, direction)
    }

    /// Count panic points grouped by test/production code.
    ///
    /// This is useful for summary statistics without retrieving all the details.
    ///
    /// # Returns
    ///
    /// Returns `(production_count, test_count)`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tethys::Tethys;
    /// use std::path::Path;
    ///
    /// let tethys = Tethys::new(Path::new("/path/to/workspace"))?;
    /// let (prod, test) = tethys.count_panic_points()?;
    /// println!("Production: {prod}, Test: {test}");
    /// # Ok::<(), tethys::Error>(())
    /// ```
    pub fn count_panic_points(&self) -> Result<(usize, usize)> {
        self.db.count_panic_points()
    }

    // === Architecture ===

    /// List all packages discovered during the last index run.
    /// Empty for non-Rust workspaces or before any index has run.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub fn get_packages(&self) -> Result<Vec<types::Package>> {
        self.db.get_packages()
    }

    /// Coupling metrics for every package, sorted per the requested key.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub fn get_coupling_metrics(
        &self,
        sort: types::CouplingSort,
    ) -> Result<Vec<types::CouplingMetrics>> {
        self.db.get_coupling_metrics(sort)
    }

    /// Detailed coupling for one package by exact name.
    /// Returns `Ok(None)` when no package matches.
    ///
    /// # Errors
    /// Returns an error if the database query fails or if the matched
    /// package row has a corrupt `source` column.
    pub fn get_package_coupling(&self, name: &str) -> Result<Option<types::CouplingDetail>> {
        self.db.get_package_coupling(name)
    }
}

#[cfg(test)]
mod arch_api_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn workspace_with_two_crates() -> (TempDir, Tethys) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        )
        .expect("workspace toml");
        for name in ["a", "b"] {
            fs::create_dir_all(root.join(format!("{name}/src"))).expect("mkdir");
            fs::write(
                root.join(format!("{name}/Cargo.toml")),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            )
            .expect("crate toml");
            fs::write(root.join(format!("{name}/src/lib.rs")), "pub fn x() {}\n")
                .expect("crate lib");
        }
        let mut tethys = Tethys::new(root).expect("Tethys::new");
        tethys.index().expect("index");
        (dir, tethys)
    }

    #[test]
    fn get_packages_returns_each_crate() {
        let (_dir, tethys) = workspace_with_two_crates();
        let mut pkgs = tethys.get_packages().expect("packages");
        pkgs.sort_by(|x, y| x.name.cmp(&y.name));
        let names: Vec<_> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn get_coupling_metrics_returns_one_row_per_crate() {
        let (_dir, tethys) = workspace_with_two_crates();
        let rows = tethys
            .get_coupling_metrics(CouplingSort::Name)
            .expect("metrics");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn get_package_coupling_unknown_returns_none() {
        let (_dir, tethys) = workspace_with_two_crates();
        assert!(
            tethys
                .get_package_coupling("missing")
                .expect("query")
                .is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static TRACED_CALLER_SQL: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn collect_caller_sql(sql: &str) {
        TRACED_CALLER_SQL
            .lock()
            .expect("caller SQL trace lock")
            .push(sql.to_string());
    }

    fn traced_direct_callers(tethys: &Tethys, qualified_name: &str) -> (Vec<Caller>, Vec<String>) {
        TRACED_CALLER_SQL
            .lock()
            .expect("caller SQL trace lock")
            .clear();
        {
            let mut connection = tethys.db.connection().expect("index connection");
            connection.trace(Some(collect_caller_sql));
        }

        let callers = tethys.get_callers(
            qualified_name,
            CallerMode::Indexed {
                call_edges: CallEdgeSelection::All,
            },
        );

        {
            let mut connection = tethys.db.connection().expect("index connection");
            connection.trace(None);
        }
        let statements =
            std::mem::take(&mut *TRACED_CALLER_SQL.lock().expect("caller SQL trace lock"));
        (callers.expect("direct caller query"), statements)
    }

    fn temp_workspace() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn new_creates_instance_for_valid_workspace() {
        let workspace = temp_workspace();
        let result = Tethys::new(workspace.path());

        assert!(result.is_ok());
        let tethys = result.unwrap();
        // Canonicalize expected path to match Tethys::new() which canonicalizes workspace_root
        // (resolves /var -> /private/var on macOS, short names on Windows)
        let expected = workspace
            .path()
            .canonicalize()
            .expect("temp dir should be canonicalizable")
            .join(".rivets")
            .join("index")
            .join("tethys.db");
        assert_eq!(tethys.db_path(), expected);
    }

    #[test]
    fn new_fails_for_nonexistent_workspace() {
        let result = Tethys::new(Path::new("/nonexistent/path/that/does/not/exist"));

        assert!(result.is_err());
    }

    #[test]
    fn build_qualified_name_with_single_segment_path() {
        let result = crate::db::build_qualified_name("open", Some(&["Index".to_string()]));
        assert_eq!(result, "Index::open");
    }

    #[test]
    fn build_qualified_name_with_multi_segment_path() {
        let result = crate::db::build_qualified_name(
            "open",
            Some(&["crate".to_string(), "db".to_string(), "Index".to_string()]),
        );
        assert_eq!(result, "crate::db::Index::open");
    }

    #[test]
    fn build_qualified_name_with_empty_path() {
        let result = crate::db::build_qualified_name("foo", Some(&[]));
        assert_eq!(result, "foo");
    }

    #[test]
    fn build_qualified_name_with_none_path() {
        let result = crate::db::build_qualified_name("bar", None);
        assert_eq!(result, "bar");
    }

    // ========================================================================
    // uri_to_path Tests
    // ========================================================================

    #[test]
    #[cfg(not(windows))]
    fn uri_to_path_handles_unix_path() {
        let uri = "file:///home/user/project/src/main.rs";
        let result = Tethys::uri_to_path(uri);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/home/user/project/src/main.rs")
        );
    }

    #[test]
    fn uri_to_path_returns_none_for_non_file_uri() {
        let uri = "https://example.com/file.rs";
        let result = Tethys::uri_to_path(uri);
        assert!(result.is_none());
    }

    #[test]
    fn uri_to_path_returns_none_for_empty_string() {
        let result = Tethys::uri_to_path("");
        assert!(result.is_none());
    }

    #[test]
    #[cfg(not(windows))]
    fn uri_to_path_decodes_percent_encoded_spaces() {
        let uri = "file:///home/user/my%20project/src/main.rs";
        let result = Tethys::uri_to_path(uri);
        assert_eq!(
            result,
            Some(PathBuf::from("/home/user/my project/src/main.rs"))
        );
    }

    // ========================================================================
    // IndexOptions Tests
    // ========================================================================

    #[test]
    fn index_options_default_has_lsp_disabled() {
        let options = IndexOptions::default();
        assert!(!options.use_lsp());
    }

    #[test]
    fn index_options_with_lsp_enables_lsp() {
        let options = IndexOptions::with_lsp();
        assert!(options.use_lsp());
    }

    #[test]
    fn index_with_options_returns_zero_lsp_resolved_when_disabled() {
        let workspace = temp_workspace();

        // Create a simple Rust file
        let src_dir = workspace.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(src_dir.join("lib.rs"), "pub fn hello() {}").expect("write file");

        let mut tethys = Tethys::new(workspace.path()).expect("create tethys");
        let stats = tethys
            .index_with_options(IndexOptions::default())
            .expect("index");

        assert_eq!(
            stats.total_lsp_resolved(),
            0,
            "LSP resolved count should be 0 when use_lsp is false"
        );
        assert!(
            stats.lsp_sessions.is_empty(),
            "LSP sessions should be empty when use_lsp is false"
        );
    }
    #[test]
    fn caller_hydration_statement_count_is_independent_of_result_count() {
        let workspace = temp_workspace();
        let src_dir = workspace.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"caller_trace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub fn one_target() {}\n\
             pub fn many_target() {}\n\
             pub fn one_caller() { one_target(); }\n\
             pub fn many_a() { many_target(); }\n\
             pub fn many_b() { many_target(); }\n\
             pub fn many_c() { many_target(); }\n\
             pub fn many_d() { many_target(); }\n",
        )
        .expect("write lib.rs");

        let mut tethys = Tethys::new(workspace.path()).expect("create tethys");
        tethys.index().expect("index");

        let (one_caller, one_caller_sql) = traced_direct_callers(&tethys, "one_target");
        let (many_callers, many_caller_sql) = traced_direct_callers(&tethys, "many_target");

        assert_eq!(one_caller.len(), 1);
        assert_eq!(many_callers.len(), 4);
        assert_eq!(
            one_caller_sql.len(),
            2,
            "target lookup plus one hydrated caller query"
        );
        assert_eq!(
            many_caller_sql.len(),
            one_caller_sql.len(),
            "caller hydration must not add one statement per result"
        );
        assert!(
            one_caller
                .iter()
                .chain(&many_callers)
                .all(|caller| caller.file == Path::new("src/lib.rs")),
            "hydrated callers must expose their workspace-relative indexed file"
        );
    }

    fn indexed_reachability_workspace() -> (TempDir, Tethys) {
        let workspace = temp_workspace();
        let src_dir = workspace.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"reachable_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub fn target() {}\npub fn left() { target(); }\npub fn right() {}\npub fn source() { left(); right(); }\n",
        )
        .expect("write lib.rs");

        let mut tethys = Tethys::new(workspace.path()).expect("create tethys");
        tethys.index().expect("index fixture");
        (workspace, tethys)
    }

    #[test]
    fn get_reachable_returns_forward_path() {
        let (_workspace, tethys) = indexed_reachability_workspace();

        let result = tethys
            .get_reachable("source", ReachabilityDirection::Forward, Some(2))
            .expect("forward reachability");

        assert_eq!(result.direction, ReachabilityDirection::Forward);
        assert_eq!(result.max_depth, 2);
        assert_eq!(result.reachable.len(), 3);
        let target = result
            .reachable
            .iter()
            .find(|entry| entry.target.qualified_name == "target")
            .expect("target is reachable");
        assert_eq!(target.path.len(), 2);
    }

    #[test]
    fn get_reachable_depth_zero_still_validates_source() {
        let (_workspace, tethys) = indexed_reachability_workspace();

        let error = tethys
            .get_reachable("missing", ReachabilityDirection::Forward, Some(0))
            .expect_err("missing source must fail at depth zero");

        assert!(matches!(error, Error::NotFound(message) if message == "symbol: missing"));
    }

    fn assert_same_reachability(left: &ReachabilityResult, right: &ReachabilityResult) {
        assert_eq!(left.source.id, right.source.id);
        assert_eq!(left.max_depth, right.max_depth);
        assert_eq!(left.direction, right.direction);
        assert_eq!(left.reachable.len(), right.reachable.len());
        for (left_entry, right_entry) in left.reachable.iter().zip(&right.reachable) {
            assert_eq!(left_entry.target.id, right_entry.target.id);
            assert_eq!(left_entry.target.is_test, right_entry.target.is_test);
            assert_eq!(left_entry.depth, right_entry.depth);
            assert_eq!(
                left_entry
                    .path
                    .iter()
                    .map(|symbol| symbol.id)
                    .collect::<Vec<_>>(),
                right_entry
                    .path
                    .iter()
                    .map(|symbol| symbol.id)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn reachability_wrappers_match_canonical_operation() {
        let (_workspace, tethys) = indexed_reachability_workspace();

        for depth in [0, 1, 3] {
            let forward = tethys
                .get_forward_reachable("source", Some(depth))
                .expect("forward wrapper");
            let canonical_forward = tethys
                .get_reachable("source", ReachabilityDirection::Forward, Some(depth))
                .expect("canonical forward");
            assert_same_reachability(&forward, &canonical_forward);

            let backward = tethys
                .get_backward_reachable("target", Some(depth))
                .expect("backward wrapper");
            let canonical_backward = tethys
                .get_reachable("target", ReachabilityDirection::Backward, Some(depth))
                .expect("canonical backward");
            assert_same_reachability(&backward, &canonical_backward);
        }
    }
}
