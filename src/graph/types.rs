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

fn depth_one_prefix_len<T>(entries: &[T], depth_of: impl Fn(&T) -> usize) -> usize {
    entries.partition_point(|entry| depth_of(entry) == 1)
}

/// Result of transitive caller analysis for a symbol.
#[derive(Debug, Clone)]
pub struct SymbolImpact {
    /// The target symbol being analyzed.
    pub target: Symbol,
    callers: Vec<SymbolImpactCaller>,
}

impl SymbolImpact {
    /// `callers` must be sorted by minimum depth ascending (the SQL traversal
    /// orders by `min_depth`); the direct/transitive split relies on it.
    pub(crate) fn new(target: Symbol, callers: Vec<SymbolImpactCaller>) -> Self {
        Self { target, callers }
    }

    /// Index of the first caller past the depth-one prefix.
    fn direct_end(&self) -> usize {
        depth_one_prefix_len(&self.callers, |caller| caller.depth)
    }

    /// All callers, ordered by minimum depth and then qualified name.
    #[must_use]
    pub fn callers(&self) -> &[SymbolImpactCaller] {
        &self.callers
    }

    /// Callers whose minimum depth is one.
    #[must_use]
    pub fn direct_callers(&self) -> &[SymbolImpactCaller] {
        &self.callers[..self.direct_end()]
    }

    /// Callers whose minimum depth is greater than one.
    #[must_use]
    pub fn transitive_callers(&self) -> &[SymbolImpactCaller] {
        &self.callers[self.direct_end()..]
    }
}

/// A dependent reached during file-impact traversal.
#[derive(Debug, Clone)]
pub struct FileImpactDependent {
    /// Workspace-relative path of the indexed dependent file.
    pub file: PathBuf,
    /// Minimum number of file-dependency edges from the dependent to the target.
    pub depth: usize,
}

/// Result of transitive dependent analysis for a file.
#[derive(Debug, Clone)]
pub struct FileImpact {
    /// Workspace-relative path of the indexed target file.
    pub target: PathBuf,
    dependents: Vec<FileImpactDependent>,
}

impl FileImpact {
    /// `dependents` must be sorted by minimum depth ascending (the SQL
    /// traversal orders by `min_depth`); the direct/transitive split relies on
    /// it.
    pub(crate) fn new(target: PathBuf, dependents: Vec<FileImpactDependent>) -> Self {
        Self { target, dependents }
    }

    /// Index of the first dependent past the depth-one prefix.
    fn direct_end(&self) -> usize {
        depth_one_prefix_len(&self.dependents, |dependent| dependent.depth)
    }

    /// All dependents, ordered by minimum depth and then file path.
    #[must_use]
    pub fn dependents(&self) -> &[FileImpactDependent] {
        &self.dependents
    }

    /// Dependents whose minimum depth is one.
    #[must_use]
    pub fn direct_dependents(&self) -> &[FileImpactDependent] {
        &self.dependents[..self.direct_end()]
    }

    /// Dependents whose minimum depth is greater than one.
    #[must_use]
    pub fn transitive_dependents(&self) -> &[FileImpactDependent] {
        &self.dependents[self.direct_end()..]
    }
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
