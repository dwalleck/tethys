//! Visibility-tightening analysis queries (tethys-xoxq).
//!
//! Lists pub Rust items whose observed use is consistent with
//! `pub(crate)`: no reference reaches them from another package. Findings
//! are suppressions, not accusations (PRD tethys-l6nt): evidence in any
//! channel excludes a symbol from the report, and anything the index
//! cannot vouch for demotes to Maybe rather than being asserted. Design
//! and falsification table: `.tethys-xoxq/design.md`; the probe measured
//! the naive refs-only rule at 33% false candidates on real data
//! (`.tethys-xoxq/findings.md`), which is why exclusion consults every
//! evidence channel.

use std::collections::HashMap;

use serde::Serialize;
use tracing::trace;

use super::Index;
use super::deprecated::Tier;
use crate::error::Result;

/// Why a candidate is capped at [`Tier::Maybe`]. An empty demotion list is
/// exactly [`Tier::Definite`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Demotion {
    /// Another indexed symbol shares this name, so use evidence could be
    /// misattributed in either direction (the tethys-53iv phantom-binding
    /// class steals qualified cross-crate refs and destroys their text).
    SharedName,
    /// Reachable from the crate root through an all-public module chain:
    /// consumers outside the indexed workspace could name it, which the
    /// index cannot observe. Lifted by `workspace_closed`.
    RootReachable,
    /// The declaring module is glob-imported inside its own package; a
    /// glob re-export would publish the item without any per-item ref
    /// (tethys-pv7w — glob re-export targets carry no refs today).
    GlobReexportRisk,
}

/// A pub item whose observed use is consistent with `pub(crate)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisibilityFinding {
    /// Symbol name as declared.
    pub name: String,
    /// Raw `symbols.kind` text (display-only; no dispatch happens on it).
    pub kind: String,
    /// Workspace-relative path of the declaring file.
    pub file: String,
    /// 1-based declaration line.
    pub line: u32,
    /// Confidence tier; [`Tier::Definite`] iff `demotions` is empty.
    pub tier: Tier,
    /// Reasons the tier is capped at Maybe; empty for Definite.
    pub demotions: Vec<Demotion>,
}

/// SQL literal of the top-level item kinds eligible for tightening advice.
/// Member and module kinds are excluded — different tightening semantics,
/// tracked at tethys-w1e9. Enum variants carry no own visibility at all.
const CANDIDATE_KINDS_SQL: &str = "('function', 'struct', 'enum', 'trait', 'type_alias', 'const')";

/// A candidate row mid-pipeline: the SQL selection plus the package
/// attribution the Rust-side evidence channels key on.
struct CandidateRow {
    name: String,
    kind: String,
    file: String,
    line: u32,
    package_id: i64,
    /// The declaring package's manifest name normalized to the identifier
    /// that appears in `use` paths (`a-lib` → `a_lib`).
    crate_ident: String,
    /// `symbols.module_path` (e.g. `crate::inner`), matched against glob
    /// import rows for the C6 demotion.
    module_path: String,
}

impl Index {
    /// Pub top-level Rust symbols with no cross-package use evidence,
    /// ordered by (file, line, name) for deterministic output.
    ///
    /// Evidence channels consulted:
    /// - (a) resolved refs whose referencing file belongs to a different
    ///   `arch_packages` row than the declaring file (SQL CTE). Same-package
    ///   refs are NOT evidence against a candidate — being used only inside
    ///   its own package is the candidate condition.
    /// - (b) `imports` rows in another package whose `source_module` head
    ///   names the candidate's crate: a named row excludes that item; a
    ///   glob row (`symbol_name = '*'`) makes every pub root item nameable
    ///   in the importing crate, so it excludes ALL of the crate's
    ///   candidates. This channel exists because the probe measured real
    ///   cross-crate uses whose refs never resolve (re-export indirection
    ///   plus name collisions — tethys-z9mr / tethys-53iv classes).
    /// - (c) unresolved refs in another package whose qualified
    ///   `reference_name` last segment equals the candidate's name (the
    ///   `pkg::mod::item()` call-without-import shape; jdly Path B
    ///   mechanics over the `idx_refs_unresolved` partial index). Matching
    ///   is deliberately last-segment-only, not crate-prefix-anchored:
    ///   evidence here SUPPRESSES an accusation, so a wide net is the
    ///   conservative direction.
    pub(crate) fn get_visibility_candidates(
        &self,
        _workspace_closed: bool,
    ) -> Result<Vec<VisibilityFinding>> {
        trace!("Querying visibility-tightening candidates");
        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "WITH cross_pkg_refs AS (
                 SELECT DISTINCT r.symbol_id
                 FROM refs r
                 JOIN arch_file_packages rfp ON rfp.file_id = r.file_id
                 JOIN symbols rs             ON rs.id = r.symbol_id
                 JOIN arch_file_packages sfp ON sfp.file_id = rs.file_id
                 WHERE rfp.package_id != sfp.package_id
             )
             SELECT s.name, s.kind, f.path, s.line, fp.package_id, p.name, s.module_path
             FROM symbols s
             JOIN files f               ON f.id = s.file_id
             JOIN arch_file_packages fp ON fp.file_id = s.file_id
             JOIN arch_packages p       ON p.id = fp.package_id
             WHERE s.visibility = 'public'
               AND f.language = 'rust'
               AND s.kind IN {CANDIDATE_KINDS_SQL}
               AND s.id NOT IN (SELECT symbol_id FROM cross_pkg_refs)
               AND s.id NOT IN (SELECT symbol_id FROM refs
                                WHERE kind = 'reexport' AND symbol_id IS NOT NULL)
             ORDER BY f.path, s.line, s.name",
        ))?;
        let rows = stmt.query_map([], |row| {
            let package_name: String = row.get(5)?;
            debug_assert!(
                !package_name.is_empty(),
                "arch_packages.name is NOT NULL UNIQUE and manifest-parsed"
            );
            Ok(CandidateRow {
                name: row.get(0)?,
                kind: row.get(1)?,
                file: row.get(2)?,
                line: row.get(3)?,
                package_id: row.get(4)?,
                crate_ident: package_name.replace('-', "_"),
                module_path: row.get(6)?,
            })
        })?;
        let mut candidates = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        let (named_imports, glob_imports) = import_evidence(&conn)?;
        let unresolved_qualified = unresolved_qualified_evidence(&conn)?;
        let same_pkg_globs = glob_import_rows(&conn)?;

        candidates.retain(|c| {
            // Owned keys: HashMap's Borrow lookup can't be fed (&str, &str)
            // for a (String, String) key (same shape as deprecated.rs's
            // ambiguity set); d is small — tens, not 10^5.
            let key = (c.crate_ident.clone(), c.name.clone());
            let used_elsewhere = named_imports
                .get(&key)
                .into_iter()
                .flatten()
                .chain(glob_imports.get(&c.crate_ident).into_iter().flatten())
                .chain(unresolved_qualified.get(&c.name).into_iter().flatten())
                .any(|&pkg| pkg != c.package_id);
            !used_elsewhere
        });

        Ok(candidates
            .into_iter()
            .map(|c| {
                let mut demotions = Vec::new();
                // C6: a glob import row in the candidate's own package
                // targeting its module — a glob re-export would publish it
                // with no per-item ref (tethys-pv7w), so Definite would be
                // unsafe. Plain same-package `use m::*` also demotes:
                // `is_reexport` isn't persisted in the imports table, so
                // pub and non-pub globs are indistinguishable at query
                // time (conservative direction).
                if same_pkg_globs.iter().any(|(source, pkg)| {
                    *pkg == c.package_id && module_matches(&c.module_path, source)
                }) {
                    demotions.push(Demotion::GlobReexportRisk);
                }
                let tier = if demotions.is_empty() {
                    Tier::Definite
                } else {
                    Tier::Maybe
                };
                VisibilityFinding {
                    name: c.name,
                    kind: c.kind,
                    file: c.file,
                    line: c.line,
                    tier,
                    demotions,
                }
            })
            .collect())
    }
}

