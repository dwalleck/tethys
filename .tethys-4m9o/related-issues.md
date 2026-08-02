# tethys-4m9o related issues — tracker prior art (2026-08-01)

Searched `rivets list -n 200` for chain / cycle / shortest / path / graph.

- **tethys-6k6b** (open epic, parent) — PRD: deepen graph analyses behind the
  Tethys seam. 4m9o is one tracer bullet; names "graph work whose database
  round trips grow with the result graph" as a user-visible symptom.
- **tethys-mv36** (closed, blocker) — collapsed one-adapter graph traits into
  concrete Index queries. Satisfied; `find_dependency_path` now lives directly
  on `Index` (src/db/graph.rs).
- **tethys-n8pu** (closed, sibling, PR #38) — file-deps one-pass hydration.
  The house pattern this slice follows: set-oriented LEFT JOIN hydration,
  statement-count probe fence via rusqlite `trace` hook
  (src/db/file_deps.rs::n8pu_probe), integration fences split from unit fences.
- **tethys-u5o5** (open, sibling) — cycles: batch and canonicalize. Shares the
  cyclic-graph substrate; do NOT fix cycle canonicalization here.
- **tethys-7a6a** (open, sibling) — reachable: unify traversal behind the
  seam. Traversal consolidation beyond dependency chains belongs there.
- **tethys-syau** (open, P3) — shortest path between two *symbols*. Future
  consumer of whatever shortest-path shape this slice establishes; not in
  scope.
- **tethys-vwrn** (filed during this probe) — `get_dependency_chain`
  non-terminating on cyclic indexes; the CTE enumerates all walks (8×10³⁰
  rows at depth 50 on tethys's own index). No prior ticket described the
  symptom. Linked `related` to tethys-4m9o, which is the fix vehicle.
