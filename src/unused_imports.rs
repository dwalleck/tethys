//! Unused-import analysis.
//!
//! Reports explicit imports whose bound name is never referenced in the
//! importing file. Unlike reachability-based analyses, unused-import
//! detection is purely file-local, so this module re-parses source files
//! directly (in parallel) rather than querying post-resolution index state —
//! re-parsing keeps line numbers, sees references *before* same-file
//! resolution discards their names, and can never be stale relative to disk.
//! The index is consulted only to corroborate confidence (is the imported
//! symbol a workspace trait?).
//!
//! ## False-positive posture
//!
//! A lint that accuses live imports loses trust permanently, so every
//! ambiguity resolves toward NOT reporting:
//!
//! - Glob imports (`use foo::*`) are skipped — usage can't be determined.
//! - Underscore aliases (`use foo::Bar as _;`) are skipped — importing a
//!   trait for method syntax only is the explicit *purpose* of that form.
//! - A textual word-boundary scan suppresses names that appear anywhere in
//!   the file beyond the `use` statement itself (macro bodies, doc comments,
//!   function-as-value positions the extractor can't see).
//! - Imports that may be traits used invisibly through method-call syntax
//!   are downgraded to [`UnusedImportConfidence::MaybeTrait`].

use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;
use tracing::{debug, warn};

use crate::Tethys;
use crate::error::Result;
use crate::languages::common::ImportStatement;
use crate::languages::module_resolver::{ModuleContext, get_module_resolver};
use crate::parallel::ParsedFileData;
use crate::types::{Language, SymbolKind};

/// How certain the analysis is that an import is genuinely unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnusedImportConfidence {
    /// The name is never referenced and cannot be a trait used invisibly
    /// through method-call syntax (it resolves to a non-trait workspace
    /// symbol, or its lowercase first letter rules out a trait).
    Definite,
    /// The name is never referenced, but it may be a trait whose methods
    /// are invoked via method-call syntax — which leaves no reference to
    /// the trait's own name anywhere in the file.
    MaybeTrait,
}

/// A single unused import finding.
#[derive(Debug, Clone, Serialize)]
pub struct UnusedImport {
    /// Workspace-relative path of the file containing the import.
    pub file: PathBuf,
    /// 1-indexed line of the `use` statement.
    pub line: u32,
    /// The name as bound in scope (the alias, if aliased).
    pub name: String,
    /// The module path the name is imported from (e.g. `std::collections`).
    pub source_module: String,
    /// Confidence that the import is genuinely unused.
    pub confidence: UnusedImportConfidence,
}

/// Per-file parse output retained for analysis.
struct ParsedForAnalysis {
    relative_path: PathBuf,
    data: ParsedFileData,
    content: String,
}

#[expect(
    clippy::missing_errors_doc,
    reason = "error docs deferred to avoid churn during active development"
)]
impl Tethys {
    /// Find explicit imports whose bound name is never referenced in the
    /// importing file.
    ///
    /// Re-parses workspace source files in parallel; results always reflect
    /// the current state on disk. The index database is used only to
    /// upgrade confidence (a name that resolves to a workspace non-trait
    /// symbol is reported as [`UnusedImportConfidence::Definite`]), so
    /// running `tethys index` first improves precision but is not required.
    ///
    /// Currently Rust-only: C# `using` directives are namespace globs,
    /// which this analysis skips by design.
    pub fn find_unused_imports(&self) -> Result<Vec<UnusedImport>> {
        let mut skipped_dirs = Vec::new();
        let files = self.discover_files(&mut skipped_dirs)?;

        let rust_files: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| {
                f.extension()
                    .and_then(|e| e.to_str())
                    .and_then(Language::from_extension)
                    == Some(Language::Rust)
            })
            .collect();

        debug!(
            file_count = rust_files.len(),
            "Scanning Rust files for unused imports"
        );

        let workspace_root = self.workspace_root.clone();

