# Design — tethys-usvm: confine cycle enumeration to strongly-connected components

## Purpose

`enumerate_cycles` pays for the whole graph even when the graph holds one
cycle. The probe measured the cost as an exact closed form — `N(N+1)/2`
visits on an N-file single-cycle workspace, 50,005,000 visits and 9.17 s at
N = 10,000, for an answer of one cycle.

The cause is a missing piece of Johnson's algorithm. The implementation has
Johnson's `blocked` set and `B` back-lists but searches every node reachable
from each start, rather than only the strongly-connected component the start
belongs to. A start that can never reach itself still costs a full walk.

This design restores the SCC restriction.

## Architecture

### Placement

| | |
|---|---|
| **Owner** | `src/db/graph.rs` — the module that already owns `enumerate_cycles`, `run_cycle_search`, and `CycleSearch`. |
| **Why here** | The SCC pass is not a capability, it is a pruning step internal to one search. It has no caller outside enumeration, no result of its own, and no meaning to the facade. Placing it anywhere else would create a seam with exactly one consumer — the shape the parent epic (tethys-6k6b) exists to remove. |
| **New seam?** | No. Nothing is added to `Index`, `Tethys`, or the CLI. `design-an-interface` is not run: this is extend-existing work behind an existing private function. |

**Forbidden.** The implementer may not:

- give the SCC pass any visibility beyond private-to-`src/db/graph.rs`;
- change the signature or return type of `enumerate_cycles`, or the shape of
  `Cycle` — the CLI (`src/cli/cycles.rs`) and the facade depend on both;
- read the database from any enumeration code — enumeration is pure CPU over
  the already-loaded `CycleSnapshot`, an invariant PR #40 established and
  `cycle_hydration_fences` enforces;
- add a recursive function. `CycleSearch::visit` and `CycleSearch::unblock`
  already recurse without a depth bound (tethys-qqbi, open, P3); a recursive
  Tarjan would add a third site and deepen that issue. The SCC pass is
  iterative with an explicit work stack.

### The change

`run_cycle_search`'s driver loop changes from "every node is a start" to
Johnson's cursor:

```
cursor = 0
while cursor < nodes.len():
    comps = tarjan(adj, induced on nodes[cursor..])       # iterative
    live  = comps that host a cycle (|comp| > 1, or one node with a self-edge)
    if live is empty: break                               # no cycles remain
    comp  = the live comp containing the least-ranked node
    start = least-ranked node of comp
    visit(start, start) confined to comp
    cursor = rank(start) + 1
```

`CycleSearch::visit` gains one prune — skip neighbours outside `comp` —
which subsumes the existing `rank < start` skip (every member of `comp` is
already at or after `start`). Both are kept: the `comp` test is the new
restriction, the rank test remains the canonicalization contract.

Ranking stays by workspace-relative path, unchanged. That is what keeps
cycle canonicalization stable across index rebuilds, and it is why the
output is expected to be byte-identical rather than merely equivalent.

## Input shapes

Every shape below is exercised by `.tethys-usvm/design_falsifier.py`.

