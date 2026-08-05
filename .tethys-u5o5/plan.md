# Plan: Batch and canonicalize file dependency cycles

The plan implements the approved claims in `.tethys-u5o5/design.md`. Each slice is a falsifiable hypothesis, touches at most two files, and is committed only after its unit tests, stress fixture, probe oracle, budget check, and regression fence pass.

## Slice 1: Enumerate complete canonical cycles in memory

**Claim:** C2, C3, and C4 — every simple directed cycle is returned once; rotations collapse; reverse direction remains distinct; path-based canonicalization and result ordering are deterministic.

**Oracle:** `.tethys-u5o5/design_falsifier.py`, which exhaustively starts DFS from every node and canonicalizes independently, compared with the start-min candidate. The Rust unit test expected set is hand-derived from the same inserted edge pairs but does not call the production enumerator.

**Stress fixture:** A six-cycle graph with `a↔b↔c` edges, both three-node orientations, overlapping two-node cycles, a self-loop, and a disconnected node; expected exact set is six canonical cycles. A sparse 1,000-node/5,000-edge graph with one known cycle exercises path pruning without a dense-SCC output explosion. These fail if the visited set suppresses overlapping cycles, direction is reversed, the start guard is wrong, or results depend on ID order.

**Loop budget:** Start-min DFS is `O(P + E)` traversal work for `P` valid simple paths examined plus `O(C × L)` materialization for `C` cycles of mean length `L`; worst-case `P` is exponential because complete simple-cycle enumeration is output-sensitive. The production-shaped stress fixture is constructed with `P ≤ 1,000,000`, `V = 1,000`, `E = 5,000`, and `C = 1`, so the bounded fixture stays below the 10^6-operation budget while the implementation retains no output cap. Neighbor sorting is `O(E log E)` once.

**Wall budget:** On-demand query; no always-on wall budget. Record the sparse stress fixture elapsed time for the slice evidence.

**Files:** `src/db/graph.rs`

**Code (advisory):** Replace the current global-visited/back-edge helper and numeric-ID normalizer with private path-aware adjacency traversal. Integrate it into the existing method by loading the indexed path map through `list_all_files()` before traversal; retain the existing scalar `ids_to_cycle` conversion and non-transactional read shape until Slice 2 removes that N+1 seam. The helper's unit seam takes indexed paths plus edges and returns canonical ID sequences.

**Impact analysis:** The changed private helpers are called only by `Index::detect_cycles()` in `src/db/graph.rs`; the public delegation chain is `src/lib.rs::Tethys::detect_cycles` → `src/db/graph.rs::Index::detect_cycles` → `src/cli/cycles.rs::run`. No public signature changes.

**Verification:**

- [ ] Unit tests pass, including exact six-cycle and sparse stress tests.
- [ ] `./.tethys-u5o5/design_falsifier.py` still prints its PASS line.
- [ ] `.tethys-u5o5/probe.py` still agrees with the binary's existing two-file real-workspace output.
- [ ] Traversal and sorting budgets hold at the stated fixture scale.
- [ ] Regression fences: `src/db/graph.rs` cycle-enumerator tests.

## Slice 2: Bulk-load paths and preserve snapshot/error contracts

**Claim:** C5, C6, and C7 — cycle projection uses indexed stored paths, exactly two set-valued reads, zero per-member lookups, and one read snapshot; dangling endpoints remain `Error::NotFound`.

**Oracle:** SQLite trace classification counts only the two `SELECT` statements, rejects the known scalar `FROM files WHERE id =` canary after the call, and compares every emitted path to the independently captured `files` map. A second SQLite connection writes between read points in the snapshot fixture; the expected result is the pre-write or post-write state, never a hybrid.

**Stress fixture:** A temporary `Index` with 1,000 indexed paths (including spaces and Unicode), 5,000 sparse edges, one four-file cycle, an empty index, and an acyclic index. Attach the existing rusqlite trace hook; manually insert a dangling endpoint in a transaction for the error case; use an external writer to attempt a mid-read edge/path change. This fails if conversion calls `get_file_by_id`, the file query is skipped for empty/acyclic inputs, a missing row is silently filtered, or the two reads use different snapshots.

**Loop budget:** File and edge loading is `O(V + E)` with `V = 1,000` and `E = 5,000` in the stress fixture (`6,000` row iterations, below 10^6) and exactly two set-valued SQL statements. Path projection is `O(C × L)` in memory. Database syscalls remain `2` SELECTs plus transaction control, independent of cycle members; scalar lookup syscalls are `0`.

**Wall budget:** On-demand query; no always-on wall budget. The trace fence is the authoritative performance oracle.

**Files:** `src/db/graph.rs`, `src/types.rs`

**Code (advisory):** In `Index::detect_cycles()`, acquire one connection guard, begin a read transaction, load `id,path` ordered by path and all `file_deps` rows, invoke the Slice 1 path-aware enumerator, project IDs through the map with typed `NotFound` errors, and commit. Update `Cycle` rustdoc to state stored relative paths, dependency direction, canonical rotation, deterministic result order, and no repeated first member. Reuse the existing connection, transaction, `HashMap`, `PathBuf`, and typed error patterns; do not call `list_all_files()` or `get_files_by_ids()` because they acquire the connection outside the transaction.

