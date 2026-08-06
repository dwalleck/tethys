---
status: accepted
---

# Graph queries use SQL recursive CTEs, not petgraph

Symbol- and file-level graph queries — transitive callers/dependents, shortest
path, and cycle detection — are implemented as concrete methods on `db::Index`
(`src/db/graph.rs`) using SQLite recursive CTEs. The 2026-01-22 storage spike
(`docs/spikes/2026-01-22-tethys-sqlite-petgraph.md`)
had designed a SQLite **+ petgraph hybrid** — load a subgraph into an in-memory
`DiGraph`, run petgraph algorithms, map results back — and we did **not** adopt
it. petgraph is not a dependency.

## Why

Keeping traversal in SQL avoids the SQL↔petgraph bridge (query nodes → build
`DiGraph` → run algorithm → map back), keeps the database as the single source of
truth with no in-memory graph to hold in sync, and lets `max_depth` bounding live
inside the query. The spike's premise — that impact, cycles, and paths *require* a
graph library — did not hold: recursive CTEs express all of them.

## Consequences

This is recorded chiefly as a **deliberate deviation from the spike**, so the spike
doc is not mistaken for the shipped design and nobody "adds the missing petgraph."
Algorithms that don't express cleanly as CTEs (e.g. Tarjan SCC for rich cycle
grouping, weighted shortest path) could justify petgraph for those *specific*
operations. A narrow internal seam should be introduced only when a second real
implementation exists.

## Amendment (2026-08-02, tethys-u5o5)

The **decision stands** — petgraph is still not a dependency and there is still
no graph adapter trait. Two of the four operations named above have since moved
off recursive CTEs, so the "recursive CTEs express all of them" premise above is
narrower than originally recorded:

- **Shortest path** (tethys-4m9o, tethys-vwrn): the CTE enumerated walks rather
  than visiting nodes, so it did not terminate on a cyclic index. Replaced by a
  visited-set BFS over one adjacency load.
- **Cycle detection** (tethys-u5o5): complete simple-cycle enumeration with
  rotation canonicalization has no natural CTE form. Replaced by a DFS over one
  adjacency snapshot.

Both still load from SQLite and hold no persistent in-memory graph, so the
"database is the single source of truth" property is intact. What is *lost* is
the third rationale — `max_depth` bounding inside the query. In-Rust traversal
must bound itself explicitly, and cycle detection currently does not (see the
enumeration cost note in the tethys-u5o5 review).

## Amendment (2026-08-06, tethys-usvm)

The **decision stands**, and this amendment exists because the case for
reopening it has now actually arrived and was still declined.

Tarjan SCC — named in Consequences above as the first example of an algorithm
that "could justify petgraph" — has shipped, hand-rolled and iterative, in
`strongly_connected_components` (`src/db/graph.rs`). Cycle enumeration needs
it to confine each search to the component its start belongs to; without that
restriction cost tracked graph size rather than answer size (50,005,000 node
visits for a single cycle over 10,000 files).

petgraph was still not adopted, for reasons specific to this use:

- The implementation is about 90 lines with one call site, entirely inside
  the module that owns cycle detection. petgraph's `tarjan_scc` would need
  the file-dependency graph marshalled into a `DiGraph` and the resulting
  node indices mapped back to `FileId` on every pass — the SQL↔petgraph
  bridge this ADR rejected, rebuilt per pass rather than once.
- The enumeration runs a pass per cursor jump over a *shrinking induced
  subgraph*. That is not a whole-graph decomposition, which is the shape
  library APIs are built around.
- tethys-6k6b is actively removing single-implementation graph seams. Adding
  a dependency whose only consumer is one private function would recreate
  the shape that epic exists to delete.

The correct reading of the original Consequences paragraph is therefore
narrower than it appears: *needing* Tarjan is not on its own a reason to take
petgraph. The reason would be needing several such algorithms, or needing one
whose correct implementation is genuinely hard to get right.

Also correcting the previous amendment's closing claim. Cycle detection still
does not bound its *recursion depth* — `CycleSearch::visit` and `unblock`
remain recursive, tracked as tethys-qqbi. But it is no longer unbounded in
*cost*: work is now `O((V+E) · min(V, C+1))`, and the component pass added
here is deliberately iterative so that it adds no new stack depth.
