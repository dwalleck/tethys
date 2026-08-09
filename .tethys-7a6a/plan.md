# Unified directional reachability: budgeted implementation plan

Approved design: `.tethys-7a6a/design.md` (2026-08-08).

Global production scale for loop budgets: 100,000 symbols ($V$), 250,000 call edges ($E$), default depth 50. The current self-index probe measured 2,936 symbols and 3,888 edges. Reachability is an on-demand query, not an always-on indexing phase; wall-clock budgets are therefore not applicable. The dominant worst-case comparison and output costs may exceed $10^6$ operations at the declared upper scale, but they are justified: adjacency sorting is required once for deterministic output, and path materialization is proportional to the explicitly requested result payload.

Every slice uses scoped TDD where it adds tests. Every slice gate is: targeted test, stress fixture, prove-it comparator against the current binary, complexity/statement check, regression fence, then full `cargo nextest run`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `cargo test --doc`. Any failure stops the build before commit.

## Slice 1: Load one deterministic reachability snapshot

**Design claim:** Claim 2 — fixed snapshot cost.

**Oracle:** SQLite trace callback with a scalar per-ID lookup canary; raw counts from `SELECT COUNT(*) FROM symbols` and `call_edges` verify snapshot cardinality.

**Stress fixture:** Compare two in-memory indexes: a two-symbol graph with one reachable target, and a 101-symbol chain with exactly 100 reachable targets plus one dangling edge inserted with foreign keys disabled. Both loads must use exactly two SELECT statements and zero per-ID selects. The large snapshot must include all 101 valid symbols and 100 valid edges while omitting the dangling endpoint when adjacency is formed.

**Loop budget:** Symbol collection $O(V)$ = 100,000 rows; edge collection $O(E)$ = 250,000 rows; adjacency construction $O(E)$; per-list sorting $\sum_v O(\deg(v)\log\deg(v))$, worst-case about 4.5 million comparisons for one pathological hub, accepted for an on-demand deterministic query. Memory $O(V+E)$. SQLite statements: exactly 2 inside `Index`; syscalls remain under the 1,000 limit.

**Wall budget:** Not applicable; on-demand query. Both the one-target and 100-target fixtures must complete within the targeted test timeout.

**Files:** `src/db/graph.rs`.

**Smallest code change:** Add the private snapshot record and loader using one deferred transaction, `SYMBOLS_COLUMNS`, `row_to_symbol`, complete `Result` collection, directional endpoint-presence filtering, and `(qualified_name, id)` neighbor ordering. Add canary-guarded unit tests beside existing cycle hydration fences.

**Preconditions:** None documented. SQL/decode failure propagation is load-bearing and enforced with `?`; no `filter_map(Result::ok)` or `.ok()` suppression.

**Output streams:** None.

**Verification:**
- [ ] Snapshot unit tests pass.
- [ ] 100-symbol stress fixture returns expected cardinality and order.
- [ ] Prototype comparisons A/B still return `RESULT: AGREE`.
- [ ] Trace reports `(2 total SELECTs, 0 per-ID SELECTs)` and scalar canary `(1, 1)`.
- [ ] Full Rust gates pass.

## Slice 2: Traverse either call-edge direction

**Design claim:** Claim 3 — directional edge semantics.

**Oracle:** Hand-enumerated asymmetric call graph plus raw SQLite `call_edges` rows; all indexed edges count regardless of provenance support.

**Stress fixture:** Graph `source→a`, `source→b`, `a→c`, `d→source`, including one edge whose only retained support is speculative. Forward depth 2 must be `[a,b,c]` in BFS order; backward depth 2 must start `[d]`; the speculative edge remains traversable.

**Loop budget:** FIFO BFS $O(V+E)$; at most 100,000 queue pops and 250,000 edge examinations. One parent and one queue insertion per discovered symbol. No additional SQL or syscalls beyond Slice 1.

