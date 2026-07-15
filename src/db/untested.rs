//! Untested-code analysis (tethys-y3bx): product functions and methods that
//! no test can reach.
//!
//! Multi-root forward closure from `is_test` symbols over the reference
//! graph (`refs.in_symbol_id → refs.symbol_id`), complemented against
//! product `function`/`method` symbols. The traversal reads `refs`, NOT
//! `call_edges`, and the choice is load-bearing: `macro_call` references
//! (tethys-8ym0) are excluded from `call_edges` by design, so an
//! assert-only-tested function is reachable through `refs` and invisible
//! through `call_edges` (30-symbol gap measured on the self-index).
//!
//! # Semantics
//!
//! "Untested" means *no test reaches this symbol through the reference
//! graph* — reachability, not verification. A reached function may still be
//! asserted on weakly or not at all.
//!
//! # Known false-positive sources (documented, deliberately not filtered)
//!
//! - Method-shape calls inside macro arguments (`assert!(x.is_valid())`)
//!   emit no reference yet (tethys-9l27), so methods tested only that way
//!   read untested.
//! - Functions defined inside macro invocations (`proptest! { fn … }`) are
//!   not indexed as symbols (tethys-0nar), so their callees look unreached.
//! - Dynamic dispatch through `dyn Trait` may produce no edge to the
//!   concrete method (type-hierarchy suppression is tethys-j2r1, dead-code
//!   stage infrastructure).
//! - Top-level references (`in_symbol_id` NULL) cannot participate in any
//!   reachability traversal — an edge needs a source symbol.
//!
//! With zero test roots the result is *indeterminate*, not "everything is
//! untested": [`UntestedReport::is_indeterminate`] flags it and the report
//! carries no findings (suppressions, not accusations).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;
use tracing::trace;

use super::Index;
use crate::error::Result;

/// A product function or method no test reaches.
#[derive(Debug, Clone, Serialize)]
pub struct UntestedFinding {
    /// Symbol name as declared.
    pub name: String,
    /// Raw `symbols.kind` text (display-only; no dispatch happens on it).
    pub kind: String,
    /// Workspace-relative path of the declaring file.
    pub file: String,
    /// 1-based declaration line.
    pub line: u32,
    /// Module path (`crate::db::graph`), for qualified display.
    pub module_path: String,
}

/// Output of [`Index::get_untested_code`]: findings plus the counts the
/// caller needs to interpret them.
#[derive(Debug, Clone, Serialize)]
pub struct UntestedReport {
    /// Number of `is_test` root symbols the closure started from.
    pub test_roots: usize,
    /// Number of product function/method symbols evaluated.
    pub product_fns: usize,
    /// Product symbols outside the closure, sorted by (file, line, name).
    pub findings: Vec<UntestedFinding>,
}

impl UntestedReport {
    /// With zero test roots every product symbol is trivially unreachable;
    /// the analysis is indeterminate rather than a full-workspace
    /// accusation (approved posture D-B; vocabulary shared with
    /// tethys-09wx).
    #[must_use]
    pub fn is_indeterminate(&self) -> bool {
        self.test_roots == 0
    }
}

/// Multi-root forward closure over an adjacency map. Roots are included in
/// the returned set. Pure so the cycle/multi-root/self-loop shapes are
/// unit-testable without a database.
fn reachable_closure(roots: &[i64], edges: &HashMap<i64, Vec<i64>>) -> HashSet<i64> {
    let mut seen: HashSet<i64> = roots.iter().copied().collect();
    let mut queue: VecDeque<i64> = roots.iter().copied().collect();
    while let Some(current) = queue.pop_front() {
        for &next in edges.get(&current).map(Vec::as_slice).unwrap_or_default() {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

impl Index {
    /// Compute the untested-code report: product functions/methods outside
    /// the forward closure of `is_test` roots over the reference graph.
    ///
    /// Single pass: one query for roots, one for edges, one for product
    /// symbols; BFS is O(symbols + refs) in memory.
    pub fn get_untested_code(&self) -> Result<UntestedReport> {
        trace!("Computing untested-code report");
        let conn = self.connection()?;

        let mut roots_stmt = conn.prepare("SELECT id FROM symbols WHERE is_test = 1")?;
        let roots: Vec<i64> = roots_stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;

        let mut edges_stmt = conn.prepare(
            "SELECT in_symbol_id, symbol_id FROM refs
             WHERE in_symbol_id IS NOT NULL AND symbol_id IS NOT NULL",
        )?;
        let mut edges: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in edges_stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (from, to): (i64, i64) = row?;
            edges.entry(from).or_default().push(to);
        }

        let reached = reachable_closure(&roots, &edges);

        let mut prod_stmt = conn.prepare(
            "SELECT s.id, s.name, s.kind, f.path, s.line, s.module_path
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             WHERE s.is_test = 0 AND s.kind IN ('function', 'method')
             ORDER BY f.path, s.line, s.name",
        )?;
        let mut product_fns = 0usize;
        let mut findings = Vec::new();
        for row in prod_stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                UntestedFinding {
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    file: row.get(3)?,
                    line: row.get(4)?,
                    module_path: row.get(5)?,
                },
            ))
        })? {
            let (id, finding) = row?;
            product_fns += 1;
            if !reached.contains(&id) {
                findings.push(finding);
            }
        }

        // Indeterminate posture: zero roots means the closure is vacuous and
        // every product symbol would be "untested" — report none instead.
        if roots.is_empty() {
            findings.clear();
        }

        trace!(
            test_roots = roots.len(),
            product_fns,
            untested = findings.len(),
            "Untested-code report complete"
        );
        Ok(UntestedReport {
            test_roots: roots.len(),
            product_fns,
            findings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_map(edges: &[(i64, i64)]) -> HashMap<i64, Vec<i64>> {
        let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(from, to) in edges {
            map.entry(from).or_default().push(to);
        }
        map
    }

    /// The plan's slice-1 stress fixture, expected closure written first:
    /// multi-root {1,2}, chain 1→10→11, cycle 11↔12, self-loop 13 from
    /// root 2, disconnected 99 — closure = {1,2,10,11,12,13}; 99 outside.
    #[test]
    fn closure_covers_chains_cycles_and_self_loops() {
        let edges = edge_map(&[
            (1, 10),
            (10, 11),
            (11, 12),
            (12, 11), // cycle back
            (2, 13),
            (13, 13), // self-loop
            (99, 1),  // edge FROM the disconnected node must not pull it in
        ]);
        let seen = reachable_closure(&[1, 2], &edges);
        let mut got: Vec<i64> = seen.into_iter().collect();
        got.sort_unstable();
        assert_eq!(got, vec![1, 2, 10, 11, 12, 13]);
    }

    /// Zero roots → empty closure (the indeterminate substrate); duplicate
    /// edges must not double-visit or loop.
    #[test]
    fn closure_zero_roots_empty_and_duplicate_edges_safe() {
        let edges = edge_map(&[(1, 2), (1, 2), (2, 1)]);
        assert!(reachable_closure(&[], &edges).is_empty());
        let seen = reachable_closure(&[1], &edges);
        assert_eq!(seen.len(), 2);
    }
}
