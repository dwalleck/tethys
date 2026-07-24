//! SQLite-backed graph queries implemented as concrete `Index` operations.

use std::collections::{HashMap, HashSet};

use rusqlite::OptionalExtension;

use super::Index;
use super::helpers::{row_to_indexed_file, row_to_symbol};
use crate::error::{Error, Result};
use crate::graph::{CallerInfo, FileDepInfo, FileImpact, FilePath, SymbolImpact};
use crate::types::{Cycle, FileId, SymbolId};

/// Default maximum depth for recursive graph traversals.
///
/// Prevents runaway recursion in deeply nested or cyclic dependency graphs.
/// Can be overridden by passing an explicit `max_depth` parameter.
pub(crate) const DEFAULT_MAX_DEPTH: u32 = 50;

/// SQL fragment requiring a call edge to have at least one supporting ref
/// whose band (per the `refs_banded` view — the single home of the ADR-0003
/// mapping) is not speculative. Empty when the filter is off. `edge` is the
/// `call_edges` alias in the enclosing query.
fn edge_support_filter(exclude_speculative: bool, edge: &str) -> String {
    if exclude_speculative {
        format!(
            " AND EXISTS (SELECT 1 FROM refs_banded rb
                          WHERE rb.in_symbol_id = {edge}.caller_symbol_id
                            AND rb.symbol_id = {edge}.callee_symbol_id
                            AND rb.band != 'speculative')"
        )
    } else {
        String::new()
    }
}

impl Index {
    /// Get symbols that directly call/reference the given symbol.
    ///
    /// `exclude_speculative` drops call edges whose every supporting ref
    /// bands speculative in `refs_banded`.
    pub fn get_callers(
        &self,
        symbol_id: SymbolId,
        exclude_speculative: bool,
    ) -> Result<Vec<CallerInfo>> {
        let conn = self.connection()?;

        // Use pre-computed call_edges table for efficient indexed lookup
        let mut stmt = conn.prepare(
            &"SELECT
                s.id, s.file_id, s.name, s.module_path, s.qualified_name,
                s.kind, s.line, s.column, s.end_line, s.end_column,
                s.signature, s.visibility, s.parent_symbol_id,
                ce.call_count
             FROM call_edges ce
             JOIN symbols s ON s.id = ce.caller_symbol_id
             WHERE ce.callee_symbol_id = ?1{exclusion}
             ORDER BY s.qualified_name"
                .replace(
                    "{exclusion}",
                    &edge_support_filter(exclude_speculative, "ce"),
                ),
        )?;

        let callers = stmt
            .query_map([symbol_id.as_i64()], |row| {
                let symbol = row_to_symbol(row)?;
                // Safety: call_count is a non-negative aggregate count
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "call_count is a non-negative SQL COUNT aggregate"
                )]
                let ref_count: usize = row.get::<_, i64>(13)? as usize;

                Ok(CallerInfo {
                    symbol,
                    reference_count: ref_count,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(callers)
    }

    /// Get symbols that the given symbol directly calls/references.
    pub fn get_callees(&self, symbol_id: SymbolId) -> Result<Vec<crate::types::Symbol>> {
        let conn = self.connection()?;

        // Use pre-computed call_edges table for efficient indexed lookup
        let mut stmt = conn.prepare(
            "SELECT
                s.id, s.file_id, s.name, s.module_path, s.qualified_name,
                s.kind, s.line, s.column, s.end_line, s.end_column,
                s.signature, s.visibility, s.parent_symbol_id,
                ce.call_count
             FROM call_edges ce
             JOIN symbols s ON s.id = ce.callee_symbol_id
             WHERE ce.caller_symbol_id = ?1
             ORDER BY s.qualified_name",
        )?;

        let callees = stmt
            .query_map([symbol_id.as_i64()], row_to_symbol)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(callees)
    }

    /// Get transitive callers for impact analysis.
    pub fn get_transitive_callers(
        &self,
        symbol_id: SymbolId,
        max_depth: Option<u32>,
        exclude_speculative: bool,
    ) -> Result<SymbolImpact> {
        let max_depth = max_depth.unwrap_or(DEFAULT_MAX_DEPTH);

        let conn = self.connection()?;

        // Use recursive CTE with call_edges table for efficient traversal
        let mut stmt = conn.prepare(
            &"WITH RECURSIVE caller_tree(symbol_id, depth) AS (
                -- Base case: direct callers from call_edges
                SELECT caller_symbol_id, 1
                FROM call_edges ce
                WHERE callee_symbol_id = ?1{exclusion}

                UNION

                -- Recursive case: callers of callers
                SELECT ce.caller_symbol_id, ct.depth + 1
                FROM call_edges ce
                JOIN caller_tree ct ON ce.callee_symbol_id = ct.symbol_id
                WHERE ct.depth < ?2{exclusion}
            )
            SELECT DISTINCT
                s.id, s.file_id, s.name, s.module_path, s.qualified_name,
                s.kind, s.line, s.column, s.end_line, s.end_column,
                s.signature, s.visibility, s.parent_symbol_id,
                MIN(ct.depth) as min_depth
            FROM caller_tree ct
            JOIN symbols s ON s.id = ct.symbol_id
            GROUP BY s.id
            ORDER BY min_depth, s.qualified_name"
                .replace(
                    "{exclusion}",
                    &edge_support_filter(exclude_speculative, "ce"),
                ),
        )?;

        let mut direct_callers = Vec::new();
        let mut transitive_callers = Vec::new();

        let rows = stmt.query_map(rusqlite::params![symbol_id.as_i64(), max_depth], |row| {
            let symbol = row_to_symbol(row)?;
            // Safety: CTE depth is bounded by max_depth (u32), so i64 value fits in u32
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "CTE depth bounded by max_depth (u32)"
            )]
            let depth: u32 = row.get::<_, i64>(13)? as u32;
            Ok((symbol, depth))
        })?;

