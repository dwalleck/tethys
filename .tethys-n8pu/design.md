# tethys-n8pu design — hydrate direct file dependency queries in one pass

Parent epic: tethys-6k6b (graph analyses behind the Tethys seam).
Blocker tethys-mv36 (concrete Index queries) closed — the direct queries are
the remaining per-ID hydration stragglers.

## Purpose

`Tethys::get_dependencies` / `Tethys::get_dependents` resolve result paths by
looping `Index::get_file_by_id` once per returned ID (`file_ids_to_paths`).
Replace that loop with set-oriented hydration: a single SQL JOIN over
`file_deps` ✕ `files` in each direction. Same observable results, same
NotFound behavior, O(1) queries per call instead of O(N).

## Architecture

- `src/db/file_deps.rs`: add `Index::get_file_dependency_paths(file_id) ->
  Result<Vec<PathBuf>>` and `Index::get_file_dependent_paths(file_id)` —
  one LEFT JOIN statement each:

  ```sql
  SELECT fd.to_file_id, f.path
  FROM file_deps fd LEFT JOIN files f ON f.id = fd.to_file_id
  WHERE fd.from_file_id = ?1
  ```

  (mirror for dependents with `fd.from_file_id`, `f.id = fd.to_file_id`,
  `WHERE fd.to_file_id = ?1`). Rows with NULL `path` are dangling rows:
  warn per row exactly like `file_ids_to_paths` does today, count them.
  Selecting `fd.to_file_id` alongside keeps the per-row warn payload
  identical (`source_file_id`, `missing_file_id`).

  The existing ID-returning `get_file_dependencies` / `get_file_dependents`
  stay byte-for-byte untouched — they are public `Index` surface whose
  contraction belongs to tethys-71if, not this slice.

- `src/lib.rs`: `get_dependencies` / `get_dependents` call the new path
  methods; the per-direction missing-count summary `debug!` is preserved.
  Private `file_ids_to_paths` is deleted (its only callers are the two
  methods being rewired).

- No schema change: `file_deps` PK `(from_file_id, to_file_id)` serves
  `WHERE from_file_id = ?1`; `idx_file_deps_to` serves `WHERE
  to_file_id = ?1`. Both directions already indexed.

- Result order: no ORDER BY is added. Both the old query and the new JOIN
  scan the same PK/`idx_file_deps_to` index in the same order, so the
  observable order is unchanged; no existing assertion depends on order
  (all set-based `any()`/sorted comparisons — verified by reading the
  consumers in tests/).

## Input shapes (get_dependencies / get_dependents)

| shape | production-reachable | claim |
|---|---|---|
| root indexed, ≥1 result | yes | C1/C2 |
| root indexed, 0 results | yes | C3 |
| root not in index (missing / unindexed file) | yes | C4 |
| root path absolute/outside workspace | yes — `relative_path` falls back as-is → NotFound like today | C4 |
| results contain duplicate edges | no — PK `(from,to)` forbids; result set | C5 |
| result IDs with dangling `files` rows | no under FK=ON (Index::open sets it) — corrupt-db defense | C7 |
| N (result count) large | yes | C6 |

## Claims

1. `get_dependencies(path)` returns exactly the workspace-relative paths of
   the files `path` directly depends on, for roots with ≥1 dependency.
2. `get_dependents(path)` returns exactly the workspace-relative paths of
   the files directly depending on `path`, for roots with ≥1 dependent.
3. A root with no dependencies/dependents returns an empty `Vec`, not an
   error.
4. A root absent from the index returns `Error::NotFound`, and the call
   issues only the root lookup (no hydration queries run).
5. Returned paths contain no duplicate indexed file.
6. Hydration is set-oriented: a non-empty `get_dependencies` /
   `get_dependents` call issues exactly 2 SQL statements (root lookup +
   one JOIN) and zero statements of the per-ID shape
   (`... FROM files WHERE id = ...`), for any result count N.
7. Dangling result rows (hand-edited db with FK enforcement bypassed) are
   skipped with the established per-row `warn!` + summary `debug!` logs;
   valid rows are still returned; the call does not error.
