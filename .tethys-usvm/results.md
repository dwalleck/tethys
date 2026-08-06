# Results — tethys-usvm

Every oracle, falsifier and regression fence run against the assembled
release binary after the final slice.

## 1. Sparse workspaces — the shape the issue is about

One directed cycle of N files, so the answer is always exactly one cycle and
only the graph grows. `visits` read from the binary's own debug log.

| files | visits before | visits after | expected (= N) | passes | secs before | secs after |
|------:|--------------:|-------------:|---------------:|-------:|------------:|-----------:|
| 10 | 55 | 10 | 10 | 2 | — | 0.005 |
| 50 | 1,275 | 50 | 50 | 2 | — | 0.005 |
| 100 | 5,050 | 100 | 100 | 2 | — | 0.003 |
| 200 | 20,100 | 200 | 200 | 2 | — | 0.005 |
| 400 | 80,200 | 400 | 400 | 2 | — | 0.005 |
| 1,000 | 500,500 | 1,000 | 1,000 | 2 | 0.15 | 0.007 |
| 2,000 | 2,001,000 | 2,000 | 2,000 | 2 | 0.43 | 0.012 |
| 4,000 | 8,002,000 | 4,000 | 4,000 | 2 | 1.59 | 0.021 |
| 10,000 | 50,005,000 | **10,000** | 10,000 | 2 | 9.17 | **0.047** |

The `visits before` column is the closed form `N(N+1)/2` derived in
`findings.md` and measured exactly at every size. After the change the count
lands on exactly `N` at every size, and the pass count is a flat 2 — one pass
finds the ring's component, one proves nothing is left.

**Counting the component passes too**, since visits alone understate the new
cost (§6). At 10,000 files the two passes spend 39,998 units — 20,000 over
the full graph, 19,998 over the graph minus its first file — for a total of
**49,998 units against 50,005,000 visits before**, a thousandfold reduction
that holds even when the added work is charged in full.

**Acceptance criterion 2** (10,000 files well under a second): 9.17 s →
**0.025 s** (min of 3; 0.047 s on the earlier run, which included more
process-start variance).

## 2. Dense workspace — the shape that could have regressed

tethys's own index: 116 files, 400 edges, 27,016 cycles.

| | before | after |
|---|---|---|
| visits (cycle search) | 135,888 | 134,790 |
| component passes | n/a | 24 |
| component work (nodes + edges scanned) | n/a | 8,311 |
| wall, min of 5 | 0.130 s | 0.104 s |
| wall, median of 5 | 0.170 s | 0.158 s |
| cycles | 27,016 | 27,016 |
| md5 of `tethys cycles` | `6cd0b5e753cfae4fc4c18a89ac165d61` | `6cd0b5e753cfae4fc4c18a89ac165d61` |

Charged honestly, total work on this shape *rises*: 135,888 units before,
143,101 after. Wall-clock still falls, because a component-scan unit is much
cheaper than a search visit — a visit pushes a path, inserts into a hash set,
and clones the path when a cycle closes. This is the one shape where the
restriction is a net cost in operations and a net win in time, and it is why
the dense fence asserts the two counters separately instead of summing them.

Densely cyclic graphs were the one place the restriction could cost more than
it saves — 24 component passes are work the old search never did.

**Read the visits column carefully.** It counts the cycle search only, and
the component passes are not in it, so the fall from 135,888 to 134,790 shows
that the *search* did not get more expensive — it is not evidence about the
passes at all. The end-to-end evidence is wall-clock, which fell alongside
it. Pass cost is tracked separately as `component_work` (added during
pre-PR review, see §5).

**Acceptance criterion 3** (output byte-for-byte identical): md5 matches, to
the byte, over 27,016 cycles.

Note the ticket's own AC pinned md5 `e80b712d9855e6807fef26e623625ab1` at
24,025 cycles. That baseline was stale — main gained a file between the
ticket being filed and this branch. The figure above is this branch's base,
verified stable across three consecutive runs before any code changed.

## 3. Design falsifier

`.tethys-usvm/design_falsifier.py`, re-run after implementation. All four
kill conditions still pass: 53 graphs agree across the current-code port, the
proposed design and an exhaustive brute-force oracle; single-cycle visits
land on N; acyclic graphs cost 0 visits; component passes stay within
`min(V, C+1)` with tightest slack 0 over 60 random dense graphs.

## 4. Regression fences