        // Parse in parallel. Files that fail to parse are skipped with a
        // warning — an unparseable file can't be analyzed, and failing the
        // whole report over one bad file helps nobody.
        let parsed: Vec<ParsedForAnalysis> = rust_files
            .par_iter()
            .filter_map(|file_path| {
                let data = match Self::parse_file_static(&workspace_root, file_path, Language::Rust)
                {
                    Ok(d) => d,
                    Err(e) => {
                        warn!(
                            file = %file_path.display(),
                            error = %e,
                            "Skipping file in unused-import analysis (parse failed)"
                        );
                        return None;
                    }
                };
                // Re-read for the textual guard. parse_file_static reads
                // internally but doesn't return the content; a second read
                // of an OS-cached file is cheap and keeps its signature
                // untouched.
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(
                            file = %file_path.display(),
                            error = %e,
                            "Skipping file in unused-import analysis (read failed)"
                        );
                        return None;
                    }
                };
                Some(ParsedForAnalysis {
                    relative_path: data.relative_path.clone(),
                    data,
                    content,
                })
            })
            .collect();

        let mut findings = Vec::new();
        for file in &parsed {
            self.analyze_file(file, &mut findings)?;
        }

        // Deterministic output regardless of parallel parse order.
        findings.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));

        Ok(findings)
    }

    /// Analyze one parsed file, appending findings.
    fn analyze_file(
        &self,
        file: &ParsedForAnalysis,
        findings: &mut Vec<UnusedImport>,
    ) -> Result<()> {
        // Names actually referenced in the file: direct names plus the first
        // path segment of qualified references (`db::open()` marks `db` used).
        // Shared with compute_dependencies via `common::referenced_names` so
        // both agree on which imports count as used.
        let referenced = crate::languages::common::referenced_names(&file.data.references);

        let resolver = get_module_resolver(Language::Rust);
        let full_path = self.workspace_root.join(&file.relative_path);
        let module_ctx = ModuleContext {
            current_file: &full_path,
            crates: self.crates(),
            anchor: resolver.file_anchor(&full_path, &self.workspace_root, self.crates()),
            namespaces: None,
        };

        for import in &file.data.imports {
            // Glob imports: usage indeterminable, skip by design.
            if import.is_glob {
                continue;
            }
            // Re-exports (`pub use`) are API surface, not local usage —
            // whether anything *external* consumes them is a different
            // analysis (dead re-export detection over the whole index).
            if import.is_reexport {
                continue;
            }
            // `use foo::Bar as _;` imports a trait for method syntax only —
            // explicitly intentional, never report.
            if import.alias.as_deref() == Some("_") {
                continue;
            }

            for name in &import.imported_names {
                let bound_name: &str = if name == "self" {
                    // `use foo::bar::{self, X}` binds the module name `bar`.
                    match import.path.last() {
                        Some(last) => last,
                        None => continue,
                    }
                } else {
                    import.alias.as_deref().unwrap_or(name)
                };

                if referenced.contains(bound_name) {
                    continue;
                }

                // Textual guard: if the bound name appears as a whole word
                // beyond the file's use statements AND its own same-name
                // definitions, assume it is used through something the
                // extractor can't see (macro body, fn-as-value, doc link) and
                // stay silent.
                //
                // Same-name definitions count as non-usage: a `mod db;`
                // declaration paired with a redundant `use crate::db::{self};`
                // puts the module name in the file twice (the decl + the use
                // path) with zero real uses. The declaration is not a use, so
                // it must not mask the redundant import.
                let non_usage_occurrences = count_in_use_statements(&file.data.imports, bound_name)
                    + file
                        .data
                        .symbols
                        .iter()
                        .filter(|s| s.name == bound_name)
                        .count();
                if count_word_occurrences(&file.content, bound_name) > non_usage_occurrences {
                    continue;
                }

                let confidence = self.classify_confidence(name, import, &module_ctx)?;

                findings.push(UnusedImport {
                    file: file.relative_path.clone(),
                    line: import.line,
                    name: bound_name.to_string(),
                    source_module: resolver.join_import(&import.path),
                    confidence,
                });
            }
        }

        Ok(())
    }

    /// Decide how confident we can be that an unreferenced import is unused.
    ///
    /// The only invisible-use channel for an import whose name appears
    /// nowhere is trait method syntax. Resolve the import against the
    /// workspace: a non-trait workspace symbol is `Definite`; a workspace
    /// trait or an unresolvable (external) `UpperCamelCase` name might be a
    /// trait, so downgrade to `MaybeTrait`. Lowercase names (functions,
    /// modules, macros) and `SCREAMING_CASE` names (constants) cannot be
    /// traits.
    fn classify_confidence(
        &self,
        imported_name: &str,
        import: &ImportStatement,
        module_ctx: &ModuleContext<'_>,
    ) -> Result<UnusedImportConfidence> {
        let starts_uppercase = imported_name.chars().next().is_some_and(char::is_uppercase);
        if !starts_uppercase {
            return Ok(UnusedImportConfidence::Definite);
        }
        // SCREAMING_SNAKE_CASE (no lowercase letters at all) is a constant
        // by Rust convention; trait names are UpperCamelCase.
        if !imported_name.chars().any(char::is_lowercase) && imported_name.len() > 1 {
            return Ok(UnusedImportConfidence::Definite);
        }

        let resolver = get_module_resolver(Language::Rust);
        if let Some(resolved) = resolver.resolve_import_segments(&import.path, module_ctx) {
            let relative = self.relative_path(&resolved);
            if let Some(file_id) = self.db.get_file_id(&relative)?
                && let Some(symbol) = self.db.search_symbol_in_file(imported_name, file_id)?
            {
                return Ok(if symbol.kind == SymbolKind::Trait {
                    UnusedImportConfidence::MaybeTrait
                } else {
                    UnusedImportConfidence::Definite
                });
            }
        }

        // External or unresolvable UpperCamelCase name — could be a trait.
        Ok(UnusedImportConfidence::MaybeTrait)
    }
}

