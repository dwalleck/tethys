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

use std::path::{Path, PathBuf};

use tracing::debug;

use crate::cargo;
use crate::resolver::resolve_module_path;
use crate::types::{CrateInfo, Language};

/// Workspace context handed to a [`ModuleResolver`].
///
/// Built once per file being resolved. `anchor` is the per-file anchor
/// produced by [`ModuleResolver::file_anchor`] (for Rust, the containing
/// crate's source root; `None` for languages without one) — computed once
/// per file so resolvers don't rescan `crates` on every call.
pub(crate) struct ModuleContext<'a> {
    pub current_file: &'a Path,
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

/// Get the module resolver implementation for a language.
pub(crate) fn get_module_resolver(lang: Language) -> &'static dyn ModuleResolver {
    match lang {
        Language::Rust => &RustModuleResolver,
        Language::CSharp => &CSharpModuleResolver,
    }
}

/// Per-file source-root anchor for Rust files.
///
/// For files inside a discovered crate, the crate's
/// [`CrateInfo::src_root`] (`lib_path.parent()`-derived, not a hardcoded
/// `src/` — the rivets-6aoc bug class). For files outside any crate,
/// falls back to the file's parent directory as a documented sentinel:
/// `crate::*` paths anchored there are semantic no-ops (they won't
/// accidentally resolve), while `self::`/`super::` arms keep working off
/// the file path directly.
pub(crate) fn rust_src_root_for(
    file: &Path,
    crates: &[CrateInfo],
    workspace_root: &Path,
) -> PathBuf {
    if let Some(crate_info) = cargo::get_crate_for_file(file, crates) {
        crate_info.src_root()
    } else {
        debug!(
            file = %file.display(),
            "File not in any known crate; using file parent as sentinel src_root"
        );
        file.parent()
            .map_or_else(|| workspace_root.to_path_buf(), Path::to_path_buf)
    }
}

/// Rust module resolution: `crate`/`self`/`super` anchors, Cargo crate
/// routing, and the implicit-crate retry for bare qualified paths.
///
/// Path translation delegates to [`crate::resolver::resolve_module_path`],
/// which this seam owns as its engine; the candidate *enumeration* for
/// qualified references lives here (ported verbatim from the pre-seam
/// `qualified_module_fallback`, rivets-044i).
pub(crate) struct RustModuleResolver;