        for row in rows {
            let (symbol, depth) = row?;

            let caller_info = CallerInfo {
                symbol,
                reference_count: 1,
            };

            if depth == 1 {
                direct_callers.push(caller_info);
            } else {
                transitive_callers.push(caller_info);
            }
        }

        Ok(SymbolImpact {
            direct_callers,
            transitive_callers,
        })
    }
}

/// Recursive-CTE prefix shared by the dependent traversals below: walks
/// `file_deps` edges upward from target file `?1`, bounding depth at `?2`.
/// Callers append their own projection over `dependent_tree(file_id, depth)`.
const DEPENDENT_TREE_CTE: &str = "WITH RECURSIVE dependent_tree(file_id, depth) AS (
                -- Base case: direct dependents
                SELECT DISTINCT fd.from_file_id, 1
                FROM file_deps fd
                WHERE fd.to_file_id = ?1

                UNION

                -- Recursive case: dependents of dependents
                SELECT DISTINCT fd.from_file_id, dt.depth + 1
                FROM file_deps fd
                JOIN dependent_tree dt ON fd.to_file_id = dt.file_id
                WHERE dt.depth < ?2
            )";

impl Index {
    /// Get direct and transitive dependents for file impact analysis.
    pub fn get_transitive_dependents(
        &self,
        file_id: FileId,
        max_depth: Option<u32>,
    ) -> Result<FileImpact> {
        let max_depth = max_depth.unwrap_or(DEFAULT_MAX_DEPTH);
        let target = self
            .get_file_by_id(file_id)?
            .ok_or_else(|| Error::NotFound(format!("file id: {}", file_id.as_i64())))?;

        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "{DEPENDENT_TREE_CTE}
            SELECT DISTINCT
                f.id, f.path, f.language, f.mtime_ns, f.size_bytes, f.content_hash, f.indexed_at,
                MIN(dt.depth) as min_depth
            FROM dependent_tree dt
            JOIN files f ON f.id = dt.file_id
            GROUP BY f.id
            ORDER BY min_depth, f.path"
        ))?;

        let mut direct_dependents = Vec::new();
        let mut transitive_dependents = Vec::new();

        let rows = stmt.query_map(rusqlite::params![file_id.as_i64(), max_depth], |row| {
            let file = row_to_indexed_file(row)?;
            // Safety: CTE depth is bounded by max_depth (u32), so i64 value fits in u32
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "CTE depth bounded by max_depth (u32)"
            )]
            let depth: u32 = row.get::<_, i64>(7)? as u32;
            Ok((file, depth))
        })?;

        for row in rows {
            let (file, depth) = row?;

            let dep_info = FileDepInfo { file, ref_count: 1 };

            if depth == 1 {
                direct_dependents.push(dep_info);
            } else {
                transitive_dependents.push(dep_info);
            }
        }

        Ok(FileImpact {
            target,
            direct_dependents,
            transitive_dependents,
        })
    }

    /// Get transitive dependent file IDs without hydrating graph DTOs.
    ///
    /// The root file is validated but excluded from the result unless a cycle
    /// reaches it again. Traversal uses the same default depth as file impact.
    pub fn get_transitive_dependent_file_ids(&self, file_id: FileId) -> Result<Vec<FileId>> {
        self.get_file_by_id(file_id)?
            .ok_or_else(|| Error::NotFound(format!("file id: {}", file_id.as_i64())))?;

        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "{DEPENDENT_TREE_CTE}
            SELECT f.id
            FROM dependent_tree dt
            JOIN files f ON f.id = dt.file_id
            GROUP BY f.id
            ORDER BY MIN(dt.depth), f.path"
        ))?;

        let file_ids = stmt
            .query_map(
                rusqlite::params![file_id.as_i64(), DEFAULT_MAX_DEPTH],
                |row| row.get::<_, i64>(0).map(FileId::from),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(file_ids)
    }

    /// Find the shortest dependency path between two files.
    pub fn find_dependency_path(
        &self,
        from_file_id: FileId,
        to_file_id: FileId,
    ) -> Result<Option<FilePath>> {
        // Same file - trivial path
        if from_file_id == to_file_id {
            let file = self
                .get_file_by_id(from_file_id)?
                .ok_or_else(|| Error::NotFound(format!("file id: {}", from_file_id.as_i64())))?;
            return Ok(Some(FilePath::single(file)));
        }

        // BFS to find shortest path using recursive CTE
        // We search forward from `from` through dependencies (what does `from` depend on?)
        let max_depth = DEFAULT_MAX_DEPTH;

        // Scope the connection lock to just the query execution
        let file_ids: Option<Vec<i64>> = {
            let conn = self.connection()?;

            let mut stmt = conn.prepare(
                "WITH RECURSIVE path_search(file_id, path, depth) AS (
                    -- Start from the source file
                    SELECT ?1, CAST(?1 AS TEXT), 0

                    UNION

                    -- Follow dependencies (files that the current file depends on)
                    SELECT fd.to_file_id,
                           ps.path || ',' || fd.to_file_id,
                           ps.depth + 1
                    FROM file_deps fd
                    JOIN path_search ps ON fd.from_file_id = ps.file_id
                    WHERE ps.depth < ?3
                )
                SELECT path
                FROM path_search
                WHERE file_id = ?2
                ORDER BY depth
                LIMIT 1",
            )?;

            let path_str: Option<String> = stmt
                .query_row(
                    rusqlite::params![from_file_id.as_i64(), to_file_id.as_i64(), max_depth],
                    |row| row.get(0),
                )
                .optional()?;

            match path_str {
                Some(s) => Some(parse_path_ids(&s)?),
                None => None,
            }
        };

        let Some(file_ids) = file_ids else {
            return Ok(None);
        };

        // Fetch files for each ID in the path
        let mut files = Vec::with_capacity(file_ids.len());
        for id in file_ids {
            let file = self
                .get_file_by_id(FileId::from(id))?
                .ok_or_else(|| Error::NotFound(format!("file id: {id}")))?;
            files.push(file);
        }

        // Use validated constructor - invariants guaranteed by construction
        Ok(FilePath::new(files))
    }

    /// Detect circular dependencies in the indexed workspace.
    pub fn detect_cycles(&self) -> Result<Vec<Cycle>> {
        let adj = self.build_adjacency_list()?;
        self.find_cycles_dfs(&adj)
    }
}

