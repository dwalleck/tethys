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

impl Index {
    /// Pub top-level Rust symbols with no cross-package use evidence,
    /// ordered by (file, line, name) for deterministic output.
    ///
    /// Evidence channel consulted here: resolved refs whose referencing
    /// file belongs to a different `arch_packages` row than the declaring
    /// file. Same-package refs are NOT evidence against a candidate —
    /// being used only inside its own package is the candidate condition.
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
             SELECT s.name, s.kind, f.path, s.line
             FROM symbols s
             JOIN files f               ON f.id = s.file_id
             JOIN arch_file_packages fp ON fp.file_id = s.file_id
             WHERE s.visibility = 'public'
               AND f.language = 'rust'
               AND s.kind IN {CANDIDATE_KINDS_SQL}
               AND s.id NOT IN (SELECT symbol_id FROM cross_pkg_refs)
             ORDER BY f.path, s.line, s.name",
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok(VisibilityFinding {
                name: row.get(0)?,
                kind: row.get(1)?,
                file: row.get(2)?,
                line: row.get(3)?,
                tier: Tier::Definite,
                demotions: Vec::new(),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
