//! Type-hierarchy queries (tethys-j2r1): walk `inherit` edges up
//! (supertypes) or down (subtypes).
//!
//! Edges come from `impl Trait for Type`, supertrait bounds, and C# base
//! lists. Unresolved edges (external supertypes like `Display`) surface as
//! name-only nodes in the up walk — they are real hierarchy facts even
//! though no in-crate symbol backs them. Method-level inherit MARKERS
//! (`in_symbol` = a method) exist for dead-code suppression and are
//! excluded from both walks. A type-level edge whose subtype could not be
//! anchored (`in_symbol` NULL — cross-file impl) is invisible to the down
//! walk; documented degrade, measured 3/37 on the self-index.
//!
//! Deliberate deviation from the recursive-CTE graph-query convention:
//! unresolved supertypes are name-only leaves with no symbol id, so the
//! frontier mixes ids and names — a Rust-side BFS expresses that
//! directly; a CTE cannot carry the name-leaf branch.

use std::collections::HashSet;

use rusqlite::params;
use serde::Serialize;
use tracing::trace;

use super::Index;
use crate::error::{Error, Result};

/// Walk direction for [`Index::get_type_hierarchy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyDirection {
    /// Supertypes only (implemented traits, extended bases).
    Up,
    /// Subtypes only (implementors, derived types).
    Down,
    /// Both walks.
    Both,
}

/// One hierarchy step. Nodes reference resolution bound to a symbol carry
/// location; unresolved supertypes (external traits) carry only the name.
#[derive(Debug, Clone, Serialize)]
pub struct HierarchyNode {
    /// Type name (bare identifier).
    pub name: String,
    /// Raw `symbols.kind` text for symbol-backed nodes; `None` for external names.
    pub kind: Option<String>,
    /// Workspace-relative declaring file for symbol-backed nodes.
    pub file: Option<String>,
    /// 1-based declaration line for symbol-backed nodes.
    pub line: Option<u32>,
    /// 1 = direct super/subtype of the queried type.
    pub depth: u32,
}

/// Output of [`Index::get_type_hierarchy`].
#[derive(Debug, Clone, Serialize)]
pub struct TypeHierarchy {
    /// The queried type as found in the index.
    pub name: String,
    /// Supertypes (traits implemented / bases extended), transitive.
    pub up: Vec<HierarchyNode>,
    /// Subtypes (implementors / derived types), transitive.
    pub down: Vec<HierarchyNode>,
}

const CONTAINER_KINDS_SQL: &str = "('struct','class','enum','trait','interface','type_alias')";