// === Helper methods for Index ===

impl Index {
    /// Build an adjacency list representation of the dependency graph.
    ///
    /// Returns a map from file ID to list of files it depends on (outgoing edges).
    fn build_adjacency_list(&self) -> Result<HashMap<FileId, Vec<FileId>>> {
        let conn = self.connection()?;

        let mut stmt = conn.prepare("SELECT from_file_id, to_file_id FROM file_deps")?;

        let rows = stmt.query_map([], |row| {
            let from: i64 = row.get(0)?;
            let to: i64 = row.get(1)?;
            Ok((FileId::from(from), FileId::from(to)))
        })?;

        let mut adj: HashMap<FileId, Vec<FileId>> = HashMap::new();
        for result in rows {
            let (from, to) = result?;
            adj.entry(from).or_default().push(to);
        }

        Ok(adj)
    }

    /// DFS-based cycle detection.
    ///
    /// Uses standard cycle detection with visited set and recursion stack.
    /// When a back edge is found, reconstructs the cycle path.
    fn find_cycles_dfs(&self, adj: &HashMap<FileId, Vec<FileId>>) -> Result<Vec<Cycle>> {
        let mut visited: HashSet<FileId> = HashSet::new();
        let mut rec_stack: HashSet<FileId> = HashSet::new();
        let mut path: Vec<FileId> = Vec::new();
        let mut cycles: Vec<Vec<FileId>> = Vec::new();

        // Get all nodes that participate in the graph
        let all_nodes: HashSet<FileId> = adj
            .iter()
            .flat_map(|(from, tos)| std::iter::once(*from).chain(tos.iter().copied()))
            .collect();

        let edge_count: usize = adj.values().map(Vec::len).sum();
        tracing::debug!(
            node_count = all_nodes.len(),
            edge_count = edge_count,
            "Starting cycle detection with DFS"
        );

        for &start in &all_nodes {
            if !visited.contains(&start) {
                dfs_visit_for_cycles(
                    start,
                    adj,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        let raw_cycle_count = cycles.len();

        // Deduplicate cycles (same cycle can be discovered from different starting nodes)
        let unique_cycles = deduplicate_cycles(cycles);

        tracing::debug!(
            raw_cycles = raw_cycle_count,
            unique_cycles = unique_cycles.len(),
            "DFS traversal complete, deduplicating cycles"
        );

        // Convert file IDs to Cycle structs with paths
        let result: Result<Vec<Cycle>> = unique_cycles
            .into_iter()
            .map(|ids| self.ids_to_cycle(&ids))
            .collect();

        if let Ok(ref cycles) = result {
            tracing::info!(cycle_count = cycles.len(), "Cycle detection complete");
        }

        result
    }

    /// Convert a list of file IDs to a `Cycle` struct with file paths.
    fn ids_to_cycle(&self, ids: &[FileId]) -> Result<Cycle> {
        let mut files = Vec::with_capacity(ids.len());

        for (idx, &id) in ids.iter().enumerate() {
            let file = self
                .get_file_by_id(id)
                .map_err(|e| {
                    tracing::error!(
                        error = %e,
                        file_id = id.as_i64(),
                        cycle_position = idx,
                        cycle_length = ids.len(),
                        "Database error while resolving file for cycle"
                    );
                    e
                })?
                .ok_or_else(|| {
                    tracing::error!(
                        file_id = id.as_i64(),
                        cycle_position = idx,
                        cycle_length = ids.len(),
                        "File not found in database but referenced in dependency cycle \
                         (possible data integrity issue)"
                    );
                    Error::NotFound(format!(
                        "file id: {} (position {} in cycle of length {})",
                        id.as_i64(),
                        idx,
                        ids.len()
                    ))
                })?;
            files.push(file.path);
        }

        Ok(Cycle { files })
    }
}

// === Cycle Detection Helper Functions ===

/// Recursive DFS visitor for cycle detection.
///
/// Traverses the graph marking nodes as visited. When a back edge is found
/// (an edge to a node still in the current DFS path/recursion stack), a cycle
/// is recorded. Back edges indicate cycles because we've reached a node we're
/// still in the process of exploring.
fn dfs_visit_for_cycles(
    node: FileId,
    adj: &HashMap<FileId, Vec<FileId>>,
    visited: &mut HashSet<FileId>,
    rec_stack: &mut HashSet<FileId>,
    path: &mut Vec<FileId>,
    cycles: &mut Vec<Vec<FileId>>,
) {
    visited.insert(node);
    rec_stack.insert(node);
    path.push(node);

    if let Some(neighbors) = adj.get(&node) {
        for &neighbor in neighbors {
            if !visited.contains(&neighbor) {
                dfs_visit_for_cycles(neighbor, adj, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(&neighbor) {
                // Back edge found - extract the cycle
                if let Some(cycle_start_idx) = path.iter().position(|&id| id == neighbor) {
                    let cycle: Vec<FileId> = path[cycle_start_idx..].to_vec();
                    cycles.push(cycle);
                }
            }
        }
    }

    path.pop();
    rec_stack.remove(&node);
}

/// Deduplicate cycles by normalizing their representation.
///
/// Two cycles are considered the same if they contain the same nodes in the same
/// circular order, regardless of which node they start with.
///
/// We only normalize the starting point, not direction, because the DFS discovers
/// cycles by following directed edges. In a directed graph, A→B→C→A and C→B→A→C
/// are topologically distinct, so direction is semantically meaningful.
fn deduplicate_cycles(cycles: Vec<Vec<FileId>>) -> Vec<Vec<FileId>> {
    let mut seen: HashSet<Vec<FileId>> = HashSet::new();
    let mut unique: Vec<Vec<FileId>> = Vec::new();

    for cycle in cycles {
        if cycle.is_empty() {
            continue;
        }

        // Normalize: rotate so the smallest ID is first
        let normalized = normalize_cycle(&cycle);

        if seen.insert(normalized.clone()) {
            unique.push(normalized);
        }
    }

    unique
}

/// Normalize a cycle by rotating it so the smallest ID is first.
fn normalize_cycle(cycle: &[FileId]) -> Vec<FileId> {
    if cycle.is_empty() {
        return Vec::new();
    }

    // Find the index of the minimum element
    let min_idx = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| id.as_i64())
        .map_or(0, |(idx, _)| idx);

    // Rotate so minimum is first
    let mut normalized = Vec::with_capacity(cycle.len());
    normalized.extend_from_slice(&cycle[min_idx..]);
    normalized.extend_from_slice(&cycle[..min_idx]);

    normalized
}

/// Parse a comma-separated path string into a vector of i64 IDs.
///
/// Used by path-finding queries that store traversal paths as comma-separated strings
/// in SQL.
fn parse_path_ids(path_str: &str) -> Result<Vec<i64>> {
    path_str
        .split(',')
        .filter(|id| !id.trim().is_empty())
        .map(|id| {
            let trimmed = id.trim();
            trimmed.parse().map_err(|e| {
                Error::Internal(format!(
                    "failed to parse ID '{trimmed}' in path '{path_str}': {e} (possible database corruption)"
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cycle_rotates_to_smallest() {
        // Test the normalization function directly
        let cycle = vec![FileId::from(5), FileId::from(2), FileId::from(8)];
        let normalized = normalize_cycle(&cycle);

        // Should rotate so 2 (smallest) is first
        assert_eq!(normalized[0].as_i64(), 2);
        assert_eq!(normalized[1].as_i64(), 8);
        assert_eq!(normalized[2].as_i64(), 5);
    }

    #[test]
    fn normalize_cycle_handles_empty() {
        let cycle: Vec<FileId> = vec![];
        let normalized = normalize_cycle(&cycle);
        assert!(normalized.is_empty());
    }

    #[test]
    fn normalize_cycle_handles_single_element() {
        let cycle = vec![FileId::from(42)];
        let normalized = normalize_cycle(&cycle);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].as_i64(), 42);
    }
}
