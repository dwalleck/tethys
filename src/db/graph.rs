//! SQLite-backed graph queries implemented as concrete `Index` operations.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

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
    ///
    /// Files and dependency edges are read in one `SQLite` snapshot, which is
    /// released before enumeration begins: cycle discovery is pure CPU work
    /// over the loaded snapshot and must not hold the database lock while it
    /// runs. Cycle members are projected from that snapshot's path map, so
    /// conversion performs no per-member database lookup.
    pub fn detect_cycles(&self) -> Result<Vec<Cycle>> {
        let CycleSnapshot { paths_by_id, adj } = self.load_cycle_snapshot()?;
        let node_count = paths_by_id.len();
        let edge_count: usize = adj.values().map(Vec::len).sum();

        let cycle_ids = enumerate_cycles(adj, &paths_by_id);
        let mut cycles = Vec::with_capacity(cycle_ids.len());
        for ids in cycle_ids {
            let mut files = Vec::with_capacity(ids.len());
            for (position, id) in ids.iter().copied().enumerate() {
                let path = paths_by_id.get(&id).cloned().ok_or_else(|| {
                    tracing::error!(
                        file_id = id.as_i64(),
                        cycle_position = position,
                        cycle_length = ids.len(),
                        "File not found in database but referenced in dependency cycle"
                    );
                    Error::NotFound(format!(
                        "file id: {} (position {} in cycle of length {})",
                        id.as_i64(),
                        position,
                        ids.len()
                    ))
                })?;
                files.push(path);
            }
            cycles.push(Cycle { files });
        }

        tracing::info!(
            node_count,
            edge_count,
            cycle_count = cycles.len(),
            "Cycle detection complete"
        );
        Ok(cycles)
    }

    /// Read the file path map and the dependency adjacency in one snapshot.
    ///
    /// Both reads share one deferred transaction, so a concurrent indexing
    /// write cannot pair a `files` row set with a `file_deps` row set from a
    /// different index state. Every edge endpoint is validated against the
    /// path map here, so enumeration and projection never observe a dangling
    /// id. The connection is dropped on return, before any cycle work runs.
    ///
    /// Neither read orders its rows: [`enumerate_cycles`] is the single owner
    /// of result ordering and re-sorts by path regardless.
    fn load_cycle_snapshot(&self) -> Result<CycleSnapshot> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;

        let mut paths_by_id = HashMap::new();
        {
            let mut stmt = tx.prepare("SELECT id, path FROM files")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    FileId::from(row.get::<_, i64>(0)?),
                    PathBuf::from(row.get::<_, String>(1)?),
                ))
            })?;
            for row in rows {
                let (id, path) = row?;
                paths_by_id.insert(id, path);
            }
        }

        let mut adj: HashMap<FileId, Vec<FileId>> = HashMap::new();
        {
            let mut stmt = tx.prepare("SELECT from_file_id, to_file_id FROM file_deps")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    FileId::from(row.get::<_, i64>(0)?),
                    FileId::from(row.get::<_, i64>(1)?),
                ))
            })?;
            for row in rows {
                let (from, to) = row?;
                for (endpoint, role) in [(from, "source"), (to, "target")] {
                    if !paths_by_id.contains_key(&endpoint) {
                        tracing::error!(
                            missing_file_id = endpoint.as_i64(),
                            from_file_id = from.as_i64(),
                            to_file_id = to.as_i64(),
                            endpoint_role = role,
                            "Dependency edge references a file absent from the files table"
                        );
                        return Err(Error::NotFound(format!(
                            "file id: {} ({} endpoint of dependency {} -> {})",
                            endpoint.as_i64(),
                            role,
                            from.as_i64(),
                            to.as_i64()
                        )));
                    }
                }
                adj.entry(from).or_default().push(to);
            }
        }

        // Read-only: end the snapshot without a write.
        drop(tx);
        Ok(CycleSnapshot { paths_by_id, adj })
    }
}