**Impact analysis:** `Index::detect_cycles()` semantics change; callers are the unchanged `Tethys::detect_cycles()` delegation in `src/lib.rs` and CLI display in `src/cli/cycles.rs`. `Cycle` rustdoc is public but its field shape is unchanged; existing integration tests are the callsite fence.

**Verification:**

- [ ] Unit tests pass, including empty, acyclic, cyclic, dangling, trace, and snapshot fixtures.
- [ ] Stress fixture returns the exact four-file cycle and path set.
- [ ] `.tethys-u5o5/probe.py` and its independent SQLite oracle still agree against the rebuilt binary.
- [ ] The `O(V + E)` and two-query/zero-scalar budgets hold at `V=1,000,E=5,000`.
- [ ] Regression fences: `cycle_query_statement_counts_are_flat`, `cycle_query_uses_one_snapshot`, and typed dangling-edge assertion.

## Slice 3: Fence the public Tethys behavior end to end

**Claim:** C1 and C8, plus the public integration forms of C2–C5 — Tethys returns exact cycle sets through the existing seam for empty, acyclic, self-loop, two-file, long, overlapping, reverse, rotated, path, and error cases.

**Oracle:** For each temporary workspace, an edge table written beside the fixture gives the expected canonical cycle set independently of the returned result. The facade is the only call surface; no test reaches `Index` internals.

**Stress fixture:** Extend `tests/graph.rs` with temporary Rust workspaces that use nested directories, spaces, Unicode, reversed file creation order, disconnected acyclic files, a self-loop authored through direct index rows, a three-file bidirectional SCC with overlapping cycles, and a four-file cycle. Expected `Vec<Cycle>` values are written before each call and compared exactly, including order and no repeated first path.

**Loop budget:** Fixture construction is `O(V + E)` file/edge inserts with at most `V = 12` and `E = 20`; cycle assertions are `O(C × L)`. No production loop is introduced in this slice. Test-only writes are not product syscalls and are bounded by the fixture.

**Wall budget:** Test-only integration fixture; no always-on production wall budget.

**Files:** `tests/graph.rs`, `src/lib.rs`

**Code (advisory):** Replace loose `contains` assertions in the existing cycle tests with exact contract assertions where they overlap the new fixtures. Add focused Tethys tests without changing CLI formatting. Clarify the public `Tethys::detect_cycles()` rustdoc; keep its return type and delegation unchanged.

**Impact analysis:** `Tethys::detect_cycles()` has existing callers in `src/cli/cycles.rs` and `tests/graph.rs`; no signature or behavior outside the approved cycle contract changes.

**Verification:**

- [ ] Targeted Tethys graph integration tests pass.
- [ ] `.tethys-u5o5/probe.py` and all design falsifiers still agree with the rebuilt binary.
- [ ] Fixture output exactly matches the independent edge tables.
- [ ] Regression fence: `tests/graph.rs` exact cycle-set tests and `tests/seam_lint.rs`.


### Every loop

- Slice 1 start-min path enumeration: `O(P + E)`, with `P` bounded by the sparse stress fixture at `≤ 1,000,000`; output-sensitive exponential behavior is explicit and not hidden behind a false linear label.
- Slice 1 retains the pre-existing scalar `ids_to_cycle` conversion only as a transitional seam; Slice 2 deletes that loop and replaces it with in-memory projection, so it is not a newly introduced production loop.
- Slice 1 neighbor/result sorting: `O(E log E)` and `O(C log C × L)` respectively; fixture values are recorded.
- Slice 2 two bulk row loops: `O(V + E)` at `V=1,000,E=5,000`, exactly `6,000` row iterations; path projection `O(C × L)`.
- Slice 3 fixture setup/assertion loops are test-only and bounded.

### Every fixture and bug class

- Six-cycle graph: missing overlapping cycles, reverse conflation, self-loop loss, duplicate rotations, and repeated-first output.
- ID/path permutation: numeric-ID canonicalization and nondeterministic HashMap order.
- Unicode/spaces/dangling row: lossy/absolute projection and silent invalid-row filtering.
- 1,000/5,000 sparse graph: unbounded setup work or per-member database reads.
- Trace canary: scalar lookup regression and vacuous statement count.
- Concurrent writer: mixed SQLite snapshots.
- Empty/acyclic indexes: skipped file load and false non-empty output.

### Doc-comment preconditions

- `Cycle.files` path/direction/ordering contracts are enforced by runtime query construction and exact integration assertions; no `debug_assert!` is used as a correctness check.
- No new public method precondition is introduced.

### Write targets

- Production library and CLI interfaces write no output in these slices. Returned cycle paths are data values. Existing CLI data remains stdout; diagnostics/errors remain stderr/error values.
- Test fixtures write only temporary workspace/index files.

### Tracker references

- No deferral or future-work claim appears in this plan; no tracker reference is required.

### Claim coverage

- C1 → Slice 3.
- C2/C3/C4 → Slice 1 implementation and Slice 3 facade fences.
- C5/C6/C7 → Slice 2 implementation and Slice 3 path/error fences.
- C8 → Slice 3.