| Claim | Fence | Result |
|---|---|---|
| 1, 5, 12 | `enumerate_cycles_covers_overlap_direction_and_self_loop` | pass (unmodified from before this change) |
| 2, 8 | `enumerate_cycles_finds_cycles_behind_acyclic_fringe`, `enumerate_cycles_keeps_disjoint_components_independent` | pass |
| 3 | `enumerate_cycles_visits_each_node_once_on_single_cycle` | pass — 400 visits for 400 files |
| 3 | `enumerate_cycles_does_not_walk_past_the_component` | pass — 2 visits, not 5 |
| 4 | `enumerate_cycles_stays_output_sensitive_on_acyclic_dag` | pass — budget tightened `nodes²` (3,600) → `nodes + edges` (384), observed 0 |
| 6 | `enumerate_cycles_does_not_regress_on_dense_graph` | pass — 410 cycles, visits under the 416 pre-change ceiling |
| 7 | `enumerate_cycles_scc_passes_stay_bounded` | pass |
| 9 | `scc_pass_is_iterative_on_deep_chain` | pass — 100,000 nodes, no overflow, 0.10 s |
| 10 | `enumerate_cycles_handles_empty_graph` | pass (unmodified) |
| 11 | `enumerate_cycles_uses_path_order_not_file_id_order` | pass (unmodified) |
| — | `scc_decomposition_matches_hand_computed_components`, `scc_respects_the_induced_subgraph`, `scc_on_empty_subgraph_returns_no_components`, `hosts_cycle_requires_two_nodes_or_a_self_edge` | pass |

Full suite: `cargo nextest run` green, clippy pedantic `-D warnings` clean,
`cargo fmt --check` clean, doctests green.

## 5. Non-vacuity — mutation testing the fences

The design named two buggy implementations its fences had to catch. Both were
run against the real test suite.

| Mutation | Caught by |
|---|---|
| `hosts_cycle` tests "component is non-empty" instead of "can close a cycle" | 3 fences fail |
| the component prune is deleted from `CycleSearch::visit` | **initially none** — see below |

The second mutation initially survived the entire suite. Every fixture was
either a ring or a graph whose cycles had only in-fringes, and for both of
those the component *is* everything the start can reach, so confining the
walk removes nothing observable.

`enumerate_cycles_does_not_walk_past_the_component` was written to close that
gap: it gives a cycle an out-fringe — a chain hanging off it, ordered after
the cycle's first file — so the rank test alone lets the walk wander down it.
The fence fails at 5 visits against 2 when the prune is removed.

This separated two effects the design had merged. The **cursor** — skipping
start nodes that cannot host a cycle — is what produced the 195× win at
10,000 files. The **component confinement** is a smaller, separate win that
only pays on cycles with trailing structure, and it is invisible to any
ring-shaped fixture.

## 6. Corrections applied during pre-PR review

The spec-axis reviewer found that `visits` counts the cycle search alone and
that the component passes contributed nothing to it. Two claims rested on
that number and should not have:

- `enumerate_cycles_does_not_regress_on_dense_graph` asserted `visits <= 416`
  under a doc comment saying anything above it meant "the component passes
  are costing more than they save." No amount of pass cost could ever have
  failed that assertion.
- The tightened acyclic budget read `visits <= nodes + edges` and observed
  **0**, which looked like the work had been eliminated. It had been
  *relocated* — into the uncounted component pass.

Fixed by counting it. `ComponentScan::work` now records nodes entered plus
edges examined per pass, accumulated as
`CycleSearchOutcome::component_work` and logged beside `visits`. The acyclic
budget now bounds `visits + component_work` and lands on **exactly 384 =
nodes + edges (60 + 324)** — tight rather than trivially satisfied, and
honestly comparable against the ~3,600 visits the same shape cost before this
change. The dense fence asserts the two quantities separately, since summing
search visits and scan units would be adding incommensurable things.

This is the second fence in this change found to be passing for a reason
unrelated to what it claimed to prove; the first was caught by mutation
testing during slice 3 (§5). Both were assertions written against intent
rather than against what the counter could observe.

Also amended `docs/adr/0002-sql-ctes-not-petgraph.md`, which named Tarjan SCC
as the example algorithm that "could justify petgraph." It shipped here
hand-rolled, so the ADR needed to record that the case arrived and was still
declined, and why.

## 6. Out of scope, still true

- `CycleSearch::visit` and `unblock` remain recursive and can still overflow
  the stack on a sufficiently deep component (tethys-qqbi, open). The
  component pass added here is iterative precisely so that issue is no
  harder than it was.
- `load_cycle_snapshot` still errors table-wide on any dangling `file_deps`
  endpoint (tethys-e3j1, open).