/// One consistent read of the file-dependency graph.
///
/// Both maps come from the same `SQLite` read snapshot, and every endpoint in
/// `adj` is guaranteed to have an entry in `paths_by_id`.
struct CycleSnapshot {
    /// Indexed workspace-relative path of every file in the snapshot.
    paths_by_id: HashMap<FileId, PathBuf>,
    /// Outgoing dependency edges: file id to the files it depends on.
    adj: HashMap<FileId, Vec<FileId>>,
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
}

/// Enumerate every simple directed cycle once, canonicalized by path.
///
/// A cycle is explored only from its lexicographically smallest indexed path,
/// which removes rotation duplicates without reversing directed edges and
/// leaves every recorded sequence already rotated to its canonical first
/// member. Results are then ordered by that canonical path sequence.
///
/// Takes `adj` by value: neighbour lists are sorted in place into the path
/// order the search depends on, so no second copy of the edge set is made.
fn enumerate_cycles(
    adj: HashMap<FileId, Vec<FileId>>,
    paths_by_id: &HashMap<FileId, PathBuf>,
) -> Vec<Vec<FileId>> {
    let outcome = run_cycle_search(adj, paths_by_id);
    tracing::debug!(
        visits = outcome.visits,
        cycle_count = outcome.cycles.len(),
        "Cycle enumeration complete"
    );
    outcome.cycles
}

/// Result of one cycle search, carrying the work it cost.
struct CycleSearchOutcome {
    /// Canonical cycles in deterministic result order.
    cycles: Vec<Vec<FileId>>,
    /// Number of node visits the search made.
    ///
    /// Logged as enumeration diagnostics, and fenced by tests to prove cost
    /// tracks cycles found rather than simple paths walked — the distinction
    /// Johnson's blocking buys.
    visits: usize,
}

/// [`enumerate_cycles`], additionally reporting how much work it took.
fn run_cycle_search(
    mut adj: HashMap<FileId, Vec<FileId>>,
    paths_by_id: &HashMap<FileId, PathBuf>,
) -> CycleSearchOutcome {
    let mut nodes: Vec<FileId> = adj
        .iter()
        .flat_map(|(from, tos)| std::iter::once(*from).chain(tos.iter().copied()))
        .collect();
    nodes.sort_by(|left, right| compare_file_ids(*left, *right, paths_by_id));
    nodes.dedup();

    for neighbors in adj.values_mut() {
        neighbors.sort_by(|left, right| compare_file_ids(*left, *right, paths_by_id));
        neighbors.dedup();
    }

    let mut search = CycleSearch {
        adj: &adj,
        paths_by_id,
        path: Vec::new(),
        blocked: HashSet::new(),
        blocked_by: HashMap::new(),
        cycles: Vec::new(),
        visits: 0,
    };
    for &start in &nodes {
        // Johnson's bookkeeping is scoped to one start node's subgraph.
        search.blocked.clear();
        search.blocked_by.clear();
        search.visit(start, start);
    }

    let mut cycles = search.cycles;
    cycles.sort_by(|left, right| compare_cycle_ids(left, right, paths_by_id));
    CycleSearchOutcome {
        cycles,
        visits: search.visits,
    }
}

/// Mutable state of one [`enumerate_cycles`] search.
///
/// Bundled so the recursion keeps a single allocation of each collection
/// instead of threading seven parameters through every frame.
struct CycleSearch<'a> {
    /// Outgoing edges, neighbour lists already in canonical path order.
    adj: &'a HashMap<FileId, Vec<FileId>>,
    /// Path map backing [`compare_file_ids`].
    paths_by_id: &'a HashMap<FileId, PathBuf>,
    /// Nodes of the walk currently being explored, in order.
    path: Vec<FileId>,
    /// Johnson's `blocked`: nodes known to reach no cycle through `start`
    /// since they were blocked. Unlike an on-path set, membership survives
    /// backtracking — that is what stops fruitless subtrees being re-walked.
    blocked: HashSet<FileId>,
    /// Johnson's `B`: for each node, the blocked nodes to release when it is
    /// unblocked.
    blocked_by: HashMap<FileId, HashSet<FileId>>,
    /// Canonically rotated cycles found so far, unordered.
    cycles: Vec<Vec<FileId>>,
    /// Node visits made, reported as [`CycleSearchOutcome::visits`].
    visits: usize,
}

