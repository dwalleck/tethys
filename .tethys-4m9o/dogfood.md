# tethys-4m9o dogfood — impact analysis for the changed symbols (2026-08-02)

Per AGENTS.md, this slice edits tethys's OWN graph query code, so the tool
cannot oracle a change to itself: `grep` is the source of truth,
`tethys callers` recorded as advisory.

## grep (recall tier — source of truth)

- `parse_path_ids` — zero references remain anywhere (deleted with the CTE).
- `FilePath::single` — zero references remain (deleted; equal-endpoint arm
  now batch-hydrates through `get_files_by_ids`).
- `find_dependency_path` — src/lib.rs (facade `get_dependency_chain`),
  src/db/graph.rs (definition + fences). No other callers.
- `get_files_by_ids` — src/db/files.rs (definition + tests),
  src/db/graph.rs (`find_dependency_path`, fences). No other callers.

## tethys callers (precision tier — advisory)

- `Index::find_dependency_path` → 1 caller reported (`chain_paths`, a test
  helper). **Missed the facade caller** `Tethys::get_dependency_chain`
  (`self.db.` receiver, src/lib.rs:461) that grep caught — a live example
  of why the AGENTS.md exception exists. Method-receiver resolution gap,
  same class as the known resolver limitations.
- `Index::get_files_by_ids` → 1 caller (`Index::find_dependency_path`) ✓.

## Post-fix probe rerun (probe2-output.txt)

Re-indexed with the branch binary (115 files, `examples/probe_4m9o.rs`
now indexes itself). All six probe pairs agree with `oracle_bfs.py`;
zero timeouts; worst case 0.1 ms. Pre-fix, three of the six timed out at
60 s (probe1-output.txt).
