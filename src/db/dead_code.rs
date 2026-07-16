//! Dead-code analysis (tethys-dvsw), db layer: the evidence funnel that
//! yields zero-evidence candidates.
//!
//! A symbol is a *candidate* when it is non-public, non-test, and of a kind
//! whose references the indexer can actually see for the file's language
//! (Rust `module` and `struct_field` are structurally invisible — module
//! path segments and Rust field reads emit no refs at all, measured 148 +
//! 427 guaranteed false positives on the self-index; C# data members DO
//! have a `field_access` channel, tethys-xebx, so they stay candidates).
//!
//! A candidate has *zero evidence* when no inbound sign of life exists:
//!
//! - no resolved reference from outside the symbol itself — ANY confidence
//!   band counts, including speculative (ADR-0003: speculative edges are
//!   dead-code SUPPRESSIONS, the opposite posture from `callers
//!   --exclude-speculative`);
//! - no unresolved reference whose `reference_name` names the symbol
//!   (bare-equal or last `::`-segment equal — post-tethys-53iv ambiguous
//!   method calls decline and land here), again ignoring self-originated
//!   rows;
//! - (slice 2) no method-level `inherit` marker, no live descendant, and
//!   not an entry point.
//!
//! Self-originated rows are excluded so a recursive-but-otherwise-dead
//! function is still reported — `rustc`'s `dead_code` lint agrees.
//!
//! The tier decision (Definite vs Maybe, via the textual word-boundary
//! scan) lives at the facade layer; this module only produces the
//! zero-evidence set.

// Consumed by `Tethys::find_dead_code` from slice 3 of the tethys-dvsw
// plan; until that facade lands, non-test builds see this module as
// unreachable. Removed in slice 3.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use rusqlite::params;
use tracing::trace;

use super::Index;
use crate::error::Result;
use crate::types::SymbolId;

/// Kinds analyzable for Rust files: kinds with at least one working
/// reference channel. Excludes `module` and `struct_field` (no channel;
/// see module docs) and `macro` (own Pass-1 binding map, out of scope).
const RUST_CANDIDATE_KINDS_SQL: &str = "('function', 'method', 'struct', 'enum', 'trait', \
     'type_alias', 'const', 'static', 'enum_variant')";

/// Kinds analyzable for C# files. Data members (fields, properties,
/// events) participate: `member_access_expression` reads produce
/// `field_access` refs (tethys-xebx); the still-invisible read shapes
/// (tethys-5uqz) surface as Maybe via the textual channel, not Definite.
const CSHARP_CANDIDATE_KINDS_SQL: &str = "('class', 'interface', 'struct', 'struct_field', \
     'method', 'property', 'event', 'delegate', 'function', 'enum')";

/// A candidate that survived the db-layer funnel: no inbound evidence.
/// Tier assignment (the textual scan) happens at the facade layer.
#[derive(Debug, Clone)]
pub(crate) struct ZeroEvidenceCandidate {
    /// Symbol row id (used by slice-2 channels and diagnostics).
    pub id: SymbolId,
    /// Bare declared name.
    pub name: String,
    /// Stored qualified name (`module::Name`).
    pub qualified_name: String,
    /// Raw `symbols.kind` text.
    pub kind: String,
    /// Raw `symbols.visibility` text (never `public` here).
    pub visibility: String,
    /// Workspace-relative declaring file path.
    pub file: String,
    /// 1-based declaration line.
    pub line: u32,
    /// 1-based end line of the declaration span, when the extractor
    /// recorded one (`NULL` end lines degrade the textual scan to
    /// line-only exclusion — the safe, Maybe-leaning direction).
    pub end_line: Option<u32>,
}

/// Last `::`-segment of a reference name; the whole name when unqualified.
/// `crate::foobar` keys as `foobar`, so a symbol named `bar` can never be
/// suppressed by it (segment-boundary matching, not `ends_with`).
fn last_segment(reference_name: &str) -> &str {
    reference_name.rsplit("::").next().unwrap_or(reference_name)
}

