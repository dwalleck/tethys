# tethys-4m9o design — batch shortest dependency-chain queries

## Purpose

`Tethys::get_dependency_chain` must answer on real indexes. The probe
(findings.md) showed the recursive CTE in `Index::find_dependency_path`
enumerates every walk to depth 50 (8×10³⁰ rows on tethys's own 114-file
index — tethys-vwrn), so every query touching the cyclic region hangs;
hydration of any found path is a per-id `get_file_by_id` loop (the 2+N
shape tethys-n8pu removed from direct deps). This slice replaces the
traversal with a storage-owned visited-set BFS and batches hydration,
preserving every observable semantic the probe recorded.

## Architecture

`Index::find_dependency_path(from, to)` (src/db/graph.rs) becomes:

1. Equal endpoints → hydrate the single id via the batch helper; return
   the one-file path. (As built: both arms funnel through
   `FilePath::new`, and the now-uncalled crate-internal
   `FilePath::single` was deleted rather than kept as dead code —
   surfaced by the pre-PR standards review.)
2. Load the full adjacency via the existing `build_adjacency_list()`
   (one statement — the `detect_cycles` precedent).
3. Visited-set BFS with parent map, expansion capped at
   `DEFAULT_MAX_DEPTH` (50) edges, early-exit when `to` is reached.
4. No route → `Ok(None)`. Route → reconstruct the id path and hydrate
   ALL members with one new `Index::get_files_by_ids(&[FileId])`
   (src/db/files.rs, `WHERE id IN (…)` via `params_from_iter`), reorder
   in path order; any missing member → `Error::NotFound` (current
   defensive behavior, kept).

`parse_path_ids` (CTE-only helper) is deleted. The facade
`Tethys::get_dependency_chain` is unchanged: endpoint validation stays
there (`get_file_id` from-then-to, established NotFound messages), and
the return type stays `Option<Vec<PathBuf>>`.

## Input shapes

Endpoint pairs: equal (C4); distinct direct-edge (C2/C3); distinct
multi-hop (C1/C2/C3); distinct disconnected (C5); from missing (C6); to
missing (C6); both missing (C6, from-first order). Graph shapes: cyclic
(C1); acyclic chain at/over the depth cap (C8); self-loop edge (C9);
node with zero out-edges and zero-edge graph (C5); multiple equal-length
shortest routes (C2 asserts length + validity, not a unique route).
Hydration id-list sizes: one (C4), two (C7), three-plus (C7); dangling
id (C10). Out of scope with reasons: duplicate edges (schema PK
`(from_file_id, to_file_id)` makes them unrepresentable); non-workspace
/ unicode path inputs (path→id mapping is untouched facade code, C6
exercises its miss path; ids drive everything below it).

## Removed-invariant sweep (step 2b)

The core move replaces a single-statement SQL traversal with
load-then-BFS-then-hydrate. Constraints removed and their coverage:
the CTE's implicit depth cap → preserved explicitly (C8); per-id
hydration's NotFound-on-dangling → preserved (C10). The single-lock
"traversal is one snapshot" property is *unchanged in posture*:
pre-change, traversal and hydration were already separate statement
windows (CTE, then N lookups); post-change there are exactly two windows
(adjacency load, then one hydrate) — one fewer interleaving window than
before, same reader (`&self`) surface `detect_cycles` already accepts.
No lock, ordering, or uniqueness property is otherwise removed.