**Wall budget:** Not applicable; on-demand query.

**Files:** `src/db/graph.rs`.

**Smallest code change:** Add `Index::get_reachable(source_id, direction, max_depth)` and a predecessor-map BFS over the snapshot. Build only the requested adjacency orientation. Return discovery metadata projected as existing `ReachablePath` records.

**Preconditions:** Source ID is supplied by the facade; a missing ID yields an empty traversal only if the source record is absent from the snapshot, but the public runtime lookup prevents that production shape. No undocumented panic/assert precondition.

**Output streams:** None.

**Verification:**
- [ ] Directional unit tests pass for both enum variants and speculative support.
- [ ] Asymmetric stress fixture matches the raw edge oracle.
- [ ] Prototype comparisons A/B still agree.
- [ ] $O(V+E)$ queue/parent counts stay within fixture bounds.
- [ ] Full Rust gates pass.

## Slice 3: Publish the canonical facade and depth contract

**Design claim:** Claim 7 — shared depth contract.

**Oracle:** The contract established by `tethys-u1rs`, existing `saturating_depth_to_u32`, and hand-counted chain depths.

**Stress fixture:** A three-edge chain plus an unknown source. Exercise `None`, 0, 1, 2, `u32::MAX`, and on 64-bit `u32::MAX + 1`; assert effective `max_depth`, target sets, NotFound-at-zero, and exactly one saturation warning.

**Loop budget:** Facade adds $O(1)$ normalization and one source lookup. Traversal inherits $O(V+E)$ from Slice 2. End-to-end statements fixed at 3.

**Wall budget:** Not applicable; on-demand query.

**Files:** `src/lib.rs`, `tests/graph.rs`.

**Smallest code change:** Add documented `Tethys::get_reachable(name, direction, Option<usize>)`; resolve source with existing first-row lookup, normalize depth through `saturating_depth_to_u32`, delegate to `Index`, and build `ReachabilityResult`. Add direct canonical integration coverage for all depth branches.

**Preconditions:** Qualified-name existence is load-bearing and enforced at runtime with `Error::NotFound`, including depth zero. Oversized depth is accepted by saturating; no panic precondition.

**Output streams:** Saturation warning is diagnostic through tracing; reachability data remains a return value.

**Verification:**
- [ ] Canonical depth integration tests pass on every reachable platform branch.
- [ ] Chain stress fixture and warning capture match the oracle.
- [ ] Prototype comparisons A/B still agree.
- [ ] Statement count remains 3 end-to-end; loop budget unchanged.
- [ ] Full Rust gates pass.

## Slice 4: Fence shortest unique path projection

**Design claim:** Claim 4 — shortest unique paths.

**Oracle:** Independent test BFS over a hand-enumerated edge set; adjacent path pairs checked directly against that set.

**Stress fixture:** Diamond with equal-depth routes, an additional longer route to the same target, and a branch whose target name duplicates another target name. Assert one row per ID, minimum depth, first predecessor, source exclusion, target last, `path.len() == depth`, and valid directional pairs in both directions.

**Loop budget:** Path reconstruction $O(\sum \text{depth})$; at default depth 50 and 100,000 returned nodes, at most 5 million symbol projections, accepted because it is the requested output size. Search remains $O(V+E)$.

**Wall budget:** Not applicable; output-bound query.

**Files:** `src/db/graph.rs`, `tests/graph.rs`.

**Smallest code change:** Add the independent fixture/oracle assertions; adjust predecessor reconstruction only if the fence exposes a mismatch.

**Preconditions:** Parent-chain completeness is an internal invariant. Violation must return a concrete error rather than panic or silently truncate.

**Output streams:** None.

**Verification:**
- [ ] `canonical_reachability_paths_are_shortest_unique_and_valid` passes.
- [ ] Diamond stress fixture matches the independent BFS in both directions.
- [ ] Prototype comparisons A/B still agree.
- [ ] Parent/path counters remain within the output-bound budget.
- [ ] Full Rust gates pass.