impl ModuleResolver for RustModuleResolver {
    fn import_separator(&self) -> &'static str {
        "::"
    }

    fn file_anchor(
        &self,
        file: &Path,
        workspace_root: &Path,
        crates: &[CrateInfo],
    ) -> Option<PathBuf> {
        Some(rust_src_root_for(file, crates, workspace_root))
    }

    fn resolve_import_segments(
        &self,
        segments: &[String],
        ctx: &ModuleContext<'_>,
    ) -> Option<PathBuf> {
        let anchor = ctx.anchor.as_deref()?;
        resolve_module_path(segments, ctx.current_file, anchor, ctx.crates)
    }

    /// Candidate enumeration for qualified references, longest prefix
    /// first. Per split, up to two file candidates in priority order:
    ///
    /// 1. **Implicit-crate**: the prefix with `crate::` prepended (skipped
    ///    when the first segment is already `crate`/`self`/`super`) —
    ///    Rust 2018+ paths like `helper::foo` from an import-less file.
    /// 2. **As-written**: the prefix unchanged, letting the
    ///    `crate`/`self`/`super`/workspace-crate arms fire.
    ///
    /// Splits with no on-disk candidate are omitted (the driver would skip
    /// them anyway). Duplicate candidates within a split are deduped — the
    /// driver's first-indexed-wins rule makes the second copy unreachable.
    fn qualified_splits(&self, ref_name: &str, ctx: &ModuleContext<'_>) -> Vec<QualifiedSplit> {
        let segments: Vec<&str> = ref_name.split("::").collect();
        if segments.len() < 2 {
            return Vec::new();
        }
        let Some(anchor) = ctx.anchor.as_deref() else {
            return Vec::new();
        };

        let mut splits = Vec::with_capacity(segments.len() - 1);
        for split in (1..segments.len()).rev() {
            let prefix = &segments[..split];
            let tail = segments[split..].join("::");
            let mut files = Vec::with_capacity(2);

            if !matches!(prefix[0], "crate" | "self" | "super") {
                let mut with_crate: Vec<String> = Vec::with_capacity(prefix.len() + 1);
                with_crate.push("crate".to_string());
                with_crate.extend(prefix.iter().map(|s| (*s).to_string()));
                if let Some(p) =
                    resolve_module_path(&with_crate, ctx.current_file, anchor, ctx.crates)
                {
                    files.push(p);
                }
            }

            let as_written: Vec<String> = prefix.iter().map(|s| (*s).to_string()).collect();
            if let Some(p) = resolve_module_path(&as_written, ctx.current_file, anchor, ctx.crates)
                && !files.contains(&p)
            {
                files.push(p);
            }

            if !files.is_empty() {
                splits.push(QualifiedSplit { files, tail });
            }
        }
        splits
    }
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

    fn ctx(root: &Path) -> ModuleContext<'_> {
        ModuleContext {
            current_file: root,
            crates: &[],
            anchor: None,
        }
    }

    #[test]
    fn registry_dispatches_by_language() {
        assert_eq!(get_module_resolver(Language::Rust).import_separator(), "::");
        assert_eq!(
            get_module_resolver(Language::CSharp).import_separator(),
            "."
        );
    }

    #[test]
    fn csharp_import_separator_is_dot() {
        assert_eq!(CSharpModuleResolver.import_separator(), ".");
    }

    #[test]
    fn csharp_declines_simple_namespace() {
        let root = Path::new("/ws");
        assert_eq!(
            CSharpModuleResolver.resolve_import("System", &ctx(root)),
            None
        );
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
        assert_eq!(
            CSharpModuleResolver.resolve_import("A::B", &ctx(root)),
            None
        );
    }

    #[test]
    fn csharp_qualified_splits_empty() {
        let root = Path::new("/ws");
        assert!(
            CSharpModuleResolver
                .qualified_splits("Foo::Bar", &ctx(root))
                .is_empty()
        );
    }

    #[test]
    fn csharp_file_anchor_none() {
        let root = Path::new("/ws");
        assert_eq!(CSharpModuleResolver.file_anchor(root, root, &[]), None);
    }

    // ===== RustModuleResolver =====

    use std::fs;

    /// Build a crate dir with the given files (relative to the crate's src/)
    /// and return its [`CrateInfo`].
    fn make_crate(root: &Path, name: &str, files: &[&str]) -> CrateInfo {
        let crate_path = root.join(name);
        let src = crate_path.join("src");
        fs::create_dir_all(&src).expect("crate src");
        fs::write(src.join("lib.rs"), "").expect("lib.rs");
        for relative in files {
            let full = src.join(relative);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("nested dir");
            }
            fs::write(&full, "").expect("nested file");
        }
        CrateInfo {
            name: name.to_string(),
            path: crate_path,
            lib_path: Some(PathBuf::from("src/lib.rs")),
            bin_paths: vec![],
        }
    }

    fn rust_ctx<'a>(
        current_file: &'a Path,
        workspace_root: &'a Path,
        crates: &'a [CrateInfo],
    ) -> ModuleContext<'a> {
        let anchor = RustModuleResolver.file_anchor(current_file, workspace_root, crates);
        ModuleContext {
            current_file,
            crates,
            anchor,
        }
    }

    #[test]
    fn rust_import_separator_is_double_colon() {
        assert_eq!(RustModuleResolver.import_separator(), "::");
    }

    /// The C6 trap shape: implicit-crate candidate (app/src/helper.rs) must
    /// come BEFORE the as-written workspace-crate candidate (helper/src/lib.rs)
    /// in the same split — the driver's first-indexed-wins + abandon-on-miss
    /// then reproduces the pre-seam interleaving exactly.
    #[test]
    fn qualified_splits_trap_ordering() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![
            make_crate(dir.path(), "app", &["helper.rs"]),
            make_crate(dir.path(), "helper", &[]),
        ];
        let current = dir.path().join("app/src/lib.rs");
        let ctx = rust_ctx(&current, dir.path(), &crates);

        let splits = RustModuleResolver.qualified_splits("helper::do_thing", &ctx);
        assert_eq!(splits.len(), 1, "two segments give exactly one split");
        assert_eq!(splits[0].tail, "do_thing");
        assert_eq!(
            splits[0].files,
            vec![
                dir.path().join("app/src/helper.rs"),
                dir.path().join("helper/src/lib.rs"),
            ],
            "implicit-crate candidate must precede as-written candidate"
        );
    }

    /// `crate::`-prefixed references suppress the implicit-crate retry:
    /// no split may contain a doubled `crate::crate::...` candidate.
    #[test]
    fn qualified_splits_crate_prefix_suppresses_retry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![make_crate(dir.path(), "app", &["db.rs"])];
        let current = dir.path().join("app/src/lib.rs");
        let ctx = rust_ctx(&current, dir.path(), &crates);

        let splits = RustModuleResolver.qualified_splits("crate::db::open", &ctx);
        assert_eq!(
            (splits[0].files.clone(), splits[0].tail.clone()),
            (vec![dir.path().join("app/src/db.rs")], "open".to_string()),
            "longest split resolves crate::db with exactly one candidate"
        );
        for s in &splits {
            assert_eq!(
                s.files.len(),
                1,
                "crate-prefixed: no implicit-crate duplicate"
            );
        }
        if let Some(second) = splits.get(1) {
            assert_eq!(second.tail, "db::open");
        }
    }

    /// Splits are ordered longest prefix first.
    #[test]
    fn qualified_splits_longest_prefix_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![make_crate(dir.path(), "app", &["a.rs", "a/b.rs"])];
        let current = dir.path().join("app/src/lib.rs");
        let ctx = rust_ctx(&current, dir.path(), &crates);

        let splits = RustModuleResolver.qualified_splits("a::b::c", &ctx);
        assert_eq!(splits.len(), 2);
        assert_eq!(splits[0].tail, "c");
        assert_eq!(splits[0].files, vec![dir.path().join("app/src/a/b.rs")]);
        assert_eq!(splits[1].tail, "b::c");
        assert_eq!(splits[1].files, vec![dir.path().join("app/src/a.rs")]);
    }

    /// Simple names (no separator) produce no splits — they never reach the
    /// qualified fallback in the driver either.
    #[test]
    fn qualified_splits_simple_name_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![make_crate(dir.path(), "app", &[])];
        let current = dir.path().join("app/src/lib.rs");
        let ctx = rust_ctx(&current, dir.path(), &crates);
        assert!(
            RustModuleResolver
                .qualified_splits("lonely", &ctx)
                .is_empty()
        );
    }

    /// `resolve_import` (provided method) splits stored `source_modules` on "::".
    #[test]
    fn rust_resolve_import_crate_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![make_crate(dir.path(), "app", &["db.rs"])];
        let current = dir.path().join("app/src/lib.rs");
        let ctx = rust_ctx(&current, dir.path(), &crates);
        assert_eq!(
            RustModuleResolver.resolve_import("crate::db", &ctx),
            Some(dir.path().join("app/src/db.rs"))
        );
        assert_eq!(
            RustModuleResolver.resolve_import("std::collections", &ctx),
            None
        );
    }

    /// Orphan files (no containing crate) anchor at their parent directory —
    /// the documented sentinel.
    #[test]
    fn rust_file_anchor_orphan_falls_back_to_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let orphan_dir = dir.path().join("scripts");
        fs::create_dir_all(&orphan_dir).expect("orphan dir");
        let orphan = orphan_dir.join("tool.rs");
        fs::write(&orphan, "").expect("orphan file");
        assert_eq!(
            RustModuleResolver.file_anchor(&orphan, dir.path(), &[]),
            Some(orphan_dir)
        );
    }

    /// Files inside a crate anchor at the crate's src root.
    #[test]
    fn rust_file_anchor_uses_crate_src_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let crates = vec![make_crate(dir.path(), "app", &[])];
        let current = dir.path().join("app/src/lib.rs");
        assert_eq!(
            RustModuleResolver.file_anchor(&current, dir.path(), &crates),
            Some(dir.path().join("app/src"))
        );
    }
}