impl<'a> CycleSearch<'a> {
    /// Walk from `node`, recording every simple cycle that closes on `start`.
    ///
    /// Returns whether any cycle through `start` was reached from `node`.
    ///
    /// Two prunes make each cycle appear exactly once. The
    /// `compare_file_ids(neighbour, start, ..) == Less` skip confines the
    /// search to nodes at or after `start` in path order, so a cycle is
    /// discovered only while walking from its own smallest member — that is
    /// what makes the recorded [`Self::path`] canonically rotated. The
    /// `blocked` set keeps each walk simple.
    ///
    /// The `found` return is Johnson's contribution and the reason cost is
    /// bounded by the number of cycles rather than the number of simple
    /// paths: a node that reached no cycle stays blocked and registers itself
    /// on each successor's [`Self::blocked_by`] list, so the subtree below it
    /// is not walked again until a successor proves a cycle exists. Without
    /// it, a graph with no cycles at all still costs one visit per simple
    /// path (tethys-u5o5 review: 72 files, 0 cycles, 228s).
    fn visit(&mut self, node: FileId, start: FileId) -> bool {
        let neighbors: &'a [FileId] = self.adj.get(&node).map_or(&[], Vec::as_slice);
        let mut found = false;

        self.visits += 1;
        self.path.push(node);
        self.blocked.insert(node);

        for &neighbor in neighbors {
            if compare_file_ids(neighbor, start, self.paths_by_id) == Ordering::Less {
                continue;
            }
            if neighbor == start {
                self.cycles.push(self.path.clone());
                found = true;
            } else if !self.blocked.contains(&neighbor) && self.visit(neighbor, start) {
                found = true;
            }
        }

        if found {
            self.unblock(node);
        } else {
            for &neighbor in neighbors {
                if compare_file_ids(neighbor, start, self.paths_by_id) != Ordering::Less {
                    self.blocked_by.entry(neighbor).or_default().insert(node);
                }
            }
        }

        self.path.pop();
        found
    }

    /// Johnson's `UNBLOCK`: release `node` and everything waiting on it.
    ///
    /// Called only when a cycle was found through `node`, so the subtrees that
    /// were blocked behind it are worth exploring again.
    fn unblock(&mut self, node: FileId) {
        self.blocked.remove(&node);
        if let Some(dependents) = self.blocked_by.remove(&node) {
            for dependent in dependents {
                if self.blocked.contains(&dependent) {
                    self.unblock(dependent);
                }
            }
        }
    }
}

/// Total order over file ids by their indexed workspace-relative path.
///
/// Path order rather than id order is what makes canonicalization stable
/// across index rebuilds, which assign ids by insertion. Ties break on id so
/// the order is total; the arms for ids absent from `paths_by_id` keep that
/// totality for callers that have not validated their endpoints (`Index::
/// detect_cycles` validates before enumerating, so it never hits them).
fn compare_file_ids(
    left: FileId,
    right: FileId,
    paths_by_id: &HashMap<FileId, PathBuf>,
) -> Ordering {
    match (paths_by_id.get(&left), paths_by_id.get(&right)) {
        (Some(left_path), Some(right_path)) => left_path
            .cmp(right_path)
            .then_with(|| left.as_i64().cmp(&right.as_i64())),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.as_i64().cmp(&right.as_i64()),
    }
}

