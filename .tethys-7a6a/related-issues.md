# Related issues

- `tethys-6k6b` (open parent epic): defines the shared graph-query contract, including direction semantics, shortest-depth uniqueness, path invariants, BFS discovery order, depth handling, and set-oriented traversal.
- `tethys-71if` (open, blocked by this issue): final contract/cutover phase after unified reachability lands.
- `tethys-3gey` (closed): introduced the legacy forward/backward reachability wrappers, per-visited-symbol queries, and partial-path cloning that this issue replaces.
- `tethys-u1rs` (closed blocker): established the shared traversal-depth contract: zero validates and returns empty; one is direct-only; omitted is 50; oversized values saturate with a warning.
- `tethys-6rxd` (closed): applied the same depth contract to file impact.
- `tethys-4m9o` (closed, PR #39): precedent for one adjacency load plus visited-set BFS and a SQLite statement-count fence.
- `tethys-u5o5` (closed, PR #40): precedent for bulk graph snapshots and deterministic cycle-safe traversal.
- `tethys-mv36` (closed): established concrete `Index` graph operations behind the `Tethys` seam.
- `tethys-6bui` (open): tracks incorrect `is_test` decoding in adjacent graph projections. The unified bulk query must select the real symbol projection, and a forward-reachability fence must assert a non-test target remains `is_test == false`; repairing the adjacent methods remains in `tethys-6bui`.
- `tethys-bvgb` (open): tracks `get_symbol_by_qualified_name` binding the first row when duplicate qualified names exist; the unified operation preserves that source-resolution behavior rather than absorbing the resolver fix.
- `tethys-3i35`, `tethys-staf`, `tethys-qtq5`, `tethys-z9mr` (mixed closed/open): track resolution under-count classes inherited by reachability; they remain outside this traversal change.
- `tethys-e3j1` (open): dangling-edge posture is reserved for `tethys-71if`, not this issue.

No duplicate of `tethys-7a6a` was found. Its recorded blocker `tethys-u1rs` is closed, so the issue is actionable.