/// Does a glob import row's `source_module` denote the candidate's module?
/// Stored paths may be absolute (`crate::inner`) or relative (`inner`,
/// `self::inner`) — relative forms match as a `::`-bounded suffix of the
/// symbol's `module_path`. Over-matching is acceptable (demotion, not
/// exclusion; suppression-safe).
fn module_matches(module_path: &str, source_module: &str) -> bool {
    let source = source_module
        .strip_prefix("self::")
        .unwrap_or(source_module);
    module_path == source || module_path.ends_with(&format!("::{source}"))
}

/// All glob import rows (`symbol_name = '*'`) with their importing
/// package, for the same-package C6 demotion. g is small (tens of rows);
/// the per-candidate scan is O(g × d) ≪ the 10^6 budget.
fn glob_import_rows(conn: &rusqlite::Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT i.source_module, fp.package_id
         FROM imports i
         JOIN arch_file_packages fp ON fp.file_id = i.file_id
         WHERE i.symbol_name = '*'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Channel (b) lookup maps, one pass over `imports` rows: named rows keyed
/// by (crate-ident head of `source_module`, item name) → importing
/// packages; crate-glob rows (`symbol_name = '*'`) keyed by head alone.
#[expect(clippy::type_complexity, reason = "two lookup maps, built together")]
fn import_evidence(
    conn: &rusqlite::Connection,
) -> Result<(
    HashMap<(String, String), Vec<i64>>,
    HashMap<String, Vec<i64>>,
)> {
    let mut named: HashMap<(String, String), Vec<i64>> = HashMap::new();
    let mut globs: HashMap<String, Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT i.symbol_name, i.source_module, fp.package_id
         FROM imports i
         JOIN arch_file_packages fp ON fp.file_id = i.file_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (symbol_name, source_module, pkg) = row?;
        let Some(head) = source_module.split("::").next().filter(|h| !h.is_empty()) else {
            continue;
        };
        if symbol_name == "*" {
            globs.entry(head.to_string()).or_default().push(pkg);
        } else {
            named
                .entry((head.to_string(), symbol_name))
                .or_default()
                .push(pkg);
        }
    }
    Ok((named, globs))
}

/// Channel (c) lookup map, one pass over unresolved qualified refs (partial
/// index `idx_refs_unresolved`), keyed by last `::` segment → referencing
/// packages — O(u + d), never O(u × d).
fn unresolved_qualified_evidence(conn: &rusqlite::Connection) -> Result<HashMap<String, Vec<i64>>> {
    let mut by_last_segment: HashMap<String, Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT r.reference_name, fp.package_id
         FROM refs r
         JOIN arch_file_packages fp ON fp.file_id = r.file_id
         WHERE r.symbol_id IS NULL
           AND r.reference_name LIKE '%::%'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (reference_name, pkg) = row?;
        let Some(last_segment) = reference_name.rsplit("::").next() else {
            continue;
        };
        by_last_segment
            .entry(last_segment.to_string())
            .or_default()
            .push(pkg);
    }
    Ok(by_last_segment)
}
