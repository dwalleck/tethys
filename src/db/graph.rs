//! SQLite-backed graph queries implemented as concrete `Index` operations.

use std::collections::{HashMap, HashSet, VecDeque};

use super::Index;
use super::helpers::row_to_symbol;
use crate::error::{Error, Result};
use crate::graph::{FileImpact, FileImpactDependent, FilePath, SymbolImpactCaller};
use crate::types::{CallEdgeSelection, Caller, Cycle, FileId, SymbolId};

/// Default maximum depth for recursive graph traversals.
///
/// Prevents runaway recursion in deeply nested or cyclic dependency graphs.
/// Can be overridden by passing an explicit `max_depth` parameter.
pub(crate) const DEFAULT_MAX_DEPTH: u32 = 50;

/// SQL fragment requiring a call edge to have at least one supporting ref
/// whose band (per the `refs_banded` view — the single home of the ADR-0003
/// mapping) is not speculative. Empty when the filter is off. `edge` is the
/// `call_edges` alias in the enclosing query.
fn edge_support_filter(call_edges: CallEdgeSelection, edge: &str) -> String {
    match call_edges {
        CallEdgeSelection::ExcludeSpeculative => format!(
            " AND EXISTS (SELECT 1 FROM refs_banded rb
                          WHERE rb.in_symbol_id = {edge}.caller_symbol_id
                            AND rb.symbol_id = {edge}.callee_symbol_id
                            AND rb.band != 'speculative')"
        ),
        CallEdgeSelection::All => String::new(),
    }
}