/// Order two canonical cycles by their path sequence, shorter first on a tie.
///
/// This is the deterministic result order callers observe: lexicographic over
/// [`compare_file_ids`], falling back to cycle length when one sequence is a
/// prefix of the other.
fn compare_cycle_ids(
    left: &[FileId],
    right: &[FileId],
    paths_by_id: &HashMap<FileId, PathBuf>,
) -> Ordering {
    for (left_id, right_id) in left.iter().zip(right) {
        let ordering = compare_file_ids(*left_id, *right_id, paths_by_id);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
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

    fn cycle_paths(
        cycles: Vec<Vec<FileId>>,
        paths_by_id: &HashMap<FileId, PathBuf>,
    ) -> Vec<Vec<PathBuf>> {
        cycles
            .into_iter()
            .map(|cycle| {
                cycle
                    .into_iter()
                    .map(|id| paths_by_id[&id].clone())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn enumerate_cycles_covers_overlap_direction_and_self_loop() {
        let paths_by_id = HashMap::from([
            (FileId::from(30), PathBuf::from("src/a.rs")),
            (FileId::from(10), PathBuf::from("src/b.rs")),
            (FileId::from(20), PathBuf::from("src/c.rs")),
            (FileId::from(40), PathBuf::from("src/self.rs")),
        ]);
        let adj = HashMap::from([
            (FileId::from(30), vec![FileId::from(10), FileId::from(20)]),
            (FileId::from(10), vec![FileId::from(30), FileId::from(20)]),
            (FileId::from(20), vec![FileId::from(30), FileId::from(10)]),
            (FileId::from(40), vec![FileId::from(40)]),
        ]);

        let got = cycle_paths(enumerate_cycles(adj, &paths_by_id), &paths_by_id);
        assert_eq!(
            got,
            vec![
                vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
                vec![
                    PathBuf::from("src/a.rs"),
                    PathBuf::from("src/b.rs"),
                    PathBuf::from("src/c.rs"),
                ],
                vec![PathBuf::from("src/a.rs"), PathBuf::from("src/c.rs")],
                vec![
                    PathBuf::from("src/a.rs"),
                    PathBuf::from("src/c.rs"),
                    PathBuf::from("src/b.rs"),
                ],
                vec![PathBuf::from("src/b.rs"), PathBuf::from("src/c.rs")],
                vec![PathBuf::from("src/self.rs")],
            ]
        );
    }

    /// Enumeration cost must track cycles found, not simple paths walked.
    ///
    /// A layered DAG (every edge points from layer `n` to layer `n+1`) has
    /// zero cycles but `width ^ (layers - 1)` simple paths — 10,077,696 here.
    /// Johnson's blocking visits each node at most once per start node, so
    /// the work is quadratic in nodes; without it the search walks every
    /// path, and this same shape took 6.4s at 10 layers and 228s at 12
    /// (tethys-u5o5 review).
    #[test]
    fn enumerate_cycles_stays_output_sensitive_on_acyclic_dag() {
        const WIDTH: i64 = 6;
        const LAYERS: i64 = 10;
        let node_count = WIDTH * LAYERS;

        let mut paths_by_id = HashMap::new();
        for id in 0..node_count {
            paths_by_id.insert(FileId::from(id), PathBuf::from(format!("src/n{id:04}.rs")));
        }
        let mut adj: HashMap<FileId, Vec<FileId>> = HashMap::new();
        for layer in 0..LAYERS - 1 {
            for from in layer * WIDTH..(layer + 1) * WIDTH {
                for to in (layer + 1) * WIDTH..(layer + 2) * WIDTH {
                    adj.entry(FileId::from(from))
                        .or_default()
                        .push(FileId::from(to));
                }
            }
        }

        let outcome = run_cycle_search(adj, &paths_by_id);

        assert!(
            outcome.cycles.is_empty(),
            "a layered DAG contains no directed cycle"
        );
        let budget = usize::try_from(node_count * node_count).expect("budget fits");
        assert!(
            outcome.visits <= budget,
            "acyclic enumeration must stay within {budget} visits (one pass per \
             start node), walked {} — cost is tracking simple paths, not cycles",
            outcome.visits
        );
    }

    #[test]
    fn enumerate_cycles_handles_empty_graph() {
        let paths_by_id = HashMap::new();
        let adj = HashMap::new();

        assert!(enumerate_cycles(adj, &paths_by_id).is_empty());
    }

    #[test]
    fn enumerate_cycles_uses_path_order_not_file_id_order() {
        let paths_by_id = HashMap::from([
            (FileId::from(1), PathBuf::from("src/z.rs")),
            (FileId::from(2), PathBuf::from("src/a.rs")),
        ]);
        let adj = HashMap::from([
            (FileId::from(1), vec![FileId::from(2)]),
            (FileId::from(2), vec![FileId::from(1)]),
        ]);

        let got = cycle_paths(enumerate_cycles(adj, &paths_by_id), &paths_by_id);
        assert_eq!(
            got,
            vec![vec![PathBuf::from("src/a.rs"), PathBuf::from("src/z.rs"),]]
        );
    }

    #[test]
    fn enumerate_cycles_scales_on_sparse_graph() {
        let mut paths_by_id = HashMap::new();
        let mut adj: HashMap<FileId, Vec<FileId>> = HashMap::new();
        for id in 0_i64..1_000 {
            paths_by_id.insert(FileId::from(id), PathBuf::from(format!("src/f{id}.rs")));
        }
        let mut add_edge = |from: i64, to: i64| {
            adj.entry(FileId::from(from))
                .or_default()
                .push(FileId::from(to));
        };
        for group in 0_i64..90 {
            let base = group * 11;
            for from in base..base + 11 {
                for to in from + 1..base + 11 {
                    add_edge(from, to);
                }
            }
        }
        for from in 990_i64..1_000 {
            for to in from + 1..1_000 {
                add_edge(from, to);
            }
        }
        for from in 0_i64..4 {
            add_edge(from, 990);
        }
        add_edge(999, 999);

        let cycles = enumerate_cycles(adj, &paths_by_id);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0], vec![FileId::from(999)]);
    }
}

#[cfg(test)]
mod cycle_hydration_fences {
    //! Fences for `detect_cycles`'s read contract: two set-valued `SELECT`s
    //! per call with no per-member scalar lookup, one read snapshot shared by
    //! both, and a typed `NotFound` for a dangling dependency endpoint.
    //!
    //! The trace counters are process-global statics, so every assertion on
    //! them must run under `cargo nextest` (process-per-test), which is the
    //! runner CI and `scripts/gate.sh` use.

    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use rusqlite::Connection;
    use tempfile::TempDir;

    use crate::db::Index;
    use crate::error::Error;
    use crate::types::{FileId, Language};

    static TRACE_SELECTS: AtomicUsize = AtomicUsize::new(0);
    static TRACE_PER_ID: AtomicUsize = AtomicUsize::new(0);
    static SNAPSHOT_WRITE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static SNAPSHOT_WRITE: Mutex<Option<SnapshotWrite>> = Mutex::new(None);

    #[derive(Debug)]
    struct SnapshotWrite {
        path: PathBuf,
        from: FileId,
        to: FileId,
    }

    fn cycle_trace_cb(sql: &str) {
        if sql.trim_start().starts_with("SELECT") {
            TRACE_SELECTS.fetch_add(1, Ordering::Relaxed);
        }
        if sql.contains("FROM files WHERE id = ") {
            TRACE_PER_ID.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn snapshot_trace_cb(sql: &str) {
        if !sql.contains("FROM file_deps") {
            return;
        }
        let pending = SNAPSHOT_WRITE.lock().expect("snapshot mutex").take();
        let Some(pending) = pending else {
            return;
        };
        let connection = Connection::open(pending.path).expect("open snapshot writer");
        connection
            .busy_timeout(Duration::from_secs(5))
            .expect("set writer timeout");
        connection
            .execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                [pending.from.as_i64(), pending.to.as_i64()],
            )
            .expect("insert concurrent edge");
        SNAPSHOT_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    fn reset_trace() {
        TRACE_SELECTS.store(0, Ordering::Relaxed);
        TRACE_PER_ID.store(0, Ordering::Relaxed);
    }

    fn trace_counts() -> (usize, usize) {
        (
            TRACE_SELECTS.load(Ordering::Relaxed),
            TRACE_PER_ID.load(Ordering::Relaxed),
        )
    }

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("tempdir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open index");
        (dir, index)
    }

    fn upsert(index: &mut Index, path: &str) -> FileId {
        index
            .upsert_file(Path::new(path), Language::Rust, 0, 0, None)
            .expect("upsert file")
    }

    fn edge(index: &mut Index, from: FileId, to: FileId) {
        index.insert_file_dependency(from, to).expect("insert edge");
    }

    fn attach_trace(index: &Index) {
        let mut connection = index.connection().expect("connection");
        connection.trace(Some(cycle_trace_cb));
    }

    #[test]
    fn cycle_query_statement_counts_are_flat() {
        let (_dir, mut index) = temp_index();
        attach_trace(&index);

        reset_trace();
        assert!(index.detect_cycles().expect("empty cycle query").is_empty());
        assert_eq!(trace_counts(), (2, 0), "empty query must use two SELECTs");

        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");
        let c = upsert(&mut index, "src/c.rs");
        reset_trace();
        assert!(
            index
                .detect_cycles()
                .expect("acyclic cycle query")
                .is_empty()
        );
        assert_eq!(trace_counts(), (2, 0), "acyclic query must use two SELECTs");

        edge(&mut index, a, b);
        edge(&mut index, b, c);
        edge(&mut index, c, a);
        reset_trace();
        assert_eq!(
            index.detect_cycles().expect("cyclic query").len(),
            1,
            "cycle fixture should return one cycle"
        );
        assert_eq!(
            trace_counts(),
            (2, 0),
            "cycle hydration must not issue scalar lookups"
        );

        reset_trace();
        index.get_file_by_id(a).expect("scalar canary");
        assert_eq!(
            trace_counts(),
            (1, 1),
            "scalar lookup canary must fire before trusting zero"
        );
    }

    #[test]
    fn cycle_query_scales_sparse_graph() {
        let (_dir, mut index) = temp_index();
        let mut ids = Vec::with_capacity(1_000);
        for id in 0_i64..1_000 {
            let path = if id == 999 {
                "src/Ω/self file.rs".to_owned()
            } else {
                format!("src/f{id}.rs")
            };
            ids.push(upsert(&mut index, &path));
        }

        {
            let mut connection = index.connection().expect("connection");
            let tx = connection.transaction().expect("edge transaction");
            for group in 0_usize..90 {
                let base = group * 11;
                for from in base..base + 11 {
                    for to in from + 1..base + 11 {
                        tx.execute(
                            "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                            rusqlite::params![ids[from].as_i64(), ids[to].as_i64()],
                        )
                        .expect("insert sparse edge");
                    }
                }
            }
            for from in 990_usize..1_000 {
                for to in from + 1..1_000 {
                    tx.execute(
                        "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                        rusqlite::params![ids[from].as_i64(), ids[to].as_i64()],
                    )
                    .expect("insert final sparse edge");
                }
            }
            for from in 0_usize..4 {
                tx.execute(
                    "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                    rusqlite::params![ids[from].as_i64(), ids[990].as_i64()],
                )
                .expect("insert cross-group edge");
            }
            tx.execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                rusqlite::params![ids[999].as_i64(), ids[999].as_i64()],
            )
            .expect("insert sparse self-loop");
            tx.commit().expect("commit sparse edges");
        }

        attach_trace(&index);
        reset_trace();
        let cycles = index.detect_cycles().expect("sparse cycle query");
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].files, vec![PathBuf::from("src/Ω/self file.rs")]);
        assert_eq!(
            trace_counts(),
            (2, 0),
            "large cycle conversion must remain two set reads"
        );
    }

    #[test]
    fn cycle_query_dangling_endpoint_returns_notfound() {
        let (dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");
        let ghost = FileId::from(99_999);
        let db_path = dir.path().join("idx.db");
        let connection = Connection::open(db_path).expect("open dangling writer");
        connection
            .pragma_update(None, "foreign_keys", false)
            .expect("disable foreign keys for corrupt fixture");
        connection
            .execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                rusqlite::params![a.as_i64(), ghost.as_i64()],
            )
            .expect("insert dangling edge");
        connection
            .execute(
                "INSERT INTO file_deps (from_file_id, to_file_id) VALUES (?1, ?2)",
                rusqlite::params![ghost.as_i64(), a.as_i64()],
            )
            .expect("insert reverse dangling edge");
        drop(connection);

        let error = index
            .detect_cycles()
            .expect_err("dangling cycle endpoint must fail");
        assert!(
            matches!(&error, Error::NotFound(message) if message.contains("99999")),
            "expected typed dangling endpoint error, got {error:?}"
        );
    }

    #[test]
    fn cycle_query_uses_one_snapshot() {
        let (dir, mut index) = temp_index();
        let a = upsert(&mut index, "src/a.rs");
        let b = upsert(&mut index, "src/b.rs");
        edge(&mut index, a, b);

        {
            let mut connection = index.connection().expect("connection");
            connection.trace(Some(snapshot_trace_cb));
        }
        SNAPSHOT_WRITE_COUNT.store(0, Ordering::Relaxed);
        *SNAPSHOT_WRITE.lock().expect("snapshot mutex") = Some(SnapshotWrite {
            path: dir.path().join("idx.db"),
            from: b,
            to: a,
        });

        let cycles = index.detect_cycles().expect("snapshot cycle query");
        assert!(
            cycles.is_empty(),
            "the concurrent reverse edge belongs to the next snapshot"
        );
        assert_eq!(
            SNAPSHOT_WRITE_COUNT.load(Ordering::Relaxed),
            1,
            "snapshot writer canary must fire"
        );
        assert!(
            SNAPSHOT_WRITE.lock().expect("snapshot mutex").is_none(),
            "snapshot writer request must be consumed"
        );
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

#[cfg(test)]
mod chain_4m9o_fences {
    //! Statement-count and boundary fences for the dependency-chain query
    //! (tethys-4m9o C7–C10). Pairs with `dependency_path_tests` above the
    //! way `file_deps::hydration_fence_tests` pairs with its integration
    //! file: these fences need the live connection's `rusqlite` trace hook,
    //! which only crate code can reach.
    //!
    //! Pre-rewrite, hydration issued one `get_file_by_id` lookup per path
    //! member (3 + L statements for an L-file chain); the BFS + batch form
    //! is exactly 2 statements connected, 1 disconnected, 1 equal —
    //! independent of path length. The per-id counter keys on the
    //! `get_file_by_id` SQL shape so a regression to per-id lookups fails
    //! loudly.

    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::TempDir;

    use crate::db::Index;
    use crate::error::Error;
    use crate::types::{FileId, Language};

    static TRACE_TOTAL: AtomicUsize = AtomicUsize::new(0);
    static TRACE_PER_ID: AtomicUsize = AtomicUsize::new(0);

    fn trace_cb(sql: &str) {
        TRACE_TOTAL.fetch_add(1, Ordering::Relaxed);
        if sql.contains("FROM files WHERE id = ") {
            TRACE_PER_ID.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reset_counts() {
        TRACE_TOTAL.store(0, Ordering::Relaxed);
        TRACE_PER_ID.store(0, Ordering::Relaxed);
    }

    fn counts() -> (usize, usize) {
        (
            TRACE_TOTAL.load(Ordering::Relaxed),
            TRACE_PER_ID.load(Ordering::Relaxed),
        )
    }

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

    /// C7: statement counts do not grow with path length, and no per-id
    /// `files` lookup ever fires. All counting lives in this single test
    /// because the trace counters are process-global statics.
    #[test]
    fn chain_query_statement_counts_are_flat() {
        let (_dir, mut index) = temp_index();
        let fa = upsert(&mut index, "src/a.rs");
        let fb = upsert(&mut index, "src/b.rs");
        let fc = upsert(&mut index, "src/c.rs");
        let ft = upsert(&mut index, "src/t.rs");
        let island = upsert(&mut index, "src/island.rs");
        edge(&mut index, fa, fb);
        edge(&mut index, fb, fc);
        edge(&mut index, fc, ft);

        {
            let mut conn = index.connection().expect("connection");
            conn.trace(Some(trace_cb));
        }

        // canary: prove the per-id predicate is alive before relying on its
        // zeros below — if get_file_by_id's SQL shape drifts away from the
        // counted string, this fails instead of the fence going vacuous
        reset_counts();
        index.get_file_by_id(fa).expect("canary lookup");
        assert_eq!(
            counts(),
            (1, 1),
            "per-id trace predicate must fire on get_file_by_id"
        );

        // len-4 chain: adjacency load + one batched hydration, nothing per-id
        reset_counts();
        let path = index
            .find_dependency_path(fa, ft)
            .expect("connected")
            .expect("path exists");
        assert_eq!(path.into_files().len(), 4);
        assert_eq!(
            counts(),
            (2, 0),
            "connected multi-hop must be adjacency + batch hydrate only"
        );

        // len-2 direct edge: same shape — counts must NOT grow with length
        reset_counts();
        let path = index
            .find_dependency_path(fa, fb)
            .expect("direct")
            .expect("path exists");
        assert_eq!(path.into_files().len(), 2);
        assert_eq!(counts(), (2, 0), "direct edge must match multi-hop counts");

        // disconnected: adjacency load only, no hydration
        reset_counts();
        assert!(
            index
                .find_dependency_path(fa, island)
                .expect("disconnected")
                .is_none()
        );
        assert_eq!(counts(), (1, 0), "disconnected must not hydrate");

        // equal endpoints: batch hydration only, no adjacency load
        reset_counts();
        let path = index
            .find_dependency_path(fa, fa)
            .expect("equal")
            .expect("self-path");
        assert_eq!(path.into_files().len(), 1);
        assert_eq!(counts(), (1, 0), "equal endpoints must skip traversal");
    }

    /// C8: the 50-edge depth cap is preserved exactly — off-by-one in
    /// either direction fails this fence.
    #[test]
    fn chain_respects_depth_cap_boundary() {
        let (_dir, mut index) = temp_index();
        let ids: Vec<FileId> = (0..=51)
            .map(|i| upsert(&mut index, &format!("src/f{i}.rs")))
            .collect();
        for w in ids.windows(2) {
            edge(&mut index, w[0], w[1]);
        }

        let at_cap = index
            .find_dependency_path(ids[0], ids[50])
            .expect("cap query")
            .expect("50-edge chain is within the cap");
        assert_eq!(at_cap.into_files().len(), 51);

        assert!(
            index
                .find_dependency_path(ids[0], ids[51])
                .expect("beyond-cap query")
                .is_none(),
            "51-edge chain must be beyond the cap"
        );
    }

    /// C9: a self-loop `file_deps` row neither hangs the BFS nor perturbs
    /// shortest-path results.
    #[test]
    fn self_loop_edge_is_inert() {
        let (_dir, mut index) = temp_index();
        let fa = upsert(&mut index, "src/a.rs");
        let fb = upsert(&mut index, "src/b.rs");
        edge(&mut index, fa, fa);
        edge(&mut index, fa, fb);
        edge(&mut index, fb, fa);

        let path = index
            .find_dependency_path(fa, fb)
            .expect("query")
            .expect("route exists");
        assert_eq!(path.into_files().len(), 2, "self-loop must not perturb");

        let this = index
            .find_dependency_path(fa, fa)
            .expect("equal")
            .expect("self-path");
        assert_eq!(this.into_files().len(), 1, "equal endpoints stay trivial");
    }

    /// C10: a dangling path member (files row gone, edge row surviving)
    /// is a `NotFound` error, never a silently shortened chain. Dangling
    /// rows are impossible under the FK pragma; fabricated here with FKs
    /// off, mirroring the n8pu dangling fence.
    #[test]
    fn dangling_path_member_is_notfound() {
        let (_dir, mut index) = temp_index();
        let fa = upsert(&mut index, "src/a.rs");
        let fb = upsert(&mut index, "src/b.rs");
        let ft = upsert(&mut index, "src/t.rs");
        edge(&mut index, fa, fb);
        edge(&mut index, fb, ft);

        {
            let conn = index.connection().expect("connection");
            conn.execute_batch(&format!(
                "PRAGMA foreign_keys=OFF;
                 DELETE FROM files WHERE id = {};
                 PRAGMA foreign_keys=ON;",
                fb.as_i64()
            ))
            .expect("fabricate dangling row");
        }

        let err = index
            .find_dependency_path(fa, ft)
            .expect_err("hole in the chain must error");
        assert!(
            matches!(err, Error::NotFound(ref m) if m.contains(&fb.as_i64().to_string())),
            "expected NotFound naming the dangling id, got: {err:?}"
        );
    }
}
