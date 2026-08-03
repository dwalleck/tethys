# Design: Batch and canonicalize file dependency cycles

## Purpose

Deepen the existing `Index::detect_cycles()` implementation behind the unchanged `Tethys::detect_cycles()` interface. The implementation will enumerate complete simple directed cycles from one consistent SQLite read snapshot, then project the selected file IDs to stored workspace-relative paths from one in-memory map.

The design is grounded by `.tethys-u5o5/probe.py`: the real indexed tethys workspace currently exposes the two-file `src/cargo.rs` ↔ `src/lib.rs` cycle through the public CLI, and an independent SQLite oracle agrees on that canonical pair. The current implementation's edge load is already set-oriented; its path conversion is the N+1 hotspot.

## Input shapes

- **Indexed file set**: empty; one file with no edges; one file with a self-loop; two files; a longer simple cycle; multiple disconnected components; an acyclic graph; an SCC containing overlapping cycles; and an SCC containing both directions.
- **Dependency edges**: zero edges; one edge; unique `(from_file_id, to_file_id)` rows; parallel duplicate rows rejected by the schema primary key; and a deliberately corrupted dangling endpoint.
- **Path values**: ASCII names; nested relative paths; spaces; Unicode; and database IDs inserted in an order unrelated to lexical path order.
- **Concurrent state**: no writer; a writer that changes rows between the two reads; and a writer that changes rows after the read transaction commits.
- **Result collection**: zero cycles; one cycle; and multiple cycles whose canonical path sequences tie at a prefix.

## Removed-invariant sweep

The change removes the current global DFS `visited` constraint as the mechanism that limits traversal to one back-edge cycle per reachable region, and replaces numeric-ID/HashSet traversal ordering with path-based canonicalization. Those old constraints guaranteed the following facts, each preserved by a new claim:

- A node was expanded at most once; the new traversal instead expands a simple path and rejects repeated members, while start-min pruning prevents duplicate rotations.
- A discovered cycle was rotated by the smallest numeric ID; the new implementation rotates by the smallest stored relative path and preserves edge direction.
- Every cycle member was independently checked by `get_file_by_id`; the new path map must still return `Error::NotFound` for any missing endpoint.
- The current connection mutex serialized each scalar read but did not define a multi-query snapshot; both bulk reads now run inside one explicit read transaction.

## Architecture and placement

- **Owner**: `src/db/graph.rs`, on the existing concrete `Index::detect_cycles()` operation. This module already owns file-graph traversal and is the only place that needs the database rows, adjacency representation, cycle enumeration, canonicalization, and path projection.
- **Seam**: the existing `Tethys::detect_cycles()` method in `src/lib.rs` remains the external seam. Its signature and `Cycle` result shape do not change.
- **Internal implementation**: acquire the `Index` connection once, begin a read transaction, run exactly one set-valued `SELECT id, path FROM files ORDER BY path` and one set-valued `SELECT from_file_id, to_file_id FROM file_deps`, then commit after conversion. Build deterministic path-sorted node and neighbor lists. For each possible start path, enumerate simple paths while refusing nodes lexically smaller than the start; record a cycle only when an edge returns to the start. Sort the resulting canonical path vectors lexicographically before constructing `Cycle` values.
- **Forbidden**: do not add a graph adapter trait, change `Tethys`, make the CLI query the database directly, call `get_file_by_id` from cycle conversion, rescan the filesystem, or silently skip a missing endpoint. The CLI's first-file repeat is presentation-only and remains outside the library result.
- **Dependencies**: reuse the existing `Index` connection/error handling, `PathBuf`, `HashMap`, `HashSet`, and SQLite transaction patterns. No new crate or public helper is needed.