impl Index {
    /// Walk the type hierarchy from the type named `name`.
    ///
    /// Errors with [`Error::NotFound`] when no container-kind symbol has
    /// that name. Cycle-guarded (a visited set bounds both walks even on
    /// malformed edge data).
    pub fn get_type_hierarchy(
        &self,
        name: &str,
        direction: HierarchyDirection,
    ) -> Result<TypeHierarchy> {
        trace!(name, "Querying type hierarchy");
        let conn = self.connection()?;

        let mut id_stmt = conn.prepare(&format!(
            "SELECT id FROM symbols WHERE name = ?1 AND kind IN {CONTAINER_KINDS_SQL}"
        ))?;
        let roots: Vec<i64> = id_stmt
            .query_map([name], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        if roots.is_empty() {
            return Err(Error::NotFound(format!("type: {name}")));
        }

        let up = if matches!(direction, HierarchyDirection::Up | HierarchyDirection::Both) {
            walk_up(&conn, &roots)?
        } else {
            Vec::new()
        };
        let down = if matches!(
            direction,
            HierarchyDirection::Down | HierarchyDirection::Both
        ) {
            walk_down(&conn, &roots)?
        } else {
            Vec::new()
        };

        Ok(TypeHierarchy {
            name: name.to_string(),
            up,
            down,
        })
    }
}

/// Transitive supertype walk: resolved targets recurse, unresolved
/// (external) names are leaves, deduped by name. Frontier of resolved
/// symbol ids; the visited set is the cycle guard.
fn walk_up(conn: &rusqlite::Connection, roots: &[i64]) -> Result<Vec<HierarchyNode>> {
    let mut up = Vec::new();
    {
        let mut visited: HashSet<i64> = roots.iter().copied().collect();
        let mut frontier = roots.to_vec();
        let mut depth = 1u32;
        let mut stmt = conn.prepare(
            "SELECT r.symbol_id, r.reference_name, s.name, s.kind, f.path, s.line
                 FROM refs r
                 LEFT JOIN symbols s ON s.id = r.symbol_id
                 LEFT JOIN files f ON f.id = s.file_id
                 WHERE r.kind = 'inherit' AND r.in_symbol_id = ?1",
        )?;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for id in frontier {
                for row in stmt.query_map(params![id], |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<u32>>(5)?,
                    ))
                })? {
                    let (sid, ref_name, sname, skind, sfile, sline) = row?;
                    match sid {
                        Some(sid) => {
                            if visited.insert(sid) {
                                up.push(HierarchyNode {
                                    name: sname.unwrap_or_default(),
                                    kind: skind,
                                    file: sfile,
                                    line: sline,
                                    depth,
                                });
                                next.push(sid);
                            }
                        }
                        None => {
                            if let Some(n) = ref_name {
                                // External supertype: name-only leaf,
                                // deduped by name.
                                if up.iter().all(|u| u.kind.is_some() || u.name != n) {
                                    up.push(HierarchyNode {
                                        name: n,
                                        kind: None,
                                        file: None,
                                        line: None,
                                        depth,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            frontier = next;
            depth += 1;
        }
    }
    Ok(up)
}

/// Transitive subtype walk: anchored `in_symbol`s of edges pointing at the
/// frontier — container kinds only (method markers excluded).
fn walk_down(conn: &rusqlite::Connection, roots: &[i64]) -> Result<Vec<HierarchyNode>> {
    let mut down = Vec::new();
    {
        let mut visited: HashSet<i64> = roots.iter().copied().collect();
        let mut frontier = roots.to_vec();
        let mut depth = 1u32;
        let mut stmt = conn.prepare(&format!(
            "SELECT s.id, s.name, s.kind, f.path, s.line
                 FROM refs r
                 JOIN symbols s ON s.id = r.in_symbol_id
                 JOIN files f ON f.id = s.file_id
                 WHERE r.kind = 'inherit' AND r.symbol_id = ?1
                   AND s.kind IN {CONTAINER_KINDS_SQL}"
        ))?;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for id in frontier {
                for row in stmt.query_map(params![id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, u32>(4)?,
                    ))
                })? {
                    let (sid, sname, skind, sfile, sline) = row?;
                    if visited.insert(sid) {
                        down.push(HierarchyNode {
                            name: sname,
                            kind: Some(skind),
                            file: Some(sfile),
                            line: Some(sline),
                            depth,
                        });
                        next.push(sid);
                    }
                }
            }
            frontier = next;
            depth += 1;
        }
    }
    Ok(down)
}

#[cfg(test)]
mod tests {
    use super::CONTAINER_KINDS_SQL;
    use crate::types::SymbolKind;

    /// The SQL literal and `SymbolKind::is_container` must agree — the SQL
    /// cannot derive from the method, so this fence catches drift when a
    /// container kind is added to one side only.
    #[test]
    fn container_kinds_sql_matches_is_container() {
        let all = [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Struct,
            SymbolKind::Class,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::Interface,
            SymbolKind::Const,
            SymbolKind::Static,
            SymbolKind::Module,
            SymbolKind::TypeAlias,
            SymbolKind::Macro,
            SymbolKind::EnumVariant,
            SymbolKind::StructField,
            SymbolKind::Property,
            SymbolKind::Event,
            SymbolKind::Delegate,
        ];
        for kind in all {
            let quoted = format!("'{}'", kind.as_str());
            assert_eq!(
                CONTAINER_KINDS_SQL.contains(&quoted),
                kind.is_container(),
                "SQL list and is_container disagree on {kind:?}"
            );
        }
    }
}