/// Count how many times `name` appears anywhere inside the file's `use`
/// statements: path segments, imported names, and aliases — across ALL
/// statements, including globs.
///
/// This is the textual-guard baseline: any word-boundary occurrence in the
/// file beyond this count must come from non-import code (or comments/
/// strings, which conservatively also suppress the finding). Counting every
/// import-side appearance uniformly keeps the guard correct for aliases
/// (`use foo::Orig as Bound;` contributes 1 to "Bound"), self-imports
/// (`use crate::db::{self};` contributes 1 to "db" via the path), and
/// names that recur as path segments of unrelated imports.
fn count_in_use_statements(imports: &[ImportStatement], name: &str) -> usize {
    imports
        .iter()
        .map(|stmt| {
            stmt.path.iter().filter(|seg| *seg == name).count()
                + stmt.imported_names.iter().filter(|n| *n == name).count()
                + usize::from(stmt.alias.as_deref() == Some(name))
        })
        .sum()
}

/// Count word-boundary occurrences of `name` in `content`.
///
/// A match counts only when not surrounded by identifier characters
/// (`[A-Za-z0-9_]`), so `Bar` does not match inside `FooBar` or `Bar2`.
fn count_word_occurrences(content: &str, name: &str) -> usize {
    if name.is_empty() {
        return 0;
    }
    let bytes = content.as_bytes();
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = content[start..].find(name) {
        let abs = start + pos;
        let end = abs + name.len();
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            count += 1;
        }
        // Advance past this match position (byte-safe: `find` returns
        // char-boundary offsets and `name` is well-formed UTF-8).
        start = abs + name.len().max(1);
    }
    count
}

