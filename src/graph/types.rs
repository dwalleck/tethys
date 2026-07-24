//! Internal result types for graph queries.

use crate::types::{IndexedFile, Symbol};

/// Information about a caller of a symbol.
#[derive(Debug, Clone)]
pub struct CallerInfo {
    /// The symbol that calls the target.
    pub symbol: Symbol,
    /// How many times it references the target.
    pub reference_count: usize,
}

/// Result of transitive caller analysis (symbol-level impact).
#[derive(Debug, Clone)]
pub struct SymbolImpact {
    /// Symbols that directly call/reference the target.
    pub direct_callers: Vec<CallerInfo>,
    /// Symbols that transitively call the target (excludes direct).
    pub transitive_callers: Vec<CallerInfo>,
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