## Slice 5: Fence self-loops and cycles

**Design claim:** Claim 5 — cycle safety.

**Oracle:** The complete fixture has nodes `S,A,B,C,D` and edges `S→S`, `S→A`, `A→B`, `B→B`, `B→C`, `C→D`, `D→S`. From `S`, both directions have the exact non-source ID set `{A,B,C,D}`; a test timeout and source-ID count provide independent termination and exclusion checks.

**Stress fixture:** Build that five-node strongly connected graph with source and interior self-loops. Traverse both directions with default and oversized finite depth; each direction must terminate, return `S` zero times, and return `A,B,C,D` exactly once.

**Loop budget:** Each reachable symbol enqueued once; $O(V+E)$ with at most 100,000 pops and 250,000 edge examinations regardless of cycles.

**Wall budget:** Not applicable; targeted test timeout catches nontermination.

**Files:** `src/db/graph.rs`, `tests/graph.rs`.

**Smallest code change:** Add cycle/self-loop regression coverage; seed or retain the source sentinel in `parents` and refuse re-parenting if the fence requires correction.

**Preconditions:** None.

**Output streams:** None.

**Verification:**
- [ ] `canonical_reachability_excludes_source_in_cycles` passes both directions.
- [ ] Stress fixture terminates under the test timeout and returns exact IDs.
- [ ] Prototype comparisons A/B still agree.
- [ ] Discovery counters prove each ID enqueued at most once.
- [ ] Full Rust gates pass.

## Slice 6: Fence BFS discovery order

**Design claim:** Claim 6 — deterministic discovery order.

**Oracle:** A written FIFO queue transcript independent of the implementation.

**Stress fixture:** `source→a,z`; `a→zz`; `z→aa`. Qualified-name adjacency discovers `[a,z,zz,aa]`, while global `(depth,name)` order is `[a,z,aa,zz]`. Add duplicate qualified names to trigger the symbol-ID tie-break.

**Loop budget:** Neighbor sorting inherited from Slice 1; no completed-result sort is allowed. Assertion is $O(R)$ over returned rows.

**Wall budget:** Not applicable.

**Files:** `src/db/graph.rs`, `tests/graph.rs`.

**Smallest code change:** Add exact-sequence regression coverage and the ID tie case; remove or correct any global result sort exposed by the fence.

**Preconditions:** None.

**Output streams:** None.

**Verification:**
- [ ] `canonical_reachability_preserves_bfs_discovery_order` passes.
- [ ] Stress fixture differs from global alphabetical order and matches FIFO transcript exactly.
- [ ] Prototype comparisons A/B still agree.
- [ ] No post-BFS sort appears in the operation; sort budget remains adjacency-only.
- [ ] Full Rust gates pass.

## Slice 7: Fence complete symbol projection

**Design claim:** Claim 8 — projection correctness.

**Oracle:** Direct SQLite query of `symbols.is_test` by target ID.

**Stress fixture:** A non-test target with `call_count = 5` and a test target with `call_count = 0`, each reachable in both directions. Flags must equal raw symbol rows, not call counts.

**Loop budget:** No additional loops; symbol decode remains $O(V)$ in the snapshot.

**Wall budget:** Not applicable.

**Files:** `src/db/graph.rs`, `tests/graph.rs`.

**Smallest code change:** Add forward/backward is-test regression fences using the complete symbol projection; correct the snapshot SELECT only if necessary. Do not edit adjacent methods tracked by `tethys-6bui`.

**Preconditions:** Every selected row must satisfy `row_to_symbol`'s concrete type contract; errors propagate.

**Output streams:** None.

**Verification:**
- [ ] `canonical_reachability_preserves_is_test` passes.
- [ ] Stress flags match raw SQLite for all four direction/target cases.
- [ ] Prototype comparisons A/B still agree; the known legacy forward flips remain evidence, not expected canonical output.
- [ ] Statement/loop budgets unchanged.
- [ ] Full Rust gates pass.