impl Index {
    /// Candidates (non-public, non-test, analyzable kind) with no inbound
    /// reference evidence: no resolved ref and no name-matching unresolved
    /// ref, self-originated rows excluded on both channels.
    ///
    /// Cost: one indexed anti-join pass over candidates
    /// (`idx_refs_symbol`), one `O(unresolved)` scan to build the
    /// name-keyed map (`idx_refs_unresolved`), `O(1)` membership per
    /// candidate. No per-candidate `LIKE` scans.
    pub(crate) fn dead_code_zero_evidence(&self) -> Result<Vec<ZeroEvidenceCandidate>> {
        trace!("Collecting zero-evidence dead-code candidates");
        let conn = self.connection()?;

        // Channel: unresolved name matches. Keyed by last segment; the
        // value set carries each row's in_symbol_id so a candidate can
        // ignore rows it originated itself (recursion must not
        // self-suppress).
        let mut unresolved_by_name: HashMap<String, HashSet<Option<i64>>> = HashMap::new();
        let mut unres_stmt = conn.prepare(
            "SELECT reference_name, in_symbol_id FROM refs
             WHERE symbol_id IS NULL AND reference_name IS NOT NULL",
        )?;
        for row in unres_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
        })? {
            let (name, in_symbol) = row?;
            unresolved_by_name
                .entry(last_segment(&name).to_string())
                .or_default()
                .insert(in_symbol);
        }

        let sql = format!(
            "SELECT s.id, s.name, s.qualified_name, s.kind, s.visibility,
                    f.path, s.line, s.end_line
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             WHERE s.visibility != 'public'
               AND s.is_test = 0
               AND ((f.language = 'rust' AND s.kind IN {RUST_CANDIDATE_KINDS_SQL})
                 OR (f.language = 'csharp' AND s.kind IN {CSHARP_CANDIDATE_KINDS_SQL}))
               AND NOT EXISTS (
                   SELECT 1 FROM refs r
                   WHERE r.symbol_id = s.id
                     AND (r.in_symbol_id IS NULL OR r.in_symbol_id != s.id))
             ORDER BY f.path, s.line, s.name"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![], |row| {
            Ok(ZeroEvidenceCandidate {
                id: SymbolId::from(row.get::<_, i64>(0)?),
                name: row.get(1)?,
                qualified_name: row.get(2)?,
                kind: row.get(3)?,
                visibility: row.get(4)?,
                file: row.get(5)?,
                line: row.get(6)?,
                end_line: row.get(7)?,
            })
        })?;

        let mut candidates = Vec::new();
        for row in rows {
            let candidate = row?;
            let suppressed =
                unresolved_by_name
                    .get(candidate.name.as_str())
                    .is_some_and(|origins| {
                        origins
                            .iter()
                            .any(|origin| *origin != Some(candidate.id.as_i64()))
                    });
            if !suppressed {
                candidates.push(candidate);
            }
        }
        trace!(
            candidates = candidates.len(),
            "Zero-evidence candidate collection complete"
        );
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::super::references::InsertReferenceParams;
    use super::super::symbols::InsertSymbolParams;
    use super::*;
    use crate::types::{FileId, Language, ResolutionStrategy, SymbolKind, Visibility};

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open index");
        (dir, index)
    }

    fn add_file(index: &mut Index, path: &str, language: Language) -> FileId {
        index
            .upsert_file(std::path::Path::new(path), language, 0, 0, None)
            .expect("upsert file")
    }

    struct Sym<'a> {
        file: FileId,
        name: &'a str,
        kind: SymbolKind,
        visibility: Visibility,
        is_test: bool,
    }

    fn add_symbol(index: &Index, sym: &Sym<'_>) -> SymbolId {
        index
            .insert_symbol(&InsertSymbolParams {
                file_id: sym.file,
                name: sym.name,
                module_path: "crate",
                qualified_name: sym.name,
                kind: sym.kind,
                line: 1,
                column: 1,
                span: None,
                signature: None,
                visibility: sym.visibility,
                parent_symbol_id: None,
                is_test: sym.is_test,
            })
            .expect("insert symbol")
    }

    fn private_fn(file: FileId, name: &str) -> Sym<'_> {
        Sym {
            file,
            name,
            kind: SymbolKind::Function,
            visibility: Visibility::Private,
            is_test: false,
        }
    }

    fn add_ref(
        index: &Index,
        target: Option<SymbolId>,
        file: FileId,
        in_symbol: Option<SymbolId>,
        name: Option<&str>,
        strategy: Option<ResolutionStrategy>,
    ) {
        index
            .insert_reference(&InsertReferenceParams {
                symbol_id: target,
                file_id: file,
                kind: "call",
                line: 5,
                column: 1,
                in_symbol_id: in_symbol,
                reference_name: name,
                strategy,
            })
            .expect("insert reference");
    }

    fn candidate_names(index: &Index) -> Vec<String> {
        index
            .dead_code_zero_evidence()
            .expect("zero evidence query")
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    /// C1 core: zero-ref private fn is a candidate; public and
    /// `is_test` twins with identical (non-)evidence are not.
    /// Kills: a missing visibility or `is_test` predicate.
    #[test]
    fn candidacy_visibility_and_is_test() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        add_symbol(&index, &private_fn(file, "dead_private"));
        add_symbol(
            &index,
            &Sym {
                visibility: Visibility::Public,
                ..private_fn(file, "dead_public")
            },
        );
        add_symbol(
            &index,
            &Sym {
                is_test: true,
                ..private_fn(file, "test_helper")
            },
        );
        assert_eq!(candidate_names(&index), vec!["dead_private"]);
    }

    /// C2: ANY resolved non-self inbound ref suppresses — including one
    /// whose strategy derives the speculative band (`unique_workspace`).
    /// Kills: copying callers' speculative-exclusion posture into the
    /// funnel (the transferred ADR-0003 AC).
    #[test]
    fn speculative_only_ref_suppresses() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let spec = add_symbol(&index, &private_fn(file, "spec_called"));
        let caller = add_symbol(&index, &private_fn(file, "caller"));
        add_ref(
            &index,
            Some(spec),
            file,
            Some(caller),
            None,
            Some(ResolutionStrategy::UniqueWorkspace),
        );
        add_ref(
            &index,
            Some(caller),
            file,
            None,
            None,
            Some(ResolutionStrategy::SameFile),
        );
        assert_eq!(candidate_names(&index), Vec::<String>::new());
    }

    /// C2: a resolved ref originating INSIDE the symbol (recursion) is
    /// not evidence — the recursive-but-dead fn stays reported, matching
    /// rustc's `dead_code` verdict. Kills: a bare `NOT EXISTS refs` join.
    #[test]
    fn self_ref_does_not_suppress() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let rec = add_symbol(&index, &private_fn(file, "recursive_dead"));
        add_ref(
            &index,
            Some(rec),
            file,
            Some(rec),
            None,
            Some(ResolutionStrategy::SameFile),
        );
        assert_eq!(candidate_names(&index), vec!["recursive_dead"]);
    }

    /// C3: unresolved refs suppress by name — bare-equal AND
    /// last-`::`-segment-equal shapes (the post-53iv decline shapes).
    #[test]
    fn unresolved_bare_and_qualified_suppress() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        add_symbol(&index, &private_fn(file, "bare_target"));
        add_symbol(&index, &private_fn(file, "qual_target"));
        add_symbol(&index, &private_fn(file, "still_dead"));
        add_ref(&index, None, file, None, Some("bare_target"), None);
        add_ref(
            &index,
            None,
            file,
            None,
            Some("crate::inner::qual_target"),
            None,
        );
        assert_eq!(candidate_names(&index), vec!["still_dead"]);
    }

    /// C3 suffix trap: unresolved `crate::foobar` must NOT suppress a
    /// symbol named `bar`. Kills: `ends_with(name)` matching without a
    /// `::` segment boundary.
    #[test]
    fn qualified_match_requires_segment_boundary() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        add_symbol(&index, &private_fn(file, "bar"));
        add_ref(&index, None, file, None, Some("crate::foobar"), None);
        assert_eq!(candidate_names(&index), vec!["bar"]);
    }

    /// C3 self-exclusion: an unresolved ref that the symbol itself
    /// originated (declined recursive call) does not suppress it.
    #[test]
    fn unresolved_self_ref_does_not_suppress() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let rec = add_symbol(&index, &private_fn(file, "rec_declined"));
        add_ref(&index, None, file, Some(rec), Some("rec_declined"), None);
        assert_eq!(candidate_names(&index), vec!["rec_declined"]);
    }

    /// C1/C10 kind rules are language-aware: Rust `struct_field` and
    /// `module` are structurally invisible (excluded); a C# `struct_field`
    /// in a C# file IS a candidate. Kills: a language-blind kind list.
    #[test]
    fn kind_exclusions_language_aware() {
        let (_dir, mut index) = temp_index();
        let rust = add_file(&mut index, "src/lib.rs", Language::Rust);
        let cs = add_file(&mut index, "src/Thing.cs", Language::CSharp);
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::StructField,
                ..private_fn(rust, "rust_field")
            },
        );
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Module,
                ..private_fn(rust, "tests_mod")
            },
        );
        add_symbol(
            &index,
            &Sym {
                file: cs,
                kind: SymbolKind::StructField,
                ..private_fn(cs, "CsField")
            },
        );
        assert_eq!(candidate_names(&index), vec!["CsField"]);
    }

    /// Determinism substrate: candidates come back ordered by
    /// (file, line, name) regardless of insertion order.
    #[test]
    fn candidates_ordered_by_file_line_name() {
        let (_dir, mut index) = temp_index();
        let b = add_file(&mut index, "src/b.rs", Language::Rust);
        let a = add_file(&mut index, "src/a.rs", Language::Rust);
        add_symbol(&index, &private_fn(b, "in_b"));
        add_symbol(&index, &private_fn(a, "in_a"));
        assert_eq!(candidate_names(&index), vec!["in_a", "in_b"]);
    }
}
