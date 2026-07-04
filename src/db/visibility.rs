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
             SELECT s.name, s.kind, f.path, s.line, fp.package_id, p.name
             FROM symbols s
             JOIN files f               ON f.id = s.file_id
             JOIN arch_file_packages fp ON fp.file_id = s.file_id
             JOIN arch_packages p       ON p.id = fp.package_id
             WHERE s.visibility = 'public'
               AND f.language = 'rust'
               AND s.kind IN {CANDIDATE_KINDS_SQL}
               AND s.id NOT IN (SELECT symbol_id FROM cross_pkg_refs)
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
            })
        })?;
        let mut candidates = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        // Channel (b): one pass over imports rows. Named rows key
        // (crate ident head, item name) → importing packages; crate-glob
        // rows key the head alone.
        let mut named_imports: std::collections::HashMap<(String, String), Vec<i64>> =
            std::collections::HashMap::new();
        let mut glob_imports: std::collections::HashMap<String, Vec<i64>> =
            std::collections::HashMap::new();
        let mut import_stmt = conn.prepare(
            "SELECT i.symbol_name, i.source_module, fp.package_id
             FROM imports i
             JOIN arch_file_packages fp ON fp.file_id = i.file_id",
        )?;
        let import_rows = import_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in import_rows {
            let (symbol_name, source_module, pkg) = row?;
            let Some(head) = source_module.split("::").next().filter(|h| !h.is_empty()) else {
                continue;
            };
            if symbol_name == "*" {
                glob_imports.entry(head.to_string()).or_default().push(pkg);
            } else {
                named_imports
                    .entry((head.to_string(), symbol_name))
                    .or_default()
                    .push(pkg);
            }
        }
        candidates.retain(|c| {
            // Owned keys: HashMap's Borrow lookup can't be fed (&str, &str)
            // for a (String, String) key (same shape as deprecated.rs's
            // ambiguity set); d is small — tens, not 10^5.
            let key = (c.crate_ident.clone(), c.name.clone());
            let imported_elsewhere = named_imports
                .get(&key)
                .into_iter()
                .flatten()
                .chain(glob_imports.get(&c.crate_ident).into_iter().flatten())
                .any(|&pkg| pkg != c.package_id);
            !imported_elsewhere
        });

        Ok(candidates
            .into_iter()
            .map(|c| VisibilityFinding {
                name: c.name,
                kind: c.kind,
                file: c.file,
                line: c.line,
                tier: Tier::Definite,
                demotions: Vec::new(),
            })
            .collect())
    }
}