## Claims and falsification

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|---|
| C1 | Empty, acyclic, self-loop, two-file, and long-cycle input shapes produce the exact specified cycle sets, with no closing repeat. | Run a Tethys integration fixture containing all five shapes. Any missing, extra, or repeated path falsifies C1. | Hand-enumerated edge-to-cycle table independent of the detector. | 10m | pending | `tests/graph.rs::detect_cycles_covers_empty_acyclic_self_two_and_long` |
| C2 | Every simple directed cycle is returned exactly once, including overlapping cycles in one SCC, without a cycle-length/count cap. | Use the six-cycle graph in `design_falsifier.py`; any difference between exhaustive enumeration and the start-min candidate falsifies the enumeration rule. | Exhaustive Python DFS from every start, canonicalized after enumeration. | 1m | **passed** | `src/db/graph.rs` cycle-enumerator unit fence plus `tests/graph.rs::detect_cycles_enumerates_overlapping_cycles` |
| C3 | Rotations deduplicate while reverse directed sequences remain distinct, and each path follows stored `from -> to` edges. | Fixture with three nodes carrying both orientations and all rotations. A count other than the exact directed cycle set falsifies C3. | Independent expected set generated from the inserted edge pairs. | 15m | pending | `tests/graph.rs::detect_cycles_deduplicates_rotations_and_preserves_direction` |
| C4 | Canonical rotation and result ordering depend on relative paths, not file IDs or HashMap order. | Index the same graph twice with reversed insertion order and paths that sort differently from IDs. Any path sequence difference or non-minimal first path falsifies C4. | Lexical sort of stored relative path vectors. | 15m | pending | `tests/graph.rs::detect_cycles_is_deterministic_and_path_canonical` |
| C5 | Every emitted member is an indexed, stored workspace-relative path; a dangling edge produces `Error::NotFound`. | Include nested Unicode/spaced paths and inject an edge endpoint absent from `files`. Any absolute/lossy path or successful invalid cycle falsifies C5. | Direct SQL inspection of indexed rows plus the documented error expectation. | 20m | pending | `tests/graph.rs::detect_cycles_preserves_paths_and_rejects_dangling_edges` |
| C6 | Hydration uses exactly two set-valued reads per call, including empty and acyclic indexes, and zero scalar file lookups during cycle conversion. | Attach a SQLite trace hook; a third `SELECT`, a scalar `FROM files WHERE id =` canary, or a count that grows with cycle members falsifies C6. | SQL trace classification independent of returned cycles. | 15m | pending | `src/db/graph.rs::cycle_query_statement_counts_are_flat` |
| C7 | Both bulk reads observe one SQLite read snapshot. | Arrange an external writer to alter `files`/`file_deps` between the two read points. A mixed file/edge state falsifies C7. | A transaction-scoped expected state recorded before the writer runs. | 30m | pending | `src/db/graph.rs::cycle_query_uses_one_snapshot` |
| C8 | The public seam remains a deep, unchanged `Tethys::detect_cycles() -> Result<Vec<Cycle>>` interface; callers receive only the cycle paths and errors above. | Compile and run the existing Tethys integration call sites after replacing the implementation. Any facade/CLI signature change or direct CLI database access falsifies C8. | Existing public integration tests and seam-lint source check. | 10m | pending | Existing `tests/graph.rs` cycle tests plus `tests/seam_lint.rs` |

The cheapest falsifier was C2. `./.tethys-u5o5/design_falsifier.py` passed:

```text
PASS: exhaustive oracle and start-min candidate agree on 6 cycles
```

The script exercises self-loop, overlapping, reverse-direction, and rotation cases. A buggy start-min condition, path-visited guard, or direction reversal would produce a distinct candidate set and fail before implementation planning.

## Complexity and output budget

Let `V` be indexed files, `E` dependency edges, and `C` the number of simple directed cycles returned. Loading costs `O(V + E)` time and `O(V + E)` memory. Enumerating all simple cycles is output-sensitive: the start-min DFS visits simple paths, so worst-case work is exponential in a dense SCC and necessarily at least `Ω(C × L)` to materialize `C` cycles of mean length `L`; no implementation can satisfy complete enumeration with a lower output bound. Path projection and canonical result sorting cost `O(C × L log(C))` comparisons after enumeration. At the observed tethys probe scale (`V = 115` indexed files), the fixed database work remains two set reads and no per-member syscalls; the stress fixture will separately exercise a sparse 1,000-file/5,000-edge graph and record `C` and wall time rather than pretending dense-SCC output is linear.

## Negative space

- No change to edge extraction, module resolution, reindexing, or the meaning of a `file_deps` row.
- No symbol-call, type-hierarchy, package, or filesystem cycle analysis.
- No new public cycle DTO, adapter trait, CLI flag, or result filter.
- No silent truncation of complete simple-cycle output and no retry policy for database errors.
- No cross-workspace path discovery or repair of corrupted index rows.

## Output stream and error posture

`Tethys::detect_cycles()` returns data only; it writes no stdout or stderr. The CLI continues to write human-readable data to stdout and has no new diagnostic stream. Database failures and dangling endpoints propagate as typed `Error` values; no `Option`/`Result` is suppressed during bulk loading or path projection.

## Plan handoff

The implementation plan must split the work into slices no larger than two files each: (1) pure cycle enumeration/canonicalization and database bulk-loading internals, (2) Tethys integration/regression fixtures, and (3) trace/snapshot fences if their test seam cannot fit with slice 1. Each slice must retain C1–C8 coverage and rerun the probe oracle before commit.