## Claims and falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|-------|-----------|--------|------|--------|------------------|
| C1 | Every endpoint pair on tethys's own cyclic index (114 files, 396 edges, 37 cycles) answers in < 1 s | Run `examples/probe_4m9o` on the six probe pairs; any timeout falsifies | Wall clock + `oracle_bfs.py` outcome class | 5m | **passed** (design prototype: worst 0.03 ms, all six agree; post-build probe rerun re-records) | integration `chain_terminates_on_cyclic_graph` — cyclic fixture; pre-fix hangs the test, post-fix returns in ms |
| C2 | Connected pairs return a chain of length BFS-distance + 1 | Fixture with a 2-edge route and a 3-edge route to the same target; any longer-than-shortest result falsifies | Hand-computed distance / `oracle_bfs.py` on real index | 5m | **passed** (probe: CTE@8 and prototype both len 8 = oracle on diameter pair) | integration `chain_prefers_shortest_route` + hardened `get_dependency_chain_finds_shortest_path` (asserts `Some` + exact len, no `if let` hedge) |
| C3 | Every consecutive chain pair is a real `file_deps` edge; first = from, last = to | Validate each hop of probe/fixture outputs against raw SQL edge rows; any missing edge falsifies | Raw SQL `file_deps` lookups | 5m | **passed** (probe path ids validated hop-by-hop) | integration fixtures assert exact expected node sequences |
| C4 | Equal indexed endpoints → `Some` one-file chain | Query (f, f); `None`, error, or len ≠ 1 falsifies | Probe run (0.1 ms) + files-table row | 5m | **passed** (probe) | hardened same-file test: asserts `Some(vec![f])`, replacing the "either is acceptable" hedge |
| C5 | Disconnected indexed endpoints → `Ok(None)`, fast | Real disconnected pair (`src/main.rs → src/db/schema.rs`) + island/zero-edge fixtures; error, hang, or `Some` falsifies | `oracle_bfs.py` NONE class | 5m | **passed** (prototype 0.02 ms; production code currently hangs — the fix target) | existing `..._returns_none_for_unconnected` + cyclic-fixture island case (pre-fix: hang) |
| C6 | Missing from / to / both → established `NotFound` before traversal, from checked first | Probe missing pairs; wrong error, wrong order, or `Ok(None)` masking falsifies | Error message text + statement absence | 5m | **passed** (probe: 0.0/0.1 ms, correct messages both directions) | existing nonexistent-from/to tests + new both-missing order fence |
| C7 | `find_dependency_path` issues exactly 2 statements connected, 1 disconnected, 1 equal — zero per-id `files WHERE id =` lookups at any path length | Trace-hook counts on len-2, len-4, disconnected, equal cases; any per-id count > 0 or growth with length falsifies | rusqlite `trace` hook on live connection (n8pu fence pattern) | 30m | pending (build) | `chain_hydration_is_batched_zero_per_id` cfg(test) fence in src/db/graph.rs; current per-id loop fails it (per-id = L) |
| C8 | Depth cap preserved: 50-edge chain → `Some(51)`, 51-edge chain → `None` | db-unit linear chains at 50/51 edges; `Some` at 51 (cap dropped) or `None` at 50 (off-by-one) falsifies | Hand-built chain arithmetic | 30m | pending (build) | `chain_respects_depth_cap_boundary`; uncapped BFS or `<=` off-by-one fails it |
| C9 | A self-loop `file_deps` row neither hangs nor perturbs results | Fixture with `(x,x)` edge; hang or wrong length falsifies | Hand-computed fixture distances | 10m | pending (build) | self-loop edge inside the cyclic fixture; visited-set-less BFS fails it |
| C10 | A dangling path-member id → `Error::NotFound`, never a silently shortened chain | Fabricate dangling id with `PRAGMA foreign_keys=OFF` (n8pu dangling-fence pattern); lenient skip returning a shorter `Some` falsifies | Raw-SQL fabricated fixture + error class | 30m | pending (build) | `chain_dangling_member_is_notfound`; a filter-and-continue hydration fails it |

Cheapest falsifier (C1/C2/C5 prototype, one run): **passed before
presenting this design** — proposed algorithm vs independent BFS oracle,
six pairs, all agree, worst 0.03 ms (probe1-output.txt + transcript).

Divergence note (C10): tethys-n8pu made *direct-deps* hydration lenient
(warn + missing count) because a missing member still leaves a valid
result set. A chain with a missing member is not a chain — the error is
the correct posture, and it preserves today's observable behavior.

## Negative space

1. No CLI subcommand is added; `get_dependency_chain` remains a
   library-only facade method, exactly as today.
2. The facade signature and result type (`Option<Vec<PathBuf>>`) do not
   change; graph-specific result DTOs are the contract-phase work of
   tethys-71if (verified open).
3. Cycle detection and canonicalization are untouched — tethys-u5o5
   (verified open) owns that; this slice only *reuses*
   `build_adjacency_list`.
4. Symbol-level shortest paths are not built — tethys-syau (verified
   open, P3).
5. The `src → tests/orphan_files.rs` edge misattribution the probe
   surfaced is not fixed here — filed as tethys-r77e; the chain query
   faithfully reports stored edges regardless of their provenance.
6. No adjacency-load perf work: the O(E) full-table load matches the
   `detect_cycles` precedent and is the settled trade for statement-count
   determinism at this index scale.

## Consumers and blast radius

`find_dependency_path` has exactly one caller (`Tethys::get_dependency_chain`);
`parse_path_ids` has exactly one caller (the CTE being deleted);
`get_dependency_chain` is exercised by tests/graph.rs and
benches/queries.rs only (grep + `tethys callers` dogfood re-run at build
time). Existing acyclic tests keep passing unchanged except the two
hedged tests, which are hardened per C2/C4.