## Slice 8: Preserve source and dangling-edge posture

**Design claim:** Claim 9 — legacy edge behavior.

**Oracle:** Existing `get_symbol_by_qualified_name` result, legacy inner-join neighbor query, and concrete rusqlite decode error type.

**Stress fixture:** Two sources share one qualified name; one call edge has a deleted endpoint with foreign keys disabled; one symbol row has a BLOB in the dynamic-typed `is_test` column to provoke decoding failure. Canonical source matches existing first-row lookup, dangling neighbor is omitted without warning/error, and corrupt decode returns an error.

**Loop budget:** Endpoint filtering is $O(E)$ hash lookup; duplicate source resolution stays one indexed query. No extra statements per endpoint.

**Wall budget:** Not applicable.

**Files:** `src/db/graph.rs`, `tests/graph.rs`.

**Smallest code change:** Add comparison-based compatibility fences; retain missing-endpoint skip and `Result` collection. Do not implement `tethys-bvgb` or change `tethys-e3j1` posture.

**Preconditions:** None. Corrupt external data is untrusted and validated by typed row decoding at point of use; errors are propagated, never matched by string.

**Output streams:** No new warning for dangling endpoints; decode errors return through the library.

**Verification:**
- [ ] `canonical_reachability_preserves_source_and_dangling_posture` passes.
- [ ] Duplicate/dangling/corrupt-row fixture matches all three independent oracles.
- [ ] Prototype comparisons A/B still agree.
- [ ] Trace shows no per-endpoint SQL growth.
- [ ] Full Rust gates pass.

## Slice 9: Fence bounded predecessor state

**Design claim:** Claim 10 — bounded search state.

**Oracle:** Discovery counters keyed by symbol ID plus structural inspection of queue and parent value types.

**Stress fixture:** A 100-node fan-out/fan-in graph with many duplicate routes and a back-edge cycle. Every non-source ID records one predecessor and one queue insertion; no queue item contains `Vec<Symbol>`.

**Loop budget:** $O(V+E)$ search; 100,000 parent entries and queue insertions maximum at production scale. Output path materialization remains separately output-bound.

**Wall budget:** Not applicable.

**Files:** `src/db/graph.rs`.

**Smallest code change:** Add a unit stress fence around the pure predecessor BFS helper or test-visible discovery accounting; keep production queue items `(SymbolId, depth)` only. Remove legacy path-cloning helper when no caller remains.

**Preconditions:** Parent lookup during reconstruction is load-bearing; absence returns a concrete error.

**Output streams:** None.

**Verification:**
- [ ] `reachability_bfs_discovers_each_symbol_once` passes.
- [ ] Fan-out/fan-in stress counts equal unique discovered IDs.
- [ ] Prototype comparisons A/B still agree.
- [ ] Structural audit confirms no queued growing path and budgets hold.
- [ ] Full Rust gates pass.

## Slice 10: Make wrappers canonical delegators

**Design claim:** Claim 1A — canonical seam for the Rust library surface.

**Oracle:** LSP references plus behavior equality between canonical method and each wrapper.

**Stress fixture:** One asymmetric graph called through all three public methods at depths 0, 1, and 3. Wrapper results must equal canonical results byte-for-byte in the matching direction, including effective saturated depth.

**Loop budget:** Wrappers add $O(1)$ delegation and zero loops/statements. One traversal implementation remains.

**Wall budget:** Not applicable.

**Files:** `src/lib.rs`, `tests/graph.rs`.

**Smallest code change:** Replace wrapper bodies with one-line calls to `get_reachable`, update docs and the saturation helper link, delete private `bfs_reachable`, and add equality coverage. Preserve wrappers until `tethys-71if`.

