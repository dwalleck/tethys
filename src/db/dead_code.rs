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
//! - no method-level `inherit` marker (`kind='inherit'` with
//!   `in_symbol_id` = the method — tethys-j2r1's suppression channel for
//!   trait-impl methods; keyed by `in_symbol_id` so markers for EXTERNAL
//!   traits, whose `symbol_id` is NULL, still suppress);
//! - for container kinds ([`crate::types::SymbolKind::is_container`]), no
//!   live transitive descendant via `parent_symbol_id` (a struct used only
//!   through its methods carries no refs on the type symbol itself);
//! - not an entry point: Rust `fn main` in a binary root (`src/main.rs`,
//!   `src/bin/`, `examples/`, `build.rs`) or any C# `Main` method
//!   (deliberate over-suppression — conservative direction).
//!
//! Self-originated rows are excluded so a recursive-but-otherwise-dead
//! function is still reported — `rustc`'s `dead_code` lint agrees.
//!
//! The tier decision (Definite vs Maybe, via the textual word-boundary
//! scan) lives at the facade layer; this module only produces the
//! zero-evidence set.

use std::collections::{HashMap, HashSet};

use rusqlite::params;
use tracing::trace;

use super::Index;
use super::hierarchy::CONTAINER_KINDS_SQL;
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
    /// Symbol row id (used for self-origin checks and liveness lookups).
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

/// True when the path is a Rust binary root whose `fn main` the toolchain
/// invokes: `src/main.rs`, `src/bin/*`, `examples/*`, `build.rs` —
/// matched per path SEGMENT (workspace- or crate-relative), so
/// `crates/x/src/bin/tool.rs` qualifies while a lib module named
/// `src/domain/examples_helper.rs` does not. A directory literally named
/// `examples` outside a crate root over-matches; over-suppression is the
/// accepted conservative direction (design C9).
fn rust_binary_root(path: &str) -> bool {
    if path == "src/main.rs" || path == "build.rs" {
        return true;
    }
    if path.ends_with("/src/main.rs") || path.ends_with("/build.rs") {
        return true;
    }
    let segments: Vec<&str> = path.split('/').collect();
    segments.windows(2).any(|pair| pair == ["src", "bin"])
        || segments[..segments.len().saturating_sub(1)].contains(&"examples")
}

