//! Language-specific symbol extraction.
//!
//! Each supported language implements the `LanguageSupport` trait, which defines
//! how to extract symbols, imports, and references from tree-sitter syntax trees.
//!
//! ## Adding a New Language
//!
//! 1. Add the variant to `Language` enum in `types.rs`
//! 2. Create a new module (e.g., `python.rs`)
//! 3. Implement `LanguageSupport` trait
//! 4. Register in `get_language_support()`
//! 5. Implement `ModuleResolver` in `module_resolver.rs` (module-path→file
//!    translation, per-file anchor, and the stored-import separator) and
//!    register it in `get_module_resolver()`. The resolution drivers in
//!    `resolve.rs` and `indexing.rs` are language-neutral and must not be
//!    edited — `tests/seam_lint.rs` enforces this.
//!
//! ## Design
//!
//! The trait-based design allows language-specific logic while maintaining
//! a uniform interface for the indexer.

pub mod common;
pub mod csharp;
pub(crate) mod module_resolver;
pub mod rust;
mod tree_sitter_utils;

use common::{ExtractedReference, ExtractedSymbol, ImportStatement};

use crate::types::{Language, Span};

/// Get the language support implementation for a language.
#[must_use]
pub fn get_language_support(lang: Language) -> &'static dyn LanguageSupport {
    match lang {
        Language::Rust => &rust::RustLanguage,
        Language::CSharp => &csharp::CSharpLanguage,
    }
}

/// Trait for language-specific symbol extraction.
///
/// Each supported language implements this trait to define how symbols,
/// imports, and references are extracted from tree-sitter syntax trees.
pub trait LanguageSupport: Send + Sync {
    /// Get the tree-sitter language for parsing.
    fn tree_sitter_language(&self) -> tree_sitter::Language;

    /// Locate the declared identifier for an indexed syntax-node span.
    ///
    /// Returns 1-indexed `(line, byte column)` coordinates. The default
    /// implementation reparses the source and reads the declaration node's
    /// tree-sitter `name` field.
    fn definition_name_position(&self, content: &[u8], span: Span) -> Option<(u32, u32)> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.tree_sitter_language()).ok()?;
        let tree = parser.parse(content, None)?;
        tree_sitter_utils::name_position_for_span(tree.root_node(), span)
    }
    /// Extract symbols from a parsed syntax tree.
    fn extract_symbols(&self, tree: &tree_sitter::Tree, content: &[u8]) -> Vec<ExtractedSymbol>;

    /// Extract references (usages) from a parsed syntax tree.
    fn extract_references(
        &self,
        tree: &tree_sitter::Tree,
        content: &[u8],
    ) -> Vec<ExtractedReference>;

    /// Extract import statements from a parsed syntax tree.
    fn extract_imports(&self, tree: &tree_sitter::Tree, content: &[u8]) -> Vec<ImportStatement>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolKind;

    #[test]
    fn definition_name_position_ignores_name_in_visibility_path() {
        let source = "pub(in crate::worker) fn worker() {}";
        let support = get_language_support(Language::Rust);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&support.tree_sitter_language())
            .expect("Rust grammar");
        let tree = parser.parse(source, None).expect("parse source");
        let symbol = support
            .extract_symbols(&tree, source.as_bytes())
            .into_iter()
            .find(|symbol| symbol.kind == SymbolKind::Function && symbol.name == "worker")
            .expect("worker function");
        let span = symbol.span.expect("function span");

        let position = support
            .definition_name_position(source.as_bytes(), span)
            .expect("declaration name position");
        let expected_column =
            u32::try_from(source.rfind("worker").expect("function name")).expect("column") + 1;

        assert_eq!(position, (1, expected_column));
    }
}