**Preconditions:** Same runtime checks as canonical method; wrappers add none.

**Output streams:** Same return values and tracing diagnostics as canonical method.

**Verification:**
- [ ] `reachability_wrappers_match_canonical_operation` passes.
- [ ] Asymmetric wrapper stress fixture matches canonical results.
- [ ] Prototype comparisons A/B still agree through the retained wrappers.
- [ ] LSP/text recall net finds no second traversal body or direct neighbor-query loop.
- [ ] Full Rust gates pass.

## Slice 11: Route every CLI direction through the canonical method

**Design claim:** Claim 1B — canonical seam for the CLI adapter.

**Oracle:** Actual binary exit status/output plus library canonical results on the same temporary workspace.

**Stress fixture:** Invoke the binary with `forward`, `f`, `backward`, `b`, mixed-case accepted values, omitted direction/depth, and `sideways`. Four canonical spellings and mixed case map correctly; omitted values use forward/10; invalid input returns exit 1 and the existing configuration error text.

**Loop budget:** Direction parsing is $O(L)$ for at most a short CLI token; no graph loop outside `Index`. CLI presentation retains its existing result-only sorting and 15-per-depth truncation.

**Wall budget:** Not applicable; CLI command is on-demand.

**Files:** `src/cli/reachable.rs`, `tests/reachable_cli.rs`.

**Smallest code change:** Parse the direction to `ReachabilityDirection`, call `Tethys::get_reachable` once, and add actual-binary integration tests. Do not change `src/main.rs`, defaults, output layout, or printer.

**Preconditions:** Invalid direction is untrusted CLI input and returns `Error::Config`; no panic. Direction diagnostic is error output, while reachability rows remain stdout data.

**Output streams:** Reachability report is data on stdout; invalid-direction message is diagnostic on stderr.

**Verification:**
- [ ] CLI integration test covers accepted spellings, defaults, both directions, and invalid input.
- [ ] Actual binary stress fixture matches canonical library direction/count facts.
- [ ] Prototype comparisons A/B still agree against the rebuilt binary.
- [ ] Parsing adds no graph statements or unbudgeted loops.
- [ ] Full Rust gates pass.

## Plan self-review

1. **Loops:** Every planned loop has asymptotic and production-scale bounds. Snapshot/sort/search and output materialization are justified; no per-node SQL loop is permitted.
2. **Fixtures:** Every slice names a plausible bug class: statement growth, wrong transpose/filter, depth off-by-one, predecessor overwrite, source rediscovery, global sorting, projection-column slip, posture drift/error suppression, duplicate enqueues, wrapper divergence, or CLI mapping drift.
3. **Doc-comment preconditions:** Source existence and corrupt-row handling use runtime errors; no correctness precondition relies only on `debug_assert!`. Internal parent completeness returns an error if violated.
4. **Write targets:** Library operations return data; CLI report remains stdout data; invalid direction and saturation warning remain diagnostics. No new file output.
5. **Tracker references:** `tethys-71if`, `tethys-6bui`, `tethys-bvgb`, and the expanded `tethys-e3j1` were verified; no anonymous deferral appears.
6. **Branch/error coverage:** Both direction arms, every depth branch, missing/duplicate source, empty/many neighbors, self-loop/cycle, equal/unequal routes, test flags, dangling/corrupt endpoints, wrapper routes, accepted/default/invalid CLI inputs each have an explicit fence.
7. **Return type:** Existing `ReachabilityResult` and `ReachablePath` express source, effective depth, direction, target, path, and depth; no meaningful outcome is collapsed.
8. **Behavioral equivalence:** CLI output/defaults and wrapper results are pinned; intentional changes are only bulk traversal, canonical routing, shared oversized-depth saturation, and correct projection in the new operation.
9. **Platform assumptions:** Only the `usize > u32::MAX` case is platform-dependent and is gated to 64-bit; all path/database behavior uses existing cross-platform test helpers.