| Shape | Covered by claim |
|---|---|
| Empty graph (no nodes, no edges) | 10 |
| Single node, no edges | 4 |
| Single node with a self-edge | 5 |
| Self-loop reachable from a non-cycle node | 5 |
| Two-node cycle | 1, 11 |
| Single simple cycle, length N | 3 |
| Acyclic chain | 4 |
| Layered acyclic DAG (many simple paths, no cycles) | 4 |
| Two disjoint cycles (separate SCCs) | 2 |
| Overlapping cycles sharing nodes (one SCC, many cycles) | 1, 2 |
| Cycle with acyclic in-fringe and out-fringe | 2, 3 |
| One-way pair `a→b` with no return edge | 12 |
| Duplicate edges (`a→b` twice) | 1 |
| Path order ≠ file-id order | 11 |
| Dense graph, cycles ≫ nodes (tethys's own index) | 6, 7 |
| 40 seeded random graphs, n ∈ [2,7] | 1 |

Out of scope, with justification: **nodes present in `paths_by_id` but in no
edge** never enter `nodes` (it is derived from `adj` alone), so they are not
starts today and are not starts after the change — the driver loop's
iteration source is unchanged. **Dangling edge endpoints** cannot reach
enumeration; `load_cycle_snapshot` rejects them first (that posture is
itself under review as tethys-e3j1, open, P3).

## Removed-invariant sweep

The change is **subtractive**: it removes the guarantee *"every node is used
as a search start, and each start explores everything reachable from it at
or after itself."* What that guarantee was silently buying:

| Removed guarantee | Still holds? | Claim |
|---|---|---|
| Every cycle's least-ranked member is used as a start, so every cycle is found in canonical rotation | Must be proven. A cycle's members are mutually reachable, so they share an SCC; the least-ranked member `m` is the least node of a non-trivial SCC of the subgraph induced on nodes ≥ `m`, so the cursor lands on it. | 2 |
| `blocked` / `blocked_by` are cleared before each start | Changes from once-per-node to once-per-SCC-search. Stale entries leaking between searches would suppress real cycles. | 8 |
| Recorded `path` is already canonically rotated | Unaffected — confining the walk removes candidates, it does not reorder the ones that remain. | 1 |
| Every node in `nodes` is visited at least once | **Deliberately broken.** That is the fix. Nodes on no cycle are never visited. | 3, 4 |

## Falsification

Cheapest falsifier ran before approval: `.tethys-usvm/design_falsifier.py`,
53 graphs, three implementations (current port / proposed design /
exhaustive brute-force oracle).

| # | Claim | Falsifier | Oracle | Cost | Status | Regression fence |
|---|---|---|---|---|---|---|
| 1 | The returned cycle set is unchanged for every graph shape | Run current, proposed, and brute-force over 53 graphs; any set difference falsifies | Exhaustive simple-path enumeration with no blocking, no SCC, no rank pruning | 2m | **passed** (53/53) | `enumerate_cycles_covers_overlap_direction_and_self_loop`, `enumerate_cycles_scales_on_sparse_graph` |
| 2 | No cycle is lost: every cycle's least-ranked member is still a start | Two disjoint cycles + cycle-with-fringe graphs; a missing cycle falsifies | Brute-force oracle | 2m | **passed** | new `enumerate_cycles_finds_cycles_behind_acyclic_fringe` |
| 3 | On an N-node single cycle, visits == N | Measure `visits` at N ∈ {10,50,100,400,1000}; anything ≠ N falsifies | Hand-derived: only the cycle's minimum survives the SCC test, and it walks N nodes | 2m | **passed** (exact at all 5) | new `enumerate_cycles_visits_each_node_once_on_single_cycle` |
| 4 | On an acyclic graph, visits == 0 | 200-node chain and 6×10 layered DAG; any visit falsifies | Definition: an acyclic graph has no non-trivial SCC | 2m | **passed** | `enumerate_cycles_stays_output_sensitive_on_acyclic_dag`, budget tightened `nodes²` → `nodes + edges` |
| 5 | A single node with a self-edge is a non-trivial SCC and still yields its 1-cycle | Self-loop graph, alone and behind a fringe node; a missing 1-cycle falsifies | Brute-force oracle | 2m | **passed** | `enumerate_cycles_scales_on_sparse_graph` (already asserts the `999` self-loop) |
| 6 | Dense graphs do not regress: visits do not increase on a cycles-≫-nodes fixture | Compare `visits` pre/post on tethys's own index (135,888 baseline); an increase falsifies | Pre-change binary, recorded baseline | 10m | pending | new `enumerate_cycles_does_not_regress_on_dense_graph` (asserts a visit ceiling on a fixed in-repo fixture, not the live index) |
| 7 | SCC passes ≤ min(V, C+1), so the SCC work is not itself quadratic | Instrument a pass counter; run 60 random dense graphs; any excess falsifies | Arithmetic bound computed from V and the cycle count | 3m | **passed** (tightest slack 0) | new `enumerate_cycles_scc_passes_stay_bounded` |
| 8 | Per-search `blocked`/`blocked_by` state does not leak between SCC searches | Two disjoint cycles: leaked blocking would suppress the second cycle | Brute-force oracle | 2m | **passed** | covered by claim 2's fence |
| 9 | The SCC pass adds no recursion depth | 100,000-node acyclic chain — one SCC pass, zero cycle-search recursion; a stack overflow falsifies | Process exit status | 10m | pending | new `scc_pass_is_iterative_on_deep_chain` |
| 10 | Empty graph returns no cycles and does not panic | Run on `{}`; a panic or non-empty result falsifies | Trivially known | 1m | **passed** | `enumerate_cycles_handles_empty_graph` |
| 11 | Canonicalization still keys on path order, not file-id order | Graph whose id order contradicts path order; id-ordered output falsifies | Hand-computed expected rotation | 2m | **passed** | `enumerate_cycles_uses_path_order_not_file_id_order` |
| 12 | Direction stays significant — `a→b` alone yields no cycle | One-way pair; any reported cycle falsifies | Brute-force oracle | 2m | **passed** | `enumerate_cycles_covers_overlap_direction_and_self_loop` |
| 13 | End-to-end output on the real index is byte-identical | `tethys cycles \| md5sum` before and after on this branch's base index; any difference falsifies | md5 of the pre-change binary's output: `6cd0b5e753cfae4fc4c18a89ac165d61` (116 files, 400 edges, 27,016 cycles) | 5m | pending | fixture tests above; see note |
| 14 | The 10,000-file single-cycle workspace completes in well under a second | Re-run `.tethys-usvm/probe.py 10000`; ≥ 1 s falsifies | Wall clock against the 9.17 s baseline | 15m | pending | claim 3's fence is the deterministic form |

### Non-vacuity — the buggy implementation each fence would catch

| # | A bug that makes it fail |
|---|---|
| 1, 2 | Treating a single-node SCC as always trivial: drops every self-loop cycle. Or advancing `cursor` past the SCC's whole membership instead of `min+1`: silently drops cycles whose minimum sits inside an already-processed component. |
| 3 | Computing SCCs but not confining `visit` to the component — the pass runs, the prune does not, and visits stay at `N(N+1)/2`. |
| 4 | Testing "SCC is non-empty" instead of "SCC is non-trivial": every singleton looks live, no start is ever skipped, and the acyclic budget blows past `nodes + edges`. |
| 6, 7 | Recomputing SCCs once per start node instead of jumping the cursor: passes become V unconditionally and dense graphs pay V full Tarjan runs. |
| 9 | A textbook recursive Tarjan — fine on every unit fixture, stack overflow at depth 100,000. |
| 11 | Ranking the SCC's least node by `FileId` rather than by path: output reorders on any index rebuild. |

**Claim 13's fence, explicitly.** The md5 is a one-shot measurement against a
live index that changes whenever the repo does — it cannot be a CI assertion
without failing on the next unrelated commit. Its deterministic form is the
fixture-based equality tests in claims 1–5, which pin behavior to graphs
checked into the repo. The md5 comparison is recorded in the PR body as
evidence, not wired into CI. **This is the one claim whose regression fence
is indirect, and it needs explicit approval.**

Note the ticket's own AC 4 pins md5 `e80b712d…` / 24,025 cycles. That baseline
is stale — main gained a file since the ticket was filed. Re-pinned above to
this branch's base.

## Negative space — what this design deliberately does not do

1. **Does not bound recursion depth in `CycleSearch::visit` or `unblock`.**
   Those remain recursive and remain able to overflow the stack on a deep
   enough SCC. That is tethys-qqbi (open, P3), whose acceptance criteria are
   about depth bounding, not cost. This design only promises not to make it
   worse — hence the iterative-Tarjan constraint.
2. **Does not change the dangling-endpoint posture** of
   `load_cycle_snapshot`, which errors table-wide on any dangling
   `file_deps` row. That is tethys-e3j1 (open, P3).
3. **Does not make enumeration output-sensitive in the strong sense.** With
   the restriction, cost is bounded by `O((V+E)·min(V, C+1))`. On a densely
   cyclic graph the binding term is `V`, not `C+1`, so a large workspace
   that is also densely cyclic still pays `V` Tarjan passes. Measured on
   tethys's own index that is ~60,000 operations against 135,888 existing
   visits — not worth structure to avoid at this scale, and claim 6 fences
   the regression.
4. **Does not add a dependency.** No `petgraph`; Tarjan is ~40 lines and the
   parent epic explicitly calls out the phantom-Petgraph-adapter shape as
   something to remove, not add.
5. **Does not change any public surface** — no CLI flag, no JSON field, no
   facade method, no `Cycle` shape change. A user cannot observe this change
   except as latency.
6. **Does not touch symbol-level cycles or `get_dependency_chain`.** The
   SQL-side walk-enumeration hazard there was tethys-vwrn (closed).

## Open decisions for approval

1. **Claim 13's fence is indirect** (md5-on-live-index recorded as PR
   evidence, deterministic coverage delegated to fixture tests). The skill
   requires explicit approval for anything short of a CI assertion.
2. **Whether to fold tethys-qqbi into this PR.** The recursion-depth fix
   touches the same two functions. Recommendation: **keep separate** — qqbi
   is a crash with its own acceptance criteria, and merging a crash fix into
   a cost fix makes both regression fences ambiguous. This design's
   iterative-Tarjan constraint keeps qqbi exactly as hard as it is today.
3. **Branch naming** — `fix/` was chosen over `feat/` though the tracker
   kind is `task`, on the grounds that this remediates shipped behavior.