8. Existing Rust and C# direct file-dependency integration tests pass
   unchanged (`tests/indexing.rs`, `csharp_l2_file_deps.rs`,
   `csharp_cross_dir_deps.rs`, `file_deps_idempotency.rs`, `graph.rs`,
   `orphan_files.rs`, `reexport_refs.rs`).
9. Public API unchanged: `Tethys::get_dependencies` /
   `get_dependents` signatures and the `Index` ID-returning getters are
   untouched.

## Falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| 1 | deps = exact workspace-relative set | probe: real temp index, deps(lib.rs) vs direct SQL | direct JOIN SQL on same db | 5m | passed (probe1-output.txt) | probe test `n8pu_probe::probe_direct_dep_hydration` |
| 2 | dependents = exact set | probe: dependents(c.rs) vs direct SQL | direct JOIN SQL | 5m | passed | same probe test |
| 3 | empty root → empty vec | probe: deps(b.rs) = [] (2 stmts, 0 per-id — empty case is already O(1) pre-fix) | direct SQL count | 5m | passed | same probe test |
| 4 | unindexed root → NotFound, 1 stmt | probe: deps(src/nope.rs); trace count == 1 | code path inspection | 5m | passed (NotFound + stmt count 1 measured) | same probe test |
| 5 | no duplicate paths | probe: assert len == set len | PK schema | 5m | passed | same probe test |
| 6 | exactly 2 stmts, 0 per-ID, any N | probe trace hook; pre-fix measured 2+N (5 total, 3 per-ID for N=3) | rusqlite `trace` hook on live connection (independent runtime measurement); code inspection of removed loop | 5m | pending (fails pre-fix by construction — that IS the ticket) | probe test asserts per-ID == 0, total == 2 per direction |
| 7 | dangling rows skipped + warn + valid rows returned | unit test: open db with FK OFF, insert dangling `file_deps` row, call through Tethys | tracing-test capture of `warn!`/`debug!`; direct SQL | 15m | pending | `n8pu_probe` unit test |
| 8 | existing Rust/C# suites pass | `cargo nextest run` | unchanged test expectations | 10m | pending | the existing tests themselves |
| 9 | public API unchanged | `cargo check` + full suite compile | — | 5m | pending | compile |

**Non-vacuity (buggy implementations that fail the fence):**
- C1/C2: wrong JOIN direction (join on `to_file_id` for deps) returns
  dependents for a deps call → probe fails.
- C3: hydration erroring on empty result instead of returning `Vec::new()`
  → probe fails.
- C4: hydrating before root validation returns `Ok([])` for a missing root
  → probe fails.
- C5: UNION/duplicate-emitting query → probe fails.
- C6: keeping the `file_ids_to_paths` loop (the bug being fixed) → per-ID
  count > 0 → probe fails.
- C7: inner JOIN silently dropping dangling rows → warn/debug capture is
  empty → test fails.
- C8: any edge-semantics regression → existing fences fail.
- C9: signature drift → compile error.

**Cheapest falsifier run:** the probe (5m, ran during prove-it-prototype).
Claims 1-5 passed pre-fix; C6's pre-fix run documents the defect being
fixed (2+N measured). The probe lives on as the regression fence in
`src/db/file_deps.rs`; the build slice adds the C3/C6 count assertions and
the C7 FK-off test.

## Negative space (deliberately not done)

1. **No change to transitive/impact traversal** — `get_transitive_dependents`,
   `get_impact`, affected-tests all untouched (ticket scope).
2. **No result ordering contract** — no ORDER BY added; order remains
   index-scan order as today; nothing in the repo asserts order.
3. **No removal of `Index::get_file_dependencies`/`get_file_dependents`
   (ID-returning)** — public surface; contraction is tethys-71if's slice.
4. **No C#-specific handling** — hydration is table-level and
   language-neutral; C# edges flow through the same `file_deps` rows.
5. **No schema or index changes** — both directions already indexed.
6. **No CLI changes** — no CLI command surfaces direct file deps.

## Tracker references

- tethys-71if (open, verified) — owns removal of the legacy ID-returning
  getters and other graph-surface contraction.
- tethys-zoi3 (open, verified) — may later extend file_deps coverage
  (target deletion); C7's defensive branch is where such tests would bite.
- tethys-8ya3 (open, verified) — write-path batching, unrelated direction.
- No deferrals introduced by this design.