/// Entry points are alive by contract with the toolchain, not by inbound
/// references — the probe measured `main` surviving only via 203
/// unrelated textual hits (design C9: remove the luck dependency).
/// C# accepts both callable kinds: the extractor classifies
/// `static void Main()` as `function`, instance methods as `method`
/// (measured on the S6 fixture), and over-suppression is the accepted
/// conservative direction.
fn is_entry_point(language: &str, kind: &str, name: &str, path: &str) -> bool {
    match language {
        "rust" => kind == "function" && name == "main" && rust_binary_root(path),
        "csharp" => (kind == "method" || kind == "function") && name == "Main",
        _ => false,
    }
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

        let has_live_descendant = live_descendant_ancestors(&conn, &unresolved_by_name)?;

        let sql = format!(
            "SELECT s.id, s.name, s.qualified_name, s.kind, s.visibility,
                    f.path, s.line, s.end_line, f.language,
                    s.kind IN {CONTAINER_KINDS_SQL} AS is_container
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
               AND NOT EXISTS (
                   SELECT 1 FROM refs m
                   WHERE m.kind = 'inherit' AND m.in_symbol_id = s.id)
             ORDER BY f.path, s.line, s.name"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![], |row| {
            Ok((
                ZeroEvidenceCandidate {
                    id: SymbolId::from(row.get::<_, i64>(0)?),
                    name: row.get(1)?,
                    qualified_name: row.get(2)?,
                    kind: row.get(3)?,
                    visibility: row.get(4)?,
                    file: row.get(5)?,
                    line: row.get(6)?,
                    end_line: row.get(7)?,
                },
                row.get::<_, String>(8)?,
                row.get::<_, bool>(9)?,
            ))
        })?;

        let mut candidates = Vec::new();
        for row in rows {
            let (candidate, language, is_container) = row?;
            let unresolved_match =
                unresolved_by_name
                    .get(candidate.name.as_str())
                    .is_some_and(|origins| {
                        origins
                            .iter()
                            .any(|origin| *origin != Some(candidate.id.as_i64()))
                    });
            let container_alive =
                is_container && has_live_descendant.contains(&candidate.id.as_i64());
            let entry_point =
                is_entry_point(&language, &candidate.kind, &candidate.name, &candidate.file);
            if !unresolved_match && !container_alive && !entry_point {
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

/// Ancestor set of every *live* symbol, walked up `parent_symbol_id`.
/// Live = `is_test`, a resolved non-self inbound ref, an `inherit`
/// marker, an unresolved name match (self-originated rows ignored), or
/// an entry point — the same evidence vocabulary as candidacy, so the
/// two cannot disagree on what counts as a sign of life. Entry points
/// confer liveness upward: a C# `Program` class whose only member is
/// `Main` is scaffolding the toolchain invokes, not dead code.
///
/// Takes the already-held connection: the `Index` connection mutex is
/// not reentrant, so re-locking from inside `dead_code_zero_evidence`
/// would self-deadlock (caught by the S2 gate hanging all 14 tests).
///
/// Cost: three indexed scans + one O(symbols) pass + an upward walk
/// that visits each ancestor once (early exit on already-marked
/// ancestors, which also guards hypothetical parent cycles).
fn live_descendant_ancestors(
    conn: &rusqlite::Connection,
    unresolved_by_name: &HashMap<String, HashSet<Option<i64>>>,
) -> Result<HashSet<i64>> {
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    let mut live: HashSet<i64> = HashSet::new();

    let mut sym_stmt = conn.prepare(
        "SELECT s.id, s.parent_symbol_id, s.name, s.is_test, s.kind, f.path, f.language
         FROM symbols s JOIN files f ON f.id = s.file_id",
    )?;
    for row in sym_stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })? {
        let (id, parent, name, is_test, kind, path, language) = row?;
        if let Some(parent) = parent {
            parent_of.insert(id, parent);
        }
        let unresolved_match = unresolved_by_name
            .get(name.as_str())
            .is_some_and(|origins| origins.iter().any(|origin| *origin != Some(id)));
        if is_test || unresolved_match || is_entry_point(&language, &kind, &name, &path) {
            live.insert(id);
        }
    }

    let mut resolved_stmt = conn.prepare(
        "SELECT DISTINCT symbol_id FROM refs
             WHERE symbol_id IS NOT NULL
               AND (in_symbol_id IS NULL OR in_symbol_id != symbol_id)",
    )?;
    for row in resolved_stmt.query_map([], |row| row.get::<_, i64>(0))? {
        live.insert(row?);
    }
    let mut marker_stmt = conn.prepare(
        "SELECT DISTINCT in_symbol_id FROM refs
             WHERE kind = 'inherit' AND in_symbol_id IS NOT NULL",
    )?;
    for row in marker_stmt.query_map([], |row| row.get::<_, i64>(0))? {
        live.insert(row?);
    }

    let mut ancestors: HashSet<i64> = HashSet::new();
    for &id in &live {
        let mut current = id;
        while let Some(&parent) = parent_of.get(&current) {
            if !ancestors.insert(parent) {
                break;
            }
            current = parent;
        }
    }
    Ok(ancestors)
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
        parent: Option<SymbolId>,
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
                parent_symbol_id: sym.parent,
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
            parent: None,
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

    /// C4: a method carrying an `inherit` marker is suppressed even when
    /// the trait is EXTERNAL — the marker row's `symbol_id` is NULL and
    /// only `in_symbol_id` points at the method. Kills: joining the
    /// marker through `symbol_id` instead of `in_symbol_id`.
    #[test]
    fn trait_impl_marker_suppresses_external_trait() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let fmt = add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Method,
                ..private_fn(file, "fmt")
            },
        );
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Method,
                ..private_fn(file, "inherent_dead")
            },
        );
        index
            .insert_reference(&InsertReferenceParams {
                symbol_id: None,
                file_id: file,
                kind: "inherit",
                line: 3,
                column: 1,
                in_symbol_id: Some(fmt),
                reference_name: Some("Display"),
                strategy: None,
            })
            .expect("insert marker");
        assert_eq!(candidate_names(&index), vec!["inherent_dead"]);
    }

    /// C5: a container with a live descendant is suppressed — including
    /// through TWO parent hops (the C# nested-class shape). A dead
    /// container with only dead children stays reported, and so do the
    /// dead children. Kills: a direct-children-only liveness check.
    #[test]
    fn container_live_descendant_transitive() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let grandparent = add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Struct,
                ..private_fn(file, "Outer")
            },
        );
        let parent = add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Struct,
                parent: Some(grandparent),
                ..private_fn(file, "Inner")
            },
        );
        let used_method = add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Method,
                parent: Some(parent),
                ..private_fn(file, "used")
            },
        );
        let caller = add_symbol(&index, &private_fn(file, "caller"));
        add_ref(
            &index,
            Some(used_method),
            file,
            Some(caller),
            None,
            Some(ResolutionStrategy::SameFile),
        );
        add_ref(
            &index,
            Some(caller),
            file,
            None,
            None,
            Some(ResolutionStrategy::SameFile),
        );
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Struct,
                ..private_fn(file, "DeadOuter")
            },
        );
        // Outer and Inner suppressed via the grandchild; DeadOuter reported.
        assert_eq!(candidate_names(&index), vec!["DeadOuter"]);
    }

    /// C5: an `is_test` descendant counts as life — test scaffolding
    /// containers are not dead. Kills: liveness limited to ref evidence.
    #[test]
    fn container_with_test_descendant_suppressed() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let scaffold = add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Struct,
                ..private_fn(file, "Scaffold")
            },
        );
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Method,
                is_test: true,
                parent: Some(scaffold),
                ..private_fn(file, "test_case")
            },
        );
        assert_eq!(candidate_names(&index), Vec::<String>::new());
    }

    /// C5 scope guard: live-descendant suppression applies to CONTAINER
    /// kinds only — a dead function is still reported even though its
    /// nested fn is referenced (from inside the dead fn itself, which is
    /// exactly why child evidence must not revive a function).
    #[test]
    fn function_not_suppressed_by_live_nested_fn() {
        let (_dir, mut index) = temp_index();
        let file = add_file(&mut index, "src/lib.rs", Language::Rust);
        let outer = add_symbol(&index, &private_fn(file, "dead_outer"));
        let nested = add_symbol(
            &index,
            &Sym {
                parent: Some(outer),
                ..private_fn(file, "nested")
            },
        );
        add_ref(
            &index,
            Some(nested),
            file,
            Some(outer),
            None,
            Some(ResolutionStrategy::SameFile),
        );
        assert_eq!(candidate_names(&index), vec!["dead_outer"]);
    }

    /// C9: entry points are excluded by path role and language — bin-root
    /// and example mains are out; a `main` in a lib module stays a
    /// candidate (kills path-blind name matching); a Rust method named
    /// `Main` stays (kills language-blind C# rule); a C# `Main` method is
    /// out.
    #[test]
    fn entry_points_excluded_by_path_and_language() {
        let (_dir, mut index) = temp_index();
        let bin = add_file(&mut index, "src/main.rs", Language::Rust);
        let crate_bin = add_file(&mut index, "crates/x/src/bin/tool.rs", Language::Rust);
        let example = add_file(&mut index, "examples/demo.rs", Language::Rust);
        let lib = add_file(&mut index, "src/util.rs", Language::Rust);
        let cs = add_file(&mut index, "src/App.cs", Language::CSharp);
        add_symbol(&index, &private_fn(bin, "main"));
        add_symbol(&index, &private_fn(crate_bin, "main"));
        add_symbol(&index, &private_fn(example, "main"));
        add_symbol(&index, &private_fn(lib, "main"));
        add_symbol(
            &index,
            &Sym {
                kind: SymbolKind::Method,
                ..private_fn(lib, "Main")
            },
        );
        add_symbol(
            &index,
            &Sym {
                file: cs,
                kind: SymbolKind::Method,
                ..private_fn(cs, "Main")
            },
        );
        let mut names = candidate_names(&index);
        names.sort();
        assert_eq!(names, vec!["Main", "main"]);
    }

    /// `rust_binary_root` path-shape table: segment matching, not
    /// substring matching. Kills: `contains("examples")` false-matching
    /// `src/examples_helper.rs`, and missing crate-relative roots.
    #[test]
    fn rust_binary_root_segment_shapes() {
        let cases = [
            ("src/main.rs", true),
            ("build.rs", true),
            ("crates/x/src/main.rs", true),
            ("crates/x/build.rs", true),
            ("src/bin/tool.rs", true),
            ("crates/x/src/bin/nested/tool.rs", true),
            ("examples/demo.rs", true),
            ("crates/x/examples/demo.rs", true),
            ("src/util.rs", false),
            ("src/examples_helper.rs", false),
            ("src/binary.rs", false),
            ("examples_data/main.rs", false),
        ];
        for (path, expected) in cases {
            assert_eq!(super::rust_binary_root(path), expected, "path {path:?}");
        }
    }
}
