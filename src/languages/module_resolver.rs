//! Per-language module resolution — the `ModuleResolver` seam.
//!
//! Pass-2 reference resolution and import-dependency computation translate
//! module paths (from `use`/`using` statements and qualified reference
//! prefixes) into workspace files. The *rules* for that translation are
//! language-specific: Rust has `crate`/`self`/`super` anchors, Cargo crate
//! roots, and an implicit-crate retry for bare paths; C# namespaces have no
//! file mapping at all (tethys-jwf9). This trait contains those rules so the
//! drivers in `resolve.rs` and `indexing.rs` stay language-neutral.
//!
//! Two separators exist, deliberately:
//!
//! - Qualified names and reference names are stored with the **canonical
//!   `::` separator for every language** (see `.separator-fix/spec.md`
//!   decision #5). That is a cache format, not per-language knowledge, so
//!   parsing stored qualified names does not go through this trait.
//! - Import `source_module` strings are stored in each language's own
//!   format (`crate::db` vs `MyApp.Models`); [`ModuleResolver::import_separator`]
//!   is the single source of truth for that format.
//!
//! Implementations perform filesystem probing only — **no database access**.
//! Database lookups stay in the drivers, which keeps candidate enumeration
//! and index state separable (and testable without a DB).

#![expect(
    dead_code,
    reason = "seam lands dark in this slice; resolve.rs/indexing.rs drivers wire it in the next slices, at which point this expectation must be removed"
)]

use std::path::{Path, PathBuf};

use crate::types::CrateInfo;

/// Workspace context handed to a [`ModuleResolver`].
///
/// Built once per file being resolved. `anchor` is the per-file anchor
/// produced by [`ModuleResolver::file_anchor`] (for Rust, the containing
/// crate's source root; `None` for languages without one) — computed once
/// per file so resolvers don't rescan `crates` on every call.
pub(crate) struct ModuleContext<'a> {
    pub current_file: &'a Path,
    pub workspace_root: &'a Path,
    pub crates: &'a [CrateInfo],
    pub anchor: Option<PathBuf>,
}

/// One prefix-split of a qualified reference name: candidate files in
/// priority order, plus the symbol tail to look up in whichever file wins.
///
/// Driver contract (enforced behaviorally by the `qualified_split_trap`
/// integration test, not assertable at runtime): within a split, the FIRST
/// candidate file present in the index claims the split; if the tail lookup
/// then misses, the split is abandoned — remaining candidates are NOT tried.
/// This preserves the pre-seam interleaving of interpretation and index
/// lookup (design claim C6).
pub(crate) struct QualifiedSplit {
    pub files: Vec<PathBuf>,
    pub tail: String,
}

/// Language-specific module-path→file translation.
pub(crate) trait ModuleResolver: Send + Sync {
    /// Separator used in this language's **stored import** `source_module`
    /// strings (`"::"` for Rust, `"."` for C#). Single source of truth for
    /// the storage-side joins and for [`ModuleResolver::resolve_import`].
    fn import_separator(&self) -> &'static str;

    /// Per-file resolution anchor, computed once per file by the driver and
    /// carried in [`ModuleContext::anchor`]. Rust: the containing crate's
    /// source root (orphan files fall back to the file's parent directory —
    /// a documented sentinel, see `cargo::src_root_for`). Languages without
    /// an anchor concept return `None`.
    fn file_anchor(
        &self,
        file: &Path,
        workspace_root: &Path,
        crates: &[CrateInfo],
    ) -> Option<PathBuf>;

    /// Translate import-path segments into the defining file, if it exists
    /// in the workspace. Filesystem probing allowed; no DB access. `None`
    /// means "this resolver cannot map the path" (external module, or the
    /// language has no mapping).
    fn resolve_import_segments(
        &self,
        segments: &[String],
        ctx: &ModuleContext<'_>,
    ) -> Option<PathBuf>;

    /// Translate a stored `source_module` string into the defining file.
    ///
    /// Empty input is a documented refusal: returns `None` (load-bearing —
    /// splitting `""` would otherwise produce a single empty segment and
    /// hand nonsense to the segment resolver).
    fn resolve_import(&self, source_module: &str, ctx: &ModuleContext<'_>) -> Option<PathBuf> {
        if source_module.is_empty() {
            return None;
        }
        let segments: Vec<String> = source_module
            .split(self.import_separator())
            .map(String::from)
            .collect();
        self.resolve_import_segments(&segments, ctx)
    }

    /// Candidate lookups for a qualified reference name (canonical `::`
    /// form) that survived import-based resolution, longest prefix first.
    /// Owns language-specific interpretations (Rust: implicit-crate retry,
    /// then as-written). Empty for languages without module semantics.
    fn qualified_splits(&self, ref_name: &str, ctx: &ModuleContext<'_>) -> Vec<QualifiedSplit>;
}

/// C# module resolution: an explicit declining stub.
///
/// C# namespaces are textual and tethys has no namespace→file index yet, so
/// every translation declines — exactly the pre-seam behavior, where C#
/// import paths never resolved through the Rust-only path logic. Implementing
/// real `using`-directive resolution is tethys-jwf9.
pub(crate) struct CSharpModuleResolver;

impl ModuleResolver for CSharpModuleResolver {
    fn import_separator(&self) -> &'static str {
        "."
    }

    fn file_anchor(
        &self,
        _file: &Path,
        _workspace_root: &Path,
        _crates: &[CrateInfo],
    ) -> Option<PathBuf> {
        None
    }

    fn resolve_import_segments(
        &self,
        _segments: &[String],
        _ctx: &ModuleContext<'_>,
    ) -> Option<PathBuf> {
        None
    }

    fn qualified_splits(&self, _ref_name: &str, _ctx: &ModuleContext<'_>) -> Vec<QualifiedSplit> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(root: &'a Path) -> ModuleContext<'a> {
        ModuleContext {
            current_file: root,
            workspace_root: root,
            crates: &[],
            anchor: None,
        }
    }

    #[test]
    fn csharp_import_separator_is_dot() {
        assert_eq!(CSharpModuleResolver.import_separator(), ".");
    }

    #[test]
    fn csharp_declines_simple_namespace() {
        let root = Path::new("/ws");
        assert_eq!(CSharpModuleResolver.resolve_import("System", &ctx(root)), None);
    }

    #[test]
    fn csharp_declines_dotted_namespace() {
        let root = Path::new("/ws");
        assert_eq!(
            CSharpModuleResolver.resolve_import("MyApp.Models", &ctx(root)),
            None
        );
    }

    #[test]
    fn csharp_declines_empty_source_module() {
        let root = Path::new("/ws");
        assert_eq!(CSharpModuleResolver.resolve_import("", &ctx(root)), None);
    }

    #[test]
    fn csharp_declines_string_containing_rust_separator() {
        // Cross-separator input (the other language's format) must decline,
        // not be misparsed into segments.
        let root = Path::new("/ws");
        assert_eq!(CSharpModuleResolver.resolve_import("A::B", &ctx(root)), None);
    }

    #[test]
    fn csharp_qualified_splits_empty() {
        let root = Path::new("/ws");
        assert!(CSharpModuleResolver
            .qualified_splits("Foo::Bar", &ctx(root))
            .is_empty());
    }

    #[test]
    fn csharp_file_anchor_none() {
        let root = Path::new("/ws");
        assert_eq!(
            CSharpModuleResolver.file_anchor(root, root, &[]),
            None
        );
    }
}
