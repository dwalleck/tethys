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
//!
//! Known limitation (tethys-ygjx): functions used only as VALUES
//! (callbacks) produce no ref at all, so a cross-crate callback-only use
//! leaves evidence solely when an import row exists for it; a fully
//! qualified value mention leaves none. The tier demotions absorb the
//! collided- and reachable-name cases; a workspace-unique, non-reachable,
//! callback-only-consumed item would read Definite — the residual risk
//! accepted in the design's negative space until ygjx lands.

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
    ///   glob row (`symbol_name = '*'`) makes the GLOBBED MODULE's items
    ///   nameable in the importing crate, so it excludes exactly that
    ///   module's candidates. This channel exists because the probe
    ///   measured real cross-crate uses whose refs never resolve
    ///   (re-export indirection plus name collisions — tethys-z9mr /
    ///   tethys-53iv classes).
    /// - (c) unresolved refs in another package whose qualified
    ///   `reference_name` last segment equals the candidate's name (the
    ///   `pkg::mod::item()` call-without-import shape; jdly Path B
    ///   mechanics over the `idx_refs_unresolved` partial index). Matching
    ///   is deliberately last-segment-only, not crate-prefix-anchored:
    ///   evidence here SUPPRESSES an accusation, so a wide net is the
    ///   conservative direction.
    ///
    /// Unless `workspace_closed`, candidates nameable from outside the
    /// indexed workspace (root-reachable through an all-public module
    /// chain) carry the [`Demotion::RootReachable`] Maybe ceiling — the
    /// index cannot observe external consumers, and publishedness is a
    /// release-process fact only the caller can assert.
    pub(crate) fn get_visibility_candidates(
        &self,
        workspace_closed: bool,
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

        let named_imports = import_evidence(&conn)?;
        let unresolved_qualified = unresolved_qualified_evidence(&conn)?;
        let glob_rows = glob_import_rows(&conn)?;
        let shared_names = shared_names(&conn)?;
        // With the ceiling lifted, the modules map is never consulted —
        // skip the pass entirely.
        let modules = if workspace_closed {
            None
        } else {
            Some(module_visibility_map(&conn)?)
        };

        candidates.retain(|c| {
            // Owned keys: HashMap's Borrow lookup can't be fed (&str, &str)
            // for a (String, String) key (same shape as deprecated.rs's
            // ambiguity set); d is small — tens, not 10^5.
            let key = (c.crate_ident.clone(), c.name.clone());
            let used_elsewhere = named_imports
                .get(&key)
                .into_iter()
                .flatten()
                .chain(unresolved_qualified.get(&c.name).into_iter().flatten())
                .any(|&pkg| pkg != c.package_id)
                // A cross-package glob import covers exactly the items of
                // the module it globs — `use x::sub::*` is evidence for
                // sub's candidates, never the whole crate (a head-keyed
                // check suppressed 5 true fig_auth candidates on the q-cli
                // oracle run).
                || glob_rows.iter().any(|(source, pkg)| {
                    *pkg != c.package_id
                        && crate_glob_covers(source, &c.crate_ident, &c.module_path)
                });
            !used_elsewhere
        });

        Ok(candidates
            .into_iter()
            .map(|c| {
                // Demotions push in enum order so the vec is canonical
                // (byte-stable JSON across runs — design C11).
                let mut demotions = Vec::new();
                // C4: ANY same-named symbol — any kind, visibility, or
                // language, twins included — makes absence-of-evidence
                // untrustworthy: tethys-53iv can steal a collided name's
                // qualified refs and destroy their text.
                if shared_names.contains(&c.name) {
                    demotions.push(Demotion::SharedName);
                }
                // C7: externally nameable items keep the Maybe ceiling
                // unless the caller asserts the workspace is closed.
                if let Some(modules) = &modules
                    && is_root_reachable(&c.module_path, modules)
                {
                    demotions.push(Demotion::RootReachable);
                }
                // C6: a glob import row in the candidate's own package
                // targeting its module — a glob re-export would publish it
                // with no per-item ref (tethys-pv7w), so Definite would be
                // unsafe. Plain same-package `use m::*` also demotes:
                // `is_reexport` isn't persisted in the imports table, so
                // pub and non-pub globs are indistinguishable at query
                // time (conservative direction).
                if glob_rows.iter().any(|(source, pkg)| {
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

/// Does a CROSS-package glob import (`use crate_ident::…::*`) cover the
/// candidate's module? The glob's source maps to a `crate::…` module path
/// by replacing its crate-ident head: `g_lib` → `crate` (root items),
/// `g_lib::sub` → `crate::sub`. Exact module equality — a glob makes only
/// the globbed module's items nameable.
fn crate_glob_covers(source_module: &str, crate_ident: &str, module_path: &str) -> bool {
    let Some(rest) = source_module.strip_prefix(crate_ident) else {
        return false;
    };
    if rest.is_empty() {
        return module_path == "crate";
    }
    rest.strip_prefix("::")
        .is_some_and(|sub| module_path.strip_prefix("crate::") == Some(sub))
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

/// Record one module row into the visibility map keyed by (parent
/// `module_path`, module name). Duplicate keys (cfg-gated twin modules,
/// path collisions) resolve as any-public-wins: if one twin is public the
/// chain COULD be nameable, and over-treating as reachable only demotes
/// to Maybe — the suppression-safe direction.
fn record_module(
    map: &mut HashMap<(String, String), bool>,
    parent: &str,
    name: &str,
    is_public: bool,
) {
    let entry = map
        .entry((parent.to_string(), name.to_string()))
        .or_insert(false);
    *entry = *entry || is_public;
}

/// Modules-by-location map for [`is_root_reachable`]: one SQL pass over
/// module-kind symbols (m ≈ 10^4 production).
fn module_visibility_map(conn: &rusqlite::Connection) -> Result<HashMap<(String, String), bool>> {
    let mut map = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT name, module_path, visibility FROM symbols WHERE kind = 'module'")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (name, parent, visibility) = row?;
        record_module(&mut map, &parent, &name, visibility == "public");
    }
    Ok(map)
}

/// Can a symbol whose `module_path` is e.g. `crate::a::b` be named from
/// outside the crate through an all-public module chain?
///
/// Walk: for each segment after `crate`, the module row keyed by
/// (path-so-far, segment) must be public. Items at the crate root
/// (`module_path` of `crate` or empty — files outside the module tree)
/// are reachable by definition. A MISSING module row is treated as
/// reachable: unknown chains get the Maybe ceiling, never a false
/// Definite (documented conservative choice, unit-fenced).
///
/// Re-exports also add reachability, but re-exported candidates are
/// excluded outright upstream (design C5), so the chain walk never sees
/// them.
fn is_root_reachable(module_path: &str, modules: &HashMap<(String, String), bool>) -> bool {
    if module_path.is_empty() || module_path == "crate" {
        return true;
    }
    let Some(rest) = module_path.strip_prefix("crate::") else {
        // Non-crate-rooted path (shouldn't occur for Rust symbols): treat
        // as reachable — unknown ⇒ Maybe ceiling, never false Definite.
        return true;
    };
    let mut parent = String::from("crate");
    for segment in rest.split("::") {
        match modules.get(&(parent.clone(), segment.to_string())) {
            Some(true) => {}
            Some(false) => return false,
            None => return true,
        }
        parent.push_str("::");
        parent.push_str(segment);
    }
    true
}

/// Names carried by MORE THAN ONE symbol row anywhere in the index —
/// COUNT of rows, not distinct locations, so cfg-twin duplicates in one
/// file count as shared. One SQL pass over symbols (~10^6 production).
fn shared_names(conn: &rusqlite::Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT name FROM symbols GROUP BY name HAVING COUNT(*) > 1")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
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

/// Channel (b) named-import lookup map, one pass over `imports` rows:
/// keyed by (crate-ident head of `source_module`, item name) → importing
/// packages. Glob rows are handled separately by [`glob_import_rows`] +
/// [`crate_glob_covers`], module-precisely.
fn import_evidence(conn: &rusqlite::Connection) -> Result<HashMap<(String, String), Vec<i64>>> {
    let mut named: HashMap<(String, String), Vec<i64>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT i.symbol_name, i.source_module, fp.package_id
         FROM imports i
         JOIN arch_file_packages fp ON fp.file_id = i.file_id
         WHERE i.symbol_name != '*'",
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
        named
            .entry((head.to_string(), symbol_name))
            .or_default()
            .push(pkg);
    }
    Ok(named)
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

#[cfg(test)]
mod tests {
    use super::{is_root_reachable, module_matches, record_module};
    use std::collections::HashMap;

    /// Design C8 unit fence. Rows 1-2 re-encode the self-index chains the
    /// design-time falsifier ran against source text (`mod db;` private →
    /// `DeprecatedSymbol` unreachable; `pub mod cargo;` → reachable). Rows
    /// 3-5 pin the documented conservative choices: crate-root items
    /// reachable, MISSING module rows reachable (unknown ⇒ Maybe ceiling),
    /// any-public-wins on duplicate keys. Kills: keying a module by its
    /// own path instead of its parent's, the walk stopping after one
    /// segment, missing-row panics.
    #[test]
    fn root_reachability_chain_walk() {
        let mut modules = HashMap::new();
        record_module(&mut modules, "crate", "db", false);
        record_module(&mut modules, "crate::db", "deprecated", false);
        record_module(&mut modules, "crate", "cargo", true);
        record_module(&mut modules, "crate", "api", true);
        record_module(&mut modules, "crate::api", "inner", true);
        // duplicate key: cfg-twin module, one public — public wins
        record_module(&mut modules, "crate", "twin_mod", false);
        record_module(&mut modules, "crate", "twin_mod", true);

        // (module_path, expected reachable)
        let cases = [
            ("crate::db::deprecated", false), // private mid-chain
            ("crate::db", false),             // private first hop
            ("crate::cargo", true),           // all-pub single hop
            ("crate::api::inner", true),      // all-pub two hops
            ("crate", true),                  // item at crate root
            ("", true),                       // outside module tree
            ("crate::unknown_mod", true),     // missing row: conservative
            ("crate::twin_mod", true),        // any-public-wins
        ];
        for (path, expected) in cases {
            assert_eq!(
                is_root_reachable(path, &modules),
                expected,
                "reachability of {path:?}"
            );
        }
    }

    /// Channel-(b) glob coverage is module-exact: `g_lib` covers only
    /// crate-root items, `g_lib::sub` only `crate::sub` — never the whole
    /// crate (the q-cli `fig_auth` over-suppression bug) and never an
    /// ident-prefix false match (`g_lib2` vs `g_lib`).
    #[test]
    fn crate_glob_coverage_is_module_exact() {
        use super::crate_glob_covers;
        assert!(crate_glob_covers("g_lib", "g_lib", "crate"));
        assert!(crate_glob_covers("g_lib::sub", "g_lib", "crate::sub"));
        assert!(!crate_glob_covers("g_lib::sub", "g_lib", "crate"));
        assert!(!crate_glob_covers("g_lib::sub", "g_lib", "crate::sub2"));
        assert!(!crate_glob_covers("g_lib", "g_lib", "crate::sub"));
        assert!(!crate_glob_covers("g_lib2", "g_lib", "crate"));
        assert!(!crate_glob_covers("other", "g_lib", "crate"));
    }

    /// C6 helper: absolute, relative, and self-relative source modules all
    /// denote the same module; suffix matching respects the `::` boundary
    /// (`crate::xinner` must not match source `inner`).
    #[test]
    fn module_match_shapes() {
        assert!(module_matches("crate::inner", "inner"));
        assert!(module_matches("crate::inner", "crate::inner"));
        assert!(module_matches("crate::inner", "self::inner"));
        assert!(module_matches("crate::a::inner", "a::inner"));
        assert!(!module_matches("crate::xinner", "inner"));
        assert!(!module_matches("crate::inner", "other"));
    }
}
