//! Graph-specific query result types.

use std::path::PathBuf;

use crate::types::{IndexedFile, Symbol};

/// A caller reached during symbol-impact traversal.
#[derive(Debug, Clone)]
pub struct SymbolImpactCaller {
    /// The calling symbol.
    pub symbol: Symbol,
    /// Workspace-relative path of the indexed file containing the caller.
    pub file: PathBuf,
    /// Minimum number of call edges from this caller to the target.
    pub depth: usize,
}

/// Result of transitive caller analysis for a symbol.
#[derive(Debug, Clone)]
pub struct SymbolImpact {
    /// The target symbol being analyzed.
    pub target: Symbol,
    callers: Vec<SymbolImpactCaller>,
}

impl SymbolImpact {
    pub(crate) fn new(target: Symbol, callers: Vec<SymbolImpactCaller>) -> Self {
        Self { target, callers }
    }

    /// All callers, ordered by minimum depth and then qualified name.
    #[must_use]
    pub fn callers(&self) -> &[SymbolImpactCaller] {
        &self.callers
    }

    /// Callers whose minimum depth is one.
    #[must_use]
    pub fn direct_callers(&self) -> &[SymbolImpactCaller] {
        let direct_end = self.callers.partition_point(|caller| caller.depth == 1);
        &self.callers[..direct_end]
    }

    /// Callers whose minimum depth is greater than one.
    #[must_use]
    pub fn transitive_callers(&self) -> &[SymbolImpactCaller] {
        let direct_end = self.callers.partition_point(|caller| caller.depth == 1);
        &self.callers[direct_end..]
    }
}

/// Information about a file dependency.
#[derive(Debug, Clone)]
pub struct FileDepInfo {
    /// The dependent/dependency file.
    pub file: IndexedFile,
    /// Number of references between the files.
    pub ref_count: usize,
}

/// Result of file-level impact analysis.
#[derive(Debug, Clone)]
pub struct FileImpact {
    /// The target file being analyzed.
    pub target: IndexedFile,
    /// Files that directly depend on the target.
    pub direct_dependents: Vec<FileDepInfo>,
    /// Files that transitively depend on the target.
    pub transitive_dependents: Vec<FileDepInfo>,
}

/// A path through the file dependency graph.
#[derive(Debug, Clone)]
pub struct FilePath {
    /// Files from source to target.
    files: Vec<IndexedFile>,
}

impl FilePath {
    /// Create a new file path, validating invariants.
    ///
    /// Returns `None` if `files` is empty.
    #[must_use]
    pub fn new(files: Vec<IndexedFile>) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        Some(Self { files })
    }

    /// Create a trivial path with a single file.
    #[must_use]
    pub fn single(file: IndexedFile) -> Self {
        Self { files: vec![file] }
    }

    /// Consume the path and return the files.
    #[must_use]
    pub fn into_files(self) -> Vec<IndexedFile> {
        self.files
    }
}