impl Index {
    /// Get symbols that directly call/reference the given symbol.
    ///
    /// [`CallEdgeSelection::ExcludeSpeculative`] drops call edges whose every
    /// supporting reference bands speculative in `refs_banded`.
    pub fn get_callers(
        &self,
        symbol_id: SymbolId,
        call_edges: CallEdgeSelection,
    ) -> Result<Vec<Caller>> {
        let conn = self.connection()?;

        // Use pre-computed call_edges table for efficient indexed lookup
        let mut stmt = conn.prepare(
            &"SELECT
                s.id, s.file_id, s.name, s.module_path, s.qualified_name,
                s.kind, s.line, s.column, s.end_line, s.end_column,
                s.signature, s.visibility, s.parent_symbol_id, s.is_test,
                f.path
             FROM call_edges ce
             JOIN symbols s ON s.id = ce.caller_symbol_id
             JOIN files f ON f.id = s.file_id
             WHERE ce.callee_symbol_id = ?1{exclusion}
             ORDER BY s.qualified_name"
                .replace("{exclusion}", &edge_support_filter(call_edges, "ce")),
        )?;

        let callers = stmt
            .query_map([symbol_id.as_i64()], |row| {
                let symbol = row_to_symbol(row)?;
                Ok(Caller {
                    symbol,
                    file: row.get::<_, String>(14)?.into(),
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
        max_depth: u32,
        call_edges: CallEdgeSelection,
    ) -> Result<Vec<SymbolImpactCaller>> {
        let conn = self.connection()?;

        // Use recursive CTE with call_edges table for efficient traversal
        let mut stmt = conn.prepare(
            &"WITH RECURSIVE caller_tree(symbol_id, depth) AS (
                -- Base case: direct callers from call_edges
                SELECT caller_symbol_id, 1
                FROM call_edges ce
                WHERE callee_symbol_id = ?1 AND ?2 >= 1{exclusion}

                UNION

                -- Recursive case: callers of callers
                SELECT ce.caller_symbol_id, ct.depth + 1
                FROM call_edges ce
                JOIN caller_tree ct ON ce.callee_symbol_id = ct.symbol_id
                WHERE ct.depth < ?2{exclusion}
            )
            SELECT
                s.id, s.file_id, s.name, s.module_path, s.qualified_name,
                s.kind, s.line, s.column, s.end_line, s.end_column,
                s.signature, s.visibility, s.parent_symbol_id, s.is_test,
                f.path, MIN(ct.depth) as min_depth
            FROM caller_tree ct
            JOIN symbols s ON s.id = ct.symbol_id
            JOIN files f ON f.id = s.file_id
            GROUP BY s.id
            ORDER BY min_depth, s.qualified_name"
                .replace("{exclusion}", &edge_support_filter(call_edges, "ce")),
        )?;

        let callers = stmt
            .query_map(rusqlite::params![symbol_id.as_i64(), max_depth], |row| {
                let symbol = row_to_symbol(row)?;
                let file = row.get::<_, String>(14)?.into();
                let depth = row.get::<_, usize>(15)?;
                Ok(SymbolImpactCaller {
                    symbol,
                    file,
                    depth,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(callers)
    }
}

/// Recursive-CTE prefix shared by the dependent traversals below: walks
/// `file_deps` edges upward from target file `?1`, bounding depth at `?2`.
/// The base case's `?2 >= 1` guard makes a zero bound yield no rows (depth
/// zero validates the target and traverses nothing), mirroring the symbol
/// `caller_tree` CTE; consumers passing `DEFAULT_MAX_DEPTH` never engage it.
/// Callers append their own projection over `dependent_tree(file_id, depth)`.
const DEPENDENT_TREE_CTE: &str = "WITH RECURSIVE dependent_tree(file_id, depth) AS (
                -- Base case: direct dependents
                SELECT DISTINCT fd.from_file_id, 1
                FROM file_deps fd
                WHERE fd.to_file_id = ?1 AND ?2 >= 1

                UNION

                -- Recursive case: dependents of dependents
                SELECT DISTINCT fd.from_file_id, dt.depth + 1
                FROM file_deps fd
                JOIN dependent_tree dt ON fd.to_file_id = dt.file_id
                WHERE dt.depth < ?2
            )";

impl Index {
    /// Get direct and transitive dependents for file impact analysis.
    pub fn get_transitive_dependents(&self, file_id: FileId, max_depth: u32) -> Result<FileImpact> {
        let target = self
            .get_file_by_id(file_id)?
            .ok_or_else(|| Error::NotFound(format!("file id: {}", file_id.as_i64())))?;

        let conn = self.connection()?;

        let mut stmt = conn.prepare(&format!(
            "{DEPENDENT_TREE_CTE}
            SELECT
                f.path, MIN(dt.depth) as min_depth
            FROM dependent_tree dt
            JOIN files f ON f.id = dt.file_id
            GROUP BY f.id
            ORDER BY min_depth, f.path"
        ))?;

        let dependents = stmt
            .query_map(rusqlite::params![file_id.as_i64(), max_depth], |row| {
                Ok(FileImpactDependent {
                    file: row.get::<_, String>(0)?.into(),
                    depth: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(FileImpact::new(target.path, dependents))
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
    ///
    /// Directed, forward through `file_deps` edges (what `from` depends
    /// on), shortest by edge count, both endpoints included. Equal
    /// endpoints yield a one-file path; disconnected endpoints yield
    /// `None`. Traversal is capped at [`DEFAULT_MAX_DEPTH`] edges — longer
    /// chains report `None`, matching the depth bound of the recursive CTE
    /// this replaces.
    ///
    /// Storage-owned visited-set BFS over one adjacency load, with the
    /// selected path hydrated in ONE batched statement (tethys-4m9o). The
    /// previous walk-enumerating CTE was non-terminating on cyclic indexes
    /// (tethys-vwrn).
    ///
    /// Unknown ids yield `None` (the [`crate::Tethys`] facade validates
    /// endpoints first); a path member whose `files` row is gone is
    /// `Error::NotFound` — a chain with a hole is not a chain.
    pub fn find_dependency_path(
        &self,
        from_file_id: FileId,
        to_file_id: FileId,
    ) -> Result<Option<FilePath>> {
        let path_ids = if from_file_id == to_file_id {
            vec![from_file_id]
        } else {
            let adj = self.build_adjacency_list()?;
            match bfs_shortest_ids(&adj, from_file_id, to_file_id, DEFAULT_MAX_DEPTH) {
                Some(ids) => ids,
                None => return Ok(None),
            }
        };

        let mut files_by_id = self.get_files_by_ids(&path_ids)?;
        let mut files = Vec::with_capacity(path_ids.len());
        for id in path_ids {
            let file = files_by_id
                .remove(&id)
                .ok_or_else(|| Error::NotFound(format!("file id: {}", id.as_i64())))?;
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

/// Visited-set BFS over the adjacency map: shortest id path `from → to`
/// by edge count, expansion capped at `max_depth` edges.
///
/// Returns the full id sequence including both endpoints, or `None` when
/// `to` is unreachable within the cap. `from == to` never traverses — the
/// caller short-circuits the equal-endpoint path before calling (enforced
/// there; this function would return `None` for it, which is wrong for
/// that case, hence the debug assert).
fn bfs_shortest_ids(
    adj: &HashMap<FileId, Vec<FileId>>,
    from: FileId,
    to: FileId,
    max_depth: u32,
) -> Option<Vec<FileId>> {
    debug_assert!(from != to, "equal endpoints are the caller's short-circuit");

    // parent map doubles as the visited set; `from` is guarded explicitly
    // so a cycle back to the source can never give it a parent.
    let mut parents: HashMap<FileId, FileId> = HashMap::new();
    let mut queue: VecDeque<(FileId, u32)> = VecDeque::new();
    queue.push_back((from, 0));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for &next in adj.get(&current).into_iter().flatten() {
            if next == from || parents.contains_key(&next) {
                continue;
            }
            parents.insert(next, current);
            if next == to {
                // BFS discovery order guarantees minimal depth at first
                // insertion; walk the parent chain back to `from`.
                let mut ids = vec![to];
                let mut cursor = to;
                while let Some(&parent) = parents.get(&cursor) {
                    ids.push(parent);
                    cursor = parent;
                }
                ids.reverse();
                return Some(ids);
            }
            queue.push_back((next, depth + 1));
        }
    }
    None
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

#[cfg(test)]
mod dependency_path_tests {
    //! db-unit fences for the visited-set BFS chain query (tethys-4m9o
    //! C1–C5). Hand-inserted rows; expected outcomes hand-computed in
    //! .tethys-4m9o/plan.md before implementation.

    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::db::Index;
    use crate::error::Error;
    use crate::types::{FileId, Language};

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open index");
        (dir, index)
    }

    fn upsert(index: &mut Index, p: &str) -> FileId {
        index
            .upsert_file(Path::new(p), Language::Rust, 0, 0, None)
            .expect("upsert file")
    }

    fn edge(index: &mut Index, from: FileId, to: FileId) {
        index.insert_file_dependency(from, to).expect("edge");
    }

    fn chain_paths(index: &Index, from: FileId, to: FileId) -> Option<Vec<PathBuf>> {
        index
            .find_dependency_path(from, to)
            .expect("find_dependency_path")
            .map(|p| p.into_files().into_iter().map(|f| f.path).collect())
    }

    /// Tie-break bug class: the 2-edge route must win over the 3-edge one.
    #[test]
    fn shortest_route_wins_over_longer_route() {
        let (_dir, mut index) = temp_index();
        let fa = upsert(&mut index, "src/a.rs");
        let fb = upsert(&mut index, "src/b.rs");
        let fc = upsert(&mut index, "src/c.rs");
        let fd = upsert(&mut index, "src/d.rs");
        let ft = upsert(&mut index, "src/t.rs");
        edge(&mut index, fa, fb);
        edge(&mut index, fb, ft);
        edge(&mut index, fa, fc);
        edge(&mut index, fc, fd);
        edge(&mut index, fd, ft);

        let got = chain_paths(&index, fa, ft).expect("route exists");
        assert_eq!(
            got,
            vec![
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/b.rs"),
                PathBuf::from("src/t.rs")
            ],
            "must take the 2-edge route"
        );
    }

    /// Non-termination bug class (tethys-vwrn): a cycle reachable from the
    /// source with the target elsewhere must return None, quickly.
    #[test]
    fn cycle_reachable_unreachable_target_returns_none() {
        let (_dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");
        let island = upsert(&mut index, "src/island.rs");
        edge(&mut index, a, b);
        edge(&mut index, b, a);

        assert_eq!(
            chain_paths(&index, a, island),
            None,
            "cycle must not hang; unreachable target is None"
        );
    }

    /// The target must still be found THROUGH the cycle region.
    #[test]
    fn target_reachable_through_cycle_is_found() {
        let (_dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");
        let t = upsert(&mut index, "src/t.rs");
        edge(&mut index, a, b);
        edge(&mut index, b, a);
        edge(&mut index, b, t);

        let got = chain_paths(&index, a, t).expect("route through cycle");
        assert_eq!(
            got,
            vec![
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/b.rs"),
                PathBuf::from("src/t.rs")
            ]
        );
    }

    /// Equal indexed endpoints are a one-file path (design C4).
    #[test]
    fn equal_endpoints_yield_single_file_path() {
        let (_dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");

        let got = chain_paths(&index, a, a).expect("self-path");
        assert_eq!(got, vec![PathBuf::from("src/a.rs")]);
    }

    /// Empty-collection bug class: a zero-edge graph is just disconnected.
    #[test]
    fn zero_edge_graph_returns_none() {
        let (_dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");

        assert_eq!(chain_paths(&index, a, b), None);
    }

    /// Equal endpoints with an unknown id keep today's `NotFound` contract.
    #[test]
    fn equal_missing_id_is_notfound() {
        let (_dir, index) = temp_index();
        let ghost = FileId::from(4242);

        let err = index
            .find_dependency_path(ghost, ghost)
            .expect_err("missing id must error");
        assert!(
            matches!(err, Error::NotFound(ref m) if m.contains("4242")),
            "expected NotFound naming the id, got: {err:?}"
        );
    }
}