/// Whether a byte is an identifier character for word-boundary purposes.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_occurrences_respects_boundaries() {
        assert_eq!(count_word_occurrences("Bar FooBar Bar2 _Bar Bar", "Bar"), 2);
        assert_eq!(count_word_occurrences("", "Bar"), 0);
        assert_eq!(count_word_occurrences("Bar", ""), 0);
        assert_eq!(count_word_occurrences("Bar::baz()", "Bar"), 1);
        assert_eq!(count_word_occurrences("x.bar()", "bar"), 1);
    }

    #[test]
    fn word_occurrences_counts_macro_bodies_and_comments() {
        // The whole point of the textual guard: these "uses" are invisible
        // to the extractor but visible to a word scan.
        let content = "use crate::helper;\nfn main() { assert!(helper()); }";
        assert_eq!(count_word_occurrences(content, "helper"), 2);
    }

    /// Build a temp workspace from (path, content) pairs and index it.
    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Tethys) {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, content) in files {
            let full = dir.path().join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&full, content).expect("write fixture");
        }
        let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");
        tethys.index().expect("index");
        (dir, tethys)
    }

    const CARGO_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

    #[test]
    fn unused_lowercase_import_is_definite() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::helper;\n\npub fn entry() {}\n",
            ),
            ("src/util.rs", "pub fn helper() {}\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "exactly one unused import: {findings:?}");
        let f = &findings[0];
        assert_eq!(f.name, "helper");
        assert_eq!(f.source_module, "crate::util");
        assert_eq!(f.line, 2);
        assert_eq!(f.confidence, UnusedImportConfidence::Definite);
    }

    #[test]
    fn used_import_is_not_reported() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::helper;\n\npub fn entry() { helper(); }\n",
            ),
            ("src/util.rs", "pub fn helper() {}\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "called import must not be reported: {findings:?}"
        );
    }

    #[test]
    fn macro_only_usage_is_not_reported() {
        // `use tracing::info` consumed only by `info!(...)` — the macro
        // reference (new Macro kind) marks it used.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "use tracing::info;\n\npub fn entry() { info!(\"hello\"); }\n",
            ),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "macro-invoked import must not be reported: {findings:?}"
        );
    }

    #[test]
    fn usage_inside_macro_token_tree_is_suppressed() {
        // `helper` appears only inside `assert!(...)` — invisible to the
        // extractor (token tree), but the textual guard sees it.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::helper;\n\npub fn entry() { assert!(helper()); }\n",
            ),
            ("src/util.rs", "pub fn helper() -> bool { true }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "token-tree usage must suppress the finding: {findings:?}"
        );
    }

    #[test]
    fn function_passed_as_value_is_suppressed() {
        // fn-as-value produces no extracted reference; the textual guard
        // must keep this from being a false accusation.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::parse;\n\n\
                 pub fn entry(v: Vec<i32>) -> Vec<i32> { v.into_iter().map(parse).collect() }\n",
            ),
            ("src/util.rs", "pub fn parse(x: i32) -> i32 { x }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "fn-as-value usage must suppress the finding: {findings:?}"
        );
    }

    #[test]
    fn underscore_alias_is_never_reported() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "use std::fmt::Write as _;\n\npub fn entry() {}\n",
            ),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "`as _` trait imports are intentional: {findings:?}"
        );
    }

    #[test]
    fn glob_import_is_never_reported() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "use std::collections::*;\n\npub fn entry() {}\n",
            ),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "glob imports are skipped: {findings:?}"
        );
    }

    #[test]
    fn external_uppercase_import_is_maybe_trait() {
        // HashMap is a struct, but we can't know that for external crates —
        // it could be a trait used via method syntax, so downgrade.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "use std::collections::HashMap;\n\npub fn entry() {}\n",
            ),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].name, "HashMap");
        assert_eq!(findings[0].confidence, UnusedImportConfidence::MaybeTrait);
    }

    #[test]
    fn workspace_struct_import_is_definite() {
        // The index can prove Config is a struct, not a trait.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::Config;\n\npub fn entry() {}\n",
            ),
            ("src/util.rs", "pub struct Config { pub x: i32 }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].name, "Config");
        assert_eq!(findings[0].confidence, UnusedImportConfidence::Definite);
    }

    #[test]
    fn workspace_trait_import_is_maybe_trait() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::Greet;\n\npub fn entry() {}\n",
            ),
            (
                "src/util.rs",
                "pub trait Greet { fn greet(&self) -> String; }\n",
            ),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].name, "Greet");
        assert_eq!(findings[0].confidence, UnusedImportConfidence::MaybeTrait);
    }

    #[test]
    fn aliased_import_checks_the_alias_binding() {
        // `use foo::Original as Renamed;` binds `Renamed`; usage of
        // `Renamed` marks it used even though `Original` never recurs.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::Config as Cfg;\n\n\
                 pub fn entry(c: Cfg) -> i32 { c.x }\n",
            ),
            ("src/util.rs", "pub struct Config { pub x: i32 }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(findings.is_empty(), "alias is used as a type: {findings:?}");
    }

    #[test]
    fn unused_aliased_import_reports_the_alias() {
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::Config as Cfg;\n\npub fn entry() {}\n",
            ),
            ("src/util.rs", "pub struct Config { pub x: i32 }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].name, "Cfg",
            "report the bound name, not the original"
        );
    }

    #[test]
    fn pub_use_reexport_is_never_reported() {
        // Re-exports exist for external consumers; local non-use is the
        // norm, not a problem. (Running this analysis on tethys itself
        // surfaced exactly this false-positive class in db/mod.rs.)
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "mod util;\npub use util::helper;\npub(crate) use util::other;\n",
            ),
            ("src/util.rs", "pub fn helper() {}\npub fn other() {}\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "pub use re-exports must never be reported: {findings:?}"
        );
    }

    #[test]
    fn screaming_case_const_import_is_definite() {
        // SCREAMING_SNAKE_CASE is a constant by convention — it cannot be
        // a trait, so no method-syntax escape hatch applies.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::MAX_SIZE;\n\npub fn entry() {}\n",
            ),
            ("src/util.rs", "pub const MAX_SIZE: usize = 10;\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].name, "MAX_SIZE");
        assert_eq!(findings[0].confidence, UnusedImportConfidence::Definite);
    }

    #[test]
    fn name_in_doc_comment_is_suppressed() {
        // Conservative: a doc-comment mention suppresses the finding. The
        // alternative (reporting it) risks accusing intra-doc-link imports.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod util;\nuse crate::util::Config;\n\n\
                 /// See [`Config`] for details.\npub fn entry() {}\n",
            ),
            ("src/util.rs", "pub struct Config { pub x: i32 }\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert!(
            findings.is_empty(),
            "doc-comment mention must suppress (intra-doc links use imports): {findings:?}"
        );
    }

    #[test]
    fn self_import_of_locally_declared_module_is_reported() {
        // `use crate::db::{self};` is redundant when the same file already
        // declares `mod db;` (the module name is in scope from the decl).
        // The textual guard must not count the `mod db;` declaration itself
        // as a use, or the redundant self-import is silently missed.
        let (_dir, tethys) = workspace(&[
            ("Cargo.toml", CARGO_TOML),
            (
                "src/lib.rs",
                "pub mod db;\nuse crate::db::{self};\n\npub fn entry() {}\n",
            ),
            ("src/db.rs", "pub fn thing() {}\n"),
        ]);

        let findings = tethys.find_unused_imports().expect("scan");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].name, "db");
        assert_eq!(findings[0].confidence, UnusedImportConfidence::Definite);
    }
}
